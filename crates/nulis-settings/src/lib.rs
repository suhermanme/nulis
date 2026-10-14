//! Schema-versioned layered settings independent from UI and persistence.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;
pub const CUSTOMIZATION_SCHEMA_VERSION: u32 = 1;
pub const MAX_CUSTOMIZATION_BYTES: u64 = 256 * 1024;
const MAX_CUSTOM_ENTRIES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsLayer {
    Defaults,
    User,
    Workspace,
}

#[derive(Clone, Debug, Default)]
pub struct SettingsStore {
    defaults: Map<String, Value>,
    user: Map<String, Value>,
    workspace: Map<String, Value>,
    languages: BTreeMap<String, Map<String, Value>>,
}

impl SettingsStore {
    #[must_use]
    pub fn with_defaults(defaults: Map<String, Value>) -> Self {
        Self {
            defaults,
            ..Self::default()
        }
    }

    pub fn replace_layer(&mut self, layer: SettingsLayer, values: Map<String, Value>) {
        *match layer {
            SettingsLayer::Defaults => &mut self.defaults,
            SettingsLayer::User => &mut self.user,
            SettingsLayer::Workspace => &mut self.workspace,
        } = values;
    }

    pub fn replace_language(&mut self, language: impl Into<String>, values: Map<String, Value>) {
        self.languages.insert(language.into(), values);
    }

    /// Returns a recursively merged immutable settings value.
    #[must_use]
    pub fn effective(&self, language: Option<&str>) -> Value {
        let mut result = Value::Object(self.defaults.clone());
        merge(&mut result, Value::Object(self.user.clone()));
        merge(&mut result, Value::Object(self.workspace.clone()));
        if let Some(values) = language.and_then(|key| self.languages.get(key)) {
            merge(&mut result, Value::Object(values.clone()));
        }
        result
    }
}

fn merge(target: &mut Value, overlay: Value) {
    match (target, overlay) {
        (Value::Object(target), Value::Object(overlay)) => {
            for (key, value) in overlay {
                merge(target.entry(key).or_insert(Value::Null), value);
            }
        }
        (target, overlay) => *target = overlay,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsDocument {
    pub schema_version: u32,
    pub values: Map<String, Value>,
}

impl SettingsDocument {
    /// Validates a settings document before it enters the store.
    ///
    /// # Errors
    /// Returns an error when its schema is newer than this application.
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.schema_version > SETTINGS_SCHEMA_VERSION {
            return Err(SettingsError::UnsupportedSchema(self.schema_version));
        }
        Ok(())
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SettingsError {
    #[error("settings schema version {0} is newer than this application supports")]
    UnsupportedSchema(u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppearanceSettings {
    pub theme: String,
    pub font_family: String,
    pub font_size: f64,
    pub line_height: f64,
    pub font_ligatures: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Keybinding {
    pub key: String,
    pub command: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum MenuEntry {
    Command { command: String },
    Separator,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomizationDocument {
    pub schema_version: u32,
    pub appearance: AppearanceSettings,
    pub keybindings: Vec<Keybinding>,
    pub menu: Vec<MenuEntry>,
    pub toolbar: Vec<String>,
}

impl Default for CustomizationDocument {
    fn default() -> Self {
        Self {
            schema_version: CUSTOMIZATION_SCHEMA_VERSION,
            appearance: AppearanceSettings {
                theme: "nulis.ember-dark".into(),
                font_family:
                    "ui-monospace, \"SFMono-Regular\", Consolas, \"Liberation Mono\", monospace"
                        .into(),
                font_size: 13.0,
                line_height: 1.7,
                font_ligatures: true,
            },
            keybindings: vec![
                Keybinding {
                    key: "Primary+N".into(),
                    command: "file.new".into(),
                },
                Keybinding {
                    key: "Primary+O".into(),
                    command: "file.open".into(),
                },
                Keybinding {
                    key: "Primary+S".into(),
                    command: "file.save".into(),
                },
                Keybinding {
                    key: "Primary+Shift+S".into(),
                    command: "file.saveAs".into(),
                },
                Keybinding {
                    key: "Primary+W".into(),
                    command: "workbench.closeFile".into(),
                },
                Keybinding {
                    key: "Primary+K".into(),
                    command: "workbench.commandPalette".into(),
                },
                Keybinding {
                    key: "Primary+Shift+L".into(),
                    command: "editor.toggleReadOnly".into(),
                },
                Keybinding {
                    key: "Primary+Z".into(),
                    command: "editor.undo".into(),
                },
                Keybinding {
                    key: "Primary+Shift+Z".into(),
                    command: "editor.redo".into(),
                },
            ],
            menu: vec![
                MenuEntry::Command {
                    command: "file.new".into(),
                },
                MenuEntry::Command {
                    command: "file.open".into(),
                },
                MenuEntry::Separator,
                MenuEntry::Command {
                    command: "file.save".into(),
                },
                MenuEntry::Command {
                    command: "file.saveAs".into(),
                },
                MenuEntry::Separator,
                MenuEntry::Command {
                    command: "workbench.closeFile".into(),
                },
            ],
            toolbar: vec![
                "file.new".into(),
                "file.open".into(),
                "file.save".into(),
                "workbench.closeFile".into(),
                "editor.undo".into(),
                "editor.redo".into(),
            ],
        }
    }
}

impl CustomizationDocument {
    /// Validates bounded appearance, shortcut, menu, and toolbar data.
    ///
    /// # Errors
    /// Returns a descriptive validation failure for unsafe or ambiguous input.
    pub fn validate(&self) -> Result<(), CustomizationError> {
        if self.schema_version != CUSTOMIZATION_SCHEMA_VERSION {
            return Err(CustomizationError::UnsupportedSchema(self.schema_version));
        }
        if !valid_id(&self.appearance.theme)
            || self.appearance.font_family.trim().is_empty()
            || self.appearance.font_family.len() > 256
            || !(8.0..=40.0).contains(&self.appearance.font_size)
            || !(1.0..=2.5).contains(&self.appearance.line_height)
        {
            return Err(CustomizationError::InvalidAppearance);
        }
        if self.keybindings.len() > MAX_CUSTOM_ENTRIES
            || self.menu.len() > MAX_CUSTOM_ENTRIES
            || self.toolbar.len() > MAX_CUSTOM_ENTRIES
        {
            return Err(CustomizationError::TooManyEntries);
        }
        let mut keys = BTreeSet::new();
        for binding in &self.keybindings {
            if !valid_chord(&binding.key) || !valid_id(&binding.command) {
                return Err(CustomizationError::InvalidBinding(binding.key.clone()));
            }
            if !keys.insert(binding.key.clone()) {
                return Err(CustomizationError::DuplicateBinding(binding.key.clone()));
            }
        }
        if matches!(self.menu.first(), Some(MenuEntry::Separator))
            || matches!(self.menu.last(), Some(MenuEntry::Separator))
            || self
                .menu
                .windows(2)
                .any(|pair| matches!(pair, [MenuEntry::Separator, MenuEntry::Separator]))
            || self
                .menu
                .iter()
                .any(|entry| matches!(entry, MenuEntry::Command { command } if !valid_id(command)))
        {
            return Err(CustomizationError::InvalidMenu);
        }
        let mut toolbar = BTreeSet::new();
        for command in &self.toolbar {
            if !valid_id(command) || !toolbar.insert(command) {
                return Err(CustomizationError::InvalidToolbar);
            }
        }
        Ok(())
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.contains('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn valid_chord(value: &str) -> bool {
    let parts = value.split('+').collect::<Vec<_>>();
    if !(2..=4).contains(&parts.len()) {
        return false;
    }
    let key = parts[parts.len() - 1];
    let modifiers = &parts[..parts.len() - 1];
    let canonical = ["Primary", "Shift", "Alt"];
    let mut last = None;
    for modifier in modifiers {
        let Some(index) = canonical.iter().position(|candidate| candidate == modifier) else {
            return false;
        };
        if last.is_some_and(|previous| index <= previous) {
            return false;
        }
        last = Some(index);
    }
    modifiers.contains(&"Primary")
        && (key.len() == 1 && key.bytes().all(|byte| byte.is_ascii_alphanumeric())
            || matches!(
                key,
                "Enter"
                    | "Escape"
                    | "Tab"
                    | "Backspace"
                    | "Delete"
                    | "ArrowUp"
                    | "ArrowDown"
                    | "ArrowLeft"
                    | "ArrowRight"
            ))
}

#[derive(Clone, Debug)]
pub struct CustomizationStore {
    path: PathBuf,
}

impl CustomizationStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Loads a validated customization document or defaults when none exists.
    ///
    /// # Errors
    /// Returns an I/O, size, JSON, or validation error.
    pub fn load(&self) -> Result<CustomizationDocument, CustomizationError> {
        if !self.path.exists() {
            return Ok(CustomizationDocument::default());
        }
        let metadata = fs::symlink_metadata(&self.path)?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_CUSTOMIZATION_BYTES {
            return Err(CustomizationError::InvalidFile);
        }
        let document: CustomizationDocument = serde_json::from_slice(&fs::read(&self.path)?)?;
        document.validate()?;
        Ok(document)
    }

    /// Atomically persists a validated document.
    ///
    /// # Errors
    /// Returns an I/O, size, serialization, or validation error.
    pub fn save(&self, document: &CustomizationDocument) -> Result<(), CustomizationError> {
        document.validate()?;
        let bytes = serde_json::to_vec_pretty(document)?;
        if bytes.len() as u64 > MAX_CUSTOMIZATION_BYTES {
            return Err(CustomizationError::InvalidFile);
        }
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(&self.path).map_err(|error| error.error)?;
        Ok(())
    }

    /// Removes persisted overrides and returns defaults.
    ///
    /// # Errors
    /// Returns an I/O error if the existing regular file cannot be removed.
    pub fn reset(&self) -> Result<CustomizationDocument, CustomizationError> {
        if self.path.exists() {
            let metadata = fs::symlink_metadata(&self.path)?;
            if !metadata.file_type().is_file() {
                return Err(CustomizationError::InvalidFile);
            }
            fs::remove_file(&self.path)?;
        }
        Ok(CustomizationDocument::default())
    }
}

#[derive(Debug, Error)]
pub enum CustomizationError {
    #[error("customization schema version {0} is newer than this application supports")]
    UnsupportedSchema(u32),
    #[error("appearance settings are invalid")]
    InvalidAppearance,
    #[error("customization contains too many entries")]
    TooManyEntries,
    #[error("invalid shortcut: {0}")]
    InvalidBinding(String),
    #[error("duplicate shortcut: {0}")]
    DuplicateBinding(String),
    #[error("menu layout is invalid")]
    InvalidMenu,
    #[error("toolbar layout is invalid")]
    InvalidToolbar,
    #[error("customization file is not a bounded regular file")]
    InvalidFile,
    #[error("customization I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("customization JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::{CustomizationDocument, CustomizationStore, SettingsLayer, SettingsStore};
    use serde_json::{json, Map};

    fn object(value: &serde_json::Value) -> Map<String, serde_json::Value> {
        value.as_object().expect("object").clone()
    }

    #[test]
    fn precedence_is_defaults_user_workspace_language() {
        let mut store = SettingsStore::with_defaults(object(&json!({
            "editor": { "tabSize": 4, "wordWrap": false }, "theme": "dark"
        })));
        store.replace_layer(
            SettingsLayer::User,
            object(&json!({
                "editor": { "tabSize": 2 }
            })),
        );
        store.replace_layer(
            SettingsLayer::Workspace,
            object(&json!({
                "editor": { "wordWrap": true }
            })),
        );
        store.replace_language(
            "rust",
            object(&json!({
                "editor": { "tabSize": 4 }
            })),
        );

        let effective = store.effective(Some("rust"));
        assert_eq!(effective["editor"]["tabSize"], 4);
        assert_eq!(effective["editor"]["wordWrap"], true);
        assert_eq!(effective["theme"], "dark");
    }

    #[test]
    fn arrays_replace_instead_of_merging_by_index() {
        let mut store = SettingsStore::with_defaults(object(&json!({"files": ["a", "b"]})));
        store.replace_layer(SettingsLayer::User, object(&json!({"files": ["c"]})));
        assert_eq!(store.effective(None)["files"], json!(["c"]));
    }

    #[test]
    fn customization_round_trips_and_resets() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = CustomizationStore::new(directory.path().join("customization.json"));
        let mut document = CustomizationDocument::default();
        document.appearance.font_size = 16.0;
        store.save(&document).expect("save customization");
        assert_eq!(store.load().expect("load customization"), document);
        assert_eq!(
            store.reset().expect("reset customization"),
            CustomizationDocument::default()
        );
    }

    #[test]
    fn customization_rejects_duplicate_shortcuts() {
        let mut document = CustomizationDocument::default();
        document.keybindings.push(document.keybindings[0].clone());
        assert!(document.validate().is_err());
    }
}
