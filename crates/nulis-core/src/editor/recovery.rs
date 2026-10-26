//! Crash recovery: checkpoints of unsaved documents in the session folder,
//! the list offered at the next start, and restoring or discarding one.

use super::{
    indentation, remove_if_present, summary, ChangeTracker, DocumentId, DocumentManager,
    DocumentSummary, EditorError, EditorSession, RecoveryCandidate, Selection,
    SparseNavigationIndex, TextEncoding, DEFAULT_HISTORY_BUDGET,
};
use nulis_document::{Document, PieceTable, PieceTableSnapshot};
use nulis_filesystem::{fingerprint, save_atomic, FileFingerprint};
use nulis_large_file::{DiskAppendStore, FileSource};
use nulis_syntax::LanguageId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

impl DocumentManager {
    /// Durably checkpoints edits without copying an entire source file.
    ///
    /// # Errors
    /// Returns an error for an unknown document or an invalid recovery source.
    pub fn checkpoint_recovery(&self, document_id: &DocumentId) -> Result<(), EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        if session.document.is_clean() {
            return Ok(());
        }
        let original = session
            .original_path
            .as_ref()
            .or(session.source_path.as_ref())
            .ok_or_else(|| {
                EditorError::Recovery("the document has no file to recover from".into())
            })?;
        let additions = session
            .additions_path
            .as_ref()
            .ok_or_else(|| EditorError::Recovery("no persistent edit store".into()))?;
        let recovery_id = match &session.recovery_id {
            Some(id) => id.clone(),
            None => additions
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| EditorError::Recovery("invalid session filename".into()))?
                .to_owned(),
        };
        if !valid_recovery_id(&recovery_id) {
            return Err(EditorError::Recovery("invalid recovery identifier".into()));
        }
        session
            .document
            .table_mut()
            .sync_additions()
            .map_err(|error| EditorError::io(&error))?;
        let source_fingerprint = fingerprint(original)
            .map_err(|error| EditorError::io(&error))?
            .ok_or_else(|| EditorError::Recovery("recovery source is missing".into()))?;
        if session.source_fingerprint.as_ref() != Some(&source_fingerprint) {
            return Err(EditorError::Recovery("the original file changed".into()));
        }
        let record = EditorRecoveryRecord {
            format_version: RECOVERY_FORMAT_VERSION,
            recovery_id: recovery_id.clone(),
            name: session.name.clone(),
            source_path: original.clone(),
            document_path: session.source_path.clone(),
            untitled: session.original_path.is_some(),
            source_fingerprint,
            additions_path: additions.clone(),
            clean_revision: session
                .document
                .is_clean()
                .then(|| session.document.table().revision()),
            encoding: session.encoding,
            language_id: session.language_id.as_str().to_owned(),
            table: session.document.table().snapshot(),
            unsaved_changes: session
                .changes
                .status()
                .available
                .then(|| session.changes.unsaved_ranges()),
        };
        let bytes = serde_json::to_vec(&record)
            .map_err(|error| EditorError::Recovery(error.to_string()))?;
        if bytes.len() as u64 > MAX_RECOVERY_MANIFEST_BYTES {
            return Err(EditorError::Recovery(
                "recovery manifest is too large".into(),
            ));
        }
        let path = recovery_manifest_path(&self.session_root, &recovery_id);
        fs::create_dir_all(self.session_root.join("recovery"))
            .map_err(|error| EditorError::io(&error))?;
        let expected = fingerprint(&path).map_err(|error| EditorError::io(&error))?;
        save_atomic(&path, expected.as_ref(), |file| {
            file.write_all(&bytes)?;
            u64::try_from(bytes.len()).map_err(std::io::Error::other)
        })
        .map_err(|error| EditorError::Recovery(error.to_string()))?;
        session.recovery_id = Some(recovery_id);
        Ok(())
    }

    /// Lists valid recovery checkpoints from previous sessions, including
    /// checkpoints whose original source changed.
    ///
    /// # Errors
    /// Returns an error if the recovery directory cannot be read.
    pub fn recovery_candidates(&self) -> Result<Vec<RecoveryCandidate>, EditorError> {
        let directory = self.session_root.join("recovery");
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(EditorError::io(&error)),
        };
        let mut candidates = Vec::new();
        for entry in entries.take(MAX_RECOVERY_DIRECTORY_ENTRIES) {
            let entry = entry.map_err(|error| EditorError::io(&error))?;
            let Some(id) = entry
                .path()
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            if let Ok(record) = self.load_recovery_record(&id) {
                let source_changed = fingerprint(&record.source_path).ok().flatten()
                    != Some(record.source_fingerprint.clone());
                candidates.push(RecoveryCandidate {
                    recovery_id: id,
                    name: record.name,
                    source_path: record
                        .document_path
                        .map(|path| path.to_string_lossy().into_owned()),
                    source_changed,
                    byte_len: record.table.document_len,
                });
                if candidates.len() == MAX_RECOVERY_CANDIDATES {
                    break;
                }
            }
        }
        candidates.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(candidates)
    }

    /// Restores one validated checkpoint into an editable document.
    ///
    /// # Errors
    /// Returns an error for an invalid checkpoint, changed source, or missing append store.
    pub fn restore_recovery(&self, recovery_id: &str) -> Result<DocumentSummary, EditorError> {
        let record = self.load_recovery_record(recovery_id)?;
        if fingerprint(&record.source_path).map_err(|error| EditorError::io(&error))?
            != Some(record.source_fingerprint.clone())
        {
            return Err(EditorError::Recovery("the original file changed".into()));
        }
        let source =
            FileSource::open(&record.source_path).map_err(|error| EditorError::io(&error))?;
        let additions = DiskAppendStore::open(&record.additions_path)
            .map_err(|error| EditorError::io(&error))?;
        let table = PieceTable::from_snapshot(
            std::sync::Arc::new(source),
            Box::new(additions),
            &record.table,
        )?;
        let language_id = LanguageId::new(record.language_id)?;
        // Checkpoints do not record whether the language was chosen by hand;
        // one that detection would not pick was.
        let language_override = language_id
            != self.languages().detect_path(
                record
                    .document_path
                    .as_deref()
                    .unwrap_or_else(|| Path::new(&record.name)),
            );
        let changes = record
            .unsaved_changes
            .as_deref()
            .map_or_else(ChangeTracker::unavailable, |ranges| {
                ChangeTracker::from_unsaved(ranges, table.len())
            });
        // Undo history is not recovered, so a recovered edit stays unsaved.
        let recovered_clean = record.clean_revision == Some(table.revision());
        let mut document = Document::new(table, DEFAULT_HISTORY_BUDGET);
        if !recovered_clean {
            document.mark_unsaved();
        }
        let number = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let id = DocumentId(format!("document-{number}"));
        let detected_indent = indentation::detect_document_unit(document.table(), record.encoding);
        let language_id_for_indent = language_id.clone();
        let capabilities = self.document_capabilities(&language_id);
        let session = EditorSession {
            name: record.name,
            source_path: record.document_path,
            source_fingerprint: if record.untitled {
                None
            } else {
                Some(record.source_fingerprint)
            },
            original_path: record.untitled.then_some(record.source_path),
            additions_path: Some(record.additions_path),
            recovery_id: Some(recovery_id.to_owned()),
            document,
            encoding: record.encoding,
            language_id,
            language_override,
            capabilities,
            detected_indent,
            indentation: self.document_indentation(&language_id_for_indent, detected_indent),
            snippet: None,
            selections: vec![Selection::caret(0)],
            rectangular_selection: None,
            preferred_scalar_columns: vec![None],
            bookmarks: BTreeSet::new(),
            bookmark_undo: VecDeque::new(),
            bookmark_redo: VecDeque::new(),
            changes,
            navigation_index: SparseNavigationIndex::default(),
        };
        let result = summary(&id, &session);
        self.sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?
            .insert(id, session);
        Ok(result)
    }

    /// Removes a recovery checkpoint after explicit user dismissal.
    ///
    /// # Errors
    /// Returns an error for an invalid identifier or filesystem failure.
    pub fn discard_recovery(&self, recovery_id: &str) -> Result<(), EditorError> {
        let record = self.load_recovery_record(recovery_id)?;
        remove_if_present(&recovery_manifest_path(&self.session_root, recovery_id))?;
        remove_if_present(&record.additions_path)?;
        if record.untitled {
            remove_if_present(&record.source_path)?;
        }
        Ok(())
    }

    fn load_recovery_record(&self, recovery_id: &str) -> Result<EditorRecoveryRecord, EditorError> {
        if !valid_recovery_id(recovery_id) {
            return Err(EditorError::Recovery("invalid recovery identifier".into()));
        }
        let path = recovery_manifest_path(&self.session_root, recovery_id);
        let metadata = fs::symlink_metadata(&path).map_err(|error| EditorError::io(&error))?;
        if !metadata.is_file() || metadata.len() > MAX_RECOVERY_MANIFEST_BYTES {
            return Err(EditorError::Recovery("invalid recovery manifest".into()));
        }
        let record: EditorRecoveryRecord =
            serde_json::from_slice(&fs::read(&path).map_err(|error| EditorError::io(&error))?)
                .map_err(|error| EditorError::Recovery(error.to_string()))?;
        if record.format_version != RECOVERY_FORMAT_VERSION || record.recovery_id != recovery_id {
            return Err(EditorError::Recovery(
                "unsupported recovery manifest".into(),
            ));
        }
        if record.untitled && record.document_path.is_some() {
            return Err(EditorError::Recovery(
                "invalid untitled recovery source".into(),
            ));
        }
        if !record.untitled && record.document_path.as_ref() != Some(&record.source_path) {
            return Err(EditorError::Recovery(
                "invalid document recovery source".into(),
            ));
        }
        let root = self
            .session_root
            .canonicalize()
            .map_err(|error| EditorError::io(&error))?;
        let additions = record
            .additions_path
            .canonicalize()
            .map_err(|error| EditorError::io(&error))?;
        if !additions.starts_with(&root) || !additions.is_file() {
            return Err(EditorError::Recovery("invalid recovery edit store".into()));
        }
        if record.untitled {
            let parent = record
                .source_path
                .parent()
                .ok_or_else(|| EditorError::Recovery("invalid untitled source".into()))?
                .canonicalize()
                .map_err(|error| EditorError::io(&error))?;
            if parent != root
                || record
                    .source_path
                    .extension()
                    .is_none_or(|extension| extension != "base")
            {
                return Err(EditorError::Recovery("invalid untitled source".into()));
            }
            match fs::symlink_metadata(&record.source_path) {
                Ok(metadata) if metadata.is_file() => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err(EditorError::Recovery("invalid untitled source".into())),
            }
        }
        Ok(record)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditorRecoveryRecord {
    format_version: u32,
    recovery_id: String,
    name: String,
    source_path: PathBuf,
    document_path: Option<PathBuf>,
    untitled: bool,
    source_fingerprint: FileFingerprint,
    additions_path: PathBuf,
    clean_revision: Option<u64>,
    encoding: TextEncoding,
    language_id: String,
    table: PieceTableSnapshot,
    /// Unsaved line change ranges; absent in older checkpoints and whenever
    /// the markers could not be computed.
    #[serde(default)]
    unsaved_changes: Option<Vec<(u64, u64)>>,
}

const RECOVERY_FORMAT_VERSION: u32 = 1;

const MAX_RECOVERY_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;

const MAX_RECOVERY_CANDIDATES: usize = 128;

const MAX_RECOVERY_DIRECTORY_ENTRIES: usize = 1024;

pub(super) fn recovery_manifest_path(session_root: &Path, recovery_id: &str) -> PathBuf {
    session_root
        .join("recovery")
        .join(format!("{recovery_id}.json"))
}

fn valid_recovery_id(id: &str) -> bool {
    id.starts_with("document-")
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}
