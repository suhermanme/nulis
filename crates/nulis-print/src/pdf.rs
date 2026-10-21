//! A PDF writer that streams pages to its output as they are laid out.
//!
//! Each object is written as soon as it is complete and only its byte offset
//! is kept, so memory does not grow with the page count beyond one reference
//! and one offset per page. The fonts, the page tree, the outline, and the
//! cross-reference table are written at the end.

use crate::{
    layout::{Gutter, Page, Run, Style},
    palette::{Palette, Rgb},
    setup::Geometry,
    PrintError,
};
use pdf_writer::{
    types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap},
    writers::{Catalog, DocumentInfo},
    Chunk, Content, Filter, Finish, Name, Rect, Ref, Str, TextStr,
};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    io::Write,
};

const REGULAR_TTF: &[u8] = include_bytes!(
    "../../../apps/desktop/src/assets/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf"
);
const BOLD_TTF: &[u8] =
    include_bytes!("../../../apps/desktop/src/assets/fonts/jetbrains-mono/JetBrainsMono-Bold.ttf");

/// Pages per outline entry for long documents.
const PAGES_PER_OUTLINE_ITEM: u64 = 100;
/// Marks a wrapped row in the gutter.
const CONTINUATION_MARKER: char = '↪';

const CATALOG: Ref = Ref::new(1);
const PAGE_TREE: Ref = Ref::new(2);
const INFO: Ref = Ref::new(3);
const REGULAR_REFS: FontRefs = FontRefs::starting_at(4);
const BOLD_REFS: FontRefs = FontRefs::starting_at(9);
const FIRST_DYNAMIC_REF: i32 = 14;

const SYSTEM_INFO: SystemInfo = SystemInfo {
    registry: Str(b"Adobe"),
    ordering: Str(b"Identity"),
    supplement: 0,
};

#[derive(Clone, Copy, Debug)]
struct FontRefs {
    type0: Ref,
    cid: Ref,
    descriptor: Ref,
    file: Ref,
    to_unicode: Ref,
}

impl FontRefs {
    const fn starting_at(first: i32) -> Self {
        Self {
            type0: Ref::new(first),
            cid: Ref::new(first + 1),
            descriptor: Ref::new(first + 2),
            file: Ref::new(first + 3),
            to_unicode: Ref::new(first + 4),
        }
    }
}

/// Metrics of the bundled font, in em.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    pub advance: f32,
    pub ascender: f32,
    pub descender: f32,
}

/// The bundled regular font's metrics, which [`Geometry::new`] needs.
///
/// # Errors
/// Returns [`PrintError::Font`] if the bundled font cannot be read.
pub fn bundled_metrics() -> Result<FontMetrics, PrintError> {
    let font = EmbeddedFont::new(REGULAR_TTF, "JetBrainsMono-Regular", REGULAR_REFS)?;
    Ok(font.metrics())
}

#[derive(Debug)]
struct EmbeddedFont {
    face: ttf_parser::Face<'static>,
    data: &'static [u8],
    name: &'static str,
    refs: FontRefs,
    glyphs: HashMap<char, u16>,
    /// Glyph to character, for copying text out of the PDF.
    used: BTreeMap<u16, char>,
}

impl EmbeddedFont {
    fn new(data: &'static [u8], name: &'static str, refs: FontRefs) -> Result<Self, PrintError> {
        let face = ttf_parser::Face::parse(data, 0)
            .map_err(|error| PrintError::Font(format!("{name}: {error}")))?;
        Ok(Self {
            face,
            data,
            name,
            refs,
            glyphs: HashMap::new(),
            used: BTreeMap::new(),
        })
    }

    fn units(&self) -> f32 {
        f32::from(self.face.units_per_em())
    }

    fn metrics(&self) -> FontMetrics {
        let advance = self
            .face
            .glyph_index('0')
            .and_then(|glyph| self.face.glyph_hor_advance(glyph))
            .unwrap_or(600);
        FontMetrics {
            advance: f32::from(advance) / self.units(),
            ascender: f32::from(self.face.ascender()) / self.units(),
            descender: f32::from(self.face.descender()) / self.units(),
        }
    }

    /// The glyph for `character`, or `None` when the font lacks it (the cell is left blank).
    fn glyph(&mut self, character: char) -> Option<u16> {
        if let Some(glyph) = self.glyphs.get(&character) {
            return (*glyph != 0).then_some(*glyph);
        }
        let glyph = self.face.glyph_index(character).map_or(0, |glyph| glyph.0);
        self.glyphs.insert(character, glyph);
        if glyph == 0 {
            return None;
        }
        self.used.entry(glyph).or_insert(character);
        Some(glyph)
    }

    /// Writes the font objects: a Type 0 font over a CID TrueType font, the
    /// whole font file (compressed), and the map from glyphs back to text.
    fn write(&self, chunks: &mut ObjectSink<'_>) -> Result<(), PrintError> {
        let units = self.units();
        let scale = |value: i16| f32::from(value) * 1000.0 / units;
        let name = Name(self.name.as_bytes());
        let metrics = self.metrics();

        let mut chunk = Chunk::new();
        chunk
            .type0_font(self.refs.type0)
            .base_font(name)
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(self.refs.cid)
            .to_unicode(self.refs.to_unicode);
        chunks.write(&chunk)?;

        let mut chunk = Chunk::new();
        chunk
            .cid_font(self.refs.cid)
            .subtype(CidFontType::Type2)
            .base_font(name)
            .system_info(SYSTEM_INFO)
            .font_descriptor(self.refs.descriptor)
            .default_width(metrics.advance * 1000.0)
            .cid_to_gid_map_predefined(Name(b"Identity"));
        chunks.write(&chunk)?;

        let bbox = self.face.global_bounding_box();
        let mut chunk = Chunk::new();
        chunk
            .font_descriptor(self.refs.descriptor)
            .name(name)
            .flags(FontFlags::FIXED_PITCH | FontFlags::SYMBOLIC)
            .bbox(Rect::new(
                scale(bbox.x_min),
                scale(bbox.y_min),
                scale(bbox.x_max),
                scale(bbox.y_max),
            ))
            .italic_angle(0.0)
            .ascent(scale(self.face.ascender()))
            .descent(scale(self.face.descender()))
            .cap_height(scale(
                self.face.capital_height().unwrap_or(self.face.ascender()),
            ))
            .stem_v(80.0)
            .font_file2(self.refs.file);
        chunks.write(&chunk)?;

        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(self.data, 6);
        let mut chunk = Chunk::new();
        chunk
            .stream(self.refs.file, &compressed)
            .filter(Filter::FlateDecode)
            .pair(
                Name(b"Length1"),
                i32::try_from(self.data.len())
                    .map_err(|_| PrintError::Font("font file is too large".into()))?,
            );
        chunks.write(&chunk)?;

        let mut cmap = UnicodeCmap::new(Name(b"Nulis-UCS"), SYSTEM_INFO);
        for (glyph, character) in &self.used {
            cmap.pair(*glyph, *character);
        }
        let cmap = cmap.finish();
        let mut chunk = Chunk::new();
        chunk
            .cmap(self.refs.to_unicode, &cmap)
            .system_info(SYSTEM_INFO);
        chunks.write(&chunk)
    }
}

/// Counts bytes written and records each object's offset.
struct ObjectSink<'a> {
    out: &'a mut dyn Write,
    position: usize,
    offsets: &'a mut Vec<(i32, usize)>,
}

impl ObjectSink<'_> {
    /// Writes a chunk holding exactly one object.
    fn write(&mut self, chunk: &Chunk) -> Result<(), PrintError> {
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
    regular: EmbeddedFont,
    bold: EmbeddedFont,
    bold_used: bool,
    page_refs: Vec<Ref>,
    next_ref: i32,
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
    /// Starts a PDF on `out`.
    ///
    /// # Errors
    /// Returns an error if the fonts cannot be read or `out` cannot be written.
    pub fn new(out: W, geometry: Geometry, palette: Palette) -> Result<Self, PrintError> {
        let mut writer = Self {
            out,
            position: 0,
            offsets: Vec::new(),
            geometry,
            palette,
            regular: EmbeddedFont::new(REGULAR_TTF, "JetBrainsMono-Regular", REGULAR_REFS)?,
            bold: EmbeddedFont::new(BOLD_TTF, "JetBrainsMono-Bold", BOLD_REFS)?,
            bold_used: false,
            page_refs: Vec::new(),
            next_ref: FIRST_DYNAMIC_REF,
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
        let reference = Ref::new(self.next_ref);
        self.next_ref = self
            .next_ref
            .checked_add(1)
            .ok_or_else(|| PrintError::Setup("too many pages for one PDF".into()))?;
        Ok(reference)
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
        let mut page_bold = false;
        if let Some(background) = self.palette.background {
            let [r, g, b] = background.unit();
            content
                .set_fill_rgb(r, g, b)
                .rect(0.0, 0.0, geometry.page_width, geometry.page_height)
                .fill_nonzero();
        }
        content.begin_text();
        let mut pen = Pen::default();
        if geometry.has_header {
            let band = self.palette.band;
            self.draw_band(&mut content, &mut pen, header, geometry.top, band);
        }
        let digits = geometry.gutter_columns.saturating_sub(1);
        for (index, row) in page.rows.iter().enumerate() {
            let top = geometry.body_row_top(u32::try_from(index).unwrap_or(u32::MAX));
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
                };
                let color = self.palette.line_number;
                self.draw_text(&mut content, &mut pen, &label, 0, top, color, false);
            }
            for run in &row.runs {
                let (color, bold) = self.run_style(run);
                page_bold |= bold;
                self.draw_text(
                    &mut content,
                    &mut pen,
                    &run.text,
                    geometry.gutter_columns + run.column,
                    top,
                    color,
                    bold,
                );
            }
        }
        if geometry.has_footer {
            let band = self.palette.band;
            self.draw_band(&mut content, &mut pen, footer, geometry.footer_top(), band);
        }
        content.end_text();
        self.bold_used |= page_bold;

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
        fonts.pair(Name(b"F1"), REGULAR_REFS.type0);
        if page_bold {
            fonts.pair(Name(b"F2"), BOLD_REFS.type0);
        }
        fonts.finish();
        resources.finish();
        page_writer.finish();
        self.write_chunk(&chunk)?;
        self.page_refs.push(page_ref);
        Ok(())
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
    ) {
        let columns = self.geometry.columns;
        let width = |text: &str| u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
        let fit = |text: &str| {
            text.chars()
                .take(usize::try_from(columns).unwrap_or(0))
                .collect::<String>()
        };
        let [left, center, right] = parts.each_ref().map(|part| fit(part));
        if !left.is_empty() {
            self.draw_text(content, pen, &left, 0, top, color, false);
        }
        if !center.is_empty() {
            let column = columns.saturating_sub(width(&center)) / 2;
            self.draw_text(content, pen, &center, column, top, color, false);
        }
        if !right.is_empty() {
            let column = columns.saturating_sub(width(&right));
            self.draw_text(content, pen, &right, column, top, color, false);
        }
    }

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
    ) {
        let geometry = self.geometry;
        let font = if bold {
            &mut self.bold
        } else {
            &mut self.regular
        };
        // Each character is placed in its own cell, so a missing glyph or a
        // font with odd widths never shifts the columns after it.
        let mut glyphs = Vec::with_capacity(text.len() * 2);
        let mut start = column;
        let flush = |content: &mut Content, pen: &mut Pen, glyphs: &mut Vec<u8>, start: u32| {
            if glyphs.is_empty() {
                return;
            }
            pen.select(content, bold, geometry.font_size, color);
            #[allow(clippy::cast_precision_loss)] // Columns are far below 2^24.
            let x = geometry.left + start as f32 * geometry.advance;
            content
                .set_text_matrix([1.0, 0.0, 0.0, 1.0, x, top - geometry.baseline])
                .show(Str(glyphs));
            glyphs.clear();
        };
        for (cell, character) in (column..).zip(text.chars()) {
            if let Some(glyph) = font.glyph(character) {
                glyphs.extend_from_slice(&glyph.to_be_bytes());
            } else {
                // Nothing is drawn; the next glyphs start after this cell.
                self.missing_glyphs += u64::from(!character.is_whitespace());
                flush(content, pen, &mut glyphs, start);
                start = cell + 1;
            }
        }
        flush(content, pen, &mut glyphs, start);
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
        self.regular.write(&mut sink)?;
        if self.bold_used {
            self.bold.write(&mut sink)?;
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
        let size = self.next_ref;
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
            // References allocated but never written (unused bold font) are free.
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
    bold: Option<bool>,
    color: Option<Rgb>,
}

impl Pen {
    fn select(&mut self, content: &mut Content, bold: bool, size: f32, color: Rgb) {
        if self.bold != Some(bold) {
            content.set_font(Name(if bold { b"F2" } else { b"F1" }), size);
            self.bold = Some(bold);
        }
        if self.color != Some(color) {
            let [r, g, b] = color.unit();
            content.set_fill_rgb(r, g, b);
            self.color = Some(color);
        }
    }
}
