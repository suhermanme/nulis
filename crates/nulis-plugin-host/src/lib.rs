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

pub const PLUGIN_API_VERSION: u32 = 1;
pub const PLUGIN_STORE_SCHEMA_VERSION: u32 = 1;
pub const MAX_INSTALLED_PLUGINS: usize = 128;
pub const MAX_PLUGIN_MANIFEST_BYTES: u64 = 262_144;
pub const MAX_PLUGIN_SOURCE_BYTES: usize = 1_048_576;
pub const MAX_PLUGIN_ARGUMENT_BYTES: usize = 262_144;
pub const MAX_PLUGIN_RESPONSE_BYTES: usize = 1_048_576;
pub const MAX_PLUGIN_COMMANDS: usize = 256;
pub const MAX_PLUGIN_OPERATIONS: usize = 256;
pub const MAX_PLUGIN_MESSAGE_BYTES: usize = 65_536;
const MAX_ID_BYTES: usize = 128;
const MAX_NAME_BYTES: usize = 128;
const MAX_VERSION_BYTES: usize = 64;
const MAX_ENTRY_PATH_BYTES: usize = 4_096;
const MAX_GRANT_STORE_BYTES: u64 = 1_048_576;

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

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginContributions {
    #[serde(default)]
    pub commands: Vec<PluginCommandContribution>,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub engines: PluginEngines,
    #[serde(default)]
    pub permissions: BTreeSet<PluginPermission>,
    #[serde(default)]
    pub activation_events: BTreeSet<String>,
    pub main: String,
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
        validate_entry_path(&self.main)?;
        if !Path::new(&self.main)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("js"))
        {
            return Err(PluginError::UnsupportedEntry(self.main.clone()));
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
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PluginPackage {
    root: PathBuf,
    source: String,
    pub manifest: PluginManifest,
    pub digest: String,
}

impl PluginPackage {
    /// Loads exactly one manifest and JavaScript entry point from a package
    /// directory without following symlinks.
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
        let entry = resolve_package_file(&root, &manifest.main)?;
        let entry_metadata = fs::metadata(&entry).map_err(PluginError::io)?;
        if entry_metadata.len() > MAX_PLUGIN_SOURCE_BYTES as u64 {
            return Err(limit("pluginSourceBytes", MAX_PLUGIN_SOURCE_BYTES as u64));
        }
        let source_bytes = fs::read(entry).map_err(PluginError::io)?;
        let source = String::from_utf8(source_bytes.clone())
            .map_err(|_| PluginError::InvalidSourceEncoding)?;
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
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn worker_request(
        &self,
        command: Option<String>,
        arguments: Vec<Value>,
    ) -> PluginWorkerRequest {
        PluginWorkerRequest {
            api_version: PLUGIN_API_VERSION,
            source: self.source.clone(),
            command,
            arguments,
        }
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
        if self.api_version != PLUGIN_API_VERSION {
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
    pub requested_permissions: BTreeSet<PluginPermission>,
    pub approved: bool,
    pub commands: Vec<PluginCommandContribution>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginApproval {
    plugin_id: String,
    package_digest: String,
    grant: PluginGrant,
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

    /// Returns a grant only when it matches the exact package digest.
    ///
    /// # Errors
    /// Returns a validation or storage error.
    pub fn grant_for(&self, package: &PluginPackage) -> Result<Option<PluginGrant>, PluginError> {
        Ok(self
            .load()?
            .approvals
            .into_iter()
            .find(|approval| {
                approval.plugin_id == package.manifest.id
                    && approval.package_digest == package.digest
            })
            .map(|approval| approval.grant))
    }

    /// Atomically records an explicit grant for one exact package revision.
    ///
    /// # Errors
    /// Returns a validation or storage error.
    pub fn approve(&self, package: &PluginPackage, grant: PluginGrant) -> Result<(), PluginError> {
        grant.validate_for(&package.manifest)?;
        let mut document = self.load()?;
        document
            .approvals
            .retain(|approval| approval.plugin_id != package.manifest.id);
        document.approvals.push(PluginApproval {
            plugin_id: package.manifest.id.clone(),
            package_digest: package.digest.clone(),
            grant,
        });
        document
            .approvals
            .sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));
        self.save(&document)
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
                let approved = self.grants.grant_for(&package)?.is_some();
                Ok(PluginSummary {
                    id: package.manifest.id,
                    name: package.manifest.name,
                    version: package.manifest.version,
                    digest: package.digest,
                    requested_permissions: package.manifest.permissions,
                    approved,
                    commands: package.manifest.contributes.commands,
                })
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
        let entry = temporary.path().join(&package.manifest.main);
        let entry_parent = entry.parent().ok_or(PluginError::MissingParent)?;
        fs::create_dir_all(entry_parent).map_err(PluginError::io)?;
        fs::write(&entry, package.source.as_bytes()).map_err(PluginError::io)?;
        sync_file(&entry)?;
        sync_parent(entry_parent)?;
        fs::rename(temporary.path(), &destination).map_err(PluginError::io)?;
        sync_parent(&self.root)?;
        let installed = PluginPackage::load(&destination)?;
        Ok(PluginSummary {
            id: installed.manifest.id,
            name: installed.manifest.name,
            version: installed.manifest.version,
            digest: installed.digest,
            requested_permissions: installed.manifest.permissions,
            approved: false,
            commands: installed.manifest.contributes.commands,
        })
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
}
