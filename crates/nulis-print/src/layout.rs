//! Turns styled source lines into pages of rows.
//!
//! Lines arrive one at a time and finished pages leave as soon as they fill,
//! so memory stays bounded by one page whatever the document's size. A very
//! long line may arrive in several segments; a segment marked as a
//! continuation carries on the row the previous segment left open.

use crate::setup::LongLines;
use nulis_syntax::SyntaxToken;

/// A highlighted range of a line's characters (indices into `chars()`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub token: SyntaxToken,
}

/// One line of the document, or one segment of a very long line.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceLine {
    /// 1-based line number.
    pub number: u64,
    /// Carries on the previous segment of the same line.
    pub continuation: bool,
    /// Starts a new printed range (the next selection): a rule is drawn before it.
    pub separator_before: bool,
    pub text: String,
    /// Sorted, non-overlapping highlights.
    pub spans: Vec<Span>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Style {
    Plain,
    Token(SyntaxToken),
    /// A visible stand-in: a control character's picture or the cut marker.
    Symbol,
}

/// Text in one style starting at a column of the row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Run {
    pub column: u32,
    pub text: String,
    pub style: Style,
    /// Cells the text covers (wide characters take two).
    pub cells: u32,
}

/// Cells a character takes: two for East Asian wide and full-width
/// characters (Chinese, Japanese, Korean, full-width forms, most emoji),
/// one for everything else.
#[must_use]
pub fn cell_width(character: char) -> u32 {
    let wide = matches!(
        u32::from(character),
        0x1100..=0x115F
            | 0x231A..=0x231B
            | 0x2329..=0x232A
            | 0x23E9..=0x23EC
            | 0x2E80..=0x303E
            | 0x3041..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xA000..=0xA4CF
            | 0xA960..=0xA97F
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE10..=0xFE19
            | 0xFE30..=0xFE6F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1F64F
            | 0x1F900..=0x1F9FF
            | 0x20000..=0x2FFFD
            | 0x30000..=0x3FFFD
    );
    if wide {
        2
    } else {
        1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gutter {
    Number(u64),
    /// A wrapped row of the line above.
    Continuation,
    /// A rule between two printed ranges; the row has no text.
    Separator,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Row {
    pub gutter: Gutter,
    pub runs: Vec<Run>,
}

impl Row {
    fn new(gutter: Gutter) -> Self {
        Self {
            gutter,
            runs: Vec::new(),
        }
    }

    fn push(&mut self, column: u32, character: char, style: Style) {
        let width = cell_width(character);
        if let Some(last) = self.runs.last_mut() {
            if last.style == style && last.column + last.cells == column {
                last.text.push(character);
                last.cells += width;
                return;
            }
        }
        self.runs.push(Run {
            column,
            text: character.to_string(),
            style,
            cells: width,
        });
    }

    /// The column after the last character.
    fn end(&self) -> u32 {
        self.runs.last().map_or(0, |run| run.column + run.cells)
    }

    /// Removes and returns the cells after the row's last space at or past
    /// `min_column`, so a wrapped line breaks between words. `None` when
    /// there is no such space (a word longer than the rest of the row).
    fn take_after_last_space(&mut self, min_column: u32) -> Option<Vec<(char, Style)>> {
        let cells = |run: &Run| {
            let mut column = run.column;
            run.text
                .chars()
                .map(|character| {
                    let at = column;
                    column += cell_width(character);
                    (at, character)
                })
                .collect::<Vec<_>>()
        };
        let space = self
            .runs
            .iter()
            .flat_map(cells)
            .rev()
            .find(|(column, character)| *character == ' ' && *column >= min_column)
            .map(|(column, _)| column)?;
        let mut tail = Vec::new();
        let mut kept = Vec::with_capacity(self.runs.len());
        for run in std::mem::take(&mut self.runs) {
            let mut head = String::new();
            for (column, character) in cells(&run) {
                if column > space {
                    tail.push((character, run.style));
                } else {
                    head.push(character);
                }
            }
            if !head.is_empty() {
                let cells = head.chars().map(cell_width).sum();
                kept.push(Run {
                    column: run.column,
                    text: head,
                    style: run.style,
                    cells,
                });
            }
        }
        self.runs = kept;
        Some(tail)
    }

    /// Ends the row with `marker` in its last cell, removing characters
    /// from the end until it fits within `columns`.
    fn mark_end(&mut self, columns: u32, marker: char) {
        while self.end() + cell_width(marker) > columns {
            let Some(last) = self.runs.last_mut() else {
                break;
            };
            if let Some(removed) = last.text.pop() {
                last.cells -= cell_width(removed);
            }
            if last.text.is_empty() {
                self.runs.pop();
            }
        }
        let end = self.end();
        self.push(end, marker, Style::Symbol);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page {
    /// 1-based page number.
    pub number: u64,
    pub rows: Vec<Row>,
}

/// Marks a line cut at the edge of the text area.
pub const CUT_MARKER: char = '…';

/// Fills pages row by row.
#[derive(Debug)]
pub struct Paginator {
    rows_per_page: usize,
    columns: u32,
    long_lines: LongLines,
    tab_width: u32,
    page: Page,
    /// The last row of the previous segment, still open for a continuation.
    open: Option<OpenRow>,
}

#[derive(Debug)]
struct OpenRow {
    row: Row,
    /// Next free column.
    column: u32,
    /// Columns from the line's start, for tab stops across wrapped rows.
    line_column: u64,
    /// The line was cut; the rest of it is dropped.
    cut: bool,
}

impl Paginator {
    /// `rows_per_page` and `columns` come from [`crate::Geometry`]; both are at least 1.
    #[must_use]
    pub fn new(rows_per_page: u32, columns: u32, long_lines: LongLines, tab_width: u32) -> Self {
        Self {
            rows_per_page: usize::try_from(rows_per_page.max(1)).unwrap_or(usize::MAX),
            columns: columns.max(1),
            long_lines,
            tab_width: tab_width.max(1),
            page: Page {
                number: 1,
                rows: Vec::new(),
            },
            open: None,
        }
    }

    /// Lays out `line` and returns the pages it completed, usually none or one.
    pub fn push(&mut self, line: &SourceLine) -> Vec<Page> {
        let mut finished = Vec::new();
        let mut open = match self.open.take() {
            Some(open) if line.continuation => open,
            previous => {
                if let Some(previous) = previous {
                    self.commit(previous.row, &mut finished);
                }
                if line.separator_before {
                    self.commit(Row::new(Gutter::Separator), &mut finished);
                }
                OpenRow {
                    row: Row::new(Gutter::Number(line.number)),
                    column: 0,
                    line_column: 0,
                    cut: false,
                }
            }
        };
        let mut spans = line.spans.iter().peekable();
        for (index, character) in line.text.chars().enumerate() {
            if open.cut {
                break;
            }
            while spans.next_if(|span| span.end <= index).is_some() {}
            let style = spans
                .peek()
                .filter(|span| span.start <= index)
                .map_or(Style::Plain, |span| Style::Token(span.token));
            if character == '\t' {
                let width = self.tab_width
                    - u32::try_from(open.line_column % u64::from(self.tab_width)).unwrap_or(0);
                for _ in 0..width {
                    self.place(&mut open, ' ', style, &mut finished);
                }
            } else if let Some(picture) = control_picture(character) {
                self.place(&mut open, picture, Style::Symbol, &mut finished);
            } else {
                self.place(&mut open, character, style, &mut finished);
            }
        }
        self.open = Some(open);
        finished
    }

    /// Ends the document and returns the pages still to be written: the page
    /// the last row filled, if any, then the last page. An empty document
    /// has one empty page.
    #[must_use]
    pub fn finish(mut self) -> Vec<Page> {
        let mut finished = Vec::new();
        if let Some(open) = self.open.take() {
            self.commit(open.row, &mut finished);
        }
        finished.push(self.page);
        finished
    }

    fn place(
        &mut self,
        open: &mut OpenRow,
        character: char,
        style: Style,
        finished: &mut Vec<Page>,
    ) {
        let width = cell_width(character);
        if open.column + width > self.columns {
            match self.long_lines {
                LongLines::Wrap => {
                    // Break after the last space in the second half of the
                    // row; a longer word is split at the edge.
                    let tail = open
                        .row
                        .take_after_last_space(self.columns / 2)
                        .unwrap_or_default();
                    let row = std::mem::replace(&mut open.row, Row::new(Gutter::Continuation));
                    self.commit(row, finished);
                    open.column = 0;
                    for (moved, moved_style) in tail {
                        open.row.push(open.column, moved, moved_style);
                        open.column += cell_width(moved);
                    }
                }
                LongLines::Cut => {
                    open.row.mark_end(self.columns, CUT_MARKER);
                    open.cut = true;
                    return;
                }
            }
        }
        open.row.push(open.column, character, style);
        open.column += width;
        open.line_column += u64::from(width);
    }

    fn commit(&mut self, row: Row, finished: &mut Vec<Page>) {
        if self.page.rows.len() == self.rows_per_page {
            let number = self.page.number + 1;
            let full = std::mem::replace(
                &mut self.page,
                Page {
                    number,
                    rows: Vec::with_capacity(self.rows_per_page),
                },
            );
            finished.push(full);
        }
        self.page.rows.push(row);
    }
}

/// The visible picture for a control character (U+2400 block), if it is one.
fn control_picture(character: char) -> Option<char> {
    match character {
        '\u{0}'..='\u{1F}' => char::from_u32(0x2400 + u32::from(character)),
        '\u{7F}' => Some('\u{2421}'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Gutter, Page, Paginator, SourceLine, Span, Style, CUT_MARKER};
    use crate::setup::LongLines;
    use nulis_syntax::SyntaxToken;

    fn line(number: u64, text: &str) -> SourceLine {
        SourceLine {
            number,
            text: text.into(),
            ..SourceLine::default()
        }
    }

    fn row_text(page: &Page, row: usize) -> String {
        let mut text = String::new();
        for run in &page.rows[row].runs {
            while u32::try_from(text.chars().count()).expect("small") < run.column {
                text.push(' ');
            }
            text.push_str(&run.text);
        }
        text
    }

    fn all_pages(paginator: Paginator, lines: &[SourceLine]) -> Vec<Page> {
        let mut paginator = paginator;
        let mut pages = Vec::new();
        for line in lines {
            pages.extend(paginator.push(line));
        }
        pages.extend(paginator.finish());
        pages
    }

    #[test]
    fn pages_fill_row_by_row_and_number_from_one() {
        let lines: Vec<_> = (1..=7).map(|number| line(number, "x")).collect();
        let pages = all_pages(Paginator::new(3, 10, LongLines::Wrap, 4), &lines);
        assert_eq!(
            pages.iter().map(|page| page.number).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            pages.iter().map(|page| page.rows.len()).collect::<Vec<_>>(),
            [3, 3, 1]
        );
        assert_eq!(pages[2].rows[0].gutter, Gutter::Number(7));
    }

    #[test]
    fn an_exactly_full_last_page_has_no_empty_page_after_it() {
        let lines: Vec<_> = (1..=6).map(|number| line(number, "x")).collect();
        let pages = all_pages(Paginator::new(3, 10, LongLines::Wrap, 4), &lines);
        assert_eq!(pages.len(), 2);
    }

    #[test]
    fn a_last_row_that_starts_a_new_page_keeps_the_full_page_before_it() {
        let mut paginator = Paginator::new(2, 10, LongLines::Wrap, 4);
        assert_eq!(paginator.push(&line(1, "a")).len(), 0);
        assert_eq!(paginator.push(&line(2, "b")).len(), 0);
        // Line 2 is still open; line 3 commits it, filling page 1.
        assert_eq!(paginator.push(&line(3, "c")).len(), 0);
        let rest = paginator.finish();
        assert_eq!(
            rest.iter().map(|page| page.number).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(rest[1].rows[0].gutter, Gutter::Number(3));
    }

    #[test]
    fn an_empty_document_prints_one_empty_page() {
        let pages = all_pages(Paginator::new(3, 10, LongLines::Wrap, 4), &[]);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].rows.len(), 0);
    }

    #[test]
    fn long_lines_wrap_with_a_continuation_gutter_or_are_cut() {
        let lines = [line(1, "abcdefghij"), line(2, "z")];
        let wrapped = all_pages(Paginator::new(10, 4, LongLines::Wrap, 4), &lines);
        let texts: Vec<_> = (0..4).map(|row| row_text(&wrapped[0], row)).collect();
        assert_eq!(texts, ["abcd", "efgh", "ij", "z"]);
        assert_eq!(wrapped[0].rows[1].gutter, Gutter::Continuation);
        assert_eq!(wrapped[0].rows[3].gutter, Gutter::Number(2));

        let cut = all_pages(Paginator::new(10, 4, LongLines::Cut, 4), &lines);
        assert_eq!(row_text(&cut[0], 0), format!("abc{CUT_MARKER}"));
        assert_eq!(row_text(&cut[0], 1), "z");
        // A line exactly as wide as the area is not marked.
        let exact = all_pages(Paginator::new(10, 4, LongLines::Cut, 4), &[line(1, "abcd")]);
        assert_eq!(row_text(&exact[0], 0), "abcd");
    }

    #[test]
    fn wrapping_breaks_between_words_and_splits_only_long_words() {
        let mut source = line(1, "aaaa bbbb cc dddddddddddd");
        source.spans = vec![Span {
            start: 5,
            end: 9,
            token: SyntaxToken::String,
        }];
        let pages = all_pages(Paginator::new(10, 8, LongLines::Wrap, 4), &[source]);
        let texts: Vec<_> = (0..pages[0].rows.len())
            .map(|row| row_text(&pages[0], row))
            .collect();
        // "dddddddddddd" is longer than half a row, so it is split at the edge.
        assert_eq!(texts, ["aaaa ", "bbbb cc ", "dddddddd", "dddd"]);
        // The moved word keeps its style.
        assert_eq!(
            pages[0].rows[1].runs[0].style,
            Style::Token(SyntaxToken::String)
        );
    }

    #[test]
    fn wide_characters_take_two_cells_and_wrap_whole() {
        // 5 columns: "ab" then 世 (2) fits, 界 (2) does not and wraps whole.
        let pages = all_pages(
            Paginator::new(10, 5, LongLines::Wrap, 4),
            &[line(1, "ab世界c")],
        );
        let rows: Vec<_> = pages[0]
            .rows
            .iter()
            .map(|row| {
                row.runs
                    .iter()
                    .map(|run| (run.column, run.text.clone(), run.cells))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(
            rows,
            [
                vec![(0, "ab世".to_owned(), 4)],
                vec![(0, "界c".to_owned(), 3)]
            ]
        );
        // Cut: the marker replaces what no longer fits.
        let cut = all_pages(
            Paginator::new(10, 4, LongLines::Cut, 4),
            &[line(1, "a世界")],
        );
        let runs: Vec<_> = cut[0].rows[0]
            .runs
            .iter()
            .map(|run| (run.column, run.text.as_str()))
            .collect();
        assert_eq!(runs, [(0, "a世"), (3, "…")]);
    }

    #[test]
    fn continuation_segments_carry_on_the_open_row() {
        let mut second = line(1, "cdef");
        second.continuation = true;
        let pages = all_pages(
            Paginator::new(10, 4, LongLines::Wrap, 4),
            &[line(1, "ab"), second],
        );
        assert_eq!(row_text(&pages[0], 0), "abcd");
        assert_eq!(row_text(&pages[0], 1), "ef");
        assert_eq!(pages[0].rows.len(), 2);
    }

    #[test]
    fn a_new_range_starts_after_a_separator_row() {
        let mut second = line(40, "later");
        second.separator_before = true;
        let pages = all_pages(
            Paginator::new(10, 20, LongLines::Wrap, 4),
            &[line(3, "first"), second],
        );
        let gutters: Vec<_> = pages[0].rows.iter().map(|row| row.gutter).collect();
        assert_eq!(
            gutters,
            [Gutter::Number(3), Gutter::Separator, Gutter::Number(40)]
        );
    }

    #[test]
    fn tabs_expand_to_stops_and_control_characters_show_pictures() {
        let pages = all_pages(
            Paginator::new(10, 40, LongLines::Wrap, 4),
            &[line(1, "a\tb\u{1b}\u{7f}")],
        );
        assert_eq!(row_text(&pages[0], 0), "a   b\u{241b}\u{2421}");
        assert_eq!(
            pages[0].rows[0].runs.last().expect("run").style,
            Style::Symbol
        );
    }

    #[test]
    fn spans_become_styled_runs() {
        let mut source = line(1, "let x = 1; // hi");
        source.spans = vec![
            Span {
                start: 0,
                end: 3,
                token: SyntaxToken::Keyword,
            },
            Span {
                start: 8,
                end: 9,
                token: SyntaxToken::Number,
            },
            Span {
                start: 11,
                end: 16,
                token: SyntaxToken::Comment,
            },
        ];
        let pages = all_pages(Paginator::new(10, 40, LongLines::Wrap, 4), &[source]);
        let runs: Vec<_> = pages[0].rows[0]
            .runs
            .iter()
            .map(|run| (run.column, run.text.as_str(), run.style))
            .collect();
        assert_eq!(
            runs,
            [
                (0, "let", Style::Token(SyntaxToken::Keyword)),
                (3, " x = ", Style::Plain),
                (8, "1", Style::Token(SyntaxToken::Number)),
                (9, "; ", Style::Plain),
                (11, "// hi", Style::Token(SyntaxToken::Comment)),
            ]
        );
    }
}
