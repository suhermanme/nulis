//! Bookmarks: toggling one on a line, moving between them, and clearing them,
//! each an undoable step.

use super::{
    push_bounded_history, BookmarkDirection, DocumentId, DocumentManager, EditorError,
    EditorSession, HistoryEntry, Selection, MAX_BOOKMARKS,
};

impl DocumentManager {
    /// Toggles a bookmark at the primary caret.
    ///
    /// # Errors
    /// Returns an error for an unknown document or when the bookmark limit is reached.
    pub fn toggle_bookmark(&self, document_id: &DocumentId) -> Result<Vec<u64>, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let offset = session
            .selections
            .first()
            .map_or(0, |selection| selection.active_byte);
        if !session.bookmarks.remove(&offset) {
            if session.bookmarks.len() == MAX_BOOKMARKS {
                return Err(EditorError::TooManyBookmarks(MAX_BOOKMARKS));
            }
            session.bookmarks.insert(offset);
        }
        Ok(session.bookmarks.iter().copied().collect())
    }

    /// Moves the primary caret to the next or previous bookmark, wrapping at
    /// the ends of the document. A document with no bookmarks is unchanged.
    ///
    /// # Errors
    /// Returns an error for an unknown document.
    pub fn move_to_bookmark(
        &self,
        document_id: &DocumentId,
        direction: BookmarkDirection,
    ) -> Result<Vec<Selection>, EditorError> {
        use std::ops::Bound::{Excluded, Unbounded};

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let current = session
            .selections
            .first()
            .map_or(0, |selection| selection.active_byte);
        let target = match direction {
            BookmarkDirection::Next => session
                .bookmarks
                .range((Excluded(current), Unbounded))
                .next()
                .or_else(|| session.bookmarks.first()),
            BookmarkDirection::Previous => session
                .bookmarks
                .range((Unbounded, Excluded(current)))
                .next_back()
                .or_else(|| session.bookmarks.last()),
        }
        .copied();
        if let Some(target) = target {
            session.selections = vec![Selection::caret(target)];
            session.rectangular_selection = None;
            session.preferred_scalar_columns = vec![None];
        }
        Ok(session.selections.clone())
    }

    /// Removes all bookmarks from a document.
    ///
    /// # Errors
    /// Returns an error for an unknown document.
    pub fn clear_bookmarks(&self, document_id: &DocumentId) -> Result<(), EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        session.bookmarks.clear();
        Ok(())
    }
}

/// Records one applied edit for bookmark relocation and line change markers.
pub(super) fn record_bookmark_edit(session: &mut EditorSession, edits: Vec<(u64, u64, usize)>) {
    if let Some(snippet) = session.snippet.as_mut() {
        if !snippet.relocate(&edits) {
            session.snippet = None;
        }
    }
    let changes = session.changes.apply(&edits);
    push_bounded_history(&mut session.bookmark_undo, HistoryEntry { edits, changes });
    session.bookmark_redo.clear();
}
