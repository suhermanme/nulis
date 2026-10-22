//! Page layout for printing and PDF export (`IMPROVEMENT_PLAN_3.md`).
//!
//! A [`LineSource`] supplies the document's lines with syntax highlights; the
//! [`Paginator`] lays them out on pages for a [`PageSetup`]; the
//! [`PdfWriter`] streams the pages to a file. [`export_pdf`] runs the whole
//! process, with a first counting pass when a header or footer shows the
//! number of pages.

mod fonts;
pub mod layout;
pub mod model;
pub mod palette;
pub mod pdf;
pub mod setup;

pub use fonts::FontMetrics;
pub use layout::{Gutter, Page, Paginator, Row, Run, SourceLine, Span, Style};
pub use model::{PageFrame, Piece, PrintedPage};
pub use palette::{ColorChoice, Palette, Rgb};
pub use pdf::{bundled_metrics, PdfSummary, PdfWriter};
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
    /// Header and footer fields; `pages` is counted by the export unless given.
    pub fields: BandFields,
    pub title: String,
    /// The document's last line number, which sizes the line-number gutter.
    pub last_line: u64,
    /// Draw characters the bundled font lacks from installed fonts.
    pub fallback_fonts: bool,
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
    let geometry = geometry(options)?;
    let mut writer = PdfWriter::new(
        out,
        geometry,
        options.palette.clone(),
        options.fallback_fonts,
    )?;
    paginate(source, options, &geometry, |page, header, footer| {
        writer.write_page(page, header, footer)
    })?;
    writer.finish(&options.title)
}

/// Lays out every line of `source` as pages for the Print Preview window,
/// handing each to `emit` as it is finished. Returns what every page shares.
///
/// # Errors
/// Returns an error for an unusable page setup or an unreadable document,
/// or the first error `emit` returns.
pub fn print_pages<S: LineSource>(
    source: &mut S,
    options: &ExportOptions,
    mut emit: impl FnMut(PrintedPage) -> Result<(), PrintError>,
) -> Result<PageFrame, PrintError> {
    let geometry = geometry(options)?;
    paginate(source, options, &geometry, |page, header, footer| {
        emit(model::place_page(&geometry, page, header, footer))
    })?;
    Ok(PageFrame::new(&options.setup, &geometry, &options.palette))
}

/// What every page of `options` shares, without laying anything out.
///
/// # Errors
/// Returns an error for an unusable page setup.
pub fn page_frame(options: &ExportOptions) -> Result<PageFrame, PrintError> {
    Ok(PageFrame::new(
        &options.setup,
        &geometry(options)?,
        &options.palette,
    ))
}

fn geometry(options: &ExportOptions) -> Result<Geometry, PrintError> {
    let metrics = bundled_metrics()?;
    Geometry::new(
        &options.setup,
        metrics.advance,
        metrics.ascender,
        metrics.descender,
        options.last_line,
    )
}

/// Lays out `source` and hands every page, with its header and footer
/// filled in, to `emit`. When a header or footer shows `{pages}`, a first
/// pass counts the pages.
fn paginate<S: LineSource>(
    source: &mut S,
    options: &ExportOptions,
    geometry: &Geometry,
    mut emit: impl FnMut(&Page, &[String; 3], &[String; 3]) -> Result<(), PrintError>,
) -> Result<(), PrintError> {
    let paginator = || {
        Paginator::new(
            geometry.rows,
            geometry.text_columns(),
            options.setup.long_lines,
            options.setup.tab_width,
        )
    };
    let mut fields = options.fields.clone();
    // A page count already given (an estimate for a preview) needs no counting pass.
    if fields.pages.is_none()
        && (options.setup.header.uses_page_count() || options.setup.footer.uses_page_count())
    {
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
    let mut layout = paginator();
    let mut write = |source: &mut S, page: &Page| {
        let header = fields.resolve(&options.setup.header, page.number);
        let footer = fields.resolve(&options.setup.footer, page.number);
        emit(page, &header, &footer)?;
        source.page_written(page.number);
        Ok::<(), PrintError>(())
    };
    while let Some(lines) = source.next_lines(highlight)? {
        for line in &lines {
            for page in layout.push(line) {
                write(source, &page)?;
            }
        }
    }
    for page in layout.finish() {
        write(source, &page)?;
    }
    Ok(())
}
