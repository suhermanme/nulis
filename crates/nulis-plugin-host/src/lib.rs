//! Validated plugin packages, explicit grants, and an isolated JavaScript worker protocol.

mod worker;

pub use worker::{execute_worker_request, run_worker_stdio, ProcessPluginRunner};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};
use thiserror::Error;

pub const PLUGIN_API_VERSION: u32 = 2;
pub const PLUGIN_API_VERSION_MIN: u32 = 1;
pub const PLUGIN_STORE_SCHEMA_VERSION: u32 = 1;
pub const MAX_INSTALLED_PLUGINS: usize = 128;
pub const MAX_PLUGIN_MANIFEST_BYTES: u64 = 262_144;
pub const MAX_PLUGIN_SOURCE_BYTES: usize = 1_048_576;
pub const MAX_PLUGIN_ARGUMENT_BYTES: usize = 262_144;
pub const MAX_PLUGIN_RESPONSE_BYTES: usize = 1_048_576;
pub const MAX_PLUGIN_COMMANDS: usize = 256;
pub const MAX_PLUGIN_OPERATIONS: usize = 256;
pub const MAX_PLUGIN_MESSAGE_BYTES: usize = 65_536;
pub const MAX_PLUGIN_LANGUAGES: usize = 32;
pub const MAX_PLUGIN_ASSOCIATIONS: usize = 64;
pub const MAX_PLUGIN_LANGUAGE_WORDS: usize = 1_024;
pub const MAX_PLUGIN_LANGUAGE_STRINGS: usize = 32;
pub const MAX_PLUGIN_LANGUAGE_BRACKETS: usize = 16;
pub const MAX_PLUGIN_SNIPPETS: usize = 256;
pub const MAX_PLUGIN_SNIPPET_BYTES: usize = 4_096;
pub const MAX_PLUGIN_REGEX_BYTES: usize = 4_096;
/// Compiled size cap for declarative regexes, so a hostile pattern cannot
/// allocate an unbounded automaton at install time.
const MAX_PLUGIN_REGEX_SIZE: usize = 1_048_576;
const MAX_ID_BYTES: usize = 128;
const MAX_NAME_BYTES: usize = 128;
const MAX_VERSION_BYTES: usize = 64;
const MAX_ENTRY_PATH_BYTES: usize = 4_096;
const MAX_GRANT_STORE_BYTES: u64 = 1_048_576;
const MAX_LANGUAGE_ID_BYTES: usize = 64;
const MAX_ASSOCIATION_BYTES: usize = 255;
const MAX_WORD_BYTES: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginEngines {
    pub nulis: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginPermission {
    ShowMessage,
}

/// A switchable contribution family. A user can disable one family for one
/// plugin without revoking the whole approval.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginCapability {
    Commands,
    Languages,
    Highlight,
    Indentation,
    Completion,
    Snippets,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginContributions {
    #[serde(default)]
    pub commands: Vec<PluginCommandContribution>,
    /// API v2 declarative languages. Ignored by a v1 host, which rejects the
    /// whole manifest as malformed.
    #[serde(default)]
    pub languages: Vec<PluginLanguageContribution>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCommandContribution {
    pub command: String,
    pub title: String,
    #[serde(default = "default_plugin_category")]
    pub category: String,
}

fn default_plugin_category() -> String {
    "Plugin".into()
}

fn default_api_version() -> u32 {
    PLUGIN_API_VERSION_MIN
}

/// One language a plugin contributes, as data only. The host validates it
/// completely at install time and builds a runtime language from it after
/// approval; no plugin code runs to define a language.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginLanguageContribution {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub filenames: Vec<String>,
    /// A regular expression tried against the first line when the file name
    /// says nothing.
    #[serde(default)]
    pub first_line: Option<String>,
    #[serde(default)]
    pub comments: PluginComments,
    /// Bracket pairs, reserved for auto-pairing; validated but not yet applied.
    #[serde(default)]
    pub brackets: Vec<[String; 2]>,
    #[serde(default)]
    pub highlight: Option<PluginHighlight>,
    #[serde(default)]
    pub indentation: Option<PluginIndentation>,
    #[serde(default)]
    pub completion: Option<PluginCompletion>,
    #[serde(default)]
    pub snippets: Vec<PluginSnippet>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginComments {
    #[serde(default)]
    pub line: Option<String>,
    #[serde(default)]
    pub block: Option<(String, String)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginHighlightMode {
    Code,
    Markup,
    Css,
    Delimited,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginHighlight {
    #[serde(default)]
    pub mode: Option<PluginHighlightMode>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub strings: Vec<PluginString>,
    #[serde(default)]
    pub case_insensitive: bool,
    /// Extra bytes allowed inside an identifier (`$`, `-`).
    #[serde(default)]
    pub ident_chars: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginString {
    pub open: String,
    pub close: String,
    #[serde(default)]
    pub escape: bool,
    #[serde(default)]
    pub multiline: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginIndentation {
    /// A line matching this regex opens a block after Enter.
    #[serde(default)]
    pub increase_after: Option<String>,
    /// A line matching this regex moves left when its first character is typed.
    #[serde(default)]
    pub outdent_on_type: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCompletion {
    #[serde(default = "default_true")]
    pub keywords: bool,
    #[serde(default = "default_true")]
    pub words: bool,
}

fn default_true() -> bool {
    true
}

impl Default for PluginCompletion {
    fn default() -> Self {
        Self {
            keywords: true,
            words: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginSnippet {
    pub prefix: String,
    pub body: String,
    #[serde(default)]
    pub description: String,
}

impl PluginLanguageContribution {
    /// Capability families this language actually declares.
    #[must_use]
    pub fn capabilities(&self) -> Vec<PluginCapability> {
        let mut capabilities = vec![PluginCapability::Languages];
        if self.highlight.is_some() {
            capabilities.push(PluginCapability::Highlight);
        }
        if self.indentation.is_some() {
            capabilities.push(PluginCapability::Indentation);
        }
        if self.completion.is_some() {
            capabilities.push(PluginCapability::Completion);
        }
        if !self.snippets.is_empty() {
            capabilities.push(PluginCapability::Snippets);
        }
        capabilities
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifest {
    /// `1` when absent, so every v1 manifest stays valid.
    #[serde(default = "default_api_version")]
    pub api_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub engines: PluginEngines,
    #[serde(default)]
    pub permissions: BTreeSet<PluginPermission>,
    #[serde(default)]
    pub activation_events: BTreeSet<String>,
    /// Required by API v1; optional in v2 so a language can be pure data.
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub contributes: PluginContributions,
}

impl PluginManifest {
    /// Performs security-relevant validation before installation or execution.
    ///
    /// # Errors
    /// Returns a typed validation error for unsafe paths, identities, events,
    /// or command contributions.
    pub fn validate(&self) -> Result<(), PluginError> {
        if !(PLUGIN_API_VERSION_MIN..=PLUGIN_API_VERSION).contains(&self.api_version) {
            return Err(PluginError::UnsupportedApi(self.api_version));
        }
        validate_id(&self.id, "plugin")?;
        if self.id.starts_with("plugin.") || self.id.starts_with("automation.") {
            return Err(PluginError::ReservedNamespace(self.id.clone()));
        }
        validate_text(&self.name, MAX_NAME_BYTES, "pluginName")?;
        validate_text(&self.version, MAX_VERSION_BYTES, "pluginVersion")?;
        validate_text(&self.engines.nulis, MAX_VERSION_BYTES, "pluginEngine")?;
        semver::Version::parse(&self.version)
            .map_err(|_| PluginError::InvalidVersion(self.version.clone()))?;
        let engine = semver::VersionReq::parse(&self.engines.nulis)
            .map_err(|_| PluginError::InvalidEngine(self.engines.nulis.clone()))?;
        let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
            .map_err(|_| PluginError::InvalidVersion(env!("CARGO_PKG_VERSION").into()))?;
        if !engine.matches(&current) {
            return Err(PluginError::UnsupportedEngine {
                required: self.engines.nulis.clone(),
                current: current.to_string(),
            });
        }
        match &self.main {
            Some(main) => {
                validate_entry_path(main)?;
                if !Path::new(main)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("js"))
                {
                    return Err(PluginError::UnsupportedEntry(main.clone()));
                }
            }
            None if self.api_version == 1 => return Err(PluginError::MissingEntry),
            None if !self.contributes.commands.is_empty() => {
                return Err(PluginError::MissingEntry);
            }
            None if !self.activation_events.is_empty() => {
                return Err(PluginError::MissingEntry);
            }
            None => {}
        }
        if self.contributes.commands.is_empty() && self.contributes.languages.is_empty() {
            return Err(PluginError::NoContributions);
        }
        if self.contributes.commands.len() > MAX_PLUGIN_COMMANDS {
            return Err(limit("pluginCommands", MAX_PLUGIN_COMMANDS as u64));
        }
        let prefix = format!("{}.", self.id);
        let mut commands = BTreeSet::new();
        for command in &self.contributes.commands {
            validate_id(&command.command, "plugin command")?;
            if !command.command.starts_with(&prefix) {
                return Err(PluginError::CommandNamespace {
                    plugin: self.id.clone(),
                    command: command.command.clone(),
                });
            }
            validate_text(&command.title, MAX_NAME_BYTES, "pluginCommandTitle")?;
            validate_text(&command.category, MAX_NAME_BYTES, "pluginCommandCategory")?;
            if !commands.insert(command.command.clone()) {
                return Err(PluginError::DuplicateCommand(command.command.clone()));
            }
        }
        for event in &self.activation_events {
            if event == "onStartup" {
                continue;
            }
            let command = event
                .strip_prefix("onCommand:")
                .ok_or_else(|| PluginError::InvalidActivation(event.clone()))?;
            if !commands.contains(command) {
                return Err(PluginError::InvalidActivation(event.clone()));
            }
        }
        for command in commands {
            let event = format!("onCommand:{command}");
            if !self.activation_events.contains(&event) {
                return Err(PluginError::MissingCommandActivation(command));
            }
        }
        self.validate_languages()
    }

    /// Every capability family this plugin declares anywhere.
    #[must_use]
    pub fn capabilities(&self) -> BTreeSet<PluginCapability> {
        let mut capabilities = BTreeSet::new();
        if !self.contributes.commands.is_empty() {
            capabilities.insert(PluginCapability::Commands);
        }
        for language in &self.contributes.languages {
            capabilities.extend(language.capabilities());
        }
        capabilities
    }

    fn validate_languages(&self) -> Result<(), PluginError> {
        if self.contributes.languages.len() > MAX_PLUGIN_LANGUAGES {
            return Err(limit("pluginLanguages", MAX_PLUGIN_LANGUAGES as u64));
        }
        let mut ids = BTreeSet::new();
        for language in &self.contributes.languages {
            validate_language_id(&language.id)?;
            if !ids.insert(language.id.as_str()) {
                return Err(PluginError::DuplicateLanguage(language.id.clone()));
            }
            validate_text(&language.display_name, MAX_NAME_BYTES, "languageName")?;
            if language.extensions.len() + language.filenames.len() > MAX_PLUGIN_ASSOCIATIONS {
                return Err(limit("pluginAssociations", MAX_PLUGIN_ASSOCIATIONS as u64));
            }
            for extension in &language.extensions {
                validate_association(&format!("*.{extension}"))?;
            }
            for filename in &language.filenames {
                validate_association(filename)?;
            }
            if let Some(pattern) = &language.first_line {
                validate_regex(pattern)?;
            }
            validate_comments(&language.comments)?;
            if language.brackets.len() > MAX_PLUGIN_LANGUAGE_BRACKETS {
                return Err(limit(
                    "pluginLanguageBrackets",
                    MAX_PLUGIN_LANGUAGE_BRACKETS as u64,
                ));
            }
            let mut brackets = BTreeSet::new();
            for [open, close] in &language.brackets {
                if open.chars().count() != 1
                    || close.chars().count() != 1
                    || open == close
                    || !brackets.insert((open, close))
                {
                    return Err(PluginError::InvalidLanguageData(
                        "a bracket pair needs two distinct single characters".into(),
                    ));
                }
            }
            if let Some(highlight) = &language.highlight {
                validate_highlight(highlight)?;
            }
            if let Some(indentation) = &language.indentation {
                if let Some(pattern) = &indentation.increase_after {
                    validate_regex(pattern)?;
                }
                if let Some(pattern) = &indentation.outdent_on_type {
                    validate_regex(pattern)?;
                }
            }
            if language.snippets.len() > MAX_PLUGIN_SNIPPETS {
                return Err(limit("pluginSnippets", MAX_PLUGIN_SNIPPETS as u64));
            }
            let mut prefixes = BTreeSet::new();
            for snippet in &language.snippets {
                validate_text(&snippet.prefix, MAX_WORD_BYTES, "snippetPrefix")?;
                if snippet.body.is_empty()
                    || snippet.body.len() > MAX_PLUGIN_SNIPPET_BYTES
                    || snippet.body.chars().any(|character| {
                        character.is_control() && !matches!(character, '\n' | '\t')
                    })
                {
                    return Err(limit("pluginSnippetBytes", MAX_PLUGIN_SNIPPET_BYTES as u64));
                }
                validate_text(&snippet.description, MAX_NAME_BYTES, "snippetDescription")?;
                if !prefixes.insert(snippet.prefix.as_str()) {
                    return Err(PluginError::InvalidLanguageData(format!(
                        "duplicate snippet prefix {}",
                        snippet.prefix
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PluginPackage {
    root: PathBuf,
    source: Option<String>,
    pub manifest: PluginManifest,
    pub digest: String,
}

impl PluginPackage {
    /// Loads exactly one manifest and, when the manifest declares one, its
    /// JavaScript entry point from a package directory without following
    /// symlinks. A declarative-only v2 package has no entry point.
    ///
    /// # Errors
    /// Returns a validation, limit, or I/O error.
    pub fn load(root: &Path) -> Result<Self, PluginError> {
        let root_metadata = fs::symlink_metadata(root).map_err(PluginError::io)?;
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            return Err(PluginError::UnsafePackage(root.display().to_string()));
        }
        let root = root.canonicalize().map_err(PluginError::io)?;
        let manifest_path = root.join("plugin.json");
        validate_regular_file(&manifest_path, &root)?;
        let metadata = fs::metadata(&manifest_path).map_err(PluginError::io)?;
        if metadata.len() > MAX_PLUGIN_MANIFEST_BYTES {
            return Err(limit("pluginManifestBytes", MAX_PLUGIN_MANIFEST_BYTES));
        }
        let manifest_bytes = fs::read(&manifest_path).map_err(PluginError::io)?;
        let manifest: PluginManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| PluginError::Malformed(error.to_string()))?;
        manifest.validate()?;
        let source_bytes = match &manifest.main {
            Some(main) => {
                let entry = resolve_package_file(&root, main)?;
                let entry_metadata = fs::metadata(&entry).map_err(PluginError::io)?;
                if entry_metadata.len() > MAX_PLUGIN_SOURCE_BYTES as u64 {
                    return Err(limit("pluginSourceBytes", MAX_PLUGIN_SOURCE_BYTES as u64));
                }
                fs::read(entry).map_err(PluginError::io)?
            }
            None => Vec::new(),
        };
        let source = if manifest.main.is_some() {
            Some(
                String::from_utf8(source_bytes.clone())
                    .map_err(|_| PluginError::InvalidSourceEncoding)?,
            )
        } else {
            None
        };
        let mut hasher = Sha256::new();
        hasher.update(&manifest_bytes);
        hasher.update([0]);
        hasher.update(&source_bytes);
        let digest = format!("{:x}", hasher.finalize());
        Ok(Self {
            root,
            source,
            manifest,
            digest,
        })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Builds the worker request for an executable package.
    ///
    /// # Errors
    /// Returns `MissingEntry` for a declarative-only package.
    pub fn worker_request(
        &self,
        command: Option<String>,
        arguments: Vec<Value>,
    ) -> Result<PluginWorkerRequest, PluginError> {
        let source = self.source.clone().ok_or(PluginError::MissingEntry)?;
        Ok(PluginWorkerRequest {
            api_version: self.manifest.api_version,
            source,
            command,
            arguments,
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginGrant {
    #[serde(default)]
    pub permissions: BTreeSet<PluginPermission>,
}

impl PluginGrant {
    /// Ensures a user grant never exceeds the manifest request.
    ///
    /// # Errors
    /// Returns an error naming the first unrequested permission.
    pub fn validate_for(&self, manifest: &PluginManifest) -> Result<(), PluginError> {
        if let Some(permission) = self.permissions.difference(&manifest.permissions).next() {
            return Err(PluginError::GrantExceedsRequest(format!(
                "permission {permission:?}"
            )));
        }
        Ok(())
    }

    /// Checks one host operation at the point of use.
    ///
    /// # Errors
    /// Returns permission denied when the required authority is absent.
    pub fn authorize(&self, operation: &PluginHostOperation) -> Result<(), PluginError> {
        let permission = operation.required_permission();
        if self.permissions.contains(&permission) {
            Ok(())
        } else {
            Err(PluginError::PermissionDenied(format!(
                "permission {permission:?}"
            )))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PluginHostOperation {
    ShowMessage { message: String },
}

impl PluginHostOperation {
    #[must_use]
    pub const fn required_permission(&self) -> PluginPermission {
        match self {
            Self::ShowMessage { .. } => PluginPermission::ShowMessage,
        }
    }

    fn validate(&self) -> Result<(), PluginError> {
        match self {
            Self::ShowMessage { message } => {
                validate_text(message, MAX_PLUGIN_MESSAGE_BYTES, "pluginMessage")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginWorkerRequest {
    pub api_version: u32,
    pub source: String,
    pub command: Option<String>,
    #[serde(default)]
    pub arguments: Vec<Value>,
}

impl PluginWorkerRequest {
    fn validate(&self) -> Result<(), PluginError> {
        if !(PLUGIN_API_VERSION_MIN..=PLUGIN_API_VERSION).contains(&self.api_version) {
            return Err(PluginError::UnsupportedApi(self.api_version));
        }
        if self.source.len() > MAX_PLUGIN_SOURCE_BYTES {
            return Err(limit("pluginSourceBytes", MAX_PLUGIN_SOURCE_BYTES as u64));
        }
        if let Some(command) = &self.command {
            validate_id(command, "plugin command")?;
        }
        let bytes = serde_json::to_vec(&self.arguments)
            .map_err(|error| PluginError::Malformed(error.to_string()))?;
        if bytes.len() > MAX_PLUGIN_ARGUMENT_BYTES {
            return Err(limit(
                "pluginArgumentBytes",
                MAX_PLUGIN_ARGUMENT_BYTES as u64,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginWorkerResponse {
    #[serde(default)]
    pub result: Value,
    #[serde(default)]
    pub operations: Vec<PluginHostOperation>,
    #[serde(default)]
    pub registered_commands: Vec<String>,
}

impl PluginWorkerResponse {
    fn validate(&self) -> Result<(), PluginError> {
        if self.operations.len() > MAX_PLUGIN_OPERATIONS {
            return Err(limit("pluginOperations", MAX_PLUGIN_OPERATIONS as u64));
        }
        for operation in &self.operations {
            operation.validate()?;
        }
        if self.registered_commands.len() > MAX_PLUGIN_COMMANDS {
            return Err(limit("pluginCommands", MAX_PLUGIN_COMMANDS as u64));
        }
        let mut commands = BTreeSet::new();
        for command in &self.registered_commands {
            validate_id(command, "plugin command")?;
            if !commands.insert(command) {
                return Err(PluginError::DuplicateCommand(command.clone()));
            }
        }
        let bytes =
            serde_json::to_vec(self).map_err(|error| PluginError::Malformed(error.to_string()))?;
        if bytes.len() > MAX_PLUGIN_RESPONSE_BYTES {
            return Err(limit(
                "pluginResponseBytes",
                MAX_PLUGIN_RESPONSE_BYTES as u64,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub digest: String,
    pub api_version: u32,
    pub requested_permissions: BTreeSet<PluginPermission>,
    pub approved: bool,
    /// False keeps an approved plugin installed but inert.
    pub enabled: bool,
    /// Families the user switched off for this plugin.
    pub disabled_capabilities: BTreeSet<PluginCapability>,
    pub commands: Vec<PluginCommandContribution>,
    pub languages: Vec<PluginLanguageSummary>,
}

/// The language half of a plugin summary: enough for Settings, without the
/// full keyword and snippet tables.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginLanguageSummary {
    pub id: String,
    pub display_name: String,
    pub extensions: Vec<String>,
    pub filenames: Vec<String>,
    pub capabilities: Vec<PluginCapability>,
}

/// Approval plus the user's run-time switches for one exact package digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginActivation {
    pub grant: PluginGrant,
    pub enabled: bool,
    pub disabled_capabilities: BTreeSet<PluginCapability>,
}

impl PluginActivation {
    /// Whether one family is currently active for this plugin.
    #[must_use]
    pub fn capability_enabled(&self, capability: PluginCapability) -> bool {
        self.enabled && !self.disabled_capabilities.contains(&capability)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginApproval {
    plugin_id: String,
    package_digest: String,
    grant: PluginGrant,
    /// Absent in a document written before v2, where an approval meant active.
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    disabled_capabilities: BTreeSet<PluginCapability>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApprovalDocument {
    schema_version: u32,
    approvals: Vec<PluginApproval>,
}

impl Default for ApprovalDocument {
    fn default() -> Self {
        Self {
            schema_version: PLUGIN_STORE_SCHEMA_VERSION,
            approvals: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PluginStore {
    path: PathBuf,
}

impl PluginStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Returns the grant and switches only when they match the exact package digest.
    ///
    /// # Errors
    /// Returns a validation or storage error.
    pub fn activation_for(
        &self,
        package: &PluginPackage,
    ) -> Result<Option<PluginActivation>, PluginError> {
        Ok(self
            .load()?
            .approvals
            .into_iter()
            .find(|approval| {
                approval.plugin_id == package.manifest.id
                    && approval.package_digest == package.digest
            })
            .map(|approval| PluginActivation {
                grant: approval.grant,
                enabled: approval.enabled,
                disabled_capabilities: approval.disabled_capabilities,
            }))
    }

    /// Returns a grant only when it matches the exact package digest.
    ///
    /// # Errors
    /// Returns a validation or storage error.
    pub fn grant_for(&self, package: &PluginPackage) -> Result<Option<PluginGrant>, PluginError> {
        Ok(self
            .activation_for(package)?
            .map(|activation| activation.grant))
    }

    /// Atomically records an explicit grant for one exact package revision.
    /// Re-approving the same digest keeps the user's enable switches.
    ///
    /// # Errors
    /// Returns a validation or storage error.
    pub fn approve(&self, package: &PluginPackage, grant: PluginGrant) -> Result<(), PluginError> {
        grant.validate_for(&package.manifest)?;
        let mut document = self.load()?;
        let existing = document
            .approvals
            .iter()
            .find(|approval| {
                approval.plugin_id == package.manifest.id
                    && approval.package_digest == package.digest
            })
            .cloned();
        document
            .approvals
            .retain(|approval| approval.plugin_id != package.manifest.id);
        document.approvals.push(PluginApproval {
            plugin_id: package.manifest.id.clone(),
            package_digest: package.digest.clone(),
            grant,
            enabled: existing.as_ref().is_none_or(|approval| approval.enabled),
            disabled_capabilities: existing
                .map(|approval| approval.disabled_capabilities)
                .unwrap_or_default(),
        });
        document
            .approvals
            .sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));
        self.save(&document)
    }

    /// Enables or disables an approved plugin without revoking it.
    ///
    /// # Errors
    /// Returns a validation or storage error.
    pub fn set_enabled(&self, plugin_id: &str, enabled: bool) -> Result<bool, PluginError> {
        validate_id(plugin_id, "plugin")?;
        let mut document = self.load()?;
        let Some(approval) = document
            .approvals
            .iter_mut()
            .find(|approval| approval.plugin_id == plugin_id)
        else {
            return Ok(false);
        };
        approval.enabled = enabled;
        self.save(&document)?;
        Ok(true)
    }

    /// Switches one capability family on or off for an approved plugin.
    ///
    /// # Errors
    /// Returns a validation or storage error.
    pub fn set_capability_enabled(
        &self,
        plugin_id: &str,
        capability: PluginCapability,
        enabled: bool,
    ) -> Result<bool, PluginError> {
        validate_id(plugin_id, "plugin")?;
        let mut document = self.load()?;
        let Some(approval) = document
            .approvals
            .iter_mut()
            .find(|approval| approval.plugin_id == plugin_id)
        else {
            return Ok(false);
        };
        if enabled {
            approval.disabled_capabilities.remove(&capability);
        } else {
            approval.disabled_capabilities.insert(capability);
        }
        self.save(&document)?;
        Ok(true)
    }

    /// Removes any grant for a plugin identifier.
    ///
    /// # Errors
    /// Returns a validation or storage error.
    pub fn revoke(&self, plugin_id: &str) -> Result<bool, PluginError> {
        validate_id(plugin_id, "plugin")?;
        let mut document = self.load()?;
        let prior = document.approvals.len();
        document
            .approvals
            .retain(|approval| approval.plugin_id != plugin_id);
        let revoked = document.approvals.len() != prior;
        if revoked {
            self.save(&document)?;
        }
        Ok(revoked)
    }

    fn load(&self) -> Result<ApprovalDocument, PluginError> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ApprovalDocument::default());
            }
            Err(error) => return Err(PluginError::io(error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PluginError::UnsafePackage(self.path.display().to_string()));
        }
        if metadata.len() > MAX_GRANT_STORE_BYTES {
            return Err(limit("pluginGrantStoreBytes", MAX_GRANT_STORE_BYTES));
        }
        let document: ApprovalDocument =
            serde_json::from_slice(&fs::read(&self.path).map_err(PluginError::io)?)
                .map_err(|error| PluginError::Malformed(error.to_string()))?;
        validate_approval_document(&document)?;
        Ok(document)
    }

    fn save(&self, document: &ApprovalDocument) -> Result<(), PluginError> {
        validate_approval_document(document)?;
        let bytes = serde_json::to_vec(document)
            .map_err(|error| PluginError::Malformed(error.to_string()))?;
        if bytes.len() as u64 > MAX_GRANT_STORE_BYTES {
            return Err(limit("pluginGrantStoreBytes", MAX_GRANT_STORE_BYTES));
        }
        let parent = self.path.parent().ok_or(PluginError::MissingParent)?;
        fs::create_dir_all(parent).map_err(PluginError::io)?;
        validate_plugin_directory(parent)?;
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(PluginError::UnsafePackage(self.path.display().to_string()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(PluginError::io(error)),
        }
        let mut temporary = tempfile::Builder::new()
            .prefix("plugin-grants-")
            .suffix(".json.tmp")
            .tempfile_in(parent)
            .map_err(PluginError::io)?;
        temporary.write_all(&bytes).map_err(PluginError::io)?;
        temporary.as_file().sync_all().map_err(PluginError::io)?;
        temporary
            .persist(&self.path)
            .map_err(|error| PluginError::io(error.error))?;
        sync_parent(parent)
    }
}

#[derive(Clone, Debug)]
pub struct PluginCatalog {
    root: PathBuf,
    grants: PluginStore,
}

impl PluginCatalog {
    #[must_use]
    pub fn new(root: PathBuf, grant_path: PathBuf) -> Self {
        Self {
            root,
            grants: PluginStore::new(grant_path),
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Discovers bounded direct-child packages in stable identifier order.
    ///
    /// # Errors
    /// Returns a validation or storage error; invalid packages are never
    /// silently activated.
    pub fn packages(&self) -> Result<Vec<PluginPackage>, PluginError> {
        fs::create_dir_all(&self.root).map_err(PluginError::io)?;
        validate_plugin_directory(&self.root)?;
        let mut packages = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(PluginError::io)? {
            let entry = entry.map_err(PluginError::io)?;
            let metadata = entry.metadata().map_err(PluginError::io)?;
            if !metadata.is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            packages.push(PluginPackage::load(&entry.path())?);
            if packages.len() > MAX_INSTALLED_PLUGINS {
                return Err(limit("installedPlugins", MAX_INSTALLED_PLUGINS as u64));
            }
        }
        packages.sort_by(|left, right| left.manifest.id.cmp(&right.manifest.id));
        let mut ids = BTreeSet::new();
        let mut commands = BTreeSet::new();
        for package in &packages {
            if !ids.insert(&package.manifest.id) {
                return Err(PluginError::DuplicatePlugin(package.manifest.id.clone()));
            }
            for command in &package.manifest.contributes.commands {
                if !commands.insert(&command.command) {
                    return Err(PluginError::DuplicateCommand(command.command.clone()));
                }
            }
        }
        Ok(packages)
    }

    /// Returns bounded metadata and exact-digest approval state.
    ///
    /// # Errors
    /// Returns a package, validation, or grant-store error.
    pub fn list(&self) -> Result<Vec<PluginSummary>, PluginError> {
        self.packages()?
            .into_iter()
            .map(|package| {
                let activation = self.grants.activation_for(&package)?;
                Ok(summary(&package, activation.as_ref()))
            })
            .collect()
    }

    /// Loads one package by validated manifest identity.
    ///
    /// # Errors
    /// Returns a package or storage error.
    pub fn get(&self, plugin_id: &str) -> Result<Option<PluginPackage>, PluginError> {
        validate_id(plugin_id, "plugin")?;
        Ok(self
            .packages()?
            .into_iter()
            .find(|package| package.manifest.id == plugin_id))
    }

    /// Returns a grant only when it matches this exact package digest.
    ///
    /// # Errors
    /// Returns a grant-store validation or storage error.
    pub fn grant_for(&self, package: &PluginPackage) -> Result<Option<PluginGrant>, PluginError> {
        self.grants.grant_for(package)
    }

    /// Returns the approval and run-time switches for this exact package digest.
    ///
    /// # Errors
    /// Returns a grant-store validation or storage error.
    pub fn activation_for(
        &self,
        package: &PluginPackage,
    ) -> Result<Option<PluginActivation>, PluginError> {
        self.grants.activation_for(package)
    }

    /// Enables or disables an approved plugin.
    ///
    /// # Errors
    /// Returns an identifier validation or storage error.
    pub fn set_enabled(&self, plugin_id: &str, enabled: bool) -> Result<bool, PluginError> {
        self.grants.set_enabled(plugin_id, enabled)
    }

    /// Switches one capability family for an approved plugin.
    ///
    /// # Errors
    /// Returns an identifier validation or storage error.
    pub fn set_capability_enabled(
        &self,
        plugin_id: &str,
        capability: PluginCapability,
        enabled: bool,
    ) -> Result<bool, PluginError> {
        self.grants
            .set_capability_enabled(plugin_id, capability, enabled)
    }

    /// Persists an exact-digest grant after checking it against the request.
    ///
    /// # Errors
    /// Returns a grant validation or storage error.
    pub fn approve(&self, package: &PluginPackage, grant: PluginGrant) -> Result<(), PluginError> {
        self.grants.approve(package, grant)
    }

    /// Removes a persisted grant by validated plugin identifier.
    ///
    /// # Errors
    /// Returns an identifier validation or storage error.
    pub fn revoke(&self, plugin_id: &str) -> Result<bool, PluginError> {
        self.grants.revoke(plugin_id)
    }

    /// Installs a validated package by copying only its manifest and entry
    /// source into a newly created package directory. Existing IDs are never
    /// overwritten implicitly.
    ///
    /// # Errors
    /// Returns a validation, conflict, or storage error.
    pub fn install(&self, source_root: &Path) -> Result<PluginSummary, PluginError> {
        let package = PluginPackage::load(source_root)?;
        fs::create_dir_all(&self.root).map_err(PluginError::io)?;
        validate_plugin_directory(&self.root)?;
        let installed = self.packages()?;
        if installed.len() >= MAX_INSTALLED_PLUGINS {
            return Err(limit("installedPlugins", MAX_INSTALLED_PLUGINS as u64));
        }
        if installed
            .iter()
            .any(|installed| installed.manifest.id == package.manifest.id)
        {
            return Err(PluginError::AlreadyInstalled(package.manifest.id));
        }
        let destination = self.root.join(&package.manifest.id);
        if destination.exists() {
            return Err(PluginError::AlreadyInstalled(package.manifest.id));
        }
        let temporary = tempfile::Builder::new()
            .prefix(".plugin-install-")
            .tempdir_in(&self.root)
            .map_err(PluginError::io)?;
        let manifest_bytes = serde_json::to_vec_pretty(&package.manifest)
            .map_err(|error| PluginError::Malformed(error.to_string()))?;
        let installed_manifest = temporary.path().join("plugin.json");
        fs::write(&installed_manifest, manifest_bytes).map_err(PluginError::io)?;
        sync_file(&installed_manifest)?;
        if let (Some(main), Some(source)) = (&package.manifest.main, &package.source) {
            let entry = temporary.path().join(main);
            let entry_parent = entry.parent().ok_or(PluginError::MissingParent)?;
            fs::create_dir_all(entry_parent).map_err(PluginError::io)?;
            fs::write(&entry, source.as_bytes()).map_err(PluginError::io)?;
            sync_file(&entry)?;
            sync_parent(entry_parent)?;
        }
        fs::rename(temporary.path(), &destination).map_err(PluginError::io)?;
        sync_parent(&self.root)?;
        let installed = PluginPackage::load(&destination)?;
        Ok(summary(&installed, None))
    }

    /// Deletes one installed package after resolving its exact validated root.
    /// Its grant is revoked first so a failed later discovery cannot reactivate it.
    ///
    /// # Errors
    /// Returns a validation or storage error.
    pub fn uninstall(&self, plugin_id: &str) -> Result<bool, PluginError> {
        let Some(package) = self.get(plugin_id)? else {
            return Ok(false);
        };
        self.grants.revoke(plugin_id)?;
        fs::remove_dir_all(package.root()).map_err(PluginError::io)?;
        sync_parent(&self.root)?;
        Ok(true)
    }
}

fn summary(package: &PluginPackage, activation: Option<&PluginActivation>) -> PluginSummary {
    PluginSummary {
        id: package.manifest.id.clone(),
        name: package.manifest.name.clone(),
        version: package.manifest.version.clone(),
        digest: package.digest.clone(),
        api_version: package.manifest.api_version,
        requested_permissions: package.manifest.permissions.clone(),
        approved: activation.is_some(),
        enabled: activation.is_none_or(|activation| activation.enabled),
        disabled_capabilities: activation
            .map(|activation| activation.disabled_capabilities.clone())
            .unwrap_or_default(),
        commands: package.manifest.contributes.commands.clone(),
        languages: package
            .manifest
            .contributes
            .languages
            .iter()
            .map(|language| PluginLanguageSummary {
                id: language.id.clone(),
                display_name: language.display_name.clone(),
                extensions: language.extensions.clone(),
                filenames: language.filenames.clone(),
                capabilities: language.capabilities(),
            })
            .collect(),
    }
}

fn validate_language_id(value: &str) -> Result<(), PluginError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_LANGUAGE_ID_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        });
    if valid {
        Ok(())
    } else {
        Err(PluginError::InvalidLanguageId(value.into()))
    }
}

/// Accepts exactly the `files.associations` forms: `*.ext`, `name.*`, or a
/// whole file name. The syntax crate re-validates when it registers them.
fn validate_association(pattern: &str) -> Result<(), PluginError> {
    let lower = pattern.trim().to_ascii_lowercase();
    let (kind, value) = match (lower.strip_prefix("*."), lower.strip_suffix('*')) {
        (Some(extension), _) => ("extension", extension.to_owned()),
        (None, Some(prefix)) => ("prefix", prefix.to_owned()),
        (None, None) => ("file name", lower.clone()),
    };
    if value.is_empty()
        || value.len() > MAX_ASSOCIATION_BYTES
        || value.contains(['*', '?', '/', '\\'])
        || kind == "extension" && value.starts_with('.')
        || value.chars().any(char::is_control)
    {
        return Err(PluginError::InvalidAssociation(pattern.into()));
    }
    Ok(())
}

fn validate_regex(pattern: &str) -> Result<(), PluginError> {
    if pattern.is_empty() || pattern.len() > MAX_PLUGIN_REGEX_BYTES {
        return Err(limit("pluginRegexBytes", MAX_PLUGIN_REGEX_BYTES as u64));
    }
    regex::RegexBuilder::new(pattern)
        .size_limit(MAX_PLUGIN_REGEX_SIZE)
        .dfa_size_limit(MAX_PLUGIN_REGEX_SIZE)
        .build()
        .map(|_| ())
        .map_err(|error| PluginError::InvalidRegex(error.to_string()))
}

fn validate_comments(comments: &PluginComments) -> Result<(), PluginError> {
    if let Some(line) = &comments.line {
        validate_marker(line)?;
    }
    if let Some((open, close)) = &comments.block {
        validate_marker(open)?;
        validate_marker(close)?;
    }
    Ok(())
}

fn validate_marker(marker: &str) -> Result<(), PluginError> {
    if marker.is_empty()
        || marker.len() > 16
        || marker
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(PluginError::InvalidLanguageData(
            "a comment marker must be 1-16 visible bytes".into(),
        ));
    }
    Ok(())
}

fn validate_highlight(highlight: &PluginHighlight) -> Result<(), PluginError> {
    if highlight.keywords.len() > MAX_PLUGIN_LANGUAGE_WORDS
        || highlight.types.len() > MAX_PLUGIN_LANGUAGE_WORDS
    {
        return Err(limit(
            "pluginLanguageWords",
            MAX_PLUGIN_LANGUAGE_WORDS as u64,
        ));
    }
    for word in highlight.keywords.iter().chain(&highlight.types) {
        validate_text(word, MAX_WORD_BYTES, "pluginLanguageWord")?;
        if word.chars().any(char::is_whitespace) {
            return Err(PluginError::InvalidLanguageData(format!(
                "keyword {word:?} contains whitespace"
            )));
        }
    }
    if highlight.strings.len() > MAX_PLUGIN_LANGUAGE_STRINGS {
        return Err(limit(
            "pluginLanguageStrings",
            MAX_PLUGIN_LANGUAGE_STRINGS as u64,
        ));
    }
    for string in &highlight.strings {
        if string.open.is_empty()
            || string.close.is_empty()
            || string.open.len() > 8
            || string.close.len() > 8
            || string
                .open
                .chars()
                .chain(string.close.chars())
                .any(char::is_control)
        {
            return Err(PluginError::InvalidLanguageData(
                "a string delimiter must be 1-8 visible bytes".into(),
            ));
        }
    }
    for character in &highlight.ident_chars {
        if character.chars().count() != 1
            || !character
                .bytes()
                .all(|byte| byte.is_ascii() && !byte.is_ascii_alphanumeric())
        {
            return Err(PluginError::InvalidLanguageData(
                "an identifier character must be one non-alphanumeric ASCII byte".into(),
            ));
        }
    }
    Ok(())
}

fn validate_approval_document(document: &ApprovalDocument) -> Result<(), PluginError> {
    if document.schema_version != PLUGIN_STORE_SCHEMA_VERSION {
        return Err(PluginError::UnsupportedStoreSchema(document.schema_version));
    }
    if document.approvals.len() > MAX_INSTALLED_PLUGINS {
        return Err(limit("pluginApprovals", MAX_INSTALLED_PLUGINS as u64));
    }
    let mut ids = BTreeSet::new();
    for approval in &document.approvals {
        validate_id(&approval.plugin_id, "plugin")?;
        if approval.package_digest.len() != 64
            || !approval
                .package_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(PluginError::InvalidDigest);
        }
        if !ids.insert(&approval.plugin_id) {
            return Err(PluginError::DuplicatePlugin(approval.plugin_id.clone()));
        }
    }
    Ok(())
}

fn resolve_package_file(root: &Path, relative: &str) -> Result<PathBuf, PluginError> {
    let mut path = root.to_owned();
    for component in Path::new(relative).components() {
        let Component::Normal(component) = component else {
            return Err(PluginError::UnsafeEntryPath(relative.into()));
        };
        path.push(component);
        validate_regular_or_directory(&path, root)?;
    }
    validate_regular_file(&path, root)?;
    Ok(path)
}

fn validate_regular_or_directory(path: &Path, root: &Path) -> Result<(), PluginError> {
    if !path.starts_with(root) {
        return Err(PluginError::UnsafeEntryPath(path.display().to_string()));
    }
    let metadata = fs::symlink_metadata(path).map_err(PluginError::io)?;
    if metadata.file_type().is_symlink() {
        return Err(PluginError::UnsafeEntryPath(path.display().to_string()));
    }
    Ok(())
}

fn validate_plugin_directory(path: &Path) -> Result<(), PluginError> {
    let metadata = fs::symlink_metadata(path).map_err(PluginError::io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(PluginError::UnsafePackage(path.display().to_string()));
    }
    Ok(())
}

fn validate_regular_file(path: &Path, root: &Path) -> Result<(), PluginError> {
    validate_regular_or_directory(path, root)?;
    if !fs::symlink_metadata(path)
        .map_err(PluginError::io)?
        .is_file()
    {
        return Err(PluginError::UnsafeEntryPath(path.display().to_string()));
    }
    Ok(())
}

fn validate_entry_path(path: &str) -> Result<(), PluginError> {
    if path.is_empty() || path.len() > MAX_ENTRY_PATH_BYTES {
        return Err(PluginError::UnsafeEntryPath(path.into()));
    }
    if Path::new(path)
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(PluginError::UnsafeEntryPath(path.into()));
    }
    Ok(())
}

fn validate_id(value: &str, kind: &str) -> Result<(), PluginError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.contains('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(PluginError::InvalidId {
            kind: kind.into(),
            value: value.chars().take(MAX_ID_BYTES).collect(),
        })
    }
}

fn validate_text(value: &str, maximum: usize, resource: &str) -> Result<(), PluginError> {
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        Err(limit(resource, maximum as u64))
    } else {
        Ok(())
    }
}

fn limit(resource: &str, limit: u64) -> PluginError {
    PluginError::LimitExceeded {
        resource: resource.into(),
        limit,
    }
}

/// Flushes a file that was just written. The handle is opened for writing
/// because Windows refuses to flush one opened only for reading.
fn sync_file(path: &Path) -> Result<(), PluginError> {
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(PluginError::io)
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), PluginError> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(PluginError::io)
}

#[cfg(not(unix))]
// The signature matches the Unix implementation, which can fail.
#[allow(clippy::unnecessary_wraps)]
fn sync_parent(_parent: &Path) -> Result<(), PluginError> {
    Ok(())
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum PluginError {
    #[error("malformed plugin data: {0}")]
    Malformed(String),
    #[error("invalid {kind} ID: {value}")]
    InvalidId { kind: String, value: String },
    #[error("plugin entry path escapes its package: {0}")]
    UnsafeEntryPath(String),
    #[error("plugin declares no commands or languages")]
    NoContributions,
    #[error("plugin command contributions require a JavaScript entry point")]
    MissingEntry,
    #[error("invalid plugin language ID: {0}")]
    InvalidLanguageId(String),
    #[error("duplicate plugin language ID: {0}")]
    DuplicateLanguage(String),
    #[error("invalid language association: {0}")]
    InvalidAssociation(String),
    #[error("invalid declarative regex: {0}")]
    InvalidRegex(String),
    #[error("invalid declarative language data: {0}")]
    InvalidLanguageData(String),
    #[error("unsafe plugin package: {0}")]
    UnsafePackage(String),
    #[error("unsupported plugin entry type: {0}")]
    UnsupportedEntry(String),
    #[error("plugin source must be UTF-8")]
    InvalidSourceEncoding,
    #[error("plugin command {command} is outside namespace {plugin}")]
    CommandNamespace { plugin: String, command: String },
    #[error("plugin uses a reserved namespace: {0}")]
    ReservedNamespace(String),
    #[error("plugin version is not semantic: {0}")]
    InvalidVersion(String),
    #[error("plugin engine requirement is invalid: {0}")]
    InvalidEngine(String),
    #[error("plugin requires Nulis {required}, but this host is {current}")]
    UnsupportedEngine { required: String, current: String },
    #[error("duplicate plugin command: {0}")]
    DuplicateCommand(String),
    #[error("duplicate plugin ID: {0}")]
    DuplicatePlugin(String),
    #[error("plugin is already installed: {0}")]
    AlreadyInstalled(String),
    #[error("invalid plugin activation event: {0}")]
    InvalidActivation(String),
    #[error("plugin command has no matching activation event: {0}")]
    MissingCommandActivation(String),
    #[error("plugin grant exceeds its request: {0}")]
    GrantExceedsRequest(String),
    #[error("plugin permission denied: {0}")]
    PermissionDenied(String),
    #[error("unsupported plugin API version: {0}")]
    UnsupportedApi(u32),
    #[error("unsupported plugin grant-store schema: {0}")]
    UnsupportedStoreSchema(u32),
    #[error("invalid plugin package digest")]
    InvalidDigest,
    #[error("plugin execution failed: {0}")]
    Execution(String),
    #[error("plugin execution timed out after {0} ms")]
    TimedOut(u64),
    #[error("plugin execution cancelled")]
    Cancelled,
    #[error("plugin worker protocol failed")]
    WorkerProtocol,
    #[error("{resource} exceeds its {limit}-byte/count limit")]
    LimitExceeded { resource: String, limit: u64 },
    #[error("plugin store path has no parent")]
    MissingParent,
    #[error("plugin storage or process operation failed: {0}")]
    Io(String),
}

impl PluginError {
    #[allow(clippy::needless_pass_by_value)] // Required as a direct `map_err` adapter.
    fn io(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{PluginCatalog, PluginError, PluginGrant, PluginPermission, PluginStore};
    use std::{collections::BTreeSet, fs};

    fn write_package(root: &std::path::Path, source: &str) {
        fs::create_dir_all(root).expect("package root");
        fs::write(
            root.join("plugin.json"),
            r#"{
              "id":"nulis.example",
              "name":"Example",
              "version":"1.0.0",
              "engines":{"nulis":">=0.1.0"},
              "permissions":["showMessage"],
              "activationEvents":["onCommand:nulis.example.hello"],
              "main":"index.js",
              "contributes":{"commands":[{"command":"nulis.example.hello","title":"Hello"}]}
            }"#,
        )
        .expect("manifest");
        fs::write(root.join("index.js"), source).expect("source");
    }

    #[test]
    fn package_validation_rejects_paths_and_command_namespace_escape() {
        let root = tempfile::tempdir().expect("root");
        write_package(root.path(), "function activate() {}");
        let package = super::PluginPackage::load(root.path()).expect("valid package");
        assert_eq!(package.manifest.id, "nulis.example");

        let manifest = root.path().join("plugin.json");
        let unsafe_manifest = fs::read_to_string(&manifest)
            .expect("read")
            .replace("\"index.js\"", "\"../index.js\"");
        fs::write(&manifest, unsafe_manifest).expect("replace manifest");
        assert!(matches!(
            super::PluginPackage::load(root.path()),
            Err(PluginError::UnsafeEntryPath(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn package_loading_never_follows_symlinked_entries() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("root");
        let external = root.path().join("external.js");
        let package_root = root.path().join("package");
        write_package(&package_root, "function activate() {}");
        fs::write(&external, "function activate() { return 1; }").expect("external source");
        fs::remove_file(package_root.join("index.js")).expect("remove entry");
        symlink(&external, package_root.join("index.js")).expect("symlink entry");

        assert!(matches!(
            super::PluginPackage::load(&package_root),
            Err(PluginError::UnsafeEntryPath(_))
        ));
    }

    #[test]
    fn manifests_enforce_engine_compatibility_and_reserved_namespaces() {
        let root = tempfile::tempdir().expect("root");
        write_package(root.path(), "function activate() {}");
        let manifest = root.path().join("plugin.json");
        let original = fs::read_to_string(&manifest).expect("read manifest");
        fs::write(&manifest, original.replace(">=0.1.0", ">=99.0.0")).expect("incompatible engine");
        assert!(matches!(
            super::PluginPackage::load(root.path()),
            Err(PluginError::UnsupportedEngine { .. })
        ));

        fs::write(
            &manifest,
            original.replace("nulis.example", "automation.example"),
        )
        .expect("reserved namespace");
        assert!(matches!(
            super::PluginPackage::load(root.path()),
            Err(PluginError::ReservedNamespace(id)) if id == "automation.example"
        ));

        fs::write(
            &manifest,
            original.replace(r#"["onCommand:nulis.example.hello"]"#, "[]"),
        )
        .expect("missing activation");
        assert!(matches!(
            super::PluginPackage::load(root.path()),
            Err(PluginError::MissingCommandActivation(command))
                if command == "nulis.example.hello"
        ));
    }

    #[test]
    fn grants_are_digest_bound_and_cannot_exceed_requests() {
        let root = tempfile::tempdir().expect("root");
        write_package(root.path(), "function activate() {}");
        let package = super::PluginPackage::load(root.path()).expect("package");
        let store = PluginStore::new(root.path().join("grants.json"));
        let grant = PluginGrant {
            permissions: BTreeSet::from([PluginPermission::ShowMessage]),
        };
        store.approve(&package, grant.clone()).expect("approve");
        assert_eq!(store.grant_for(&package).expect("grant"), Some(grant));

        fs::write(
            root.path().join("index.js"),
            "function activate() { return 1; }",
        )
        .expect("change source");
        let changed = super::PluginPackage::load(root.path()).expect("changed package");
        assert_eq!(store.grant_for(&changed).expect("changed grant"), None);

        let manifest = root.path().join("plugin.json");
        let without_permission = fs::read_to_string(&manifest)
            .expect("manifest")
            .replace(r#"["showMessage"]"#, "[]");
        fs::write(&manifest, without_permission).expect("remove permission request");
        let restricted = super::PluginPackage::load(root.path()).expect("restricted package");
        assert!(matches!(
            store.approve(
                &restricted,
                PluginGrant {
                    permissions: BTreeSet::from([PluginPermission::ShowMessage])
                }
            ),
            Err(PluginError::GrantExceedsRequest(_))
        ));
    }

    #[test]
    fn catalog_is_stable_and_reports_exact_approval_state() {
        let root = tempfile::tempdir().expect("root");
        let packages = root.path().join("plugins");
        write_package(&packages.join("example"), "function activate() {}");
        let catalog = PluginCatalog::new(packages, root.path().join("grants.json"));
        assert!(!catalog.list().expect("list")[0].approved);
        let package = catalog.get("nulis.example").expect("get").expect("package");
        catalog
            .approve(
                &package,
                PluginGrant {
                    permissions: package.manifest.permissions.clone(),
                },
            )
            .expect("approve");
        assert!(catalog.list().expect("approved list")[0].approved);
        assert!(catalog.revoke("nulis.example").expect("revoke"));
    }

    #[test]
    fn catalog_installs_only_validated_files_without_overwriting_and_uninstalls() {
        let root = tempfile::tempdir().expect("root");
        let source = root.path().join("source");
        let installed = root.path().join("installed");
        write_package(&source, "function activate() {}");
        fs::write(source.join("ignored.txt"), "not installed").expect("extra file");
        let catalog = PluginCatalog::new(installed.clone(), root.path().join("grants.json"));

        let summary = catalog.install(&source).expect("install");
        assert_eq!(summary.id, "nulis.example");
        assert!(!summary.approved);
        let destination = installed.join("nulis.example");
        assert!(destination.join("plugin.json").is_file());
        assert!(destination.join("index.js").is_file());
        assert!(!destination.join("ignored.txt").exists());
        assert!(matches!(
            catalog.install(&source),
            Err(PluginError::AlreadyInstalled(id)) if id == "nulis.example"
        ));
        assert!(catalog.uninstall("nulis.example").expect("uninstall"));
        assert!(!destination.exists());
        assert!(!catalog
            .uninstall("nulis.example")
            .expect("missing uninstall"));
    }

    #[test]
    fn catalog_rejects_cross_plugin_command_collisions() {
        let root = tempfile::tempdir().expect("root");
        let packages = root.path().join("plugins");
        let first = packages.join("first");
        let second = packages.join("second");
        write_package(&first, "function activate() {}");
        write_package(&second, "function activate() {}");
        let first_manifest = fs::read_to_string(first.join("plugin.json"))
            .expect("first manifest")
            .replace("nulis.example.hello", "nulis.example.hello.hello");
        fs::write(first.join("plugin.json"), first_manifest).expect("first collision");
        let second_manifest = fs::read_to_string(second.join("plugin.json"))
            .expect("second manifest")
            .replace("nulis.example", "nulis.example.hello");
        fs::write(second.join("plugin.json"), second_manifest).expect("second collision");
        let catalog = PluginCatalog::new(packages, root.path().join("grants.json"));

        assert!(matches!(
            catalog.packages(),
            Err(PluginError::DuplicateCommand(command))
                if command == "nulis.example.hello.hello"
        ));
    }

    fn declarative_manifest() -> serde_json::Value {
        serde_json::json!({
          "apiVersion": 2,
          "id": "acme.nim",
          "name": "Nim language",
          "version": "2.0.0",
          "engines": { "nulis": ">=0.1.0" },
          "contributes": { "languages": [{
            "id": "nim",
            "displayName": "Nim",
            "extensions": ["nim", "nims"],
            "filenames": ["config.nim.cfg"],
            "firstLine": "^#!.*\\bnim\\b",
            "comments": { "line": "#", "block": ["#[", "]#"] },
            "brackets": [["(", ")"]],
            "highlight": {
              "mode": "code",
              "keywords": ["proc", "let", "var"],
              "types": ["int", "string"],
              "strings": [{ "open": "\"", "close": "\"", "escape": true }],
              "identChars": ["_"]
            },
            "indentation": {
              "increaseAfter": ":$",
              "outdentOnType": "^\\s*(elif|else|except|finally)\\b"
            },
            "completion": { "keywords": true, "words": true },
            "snippets": [{
              "prefix": "proc",
              "body": "proc ${1:name}() =\n  $0",
              "description": "Procedure"
            }]
          }] }
        })
    }

    fn write_declarative_package(root: &std::path::Path) {
        fs::create_dir_all(root).expect("package root");
        fs::write(
            root.join("plugin.json"),
            serde_json::to_vec_pretty(&declarative_manifest()).expect("manifest"),
        )
        .expect("write manifest");
    }

    #[test]
    fn a_v2_declarative_manifest_needs_no_entry_point() {
        let root = tempfile::tempdir().expect("root");
        write_declarative_package(root.path());
        let package = super::PluginPackage::load(root.path()).expect("valid package");
        assert_eq!(package.manifest.api_version, 2);
        assert!(package.manifest.main.is_none());
        assert!(package.source().is_none());
        assert!(matches!(
            package.worker_request(None, Vec::new()),
            Err(PluginError::MissingEntry)
        ));
        let language = &package.manifest.contributes.languages[0];
        assert_eq!(
            language.capabilities(),
            vec![
                super::PluginCapability::Languages,
                super::PluginCapability::Highlight,
                super::PluginCapability::Indentation,
                super::PluginCapability::Completion,
                super::PluginCapability::Snippets,
            ]
        );
        assert_eq!(
            package.manifest.capabilities(),
            BTreeSet::from([
                super::PluginCapability::Languages,
                super::PluginCapability::Highlight,
                super::PluginCapability::Indentation,
                super::PluginCapability::Completion,
                super::PluginCapability::Snippets,
            ])
        );
    }

    #[test]
    fn every_v1_manifest_keeps_its_meaning() {
        let root = tempfile::tempdir().expect("root");
        write_package(root.path(), "function activate() {}");
        let package = super::PluginPackage::load(root.path()).expect("v1 package");
        assert_eq!(package.manifest.api_version, 1);
        assert_eq!(package.manifest.main.as_deref(), Some("index.js"));
        assert!(package.worker_request(None, Vec::new()).is_ok());
    }

    fn manifest_from(value: serde_json::Value) -> Result<super::PluginManifest, PluginError> {
        let manifest: super::PluginManifest = serde_json::from_value(value)
            .map_err(|error| PluginError::Malformed(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    #[test]
    fn a_v2_manifest_without_an_entry_point_cannot_run_code() {
        let mut commands = declarative_manifest();
        commands["contributes"]
            .as_object_mut()
            .expect("contributions")
            .insert(
                "commands".into(),
                serde_json::json!([{ "command": "acme.nim.run", "title": "Run" }]),
            );
        assert!(matches!(
            manifest_from(commands),
            Err(PluginError::MissingEntry)
        ));

        let mut startup = declarative_manifest();
        startup["activationEvents"] = serde_json::json!(["onStartup"]);
        assert!(matches!(
            manifest_from(startup),
            Err(PluginError::MissingEntry)
        ));
    }

    #[test]
    fn hostile_declarative_languages_are_rejected_before_installation() {
        let cases = [
            (serde_json::json!({ "firstLine": "(" }), "invalid regex"),
            (
                serde_json::json!({ "firstLine": "a".repeat(super::MAX_PLUGIN_REGEX_BYTES + 1) }),
                "oversized regex",
            ),
            (
                serde_json::json!({ "extensions": ["../nim"] }),
                "escaping extension",
            ),
            (
                serde_json::json!({ "highlight": { "strings": [{ "open": "", "close": "\"" }] } }),
                "empty delimiter",
            ),
            (
                serde_json::json!({ "brackets": [["(", "("]] }),
                "mismatched bracket",
            ),
            (
                serde_json::json!({ "snippets": [{ "prefix": "x", "body": "a\u{0}b" }] }),
                "control character in a snippet",
            ),
        ];
        for (overrides, expected) in cases {
            let mut value = declarative_manifest();
            let language = value["contributes"]["languages"][0]
                .as_object_mut()
                .expect("language");
            for (key, replacement) in overrides.as_object().expect("overrides") {
                language.insert(key.clone(), replacement.clone());
            }
            assert!(
                matches!(
                    manifest_from(value),
                    Err(PluginError::InvalidRegex(_)
                        | PluginError::InvalidLanguageData(_)
                        | PluginError::InvalidAssociation(_)
                        | PluginError::LimitExceeded { .. })
                ),
                "{expected} must be rejected"
            );
        }

        let mut unknown = declarative_manifest();
        unknown["contributes"]["languages"][0]["script"] = serde_json::json!("main.lua");
        assert!(matches!(
            manifest_from(unknown),
            Err(PluginError::Malformed(_))
        ));

        let mut duplicate = declarative_manifest();
        let language = duplicate["contributes"]["languages"][0].clone();
        duplicate["contributes"]["languages"]
            .as_array_mut()
            .expect("languages")
            .push(language);
        assert!(matches!(
            manifest_from(duplicate),
            Err(PluginError::DuplicateLanguage(id)) if id == "nim"
        ));

        let mut unsupported = declarative_manifest();
        unsupported["apiVersion"] = serde_json::json!(3);
        assert!(matches!(
            manifest_from(unsupported),
            Err(PluginError::UnsupportedApi(3))
        ));
    }

    #[test]
    fn disabling_a_plugin_or_one_family_keeps_the_approval() {
        let root = tempfile::tempdir().expect("root");
        write_package(root.path(), "function activate() {}");
        let package = super::PluginPackage::load(root.path()).expect("package");
        let store = PluginStore::new(root.path().join("grants.json"));
        let grant = PluginGrant {
            permissions: BTreeSet::from([PluginPermission::ShowMessage]),
        };
        store.approve(&package, grant.clone()).expect("approve");
        let activation = store
            .activation_for(&package)
            .expect("activation")
            .expect("approved");
        assert!(activation.enabled);
        assert_eq!(activation.disabled_capabilities.len(), 0);

        assert!(store
            .set_capability_enabled("nulis.example", super::PluginCapability::Commands, false)
            .expect("disable commands"));
        assert!(store.set_enabled("nulis.example", false).expect("disable"));
        let disabled = store
            .activation_for(&package)
            .expect("activation")
            .expect("still approved");
        assert_eq!(disabled.grant, grant);
        assert!(!disabled.enabled);
        assert!(!disabled.capability_enabled(super::PluginCapability::Commands));
        assert!(disabled
            .disabled_capabilities
            .contains(&super::PluginCapability::Commands));
        assert!(!disabled
            .disabled_capabilities
            .contains(&super::PluginCapability::Languages));

        assert!(store.set_enabled("nulis.example", true).expect("enable"));
        let enabled = store
            .activation_for(&package)
            .expect("activation")
            .expect("approved");
        assert!(enabled.enabled);
        assert!(!enabled.capability_enabled(super::PluginCapability::Commands));
        assert!(store
            .set_capability_enabled("nulis.example", super::PluginCapability::Commands, true)
            .expect("enable commands"));
        assert!(store
            .activation_for(&package)
            .expect("activation")
            .expect("approved")
            .capability_enabled(super::PluginCapability::Commands));

        // A document written before v2 has no switches and still means active.
        let path = root.path().join("grants.json");
        let mut document: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("grants")).expect("document");
        document["approvals"][0]
            .as_object_mut()
            .expect("approval")
            .remove("enabled");
        document["approvals"][0]
            .as_object_mut()
            .expect("approval")
            .remove("disabledCapabilities");
        fs::write(&path, serde_json::to_vec(&document).expect("serialize")).expect("write");
        let legacy = store
            .activation_for(&package)
            .expect("activation")
            .expect("approved");
        assert!(legacy.enabled);
        assert_eq!(legacy.disabled_capabilities.len(), 0);
    }

    #[test]
    fn a_declarative_plugin_summarizes_its_languages() {
        let root = tempfile::tempdir().expect("root");
        let package = root.path().join("nim");
        write_declarative_package(&package);
        let catalog =
            PluginCatalog::new(root.path().join("plugins"), root.path().join("grants.json"));
        let installed = catalog.install(&package).expect("install");
        assert_eq!(installed.api_version, 2);
        assert_eq!(installed.languages.len(), 1);
        assert_eq!(installed.languages[0].id, "nim");
        assert_eq!(installed.languages[0].display_name, "Nim");
        assert_eq!(installed.languages[0].extensions, ["nim", "nims"]);
        assert!(installed.languages[0]
            .capabilities
            .contains(&super::PluginCapability::Snippets));
        assert_eq!(installed.commands.len(), 0);
        assert!(!installed.approved);

        let stored = catalog.get("acme.nim").expect("get").expect("package");
        catalog
            .approve(&stored, PluginGrant::default())
            .expect("approve");
        assert!(catalog
            .set_capability_enabled("acme.nim", super::PluginCapability::Languages, false)
            .expect("switch language off"));
        let summary = catalog.list().expect("list").remove(0);
        assert!(summary
            .disabled_capabilities
            .contains(&super::PluginCapability::Languages));
    }

    #[test]
    fn the_declarative_example_package_is_installable_as_is() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/plugins/nulis-nim");
        let package = super::PluginPackage::load(&root).expect("example package");
        assert_eq!(package.manifest.api_version, 2);
        assert!(package.manifest.main.is_none());
        let language = &package.manifest.contributes.languages[0];
        assert_eq!(language.id, "nim");
        assert_eq!(language.display_name, "Nim");
        assert_eq!(language.extensions, ["nim", "nims"]);
        assert!(language
            .capabilities()
            .contains(&super::PluginCapability::Snippets));
    }
}
