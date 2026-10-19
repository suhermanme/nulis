//! Replace in Files, in two steps. A preview lists the files that match,
//! with a stamp of each file's size and modification time. Replacing then
//! touches only the previewed files: a file open in the editor is changed in
//! memory (one undo step, left unsaved), and a closed file is changed only if
//! its stamp still matches, through the same atomic save as Save.

use super::{
    DocumentId, DocumentManager, DocumentSummary, EditorError, FileSearchScope,
    MAX_WORKSPACE_SEARCH_RESULTS,
};
use nulis_search::SearchOptions;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// Files one Replace in Files may change.
pub const MAX_REPLACE_FILES: usize = 1_000;

/// A file's size and modification time when it was previewed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileStamp {
    #[serde(with = "super::decimal_u64")]
    pub size: u64,
    /// Nanoseconds since the Unix epoch, as decimal text, when the platform reports it.
    pub modified: Option<String>,
}

impl FileStamp {
    fn of(path: &Path) -> Result<Self, EditorError> {
        let metadata = fs::metadata(path).map_err(|error| EditorError::io(&error))?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_nanos().to_string());
        Ok(Self {
            size: metadata.len(),
            modified,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacePreviewFile {
    pub path: String,
    /// Matches found on disk (an open document may hold different text).
    pub matches: usize,
    pub stamp: FileStamp,
    /// The file is open in the editor, so its in-memory text is changed.
    pub open: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacePreview {
    pub files: Vec<ReplacePreviewFile>,
    pub files_scanned: usize,
    /// The search stopped at a limit; only listed files would change.
    pub truncated: bool,
}

/// One previewed file to change.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplaceTarget {
    pub path: String,
    pub stamp: FileStamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceFileOutcome {
    pub path: String,
    pub replacements: usize,
    /// Why the file was not changed.
    pub error: Option<String>,
    /// The new state of a document open in the editor.
    pub document: Option<DocumentSummary>,
}

fn same_file(left: &Path, right: &Path) -> bool {
    let canonical = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    left == right || canonical(left) == canonical(right)
}

impl DocumentManager {
    /// The open document whose source is `path`, if any.
    fn document_for_path(&self, path: &Path) -> Result<Option<DocumentId>, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        Ok(sessions
            .iter()
            .find(|(_, session)| {
                session
                    .source_path
                    .as_deref()
                    .is_some_and(|source| same_file(source, path))
            })
            .map(|(id, _)| id.clone()))
    }

    /// Lists the files below `root` that contain `query`, with their stamps.
    ///
    /// # Errors
    /// Returns an error for an invalid root or pattern.
    pub fn preview_replace_in_files(
        &self,
        root: &Path,
        query: &str,
        options: SearchOptions,
        scope: &FileSearchScope,
    ) -> Result<ReplacePreview, EditorError> {
        let found =
            self.find_in_files(root, query, options, scope, MAX_WORKSPACE_SEARCH_RESULTS)?;
        let mut files: Vec<ReplacePreviewFile> = Vec::new();
        let mut truncated = found.truncated;
        for found in found.matches {
            if let Some(last) = files.last_mut().filter(|last| last.path == found.path) {
                last.matches += 1;
                continue;
            }
            if files.len() == MAX_REPLACE_FILES {
                truncated = true;
                break;
            }
            let path = PathBuf::from(&found.path);
            let Ok(stamp) = FileStamp::of(&path) else {
                continue;
            };
            files.push(ReplacePreviewFile {
                open: self.document_for_path(&path)?.is_some(),
                path: found.path,
                matches: 1,
                stamp,
            });
        }
        Ok(ReplacePreview {
            files,
            files_scanned: found.files_scanned,
            truncated,
        })
    }

    /// Replaces every match in each previewed file and reports per file.
    /// A failure in one file does not stop the others.
    ///
    /// # Errors
    /// Returns an error only for more than [`MAX_REPLACE_FILES`] targets.
    pub fn replace_in_files(
        &self,
        targets: &[ReplaceTarget],
        query: &str,
        replacement: &str,
        options: SearchOptions,
    ) -> Result<Vec<ReplaceFileOutcome>, EditorError> {
        if targets.len() > MAX_REPLACE_FILES {
            return Err(EditorError::WorkspaceSearchLimit(targets.len()));
        }
        Ok(targets
            .iter()
            .map(|target| {
                let outcome = |replacements, error: Option<String>, document| ReplaceFileOutcome {
                    path: target.path.clone(),
                    replacements,
                    error,
                    document,
                };
                match self.replace_in_file(target, query, replacement, options) {
                    Ok((replacements, document)) => outcome(replacements, None, document),
                    Err(error) => outcome(0, Some(error.to_string()), None),
                }
            })
            .collect())
    }

    fn replace_in_file(
        &self,
        target: &ReplaceTarget,
        query: &str,
        replacement: &str,
        options: SearchOptions,
    ) -> Result<(usize, Option<DocumentSummary>), EditorError> {
        let path = Path::new(&target.path);
        if let Some(id) = self.document_for_path(path)? {
            let current = self.summary(&id)?;
            if current.read_only {
                return Err(EditorError::Io("the document is in Read-Only mode".into()));
            }
            let result =
                self.replace_all(&id, current.revision, query, replacement, options, None)?;
            return Ok((result.replacements, Some(result.document)));
        }
        if FileStamp::of(path)? != target.stamp {
            return Err(EditorError::Io(
                "the file changed on disk after the preview".into(),
            ));
        }
        let opened = self.open_file(path)?;
        let changed = self
            .replace_all(
                &opened.document_id,
                opened.revision,
                query,
                replacement,
                options,
                None,
            )
            .and_then(|result| {
                if result.replacements > 0 {
                    self.save(&opened.document_id, result.document.revision)?;
                }
                Ok(result.replacements)
            });
        // The temporary document goes away whether or not the save worked;
        // after a failure the file on disk is unchanged.
        let closed = self.close_document(&opened.document_id, true);
        let replacements = changed?;
        closed?;
        Ok((replacements, None))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DocumentManager, FileSearchScope};
    use super::ReplaceTarget;
    use nulis_search::SearchOptions;
    use std::fs;

    fn scope() -> FileSearchScope {
        FileSearchScope {
            include: vec!["*.txt".into()],
            recursive: true,
        }
    }

    #[test]
    fn preview_then_replace_changes_closed_files_and_open_documents_in_memory() {
        let root = tempfile::tempdir().expect("root");
        let closed = root.path().join("closed.txt");
        let open = root.path().join("open.txt");
        fs::write(&closed, "cat cat\r\nbird").expect("closed file");
        fs::write(&open, "cat").expect("open file");
        fs::write(root.path().join("skip.md"), "cat").expect("filtered file");
        let manager = DocumentManager::default();
        let opened = manager.open_file(&open).expect("open document");

        let preview = manager
            .preview_replace_in_files(root.path(), "cat", SearchOptions::default(), &scope())
            .expect("preview");
        let mut files = preview
            .files
            .iter()
            .map(|file| (file.path.ends_with("closed.txt"), file.matches, file.open))
            .collect::<Vec<_>>();
        files.sort_unstable();
        assert_eq!(files, vec![(false, 1, true), (true, 2, false)]);
        assert_eq!(
            fs::read_to_string(&closed).expect("unchanged"),
            "cat cat\r\nbird",
            "preview writes nothing"
        );

        let targets = preview
            .files
            .iter()
            .map(|file| ReplaceTarget {
                path: file.path.clone(),
                stamp: file.stamp.clone(),
            })
            .collect::<Vec<_>>();
        let outcomes = manager
            .replace_in_files(&targets, "cat", "dog", SearchOptions::default())
            .expect("replace");
        assert!(
            outcomes.iter().all(|outcome| outcome.error.is_none()),
            "{outcomes:?}"
        );
        assert_eq!(
            fs::read_to_string(&closed).expect("closed"),
            "dog dog\r\nbird"
        );
        // The open document changed in memory only.
        assert_eq!(fs::read_to_string(&open).expect("open on disk"), "cat");
        let open_outcome = outcomes
            .iter()
            .find(|outcome| outcome.path.ends_with("open.txt"))
            .expect("open outcome");
        let document = open_outcome.document.as_ref().expect("open summary");
        assert_eq!(document.document_id, opened.document_id);
        assert!(document.dirty);
        assert_eq!(
            manager
                .markdown_preview_source(&opened.document_id)
                .expect("text")
                .text,
            "dog"
        );
    }

    #[test]
    fn a_file_changed_after_the_preview_is_skipped_and_others_continue() {
        let root = tempfile::tempdir().expect("root");
        let first = root.path().join("a.txt");
        let second = root.path().join("b.txt");
        fs::write(&first, "cat").expect("first");
        fs::write(&second, "cat").expect("second");
        let manager = DocumentManager::default();
        let preview = manager
            .preview_replace_in_files(root.path(), "cat", SearchOptions::default(), &scope())
            .expect("preview");
        fs::write(&first, "cat and more cat").expect("changed elsewhere");
        let targets = preview
            .files
            .iter()
            .map(|file| ReplaceTarget {
                path: file.path.clone(),
                stamp: file.stamp.clone(),
            })
            .collect::<Vec<_>>();
        let outcomes = manager
            .replace_in_files(&targets, "cat", "dog", SearchOptions::default())
            .expect("replace");
        let first_outcome = outcomes
            .iter()
            .find(|outcome| outcome.path.ends_with("a.txt"))
            .expect("first outcome");
        assert!(first_outcome
            .error
            .as_deref()
            .is_some_and(|error| error.contains("changed on disk")));
        assert_eq!(
            fs::read_to_string(&first).expect("first"),
            "cat and more cat"
        );
        assert_eq!(fs::read_to_string(&second).expect("second"), "dog");
    }

    #[test]
    fn read_only_open_documents_are_reported_not_changed() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("locked.txt");
        fs::write(&path, "cat").expect("file");
        let manager = DocumentManager::default();
        let opened = manager.open_file(&path).expect("open");
        manager
            .toggle_read_only(&opened.document_id)
            .expect("read only");
        let preview = manager
            .preview_replace_in_files(root.path(), "cat", SearchOptions::default(), &scope())
            .expect("preview");
        let targets = vec![ReplaceTarget {
            path: preview.files[0].path.clone(),
            stamp: preview.files[0].stamp.clone(),
        }];
        let outcomes = manager
            .replace_in_files(&targets, "cat", "dog", SearchOptions::default())
            .expect("replace");
        assert!(outcomes[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Read-Only")));
        assert_eq!(
            manager
                .markdown_preview_source(&opened.document_id)
                .expect("text")
                .text,
            "cat"
        );
    }
}
