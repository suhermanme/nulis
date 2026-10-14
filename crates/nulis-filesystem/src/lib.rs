//! Filesystem provider boundary and safe-save policy types.

mod compare;
mod places;
mod recovery;
mod save;
mod session;
mod workspace;

pub use compare::{
    compare_folders, FolderCompareEntry, FolderCompareError, FolderCompareResult,
    FolderCompareStatus, FolderEntryKind, MAX_FOLDER_COMPARE_ENTRIES, MAX_FOLDER_COMPARE_RESULTS,
};
pub use places::{list_place, places, Place, PlaceDirectory, PlaceEntry, PlaceKind};
pub use recovery::{checkpoint_recovery, load_recovery_manifest, RecoveryError, RecoveryManifest};
pub use save::{fingerprint, save_atomic, FileFingerprint, SaveError};
pub use session::{
    LayoutGroup, LayoutNode, SessionError, SessionStore, SplitDirection, WorkbenchSession,
    DEFAULT_WORKSPACE_NAME, MAX_LAYOUT_DEPTH, MAX_LAYOUT_GROUPS, MAX_LAYOUT_PANES,
    MAX_WORKSPACE_NAME_CHARS, SESSION_SCHEMA_VERSION,
};
pub use workspace::{
    WorkspaceDirectory, WorkspaceEntry, WorkspaceEntryKind, WorkspaceError, WorkspaceService,
    WorkspaceSummary, MAX_WORKSPACE_DIRECTORY_ENTRIES,
};

use std::{io, path::Path};

pub trait FileSystemProvider: Send + Sync {
    /// Returns the current byte length of a resource.
    ///
    /// # Errors
    /// Returns an I/O error when metadata is unavailable or access is denied.
    fn metadata_len(&self, path: &Path) -> io::Result<u64>;

    /// Reads a bounded byte range without materializing the file.
    ///
    /// # Errors
    /// Returns an I/O error when the resource cannot be opened or read.
    fn read_range(&self, path: &Path, offset: u64, buffer: &mut [u8]) -> io::Result<usize>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConcurrentChangePolicy {
    Reject,
    ConfirmBeforeReplace,
}
