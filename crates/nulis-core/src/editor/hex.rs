//! The hex view: its bounded rows, typing a nibble, replacing and deleting
//! bytes, and copying a byte selection.

use super::{
    editable_ranges, primary_selection_range, record_bookmark_edit, relocate_bookmarks, summary,
    validate_revision, DocumentId, DocumentManager, EditorError, EditorSession, HexCaret,
    HexClipboardPayload, HexCursor, HexEditResult, HexViewportRequest, HexViewportRow,
    HexViewportSnapshot, Selection, MAX_HEX_CLIPBOARD_BYTES, MAX_HEX_EDIT_BYTES,
    MAX_HEX_VIEWPORT_BYTES,
};
use nulis_document::ByteOffset;
use nulis_hex::{parse_hex, replace_nibble, HexLayout};

impl DocumentManager {
    /// Returns a bounded raw-byte snapshot aligned to a hexadecimal row.
    /// The returned data is independent of the document's text encoding.
    ///
    /// # Errors
    /// Returns an error for an unknown document, invalid layout, out-of-range
    /// offset, or a request beyond the hexadecimal viewport budget.
    pub fn hex_viewport(
        &self,
        request: &HexViewportRequest,
    ) -> Result<HexViewportSnapshot, EditorError> {
        let layout = HexLayout::new(request.bytes_per_row)?;
        let row_width = u64::from(layout.bytes_per_row());
        if request.max_bytes < row_width || request.max_bytes > MAX_HEX_VIEWPORT_BYTES {
            return Err(EditorError::HexViewportLimit {
                requested: request.max_bytes,
                minimum: row_width,
                maximum: MAX_HEX_VIEWPORT_BYTES,
            });
        }
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(&request.document_id)
            .ok_or_else(|| EditorError::UnknownDocument(request.document_id.clone()))?;
        let document_len = session.document.table().len();
        if request.start_byte > document_len {
            return Err(EditorError::OffsetOutOfRange {
                offset: request.start_byte,
                document_len,
            });
        }
        let start = layout.align_offset(ByteOffset(request.start_byte));
        let budget = request.max_bytes - request.max_bytes % row_width;
        let read_len = document_len.saturating_sub(start.0).min(budget);
        let bytes = session.document.table().read_vec(start.0, read_len)?;
        let rows = layout
            .rows(start, &bytes)
            .into_iter()
            .map(|row| HexViewportRow {
                start_byte: row.offset.0,
                bytes: row.bytes.to_vec(),
            })
            .collect();
        let consumed_end = start
            .0
            .checked_add(read_len)
            .ok_or(EditorError::OffsetOverflow)?;
        Ok(HexViewportSnapshot {
            document_id: request.document_id.clone(),
            revision: session.document.table().revision(),
            document_byte_len: document_len,
            bytes_per_row: layout.bytes_per_row(),
            rows,
            selections: session.selections.clone(),
            next: (consumed_end < document_len).then_some(HexCursor {
                byte_offset: consumed_end,
            }),
        })
    }

    /// Applies one hexadecimal digit to the primary byte selection. Typing a
    /// high nibble retains the byte position for the low nibble; typing a low
    /// nibble advances to the next byte.
    ///
    /// # Errors
    /// Returns an error for a stale revision, invalid nibble, invalid selection,
    /// or backing-store failure.
    pub fn type_hex_nibble(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        high_nibble: bool,
        nibble: u8,
        overwrite: bool,
    ) -> Result<HexEditResult, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let (start, end) = primary_selection_range(session);
        let document_len = session.document.table().len();
        if end > document_len {
            return Err(EditorError::OffsetOutOfRange {
                offset: end,
                document_len,
            });
        }
        let existing = if start == end && start < document_len {
            session.document.table().read_vec(start, 1)?[0]
        } else {
            0
        };
        let byte = replace_nibble(existing, high_nibble, nibble)?;
        let delete_end = if start == end && (overwrite || !high_nibble) {
            start.saturating_add(1).min(document_len)
        } else {
            end
        };
        let caret = HexCaret {
            byte_offset: if high_nibble {
                start
            } else {
                start.checked_add(1).ok_or(EditorError::OffsetOverflow)?
            },
            high_nibble: !high_nibble,
        };
        apply_hex_replacement(session, document_id, start, delete_end, &[byte], caret)
    }

    /// Replaces the primary byte selection with parsed hexadecimal bytes. A
    /// collapsed selection either inserts or overwrites raw bytes.
    ///
    /// # Errors
    /// Returns an error for stale state, malformed/oversized hexadecimal input,
    /// or backing-store failure.
    pub fn replace_hex_bytes(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        hex: &str,
        overwrite: bool,
    ) -> Result<HexEditResult, EditorError> {
        let bytes = parse_hex(hex, MAX_HEX_EDIT_BYTES)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let (start, selected_end) = primary_selection_range(session);
        let document_len = session.document.table().len();
        let byte_len = u64::try_from(bytes.len()).map_err(|_| EditorError::OffsetOverflow)?;
        let end = if start != selected_end {
            selected_end
        } else if overwrite {
            start.saturating_add(byte_len).min(document_len)
        } else {
            start
        };
        let byte_offset = start
            .checked_add(byte_len)
            .ok_or(EditorError::OffsetOverflow)?;
        let caret = HexCaret {
            byte_offset,
            high_nibble: true,
        };
        if bytes.is_empty() {
            session.selections = vec![Selection::caret(start)];
            session.rectangular_selection = None;
            return Ok(HexEditResult {
                document: summary(document_id, session),
                caret,
            });
        }
        apply_hex_replacement(session, document_id, start, end, &bytes, caret)
    }

    /// Deletes the selected raw-byte range or exactly one preceding byte.
    ///
    /// # Errors
    /// Returns an error for stale state or backing-store failure.
    pub fn delete_hex_backward(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
    ) -> Result<HexEditResult, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let (mut start, end) = primary_selection_range(session);
        if start == end {
            if start == 0 {
                return Ok(HexEditResult {
                    document: summary(document_id, session),
                    caret: HexCaret {
                        byte_offset: 0,
                        high_nibble: true,
                    },
                });
            }
            start -= 1;
        }
        let caret = HexCaret {
            byte_offset: start,
            high_nibble: true,
        };
        apply_hex_replacement(session, document_id, start, end, &[], caret)
    }

    /// Reads selected bytes as uppercase, space-separated hexadecimal text.
    ///
    /// # Errors
    /// Returns an error for an unknown document, oversized selection, or
    /// backing-store failure.
    pub fn hex_selection(
        &self,
        document_id: &DocumentId,
    ) -> Result<HexClipboardPayload, EditorError> {
        const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let ranges = editable_ranges(&session.selections)
            .into_iter()
            .filter(|(start, end)| start != end)
            .collect::<Vec<_>>();
        let selected = ranges.iter().try_fold(0_u64, |total, (start, end)| {
            total
                .checked_add(end - start)
                .ok_or(EditorError::OffsetOverflow)
        })?;
        if selected > MAX_HEX_CLIPBOARD_BYTES {
            return Err(EditorError::HexClipboardTooLarge {
                selected,
                maximum: MAX_HEX_CLIPBOARD_BYTES,
            });
        }
        let capacity = usize::try_from(selected)
            .map_err(|_| EditorError::OffsetOverflow)?
            .saturating_mul(3);
        let mut hex = String::with_capacity(capacity);
        for (start, end) in ranges {
            for byte in session.document.table().read_vec(start, end - start)? {
                if !hex.is_empty() {
                    hex.push(' ');
                }
                hex.push(char::from(DIGITS[usize::from(byte >> 4)]));
                hex.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
            }
        }
        Ok(HexClipboardPayload {
            hex,
            revision: session.document.table().revision(),
        })
    }
}

fn apply_hex_replacement(
    session: &mut EditorSession,
    document_id: &DocumentId,
    start: u64,
    end: u64,
    inserted: &[u8],
    caret: HexCaret,
) -> Result<HexEditResult, EditorError> {
    if end < start {
        return Err(EditorError::InvalidRange { start, end });
    }
    session.document.replace(start, end - start, inserted)?;
    let bookmark_edits = vec![(start, end, inserted.len())];
    relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
    record_bookmark_edit(session, bookmark_edits);
    session.selections = vec![Selection::caret(caret.byte_offset)];
    session.rectangular_selection = None;
    session.preferred_scalar_columns = vec![None];
    session.navigation_index.reset();
    Ok(HexEditResult {
        document: summary(document_id, session),
        caret,
    })
}
