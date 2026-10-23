//! Renaming an open document's file in its folder.
//!
//! The document keeps reading its original bytes through the handle it
//! already holds, so a rename never reloads or changes the text, and unsaved
//! edits stay unsaved.

use super::{fingerprint, summary, DocumentId, DocumentManager, DocumentSummary, EditorError};
use std::{fs, path::Path};

/// Longest file name accepted, in bytes.
pub const MAX_FILE_NAME_BYTES: usize = 255;

impl DocumentManager {
    /// Renames the document's file to `name` in the same folder and returns
    /// the updated summary. Refuses untitled documents, names with
    /// path separators, an existing destination (except a change of letter
    /// case only), and a file that changed on disk since it was opened.
    ///
    /// # Errors
    /// Returns an error for the cases above or when the rename fails.
    pub fn rename_file(
        &self,
        document_id: &DocumentId,
        name: &str,
    ) -> Result<DocumentSummary, EditorError> {
        let name = name.trim();
        validate_name(name)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let source = session
            .source_path
            .clone()
            .ok_or_else(|| EditorError::Rename("save the document before renaming it".into()))?;
        let destination = source.parent().unwrap_or(Path::new("")).join(name);
        if destination == source {
            return Ok(summary(document_id, session));
        }
        let current = fingerprint(&source).map_err(|error| EditorError::io(&error))?;
        if current.is_none() {
            return Err(EditorError::Rename(format!(
                "{} no longer exists",
                source.display()
            )));
        }
        if current != session.source_fingerprint {
            return Err(EditorError::Rename(
                "the file changed on disk; reload or save it first".into(),
            ));
        }
        // An existing destination is refused, unless it is this same file
        // under another letter case on a case-insensitive file system. That
        // is decided by name, not by fingerprint: on Windows two different
        // files can share a fingerprint (size and modification time).
        if fs::symlink_metadata(&destination).is_ok() && !is_case_only_rename(&source, name)? {
            return Err(EditorError::Rename(format!("{name} already exists")));
        }
        fs::rename(&source, &destination).map_err(|error| EditorError::io(&error))?;
        name.clone_into(&mut session.name);
        session.source_fingerprint =
            fingerprint(&destination).map_err(|error| EditorError::io(&error))?;
        session.language_id = self.languages().detect_path(&destination);
        session.source_path = Some(destination);
        Ok(summary(document_id, session))
    }
}

/// Whether `name` differs from the source's name only in letter case and no
/// other entry in the folder is spelled exactly `name`.
fn is_case_only_rename(source: &Path, name: &str) -> Result<bool, EditorError> {
    let current = source
        .file_name()
        .map(|current| current.to_string_lossy().to_lowercase());
    if current.as_deref() != Some(name.to_lowercase().as_str()) {
        return Ok(false);
    }
    let folder = source.parent().unwrap_or(Path::new("."));
    let folder = if folder.as_os_str().is_empty() {
        Path::new(".")
    } else {
        folder
    };
    let exact = fs::read_dir(folder)
        .map_err(|error| EditorError::io(&error))?
        .flatten()
        .any(|entry| entry.file_name() == std::ffi::OsStr::new(name));
    Ok(!exact)
}

fn validate_name(name: &str) -> Result<(), EditorError> {
    let invalid = name.is_empty()
        || name == "."
        || name == ".."
        || name.len() > MAX_FILE_NAME_BYTES
        || name.contains(['/', '\\', '\0'])
        || (cfg!(windows) && name.contains([':', '*', '?', '"', '<', '>', '|']));
    if invalid {
        return Err(EditorError::Rename(format!(
            "{name:?} is not a valid file name"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{DocumentManager, EditorError};
    use std::fs;

    fn folder() -> tempfile::TempDir {
        tempfile::tempdir().expect("temporary folder")
    }

    #[test]
    fn renames_the_file_and_keeps_text_and_edits() {
        let dir = folder();
        let source = dir.path().join("notes.txt");
        fs::write(&source, "hello").unwrap();
        let manager = DocumentManager::default();
        let opened = manager.open_file(&source).unwrap();
        let id = opened.document_id;
        manager
            .insert_text(&id, opened.revision, "!", false)
            .expect("edit");
        let renamed = manager.rename_file(&id, " notes.md ").expect("rename");
        assert_eq!(renamed.name, "notes.md");
        assert_eq!(renamed.language_id, "markdown");
        assert!(renamed.dirty);
        assert!(!source.exists());
        let destination = dir.path().join("notes.md");
        assert_eq!(renamed.source_path.as_deref(), destination.to_str());
        assert_eq!(fs::read_to_string(&destination).unwrap(), "hello");
        let saved = manager.save(&id, renamed.revision).expect("save");
        assert!(!saved.dirty);
        assert_eq!(fs::read_to_string(&destination).unwrap(), "!hello");
    }

    #[test]
    fn refuses_bad_names_existing_files_and_untitled_documents() {
        let dir = folder();
        let source = dir.path().join("a.txt");
        fs::write(&source, "a").unwrap();
        fs::write(dir.path().join("b.txt"), "b").unwrap();
        let manager = DocumentManager::default();
        let id = manager.open_file(&source).unwrap().document_id;
        for name in ["", "..", "sub/c.txt", "sub\\c.txt"] {
            assert!(
                matches!(manager.rename_file(&id, name), Err(EditorError::Rename(_))),
                "{name:?}"
            );
        }
        assert_eq!(
            manager.rename_file(&id, "b.txt"),
            Err(EditorError::Rename("b.txt already exists".into()))
        );
        assert_eq!(fs::read_to_string(dir.path().join("b.txt")).unwrap(), "b");
        assert_eq!(manager.rename_file(&id, "a.txt").unwrap().name, "a.txt");

        let untitled = manager
            .create_untitled("Untitled".into(), String::new())
            .unwrap()
            .document_id;
        assert!(matches!(
            manager.rename_file(&untitled, "x.txt"),
            Err(EditorError::Rename(_))
        ));
    }

    #[test]
    fn a_rename_that_only_changes_letter_case_is_allowed() {
        let dir = folder();
        let source = dir.path().join("readme.txt");
        fs::write(&source, "r").unwrap();
        let manager = DocumentManager::default();
        let id = manager.open_file(&source).unwrap().document_id;
        assert_eq!(
            manager.rename_file(&id, "README.txt").unwrap().name,
            "README.txt"
        );
        let names = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["README.txt".to_owned()]);
    }

    #[test]
    fn refuses_a_file_that_changed_or_vanished() {
        let dir = folder();
        let source = dir.path().join("a.txt");
        fs::write(&source, "a").unwrap();
        let manager = DocumentManager::default();
        let id = manager.open_file(&source).unwrap().document_id;
        fs::write(&source, "changed elsewhere").unwrap();
        assert!(matches!(
            manager.rename_file(&id, "b.txt"),
            Err(EditorError::Rename(_))
        ));
        fs::remove_file(&source).unwrap();
        assert!(matches!(
            manager.rename_file(&id, "b.txt"),
            Err(EditorError::Rename(_))
        ));
        assert!(!dir.path().join("b.txt").exists());
    }
}
