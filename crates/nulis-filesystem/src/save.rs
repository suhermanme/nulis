use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, Metadata},
    io::{self, Write},
    path::Path,
    time::SystemTime,
};
use tempfile::{Builder, NamedTempFile};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFingerprint {
    pub len: u64,
    pub modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl FileFingerprint {
    fn from_metadata(metadata: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        }
    }
}

/// Captures cheap identity/change metadata without reading file contents.
///
/// # Errors
/// Returns an I/O error other than a missing path.
pub fn fingerprint(path: &Path) -> io::Result<Option<FileFingerprint>> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(FileFingerprint::from_metadata(&metadata))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Writes a same-directory temporary file, validates and syncs it, checks that
/// the destination has not changed, and atomically replaces it where supported.
///
/// # Errors
/// Returns without replacing the destination if writing, validation, syncing,
/// concurrent-change detection, or platform replacement fails.
pub fn save_atomic<F>(
    destination: &Path,
    expected: Option<&FileFingerprint>,
    writer: F,
) -> Result<FileFingerprint, SaveError>
where
    F: FnOnce(&mut File) -> io::Result<u64>,
{
    let before = fingerprint(destination)?;
    if before.as_ref() != expected {
        return Err(SaveError::ConcurrentModification);
    }
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let destination_name = destination
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("document");
    let prefix = format!(".{destination_name}.nulis-save-");
    let mut temporary = Builder::new()
        .prefix(&prefix)
        .suffix(".tmp")
        .tempfile_in(parent)?;
    if before.is_some() {
        temporary
            .as_file()
            .set_permissions(fs::metadata(destination)?.permissions())?;
    }

    let expected_len = writer(temporary.as_file_mut())?;
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    let actual_len = temporary.as_file().metadata()?.len();
    if actual_len != expected_len {
        return Err(SaveError::LengthMismatch {
            expected: expected_len,
            actual: actual_len,
        });
    }
    if fingerprint(destination)?.as_ref() != expected {
        return Err(SaveError::ConcurrentModification);
    }

    let persisted = if before.is_some() {
        replace_destination(temporary, destination)?
    } else {
        temporary
            .persist_noclobber(destination)
            .map_err(|error| SaveError::Io(error.error))?
    };
    persisted.sync_all()?;
    sync_directory(parent)?;
    fingerprint(destination)?.ok_or(SaveError::ReplacementMissing)
}

/// Atomically replaces `destination` with the finished temporary file.
#[cfg(unix)]
fn replace_destination(temporary: NamedTempFile, destination: &Path) -> io::Result<File> {
    temporary.persist(destination).map_err(|error| error.error)
}

/// Atomically replaces `destination` with the finished temporary file.
///
/// The editor keeps the file it is saving open for reading, and on Windows a
/// plain `MoveFileExW` replacement of an open file is refused with "access is
/// denied". `std::fs::rename` asks for POSIX rename semantics, which replace
/// the name while existing readers keep the old content until they close.
#[cfg(windows)]
fn replace_destination(temporary: NamedTempFile, destination: &Path) -> io::Result<File> {
    let (file, path) = temporary.keep().map_err(|error| error.error)?;
    match fs::rename(&path, destination) {
        Ok(()) => Ok(file),
        Err(error) => {
            let _ = fs::remove_file(&path);
            Err(error)
        }
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
// The signature matches the Unix implementation, which can fail.
#[allow(clippy::unnecessary_wraps)]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("the destination changed since it was opened")]
    ConcurrentModification,
    #[error("writer reported {expected} bytes but temporary output contains {actual}")]
    LengthMismatch { expected: u64, actual: u64 },
    #[error("destination is missing after replacement")]
    ReplacementMissing,
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::{fingerprint, save_atomic, SaveError};
    use nulis_document::PieceTable;
    use std::{
        fs, io,
        path::{Path, PathBuf},
    };

    fn fixture(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("nulis-save-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).expect("fixture directory");
        directory
    }

    fn assert_no_temporary_files(directory: &Path) {
        let leftovers = fs::read_dir(directory)
            .expect("read directory")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".nulis-save-"))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn interrupted_write_preserves_original_and_cleans_temporary_file() {
        let directory = fixture("interrupted");
        let path = directory.join("data.txt");
        fs::write(&path, b"original").expect("original");
        let expected = fingerprint(&path).expect("fingerprint").expect("exists");
        let result = save_atomic(&path, Some(&expected), |file| {
            use std::io::Write as _;
            file.write_all(b"partial")?;
            Err(io::Error::other("simulated interruption"))
        });
        assert!(matches!(result, Err(SaveError::Io(_))));
        assert_eq!(fs::read(&path).expect("read"), b"original");
        assert_no_temporary_files(&directory);
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn concurrent_change_is_rejected_without_overwrite() {
        let directory = fixture("concurrent");
        let path = directory.join("data.txt");
        fs::write(&path, b"first").expect("original");
        let expected = fingerprint(&path).expect("fingerprint").expect("exists");
        fs::write(&path, b"external change with another length").expect("external edit");
        let result = save_atomic(&path, Some(&expected), |file| {
            use std::io::Write as _;
            file.write_all(b"ours")?;
            Ok(4)
        });
        assert!(matches!(result, Err(SaveError::ConcurrentModification)));
        assert_eq!(
            fs::read(&path).expect("read"),
            b"external change with another length"
        );
        assert_no_temporary_files(&directory);
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn successful_save_replaces_complete_output() {
        let directory = fixture("success");
        let path = directory.join("data.txt");
        fs::write(&path, b"old").expect("original");
        let expected = fingerprint(&path).expect("fingerprint").expect("exists");
        let saved = save_atomic(&path, Some(&expected), |file| {
            use std::io::Write as _;
            file.write_all(b"new contents")?;
            Ok(12)
        })
        .expect("save");
        assert_eq!(saved.len, 12);
        assert_eq!(fs::read(&path).expect("read"), b"new contents");
        assert_no_temporary_files(&directory);
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn piece_table_streams_directly_into_safe_save() {
        let directory = fixture("piece-table");
        let path = directory.join("data.txt");
        fs::write(&path, b"0123456789").expect("original");
        let expected = fingerprint(&path).expect("fingerprint").expect("exists");
        let mut table = PieceTable::from_bytes(b"0123456789".to_vec());
        table.replace(3, 4, b"Nulis").expect("edit");
        save_atomic(&path, Some(&expected), |file| table.write_to(file)).expect("save");
        assert_eq!(fs::read(&path).expect("read"), b"012Nulis789");
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn new_file_save_does_not_require_a_preexisting_destination() {
        let directory = fixture("new-file");
        let path = directory.join("data.txt");
        let saved = save_atomic(&path, None, |file| {
            use std::io::Write as _;
            file.write_all(b"created")?;
            Ok(7)
        })
        .expect("save");
        assert_eq!(saved.len, 7);
        assert_eq!(fs::read(&path).expect("read"), b"created");
        assert_no_temporary_files(&directory);
        fs::remove_dir_all(directory).expect("cleanup");
    }
}
