//! Page Setup: paper, margins, font size, and what goes on each page.

use crate::PrintError;
use serde::{Deserialize, Serialize};

const POINTS_PER_MM: f32 = 72.0 / 25.4;
/// Line height as a multiple of the font size.
const LINE_HEIGHT_EM: f32 = 1.3;
/// Narrowest text area accepted, in characters.
const MIN_TEXT_COLUMNS: u32 = 10;
/// Longest header or footer template accepted, in bytes.
pub const MAX_TEMPLATE_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PaperSize {
    A3,
    A4,
    A5,
    Letter,
    Legal,
}

impl PaperSize {
    /// Portrait width and height in millimetres.
    #[must_use]
    pub fn millimetres(self) -> (f32, f32) {
        match self {
            Self::A3 => (297.0, 420.0),
            Self::A4 => (210.0, 297.0),
            Self::A5 => (148.0, 210.0),
            Self::Letter => (215.9, 279.4),
            Self::Legal => (215.9, 355.6),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// What happens to a line longer than the text area.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LongLines {
    /// Continue on the next row, marked in the gutter.
    Wrap,
    /// Stop at the edge, marked with an ellipsis.
    Cut,
}

/// Margins in millimetres.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Margins {
    #[must_use]
    pub const fn uniform(millimetres: f32) -> Self {
        Self {
            top: millimetres,
            right: millimetres,
            bottom: millimetres,
            left: millimetres,
        }
    }
}

/// A header or footer: text at the left, centre, and right. Each part may
/// use the fields `{file}`, `{path}`, `{date}`, `{page}`, and `{pages}`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Band {
    pub left: String,
    pub center: String,
    pub right: String,
}

impl Band {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.left.is_empty() && self.center.is_empty() && self.right.is_empty()
    }

    /// Whether any part uses `{pages}`, which needs a counting pass first.
    #[must_use]
    pub fn uses_page_count(&self) -> bool {
        [&self.left, &self.center, &self.right]
            .iter()
            .any(|part| part.contains("{pages}"))
    }

    fn parts(&self) -> [&String; 3] {
        [&self.left, &self.center, &self.right]
    }
}

/// Values for the header and footer fields.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BandFields {
    pub file: String,
    pub path: String,
    pub date: String,
    pub pages: Option<u64>,
}

impl BandFields {
    /// The band's three parts with every field filled in for `page`.
    #[must_use]
    pub fn resolve(&self, band: &Band, page: u64) -> [String; 3] {
        let pages = self
            .pages
            .map_or_else(|| "?".to_owned(), |pages| pages.to_string());
        band.parts().map(|part| {
            part.replace("{file}", &self.file)
                .replace("{path}", &self.path)
                .replace("{date}", &self.date)
                .replace("{pages}", &pages)
                .replace("{page}", &page.to_string())
        })
    }
}

/// Where a header's or footer's three parts start on a row `columns` wide:
/// left at the edge, centre in the middle, right against the far edge. Each
/// part is cut to the row; empty parts are left out.
pub(crate) fn band_layout(columns: u32, parts: &[String; 3]) -> Vec<(u32, String)> {
    let width = |text: &str| text.chars().map(crate::layout::cell_width).sum::<u32>();
    let fit = |text: &str| {
        text.chars()
            .take(usize::try_from(columns).unwrap_or(0))
            .collect::<String>()
    };
    let [left, center, right] = parts.each_ref().map(|part| fit(part));
    let mut placed = Vec::with_capacity(3);
    if !left.is_empty() {
        placed.push((0, left));
    }
    if !center.is_empty() {
        placed.push((columns.saturating_sub(width(&center)) / 2, center));
    }
    if !right.is_empty() {
        placed.push((columns.saturating_sub(width(&right)), right));
    }
    placed
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct PageSetup {
    pub paper: PaperSize,
    pub orientation: Orientation,
    pub margins: Margins,
    /// Text size in points.
    pub font_size: f32,
    pub line_numbers: bool,
    pub long_lines: LongLines,
    pub syntax_highlighting: bool,
    /// Columns between tab stops.
    pub tab_width: u32,
    pub header: Band,
    pub footer: Band,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            paper: PaperSize::A4,
            orientation: Orientation::Portrait,
            margins: Margins::uniform(15.0),
            font_size: 9.0,
            line_numbers: true,
            long_lines: LongLines::Wrap,
            syntax_highlighting: true,
            tab_width: 4,
            header: Band {
                left: "{file}".into(),
                center: String::new(),
                right: "{date}".into(),
            },
            footer: Band {
                left: String::new(),
                center: "Page {page} of {pages}".into(),
                right: String::new(),
            },
        }
    }
}

impl PageSetup {
    /// Checks every value is in range.
    ///
    /// # Errors
    /// Returns [`PrintError::Setup`] naming the first value out of range.
    pub fn validate(&self) -> Result<(), PrintError> {
        let margins = [
            self.margins.top,
            self.margins.right,
            self.margins.bottom,
            self.margins.left,
        ];
        if !margins
            .iter()
            .all(|margin| margin.is_finite() && (0.0..=80.0).contains(margin))
        {
            return Err(PrintError::Setup(
                "margins must be between 0 and 80 mm".into(),
            ));
        }
        if !self.font_size.is_finite() || !(6.0..=24.0).contains(&self.font_size) {
            return Err(PrintError::Setup(
                "font size must be between 6 and 24 points".into(),
            ));
        }
        if !(1..=16).contains(&self.tab_width) {
            return Err(PrintError::Setup(
                "tab width must be between 1 and 16".into(),
            ));
        }
        let templates = self.header.parts().into_iter().chain(self.footer.parts());
        if templates
            .into_iter()
            .any(|part| part.len() > MAX_TEMPLATE_BYTES || part.chars().any(char::is_control))
        {
            return Err(PrintError::Setup(format!(
                "header and footer parts are one line of at most {MAX_TEMPLATE_BYTES} bytes"
            )));
        }
        Ok(())
    }

    /// Page width and height in points, after orientation.
    #[must_use]
    pub fn page_size_points(&self) -> (f32, f32) {
        let (width, height) = self.paper.millimetres();
        let (width, height) = match self.orientation {
            Orientation::Portrait => (width, height),
            Orientation::Landscape => (height, width),
        };
        (width * POINTS_PER_MM, height * POINTS_PER_MM)
    }
}

/// Where everything goes on a page, in points from the bottom-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub page_width: f32,
    pub page_height: f32,
    pub left: f32,
    /// Top of the first header or body row.
    pub top: f32,
    pub font_size: f32,
    pub line_height: f32,
    /// Width of one character cell.
    pub advance: f32,
    /// Distance from a row's top to its text baseline.
    pub baseline: f32,
    /// Characters across the margin box.
    pub columns: u32,
    /// Characters reserved for line numbers and a space (0 without line numbers).
    pub gutter_columns: u32,
    /// Body rows per page.
    pub rows: u32,
    pub has_header: bool,
    pub has_footer: bool,
}

impl Geometry {
    /// Lays out `setup` for a monospace font whose cells are `advance_em`
    /// wide and whose ascender and descender are given in em, with room in the
    /// gutter for line numbers up to `last_line`.
    ///
    /// # Errors
    /// Returns [`PrintError::Setup`] when the margins leave too little room.
    pub fn new(
        setup: &PageSetup,
        advance_em: f32,
        ascender_em: f32,
        descender_em: f32,
        last_line: u64,
    ) -> Result<Self, PrintError> {
        setup.validate()?;
        let (page_width, page_height) = setup.page_size_points();
        let margins = setup.margins;
        let box_width = page_width - (margins.left + margins.right) * POINTS_PER_MM;
        let box_height = page_height - (margins.top + margins.bottom) * POINTS_PER_MM;
        let line_height = setup.font_size * LINE_HEIGHT_EM;
        let advance = setup.font_size * advance_em;
        let text_height = setup.font_size * (ascender_em + descender_em.abs());
        let baseline = (line_height - text_height) / 2.0 + setup.font_size * ascender_em;
        let has_header = !setup.header.is_empty();
        let has_footer = !setup.footer.is_empty();
        // A header or footer takes its row and half a row of space.
        let bands = 1.5 * (f32::from(u8::from(has_header)) + f32::from(u8::from(has_footer)));
        let rows = floor_u32(box_height / line_height - bands);
        let columns = floor_u32(box_width / advance);
        let gutter_columns = if setup.line_numbers {
            decimal_digits(last_line.max(1)).max(3) + 1
        } else {
            0
        };
        if rows < 1 || columns < gutter_columns + MIN_TEXT_COLUMNS {
            return Err(PrintError::Setup(
                "the margins leave too little room for text".into(),
            ));
        }
        Ok(Self {
            page_width,
            page_height,
            left: margins.left * POINTS_PER_MM,
            top: page_height - margins.top * POINTS_PER_MM,
            font_size: setup.font_size,
            line_height,
            advance,
            baseline,
            columns,
            gutter_columns,
            rows,
            has_header,
            has_footer,
        })
    }

    /// Characters available for text after the gutter.
    #[must_use]
    pub fn text_columns(&self) -> u32 {
        self.columns - self.gutter_columns
    }

    /// Top of body row `row` (0-based).
    #[must_use]
    pub fn body_row_top(&self, row: u32) -> f32 {
        let header = if self.has_header { 1.5 } else { 0.0 };
        self.top - (header + row_f32(row)) * self.line_height
    }

    /// Top of the footer row.
    #[must_use]
    pub fn footer_top(&self) -> f32 {
        self.body_row_top(self.rows) - 0.5 * self.line_height
    }
}

#[allow(clippy::cast_precision_loss)] // Rows per page are far below 2^24.
fn row_f32(row: u32) -> f32 {
    row as f32
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped to 0..=65535 first.
fn floor_u32(value: f32) -> u32 {
    if value.is_finite() {
        value.floor().clamp(0.0, f32::from(u16::MAX)) as u32
    } else {
        0
    }
}

/// Number of decimal digits in `value`.
#[must_use]
pub fn decimal_digits(value: u64) -> u32 {
    value.checked_ilog10().map_or(1, |digits| digits + 1)
}

#[cfg(test)]
mod tests {
    use super::{decimal_digits, Band, BandFields, Geometry, Orientation, PageSetup, PaperSize};

    #[test]
    fn default_a4_geometry_fits_the_margin_box() {
        let setup = PageSetup::default();
        let geometry = Geometry::new(&setup, 0.6, 1.02, -0.3, 120).expect("geometry");
        assert!((geometry.page_width - 595.28).abs() < 0.01);
        assert!((geometry.page_height - 841.89).abs() < 0.01);
        // (210 − 30) mm = 510.2 pt over 5.4 pt cells.
        assert_eq!(geometry.columns, 94);
        assert_eq!(geometry.gutter_columns, 4);
        // (297 − 30) mm = 756.9 pt over 11.7 pt rows, minus 3 rows of bands.
        assert_eq!(geometry.rows, 61);
        assert!(
            geometry.footer_top() - geometry.line_height
                >= setup.margins.bottom * 72.0 / 25.4 - 0.01
        );
    }

    #[test]
    fn landscape_swaps_the_paper_and_the_gutter_grows_with_line_numbers() {
        let setup = PageSetup {
            paper: PaperSize::Letter,
            orientation: Orientation::Landscape,
            ..PageSetup::default()
        };
        let (width, height) = setup.page_size_points();
        assert!(width > height);
        let geometry = Geometry::new(&setup, 0.6, 1.0, -0.3, 1_234_567).expect("geometry");
        assert_eq!(geometry.gutter_columns, 8);
    }

    #[test]
    fn bad_values_are_refused() {
        let wide = PageSetup {
            margins: super::Margins::uniform(80.0),
            paper: PaperSize::A5,
            ..PageSetup::default()
        };
        assert!(Geometry::new(&wide, 0.6, 1.0, -0.3, 1).is_err());
        for setup in [
            PageSetup {
                font_size: 2.0,
                ..PageSetup::default()
            },
            PageSetup {
                tab_width: 0,
                ..PageSetup::default()
            },
            PageSetup {
                margins: super::Margins::uniform(f32::NAN),
                ..PageSetup::default()
            },
            PageSetup {
                header: Band {
                    left: "a\nb".into(),
                    ..Band::default()
                },
                ..PageSetup::default()
            },
        ] {
            assert!(setup.validate().is_err(), "{setup:?}");
        }
    }

    #[test]
    fn band_fields_are_filled_in() {
        let fields = BandFields {
            file: "notes.txt".into(),
            path: "/tmp/notes.txt".into(),
            date: "21 October 2026".into(),
            pages: Some(12),
        };
        let band = Band {
            left: "{file} ({path})".into(),
            center: "Page {page} of {pages}".into(),
            right: "{date}".into(),
        };
        assert_eq!(
            fields.resolve(&band, 3),
            [
                "notes.txt (/tmp/notes.txt)".to_owned(),
                "Page 3 of 12".to_owned(),
                "21 October 2026".to_owned()
            ]
        );
        assert!(band.uses_page_count());
        assert_eq!(decimal_digits(0), 1);
        assert_eq!(decimal_digits(999), 3);
        assert_eq!(decimal_digits(1000), 4);
    }
}
