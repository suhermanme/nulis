//! Selections and the caret: setting them, a rectangular selection, the word
//! at a point, and moving by character, word, line, and page.

use super::{
    decode_text, deduplicate_selections, find_line_boundary, find_line_start,
    line_number_at_offset, line_ops, read_byte, validate_revision, DocumentId, DocumentManager,
    EditorError, HorizontalDirection, RectangularSelection, Selection, TextEncoding,
    VerticalDirection, MAX_RECTANGULAR_ROWS, MAX_SELECTIONS, MAX_WORD_SCAN_BYTES,
    NAVIGATION_CHUNK_BYTES,
};
use nulis_document::PieceTable;
use unicode_segmentation::UnicodeSegmentation;

impl DocumentManager {
    /// Selects a Unicode word, whitespace run, or punctuation run at a byte position.
    /// The scan is bounded independently of the source file size.
    ///
    /// # Errors
    /// Returns an error for an invalid offset, an overlong token, or a source read failure.
    pub fn select_word_at(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        byte_offset: u64,
    ) -> Result<Selection, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let table = session.document.table();
        let len = table.len();
        if byte_offset > len {
            return Err(EditorError::OffsetOutOfRange {
                offset: byte_offset,
                document_len: len,
            });
        }
        let selection = word_selection(table, session.encoding, byte_offset)?;
        session.selections = vec![selection];
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None];
        Ok(selection)
    }

    /// Replaces all selection state after validating every 64-bit offset.
    ///
    /// # Errors
    /// Returns an error for unknown documents, no selections, or out-of-range offsets.
    pub fn set_selections(
        &self,
        document_id: &DocumentId,
        selections: Vec<Selection>,
    ) -> Result<Vec<Selection>, EditorError> {
        if selections.is_empty() {
            return Err(EditorError::EmptySelections);
        }
        if selections.len() > MAX_SELECTIONS {
            return Err(EditorError::TooManySelections {
                count: selections.len(),
                maximum: MAX_SELECTIONS,
            });
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let len = session.document.table().len();
        if let Some(offset) = selections
            .iter()
            .flat_map(|selection| [selection.anchor_byte, selection.active_byte])
            .find(|offset| *offset > len)
        {
            return Err(EditorError::OffsetOutOfRange {
                offset,
                document_len: len,
            });
        }
        session.selections = deduplicate_selections(selections);
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None; session.selections.len()];
        Ok(session.selections.clone())
    }

    /// Replaces normal selection state with one constant-size rectangular
    /// descriptor. Line-local byte ranges remain derived and bounded.
    ///
    /// # Errors
    /// Returns an error for an unknown document, invalid offsets, or a
    /// rectangle taller than [`MAX_RECTANGULAR_ROWS`].
    pub fn set_rectangular_selection(
        &self,
        document_id: &DocumentId,
        anchor_byte: u64,
        active_byte: u64,
        requested_anchor_column: u64,
        requested_active_column: u64,
    ) -> Result<RectangularSelection, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let document_len = session.document.table().len();
        if let Some(offset) = [anchor_byte, active_byte]
            .into_iter()
            .find(|offset| *offset > document_len)
        {
            return Err(EditorError::OffsetOutOfRange {
                offset,
                document_len,
            });
        }
        let table = session.document.table();
        let anchor_start = find_line_start(table, anchor_byte)?;
        let active_start = find_line_start(table, active_byte)?;
        let anchor_boundary = find_line_boundary(table, anchor_start)?;
        let active_boundary = find_line_boundary(table, active_start)?;
        let anchor_line = line_number_at_offset(table, &session.navigation_index, anchor_start)?;
        let active_line = line_number_at_offset(table, &session.navigation_index, active_start)?;
        let row_count = anchor_line.abs_diff(active_line).saturating_add(1);
        if row_count > MAX_RECTANGULAR_ROWS as u64 {
            return Err(EditorError::RectangularSelectionTooTall {
                rows: row_count,
                maximum: MAX_RECTANGULAR_ROWS,
            });
        }
        let anchor_column = scalar_count(
            table,
            anchor_start,
            anchor_byte.min(anchor_boundary.content_end),
            session.encoding,
        )?;
        let active_column = scalar_count(
            table,
            active_start,
            active_byte.min(active_boundary.content_end),
            session.encoding,
        )?;
        let rectangle = RectangularSelection {
            anchor_byte,
            active_byte,
            anchor_line,
            active_line,
            anchor_column: if anchor_byte >= anchor_boundary.content_end {
                anchor_column.max(requested_anchor_column)
            } else {
                anchor_column
            },
            active_column: if active_byte >= active_boundary.content_end {
                active_column.max(requested_active_column)
            } else {
                active_column
            },
        };
        session.selections = vec![Selection::caret(active_byte)];
        session.rectangular_selection = Some(rectangle);
        session.preferred_scalar_columns = vec![None];
        Ok(rectangle)
    }

    /// Moves every caret by one decoded UTF-8 scalar while preserving
    /// malformed bytes as individually navigable units, or to the start or
    /// end of its line or of the document.
    ///
    /// # Errors
    /// Returns an error for an unknown document or backing-store read failure.
    pub fn move_horizontal(
        &self,
        document_id: &DocumentId,
        direction: HorizontalDirection,
        extend: bool,
    ) -> Result<Vec<Selection>, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let mut moved = Vec::with_capacity(session.selections.len());
        for current in &session.selections {
            let active = match direction {
                HorizontalDirection::Left if current.active_byte > 0 => {
                    let probe_start = current.active_byte.saturating_sub(4);
                    let bytes = session
                        .document
                        .table()
                        .read_vec(probe_start, current.active_byte - probe_start)?;
                    let relative = match session.encoding {
                        TextEncoding::Utf8 => {
                            previous_scalar_start(&bytes).unwrap_or(bytes.len() - 1)
                        }
                        TextEncoding::Windows1252 => bytes.len() - 1,
                    };
                    probe_start
                        .checked_add(
                            u64::try_from(relative).map_err(|_| EditorError::OffsetOverflow)?,
                        )
                        .ok_or(EditorError::OffsetOverflow)?
                }
                HorizontalDirection::Left | HorizontalDirection::DocumentStart => 0,
                HorizontalDirection::Right
                    if current.active_byte < session.document.table().len() =>
                {
                    let available = (session.document.table().len() - current.active_byte).min(4);
                    let bytes = session
                        .document
                        .table()
                        .read_vec(current.active_byte, available)?;
                    let width = navigable_scalar_width(session.encoding, &bytes);
                    current
                        .active_byte
                        .checked_add(u64::try_from(width).map_err(|_| EditorError::OffsetOverflow)?)
                        .ok_or(EditorError::OffsetOverflow)?
                }
                HorizontalDirection::Right | HorizontalDirection::DocumentEnd => {
                    session.document.table().len()
                }
                HorizontalDirection::LineStart => {
                    line_ops::smart_line_start(session.document.table(), current.active_byte)?
                }
                HorizontalDirection::LineEnd => {
                    let table = session.document.table();
                    find_line_boundary(table, find_line_start(table, current.active_byte)?)?
                        .content_end
                }
            };
            moved.push(if extend {
                Selection {
                    anchor_byte: current.anchor_byte,
                    active_byte: active,
                }
            } else {
                Selection::caret(active)
            });
        }
        session.selections = deduplicate_selections(moved);
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None; session.selections.len()];
        Ok(session.selections.clone())
    }

    /// Moves every caret by one logical line while retaining its desired
    /// UTF-8 scalar column across shorter lines. Line discovery and scalar
    /// counting use fixed-size reads, including for multi-gigabyte documents.
    ///
    /// # Errors
    /// Returns an error for an unknown document or backing-store read failure.
    pub fn move_vertical(
        &self,
        document_id: &DocumentId,
        direction: VerticalDirection,
        extend: bool,
    ) -> Result<Vec<Selection>, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        let mut moved = Vec::with_capacity(session.selections.len());
        let mut preferred = Vec::with_capacity(session.selections.len());
        for (index, current) in session.selections.iter().copied().enumerate() {
            let current_start = find_line_start(table, current.active_byte)?;
            let current_line = find_line_boundary(table, current_start)?;
            let current_text_offset = current.active_byte.min(current_line.content_end);
            let preferred_column = session
                .preferred_scalar_columns
                .get(index)
                .copied()
                .flatten()
                .map_or_else(
                    || scalar_count(table, current_start, current_text_offset, session.encoding),
                    Ok,
                )?;
            let target_range = match direction {
                VerticalDirection::Up => previous_line_range(table, current_start)?,
                VerticalDirection::Down => match current_line.next_start {
                    Some(start) => {
                        let boundary = find_line_boundary(table, start)?;
                        Some((start, boundary.content_end))
                    }
                    None => None,
                },
            };
            let active = match target_range {
                Some((start, end)) => {
                    byte_at_scalar_column(table, start, end, preferred_column, session.encoding)?
                }
                None => current.active_byte,
            };
            preferred.push(Some(preferred_column));
            moved.push(if extend {
                Selection {
                    anchor_byte: current.anchor_byte,
                    active_byte: active,
                }
            } else {
                Selection::caret(active)
            });
        }
        session.selections = moved;
        session.rectangular_selection = None;
        session.preferred_scalar_columns = preferred;
        Ok(session.selections.clone())
    }
}

pub(super) fn previous_scalar_start(bytes: &[u8]) -> Option<usize> {
    (0..bytes.len()).find(|start| {
        std::str::from_utf8(&bytes[*start..]).is_ok_and(|suffix| suffix.chars().count() == 1)
    })
}

fn next_scalar_len(bytes: &[u8]) -> Option<usize> {
    (1..=bytes.len()).find(|end| {
        std::str::from_utf8(&bytes[..*end]).is_ok_and(|prefix| prefix.chars().count() == 1)
    })
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum WordKind {
    Word,
    Space,
    Punctuation,
    LineBreak,
}

fn word_kind(grapheme: &str) -> WordKind {
    let first = grapheme.chars().next().unwrap_or(' ');
    if grapheme_has_line_break(grapheme) {
        WordKind::LineBreak
    } else if first.is_alphanumeric() || first == '_' {
        WordKind::Word
    } else if first.is_whitespace() {
        WordKind::Space
    } else {
        WordKind::Punctuation
    }
}

fn grapheme_has_line_break(grapheme: &str) -> bool {
    grapheme.contains('\r') || grapheme.contains('\n')
}

fn word_selection(
    table: &PieceTable,
    encoding: TextEncoding,
    byte_offset: u64,
) -> Result<Selection, EditorError> {
    let len = table.len();
    if len == 0 {
        return Ok(Selection::caret(0));
    }
    let half = MAX_WORD_SCAN_BYTES / 2;
    let start = byte_offset.saturating_sub(half);
    let end = byte_offset.saturating_add(half).min(len);
    let bytes = table.read_vec(start, end - start)?;
    let (text, widths) = decode_text(encoding, &bytes);
    let mut source_boundaries = Vec::with_capacity(widths.len() + 1);
    let mut source_offset = 0_u64;
    for ((utf8_index, _), width) in text.char_indices().zip(widths) {
        source_boundaries.push((utf8_index, source_offset));
        source_offset += u64::from(width);
    }
    source_boundaries.push((text.len(), source_offset));
    let mut runs = Vec::new();
    for (utf8_index, grapheme) in text.grapheme_indices(true) {
        let next_index = utf8_index + grapheme.len();
        let start_index = source_boundaries.partition_point(|(position, _)| *position < utf8_index);
        let end_index = source_boundaries.partition_point(|(position, _)| *position < next_index);
        let start_byte = start + source_boundaries[start_index].1;
        let end_byte = start + source_boundaries[end_index].1;
        runs.push((start_byte, end_byte, word_kind(grapheme)));
    }
    let mut index = runs.partition_point(|(_, end_byte, _)| *end_byte <= byte_offset);
    if index == runs.len() {
        index = index.saturating_sub(1);
    }
    if runs[index].2 == WordKind::LineBreak {
        if index > 0 && runs[index - 1].2 != WordKind::LineBreak {
            index -= 1;
        } else if index + 1 < runs.len() {
            index += 1;
        } else {
            return Ok(Selection::caret(byte_offset));
        }
    }
    let kind = runs[index].2;
    let mut first = index;
    let mut last = index;
    while first > 0 && runs[first - 1].2 == kind {
        first -= 1;
    }
    while last + 1 < runs.len() && runs[last + 1].2 == kind {
        last += 1;
    }
    if (first == 0 && start > 0) || (last + 1 == runs.len() && end < len) {
        return Err(EditorError::WordSelectionTooLong);
    }
    Ok(Selection {
        anchor_byte: runs[first].0,
        active_byte: runs[last].1,
    })
}

fn previous_line_range(
    table: &PieceTable,
    current_start: u64,
) -> Result<Option<(u64, u64)>, EditorError> {
    if current_start == 0 {
        return Ok(None);
    }
    let last_ending_byte = read_byte(table, current_start - 1)?;
    let ending_len = if last_ending_byte == b'\n'
        && current_start >= 2
        && read_byte(table, current_start - 2)? == b'\r'
    {
        2
    } else {
        1
    };
    let content_end = current_start - ending_len;
    Ok(Some((find_line_start(table, content_end)?, content_end)))
}

pub(super) fn scalar_count(
    table: &PieceTable,
    start: u64,
    end: u64,
    encoding: TextEncoding,
) -> Result<u64, EditorError> {
    let mut cursor = start;
    let mut count = 0_u64;
    while cursor < end {
        let (bytes, core_size) = scalar_chunk(table, cursor, end)?;
        let mut relative = 0_usize;
        while relative < core_size {
            relative = relative
                .checked_add(navigable_scalar_width(encoding, &bytes[relative..]))
                .ok_or(EditorError::OffsetOverflow)?;
            count = count.checked_add(1).ok_or(EditorError::OffsetOverflow)?;
        }
        cursor = cursor
            .checked_add(u64::try_from(relative).map_err(|_| EditorError::OffsetOverflow)?)
            .ok_or(EditorError::OffsetOverflow)?;
    }
    Ok(count)
}

pub(super) fn byte_at_scalar_column(
    table: &PieceTable,
    start: u64,
    end: u64,
    column: u64,
    encoding: TextEncoding,
) -> Result<u64, EditorError> {
    let mut cursor = start;
    let mut remaining_column = column;
    while cursor < end && remaining_column > 0 {
        let (bytes, core_size) = scalar_chunk(table, cursor, end)?;
        let mut relative = 0_usize;
        while relative < core_size && remaining_column > 0 {
            relative = relative
                .checked_add(navigable_scalar_width(encoding, &bytes[relative..]))
                .ok_or(EditorError::OffsetOverflow)?;
            remaining_column -= 1;
        }
        cursor = cursor
            .checked_add(u64::try_from(relative).map_err(|_| EditorError::OffsetOverflow)?)
            .ok_or(EditorError::OffsetOverflow)?;
    }
    Ok(cursor.min(end))
}

fn scalar_chunk(
    table: &PieceTable,
    cursor: u64,
    end: u64,
) -> Result<(Vec<u8>, usize), EditorError> {
    let remaining = end - cursor;
    let core_len = remaining.min(NAVIGATION_CHUNK_BYTES);
    let read_len = remaining.min(core_len.saturating_add(3));
    let bytes = table.read_vec(cursor, read_len)?;
    let core_size = usize::try_from(core_len).map_err(|_| EditorError::OffsetOverflow)?;
    Ok((bytes, core_size))
}

pub(super) fn navigable_scalar_width(encoding: TextEncoding, bytes: &[u8]) -> usize {
    match encoding {
        TextEncoding::Utf8 => next_scalar_len(&bytes[..bytes.len().min(4)]).unwrap_or(1),
        TextEncoding::Windows1252 => 1,
    }
}
