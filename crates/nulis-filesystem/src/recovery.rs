use crate::{fingerprint, save_atomic, FileFingerprint, SaveError};
use nulis_document::{PieceTable, PieceTableSnapshot};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{self, BufReader, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

const RECOVERY_FORMAT_VERSION: u32 = 1;

/// A durable recovery pointer. It contains no document bytes: original bytes
/// remain in the source and inserted bytes remain in the append-only store.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryManifest {
    pub format_version: u32,
    pub session_id: String,
    pub source_path: PathBuf,
    pub additions_path: PathBuf,
    pub source_fingerprint: Option<FileFingerprint>,
    pub table: PieceTableSnapshot,
}

impl RecoveryManifest {
    /// Checks whether the original file still has the identity recorded at the
    /// recovery checkpoint. Loading content should stop if this returns false.
    ///
    /// # Errors
    /// Returns an I/O error when the source metadata cannot be inspected.
    pub fn source_is_current(&self) -> io::Result<bool> {
        Ok(fingerprint(&self.source_path)? == self.source_fingerprint)
    }
}

/// Flushes inserted bytes first, then atomically commits a versioned recovery
/// manifest. The returned fingerprint is used as the expected value for the
/// next checkpoint, preventing competing session writers from clobbering it.
///
/// # Errors
/// Returns an error if append-store sync, serialization, or safe persistence fails.
#[allow(clippy::too_many_arguments)]
pub fn checkpoint_recovery(
    manifest_path: &Path,
    expected_manifest: Option<&FileFingerprint>,
    session_id: String,
    source_path: PathBuf,
    additions_path: PathBuf,
    source_fingerprint: Option<FileFingerprint>,
    table: &mut PieceTable,
) -> Result<(RecoveryManifest, FileFingerprint), RecoveryError> {
    table.sync_additions()?;
    let manifest = RecoveryManifest {
        format_version: RECOVERY_FORMAT_VERSION,
        session_id,
        source_path,
        additions_path,
        source_fingerprint,
        table: table.snapshot(),
    };
    let saved = save_atomic(manifest_path, expected_manifest, |file| {
        let mut writer = CountingWriter::new(file);
        serde_json::to_writer(&mut writer, &manifest).map_err(io::Error::other)?;
        writer.write_all(b"\n")?;
        Ok(writer.count)
    })?;
    Ok((manifest, saved))
}

/// Loads and version-checks a recovery manifest without opening any paths
/// named by it. Path trust and source identity are explicit caller decisions.
///
/// # Errors
/// Returns an error for I/O, malformed JSON, or an unsupported format version.
pub fn load_recovery_manifest(path: &Path) -> Result<RecoveryManifest, RecoveryError> {
    let manifest: RecoveryManifest = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    if manifest.format_version != RECOVERY_FORMAT_VERSION {
        return Err(RecoveryError::UnsupportedVersion(manifest.format_version));
    }
    Ok(manifest)
}

struct CountingWriter<'a> {
    inner: &'a mut File,
    count: u64,
}

impl<'a> CountingWriter<'a> {
    fn new(inner: &'a mut File) -> Self {
        Self { inner, count: 0 }
    }
}

impl Write for CountingWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.count = self
            .count
            .checked_add(u64::try_from(written).expect("usize always fits into u64"))
            .ok_or_else(|| io::Error::other("recovery manifest length overflow"))?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("unsupported recovery manifest version {0}")]
    UnsupportedVersion(u32),
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::{checkpoint_recovery, load_recovery_manifest};
    use crate::fingerprint;
    use nulis_document::PieceTable;
    use nulis_large_file::{DiskAppendStore, FileSource};
    use std::{fs, path::PathBuf, sync::Arc};

    fn fixture() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("nulis-recovery-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).expect("fixture directory");
        directory
    }

    #[test]
    fn durable_manifest_restores_a_disk_backed_edit_session() {
        let directory = fixture();
        let source_path = directory.join("source.txt");
        let additions_path = directory.join("session.add");
        let manifest_path = directory.join("session.json");
        fs::write(&source_path, b"alpha beta gamma").expect("source");
        let source_fingerprint = fingerprint(&source_path).expect("fingerprint");
        let original = Arc::new(FileSource::open(&source_path).expect("open source"));
        let additions = DiskAppendStore::create(&additions_path).expect("additions");
        let mut table = PieceTable::new(original, Box::new(additions));
        table.replace(6, 4, b"Nulis").expect("edit");

        let (_, manifest_fingerprint) = checkpoint_recovery(
            &manifest_path,
            None,
            "test-session".to_owned(),
            source_path.clone(),
            additions_path.clone(),
            source_fingerprint,
            &mut table,
        )
        .expect("checkpoint");
        assert!(manifest_fingerprint.len > 0);
        drop(table);

        let manifest = load_recovery_manifest(&manifest_path).expect("load manifest");
        assert!(manifest.source_is_current().expect("source identity"));
        let recovered = PieceTable::from_snapshot(
            Arc::new(FileSource::open(&manifest.source_path).expect("reopen source")),
            Box::new(DiskAppendStore::open(&manifest.additions_path).expect("reopen additions")),
            &manifest.table,
        )
        .expect("restore table");
        assert_eq!(
            recovered.read_vec(0, recovered.len()).expect("read"),
            b"alpha Nulis gamma"
        );
        // Windows cannot remove a directory whose files are still open.
        drop(recovered);
        fs::remove_dir_all(directory).expect("cleanup");
    }
}
