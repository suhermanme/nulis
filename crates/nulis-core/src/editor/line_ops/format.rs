//! Line formatting: indent, outdent, toggle comment, trim trailing spaces,
//! tabs and spaces, and sorting. Each works on whole lines of a block and is
//! one edit per block, so the command is one undo step.

use super::{read, LineBlock, Plan, PlannedEdit};
use crate::editor::EditorError;
use nulis_syntax::CommentSyntax;
use serde::Deserialize;

/// Settings for the formatting operations.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct LineFormatOptions {
    /// Columns per tab stop, 1 to 16.
    pub tab_width: u8,
    /// Indent with spaces instead of a tab.
    pub indent_with_spaces: bool,
    /// Sort without regard to letter case.
    pub ignore_case: bool,
}

impl Default for LineFormatOptions {
    fn default() -> Self {
        Self {
            tab_width: 4,
            indent_with_spaces: false,
            ignore_case: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FormatKind {
    Indent,
    Outdent,
    ToggleComment,
    TrimTrailingSpaces,
    TabsToSpaces,
    SpacesToTabs,
    Sort { descending: bool, unique: bool },
}

impl FormatKind {
    /// Operations that apply to the whole document when nothing is selected.
    pub(super) fn whole_document_without_selection(self) -> bool {
        !matches!(self, Self::Indent | Self::Outdent | Self::ToggleComment)
    }
}

struct Line {
    content: Vec<u8>,
    ending: Vec<u8>,
}

/// How a changed line maps old columns to new ones.
struct LineChange {
    old_start: usize,
    old_len: usize,
    new_start: usize,
    new_len: usize,
    /// Bytes added (or removed, if negative) at `at`; columns past it shift.
    shift: i128,
    at: usize,
}

fn split_lines(bytes: &[u8]) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        let ending = match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => 2,
            b'\r' | b'\n' => 1,
            _ => {
                index += 1;
                continue;
            }
        };
        lines.push(Line {
            content: bytes[start..index].to_vec(),
            ending: bytes[index..index + ending].to_vec(),
        });
        index += ending;
        start = index;
    }
    if start < bytes.len() || lines.is_empty() {
        lines.push(Line {
            content: bytes[start..].to_vec(),
            ending: Vec::new(),
        });
    }
    lines
}

fn leading_whitespace(content: &[u8]) -> usize {
    content
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count()
}

fn is_blank(content: &[u8]) -> bool {
    leading_whitespace(content) == content.len()
}

/// Display columns of `bytes`, with tabs to the next stop and UTF-8
/// continuation bytes not counted.
fn columns(bytes: &[u8], start_column: usize, tab_width: usize) -> usize {
    bytes.iter().fold(start_column, |column, byte| match byte {
        b'\t' => (column / tab_width + 1) * tab_width,
        byte if byte & 0xC0 == 0x80 => column,
        _ => column + 1,
    })
}

fn expand_tabs(content: &[u8], tab_width: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(content.len());
    let mut column = 0;
    for &byte in content {
        if byte == b'\t' {
            let next = (column / tab_width + 1) * tab_width;
            output.resize(output.len() + (next - column), b' ');
            column = next;
        } else {
            output.push(byte);
            if byte & 0xC0 != 0x80 {
                column += 1;
            }
        }
    }
    output
}

fn compress_leading(content: &[u8], tab_width: usize) -> Vec<u8> {
    let lead = leading_whitespace(content);
    let width = columns(&content[..lead], 0, tab_width);
    let mut output = vec![b'\t'; width / tab_width];
    output.resize(output.len() + width % tab_width, b' ');
    output.extend_from_slice(&content[lead..]);
    output
}

fn toggle_comments(lines: &mut [Line], syntax: CommentSyntax) -> Vec<(i128, usize)> {
    let mut changes = vec![(0, 0); lines.len()];
    let filled = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !is_blank(&line.content))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if filled.is_empty() {
        return changes;
    }
    if let Some(marker) = syntax.line {
        let marker = marker.as_bytes();
        let commented = filled.iter().all(|&index| {
            let content = &lines[index].content;
            content[leading_whitespace(content)..].starts_with(marker)
        });
        // New markers line up at the shallowest indentation in the block.
        let column = filled
            .iter()
            .map(|&index| leading_whitespace(&lines[index].content))
            .min()
            .unwrap_or(0);
        for &index in &filled {
            let content = &mut lines[index].content;
            if commented {
                let at = leading_whitespace(content);
                let mut length = marker.len();
                if content.get(at + length) == Some(&b' ') {
                    length += 1;
                }
                content.drain(at..at + length);
                changes[index] = (-(length as i128), at);
            } else {
                let mut inserted = marker.to_vec();
                inserted.push(b' ');
                content.splice(column..column, inserted.iter().copied());
                changes[index] = (inserted.len() as i128, column);
            }
        }
    } else if let Some((open, close)) = syntax.block {
        let (open, close) = (open.as_bytes(), close.as_bytes());
        let commented = filled.iter().all(|&index| {
            let content = &lines[index].content;
            let body = content[leading_whitespace(content)..].trim_ascii_end();
            body.starts_with(open)
                && body.ends_with(close)
                && body.len() >= open.len() + close.len()
        });
        for &index in &filled {
            let content = &mut lines[index].content;
            let at = leading_whitespace(content);
            if commented {
                let end = at + content[at..].trim_ascii_end().len();
                let mut inner = content[at + open.len()..end - close.len()].to_vec();
                let removed_front = open.len() + usize::from(inner.first() == Some(&b' '));
                if inner.first() == Some(&b' ') {
                    inner.remove(0);
                }
                if inner.last() == Some(&b' ') {
                    inner.pop();
                }
                let tail = content[end..].to_vec();
                content.truncate(at);
                content.extend(inner);
                content.extend(tail);
                changes[index] = (-(removed_front as i128), at);
            } else {
                let mut wrapped = open.to_vec();
                wrapped.push(b' ');
                wrapped.extend_from_slice(&content[at..]);
                wrapped.push(b' ');
                wrapped.extend_from_slice(close);
                content.truncate(at);
                content.extend(wrapped);
                changes[index] = ((open.len() + 1) as i128, at);
            }
        }
    }
    changes
}

impl Plan<'_> {
    pub(super) fn format_lines(
        &self,
        kind: FormatKind,
        options: LineFormatOptions,
        comment: Option<CommentSyntax>,
    ) -> Result<Vec<PlannedEdit>, EditorError> {
        let tab_width = usize::from(options.tab_width.clamp(1, 16));
        let unit: Vec<u8> = if options.indent_with_spaces {
            vec![b' '; tab_width]
        } else {
            vec![b'\t']
        };
        let mut edits = Vec::with_capacity(self.blocks.len());
        for block in &self.blocks {
            let end = block.end(self.len);
            let original = read(self.table, block.start, end)?;
            let mut lines = split_lines(&original);
            let old_layout = layout(&lines);
            let (changes, reordered) =
                reformat(&mut lines, kind, &unit, tab_width, options, comment)?;
            let mut replacement = Vec::with_capacity(original.len());
            for line in &lines {
                replacement.extend_from_slice(&line.content);
                replacement.extend_from_slice(&line.ending);
            }
            let new_layout = layout(&lines);
            let selections = if reordered {
                // The lines moved: select the sorted block.
                vec![(0, replacement.len() as i128)]
            } else {
                let line_changes = old_layout
                    .iter()
                    .zip(&new_layout)
                    .zip(&changes)
                    .map(|((old, new), (shift, at))| LineChange {
                        old_start: old.0,
                        old_len: old.1,
                        new_start: new.0,
                        new_len: new.1,
                        shift: *shift,
                        at: *at,
                    })
                    .collect::<Vec<_>>();
                self.points_in(block)
                    .map(|(anchor, active)| {
                        (
                            map_offset(&line_changes, anchor - block.start),
                            map_offset(&line_changes, active - block.start),
                        )
                    })
                    .collect()
            };
            if replacement == original {
                // Unchanged: a no-op edit that only keeps this block's carets.
                edits.push(PlannedEdit {
                    start: block.start,
                    end: block.start,
                    inserted: Vec::new(),
                    selections: if reordered {
                        self.points_in(block)
                            .map(|(anchor, active)| {
                                (
                                    i128::from(anchor - block.start),
                                    i128::from(active - block.start),
                                )
                            })
                            .collect()
                    } else {
                        selections
                    },
                });
                continue;
            }
            edits.push(PlannedEdit {
                start: block.start,
                end,
                inserted: replacement,
                selections,
            });
        }
        if edits
            .iter()
            .all(|edit| edit.start == edit.end && edit.inserted.is_empty())
        {
            return Ok(Vec::new());
        }
        Ok(edits)
    }

    fn points_in<'b>(&'b self, block: &'b LineBlock) -> impl Iterator<Item = (u64, u64)> + 'b {
        self.points.iter().copied().filter(move |(anchor, active)| {
            let low = (*anchor).min(*active);
            low >= block.start && low <= block.end(self.len)
        })
    }
}

/// Changes the lines for one operation and reports, per line, where bytes
/// were added or removed, and whether lines were reordered.
fn reformat(
    lines: &mut Vec<Line>,
    kind: FormatKind,
    unit: &[u8],
    tab_width: usize,
    options: LineFormatOptions,
    comment: Option<CommentSyntax>,
) -> Result<(Vec<(i128, usize)>, bool), EditorError> {
    // (shift, at) per line: where the line's bytes moved.
    let mut changes = vec![(0_i128, 0_usize); lines.len()];
    let mut reordered = false;
    match kind {
        FormatKind::Indent => {
            for (line, change) in lines.iter_mut().zip(&mut changes) {
                if !line.content.is_empty() {
                    line.content.splice(0..0, unit.iter().copied());
                    *change = (unit.len() as i128, 0);
                }
            }
        }
        FormatKind::Outdent => {
            for (line, change) in lines.iter_mut().zip(&mut changes) {
                let removed = match line.content.first() {
                    Some(b'\t') => 1,
                    Some(b' ') => line
                        .content
                        .iter()
                        .take(tab_width)
                        .take_while(|byte| **byte == b' ')
                        .count(),
                    _ => 0,
                };
                line.content.drain(..removed);
                *change = (-(removed as i128), 0);
            }
        }
        FormatKind::ToggleComment => {
            let syntax = comment.ok_or(EditorError::NoCommentSyntax)?;
            changes = toggle_comments(lines, syntax);
        }
        FormatKind::TrimTrailingSpaces => {
            for line in lines.iter_mut() {
                let kept = line.content.len()
                    - line
                        .content
                        .iter()
                        .rev()
                        .take_while(|byte| matches!(byte, b' ' | b'\t'))
                        .count();
                line.content.truncate(kept);
            }
        }
        FormatKind::TabsToSpaces => {
            for line in lines.iter_mut() {
                line.content = expand_tabs(&line.content, tab_width);
            }
        }
        FormatKind::SpacesToTabs => {
            for line in lines.iter_mut() {
                line.content = compress_leading(&line.content, tab_width);
            }
        }
        FormatKind::Sort { descending, unique } => {
            reordered = true;
            *lines = sort_lines(
                std::mem::take(lines),
                descending,
                unique,
                options.ignore_case,
            );
        }
    }
    Ok((changes, reordered))
}

/// (start, content length) of each line within its block.
fn layout(lines: &[Line]) -> Vec<(usize, usize)> {
    let mut offset = 0;
    lines
        .iter()
        .map(|line| {
            let entry = (offset, line.content.len());
            offset += line.content.len() + line.ending.len();
            entry
        })
        .collect()
}

fn map_offset(lines: &[LineChange], offset: u64) -> i128 {
    let offset = usize::try_from(offset).unwrap_or(usize::MAX);
    let line = lines
        .iter()
        .rev()
        .find(|line| line.old_start <= offset)
        .or_else(|| lines.first());
    let Some(line) = line else { return 0 };
    let column = (offset - line.old_start).min(line.old_len);
    let moved = if column >= line.at {
        (column as i128 + line.shift).max(line.at as i128)
    } else {
        column as i128
    };
    line.new_start as i128 + moved.clamp(0, line.new_len as i128)
}

fn sort_lines(
    mut lines: Vec<Line>,
    descending: bool,
    unique: bool,
    ignore_case: bool,
) -> Vec<Line> {
    let last_ending = lines
        .last()
        .map(|line| line.ending.clone())
        .unwrap_or_default();
    let inner_ending = lines
        .iter()
        .find(|line| !line.ending.is_empty())
        .map_or_else(|| b"\n".to_vec(), |line| line.ending.clone());
    let key = |line: &Line| {
        let text = String::from_utf8_lossy(&line.content).into_owned();
        if ignore_case {
            text.to_lowercase()
        } else {
            text
        }
    };
    lines.sort_by_cached_key(key);
    if descending {
        lines.reverse();
    }
    if unique {
        lines.dedup_by(|later, earlier| key(later) == key(earlier));
    }
    let count = lines.len();
    for (index, line) in lines.iter_mut().enumerate() {
        line.ending = if index + 1 == count {
            last_ending.clone()
        } else {
            inner_ending.clone()
        };
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::super::super::{DocumentId, DocumentManager, Selection, TextTransform};
    use super::super::LineOperation;
    use super::LineFormatOptions;
    use std::fs;

    fn open(text: &str) -> (DocumentManager, DocumentId) {
        let manager = DocumentManager::default();
        let id = manager
            .create_untitled("t".into(), text.into())
            .expect("document")
            .document_id;
        (manager, id)
    }

    fn text(manager: &DocumentManager, id: &DocumentId) -> String {
        manager.markdown_preview_source(id).expect("text").text
    }

    fn select(manager: &DocumentManager, id: &DocumentId, anchor: u64, active: u64) {
        manager
            .set_selections(
                id,
                vec![Selection {
                    anchor_byte: anchor,
                    active_byte: active,
                }],
            )
            .expect("selection");
    }

    fn apply(
        manager: &DocumentManager,
        id: &DocumentId,
        operation: LineOperation,
        options: LineFormatOptions,
    ) -> Vec<Selection> {
        let revision = manager.summary(id).expect("summary").revision;
        manager
            .line_operation_with(id, revision, operation, options)
            .expect("format")
            .selections
    }

    fn run(manager: &DocumentManager, id: &DocumentId, operation: LineOperation) -> Vec<Selection> {
        apply(manager, id, operation, LineFormatOptions::default())
    }

    #[test]
    fn indent_and_outdent_selected_lines_keep_the_caret_with_its_text() {
        let (manager, id) = open("a\n\nb\r\nc");
        select(&manager, &id, 0, 6);
        run(&manager, &id, LineOperation::Indent);
        assert_eq!(
            text(&manager, &id),
            "\ta\n\n\tb\r\nc",
            "empty lines stay empty; c is not selected"
        );
        let spaces = LineFormatOptions {
            indent_with_spaces: true,
            tab_width: 2,
            ..LineFormatOptions::default()
        };
        select(&manager, &id, 0, 0);
        apply(&manager, &id, LineOperation::Indent, spaces);
        assert_eq!(text(&manager, &id), "  \ta\n\n\tb\r\nc");
        select(&manager, &id, 3, 3);
        let carets = run(&manager, &id, LineOperation::Outdent);
        assert_eq!(
            text(&manager, &id),
            "\ta\n\n\tb\r\nc",
            "up to a tab width of spaces"
        );
        assert_eq!(
            carets,
            vec![Selection::caret(1)],
            "the caret stays before a"
        );
        run(&manager, &id, LineOperation::Outdent);
        assert_eq!(text(&manager, &id), "a\n\n\tb\r\nc");
    }

    #[test]
    fn toggle_comment_uses_the_language_and_aligns_markers() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("main.rs");
        fs::write(&path, "fn a() {\n    one();\n\n    two();\n}").expect("file");
        let manager = DocumentManager::default();
        let id = manager.open_file(&path).expect("open").document_id;
        select(&manager, &id, 9, 30);
        run(&manager, &id, LineOperation::ToggleComment);
        assert_eq!(
            text(&manager, &id),
            "fn a() {\n    // one();\n\n    // two();\n}"
        );
        select(&manager, &id, 9, 34);
        run(&manager, &id, LineOperation::ToggleComment);
        assert_eq!(
            text(&manager, &id),
            "fn a() {\n    one();\n\n    two();\n}",
            "uncomments when all are commented"
        );

        let html = root.path().join("page.html");
        fs::write(&html, "<p>x</p>").expect("html");
        let page = manager.open_file(&html).expect("open html").document_id;
        run(&manager, &page, LineOperation::ToggleComment);
        assert_eq!(text(&manager, &page), "<!-- <p>x</p> -->");
        run(&manager, &page, LineOperation::ToggleComment);
        assert_eq!(text(&manager, &page), "<p>x</p>");

        let (plain, plain_id) = open("text");
        let revision = plain.summary(&plain_id).expect("summary").revision;
        assert!(plain
            .line_operation(&plain_id, revision, LineOperation::ToggleComment)
            .is_err());
    }

    #[test]
    fn whitespace_cleanups_cover_the_whole_document_without_a_selection() {
        let (manager, id) = open("a  \t\n\tb\t\n        c");
        run(&manager, &id, LineOperation::TrimTrailingSpaces);
        assert_eq!(text(&manager, &id), "a\n\tb\n        c");
        run(&manager, &id, LineOperation::SpacesToTabs);
        assert_eq!(text(&manager, &id), "a\n\tb\n\t\tc");
        apply(
            &manager,
            &id,
            LineOperation::TabsToSpaces,
            LineFormatOptions {
                tab_width: 2,
                ..LineFormatOptions::default()
            },
        );
        assert_eq!(text(&manager, &id), "a\n  b\n    c");
        manager.undo(&id).expect("undo");
        assert_eq!(
            text(&manager, &id),
            "a\n\tb\n\t\tc",
            "each cleanup is one undo step"
        );
    }

    #[test]
    fn sorting_keeps_line_endings_and_can_drop_repeats() {
        let (manager, id) = open("pear\napple\nPear\napple");
        run(&manager, &id, LineOperation::SortAscending);
        assert_eq!(
            text(&manager, &id),
            "Pear\napple\napple\npear",
            "no final line ending is added"
        );
        run(&manager, &id, LineOperation::SortDescending);
        assert_eq!(text(&manager, &id), "pear\napple\napple\nPear");
        apply(
            &manager,
            &id,
            LineOperation::SortUnique,
            LineFormatOptions {
                ignore_case: true,
                ..LineFormatOptions::default()
            },
        );
        assert_eq!(text(&manager, &id), "apple\npear");
        let (selected, selected_id) = open("c\nb\na\nz\n");
        select(&selected, &selected_id, 0, 6);
        run(&selected, &selected_id, LineOperation::SortAscending);
        assert_eq!(
            text(&selected, &selected_id),
            "a\nb\nc\nz\n",
            "only the selected lines"
        );
    }

    #[test]
    fn case_and_base64_transforms() {
        let (manager, id) = open("hello WORLD it's");
        select(&manager, &id, 0, 16);
        let transform = |transform| {
            let revision = manager.summary(&id).expect("summary").revision;
            manager.transform_selections(&id, revision, transform)
        };
        transform(TextTransform::Capitalize).expect("capitalize");
        assert_eq!(text(&manager, &id), "Hello World It's");
        select(&manager, &id, 0, 16);
        transform(TextTransform::InvertCase).expect("invert");
        assert_eq!(text(&manager, &id), "hELLO wORLD iT'S");
        select(&manager, &id, 0, 16);
        transform(TextTransform::Base64Encode).expect("encode");
        assert_eq!(text(&manager, &id), "aEVMTE8gd09STEQgaVQnUw==");
        select(&manager, &id, 0, 24);
        transform(TextTransform::Base64Decode).expect("decode");
        assert_eq!(text(&manager, &id), "hELLO wORLD iT'S");
        select(&manager, &id, 0, 5);
        assert!(
            transform(TextTransform::Base64Decode).is_err(),
            "not Base64"
        );
    }
}
