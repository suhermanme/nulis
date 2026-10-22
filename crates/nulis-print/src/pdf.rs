//! A PDF writer that streams pages to its output as they are laid out.
//!
//! Each object is written as soon as it is complete and only its byte offset
//! is kept, so memory does not grow with the page count beyond one reference
//! and one offset per page. The fonts, the page tree, the outline, and the
//! cross-reference table are written at the end.

use crate::{
    fonts::{FontMetrics, FontRefs, FontSet, RefSource, REGULAR_TTF},
    layout::{cell_width, Gutter, Page, Run, Style},
    palette::{Palette, Rgb},
    setup::{band_layout, Geometry},
    PrintError,
};
use pdf_writer::{
    writers::{Catalog, DocumentInfo},
    Chunk, Content, Filter, Finish, Name, Rect, Ref, Str, TextStr,
};
use std::{collections::BTreeSet, fmt::Write as _, io::Write};

/// Pages per outline entry for long documents.
const PAGES_PER_OUTLINE_ITEM: u64 = 100;
/// Marks a wrapped row in the gutter.
pub(crate) const CONTINUATION_MARKER: char = '↪';

const CATALOG: Ref = Ref::new(1);
const PAGE_TREE: Ref = Ref::new(2);
const INFO: Ref = Ref::new(3);
const REGULAR_REFS: FontRefs = FontRefs::starting_at(4);
const BOLD_REFS: FontRefs = FontRefs::starting_at(9);
const FIRST_DYNAMIC_REF: i32 = 14;

/// The bundled regular font's metrics, which [`Geometry::new`] needs.
///
/// # Errors
/// Returns [`PrintError::Font`] if the bundled font cannot be read.
pub fn bundled_metrics() -> Result<FontMetrics, PrintError> {
    Ok(crate::fonts::EmbeddedFont::bundled(
        REGULAR_TTF,
        "JetBrainsMono-Regular",
        REGULAR_REFS,
        "F1",
    )?
    .metrics())
}

/// Object numbers handed out while writing.
#[derive(Debug)]
struct RefCounter {
    next: i32,
}

impl RefSource for RefCounter {
    fn allocate(&mut self) -> Result<Ref, PrintError> {
        let reference = Ref::new(self.next);
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| PrintError::Setup("too many pages for one PDF".into()))?;
        Ok(reference)
    }
}

/// Counts bytes written and records each object's offset.
pub(crate) struct ObjectSink<'a> {
    out: &'a mut dyn Write,
    position: usize,
    offsets: &'a mut Vec<(i32, usize)>,
}

impl ObjectSink<'_> {
    /// Writes a chunk holding exactly one object.
    pub(crate) fn write(&mut self, chunk: &Chunk) -> Result<(), PrintError> {
        if let Some(reference) = chunk.refs().next() {
            self.offsets.push((reference.get(), self.position));
        }
        self.raw(chunk.as_bytes())
    }

    fn raw(&mut self, bytes: &[u8]) -> Result<(), PrintError> {
        self.out.write_all(bytes)?;
        self.position += bytes.len();
        Ok(())
    }
}

/// Information about the finished file.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PdfSummary {
    pub pages: u64,
    /// Characters left blank because the font has no glyph for them.
    pub missing_glyphs: u64,
    pub bytes: u64,
}

/// Streams pages into a PDF file.
pub struct PdfWriter<W: Write> {
    out: W,
    position: usize,
    offsets: Vec<(i32, usize)>,
    geometry: Geometry,
    palette: Palette,
    fonts: FontSet,
    /// Fonts drawn with on any page (the regular font is always written).
    fonts_used: BTreeSet<usize>,
    /// Fonts drawn with on the page being written.
    page_fonts: BTreeSet<usize>,
    page_refs: Vec<Ref>,
    refs: RefCounter,
    missing_glyphs: u64,
}

impl<W: Write> std::fmt::Debug for PdfWriter<W> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PdfWriter")
            .field("pages", &self.page_refs.len())
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}

impl<W: Write> PdfWriter<W> {
    /// Starts a PDF on `out`. With `fallback_fonts`, characters the bundled
    /// font lacks are drawn from installed fonts that have them.
    ///
    /// # Errors
    /// Returns an error if the fonts cannot be read or `out` cannot be written.
    pub fn new(
        out: W,
        geometry: Geometry,
        palette: Palette,
        fallback_fonts: bool,
    ) -> Result<Self, PrintError> {
        let mut writer = Self {
            out,
            position: 0,
            offsets: Vec::new(),
            geometry,
            palette,
            fonts: FontSet::new(REGULAR_REFS, BOLD_REFS, fallback_fonts)?,
            fonts_used: BTreeSet::from([0]),
            page_fonts: BTreeSet::new(),
            page_refs: Vec::new(),
            refs: RefCounter {
                next: FIRST_DYNAMIC_REF,
            },
            missing_glyphs: 0,
        };
        // The second line marks the file as binary for transfer tools.
        let mut sink = writer.sink();
        sink.raw(b"%PDF-1.7\n%\x80\x80\x80\x80\n")?;
        writer.position = sink.position;
        Ok(writer)
    }

    fn sink(&mut self) -> ObjectSink<'_> {
        ObjectSink {
            out: &mut self.out,
            position: self.position,
            offsets: &mut self.offsets,
        }
    }

    fn write_chunk(&mut self, chunk: &Chunk) -> Result<(), PrintError> {
        let mut sink = self.sink();
        sink.write(chunk)?;
        self.position = sink.position;
        Ok(())
    }

    fn allocate(&mut self) -> Result<Ref, PrintError> {
        self.refs.allocate()
    }

    /// Writes one page with its header and footer (each already filled in:
    /// left, centre, right).
    ///
    /// # Errors
    /// Returns an error if the output cannot be written.
    pub fn write_page(
        &mut self,
        page: &Page,
        header: &[String; 3],
        footer: &[String; 3],
    ) -> Result<(), PrintError> {
        let geometry = self.geometry;
        let mut content = Content::new();
        self.page_fonts.clear();
        if let Some(background) = self.palette.background {
            let [r, g, b] = background.unit();
            content
                .set_fill_rgb(r, g, b)
                .rect(0.0, 0.0, geometry.page_width, geometry.page_height)
                .fill_nonzero();
        }
        content.begin_text();
        let mut pen = Pen::default();
        let mut separators = Vec::new();
        if geometry.has_header {
            let band = self.palette.band;
            self.draw_band(&mut content, &mut pen, header, geometry.top, band)?;
        }
        let digits = geometry.gutter_columns.saturating_sub(1);
        for (index, row) in page.rows.iter().enumerate() {
            let top = geometry.body_row_top(u32::try_from(index).unwrap_or(u32::MAX));
            if row.gutter == Gutter::Separator {
                separators.push(top - geometry.line_height / 2.0);
                continue;
            }
            if digits > 0 {
                let label = match row.gutter {
                    Gutter::Number(number) => format!(
                        "{number:>width$}",
                        width = usize::try_from(digits).unwrap_or(0)
                    ),
                    Gutter::Continuation => format!(
                        "{CONTINUATION_MARKER:>width$}",
                        width = usize::try_from(digits).unwrap_or(0)
                    ),
                    Gutter::Separator => String::new(),
                };
                let color = self.palette.line_number;
                self.draw_text(&mut content, &mut pen, &label, 0, top, color, false)?;
            }
            for run in &row.runs {
                let (color, bold) = self.run_style(run);
                self.draw_text(
                    &mut content,
                    &mut pen,
                    &run.text,
                    geometry.gutter_columns + run.column,
                    top,
                    color,
                    bold,
                )?;
            }
        }
        if geometry.has_footer {
            let band = self.palette.band;
            self.draw_band(&mut content, &mut pen, footer, geometry.footer_top(), band)?;
        }
        content.end_text();
        self.draw_rules(&mut content, &separators);
        self.fonts_used.extend(self.page_fonts.iter().copied());

        let content_ref = self.allocate()?;
        let page_ref = self.allocate()?;
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&content.finish(), 6);
        let mut chunk = Chunk::new();
        chunk
            .stream(content_ref, &compressed)
            .filter(Filter::FlateDecode);
        self.write_chunk(&chunk)?;

        let mut chunk = Chunk::new();
        let mut page_writer = chunk.page(page_ref);
        page_writer
            .parent(PAGE_TREE)
            .media_box(Rect::new(
                0.0,
                0.0,
                geometry.page_width,
                geometry.page_height,
            ))
            .contents(content_ref);
        let mut resources = page_writer.resources();
        let mut fonts = resources.fonts();
        for index in &self.page_fonts {
            let font = self.fonts.font(*index);
            fonts.pair(Name(font.resource.as_bytes()), font.refs_type0());
        }
        fonts.finish();
        resources.finish();
        page_writer.finish();
        self.write_chunk(&chunk)?;
        self.page_refs.push(page_ref);
        Ok(())
    }

    /// Thin rules across the text area at each height, in the line-number color.
    fn draw_rules(&self, content: &mut Content, heights: &[f32]) {
        if heights.is_empty() {
            return;
        }
        let geometry = self.geometry;
        let [r, g, b] = self.palette.line_number.unit();
        #[allow(clippy::cast_precision_loss)] // Columns are far below 2^24.
        let width = geometry.columns as f32 * geometry.advance;
        content.set_stroke_rgb(r, g, b).set_line_width(0.5);
        for y in heights {
            content
                .move_to(geometry.left, *y)
                .line_to(geometry.left + width, *y);
        }
        content.stroke();
    }

    fn run_style(&self, run: &Run) -> (Rgb, bool) {
        match run.style {
            Style::Plain => (self.palette.text, false),
            Style::Symbol => (self.palette.symbol, false),
            Style::Token(token) => {
                let style = self.palette.token(token);
                (style.color, style.bold)
            }
        }
    }

    fn draw_band(
        &mut self,
        content: &mut Content,
        pen: &mut Pen,
        parts: &[String; 3],
        top: f32,
        color: Rgb,
    ) -> Result<(), PrintError> {
        for (column, text) in band_layout(self.geometry.columns, parts) {
            self.draw_text(content, pen, &text, column, top, color, false)?;
        }
        Ok(())
    }

    /// Draws `text` from `column`, one cell per character (two for wide
    /// characters), so a missing glyph or a font with other widths never
    /// shifts the columns after it. Runs of bundled-font glyphs are drawn
    /// together; fallback and wide glyphs are placed one by one, narrowed if
    /// they are wider than their cells.
    #[allow(clippy::too_many_arguments)] // Position, color, and weight of one run.
    fn draw_text(
        &mut self,
        content: &mut Content,
        pen: &mut Pen,
        text: &str,
        column: u32,
        top: f32,
        color: Rgb,
        bold: bool,
    ) -> Result<(), PrintError> {
        let geometry = self.geometry;
        let baseline = top - geometry.baseline;
        let x_at = |cell: u32| {
            #[allow(clippy::cast_precision_loss)] // Columns are far below 2^24.
            let cells = cell as f32;
            geometry.left + cells * geometry.advance
        };
        let mut batch: Vec<u8> = Vec::with_capacity(text.len() * 2);
        let mut batch_font = 0;
        let mut batch_start = column;
        let mut cell = column;
        for character in text.chars() {
            let width = cell_width(character);
            let placed = self.fonts.place(character, bold, &mut self.refs)?;
            let joins_batch = placed.is_some_and(|placed| {
                placed.font < 2 && width == 1 && (batch.is_empty() || placed.font == batch_font)
            });
            if !joins_batch && !batch.is_empty() {
                let font = &self.fonts.font(batch_font).resource;
                pen.select(content, font, geometry.font_size, color);
                content
                    .set_text_matrix([1.0, 0.0, 0.0, 1.0, x_at(batch_start), baseline])
                    .show(Str(&batch));
                batch.clear();
            }
            match placed {
                Some(placed) if joins_batch => {
                    if batch.is_empty() {
                        batch_font = placed.font;
                        batch_start = cell;
                    }
                    batch.extend_from_slice(&placed.glyph.to_be_bytes());
                    self.page_fonts.insert(placed.font);
                }
                Some(placed) => {
                    #[allow(clippy::cast_precision_loss)] // One or two cells.
                    let room = width as f32 * geometry.advance;
                    let natural = placed.advance * geometry.font_size;
                    let squeeze = if natural > room && natural > 0.0 {
                        room / natural
                    } else {
                        1.0
                    };
                    let offset = (room - natural * squeeze) / 2.0;
                    let font = &self.fonts.font(placed.font).resource;
                    pen.select(content, font, geometry.font_size, color);
                    content
                        .set_text_matrix([squeeze, 0.0, 0.0, 1.0, x_at(cell) + offset, baseline])
                        .show(Str(&placed.glyph.to_be_bytes()));
                    self.page_fonts.insert(placed.font);
                }
                None => {
                    // Nothing is drawn; the cell stays blank.
                    self.missing_glyphs += u64::from(!character.is_whitespace());
                }
            }
            cell += width;
        }
        if !batch.is_empty() {
            let font = &self.fonts.font(batch_font).resource;
            pen.select(content, font, geometry.font_size, color);
            content
                .set_text_matrix([1.0, 0.0, 0.0, 1.0, x_at(batch_start), baseline])
                .show(Str(&batch));
        }
        Ok(())
    }

    /// Writes the fonts, page tree, outline, document information, and
    /// cross-reference table, and returns the output.
    ///
    /// # Errors
    /// Returns an error if the output cannot be written.
    pub fn finish(mut self, title: &str) -> Result<(W, PdfSummary), PrintError> {
        let mut sink = ObjectSink {
            out: &mut self.out,
            position: self.position,
            offsets: &mut self.offsets,
        };
        for index in 0..self.fonts.len() {
            if self.fonts_used.contains(&index) {
                self.fonts.font(index).write(&mut sink)?;
            }
        }
        self.position = sink.position;

        let mut chunk = Chunk::new();
        chunk
            .pages(PAGE_TREE)
            .kids(self.page_refs.iter().copied())
            .count(
                i32::try_from(self.page_refs.len())
                    .map_err(|_| PrintError::Setup("too many pages".into()))?,
            );
        self.write_chunk(&chunk)?;

        let outline = self.write_outline()?;

        let mut chunk = Chunk::new();
        let mut catalog = chunk.indirect(CATALOG).start::<Catalog>();
        catalog.pages(PAGE_TREE);
        if let Some(outline) = outline {
            catalog.outlines(outline);
        }
        catalog.finish();
        self.write_chunk(&chunk)?;

        let producer = format!("Nulis {}", env!("CARGO_PKG_VERSION"));
        let mut chunk = Chunk::new();
        let mut info = chunk.indirect(INFO).start::<DocumentInfo>();
        info.title(TextStr(title))
            .producer(TextStr(&producer))
            .creator(TextStr("Nulis"));
        info.finish();
        self.write_chunk(&chunk)?;

        self.write_cross_references()?;
        self.out.flush()?;
        let summary = PdfSummary {
            pages: u64::try_from(self.page_refs.len()).unwrap_or(u64::MAX),
            missing_glyphs: self.missing_glyphs,
            bytes: u64::try_from(self.position).unwrap_or(u64::MAX),
        };
        Ok((self.out, summary))
    }

    /// One entry per hundred pages, for documents longer than that.
    fn write_outline(&mut self) -> Result<Option<Ref>, PrintError> {
        let pages = u64::try_from(self.page_refs.len()).unwrap_or(u64::MAX);
        if pages <= PAGES_PER_OUTLINE_ITEM {
            return Ok(None);
        }
        let root = self.allocate()?;
        let count = pages.div_ceil(PAGES_PER_OUTLINE_ITEM);
        let items = (0..count)
            .map(|_| self.allocate())
            .collect::<Result<Vec<_>, _>>()?;
        for (index, item) in items.iter().enumerate() {
            let first_page = u64::try_from(index).unwrap_or(0) * PAGES_PER_OUTLINE_ITEM;
            let last_page = (first_page + PAGES_PER_OUTLINE_ITEM).min(pages);
            let target = self.page_refs[usize::try_from(first_page).unwrap_or(0)];
            let title = format!("Pages {}–{}", first_page + 1, last_page);
            let mut chunk = Chunk::new();
            let mut writer = chunk.outline_item(*item);
            writer.title(TextStr(&title)).parent(root);
            if index > 0 {
                writer.prev(items[index - 1]);
            }
            if let Some(next) = items.get(index + 1) {
                writer.next(*next);
            }
            writer
                .dest()
                .page(target)
                .xyz(0.0, self.geometry.page_height, None);
            writer.finish();
            self.write_chunk(&chunk)?;
        }
        let mut chunk = Chunk::new();
        chunk
            .outline(root)
            .first(items[0])
            .last(*items.last().expect("at least one outline item"))
            .count(i32::try_from(items.len()).unwrap_or(i32::MAX));
        self.write_chunk(&chunk)?;
        Ok(Some(root))
    }

    fn write_cross_references(&mut self) -> Result<(), PrintError> {
        let size = self.refs.next;
        let mut offsets = vec![None; usize::try_from(size).unwrap_or(0)];
        for (reference, offset) in &self.offsets {
            if let Some(slot) = usize::try_from(*reference)
                .ok()
                .and_then(|index| offsets.get_mut(index))
            {
                *slot = Some(*offset);
            }
        }
        let start = self.position;
        let mut table = format!("xref\n0 {size}\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            // References allocated but never written (an unused font) are free.
            match offset {
                Some(offset) => {
                    let _ = writeln!(table, "{offset:010} 00000 n ");
                }
                None => table.push_str("0000000000 65535 f \n"),
            }
        }
        let _ = write!(
            table,
            "trailer\n<< /Size {size} /Root {} 0 R /Info {} 0 R >>\nstartxref\n{start}\n%%EOF\n",
            CATALOG.get(),
            INFO.get()
        );
        let mut sink = self.sink();
        sink.raw(table.as_bytes())?;
        self.position = sink.position;
        Ok(())
    }
}

/// The text state already set in a content stream, so it is only changed when needed.
#[derive(Debug, Default)]
struct Pen {
    font: Option<String>,
    color: Option<Rgb>,
}

impl Pen {
    fn select(&mut self, content: &mut Content, font: &str, size: f32, color: Rgb) {
        if self.font.as_deref() != Some(font) {
            content.set_font(Name(font.as_bytes()), size);
            self.font = Some(font.to_owned());
        }
        if self.color != Some(color) {
            let [r, g, b] = color.unit();
            content.set_fill_rgb(r, g, b);
            self.color = Some(color);
        }
    }
}
