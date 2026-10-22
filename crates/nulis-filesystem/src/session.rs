use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::fs::File;
use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};
use thiserror::Error;

pub const SESSION_SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_WORKSPACE_NAME: &str = "Workspace";
/// Top-level tabs; bounded by the document limit because each holds at least one.
pub const MAX_LAYOUT_GROUPS: usize = 128;
pub const MAX_LAYOUT_PANES: usize = 16;
pub const MAX_LAYOUT_DEPTH: usize = 6;
pub const MAX_WORKSPACE_NAME_CHARS: usize = 80;
const MAX_SESSION_BYTES: u64 = 1024 * 1024;
const MAX_SESSION_DOCUMENTS: usize = 128;
const MAX_SESSION_PATH_BYTES: usize = 4_096;
const MAX_LAYOUT_ID_BYTES: usize = 64;
const MIN_SIDEBAR_WIDTH: u16 = 180;
const MAX_SIDEBAR_WIDTH: u16 = 480;
const MIN_SPLIT_RATIO: u16 = 100;
const MAX_SPLIT_RATIO: u16 = 900;

/// Direction in which a branch places its two children.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SplitDirection {
    /// Children sit side by side.
    Horizontal,
    /// Children are stacked.
    Vertical,
}

/// One node of a tab's validated pane tree.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LayoutNode {
    /// A pane. The desktop shows one document per pane; older sessions may list several.
    Pane {
        id: String,
        tabs: Vec<String>,
        active_tab: Option<String>,
    },
    /// A split whose `ratio` is the first child's share in thousandths.
    Split {
        direction: SplitDirection,
        ratio: u16,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

/// A top-level tab holding one pane tree. A tab with several panes is a
/// workspace and `name` is its display name.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutGroup {
    pub id: String,
    pub name: String,
    pub root: LayoutNode,
    pub active_pane: String,
}

/// Schema-v2 workbench state. Document references are stable identities
/// (`path:<source path>` or `recovery:<checkpoint id>`), never core document IDs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkbenchSession {
    pub schema_version: u32,
    pub workspace_root: Option<String>,
    pub documents: Vec<String>,
    /// Top-level tabs in strip order.
    pub layouts: Vec<LayoutGroup>,
    pub active_layout: Option<String>,
    pub sidebar_visible: bool,
    pub sidebar_width: u16,
    pub active_panel: String,
    pub view_modes: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkbenchSessionV1 {
    #[allow(dead_code)] // Checked by the caller before this shape is chosen.
    schema_version: u32,
    workspace_root: Option<String>,
    open_paths: Vec<String>,
    active_path: Option<String>,
    secondary_path: Option<String>,
    split: bool,
    sidebar_visible: bool,
    sidebar_width: u16,
    active_panel: String,
    view_modes: BTreeMap<String, String>,
}

fn path_reference(path: &str) -> String {
    format!("path:{path}")
}

impl From<WorkbenchSessionV1> for WorkbenchSession {
    /// Maps the single optional split onto a layout tree without losing
    /// open paths or view modes.
    fn from(old: WorkbenchSessionV1) -> Self {
        let mut seen = std::collections::BTreeSet::new();
        let documents: Vec<String> = old
            .open_paths
            .iter()
            .filter(|path| seen.insert(path.as_str()))
            .map(|path| path_reference(path))
            .collect();
        let known = |path: &Option<String>| {
            path.as_deref()
                .map(path_reference)
                .filter(|reference| documents.contains(reference))
        };
        let primary = LayoutNode::Pane {
            id: "pane-1".into(),
            active_tab: known(&old.active_path).or_else(|| documents.first().cloned()),
            tabs: documents.clone(),
        };
        let root = match known(&old.secondary_path).filter(|_| old.split) {
            Some(secondary) => LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 500,
                first: Box::new(primary),
                second: Box::new(LayoutNode::Pane {
                    id: "pane-2".into(),
                    tabs: vec![secondary.clone()],
                    active_tab: Some(secondary),
                }),
            },
            None => primary,
        };
        Self {
            schema_version: SESSION_SCHEMA_VERSION,
            workspace_root: old.workspace_root,
            documents,
            layouts: vec![LayoutGroup {
                id: "layout-1".into(),
                name: "Layout 1".into(),
                root,
                active_pane: "pane-1".into(),
            }],
            active_layout: Some("layout-1".into()),
            sidebar_visible: old.sidebar_visible,
            sidebar_width: old.sidebar_width,
            active_panel: old.active_panel,
            view_modes: old
                .view_modes
                .into_iter()
                .map(|(path, mode)| (path_reference(&path), mode))
                .collect(),
        }
    }
}

impl Default for WorkbenchSession {
    fn default() -> Self {
        Self {
            schema_version: SESSION_SCHEMA_VERSION,
            workspace_root: None,
            documents: Vec::new(),
            layouts: Vec::new(),
            active_layout: None,
            sidebar_visible: true,
            sidebar_width: 260,
            active_panel: "files".into(),
            view_modes: BTreeMap::new(),
        }
    }
}

fn valid_layout_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_LAYOUT_ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_display_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.chars().count() <= MAX_WORKSPACE_NAME_CHARS
        && !name.chars().any(char::is_control)
}

fn valid_reference(reference: &str) -> bool {
    reference
        .strip_prefix("path:")
        .or_else(|| reference.strip_prefix("recovery:"))
        .is_some_and(|identity| !identity.is_empty())
}

impl LayoutGroup {
    fn validate(&self, documents: &[String]) -> Result<(), SessionError> {
        if !valid_layout_id(&self.id) || !valid_display_name(&self.name) {
            return Err(SessionError::WorkspaceName);
        }
        let mut pane_ids = Vec::new();
        validate_node(&self.root, 1, documents, &mut pane_ids)?;
        if !pane_ids.contains(&self.active_pane.as_str()) {
            return Err(SessionError::Layout("active pane does not exist".into()));
        }
        Ok(())
    }
}

fn validate_node<'a>(
    node: &'a LayoutNode,
    depth: usize,
    documents: &[String],
    pane_ids: &mut Vec<&'a str>,
) -> Result<(), SessionError> {
    if depth > MAX_LAYOUT_DEPTH {
        return Err(SessionError::LayoutTooLarge("depth", MAX_LAYOUT_DEPTH));
    }
    match node {
        LayoutNode::Pane {
            id,
            tabs,
            active_tab,
        } => {
            if !valid_layout_id(id) || pane_ids.contains(&id.as_str()) {
                return Err(SessionError::Layout("invalid or duplicate pane".into()));
            }
            pane_ids.push(id);
            if pane_ids.len() > MAX_LAYOUT_PANES {
                return Err(SessionError::LayoutTooLarge("panes", MAX_LAYOUT_PANES));
            }
            let unique = tabs.iter().collect::<std::collections::BTreeSet<_>>().len();
            if unique != tabs.len() || tabs.iter().any(|tab| !documents.contains(tab)) {
                return Err(SessionError::Layout(
                    "pane tabs must be unique session documents".into(),
                ));
            }
            // A pane without tabs is valid only as the root of its layout.
            if (tabs.is_empty() && depth > 1)
                || active_tab.as_ref().is_some_and(|tab| !tabs.contains(tab))
                || (active_tab.is_none() && !tabs.is_empty())
            {
                return Err(SessionError::Layout("invalid pane tab state".into()));
            }
            Ok(())
        }
        LayoutNode::Split {
            ratio,
            first,
            second,
            ..
        } => {
            if !(MIN_SPLIT_RATIO..=MAX_SPLIT_RATIO).contains(ratio) {
                return Err(SessionError::Layout(format!(
                    "split ratio {ratio} is out of range"
                )));
            }
            validate_node(first, depth + 1, documents, pane_ids)?;
            validate_node(second, depth + 1, documents, pane_ids)
        }
    }
}

impl WorkbenchSession {
    /// Validates all persisted state before it is stored or returned.
    ///
    /// # Errors
    /// Returns an error for unsupported schemas, excessive paths or layouts,
    /// dangling references, invalid layout values, or unsupported view modes.
    pub fn validate(&self) -> Result<(), SessionError> {
        if self.schema_version != SESSION_SCHEMA_VERSION {
            return Err(SessionError::UnsupportedSchema(self.schema_version));
        }
        if self.documents.len() > MAX_SESSION_DOCUMENTS
            || self.view_modes.len() > MAX_SESSION_DOCUMENTS
        {
            return Err(SessionError::TooManyDocuments);
        }
        for path in self
            .documents
            .iter()
            .chain(self.workspace_root.iter())
            .chain(self.view_modes.keys())
        {
            if path.len() > MAX_SESSION_PATH_BYTES {
                return Err(SessionError::PathTooLong(path.len()));
            }
        }
        let unique = self
            .documents
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        if unique.len() != self.documents.len()
            || self.documents.iter().any(|item| !valid_reference(item))
        {
            return Err(SessionError::Layout("invalid document reference".into()));
        }
        if self.layouts.len() > MAX_LAYOUT_GROUPS {
            return Err(SessionError::LayoutTooLarge("tabs", MAX_LAYOUT_GROUPS));
        }
        let mut layout_ids = std::collections::BTreeSet::new();
        for layout in &self.layouts {
            layout.validate(&self.documents)?;
            if !layout_ids.insert(layout.id.as_str()) {
                return Err(SessionError::Layout("duplicate tab".into()));
            }
        }
        if self
            .active_layout
            .as_deref()
            .is_some_and(|active| !layout_ids.contains(active))
        {
            return Err(SessionError::Layout("active tab does not exist".into()));
        }
        if !(MIN_SIDEBAR_WIDTH..=MAX_SIDEBAR_WIDTH).contains(&self.sidebar_width) {
            return Err(SessionError::SidebarWidth(self.sidebar_width));
        }
        if !matches!(self.active_panel.as_str(), "files" | "search") {
            return Err(SessionError::ActivePanel(self.active_panel.clone()));
        }
        if let Some(mode) = self
            .view_modes
            .values()
            .find(|mode| !matches!(mode.as_str(), "text" | "hex"))
        {
            return Err(SessionError::ViewMode(mode.clone()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Loads validated session state, returning defaults when no session has
    /// been saved yet.
    ///
    /// # Errors
    /// Returns an error for oversized, malformed, unsupported, or unreadable state.
    pub fn load(&self) -> Result<WorkbenchSession, SessionError> {
        let metadata = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(WorkbenchSession::default());
            }
            Err(error) => return Err(SessionError::io(error)),
        };
        if metadata.len() > MAX_SESSION_BYTES {
            return Err(SessionError::TooLarge(metadata.len()));
        }
        let bytes = fs::read(&self.path).map_err(SessionError::io)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| SessionError::Malformed(error.to_string()))?;
        let malformed = |error: serde_json::Error| SessionError::Malformed(error.to_string());
        let mut session: WorkbenchSession = match value
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
        {
            Some(1) => serde_json::from_value::<WorkbenchSessionV1>(value)
                .map_err(malformed)?
                .into(),
            _ => serde_json::from_value(value).map_err(malformed)?,
        };
        // The Remote panel was removed; a session saved with it open shows Files.
        if session.active_panel == "remote" {
            session.active_panel = "files".into();
        }
        session.validate()?;
        Ok(session)
    }

    /// Atomically stores validated session state in the application data directory.
    ///
    /// # Errors
    /// Returns an error for invalid state, serialization, or durable file operations.
    pub fn save(&self, session: &WorkbenchSession) -> Result<(), SessionError> {
        session.validate()?;
        let bytes = serde_json::to_vec(session)
            .map_err(|error| SessionError::Malformed(error.to_string()))?;
        let byte_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if byte_len > MAX_SESSION_BYTES {
            return Err(SessionError::TooLarge(byte_len));
        }
        let parent = self.path.parent().ok_or(SessionError::MissingParent)?;
        fs::create_dir_all(parent).map_err(SessionError::io)?;
        let mut temporary = tempfile::Builder::new()
            .prefix("workbench-")
            .suffix(".json.tmp")
            .tempfile_in(parent)
            .map_err(SessionError::io)?;
        temporary.write_all(&bytes).map_err(SessionError::io)?;
        temporary.as_file().sync_all().map_err(SessionError::io)?;
        temporary
            .persist(&self.path)
            .map_err(|error| SessionError::io(error.error))?;
        sync_parent(parent)?;
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent(parent: &std::path::Path) -> Result<(), SessionError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(SessionError::io)
}

#[cfg(not(unix))]
// The signature matches the Unix implementation, which can fail.
#[allow(clippy::unnecessary_wraps)]
fn sync_parent(_parent: &std::path::Path) -> Result<(), SessionError> {
    Ok(())
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("unsupported session schema version {0}")]
    UnsupportedSchema(u32),
    #[error("session contains too many open documents")]
    TooManyDocuments,
    #[error("workspace name must be 1 to {MAX_WORKSPACE_NAME_CHARS} trimmed characters")]
    WorkspaceName,
    #[error("layout is too large: at most {1} {0} are supported")]
    LayoutTooLarge(&'static str, usize),
    #[error("invalid session layout: {0}")]
    Layout(String),
    #[error("session path has {0} bytes and exceeds the limit")]
    PathTooLong(usize),
    #[error("session sidebar width must be in {MIN_SIDEBAR_WIDTH}..={MAX_SIDEBAR_WIDTH}, got {0}")]
    SidebarWidth(u16),
    #[error("unsupported session panel {0:?}")]
    ActivePanel(String),
    #[error("unsupported session view mode {0:?}")]
    ViewMode(String),
    #[error("session state is {0} bytes and exceeds the limit")]
    TooLarge(u64),
    #[error("session file has no parent directory")]
    MissingParent,
    #[error("malformed session state: {0}")]
    Malformed(String),
    #[error("session filesystem operation failed: {0}")]
    Io(String),
}

impl SessionError {
    // `Result::map_err` supplies the owned I/O error to this adapter.
    #[allow(clippy::needless_pass_by_value)]
    fn io(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LayoutGroup, LayoutNode, SessionError, SessionStore, SplitDirection, WorkbenchSession,
        MAX_LAYOUT_DEPTH, MAX_LAYOUT_GROUPS, MAX_LAYOUT_PANES,
    };
    use std::collections::BTreeMap;

    fn pane(id: &str, tabs: &[&str]) -> LayoutNode {
        LayoutNode::Pane {
            id: id.into(),
            tabs: tabs.iter().map(|tab| (*tab).into()).collect(),
            active_tab: tabs.first().map(|tab| (*tab).into()),
        }
    }

    fn split(direction: SplitDirection, first: LayoutNode, second: LayoutNode) -> LayoutNode {
        LayoutNode::Split {
            direction,
            ratio: 400,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    fn balanced(first: usize, count: usize, tab: &str) -> LayoutNode {
        if count == 1 {
            return pane(&format!("pane-{first}"), &[tab]);
        }
        let half = count / 2;
        split(
            SplitDirection::Horizontal,
            balanced(first, half, tab),
            balanced(first + half, count - half, tab),
        )
    }

    fn session_with(root: LayoutNode, documents: &[&str]) -> WorkbenchSession {
        WorkbenchSession {
            documents: documents.iter().map(|item| (*item).into()).collect(),
            layouts: vec![LayoutGroup {
                id: "layout-1".into(),
                name: "Review".into(),
                root,
                active_pane: "pane-1".into(),
            }],
            active_layout: Some("layout-1".into()),
            ..WorkbenchSession::default()
        }
    }

    #[test]
    fn a_session_saved_with_the_removed_remote_panel_opens_on_files() {
        let root = tempfile::tempdir().expect("session root");
        let store = SessionStore::new(root.path().join("workbench.json"));
        let mut saved = serde_json::to_value(WorkbenchSession::default()).expect("json");
        saved["activePanel"] = "remote".into();
        std::fs::write(root.path().join("workbench.json"), saved.to_string()).expect("write");
        assert_eq!(store.load().expect("load").active_panel, "files");
    }

    #[test]
    fn session_round_trip_preserves_names_documents_and_nested_layout() {
        let root = tempfile::tempdir().expect("session root");
        let store = SessionStore::new(root.path().join("workbench.json"));
        let a = "path:/project/a.txt";
        let b = "path:/project/b.bin";
        let draft = "recovery:document-abc";
        let session = WorkbenchSession {
            workspace_root: Some("/project".into()),
            sidebar_width: 320,
            active_panel: "search".into(),
            view_modes: BTreeMap::from([(b.into(), "hex".into())]),
            ..session_with(
                split(
                    SplitDirection::Horizontal,
                    pane("pane-1", &[a, draft]),
                    split(
                        SplitDirection::Vertical,
                        pane("pane-2", &[b]),
                        pane("pane-3", &[a]),
                    ),
                ),
                &[a, b, draft],
            )
        };
        store.save(&session).expect("save session");
        assert_eq!(store.load().expect("load session"), session);
    }

    #[test]
    fn schema_v1_split_state_migrates_without_losing_paths_or_view_modes() {
        let root = tempfile::tempdir().expect("session root");
        let path = root.path().join("workbench.json");
        std::fs::write(
            &path,
            serde_json::json!({
                "schemaVersion": 1,
                "workspaceRoot": "/project",
                "openPaths": ["/project/a.txt", "/project/b.bin"],
                "activePath": "/project/b.bin",
                "secondaryPath": "/project/a.txt",
                "split": true,
                "sidebarVisible": false,
                "sidebarWidth": 300,
                "activePanel": "search",
                "viewModes": { "/project/b.bin": "hex" }
            })
            .to_string(),
        )
        .expect("write v1 session");
        let migrated = SessionStore::new(path).load().expect("migrate");
        assert_eq!(migrated.schema_version, 2);
        assert_eq!(migrated.active_layout.as_deref(), Some("layout-1"));
        assert_eq!(
            migrated.documents,
            vec!["path:/project/a.txt", "path:/project/b.bin"]
        );
        assert_eq!(migrated.view_modes["path:/project/b.bin"], "hex");
        assert!(!migrated.sidebar_visible);
        let LayoutNode::Split { first, second, .. } = &migrated.layouts[0].root else {
            panic!("the v1 split becomes a branch");
        };
        assert_eq!(
            **first,
            LayoutNode::Pane {
                id: "pane-1".into(),
                tabs: migrated.documents.clone(),
                active_tab: Some("path:/project/b.bin".into()),
            }
        );
        assert_eq!(**second, pane("pane-2", &["path:/project/a.txt"]));
    }

    #[test]
    fn invalid_layout_state_is_rejected_before_persistence() {
        let root = tempfile::tempdir().expect("session root");
        let store = SessionStore::new(root.path().join("workbench.json"));
        let session = WorkbenchSession {
            sidebar_width: 10,
            ..WorkbenchSession::default()
        };
        assert!(store.save(&session).is_err());
        assert!(!root.path().join("workbench.json").exists());
    }

    #[test]
    fn layout_limits_references_and_names_are_validated() {
        let a = "path:/a";
        // Dangling tab reference.
        assert!(session_with(pane("pane-1", &[a]), &[]).validate().is_err());
        // Duplicate pane identity.
        let duplicate = split(
            SplitDirection::Vertical,
            pane("pane-1", &[a]),
            pane("pane-1", &[a]),
        );
        assert!(session_with(duplicate, &[a]).validate().is_err());
        // An empty pane may only be the root.
        let hollow = split(
            SplitDirection::Vertical,
            pane("pane-1", &[a]),
            pane("pane-2", &[]),
        );
        assert!(session_with(hollow, &[a]).validate().is_err());
        assert!(session_with(pane("pane-1", &[]), &[]).validate().is_ok());

        // Depth and pane-count limits.
        let mut deep = pane("pane-1", &[a]);
        for level in 0..MAX_LAYOUT_DEPTH {
            deep = split(
                SplitDirection::Horizontal,
                deep,
                pane(&format!("deep-{level}"), &[a]),
            );
        }
        assert!(matches!(
            session_with(deep, &[a]).validate(),
            Err(SessionError::LayoutTooLarge("depth", _))
        ));
        assert!(session_with(balanced(1, MAX_LAYOUT_PANES, a), &[a])
            .validate()
            .is_ok());
        assert!(matches!(
            session_with(balanced(1, MAX_LAYOUT_PANES + 1, a), &[a]).validate(),
            Err(SessionError::LayoutTooLarge("panes", _))
        ));

        // Tab count, ratio, and display names.
        let crowded = WorkbenchSession {
            layouts: (0..=MAX_LAYOUT_GROUPS)
                .map(|index| LayoutGroup {
                    id: format!("layout-{index}"),
                    name: format!("Layout {index}"),
                    root: pane("pane-1", &[]),
                    active_pane: "pane-1".into(),
                })
                .collect(),
            active_layout: Some("layout-0".into()),
            ..WorkbenchSession::default()
        };
        assert!(crowded.validate().is_err());
        let mut lopsided = split(
            SplitDirection::Horizontal,
            pane("pane-1", &[a]),
            pane("pane-2", &[a]),
        );
        if let LayoutNode::Split { ratio, .. } = &mut lopsided {
            *ratio = 990;
        }
        assert!(session_with(lopsided, &[a]).validate().is_err());
        for name in ["", "  padded ", &"x".repeat(81)] {
            let mut named = session_with(pane("pane-1", &[a]), &[a]);
            named.layouts[0].name = name.into();
            assert!(matches!(named.validate(), Err(SessionError::WorkspaceName)));
        }
        // No open tab is a valid session; a dangling active tab is not.
        assert!(WorkbenchSession::default().validate().is_ok());
        let dangling = WorkbenchSession {
            active_layout: Some("layout-9".into()),
            ..WorkbenchSession::default()
        };
        assert!(dangling.validate().is_err());
    }

    #[test]
    fn unknown_session_schemas_are_rejected() {
        for schema_version in [0, 1, 3] {
            let session = WorkbenchSession {
                schema_version,
                ..WorkbenchSession::default()
            };
            assert!(session.validate().is_err());
        }
    }
}
