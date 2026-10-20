//! Moving between edited lines and bookmarks: next and previous change, the
//! list of bookmarked lines, and deleting bookmarked lines.

use super::{
    find_all::{line_preview, LineTracker},
    find_line_boundary, find_line_start, DocumentId, DocumentManager, EditorError, FoundLine,
    LineEditResult, LineOperation, Selection,
};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ChangeDirection {
    Next,
    Previous,
}

impl DocumentManager {
    /// The start of the next (or previous) edited line after (or before) the
    /// line holding `from`, according to the line change indicator; `None`
    /// when there is none in that direction.
    ///
    /// # Errors
    /// Returns an error for an unknown document or a read failure.
    pub fn change_near(
        &self,
        document_id: &DocumentId,
        from: u64,
        direction: ChangeDirection,
    ) -> Result<Option<u64>, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        let from = from.min(table.len());
        let line_start = find_line_start(table, from)?;
        let line = find_line_boundary(table, line_start)?;
        let mut starts = session
            .changes
            .ranges()
            .iter()
            .map(|range| range.start)
            .collect::<Vec<_>>();
        starts.sort_unstable();
        let target = match direction {
            ChangeDirection::Next => {
                let after = line.next_start.unwrap_or(table.len().saturating_add(1));
                starts.into_iter().find(|start| *start >= after)
            }
            ChangeDirection::Previous => starts.into_iter().rev().find(|start| *start < line_start),
        };
        target
            .map(|offset| find_line_start(table, offset.min(table.len())))
            .transpose()
    }

    /// Every bookmarked line with its number and the start of its text.
    ///
    /// # Errors
    /// Returns an error for an unknown document or a read failure.
    pub fn bookmark_lines(&self, document_id: &DocumentId) -> Result<Vec<FoundLine>, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        let mut tracker = LineTracker::at_start();
        let mut lines: Vec<FoundLine> = Vec::new();
        // Bookmarks are kept in byte order, so one forward pass numbers them all.
        for &offset in &session.bookmarks {
            let offset = offset.min(table.len());
            let (number, start) = tracker.advance(table, offset)?;
            if lines.last().is_some_and(|last| last.start_byte == start) {
                continue;
            }
            let (text, truncated) = line_preview(table, session.encoding, start)?;
            lines.push(FoundLine {
                line: number,
                start_byte: start,
                text,
                truncated,
                matches: 1,
            });
        }
        Ok(lines)
    }

    /// Deletes every bookmarked line as one undo step and clears the bookmarks.
    ///
    /// # Errors
    /// As for [`Self::line_operation`].
    pub fn delete_bookmarked_lines(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
    ) -> Result<LineEditResult, EditorError> {
        let (bookmarks, previous) = {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| EditorError::StateUnavailable)?;
            let session = sessions
                .get_mut(document_id)
                .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
            let previous = session.selections.clone();
            let bookmarks = session
                .bookmarks
                .iter()
                .map(|offset| Selection::caret(*offset))
                .collect::<Vec<_>>();
            if !bookmarks.is_empty() {
                session.selections.clone_from(&bookmarks);
                session.rectangular_selection = None;
            }
            (bookmarks, previous)
        };
        if bookmarks.is_empty() {
            return Err(EditorError::Io("there are no bookmarks".into()));
        }
        let result =
            self.line_operation(document_id, expected_revision, LineOperation::DeleteLines);
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        if let Some(session) = sessions.get_mut(document_id) {
            match &result {
                Ok(_) => session.bookmarks.clear(),
                // Nothing changed: put the user's selection back.
                Err(_) => session.selections = previous,
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DocumentManager, Selection};
    use super::ChangeDirection;

    #[test]
    fn next_and_previous_change_follow_edited_lines() {
        let manager = DocumentManager::default();
        let opened = manager
            .create_untitled("t".into(), "a\nb\nc\nd\ne".into())
            .expect("document");
        let id = opened.document_id;
        manager
            .set_selections(&id, vec![Selection::caret(2)])
            .expect("caret");
        let edited = manager
            .insert_text(&id, opened.revision, "x", false)
            .expect("edit b");
        manager
            .set_selections(&id, vec![Selection::caret(9)])
            .expect("caret");
        manager
            .insert_text(&id, edited.revision, "y", false)
            .expect("edit e");
        // "a\nxb\nc\nd\nye": changes on lines 2 and 5.
        assert_eq!(
            manager
                .change_near(&id, 0, ChangeDirection::Next)
                .expect("next"),
            Some(2)
        );
        assert_eq!(
            manager
                .change_near(&id, 3, ChangeDirection::Next)
                .expect("next"),
            Some(9)
        );
        assert_eq!(
            manager
                .change_near(&id, 9, ChangeDirection::Next)
                .expect("none"),
            None
        );
        assert_eq!(
            manager
                .change_near(&id, 9, ChangeDirection::Previous)
                .expect("previous"),
            Some(2)
        );
        assert_eq!(
            manager
                .change_near(&id, 2, ChangeDirection::Previous)
                .expect("none"),
            None
        );
    }

    #[test]
    fn bookmarks_are_listed_with_line_numbers_and_can_be_deleted_as_lines() {
        let manager = DocumentManager::default();
        let opened = manager
            .create_untitled("t".into(), "one\ntwo\nthree\nfour".into())
            .expect("document");
        let id = opened.document_id;
        for offset in [5, 15] {
            manager
                .set_selections(&id, vec![Selection::caret(offset)])
                .expect("caret");
            manager.toggle_bookmark(&id).expect("bookmark");
        }
        let lines = manager.bookmark_lines(&id).expect("list");
        let listed = lines
            .iter()
            .map(|line| (line.line, line.text.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(listed, vec![(2, "two"), (4, "four")]);
        manager
            .delete_bookmarked_lines(&id, opened.revision)
            .expect("delete");
        assert_eq!(
            manager.markdown_preview_source(&id).expect("text").text,
            "one\nthree"
        );
        assert_eq!(manager.bookmark_lines(&id).expect("list").len(), 0);
        manager.undo(&id).expect("undo");
        assert_eq!(
            manager.markdown_preview_source(&id).expect("text").text,
            "one\ntwo\nthree\nfour"
        );
    }

    #[test]
    fn counts_characters_words_and_lines_in_one_pass() {
        let manager = DocumentManager::default();
        let id = manager
            .create_untitled("t".into(), "héllo  world\r\nnext\rlast".into())
            .expect("document")
            .document_id;
        let count = manager.character_count(&id).expect("count");
        assert_eq!(count.characters.as_deref(), Some("23"));
        assert_eq!(count.words.as_deref(), Some("4"));
        assert_eq!(count.lines.as_deref(), Some("3"));
    }
}
