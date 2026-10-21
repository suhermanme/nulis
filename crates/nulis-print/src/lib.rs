//! Page layout for printing and PDF export (`IMPROVEMENT_PLAN_3.md`).
//!
//! A [`LineSource`] supplies the document's lines with syntax highlights; the
//! [`Paginator`] lays them out on pages for a [`PageSetup`]; the
//! [`PdfWriter`] streams the pages to a file. [`export_pdf`] runs the whole
//! process, with a first counting pass when a header or footer shows the
//! number of pages.

pub mod layout;
pub mod palette;
pub mod pdf;
pub mod setup;

pub use layout::{Gutter, Page, Paginator, Row, Run, SourceLine, Span, Style};
pub use palette::{ColorChoice, Palette, Rgb};
pub use pdf::{bundled_metrics, FontMetrics, PdfSummary, PdfWriter};
pub use setup::{
    Band, BandFields, Geometry, LongLines, Margins, Orientation, PageSetup, PaperSize,
};

use std::io::Write;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PrintError {
    #[error("{0}")]
    Setup(String),
    #[error("font error: {0}")]
    Font(String),
    #[error("could not read the document: {0}")]
    Source(String),
    #[error("could not write the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("cancelled")]
    Cancelled,
}

/// The document's lines, read from the start as often as needed.
pub trait LineSource {
    /// Goes back to the first line.
    ///
    /// # Errors
    /// Returns [`PrintError::Source`] if the document can no longer be read.
    fn rewind(&mut self) -> Result<(), PrintError>;

    /// The next batch of lines, or `None` after the last one. Highlights are
    /// only needed when `highlight` is true.
    ///
    /// # Errors
    /// Returns [`PrintError::Source`] if the document can no longer be read,
    /// for example because it changed.
    fn next_lines(&mut self, highlight: bool) -> Result<Option<Vec<SourceLine>>, PrintError>;

    /// Called after each page is written, for progress.
    fn page_written(&mut self, _number: u64) {}
}

/// The fewest pages `setup` can take for `rows` rows (lines, plus one per
/// separator), before any long line wraps, with line numbers up to
/// `last_line`. Cheap, for asking before a very large export.
///
/// # Errors
/// Returns an error for an unusable page setup.
pub fn estimate_pages(setup: &PageSetup, rows: u64, last_line: u64) -> Result<u64, PrintError> {
    let metrics = bundled_metrics()?;
    let geometry = Geometry::new(
        setup,
        metrics.advance,
        metrics.ascender,
        metrics.descender,
        last_line,
    )?;
    Ok(rows.max(1).div_ceil(u64::from(geometry.rows)))
}

/// What to export and how.
#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub setup: PageSetup,
    pub palette: Palette,
    /// Header and footer fields; `pages` is filled in by the export.
    pub fields: BandFields,
    pub title: String,
    /// The document's last line number, which sizes the line-number gutter.
    pub last_line: u64,
}

/// Lays out every line of `source` and writes it as a PDF to `out`.
///
/// # Errors
/// Returns an error for an unusable page setup, an unreadable document, or a
/// failed write.
pub fn export_pdf<S: LineSource, W: Write>(
    source: &mut S,
    out: W,
    options: &ExportOptions,
) -> Result<(W, PdfSummary), PrintError> {
    let metrics = bundled_metrics()?;
    let geometry = Geometry::new(
        &options.setup,
        metrics.advance,
        metrics.ascender,
        metrics.descender,
        options.last_line,
    )?;
    let paginator = || {
        Paginator::new(
            geometry.rows,
            geometry.text_columns(),
            options.setup.long_lines,
            options.setup.tab_width,
        )
    };
    let mut fields = options.fields.clone();
    if options.setup.header.uses_page_count() || options.setup.footer.uses_page_count() {
        source.rewind()?;
        let mut counter = paginator();
        let mut pages = 0_u64;
        while let Some(lines) = source.next_lines(false)? {
            for line in &lines {
                pages += u64::try_from(counter.push(line).len()).unwrap_or(0);
            }
        }
        pages += u64::try_from(counter.finish().len()).unwrap_or(0);
        fields.pages = Some(pages);
    }

    source.rewind()?;
    let highlight = options.setup.syntax_highlighting;
    let mut writer = PdfWriter::new(out, geometry, options.palette.clone())?;
    let mut layout = paginator();
    let write = |writer: &mut PdfWriter<W>, source: &mut S, page: &Page| {
        let header = fields.resolve(&options.setup.header, page.number);
        let footer = fields.resolve(&options.setup.footer, page.number);
        writer.write_page(page, &header, &footer)?;
        source.page_written(page.number);
        Ok::<(), PrintError>(())
    };
    while let Some(lines) = source.next_lines(highlight)? {
        for line in &lines {
            for page in layout.push(line) {
                write(&mut writer, source, &page)?;
            }
        }
    }
    for page in layout.finish() {
        write(&mut writer, source, &page)?;
    }
    writer.finish(&options.title)
}
