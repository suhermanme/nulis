use nulis_large_file::{FileSource, ReadAt};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const MAX_FOLDER_COMPARE_ENTRIES: usize = 10_000;
pub const MAX_FOLDER_COMPARE_RESULTS: usize = 5_000;
const COMPARE_CHUNK_BYTES: usize = 64 * 1024;
const MAX_ROOT_PATH_BYTES: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FolderEntryKind {
    Directory,
    File,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FolderCompareStatus {
    LeftOnly,
    RightOnly,
    Different,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderCompareEntry {
    pub relative_path: String,
    pub status: FolderCompareStatus,
    pub left_kind: Option<FolderEntryKind>,
    pub right_kind: Option<FolderEntryKind>,
    pub left_byte_len: Option<String>,
    pub right_byte_len: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderCompareResult {
    pub left_root: String,
    pub right_root: String,
    pub entries: Vec<FolderCompareEntry>,
    pub identical_files: usize,
    pub compared_files: usize,
    pub skipped_symlinks: usize,
    pub truncated: bool,
}

#[derive(Debug, Error)]
pub enum FolderCompareError {
    #[error("folder comparison result limit must be in 1..={MAX_FOLDER_COMPARE_RESULTS}, got {0}")]
    ResultLimit(usize),
    #[error("folder comparison root path exceeds {MAX_ROOT_PATH_BYTES} bytes")]
    PathTooLong,
    #[error("folder comparison root is not a directory: {0}")]
    NotDirectory(String),
    #[error("folder comparison failed: {0}")]
    Io(String),
}

#[derive(Clone, Debug)]
struct EntryMetadata {
    path: PathBuf,
    kind: FolderEntryKind,
    byte_len: Option<u64>,
}

#[derive(Debug)]
struct CollectedTree {
    entries: BTreeMap<PathBuf, EntryMetadata>,
    skipped_symlinks: usize,
    truncated: bool,
}

/// Recursively compares two folder trees without traversing symlinks or
/// materializing file contents. File equality is checked in 64 KiB chunks.
/// Only differences are retained in the result.
///
/// # Errors
/// Returns an error for invalid roots/limits or filesystem failures.
pub fn compare_folders(
    left_root: &Path,
    right_root: &Path,
    max_results: usize,
) -> Result<FolderCompareResult, FolderCompareError> {
    if !(1..=MAX_FOLDER_COMPARE_RESULTS).contains(&max_results) {
        return Err(FolderCompareError::ResultLimit(max_results));
    }
    let left_root = canonical_directory(left_root)?;
    let right_root = canonical_directory(right_root)?;
    let left = collect_tree(&left_root)?;
    let right = collect_tree(&right_root)?;
    let paths = left
        .entries
        .keys()
        .chain(right.entries.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut entries = Vec::new();
    let mut identical_files = 0_usize;
    let mut compared_files = 0_usize;
    let mut truncated = left.truncated || right.truncated;
    for relative in paths {
        let left_entry = left.entries.get(&relative);
        let right_entry = right.entries.get(&relative);
        let status = match (left_entry, right_entry) {
            (Some(_), None) => Some(FolderCompareStatus::LeftOnly),
            (None, Some(_)) => Some(FolderCompareStatus::RightOnly),
            (Some(left_entry), Some(right_entry)) if left_entry.kind != right_entry.kind => {
                Some(FolderCompareStatus::Different)
            }
            (Some(left_entry), Some(right_entry))
                if left_entry.kind == FolderEntryKind::Directory
                    && right_entry.kind == FolderEntryKind::Directory =>
            {
                None
            }
            (Some(left_entry), Some(right_entry)) => {
                compared_files = compared_files.saturating_add(1);
                if files_equal(left_entry, right_entry)? {
                    identical_files = identical_files.saturating_add(1);
                    None
                } else {
                    Some(FolderCompareStatus::Different)
                }
            }
            (None, None) => unreachable!(),
        };
        let Some(status) = status else { continue };
        if entries.len() == max_results {
            truncated = true;
            break;
        }
        entries.push(FolderCompareEntry {
            relative_path: relative.to_string_lossy().replace('\\', "/"),
            status,
            left_kind: left_entry.map(|entry| entry.kind),
            right_kind: right_entry.map(|entry| entry.kind),
            left_byte_len: left_entry
                .and_then(|entry| entry.byte_len)
                .map(|len| len.to_string()),
            right_byte_len: right_entry
                .and_then(|entry| entry.byte_len)
                .map(|len| len.to_string()),
        });
    }
    Ok(FolderCompareResult {
        left_root: left_root.to_string_lossy().into_owned(),
        right_root: right_root.to_string_lossy().into_owned(),
        entries,
        identical_files,
        compared_files,
        skipped_symlinks: left.skipped_symlinks.saturating_add(right.skipped_symlinks),
        truncated,
    })
}

fn canonical_directory(path: &Path) -> Result<PathBuf, FolderCompareError> {
    if path.as_os_str().to_string_lossy().len() > MAX_ROOT_PATH_BYTES {
        return Err(FolderCompareError::PathTooLong);
    }
    let canonical = path.canonicalize().map_err(io_error)?;
    if canonical.is_dir() {
        Ok(canonical)
    } else {
        Err(FolderCompareError::NotDirectory(path.display().to_string()))
    }
}

fn collect_tree(root: &Path) -> Result<CollectedTree, FolderCompareError> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    let mut skipped_symlinks = 0_usize;
    let mut truncated = false;
    while let Some(directory) = pending.pop() {
        let mut children = fs::read_dir(&directory)
            .map_err(io_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(io_error)?;
        children.sort_by_key(fs::DirEntry::file_name);
        for child in children {
            let path = child.path();
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                skipped_symlinks = skipped_symlinks.saturating_add(1);
                continue;
            }
            let kind = if metadata.is_dir() {
                FolderEntryKind::Directory
            } else if metadata.is_file() {
                FolderEntryKind::File
            } else {
                continue;
            };
            if entries.len() == MAX_FOLDER_COMPARE_ENTRIES {
                truncated = true;
                pending.clear();
                break;
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|error| FolderCompareError::Io(error.to_string()))?
                .to_path_buf();
            entries.insert(
                relative,
                EntryMetadata {
                    path: path.clone(),
                    kind,
                    byte_len: metadata.is_file().then_some(metadata.len()),
                },
            );
            if metadata.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(CollectedTree {
        entries,
        skipped_symlinks,
        truncated,
    })
}

fn files_equal(left: &EntryMetadata, right: &EntryMetadata) -> Result<bool, FolderCompareError> {
    if left.byte_len != right.byte_len {
        return Ok(false);
    }
    let length = left.byte_len.unwrap_or(0);
    let left_source = FileSource::open(&left.path).map_err(io_error)?;
    let right_source = FileSource::open(&right.path).map_err(io_error)?;
    let mut offset = 0_u64;
    let mut left_bytes = vec![0; COMPARE_CHUNK_BYTES];
    let mut right_bytes = vec![0; COMPARE_CHUNK_BYTES];
    while offset < length {
        let count = usize::try_from((length - offset).min(COMPARE_CHUNK_BYTES as u64))
            .map_err(|error| FolderCompareError::Io(error.to_string()))?;
        left_source
            .read_exact_at(offset, &mut left_bytes[..count])
            .map_err(io_error)?;
        right_source
            .read_exact_at(offset, &mut right_bytes[..count])
            .map_err(io_error)?;
        if left_bytes[..count] != right_bytes[..count] {
            return Ok(false);
        }
        offset = offset.saturating_add(count as u64);
    }
    Ok(true)
}

#[allow(clippy::needless_pass_by_value)] // `Result::map_err` supplies an owned I/O error.
fn io_error(error: std::io::Error) -> FolderCompareError {
    FolderCompareError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{compare_folders, FolderCompareStatus};
    use std::fs;

    #[test]
    fn comparison_is_recursive_deterministic_and_content_aware() {
        let left = tempfile::tempdir().expect("left");
        let right = tempfile::tempdir().expect("right");
        fs::create_dir(left.path().join("nested")).expect("left nested");
        fs::create_dir(right.path().join("nested")).expect("right nested");
        fs::write(left.path().join("same.bin"), [0, 1, 2]).expect("left same");
        fs::write(right.path().join("same.bin"), [0, 1, 2]).expect("right same");
        fs::write(left.path().join("nested/change.txt"), "before").expect("left change");
        fs::write(right.path().join("nested/change.txt"), "after!").expect("right change");
        fs::write(left.path().join("left.txt"), "left").expect("left only");
        fs::write(right.path().join("right.txt"), "right").expect("right only");

        let result = compare_folders(left.path(), right.path(), 10).expect("compare");
        assert_eq!(result.identical_files, 1);
        assert_eq!(result.compared_files, 2);
        assert_eq!(result.entries.len(), 3);
        assert_eq!(result.entries[0].relative_path, "left.txt");
        assert_eq!(result.entries[0].status, FolderCompareStatus::LeftOnly);
        assert_eq!(result.entries[1].relative_path, "nested/change.txt");
        assert_eq!(result.entries[1].status, FolderCompareStatus::Different);
        assert_eq!(result.entries[2].status, FolderCompareStatus::RightOnly);
    }

    #[test]
    fn result_count_is_bounded() {
        let left = tempfile::tempdir().expect("left");
        let right = tempfile::tempdir().expect("right");
        for index in 0..4 {
            fs::write(left.path().join(format!("{index}.txt")), "left").expect("file");
        }
        let result = compare_folders(left.path(), right.path(), 2).expect("compare");
        assert_eq!(result.entries.len(), 2);
        assert!(result.truncated);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_counted_but_never_followed() {
        use std::os::unix::fs::symlink;

        let left = tempfile::tempdir().expect("left");
        let right = tempfile::tempdir().expect("right");
        let outside = tempfile::tempdir().expect("outside");
        fs::write(outside.path().join("secret.txt"), "secret").expect("outside file");
        symlink(outside.path(), left.path().join("linked")).expect("symlink");
        let result = compare_folders(left.path(), right.path(), 10).expect("compare");
        assert_eq!(result.skipped_symlinks, 1);
        assert_eq!(result.entries, Vec::new());
    }
}
