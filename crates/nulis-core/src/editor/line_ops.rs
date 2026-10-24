//! Line-oriented editing: forward delete, select all, and whole-line
//! operations. Every edit is one revision-checked `replace_many`, so it is one
//! undo step and moves bookmarks and line-change markers like any other edit.
//! Only the lines being edited are read, never the whole document.

use super::{
    active_rectangle_caret, carets_after_replacements, column_after_edit, editable_ranges,
    find_line_boundary, find_line_start, merge_ranges, navigable_scalar_width, read_byte,
    record_bookmark_edit, rectangular_ranges, relocate_bookmarks, summary, validate_revision,
    DocumentId, DocumentManager, DocumentSummary, EditorError, EditorSession, Selection,
    MAX_EDIT_BYTES,
};
use nulis_document::PieceTable;

mod format;
use format::FormatKind;
pub use format::LineFormatOptions;
use serde::{Deserialize, Serialize};

/// How far into a line Home looks for its first non-blank character.
pub(super) const SMART_HOME_SCAN_BYTES: u64 = 4 * 1024;

/// A whole-line command applied to every line touched by a selection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum LineOperation {
    SelectLines,
    DeleteLines,
    DuplicateLines,
    MoveLinesUp,
    MoveLinesDown,
    DeleteToLineStart,
    DeleteToLineEnd,
    InsertLineBelow,
    /// Adds one indentation unit to each non-empty line.
    Indent,
    /// Removes one indentation unit (a tab or up to a tab width of spaces).
    Outdent,
    /// Comments out the lines, or uncomments them when all are commented.
    ToggleComment,
    TrimTrailingSpaces,
    TabsToSpaces,
    /// Turns leading spaces into tabs.
    SpacesToTabs,
    SortAscending,
    SortDescending,
    /// Sorts ascending and keeps one copy of repeated lines.
    SortUnique,
    /// Pads delimited fields (CSV and similar) so the columns line up.
    AlignColumns,
    /// Removes that padding, leaving plain delimited text.
    UnalignColumns,
}

impl LineOperation {
    fn format_kind(self) -> Option<FormatKind> {
        Some(match self {
            Self::Indent => FormatKind::Indent,
            Self::Outdent => FormatKind::Outdent,
            Self::ToggleComment => FormatKind::ToggleComment,
            Self::TrimTrailingSpaces => FormatKind::TrimTrailingSpaces,
            Self::TabsToSpaces => FormatKind::TabsToSpaces,
            Self::SpacesToTabs => FormatKind::SpacesToTabs,
            Self::SortAscending => FormatKind::Sort {
                descending: false,
                unique: false,
            },
            Self::SortDescending => FormatKind::Sort {
                descending: true,
                unique: false,
            },
            Self::SortUnique => FormatKind::Sort {
                descending: false,
                unique: true,
            },
            Self::AlignColumns => FormatKind::AlignColumns,
            Self::UnalignColumns => FormatKind::UnalignColumns,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineEditResult {
    pub document: DocumentSummary,
    pub selections: Vec<Selection>,
}

/// Consecutive whole lines: `start` of the first, the content end and the
/// start of the line after the last (`None` when it is the final line).
#[derive(Clone, Copy, Debug)]
struct LineBlock {
    start: u64,
    content_end: u64,
    next_start: Option<u64>,
}

impl LineBlock {
    fn end(&self, document_len: u64) -> u64 {
        self.next_start.unwrap_or(document_len)
    }
}

/// One planned replacement and the selections it leaves, relative to the
/// replacement's start in the edited document.
struct PlannedEdit {
    start: u64,
    end: u64,
    inserted: Vec<u8>,
    selections: Vec<(i128, i128)>,
}

impl DocumentManager {
    /// Selects the whole document.
    ///
    /// # Errors
    /// Returns an error for an unknown document.
    pub fn select_all(&self, document_id: &DocumentId) -> Result<Vec<Selection>, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        session.selections = vec![Selection {
            anchor_byte: 0,
            active_byte: session.document.table().len(),
        }];
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None];
        Ok(session.selections.clone())
    }

    /// Deletes each selection, or the character after each caret. A CRLF
    /// pair is one character; malformed bytes are deleted one at a time.
    /// In a rectangle, a caret at the end of its row deletes nothing.
    ///
    /// # Errors
    /// Returns an error for unknown documents, stale revisions, read-only
    /// documents, or storage failure.
    pub fn delete_forward(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let rectangle = session.rectangular_selection;
        let source = selection_ranges(session)?;
        let rows = source.len();
        let table = session.document.table();
        let len = table.len();
        let mut ranges = Vec::with_capacity(source.len());
        for (start, end) in source {
            if start != end {
                ranges.push((start, end));
                continue;
            }
            if start >= len {
                continue;
            }
            if rectangle.is_some()
                && start >= find_line_boundary(table, find_line_start(table, start)?)?.content_end
            {
                continue;
            }
            let bytes = table.read_vec(start, (len - start).min(4))?;
            let width = if bytes.starts_with(b"\r\n") {
                2
            } else {
                navigable_scalar_width(session.encoding, &bytes)
            };
            let width = u64::try_from(width).map_err(|_| EditorError::OffsetOverflow)?;
            ranges.push((start, start + width));
        }
        let ranges = merge_ranges(ranges);
        if ranges.is_empty() {
            return Ok(summary(document_id, session));
        }
        check_edit_size(ranges.iter().map(|(start, end)| end - start))?;
        let batch = ranges
            .iter()
            .rev()
            .map(|(start, end)| (*start, end - start, Vec::new()))
            .collect::<Vec<_>>();
        session.document.replace_many(&batch)?;
        let edits = ranges
            .iter()
            .map(|(start, end)| (*start, *end, 0))
            .collect::<Vec<_>>();
        relocate_bookmarks(&mut session.bookmarks, &edits)?;
        record_bookmark_edit(session, edits);
        let carets = carets_after_replacements(&ranges, 0)?;
        // Like Backspace, a rectangle keeps its column while every row took part.
        let kept = rectangle
            .filter(|_| carets.len() == rows)
            .and_then(|rectangle| {
                let left = rectangle.anchor_column.min(rectangle.active_column);
                column_after_edit(rectangle, &carets, left)
            });
        session.selections = active_rectangle_caret(rectangle, carets);
        session.rectangular_selection = kept;
        session.preferred_scalar_columns = vec![None; session.selections.len()];
        session.navigation_index.reset();
        Ok(summary(document_id, session))
    }

    /// Applies a whole-line command to every line touched by a selection.
    /// Adjacent or overlapping lines form one block. Moving does nothing when
    /// any block is already at the top or bottom of the document.
    ///
    /// # Errors
    /// Returns an error for unknown documents, stale revisions, read-only
    /// documents, edits larger than [`MAX_EDIT_BYTES`], or storage failure.
    pub fn line_operation(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        operation: LineOperation,
    ) -> Result<LineEditResult, EditorError> {
        self.line_operation_with(
            document_id,
            expected_revision,
            operation,
            LineFormatOptions::default(),
        )
    }

    /// [`Self::line_operation`] with tab width, indentation, and sort
    /// settings. Trimming, tab conversion, and sorting cover the whole
    /// document when nothing is selected (within [`MAX_EDIT_BYTES`]).
    ///
    /// # Errors
    /// As for [`Self::line_operation`], and for Toggle Comment in a language
    /// without comments.
    pub fn line_operation_with(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        operation: LineOperation,
        options: LineFormatOptions,
    ) -> Result<LineEditResult, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let ranges = selection_ranges(session)?;
        let table = session.document.table();
        let format = operation.format_kind();
        let whole_document = format.is_some_and(FormatKind::whole_document_without_selection)
            && ranges.iter().all(|(start, end)| start == end);
        let plan = Plan {
            table,
            len: table.len(),
            blocks: if whole_document {
                line_blocks(table, &[(0, table.len())])?
            } else {
                line_blocks(table, &ranges)?
            },
            points: selection_points(session),
        };
        let edits = match operation {
            LineOperation::SelectLines => {
                let selections = plan
                    .blocks
                    .iter()
                    .map(|block| Selection {
                        anchor_byte: block.start,
                        active_byte: block.end(plan.len),
                    })
                    .collect::<Vec<_>>();
                set_selections(session, selections.clone());
                return Ok(LineEditResult {
                    document: summary(document_id, session),
                    selections,
                });
            }
            LineOperation::DeleteLines => plan.delete_lines()?,
            LineOperation::DuplicateLines => plan.duplicate_lines()?,
            LineOperation::MoveLinesUp => plan.move_lines_up()?,
            LineOperation::MoveLinesDown => plan.move_lines_down()?,
            LineOperation::DeleteToLineStart => plan.delete_to_line_edge(&ranges, true)?,
            LineOperation::DeleteToLineEnd => plan.delete_to_line_edge(&ranges, false)?,
            LineOperation::InsertLineBelow => plan.insert_line_below()?,
            _ => {
                let kind = format.ok_or(EditorError::StateUnavailable)?;
                let comment = self.languages().comment_syntax(&session.language_id);
                plan.format_lines(kind, options, comment.as_ref())?
            }
        };
        if edits.is_empty() {
            return Ok(unchanged(document_id, session));
        }
        let selections = commit(session, &edits)?;
        Ok(LineEditResult {
            document: summary(document_id, session),
            selections,
        })
    }
}

/// What a line operation works on, read before anything is changed.
struct Plan<'a> {
    table: &'a PieceTable,
    len: u64,
    blocks: Vec<LineBlock>,
    points: Vec<(u64, u64)>,
}

fn relative(offset: u64, base: u64) -> i128 {
    i128::from(offset) - i128::from(base)
}

fn width(bytes: &[u8]) -> Result<i128, EditorError> {
    i128::try_from(bytes.len()).map_err(|_| EditorError::OffsetOverflow)
}

impl Plan<'_> {
    /// Selections that start in `block`, relative to its start, shifted by
    /// `shift` and clamped to `limit`.
    fn carried(&self, block: &LineBlock, shift: i128, limit: u64) -> Vec<(i128, i128)> {
        self.points
            .iter()
            .filter(|(anchor, active)| {
                let low = (*anchor).min(*active);
                low >= block.start && low <= block.end(self.len)
            })
            .map(|(anchor, active)| {
                (
                    relative((*anchor).min(limit), block.start) + shift,
                    relative((*active).min(limit), block.start) + shift,
                )
            })
            .collect()
    }

    fn delete_lines(&self) -> Result<Vec<PlannedEdit>, EditorError> {
        let mut edits = Vec::with_capacity(self.blocks.len());
        for block in &self.blocks {
            let start = match block.next_start {
                Some(_) => block.start,
                // The last line takes the line ending before it with it.
                None if block.start > 0 => {
                    block.start - ending_before(self.table, block.start)?.len() as u64
                }
                None => 0,
            };
            // The caret goes to the start of the line that now holds `start`.
            let caret = relative(find_line_start(self.table, start)?, start);
            edits.push(PlannedEdit {
                start,
                end: block.end(self.len),
                inserted: Vec::new(),
                selections: vec![(caret, caret)],
            });
        }
        Ok(edits)
    }

    /// Copies each block below itself; the selections move to the copy, so
    /// repeating the command continues downward.
    fn duplicate_lines(&self) -> Result<Vec<PlannedEdit>, EditorError> {
        let mut edits = Vec::with_capacity(self.blocks.len());
        for block in &self.blocks {
            let (at, inserted, shift) = if let Some(next) = block.next_start {
                (next, read(self.table, block.start, next)?, 0)
            } else {
                let mut inserted = block_ending(self.table, block)?;
                let shift = width(&inserted)?;
                inserted.extend(read(self.table, block.start, block.content_end)?);
                (self.len, inserted, shift)
            };
            edits.push(PlannedEdit {
                start: at,
                end: at,
                inserted,
                selections: self.carried(block, shift, self.len),
            });
        }
        Ok(edits)
    }

    /// Swaps each block with the line above; nothing moves if any block is
    /// already at the top.
    fn move_lines_up(&self) -> Result<Vec<PlannedEdit>, EditorError> {
        if self.blocks.iter().any(|block| block.start == 0) {
            return Ok(Vec::new());
        }
        let mut edits = Vec::with_capacity(self.blocks.len());
        for block in &self.blocks {
            let end = block.end(self.len);
            let above = find_line_start(self.table, block.start - 1)?;
            let mut inserted = read(self.table, block.start, end)?;
            if block.next_start.is_some() {
                inserted.extend(read(self.table, above, block.start)?);
            } else {
                // The block was the last line: it takes the ending above.
                let ending = ending_before(self.table, block.start)?;
                let above_end = block.start - ending.len() as u64;
                inserted.extend(&ending);
                inserted.extend(read(self.table, above, above_end)?);
            }
            edits.push(PlannedEdit {
                start: above,
                end,
                inserted,
                selections: self.carried(block, 0, self.len),
            });
        }
        Ok(edits)
    }

    /// Swaps each block with the line below; nothing moves if any block is
    /// already at the bottom.
    fn move_lines_down(&self) -> Result<Vec<PlannedEdit>, EditorError> {
        if self.blocks.iter().any(|block| block.next_start.is_none()) {
            return Ok(Vec::new());
        }
        let mut edits = Vec::with_capacity(self.blocks.len());
        for block in &self.blocks {
            let below_start = block.end(self.len);
            let below = find_line_boundary(self.table, below_start)?;
            let (end, mut inserted, block_text, limit) = if let Some(next) = below.next_start {
                (
                    next,
                    read(self.table, below_start, next)?,
                    read(self.table, block.start, below_start)?,
                    below_start,
                )
            } else {
                // The line below is the last one: it gains the block's
                // ending and the block becomes the last line.
                let mut lower = read(self.table, below_start, self.len)?;
                lower.extend(read(self.table, block.content_end, below_start)?);
                (
                    self.len,
                    lower,
                    read(self.table, block.start, block.content_end)?,
                    block.content_end,
                )
            };
            let shift = width(&inserted)?;
            inserted.extend(block_text);
            edits.push(PlannedEdit {
                start: block.start,
                end,
                inserted,
                selections: self.carried(block, shift, limit),
            });
        }
        Ok(edits)
    }

    /// Deletes each selection, or from each caret to its line start or end.
    fn delete_to_line_edge(
        &self,
        ranges: &[(u64, u64)],
        to_start: bool,
    ) -> Result<Vec<PlannedEdit>, EditorError> {
        let mut targets = Vec::with_capacity(ranges.len());
        for &(start, end) in ranges {
            if start != end {
                targets.push((start, end));
            } else if to_start {
                targets.push((find_line_start(self.table, start)?, start));
            } else {
                let line = find_line_start(self.table, start)?;
                let content_end = find_line_boundary(self.table, line)?.content_end;
                targets.push((start, content_end.max(start)));
            }
        }
        Ok(merge_ranges(targets)
            .into_iter()
            .filter(|(start, end)| start != end)
            .map(|(start, end)| PlannedEdit {
                start,
                end,
                inserted: Vec::new(),
                selections: vec![(0, 0)],
            })
            .collect())
    }

    /// Opens an empty line below each block, indented like the block's last
    /// line, without splitting the line at the caret.
    fn insert_line_below(&self) -> Result<Vec<PlannedEdit>, EditorError> {
        let mut edits = Vec::with_capacity(self.blocks.len());
        for block in &self.blocks {
            let mut inserted = block_ending(self.table, block)?;
            let last_line = find_line_start(self.table, block.content_end)?;
            inserted.extend(leading_blanks(self.table, last_line, block.content_end)?);
            let caret = width(&inserted)?;
            edits.push(PlannedEdit {
                start: block.content_end,
                end: block.content_end,
                inserted,
                selections: vec![(caret, caret)],
            });
        }
        Ok(edits)
    }
}

/// Finds the caret target for Home: the first non-blank character of the
/// line, or the line start when the caret is already there.
pub(super) fn smart_line_start(table: &PieceTable, offset: u64) -> Result<u64, EditorError> {
    let start = find_line_start(table, offset)?;
    let content_end = find_line_boundary(table, start)?.content_end;
    let indent = start + leading_blanks(table, start, content_end)?.len() as u64;
    Ok(if offset == indent { start } else { indent })
}

fn selection_ranges(session: &EditorSession) -> Result<Vec<(u64, u64)>, EditorError> {
    if session.rectangular_selection.is_some() {
        Ok(rectangular_ranges(session)?
            .into_iter()
            .map(|range| (range.start, range.end))
            .collect())
    } else {
        Ok(editable_ranges(&session.selections))
    }
}

/// Anchor and active offsets of the selections an operation carries along.
fn selection_points(session: &EditorSession) -> Vec<(u64, u64)> {
    match session.rectangular_selection {
        Some(rectangle) => vec![(rectangle.active_byte, rectangle.active_byte)],
        None => session
            .selections
            .iter()
            .map(|selection| (selection.anchor_byte, selection.active_byte))
            .collect(),
    }
}

fn line_blocks(table: &PieceTable, ranges: &[(u64, u64)]) -> Result<Vec<LineBlock>, EditorError> {
    let len = table.len();
    let mut sorted = ranges.to_vec();
    sorted.sort_unstable();
    let mut blocks: Vec<LineBlock> = Vec::with_capacity(sorted.len());
    for (start, end) in sorted {
        let first = find_line_start(table, start)?;
        // A selection that ends exactly at a line start does not include that line.
        let probe = if end > start && find_line_start(table, end)? == end {
            end - 1
        } else {
            end
        };
        let last = find_line_boundary(table, find_line_start(table, probe.max(start))?)?;
        let block = LineBlock {
            start: first,
            content_end: last.content_end,
            next_start: last.next_start,
        };
        match blocks.last_mut() {
            Some(previous) if block.start <= previous.end(len) => {
                if block.end(len) >= previous.end(len) {
                    previous.content_end = block.content_end;
                    previous.next_start = block.next_start;
                }
            }
            _ => blocks.push(block),
        }
    }
    Ok(blocks)
}

fn read(table: &PieceTable, start: u64, end: u64) -> Result<Vec<u8>, EditorError> {
    let len = end - start;
    if len > MAX_EDIT_BYTES as u64 {
        return Err(EditorError::EditTooLarge(
            usize::try_from(len).unwrap_or(usize::MAX),
        ));
    }
    Ok(table.read_vec(start, len)?)
}

/// The line ending that ends just before `line_start` (which must be > 0).
fn ending_before(table: &PieceTable, line_start: u64) -> Result<Vec<u8>, EditorError> {
    Ok(match read_byte(table, line_start - 1)? {
        b'\n' if line_start >= 2 && read_byte(table, line_start - 2)? == b'\r' => b"\r\n".to_vec(),
        b'\n' => b"\n".to_vec(),
        _ => b"\r".to_vec(),
    })
}

/// The block's own line ending, else the one before it, else the first in
/// the document, else LF.
fn block_ending(table: &PieceTable, block: &LineBlock) -> Result<Vec<u8>, EditorError> {
    if let Some(next) = block.next_start {
        return read(table, block.content_end, next);
    }
    if block.start > 0 {
        return ending_before(table, block.start);
    }
    let first = find_line_boundary(table, 0)?;
    match first.next_start {
        Some(next) => read(table, first.content_end, next),
        None => Ok(b"\n".to_vec()),
    }
}

/// Spaces and tabs at the start of a line, read from a bounded prefix.
fn leading_blanks(
    table: &PieceTable,
    start: u64,
    content_end: u64,
) -> Result<Vec<u8>, EditorError> {
    let scan = (content_end - start).min(SMART_HOME_SCAN_BYTES);
    let mut bytes = table.read_vec(start, scan)?;
    let blanks = bytes
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    bytes.truncate(blanks);
    Ok(bytes)
}

fn check_edit_size(lengths: impl Iterator<Item = u64>) -> Result<(), EditorError> {
    let total = lengths.fold(0_u64, u64::saturating_add);
    if total > MAX_EDIT_BYTES as u64 {
        return Err(EditorError::EditTooLarge(
            usize::try_from(total).unwrap_or(usize::MAX),
        ));
    }
    Ok(())
}

fn unchanged(document_id: &DocumentId, session: &EditorSession) -> LineEditResult {
    LineEditResult {
        document: summary(document_id, session),
        selections: session.selections.clone(),
    }
}

fn set_selections(session: &mut EditorSession, selections: Vec<Selection>) {
    session.preferred_scalar_columns = vec![None; selections.len()];
    session.selections = selections;
    session.rectangular_selection = None;
}

/// Applies ascending, non-overlapping edits as one undo step and returns the
/// resulting selections.
fn commit(
    session: &mut EditorSession,
    edits: &[PlannedEdit],
) -> Result<Vec<Selection>, EditorError> {
    check_edit_size(edits.iter().map(|edit| edit.end - edit.start))?;
    check_edit_size(edits.iter().map(|edit| edit.inserted.len() as u64))?;
    let batch = edits
        .iter()
        .rev()
        .map(|edit| (edit.start, edit.end - edit.start, edit.inserted.clone()))
        .collect::<Vec<_>>();
    session.document.replace_many(&batch)?;
    let bookmark_edits = edits
        .iter()
        .map(|edit| (edit.start, edit.end, edit.inserted.len()))
        .collect::<Vec<_>>();
    relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
    record_bookmark_edit(session, bookmark_edits);

    let new_len = i128::from(session.document.table().len());
    let to_offset = |value: i128| u64::try_from(value.clamp(0, new_len)).unwrap_or(0);
    let mut delta = 0_i128;
    let mut selections = Vec::new();
    for edit in edits {
        let base = i128::from(edit.start) + delta;
        for (anchor, active) in &edit.selections {
            selections.push(Selection {
                anchor_byte: to_offset(base + anchor),
                active_byte: to_offset(base + active),
            });
        }
        delta += i128::try_from(edit.inserted.len()).map_err(|_| EditorError::OffsetOverflow)?
            - i128::from(edit.end - edit.start);
    }
    if selections.is_empty() {
        selections.push(Selection::caret(0));
    }
    let selections = super::deduplicate_selections(selections);
    set_selections(session, selections.clone());
    session.navigation_index.reset();
    Ok(selections)
}

#[cfg(test)]
mod tests {
    use super::super::{DocumentId, DocumentManager, EditorError, HorizontalDirection, Selection};
    use super::LineOperation;

    fn open(text: &str) -> (DocumentManager, DocumentId) {
        let manager = DocumentManager::default();
        let id = manager
            .create_untitled("test".into(), text.into())
            .expect("document")
            .document_id;
        (manager, id)
    }

    fn text(manager: &DocumentManager, id: &DocumentId) -> String {
        manager.markdown_preview_source(id).expect("text").text
    }

    fn revision(manager: &DocumentManager, id: &DocumentId) -> u64 {
        manager.summary(id).expect("summary").revision
    }

    fn select(manager: &DocumentManager, id: &DocumentId, ranges: &[(u64, u64)]) {
        manager
            .set_selections(
                id,
                ranges
                    .iter()
                    .map(|(anchor, active)| Selection {
                        anchor_byte: *anchor,
                        active_byte: *active,
                    })
                    .collect(),
            )
            .expect("selections");
    }

    fn run(manager: &DocumentManager, id: &DocumentId, operation: LineOperation) -> Vec<Selection> {
        manager
            .line_operation(id, revision(manager, id), operation)
            .expect("line operation")
            .selections
    }

    fn carets(selections: &[Selection]) -> Vec<u64> {
        selections
            .iter()
            .map(|selection| selection.active_byte)
            .collect()
    }

    #[test]
    fn home_is_smart_and_end_stops_before_the_line_ending() {
        let (manager, id) = open("one\r\n    two\r\nlast");
        select(&manager, &id, &[(12, 12)]);
        let home = |manager: &DocumentManager| {
            carets(
                &manager
                    .move_horizontal(&id, HorizontalDirection::LineStart, false)
                    .expect("home"),
            )
        };
        assert_eq!(home(&manager), vec![9], "first non-blank");
        assert_eq!(home(&manager), vec![5], "then the line start");
        assert_eq!(home(&manager), vec![9], "and back");
        let end = manager
            .move_horizontal(&id, HorizontalDirection::LineEnd, true)
            .expect("end");
        assert_eq!(
            end,
            vec![Selection {
                anchor_byte: 9,
                active_byte: 12
            }]
        );
        select(&manager, &id, &[(16, 16)]);
        assert_eq!(
            carets(
                &manager
                    .move_horizontal(&id, HorizontalDirection::LineEnd, false)
                    .expect("end")
            ),
            vec![18],
            "last line without an ending",
        );
        assert_eq!(
            carets(
                &manager
                    .move_horizontal(&id, HorizontalDirection::DocumentStart, false)
                    .expect("start")
            ),
            vec![0],
        );
        assert_eq!(
            carets(
                &manager
                    .move_horizontal(&id, HorizontalDirection::DocumentEnd, false)
                    .expect("end")
            ),
            vec![18],
        );
    }

    #[test]
    fn delete_forward_removes_one_character_crlf_or_selection() {
        let (manager, id) = open("a\u{e9}\r\nb");
        select(&manager, &id, &[(1, 1)]);
        manager
            .delete_forward(&id, revision(&manager, &id))
            .expect("delete");
        assert_eq!(text(&manager, &id), "a\r\nb", "whole UTF-8 scalar");
        manager
            .delete_forward(&id, revision(&manager, &id))
            .expect("delete");
        assert_eq!(text(&manager, &id), "ab", "CRLF is one character");
        select(&manager, &id, &[(2, 2)]);
        let unchanged = revision(&manager, &id);
        manager.delete_forward(&id, unchanged).expect("at end");
        assert_eq!(revision(&manager, &id), unchanged, "nothing after the end");
        select(&manager, &id, &[(0, 2)]);
        manager
            .delete_forward(&id, revision(&manager, &id))
            .expect("selection");
        assert_eq!(text(&manager, &id), "");
        manager.undo(&id).expect("undo");
        assert_eq!(text(&manager, &id), "ab");
    }

    #[test]
    fn delete_forward_rejects_stale_revisions_and_read_only_documents() {
        let (manager, id) = open("abc");
        select(&manager, &id, &[(0, 0)]);
        let stale = revision(&manager, &id) + 1;
        assert!(matches!(
            manager.delete_forward(&id, stale),
            Err(EditorError::StaleRevision { .. })
        ));
        manager.toggle_read_only(&id).expect("read only");
        assert!(manager
            .delete_forward(&id, revision(&manager, &id))
            .is_err());
        assert_eq!(text(&manager, &id), "abc");
    }

    #[test]
    fn select_all_and_select_lines() {
        let (manager, id) = open("one\ntwo\nthree");
        assert_eq!(
            manager.select_all(&id).expect("all"),
            vec![Selection {
                anchor_byte: 0,
                active_byte: 13
            }],
        );
        select(&manager, &id, &[(1, 1), (5, 5)]);
        assert_eq!(
            run(&manager, &id, LineOperation::SelectLines),
            vec![Selection {
                anchor_byte: 0,
                active_byte: 8
            }],
            "adjacent lines form one block",
        );
        select(&manager, &id, &[(9, 9)]);
        assert_eq!(
            run(&manager, &id, LineOperation::SelectLines),
            vec![Selection {
                anchor_byte: 8,
                active_byte: 13
            }],
        );
    }

    #[test]
    fn delete_lines_is_one_undo_step_and_handles_the_last_line() {
        let (manager, id) = open("one\ntwo\nthree");
        select(&manager, &id, &[(5, 5)]);
        assert_eq!(
            carets(&run(&manager, &id, LineOperation::DeleteLines)),
            vec![4]
        );
        assert_eq!(text(&manager, &id), "one\nthree");
        select(&manager, &id, &[(6, 6)]);
        assert_eq!(
            carets(&run(&manager, &id, LineOperation::DeleteLines)),
            vec![0]
        );
        assert_eq!(text(&manager, &id), "one");
        manager.undo(&id).expect("undo");
        assert_eq!(text(&manager, &id), "one\nthree");
        // A selection ending at a line start leaves that line alone.
        select(&manager, &id, &[(0, 4)]);
        run(&manager, &id, LineOperation::DeleteLines);
        assert_eq!(text(&manager, &id), "three");
    }

    #[test]
    fn duplicate_lines_keeps_endings_and_follows_the_copy() {
        let (manager, id) = open("a\r\nb");
        select(&manager, &id, &[(0, 1)]);
        let selections = run(&manager, &id, LineOperation::DuplicateLines);
        assert_eq!(text(&manager, &id), "a\r\na\r\nb");
        assert_eq!(
            selections,
            vec![Selection {
                anchor_byte: 3,
                active_byte: 4
            }]
        );
        select(&manager, &id, &[(7, 7)]);
        let selections = run(&manager, &id, LineOperation::DuplicateLines);
        assert_eq!(
            text(&manager, &id),
            "a\r\na\r\nb\r\nb",
            "last line borrows the ending above"
        );
        assert_eq!(carets(&selections), vec![10]);
    }

    #[test]
    fn move_lines_up_and_down_swap_with_neighbors() {
        let (manager, id) = open("one\ntwo\nthree");
        select(&manager, &id, &[(5, 5)]);
        assert_eq!(
            carets(&run(&manager, &id, LineOperation::MoveLinesUp)),
            vec![1]
        );
        assert_eq!(text(&manager, &id), "two\none\nthree");
        let unchanged = revision(&manager, &id);
        run(&manager, &id, LineOperation::MoveLinesUp);
        assert_eq!(revision(&manager, &id), unchanged, "already at the top");
        assert_eq!(
            carets(&run(&manager, &id, LineOperation::MoveLinesDown)),
            vec![5]
        );
        assert_eq!(
            carets(&run(&manager, &id, LineOperation::MoveLinesDown)),
            vec![11]
        );
        assert_eq!(
            text(&manager, &id),
            "one\nthree\ntwo",
            "the old last line gains an ending"
        );
        assert_eq!(
            carets(&run(&manager, &id, LineOperation::MoveLinesUp)),
            vec![5]
        );
        assert_eq!(text(&manager, &id), "one\ntwo\nthree");
    }

    #[test]
    fn several_carets_move_their_lines_together() {
        let (manager, id) = open("a\nb\nc\nd\n");
        select(&manager, &id, &[(2, 2), (6, 6)]);
        assert_eq!(
            carets(&run(&manager, &id, LineOperation::MoveLinesUp)),
            vec![0, 4]
        );
        assert_eq!(text(&manager, &id), "b\na\nd\nc\n");
        manager.undo(&id).expect("undo");
        assert_eq!(text(&manager, &id), "a\nb\nc\nd\n", "one undo step");
    }

    #[test]
    fn delete_to_line_edges_and_insert_line_below() {
        let (manager, id) = open("  abc def\nnext");
        select(&manager, &id, &[(5, 5)]);
        run(&manager, &id, LineOperation::DeleteToLineEnd);
        assert_eq!(text(&manager, &id), "  abc\nnext");
        select(&manager, &id, &[(4, 4)]);
        assert_eq!(
            carets(&run(&manager, &id, LineOperation::DeleteToLineStart)),
            vec![0]
        );
        assert_eq!(text(&manager, &id), "c\nnext");
        let (manager, id) = open("    indented\r\nx");
        select(&manager, &id, &[(6, 6)]);
        assert_eq!(
            carets(&run(&manager, &id, LineOperation::InsertLineBelow)),
            vec![18]
        );
        assert_eq!(text(&manager, &id), "    indented\r\n    \r\nx");
    }

    #[test]
    fn line_operations_refuse_read_only_documents() {
        let (manager, id) = open("one\ntwo");
        manager.toggle_read_only(&id).expect("read only");
        assert!(manager
            .line_operation(&id, revision(&manager, &id), LineOperation::DeleteLines)
            .is_err());
        assert_eq!(text(&manager, &id), "one\ntwo");
    }
}
