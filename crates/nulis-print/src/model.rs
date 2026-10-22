//! Pages as positioned text, for the Print Preview window.
//!
//! The PDF writer and Print use the same [`Page`]s from the [`Paginator`]
//! (`crate::Paginator`); this module turns one into text pieces placed on a
//! grid of character cells, which the window draws as absolutely positioned
//! HTML. A character the bundled font lacks, and every wide character, is a
//! piece of its own, so a fallback font with other widths never shifts the
//! columns after it (the PDF writer places such glyphs one by one too).

use crate::{
    fonts::bundled_has_glyph,
    layout::{cell_width, Gutter, Page, Style},
    palette::{token_index, Palette, Rgb},
    pdf::CONTINUATION_MARKER,
    setup::{band_layout, Geometry, PageSetup},
};
use serde::Serialize;

/// Style indices in [`PageFrame::styles`]; syntax tokens follow from
/// [`STYLE_FIRST_TOKEN`] in `nulis_syntax::SyntaxToken` order.
pub const STYLE_TEXT: u8 = 0;
pub const STYLE_SYMBOL: u8 = 1;
pub const STYLE_LINE_NUMBER: u8 = 2;
pub const STYLE_BAND: u8 = 3;
pub const STYLE_FIRST_TOKEN: u8 = 4;

const MM_PER_POINT: f32 = 25.4 / 72.0;

/// Text starting at a cell: `(column, cells, style, text)`. Columns count
/// from the left margin, header and footer included.
pub type Piece = (u32, u32, u8, String);

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PieceStyle {
    /// `#rrggbb`.
    pub color: String,
    pub bold: bool,
}

/// What every page shares: size, grid, and colors. Lengths are in points
/// and measured from the page's top-left corner.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageFrame {
    /// Page size after orientation.
    pub width_mm: f32,
    pub height_mm: f32,
    /// Paper size before orientation, as the print dialog takes it.
    pub paper_width_mm: f32,
    pub paper_height_mm: f32,
    pub landscape: bool,
    pub margin_left_pt: f32,
    pub header_top_pt: f32,
    pub body_top_pt: f32,
    pub footer_top_pt: f32,
    pub font_size_pt: f32,
    pub line_height_pt: f32,
    /// Width of one character cell.
    pub cell_pt: f32,
    /// Cells across the margin box.
    pub columns: u32,
    pub rows: u32,
    pub has_header: bool,
    pub has_footer: bool,
    /// Page background; `None` leaves the paper white.
    pub background: Option<String>,
    /// Color of the rule between printed ranges.
    pub rule_color: String,
    pub styles: Vec<PieceStyle>,
}

impl PageFrame {
    #[must_use]
    pub fn new(setup: &PageSetup, geometry: &Geometry, palette: &Palette) -> Self {
        let (paper_width_mm, paper_height_mm) = setup.paper.millimetres();
        let style = |color: Rgb, bold: bool| PieceStyle {
            color: hex(color),
            bold,
        };
        let mut styles = vec![
            style(palette.text, false),
            style(palette.symbol, false),
            style(palette.line_number, false),
            style(palette.band, false),
        ];
        styles.extend(
            palette
                .token_styles()
                .iter()
                .map(|token| style(token.color, token.bold)),
        );
        Self {
            width_mm: geometry.page_width * MM_PER_POINT,
            height_mm: geometry.page_height * MM_PER_POINT,
            paper_width_mm,
            paper_height_mm,
            landscape: setup.orientation == crate::Orientation::Landscape,
            margin_left_pt: geometry.left,
            header_top_pt: geometry.page_height - geometry.top,
            body_top_pt: geometry.page_height - geometry.body_row_top(0),
            footer_top_pt: geometry.page_height - geometry.footer_top(),
            font_size_pt: geometry.font_size,
            line_height_pt: geometry.line_height,
            cell_pt: geometry.advance,
            columns: geometry.columns,
            rows: geometry.rows,
            has_header: geometry.has_header,
            has_footer: geometry.has_footer,
            background: palette.background.map(hex),
            rule_color: hex(palette.line_number),
            styles,
        }
    }
}

/// One page, ready to draw.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintedPage {
    pub number: u64,
    pub header: Vec<Piece>,
    pub footer: Vec<Piece>,
    /// Body rows from the top; a separator row is empty.
    pub rows: Vec<Vec<Piece>>,
    /// Rows that are a rule between two printed ranges.
    pub rules: Vec<u32>,
}

/// Places `page` with its resolved header and footer, as the PDF writer
/// draws it.
#[must_use]
pub fn place_page(
    geometry: &Geometry,
    page: &Page,
    header: &[String; 3],
    footer: &[String; 3],
) -> PrintedPage {
    let mut placed = PrintedPage {
        number: page.number,
        ..PrintedPage::default()
    };
    if geometry.has_header {
        for (column, text) in band_layout(geometry.columns, header) {
            push_pieces(&mut placed.header, column, &text, STYLE_BAND);
        }
    }
    if geometry.has_footer {
        for (column, text) in band_layout(geometry.columns, footer) {
            push_pieces(&mut placed.footer, column, &text, STYLE_BAND);
        }
    }
    let digits = usize::try_from(geometry.gutter_columns.saturating_sub(1)).unwrap_or(0);
    for (index, row) in page.rows.iter().enumerate() {
        let mut pieces = Vec::new();
        if row.gutter == Gutter::Separator {
            placed.rules.push(u32::try_from(index).unwrap_or(u32::MAX));
            placed.rows.push(pieces);
            continue;
        }
        if digits > 0 {
            let label = match row.gutter {
                Gutter::Number(number) => format!("{number:>digits$}"),
                Gutter::Continuation => format!("{CONTINUATION_MARKER:>digits$}"),
                Gutter::Separator => String::new(),
            };
            push_pieces(&mut pieces, 0, &label, STYLE_LINE_NUMBER);
        }
        for run in &row.runs {
            let style = match run.style {
                Style::Plain => STYLE_TEXT,
                Style::Symbol => STYLE_SYMBOL,
                Style::Token(token) => {
                    STYLE_FIRST_TOKEN + u8::try_from(token_index(token)).unwrap_or(0)
                }
            };
            push_pieces(
                &mut pieces,
                geometry.gutter_columns + run.column,
                &run.text,
                style,
            );
        }
        placed.rows.push(pieces);
    }
    placed
}

/// Appends `text` from `column` as pieces: runs of narrow characters the
/// bundled font has, and every other character on its own.
fn push_pieces(pieces: &mut Vec<Piece>, column: u32, text: &str, style: u8) {
    let mut start = column;
    let mut cell = column;
    let mut batch = String::new();
    for character in text.chars() {
        let width = cell_width(character);
        if width == 1 && bundled_has_glyph(character) {
            if batch.is_empty() {
                start = cell;
            }
            batch.push(character);
        } else {
            if !batch.is_empty() {
                pieces.push((start, cell - start, style, std::mem::take(&mut batch)));
            }
            if !character.is_whitespace() {
                pieces.push((cell, width, style, character.to_string()));
            }
        }
        cell += width;
    }
    if !batch.is_empty() {
        pieces.push((start, cell - start, style, batch));
    }
}

fn hex(color: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", color.0, color.1, color.2)
}

#[cfg(test)]
mod tests {
    use super::{place_page, push_pieces, PageFrame, STYLE_BAND, STYLE_LINE_NUMBER, STYLE_TEXT};
    use crate::{
        layout::{Gutter, Page, Row, Run, Style},
        setup::{Geometry, PageSetup},
        Palette,
    };

    fn geometry(setup: &PageSetup) -> Geometry {
        let metrics = crate::bundled_metrics().expect("metrics");
        Geometry::new(
            setup,
            metrics.advance,
            metrics.ascender,
            metrics.descender,
            120,
        )
        .expect("geometry")
    }

    #[test]
    fn wide_and_missing_characters_are_pieces_of_their_own() {
        let mut pieces = Vec::new();
        push_pieces(&mut pieces, 2, "ab中c\u{A9A0}d e", STYLE_TEXT);
        assert_eq!(
            pieces,
            [
                (2, 2, STYLE_TEXT, "ab".to_owned()),
                (4, 2, STYLE_TEXT, "中".to_owned()),
                (6, 1, STYLE_TEXT, "c".to_owned()),
                (7, 1, STYLE_TEXT, "\u{A9A0}".to_owned()),
                (8, 3, STYLE_TEXT, "d e".to_owned()),
            ]
        );
    }

    #[test]
    fn a_page_places_bands_gutter_runs_and_rules_like_the_pdf() {
        let setup = PageSetup::default();
        let geometry = geometry(&setup);
        let page = Page {
            number: 2,
            rows: vec![
                Row {
                    gutter: Gutter::Number(7),
                    runs: vec![Run {
                        column: 0,
                        text: "let".into(),
                        style: Style::Plain,
                        cells: 3,
                    }],
                },
                Row {
                    gutter: Gutter::Separator,
                    runs: Vec::new(),
                },
                Row {
                    gutter: Gutter::Continuation,
                    runs: Vec::new(),
                },
            ],
        };
        let header = ["notes.txt".to_owned(), String::new(), "today".to_owned()];
        let footer = [String::new(), "Page 2 of 3".to_owned(), String::new()];
        let placed = place_page(&geometry, &page, &header, &footer);
        let columns = geometry.columns;
        assert_eq!(
            placed.header,
            [
                (0, 9, STYLE_BAND, "notes.txt".to_owned()),
                (columns - 5, 5, STYLE_BAND, "today".to_owned()),
            ]
        );
        assert_eq!(placed.footer[0].0, (columns - 11) / 2);
        // A three-digit gutter and a space: "  7", then the text.
        assert_eq!(
            placed.rows[0][0],
            (0, 3, STYLE_LINE_NUMBER, "  7".to_owned())
        );
        assert_eq!(placed.rows[0][1], (4, 3, STYLE_TEXT, "let".to_owned()));
        assert_eq!(placed.rules, [1]);
        assert_eq!(placed.rows[1], Vec::new());
        // The continuation arrow is in the bundled font.
        assert_eq!(
            placed.rows[2][0],
            (0, 3, STYLE_LINE_NUMBER, "  ↪".to_owned())
        );

        let frame = PageFrame::new(&setup, &geometry, &Palette::print());
        assert!((frame.width_mm - 210.0).abs() < 0.01);
        assert!((frame.header_top_pt - 15.0 * 72.0 / 25.4).abs() < 0.01);
        assert!(frame.body_top_pt > frame.header_top_pt);
        assert!(frame.footer_top_pt > frame.body_top_pt);
        assert_eq!(frame.styles.len(), 4 + 9);
        assert_eq!(frame.background, None);
    }
}
