use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
use thiserror::Error;

pub const MAX_WORKSPACE_DIRECTORY_ENTRIES: usize = 1_000;
const MAX_WORKSPACE_PATH_BYTES: usize = 4_096;
/// Most files Quick Open lists from one workspace.
pub const MAX_WORKSPACE_FILE_LIST: usize = 20_000;
/// Most folders read while listing them.
const MAX_WORKSPACE_LIST_DIRECTORIES: usize = 5_000;
const MAX_WORKSPACE_LIST_DEPTH: usize = 32;
/// Folders Quick Open does not descend into, besides those starting with a dot.
const SKIPPED_DIRECTORIES: &[&str] = &["node_modules"];

/// Regular files below the workspace root, relative and sorted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileList {
    pub files: Vec<String>,
    /// More files or folders existed than were listed.
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    pub root: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceEntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntry {
    pub name: String,
    pub relative_path: String,
    pub kind: WorkspaceEntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDirectory {
    pub relative_path: String,
    pub entries: Vec<WorkspaceEntry>,
    pub truncated: bool,
}

#[derive(Debug, Default)]
pub struct WorkspaceService {
    root: Mutex<Option<PathBuf>>,
}

impl WorkspaceService {
    /// Selects one canonical directory as the current workspace.
    ///
    /// # Errors
    /// Returns an error when the path is too long, unavailable, or not a directory.
    pub fn open(&self, root: &Path) -> Result<WorkspaceSummary, WorkspaceError> {
        validate_path_size(root)?;
        let canonical = root.canonicalize().map_err(WorkspaceError::io)?;
        if !canonical.is_dir() {
            return Err(WorkspaceError::NotDirectory(root.display().to_string()));
        }
        let summary = workspace_summary(&canonical);
        *self
            .root
            .lock()
            .map_err(|_| WorkspaceError::StateUnavailable)? = Some(canonical);
        Ok(summary)
    }

    /// Clears the selected workspace root.
    ///
    /// # Errors
    /// Returns an error if workspace state is unavailable.
    pub fn close(&self) -> Result<(), WorkspaceError> {
        *self
            .root
            .lock()
            .map_err(|_| WorkspaceError::StateUnavailable)? = None;
        Ok(())
    }

    /// Returns the current workspace without exposing mutable state.
    ///
    /// # Errors
    /// Returns an error if workspace state is unavailable.
    pub fn current(&self) -> Result<Option<WorkspaceSummary>, WorkspaceError> {
        Ok(self
            .root
            .lock()
            .map_err(|_| WorkspaceError::StateUnavailable)?
            .as_deref()
            .map(workspace_summary))
    }

    /// Lists one directory while retaining at most `maximum + 1` entries in
    /// memory. Symlinks are reported but never followed.
    ///
    /// # Errors
    /// Returns an error for an unavailable workspace, unsafe relative path,
    /// invalid limit, symlink traversal, or filesystem failure.
    pub fn list(
        &self,
        relative_path: &str,
        maximum: usize,
    ) -> Result<WorkspaceDirectory, WorkspaceError> {
        if !(1..=MAX_WORKSPACE_DIRECTORY_ENTRIES).contains(&maximum) {
            return Err(WorkspaceError::EntryLimit(maximum));
        }
        let (root, directory) = self.resolve(relative_path)?;
        let metadata = fs::symlink_metadata(&directory).map_err(WorkspaceError::io)?;
        if metadata.file_type().is_symlink() {
            return Err(WorkspaceError::SymlinkTraversal(relative_path.into()));
        }
        if !metadata.is_dir() {
            return Err(WorkspaceError::NotDirectory(relative_path.into()));
        }

        let mut retained = BTreeMap::new();
        let mut total = 0_usize;
        for entry in fs::read_dir(&directory).map_err(WorkspaceError::io)? {
            let entry = entry.map_err(WorkspaceError::io)?;
            total = total.saturating_add(1);
            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = fs::symlink_metadata(entry.path()).map_err(WorkspaceError::io)?;
            let kind = if metadata.file_type().is_symlink() {
                WorkspaceEntryKind::Symlink
            } else if metadata.is_dir() {
                WorkspaceEntryKind::Directory
            } else if metadata.is_file() {
                WorkspaceEntryKind::File
            } else {
                WorkspaceEntryKind::Other
            };
            let rank = match kind {
                WorkspaceEntryKind::Directory => 0_u8,
                WorkspaceEntryKind::File => 1,
                WorkspaceEntryKind::Symlink => 2,
                WorkspaceEntryKind::Other => 3,
            };
            let relative = entry
                .path()
                .strip_prefix(&root)
                .map_err(|_| WorkspaceError::OutsideRoot(relative_path.into()))?
                .to_string_lossy()
                .into_owned();
            retained.insert(
                (rank, name.to_lowercase(), name.clone()),
                WorkspaceEntry {
                    name,
                    relative_path: relative,
                    kind,
                    byte_len: metadata.is_file().then(|| metadata.len().to_string()),
                },
            );
            if retained.len() > maximum {
                retained.pop_last();
            }
        }
        Ok(WorkspaceDirectory {
            relative_path: normalize_relative(relative_path)?
                .to_string_lossy()
                .into_owned(),
            entries: retained.into_values().collect(),
            truncated: total > maximum,
        })
    }

    /// Lists regular files below the workspace root, breadth first, for
    /// Quick Open. Symlinks, folders starting with a dot, and `node_modules`
    /// are skipped; unreadable folders are skipped silently.
    ///
    /// # Errors
    /// Returns an error when no workspace is open or the root cannot be read.
    pub fn list_files(&self) -> Result<WorkspaceFileList, WorkspaceError> {
        let root = self
            .root
            .lock()
            .map_err(|_| WorkspaceError::StateUnavailable)?
            .clone()
            .ok_or(WorkspaceError::NoWorkspace)?;
        fs::read_dir(&root).map_err(WorkspaceError::io)?;
        let mut files = Vec::new();
        let mut truncated = false;
        let mut queue = std::collections::VecDeque::from([(root.clone(), 0_usize)]);
        let mut visited = 0_usize;
        'walk: while let Some((directory, depth)) = queue.pop_front() {
            visited += 1;
            if visited > MAX_WORKSPACE_LIST_DIRECTORIES {
                truncated = true;
                break;
            }
            let Ok(entries) = fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if kind.is_dir() {
                    if name.starts_with('.') || SKIPPED_DIRECTORIES.contains(&name.as_ref()) {
                        continue;
                    }
                    if depth + 1 >= MAX_WORKSPACE_LIST_DEPTH {
                        truncated = true;
                    } else {
                        queue.push_back((entry.path(), depth + 1));
                    }
                } else if kind.is_file() {
                    if files.len() == MAX_WORKSPACE_FILE_LIST {
                        truncated = true;
                        break 'walk;
                    }
                    if let Ok(relative) = entry.path().strip_prefix(&root) {
                        files.push(relative.to_string_lossy().into_owned());
                    }
                }
            }
        }
        files.sort_by_key(|path| path.to_lowercase());
        Ok(WorkspaceFileList { files, truncated })
    }

    /// Resolves a regular file below the current workspace without following
    /// a final symlink.
    ///
    /// # Errors
    /// Returns an error for unsafe paths, missing workspaces, symlinks, or
    /// non-regular files.
    pub fn resolve_file(&self, relative_path: &str) -> Result<PathBuf, WorkspaceError> {
        let (_, path) = self.resolve(relative_path)?;
        let metadata = fs::symlink_metadata(&path).map_err(WorkspaceError::io)?;
        if metadata.file_type().is_symlink() {
            return Err(WorkspaceError::SymlinkTraversal(relative_path.into()));
        }
        if !metadata.is_file() {
            return Err(WorkspaceError::NotFile(relative_path.into()));
        }
        Ok(path)
    }

    fn resolve(&self, relative_path: &str) -> Result<(PathBuf, PathBuf), WorkspaceError> {
        let root = self
            .root
            .lock()
            .map_err(|_| WorkspaceError::StateUnavailable)?
            .clone()
            .ok_or(WorkspaceError::NoWorkspace)?;
        let relative = normalize_relative(relative_path)?;
        let mut candidate = root.clone();
        for component in relative.components() {
            candidate.push(component.as_os_str());
            let metadata = fs::symlink_metadata(&candidate).map_err(WorkspaceError::io)?;
            if metadata.file_type().is_symlink() {
                return Err(WorkspaceError::SymlinkTraversal(relative_path.into()));
            }
        }
        if !candidate.starts_with(&root) {
            return Err(WorkspaceError::OutsideRoot(relative_path.into()));
        }
        Ok((root, candidate))
    }
}

fn workspace_summary(root: &Path) -> WorkspaceSummary {
    WorkspaceSummary {
        root: root.to_string_lossy().into_owned(),
        name: root.file_name().map_or_else(
            || root.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
    }
}

fn normalize_relative(path: &str) -> Result<PathBuf, WorkspaceError> {
    if path.len() > MAX_WORKSPACE_PATH_BYTES {
        return Err(WorkspaceError::PathTooLong(path.len()));
    }
    let mut normalized = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(value) => normalized.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(WorkspaceError::UnsafePath(path.into()));
            }
        }
    }
    Ok(normalized)
}

fn validate_path_size(path: &Path) -> Result<(), WorkspaceError> {
    let length = path.as_os_str().to_string_lossy().len();
    if length > MAX_WORKSPACE_PATH_BYTES {
        Err(WorkspaceError::PathTooLong(length))
    } else {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("no workspace is open")]
    NoWorkspace,
    #[error("workspace state is unavailable")]
    StateUnavailable,
    #[error("workspace entry limit must be in 1..={MAX_WORKSPACE_DIRECTORY_ENTRIES}, got {0}")]
    EntryLimit(usize),
    #[error("workspace path has {0} bytes and exceeds the limit")]
    PathTooLong(usize),
    #[error("unsafe workspace-relative path: {0}")]
    UnsafePath(String),
    #[error("path escapes the workspace root: {0}")]
    OutsideRoot(String),
    #[error("workspace symlinks cannot be traversed: {0}")]
    SymlinkTraversal(String),
    #[error("path is not a directory: {0}")]
    NotDirectory(String),
    #[error("path is not a regular file: {0}")]
    NotFile(String),
    #[error("workspace filesystem operation failed: {0}")]
    Io(String),
}

impl WorkspaceError {
    // `Result::map_err` supplies the owned I/O error to this adapter.
    #[allow(clippy::needless_pass_by_value)]
    fn io(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkspaceEntryKind, WorkspaceService};
    use std::fs;

    #[test]
    fn listing_is_sorted_bounded_and_rejects_traversal() {
        let root = tempfile::tempdir().expect("workspace");
        fs::create_dir(root.path().join("folder")).expect("folder");
        fs::write(root.path().join("z.txt"), "z").expect("z");
        fs::write(root.path().join("a.txt"), "abc").expect("a");
        let service = WorkspaceService::default();
        service.open(root.path()).expect("open workspace");
        let listing = service.list("", 2).expect("listing");
        assert!(listing.truncated);
        assert_eq!(listing.entries.len(), 2);
        assert_eq!(listing.entries[0].kind, WorkspaceEntryKind::Directory);
        assert_eq!(listing.entries[0].name, "folder");
        assert_eq!(listing.entries[1].name, "a.txt");
        assert!(service.list("../outside", 10).is_err());
    }

    #[test]
    fn file_list_walks_folders_and_skips_dot_folders_and_node_modules() {
        let root = tempfile::tempdir().expect("workspace");
        for folder in ["src/deep", ".git", "node_modules/pkg"] {
            fs::create_dir_all(root.path().join(folder)).expect("folder");
        }
        for file in [
            "README.md",
            ".env",
            "src/main.rs",
            "src/deep/Util.rs",
            ".git/HEAD",
            "node_modules/pkg/index.js",
        ] {
            fs::write(root.path().join(file), "x").expect("file");
        }
        let service = WorkspaceService::default();
        assert!(service.list_files().is_err());
        service.open(root.path()).expect("open workspace");
        let listing = service.list_files().expect("files");
        let separator = std::path::MAIN_SEPARATOR;
        assert_eq!(
            listing.files,
            vec![
                ".env".to_owned(),
                "README.md".to_owned(),
                format!("src{separator}deep{separator}Util.rs"),
                format!("src{separator}main.rs"),
            ]
        );
        assert!(!listing.truncated);
    }

    #[test]
    fn files_resolve_only_below_the_workspace_root() {
        let root = tempfile::tempdir().expect("workspace");
        fs::create_dir(root.path().join("nested")).expect("nested");
        fs::write(root.path().join("nested/data.bin"), [1, 2, 3]).expect("data");
        let service = WorkspaceService::default();
        service.open(root.path()).expect("open workspace");
        assert_eq!(
            service.resolve_file("nested/data.bin").expect("file"),
            root.path()
                .canonicalize()
                .expect("canonical root")
                .join("nested/data.bin")
        );
        assert!(service.resolve_file("nested").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_entries_are_visible_but_never_traversed() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().expect("workspace");
        let outside = tempfile::tempdir().expect("outside");
        fs::create_dir(outside.path().join("nested")).expect("nested");
        fs::write(outside.path().join("nested/secret"), "nope").expect("secret");
        symlink(outside.path(), root.path().join("linked")).expect("symlink");
        let service = WorkspaceService::default();
        service.open(root.path()).expect("open workspace");
        let listing = service.list("", 10).expect("listing");
        assert_eq!(listing.entries[0].kind, WorkspaceEntryKind::Symlink);
        assert!(service.list("linked", 10).is_err());
        assert!(service.list("linked/nested", 10).is_err());
        assert!(service.resolve_file("linked/nested/secret").is_err());
    }
}
