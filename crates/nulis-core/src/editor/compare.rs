//! Comparing two open documents and copying a difference from one to the other.

use super::{
    record_bookmark_edit, relocate_bookmarks, summary, validate_revision, DocumentId,
    DocumentManager, DocumentSummary, EditorError, Selection, MAX_MERGE_BYTES,
};
use nulis_diff::{
    compare_binary, compare_text, BinaryDiffRequest, CompareMode, DiffPage, TextDiffRequest,
};

impl DocumentManager {
    /// Compares bounded windows from two open documents without materializing
    /// either complete source.
    ///
    /// # Errors
    /// Returns an error for unknown documents, invalid bounds, or source reads.
    pub(crate) fn compare_documents(
        &self,
        request: &DocumentCompareRequest,
    ) -> Result<DiffPage, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let left = sessions
            .get(&request.left_id)
            .ok_or_else(|| EditorError::UnknownDocument(request.left_id.clone()))?;
        let right = sessions
            .get(&request.right_id)
            .ok_or_else(|| EditorError::UnknownDocument(request.right_id.clone()))?;
        match request.mode {
            CompareMode::Text => compare_text(
                left.document.table(),
                right.document.table(),
                TextDiffRequest {
                    left_start_byte: request.left_start_byte,
                    right_start_byte: request.right_start_byte,
                    left_start_line: request.left_start_line,
                    right_start_line: request.right_start_line,
                    max_bytes: request.max_bytes,
                    max_changes: request.max_changes,
                },
            ),
            CompareMode::Binary => {
                if request.left_start_byte != request.right_start_byte {
                    return Err(EditorError::Diff(
                        "binary comparison requires aligned source offsets".into(),
                    ));
                }
                compare_binary(
                    left.document.table(),
                    right.document.table(),
                    BinaryDiffRequest {
                        start_byte: request.left_start_byte,
                        max_bytes: request.max_bytes,
                        max_changes: request.max_changes,
                    },
                )
            }
        }
        .map_err(|error| EditorError::Diff(error.to_string()))
    }

    /// Copies one exact byte range from a source document over a range in a
    /// target document as one revision-checked, undoable edit.
    ///
    /// # Errors
    /// Returns an error for stale revisions, invalid/oversized ranges, unknown
    /// documents, or backing-store failures.
    pub(crate) fn apply_compare_change(
        &self,
        request: &ApplyCompareRequest,
    ) -> Result<DocumentSummary, EditorError> {
        if request.source_end < request.source_start {
            return Err(EditorError::InvalidRange {
                start: request.source_start,
                end: request.source_end,
            });
        }
        if request.target_end < request.target_start {
            return Err(EditorError::InvalidRange {
                start: request.target_start,
                end: request.target_end,
            });
        }
        let source_len = request.source_end - request.source_start;
        if source_len > MAX_MERGE_BYTES {
            return Err(EditorError::MergeTooLarge {
                selected: source_len,
                maximum: MAX_MERGE_BYTES,
            });
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let copied = {
            let source = sessions
                .get(&request.source_id)
                .ok_or_else(|| EditorError::UnknownDocument(request.source_id.clone()))?;
            source
                .document
                .table()
                .read_vec(request.source_start, source_len)?
        };
        let target = sessions
            .get_mut(&request.target_id)
            .ok_or_else(|| EditorError::UnknownDocument(request.target_id.clone()))?;
        validate_revision(target, request.expected_target_revision)?;
        if request.target_end > target.document.table().len() {
            return Err(EditorError::OffsetOutOfRange {
                offset: request.target_end,
                document_len: target.document.table().len(),
            });
        }
        target.document.replace(
            request.target_start,
            request.target_end - request.target_start,
            &copied,
        )?;
        let inserted_len = u64::try_from(copied.len()).map_err(|_| EditorError::OffsetOverflow)?;
        let caret = request
            .target_start
            .checked_add(inserted_len)
            .ok_or(EditorError::OffsetOverflow)?;
        target.selections = vec![Selection::caret(caret)];
        target.rectangular_selection = None;
        let bookmark_edits = vec![(request.target_start, request.target_end, copied.len())];
        relocate_bookmarks(&mut target.bookmarks, &bookmark_edits)?;
        record_bookmark_edit(target, bookmark_edits);
        target.preferred_scalar_columns = vec![None];
        target.navigation_index.reset();
        Ok(summary(&request.target_id, target))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DocumentCompareRequest {
    pub left_id: DocumentId,
    pub right_id: DocumentId,
    pub mode: CompareMode,
    pub left_start_byte: u64,
    pub right_start_byte: u64,
    pub left_start_line: u64,
    pub right_start_line: u64,
    pub max_bytes: usize,
    pub max_changes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApplyCompareRequest {
    pub source_id: DocumentId,
    pub target_id: DocumentId,
    pub expected_target_revision: u64,
    pub source_start: u64,
    pub source_end: u64,
    pub target_start: u64,
    pub target_end: u64,
}
