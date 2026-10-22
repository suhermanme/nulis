//! Changes made to open files by other programs: detection, Reload, and
//! keeping the editor's version. Detection only asks the file system for
//! each file's size, time, and identity; it reads no content.

use super::{
    recovery_manifest_path, remove_if_present, summary, DocumentId, DocumentManager,
    DocumentSummary, EditorError, Selection, SparseNavigationIndex, DEFAULT_HISTORY_BUDGET,
};
use crate::line_changes::ChangeTracker;
use nulis_document::{Document, PieceTable};
use nulis_filesystem::{fingerprint, FileFingerprint};
use nulis_large_file::{DiskAppendStore, FileSource};
use serde::Serialize;
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExternalChangeKind {
    Modified,
    Deleted,
}

/// An open document whose file is no longer the one it was opened or saved as.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalChange {
    pub document_id: DocumentId,
    pub path: String,
    pub kind: ExternalChangeKind,
    /// The document has edits that a Reload would discard.
    pub dirty: bool,
}

/// A freshly opened local file: its document, fingerprint, and edit store.
pub(super) struct OpenedSource {
    pub document: Document,
    pub fingerprint: Option<FileFingerprint>,
    pub additions_path: PathBuf,
}

impl DocumentManager {
    /// Opens `path` as a disk-backed document without reading its content.
    pub(super) fn open_source(&self, path: &Path) -> Result<OpenedSource, EditorError> {
        let fingerprint_before = fingerprint(path).map_err(|error| EditorError::io(&error))?;
        let source = FileSource::open(path).map_err(|error| EditorError::io(&error))?;
        let source_fingerprint = fingerprint(path).map_err(|error| EditorError::io(&error))?;
        if fingerprint_before != source_fingerprint || source_fingerprint.is_none() {
            return Err(EditorError::SourceChangedWhileOpening);
        }
        fs::create_dir_all(&self.session_root).map_err(|error| EditorError::io(&error))?;
        let temporary = tempfile::Builder::new()
            .prefix("document-")
            .suffix(".add")
            .tempfile_in(&self.session_root)
            .map_err(|error| EditorError::io(&error))?;
        let (temporary_file, additions_path) = temporary
            .keep()
            .map_err(|error| EditorError::io(&error.error))?;
        drop(temporary_file);
        let additions = match DiskAppendStore::open(&additions_path) {
            Ok(additions) => additions,
            Err(error) => {
                let _ = fs::remove_file(&additions_path);
                return Err(EditorError::io(&error));
            }
        };
        Ok(OpenedSource {
            document: Document::new(
                PieceTable::new(std::sync::Arc::new(source), Box::new(additions)),
                DEFAULT_HISTORY_BUDGET,
            ),
            fingerprint: source_fingerprint,
            additions_path,
        })
    }

    /// Lists open local documents whose file was changed or removed outside
    /// Nulis since it was opened, saved, reloaded, or last kept.
    ///
    /// # Errors
    /// Returns an error if document state is unavailable.
    pub fn external_changes(&self) -> Result<Vec<ExternalChange>, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let mut changes = sessions
            .iter()
            .filter_map(|(id, session)| {
                let path = session.source_path.as_ref()?;
                let expected = session.source_fingerprint.as_ref()?;
                // A file that cannot be examined right now is not reported.
                let kind = match fingerprint(path) {
                    Ok(None) => ExternalChangeKind::Deleted,
                    Ok(Some(current)) if current != *expected => ExternalChangeKind::Modified,
                    _ => return None,
                };
                Some(ExternalChange {
                    document_id: id.clone(),
                    path: path.to_string_lossy().into_owned(),
                    kind,
                    dirty: !session.document.is_clean(),
                })
            })
            .collect::<Vec<_>>();
        changes.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(changes)
    }

    /// Keeps the editor's version of a document whose file changed outside:
    /// the change is no longer reported, and the next Save replaces the file
    /// (or creates it again if it was removed).
    ///
    /// # Errors
    /// Returns an error for an unknown document or one without a local file.
    pub fn keep_external_version(
        &self,
        document_id: &DocumentId,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let path = session
            .source_path
            .clone()
            .ok_or(EditorError::SavePathRequired)?;
        session.source_fingerprint = fingerprint(&path).map_err(|error| EditorError::io(&error))?;
        Ok(summary(document_id, session))
    }

    /// Replaces a document's content with its file as it is now, in the same
    /// document (so tabs and views stay). Undo history and line-change
    /// markers start over; carets and bookmarks keep their byte positions
    /// where the new content is long enough.
    ///
    /// # Errors
    /// Returns an error for a document without a local file, one with unsaved
    /// edits unless `discard` is set, or a file that cannot be opened.
    pub fn reload(
        &self,
        document_id: &DocumentId,
        discard: bool,
    ) -> Result<DocumentSummary, EditorError> {
        let path = {
            let sessions = self
                .sessions
                .lock()
                .map_err(|_| EditorError::StateUnavailable)?;
            let session = sessions
                .get(document_id)
                .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
            if !discard && !session.document.is_clean() {
                return Err(EditorError::Recovery(
                    "unsaved edits require confirmation before reloading".into(),
                ));
            }
            session
                .source_path
                .clone()
                .ok_or(EditorError::SavePathRequired)?
        };
        // Opened outside the lock: the file may be large or slow to reach.
        let opened = self.open_source(&path)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let Some(session) = sessions.get_mut(document_id) else {
            let _ = fs::remove_file(&opened.additions_path);
            return Err(EditorError::UnknownDocument(document_id.clone()));
        };
        let len = opened.document.table().len();
        let read_only = session.document.is_read_only();
        let old_additions = session.additions_path.replace(opened.additions_path);
        session.document = opened.document;
        if read_only {
            session.document.set_read_only(true);
        }
        session.source_fingerprint = opened.fingerprint;
        let clamp = |offset: u64| offset.min(len);
        session.selections = session
            .selections
            .iter()
            .map(|selection| Selection {
                anchor_byte: clamp(selection.anchor_byte),
                active_byte: clamp(selection.active_byte),
            })
            .collect();
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None; session.selections.len()];
        session.bookmarks.retain(|offset| *offset <= len);
        session.bookmark_undo = VecDeque::new();
        session.bookmark_redo = VecDeque::new();
        session.changes = ChangeTracker::default();
        session.navigation_index = SparseNavigationIndex::default();
        // Discarded edits must not come back as a recovery choice.
        if let Some(id) = session.recovery_id.take() {
            let _ = remove_if_present(&recovery_manifest_path(&self.session_root, &id));
        }
        let result = summary(document_id, session);
        drop(sessions);
        if let Some(old) = old_additions {
            let _ = remove_if_present(&old);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DocumentManager, Selection};
    use super::ExternalChangeKind;
    use std::{fs, thread, time::Duration};

    /// Rewrites a file so its fingerprint changes even on coarse clocks.
    fn rewrite(path: &std::path::Path, text: &str) {
        thread::sleep(Duration::from_millis(20));
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, text).expect("write");
        fs::rename(&temporary, path).expect("replace");
    }

    fn text(manager: &DocumentManager, id: &super::DocumentId) -> String {
        manager.markdown_preview_source(id).expect("text").text
    }

    #[test]
    fn outside_changes_are_reported_until_reloaded() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("notes.txt");
        fs::write(&path, "one\ntwo\nthree").expect("file");
        let manager = DocumentManager::default();
        let id = manager.open_file(&path).expect("open").document_id;
        assert_eq!(manager.external_changes().expect("check").len(), 0);

        rewrite(&path, "new");
        let changes = manager.external_changes().expect("check");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, ExternalChangeKind::Modified);
        assert!(!changes[0].dirty);

        manager
            .set_selections(&id, vec![Selection::caret(9)])
            .expect("caret");
        let reloaded = manager.reload(&id, false).expect("reload");
        assert_eq!(reloaded.document_id, id, "same document, same tab");
        assert_eq!(text(&manager, &id), "new");
        assert_eq!(manager.external_changes().expect("check").len(), 0);
        assert!(manager.undo(&id).is_ok());
        assert_eq!(text(&manager, &id), "new", "nothing to undo after a reload");
    }

    #[test]
    fn reload_with_unsaved_edits_needs_discard() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("draft.txt");
        fs::write(&path, "disk").expect("file");
        let manager = DocumentManager::default();
        let opened = manager.open_file(&path).expect("open");
        manager
            .insert_text(&opened.document_id, opened.revision, "mine ", false)
            .expect("edit");
        rewrite(&path, "theirs");
        assert!(manager.external_changes().expect("check")[0].dirty);
        assert!(manager.reload(&opened.document_id, false).is_err());
        assert_eq!(text(&manager, &opened.document_id), "mine disk");
        let reloaded = manager.reload(&opened.document_id, true).expect("discard");
        assert!(!reloaded.dirty);
        assert_eq!(text(&manager, &opened.document_id), "theirs");
    }

    #[test]
    fn keeping_the_editor_version_stops_reports_and_lets_save_overwrite() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("kept.txt");
        fs::write(&path, "disk").expect("file");
        let manager = DocumentManager::default();
        let opened = manager.open_file(&path).expect("open");
        let edited = manager
            .insert_text(&opened.document_id, opened.revision, "mine ", false)
            .expect("edit");
        rewrite(&path, "theirs");
        assert!(
            manager.save(&opened.document_id, edited.revision).is_err(),
            "save refuses at first"
        );
        manager
            .keep_external_version(&opened.document_id)
            .expect("keep");
        assert_eq!(manager.external_changes().expect("check").len(), 0);
        manager
            .save(&opened.document_id, edited.revision)
            .expect("save");
        assert_eq!(fs::read_to_string(&path).expect("saved"), "mine disk");
    }

    #[test]
    fn a_removed_file_is_reported_as_deleted() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("gone.txt");
        fs::write(&path, "x").expect("file");
        let manager = DocumentManager::default();
        manager.open_file(&path).expect("open");
        fs::remove_file(&path).expect("remove");
        let changes = manager.external_changes().expect("check");
        assert_eq!(changes[0].kind, ExternalChangeKind::Deleted);
    }
}
