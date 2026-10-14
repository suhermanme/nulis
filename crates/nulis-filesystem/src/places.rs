//! Whole-machine browsing for the Places explorer: the computer root, mounted
//! volumes, and the home directory, listed one bounded directory at a time.

use crate::{WorkspaceEntryKind, WorkspaceError, MAX_WORKSPACE_DIRECTORY_ENTRIES};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MAX_PLACE_PATH_BYTES: usize = 4_096;
const MAX_VOLUMES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaceKind {
    /// The root of the boot volume.
    Computer,
    Volume,
    Home,
}

/// A starting point for browsing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub path: String,
    pub kind: PlaceKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceEntry {
    pub name: String,
    pub path: String,
    /// A symlink is reported as what it points to; a broken one is `other`.
    pub kind: WorkspaceEntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceDirectory {
    pub path: String,
    pub entries: Vec<PlaceEntry>,
    pub truncated: bool,
}

fn directories_in(parent: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .take(MAX_VOLUMES)
        .collect();
    found.sort();
    found
}

fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Lists the computer root, other mounted volumes, and the home directory.
#[must_use]
pub fn places() -> Vec<Place> {
    let place = |name: String, path: &Path, kind| Place {
        name,
        path: path.to_string_lossy().into_owned(),
        kind,
    };
    let mut result = Vec::new();
    if cfg!(windows) {
        for letter in b'A'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", char::from(letter)));
            if root.is_dir() {
                let kind = if result.is_empty() {
                    PlaceKind::Computer
                } else {
                    PlaceKind::Volume
                };
                result.push(place(format!("{}:", char::from(letter)), &root, kind));
            }
        }
    } else {
        let root = Path::new("/");
        // macOS lists every volume under /Volumes; the boot volume is the one
        // that resolves to the root, and it lends the root its name.
        let mut volumes = directories_in("/Volumes");
        let user = std::env::var("USER").unwrap_or_default();
        if !user.is_empty() {
            volumes.extend(directories_in(&format!("/media/{user}")));
            volumes.extend(directories_in(&format!("/run/media/{user}")));
        }
        volumes.extend(directories_in("/mnt"));
        let boot = volumes
            .iter()
            .position(|volume| volume.canonicalize().is_ok_and(|target| target == root));
        let name = boot.map_or_else(
            || "Computer".to_owned(),
            |index| display_name(&volumes.remove(index)),
        );
        result.push(place(name, root, PlaceKind::Computer));
        for volume in volumes.iter().take(MAX_VOLUMES) {
            result.push(place(display_name(volume), volume, PlaceKind::Volume));
        }
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let Some(home) = home.map(PathBuf::from).filter(|path| path.is_dir()) {
        result.push(place(display_name(&home), &home, PlaceKind::Home));
    }
    result
}

/// Lists one directory anywhere on the machine, directories first, retaining
/// at most `maximum` entries. Entries that cannot be read are skipped.
///
/// # Errors
/// Returns an error for a relative or oversized path, an invalid limit, a
/// path that is not a directory, or a directory that cannot be read.
pub fn list_place(
    path: &Path,
    maximum: usize,
    show_hidden: bool,
) -> Result<PlaceDirectory, WorkspaceError> {
    if !(1..=MAX_WORKSPACE_DIRECTORY_ENTRIES).contains(&maximum) {
        return Err(WorkspaceError::EntryLimit(maximum));
    }
    let text = path.to_string_lossy();
    if text.len() > MAX_PLACE_PATH_BYTES {
        return Err(WorkspaceError::PathTooLong(text.len()));
    }
    if !path.is_absolute() {
        return Err(WorkspaceError::UnsafePath(text.into_owned()));
    }
    let io = |error: std::io::Error| WorkspaceError::Io(error.to_string());
    if !fs::metadata(path).map_err(io)?.is_dir() {
        return Err(WorkspaceError::NotDirectory(text.into_owned()));
    }

    let mut retained = BTreeMap::new();
    let mut total = 0_usize;
    for entry in fs::read_dir(path).map_err(io)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        total = total.saturating_add(1);
        // Following the link shows `/etc` or `/tmp` as the folders they are.
        let metadata = fs::metadata(entry.path()).ok();
        let kind = match &metadata {
            Some(metadata) if metadata.is_dir() => WorkspaceEntryKind::Directory,
            Some(metadata) if metadata.is_file() => WorkspaceEntryKind::File,
            _ => WorkspaceEntryKind::Other,
        };
        let rank = u8::from(kind != WorkspaceEntryKind::Directory);
        retained.insert(
            (rank, name.to_lowercase(), name.clone()),
            PlaceEntry {
                path: entry.path().to_string_lossy().into_owned(),
                name,
                kind,
                byte_len: metadata
                    .filter(std::fs::Metadata::is_file)
                    .map(|metadata| metadata.len().to_string()),
            },
        );
        if retained.len() > maximum {
            retained.pop_last();
        }
    }
    Ok(PlaceDirectory {
        path: text.into_owned(),
        entries: retained.into_values().collect(),
        truncated: total > maximum,
    })
}

#[cfg(test)]
mod tests {
    use super::{list_place, places, PlaceKind};
    use crate::WorkspaceEntryKind;
    use std::{fs, path::Path};

    #[test]
    fn listing_is_absolute_sorted_bounded_and_hides_dotfiles_by_default() {
        let root = tempfile::tempdir().expect("root");
        fs::create_dir(root.path().join("src")).expect("src");
        fs::create_dir(root.path().join(".git")).expect("git");
        fs::write(root.path().join("b.txt"), "bb").expect("b");
        fs::write(root.path().join("A.md"), "a").expect("a");
        fs::write(root.path().join(".env"), "x").expect("env");

        let listing = list_place(root.path(), 10, false).expect("list");
        let names: Vec<_> = listing
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, ["src", "A.md", "b.txt"]);
        assert_eq!(listing.entries[0].kind, WorkspaceEntryKind::Directory);
        assert_eq!(listing.entries[2].byte_len.as_deref(), Some("2"));
        assert!(Path::new(&listing.entries[1].path).is_absolute());
        assert!(!listing.truncated);

        let all = list_place(root.path(), 10, true).expect("list hidden");
        assert_eq!(all.entries.len(), 5);
        assert_eq!(all.entries[0].name, ".git");

        let bounded = list_place(root.path(), 2, true).expect("bounded");
        assert_eq!(bounded.entries.len(), 2);
        assert!(bounded.truncated);

        assert!(list_place(Path::new("relative/dir"), 10, false).is_err());
        assert!(list_place(&root.path().join("b.txt"), 10, false).is_err());
        assert!(list_place(root.path(), 0, false).is_err());
        assert!(list_place(root.path(), 100_000, false).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_shown_as_their_targets_and_broken_ones_do_not_fail_the_listing() {
        let root = tempfile::tempdir().expect("root");
        fs::create_dir(root.path().join("real")).expect("real");
        std::os::unix::fs::symlink(root.path().join("real"), root.path().join("alias"))
            .expect("alias");
        std::os::unix::fs::symlink(root.path().join("gone"), root.path().join("broken"))
            .expect("broken");
        let listing = list_place(root.path(), 10, false).expect("list");
        let kinds: Vec<_> = listing
            .entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("alias", WorkspaceEntryKind::Directory),
                ("real", WorkspaceEntryKind::Directory),
                ("broken", WorkspaceEntryKind::Other),
            ]
        );
    }

    #[test]
    fn places_start_at_the_computer_and_include_home() {
        let found = places();
        assert_eq!(found[0].kind, PlaceKind::Computer);
        assert!(Path::new(&found[0].path).is_absolute());
        assert_eq!(
            found
                .iter()
                .filter(|place| place.kind == PlaceKind::Computer)
                .count(),
            1
        );
        if std::env::var_os("HOME").is_some() {
            assert_eq!(found.last().map(|place| place.kind), Some(PlaceKind::Home));
        }
    }
}
