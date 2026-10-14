//! Bounded recorded macros and capability-gated sandboxed scripts.

use rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, Position};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(unix)]
use std::fs::File;
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};
use thiserror::Error;

pub const AUTOMATION_SCHEMA_VERSION: u32 = 1;
pub const SCRIPT_API_VERSION: u32 = 1;
pub const MAX_MACRO_ACTIONS: usize = 1_024;
pub const MAX_MACRO_BYTES: usize = 1_048_576;
pub const MAX_ARGUMENT_BYTES: usize = 262_144;
pub const MAX_JSON_DEPTH: usize = 32;
pub const MAX_SCRIPT_SOURCE_BYTES: usize = 1_048_576;
pub const MAX_SCRIPT_MEMORY_BYTES: u64 = 64 * 1_048_576;
pub const MAX_SCRIPT_EXECUTION_MILLIS: u64 = 10_000;
pub const MAX_SCRIPT_OPERATIONS: u64 = 100_000_000;
pub const MAX_SCRIPT_HOST_CALLS: u32 = 10_000;
pub const MAX_SCRIPT_OUTPUT_BYTES: u64 = 1_048_576;
pub const MAX_SCRIPT_COMMANDS: usize = 256;
pub const MAX_SAVED_MACROS: usize = 128;
pub const MAX_MACRO_LIBRARY_BYTES: u64 = 1_048_576;
pub const MAX_SAVED_SCRIPTS: usize = 64;
pub const MAX_SCRIPT_LIBRARY_BYTES: u64 = 4 * 1_048_576;
pub const CURRENT_REVISION_ARGUMENT: &str = "$nulis.currentRevision";

const MAX_ID_BYTES: usize = 128;
const MAX_NAME_BYTES: usize = 128;
const MAX_FAILURE_BYTES: usize = 1_024;
const SCRIPT_MAX_VARIABLES: usize = 32;
const SCRIPT_MAX_EXPR_DEPTH: usize = 16;
const SCRIPT_MAX_INTERNED_STRINGS: usize = 256;
const TERMINATION_NONE: u8 = 0;
const TERMINATION_CANCELLED: u8 = 1;
const TERMINATION_TIMED_OUT: u8 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordedCommand {
    pub command: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MacroDefinition {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub actions: Vec<RecordedCommand>,
}

impl MacroDefinition {
    /// Parses a macro only after enforcing the serialized input budget.
    ///
    /// # Errors
    /// Returns a typed validation error for oversized, malformed, or invalid
    /// macro data.
    pub fn from_json(bytes: &[u8]) -> Result<Self, AutomationError> {
        if bytes.len() > MAX_MACRO_BYTES {
            return Err(limit("macroBytes", MAX_MACRO_BYTES as u64));
        }
        let definition: Self = serde_json::from_slice(bytes)
            .map_err(|error| AutomationError::Malformed(error.to_string()))?;
        definition.validate()?;
        Ok(definition)
    }

    /// Validates identifiers and all count, nesting, and byte budgets.
    ///
    /// # Errors
    /// Returns a typed validation error when any invariant is violated.
    pub fn validate(&self) -> Result<(), AutomationError> {
        if self.schema_version != AUTOMATION_SCHEMA_VERSION {
            return Err(AutomationError::UnsupportedSchema(self.schema_version));
        }
        validate_id(&self.id, "macro")?;
        validate_name(&self.name)?;
        if self.actions.is_empty() {
            return Err(AutomationError::EmptyMacro);
        }
        if self.actions.len() > MAX_MACRO_ACTIONS {
            return Err(limit("macroActions", MAX_MACRO_ACTIONS as u64));
        }

        let mut aggregate = self.id.len().saturating_add(self.name.len());
        for action in &self.actions {
            validate_id(&action.command, "command")?;
            let argument_bytes = bounded_json_size(&action.arguments, MAX_ARGUMENT_BYTES)?;
            aggregate = aggregate
                .saturating_add(action.command.len())
                .saturating_add(argument_bytes);
            if aggregate > MAX_MACRO_BYTES {
                return Err(limit("macroBytes", MAX_MACRO_BYTES as u64));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MacroSummary {
    pub id: String,
    pub name: String,
    pub action_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordingStatus {
    pub recording: bool,
    pub id: Option<String>,
    pub name: Option<String>,
    pub action_count: usize,
    pub failed: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MacroLibrary {
    schema_version: u32,
    macros: Vec<MacroDefinition>,
}

impl Default for MacroLibrary {
    fn default() -> Self {
        Self {
            schema_version: AUTOMATION_SCHEMA_VERSION,
            macros: Vec::new(),
        }
    }
}

impl MacroLibrary {
    fn validate(&self) -> Result<(), AutomationError> {
        if self.schema_version != AUTOMATION_SCHEMA_VERSION {
            return Err(AutomationError::UnsupportedSchema(self.schema_version));
        }
        if self.macros.len() > MAX_SAVED_MACROS {
            return Err(limit("savedMacros", MAX_SAVED_MACROS as u64));
        }
        let mut ids = BTreeSet::new();
        for definition in &self.macros {
            definition.validate()?;
            if !ids.insert(&definition.id) {
                return Err(AutomationError::DuplicateMacro(definition.id.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct MacroStore {
    path: PathBuf,
}

impl MacroStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Returns bounded metadata for all saved macros in stable identifier order.
    ///
    /// # Errors
    /// Returns a validation or storage error for invalid persisted data.
    pub fn list(&self) -> Result<Vec<MacroSummary>, AutomationError> {
        let mut definitions = self.load()?.macros;
        definitions.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(definitions
            .into_iter()
            .map(|definition| MacroSummary {
                id: definition.id,
                name: definition.name,
                action_count: definition.actions.len(),
            })
            .collect())
    }

    /// Loads one validated macro by exact identifier.
    ///
    /// # Errors
    /// Returns a validation or storage error for invalid persisted data.
    pub fn get(&self, id: &str) -> Result<Option<MacroDefinition>, AutomationError> {
        validate_id(id, "macro")?;
        Ok(self
            .load()?
            .macros
            .into_iter()
            .find(|definition| definition.id == id))
    }

    /// Atomically inserts or replaces one validated macro.
    ///
    /// # Errors
    /// Returns an error when the definition or resulting library exceeds a
    /// limit, or when durable persistence fails.
    pub fn upsert(&self, definition: MacroDefinition) -> Result<(), AutomationError> {
        definition.validate()?;
        let mut library = self.load()?;
        if let Some(existing) = library
            .macros
            .iter_mut()
            .find(|existing| existing.id == definition.id)
        {
            *existing = definition;
        } else {
            library.macros.push(definition);
        }
        library.macros.sort_by(|left, right| left.id.cmp(&right.id));
        self.save(&library)
    }

    /// Removes one macro and atomically persists the remaining library.
    ///
    /// # Errors
    /// Returns a validation or storage error for invalid persisted data.
    pub fn delete(&self, id: &str) -> Result<bool, AutomationError> {
        validate_id(id, "macro")?;
        let mut library = self.load()?;
        let previous_len = library.macros.len();
        library.macros.retain(|definition| definition.id != id);
        let deleted = library.macros.len() != previous_len;
        if deleted {
            self.save(&library)?;
        }
        Ok(deleted)
    }

    fn load(&self) -> Result<MacroLibrary, AutomationError> {
        let metadata = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(MacroLibrary::default());
            }
            Err(error) => return Err(AutomationError::io(error)),
        };
        if metadata.len() > MAX_MACRO_LIBRARY_BYTES {
            return Err(limit("macroLibraryBytes", MAX_MACRO_LIBRARY_BYTES));
        }
        let bytes = fs::read(&self.path).map_err(AutomationError::io)?;
        let library: MacroLibrary = serde_json::from_slice(&bytes)
            .map_err(|error| AutomationError::Malformed(error.to_string()))?;
        library.validate()?;
        Ok(library)
    }

    fn save(&self, library: &MacroLibrary) -> Result<(), AutomationError> {
        library.validate()?;
        let bytes = serde_json::to_vec(library)
            .map_err(|error| AutomationError::Malformed(error.to_string()))?;
        let byte_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if byte_len > MAX_MACRO_LIBRARY_BYTES {
            return Err(limit("macroLibraryBytes", MAX_MACRO_LIBRARY_BYTES));
        }
        let parent = self.path.parent().ok_or(AutomationError::MissingParent)?;
        fs::create_dir_all(parent).map_err(AutomationError::io)?;
        let mut temporary = tempfile::Builder::new()
            .prefix("macros-")
            .suffix(".json.tmp")
            .tempfile_in(parent)
            .map_err(AutomationError::io)?;
        temporary.write_all(&bytes).map_err(AutomationError::io)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(AutomationError::io)?;
        temporary
            .persist(&self.path)
            .map_err(|error| AutomationError::io(error.error))?;
        sync_parent(parent)
    }
}

#[derive(Clone, Debug, Default)]
pub struct MacroRecorder {
    active: Option<ActiveRecording>,
}

#[derive(Clone, Debug)]
struct ActiveRecording {
    definition: MacroDefinition,
    failure: Option<AutomationError>,
}

impl MacroRecorder {
    /// Starts a new empty recording. Only one recording may be active.
    ///
    /// # Errors
    /// Returns an identity validation error or [`AutomationError::RecordingActive`].
    pub fn start(&mut self, id: String, name: String) -> Result<(), AutomationError> {
        if self.active.is_some() {
            return Err(AutomationError::RecordingActive);
        }
        validate_id(&id, "macro")?;
        validate_name(&name)?;
        self.active = Some(ActiveRecording {
            definition: MacroDefinition {
                schema_version: AUTOMATION_SCHEMA_VERSION,
                id,
                name,
                actions: Vec::new(),
            },
            failure: None,
        });
        Ok(())
    }

    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.active.is_some()
    }

    #[must_use]
    pub fn status(&self) -> RecordingStatus {
        let Some(active) = &self.active else {
            return RecordingStatus {
                recording: false,
                id: None,
                name: None,
                action_count: 0,
                failed: None,
            };
        };
        RecordingStatus {
            recording: true,
            id: Some(active.definition.id.clone()),
            name: Some(active.definition.name.clone()),
            action_count: active.definition.actions.len(),
            failed: active.failure.as_ref().map(ToString::to_string),
        }
    }

    /// Appends one command to the active recording. Calls made while idle are
    /// ignored. Once a limit is exceeded, the recording retains its prior
    /// actions and reports the failure until cancelled.
    ///
    /// # Errors
    /// Returns the first validation failure for the active recording.
    pub fn record(&mut self, command: RecordedCommand) -> Result<bool, AutomationError> {
        let Some(active) = &mut self.active else {
            return Ok(false);
        };
        if let Some(error) = &active.failure {
            return Err(error.clone());
        }
        active.definition.actions.push(command);
        if let Err(error) = active.definition.validate() {
            active.definition.actions.pop();
            active.failure = Some(error.clone());
            return Err(error);
        }
        Ok(true)
    }

    /// Returns a validated snapshot without ending the recording.
    ///
    /// # Errors
    /// Returns an error while idle, after overflow, or for an empty macro.
    pub fn snapshot(&self) -> Result<MacroDefinition, AutomationError> {
        let active = self
            .active
            .as_ref()
            .ok_or(AutomationError::RecordingInactive)?;
        if let Some(error) = &active.failure {
            return Err(error.clone());
        }
        active.definition.validate()?;
        Ok(active.definition.clone())
    }

    pub fn cancel(&mut self) -> bool {
        self.active.take().is_some()
    }

    /// Ends the active recording and returns its validated definition.
    ///
    /// # Errors
    /// Returns an error while idle, after overflow, or for an empty macro.
    pub fn finish(&mut self) -> Result<MacroDefinition, AutomationError> {
        let definition = self.snapshot()?;
        self.active = None;
        Ok(definition)
    }
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), AutomationError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(AutomationError::io)
}

#[cfg(not(unix))]
// The signature matches the Unix implementation, which can fail.
#[allow(clippy::unnecessary_wraps)]
fn sync_parent(_parent: &Path) -> Result<(), AutomationError> {
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SandboxCapability {
    ExecuteCommand,
    ReadDocument,
    EditDocument,
    ReadWorkspace,
    WriteWorkspace,
    ReadSettings,
    ShowMessage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostOperation<'a> {
    ExecuteCommand(&'a str),
    ReadDocument,
    EditDocument,
    ReadWorkspace,
    WriteWorkspace,
    ReadSettings,
    ShowMessage,
}

impl HostOperation<'_> {
    #[must_use]
    pub const fn required_capability(&self) -> SandboxCapability {
        match self {
            Self::ExecuteCommand(_) => SandboxCapability::ExecuteCommand,
            Self::ReadDocument => SandboxCapability::ReadDocument,
            Self::EditDocument => SandboxCapability::EditDocument,
            Self::ReadWorkspace => SandboxCapability::ReadWorkspace,
            Self::WriteWorkspace => SandboxCapability::WriteWorkspace,
            Self::ReadSettings => SandboxCapability::ReadSettings,
            Self::ShowMessage => SandboxCapability::ShowMessage,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SandboxLimits {
    pub memory_bytes: u64,
    pub execution_millis: u64,
    pub operations: u64,
    pub host_calls: u32,
    pub output_bytes: u64,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            memory_bytes: 16 * 1_048_576,
            execution_millis: 1_000,
            operations: 5_000_000,
            host_calls: 1_000,
            output_bytes: 65_536,
        }
    }
}

impl SandboxLimits {
    /// Ensures every runtime budget is nonzero and below its hard ceiling.
    ///
    /// # Errors
    /// Returns a limit error naming the invalid resource.
    pub fn validate(&self) -> Result<(), AutomationError> {
        validate_budget(
            "scriptMemoryBytes",
            self.memory_bytes,
            MAX_SCRIPT_MEMORY_BYTES,
        )?;
        validate_budget(
            "scriptExecutionMillis",
            self.execution_millis,
            MAX_SCRIPT_EXECUTION_MILLIS,
        )?;
        validate_budget("scriptOperations", self.operations, MAX_SCRIPT_OPERATIONS)?;
        validate_budget(
            "scriptHostCalls",
            u64::from(self.host_calls),
            u64::from(MAX_SCRIPT_HOST_CALLS),
        )?;
        validate_budget(
            "scriptOutputBytes",
            self.output_bytes,
            MAX_SCRIPT_OUTPUT_BYTES,
        )
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SandboxPolicy {
    #[serde(default)]
    pub capabilities: BTreeSet<SandboxCapability>,
    #[serde(default)]
    pub allowed_commands: BTreeSet<String>,
    pub limits: SandboxLimits,
}

impl SandboxPolicy {
    /// Validates the grant. The default remains deny-all.
    ///
    /// # Errors
    /// Returns an error for invalid budgets, command identifiers, or an
    /// oversized command allowlist.
    pub fn validate(&self) -> Result<(), AutomationError> {
        self.limits.validate()?;
        if self.allowed_commands.len() > MAX_SCRIPT_COMMANDS {
            return Err(limit("scriptAllowedCommands", MAX_SCRIPT_COMMANDS as u64));
        }
        for command in &self.allowed_commands {
            validate_id(command, "command")?;
        }
        if !self
            .capabilities
            .contains(&SandboxCapability::ExecuteCommand)
            && !self.allowed_commands.is_empty()
        {
            return Err(AutomationError::InconsistentPolicy(
                "allowedCommands requires executeCommand capability".into(),
            ));
        }
        Ok(())
    }

    /// Authorizes one narrow host operation. Commands require both the
    /// capability and an exact allowlist match.
    ///
    /// # Errors
    /// Returns [`AutomationError::PermissionDenied`] when authority was not
    /// explicitly granted.
    pub fn authorize(&self, operation: &HostOperation<'_>) -> Result<(), AutomationError> {
        self.validate()?;
        let capability = operation.required_capability();
        if !self.capabilities.contains(&capability) {
            return Err(AutomationError::PermissionDenied(format!(
                "capability {capability:?}"
            )));
        }
        if let HostOperation::ExecuteCommand(command) = operation {
            if !self.allowed_commands.contains(*command) {
                return Err(AutomationError::PermissionDenied(format!(
                    "command {command}"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptDefinition {
    pub api_version: u32,
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub requested_capabilities: BTreeSet<SandboxCapability>,
    #[serde(default)]
    pub requested_commands: BTreeSet<String>,
}

impl ScriptDefinition {
    /// Validates a script request without granting any requested authority.
    ///
    /// # Errors
    /// Returns a typed error for an unsupported API version or exceeded input
    /// budget.
    pub fn validate(&self) -> Result<(), AutomationError> {
        if self.api_version != SCRIPT_API_VERSION {
            return Err(AutomationError::UnsupportedScriptApi(self.api_version));
        }
        validate_name(&self.name)?;
        if self.source.len() > MAX_SCRIPT_SOURCE_BYTES {
            return Err(limit("scriptSourceBytes", MAX_SCRIPT_SOURCE_BYTES as u64));
        }
        if self.requested_commands.len() > MAX_SCRIPT_COMMANDS {
            return Err(limit("scriptRequestedCommands", MAX_SCRIPT_COMMANDS as u64));
        }
        for command in &self.requested_commands {
            validate_id(command, "command")?;
        }
        if !self
            .requested_capabilities
            .contains(&SandboxCapability::ExecuteCommand)
            && !self.requested_commands.is_empty()
        {
            return Err(AutomationError::InconsistentPolicy(
                "requestedCommands requires executeCommand capability".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredScript {
    pub id: String,
    pub definition: ScriptDefinition,
    #[serde(default)]
    pub approved_policy: Option<SandboxPolicy>,
}

impl StoredScript {
    fn validate(&self) -> Result<(), AutomationError> {
        validate_id(&self.id, "script")?;
        self.definition.validate()?;
        if let Some(policy) = &self.approved_policy {
            policy.validate()?;
            validate_script_grant(&self.definition, policy)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptSummary {
    pub id: String,
    pub name: String,
    pub requested_capabilities: BTreeSet<SandboxCapability>,
    pub requested_commands: BTreeSet<String>,
    pub approved: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScriptLibrary {
    schema_version: u32,
    scripts: Vec<StoredScript>,
}

impl Default for ScriptLibrary {
    fn default() -> Self {
        Self {
            schema_version: AUTOMATION_SCHEMA_VERSION,
            scripts: Vec::new(),
        }
    }
}

impl ScriptLibrary {
    fn validate(&self) -> Result<(), AutomationError> {
        if self.schema_version != AUTOMATION_SCHEMA_VERSION {
            return Err(AutomationError::UnsupportedSchema(self.schema_version));
        }
        if self.scripts.len() > MAX_SAVED_SCRIPTS {
            return Err(limit("savedScripts", MAX_SAVED_SCRIPTS as u64));
        }
        let mut ids = BTreeSet::new();
        for script in &self.scripts {
            script.validate()?;
            if !ids.insert(&script.id) {
                return Err(AutomationError::DuplicateScript(script.id.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ScriptStore {
    path: PathBuf,
}

impl ScriptStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Returns script metadata without exposing source text.
    ///
    /// # Errors
    /// Returns a validation or storage error for invalid persisted data.
    pub fn list(&self) -> Result<Vec<ScriptSummary>, AutomationError> {
        let mut scripts = self.load()?.scripts;
        scripts.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(scripts
            .into_iter()
            .map(|script| ScriptSummary {
                id: script.id,
                name: script.definition.name,
                requested_capabilities: script.definition.requested_capabilities,
                requested_commands: script.definition.requested_commands,
                approved: script.approved_policy.is_some(),
            })
            .collect())
    }

    /// Loads one validated script and its separate approval grant.
    ///
    /// # Errors
    /// Returns a validation or storage error for invalid persisted data.
    pub fn get(&self, id: &str) -> Result<Option<StoredScript>, AutomationError> {
        validate_id(id, "script")?;
        Ok(self
            .load()?
            .scripts
            .into_iter()
            .find(|script| script.id == id))
    }

    /// Saves a definition. Any change to an existing definition revokes its
    /// prior approval; an identical save preserves it.
    ///
    /// # Errors
    /// Returns a validation, limit, or storage error.
    pub fn upsert(&self, id: String, definition: ScriptDefinition) -> Result<(), AutomationError> {
        validate_id(&id, "script")?;
        definition.validate()?;
        let mut library = self.load()?;
        if let Some(existing) = library.scripts.iter_mut().find(|script| script.id == id) {
            if existing.definition != definition {
                existing.approved_policy = None;
            }
            existing.definition = definition;
        } else {
            library.scripts.push(StoredScript {
                id,
                definition,
                approved_policy: None,
            });
        }
        library
            .scripts
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.save(&library)
    }

    /// Persists a grant only after proving it is a subset of the script request.
    ///
    /// # Errors
    /// Returns an error for an unknown script or invalid/excessive grant.
    pub fn approve(&self, id: &str, policy: SandboxPolicy) -> Result<(), AutomationError> {
        validate_id(id, "script")?;
        policy.validate()?;
        let mut library = self.load()?;
        let script = library
            .scripts
            .iter_mut()
            .find(|script| script.id == id)
            .ok_or_else(|| AutomationError::UnknownScript(id.into()))?;
        validate_script_grant(&script.definition, &policy)?;
        script.approved_policy = Some(policy);
        self.save(&library)
    }

    /// Removes a persisted grant without changing the script.
    ///
    /// # Errors
    /// Returns a validation or storage error.
    pub fn revoke(&self, id: &str) -> Result<bool, AutomationError> {
        validate_id(id, "script")?;
        let mut library = self.load()?;
        let Some(script) = library.scripts.iter_mut().find(|script| script.id == id) else {
            return Ok(false);
        };
        let revoked = script.approved_policy.take().is_some();
        if revoked {
            self.save(&library)?;
        }
        Ok(revoked)
    }

    /// Removes one script and its grant.
    ///
    /// # Errors
    /// Returns a validation or storage error.
    pub fn delete(&self, id: &str) -> Result<bool, AutomationError> {
        validate_id(id, "script")?;
        let mut library = self.load()?;
        let previous = library.scripts.len();
        library.scripts.retain(|script| script.id != id);
        let deleted = library.scripts.len() != previous;
        if deleted {
            self.save(&library)?;
        }
        Ok(deleted)
    }

    fn load(&self) -> Result<ScriptLibrary, AutomationError> {
        let metadata = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ScriptLibrary::default());
            }
            Err(error) => return Err(AutomationError::io(error)),
        };
        if metadata.len() > MAX_SCRIPT_LIBRARY_BYTES {
            return Err(limit("scriptLibraryBytes", MAX_SCRIPT_LIBRARY_BYTES));
        }
        let bytes = fs::read(&self.path).map_err(AutomationError::io)?;
        let library: ScriptLibrary = serde_json::from_slice(&bytes)
            .map_err(|error| AutomationError::Malformed(error.to_string()))?;
        library.validate()?;
        Ok(library)
    }

    fn save(&self, library: &ScriptLibrary) -> Result<(), AutomationError> {
        library.validate()?;
        let bytes = serde_json::to_vec(library)
            .map_err(|error| AutomationError::Malformed(error.to_string()))?;
        let byte_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if byte_len > MAX_SCRIPT_LIBRARY_BYTES {
            return Err(limit("scriptLibraryBytes", MAX_SCRIPT_LIBRARY_BYTES));
        }
        let parent = self.path.parent().ok_or(AutomationError::MissingParent)?;
        fs::create_dir_all(parent).map_err(AutomationError::io)?;
        let mut temporary = tempfile::Builder::new()
            .prefix("scripts-")
            .suffix(".json.tmp")
            .tempfile_in(parent)
            .map_err(AutomationError::io)?;
        temporary.write_all(&bytes).map_err(AutomationError::io)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(AutomationError::io)?;
        temporary
            .persist(&self.path)
            .map_err(|error| AutomationError::io(error.error))?;
        sync_parent(parent)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ScriptHostOperation {
    ExecuteCommand { command: String },
    ReadDocument,
    EditDocument,
    ReadWorkspace,
    WriteWorkspace,
    ReadSettings,
    ShowMessage,
}

impl ScriptHostOperation {
    fn borrowed(&self) -> HostOperation<'_> {
        match self {
            Self::ExecuteCommand { command } => HostOperation::ExecuteCommand(command),
            Self::ReadDocument => HostOperation::ReadDocument,
            Self::EditDocument => HostOperation::EditDocument,
            Self::ReadWorkspace => HostOperation::ReadWorkspace,
            Self::WriteWorkspace => HostOperation::WriteWorkspace,
            Self::ReadSettings => HostOperation::ReadSettings,
            Self::ShowMessage => HostOperation::ShowMessage,
        }
    }

    fn label(&self) -> String {
        match self {
            Self::ExecuteCommand { command } => format!("executeCommand:{command}"),
            Self::ReadDocument => "readDocument".into(),
            Self::EditDocument => "editDocument".into(),
            Self::ReadWorkspace => "readWorkspace".into(),
            Self::WriteWorkspace => "writeWorkspace".into(),
            Self::ReadSettings => "readSettings".into(),
            Self::ShowMessage => "showMessage".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptHostCall {
    pub operation: ScriptHostOperation,
    #[serde(default)]
    pub arguments: Value,
}

/// Trusted application adapter for the narrow script host surface.
pub trait ScriptHost: Send + 'static {
    /// Performs one already-authorized call and returns bounded JSON data.
    /// Long-running implementations must poll `execution.check(...)` so
    /// cancellation and the deadline also apply while trusted host code runs.
    ///
    /// # Errors
    /// Returns a display-safe failure message. The runtime truncates it before
    /// exposing it to callers.
    fn call(
        &mut self,
        request: ScriptHostCall,
        execution: &ScriptExecutionGuard,
    ) -> Result<Value, String>;
}

#[derive(Clone, Debug, Default)]
pub struct ScriptCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl ScriptCancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug)]
pub struct ScriptExecutionGuard {
    cancellation: ScriptCancellationToken,
    started: Instant,
    execution_millis: u64,
}

impl ScriptExecutionGuard {
    fn new(cancellation: ScriptCancellationToken, execution_millis: u64) -> Self {
        Self {
            cancellation,
            started: Instant::now(),
            execution_millis,
        }
    }

    #[must_use]
    pub fn cancellation_requested(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    #[must_use]
    pub fn remaining(&self) -> Duration {
        Duration::from_millis(self.execution_millis).saturating_sub(self.started.elapsed())
    }

    /// Checks cancellation and elapsed time from trusted host code.
    ///
    /// # Errors
    /// Returns the same typed cancellation or timeout error as the evaluator.
    pub fn check(&self, host_calls: u32) -> Result<(), AutomationError> {
        if self.cancellation_requested() {
            Err(AutomationError::ScriptCancelled { host_calls })
        } else if self.remaining().is_zero() {
            Err(AutomationError::ScriptTimedOut {
                execution_millis: self.execution_millis,
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptRunReport {
    pub operations: u64,
    pub host_calls: u32,
    pub output: String,
}

struct ScriptRuntimeState<H> {
    host: H,
    policy: SandboxPolicy,
    execution: ScriptExecutionGuard,
    host_calls: u32,
    output: String,
    failure: Option<AutomationError>,
}

/// Runs one Rhai script in a raw engine with no module resolver or ambient OS
/// functions. Only the named `nulis_*` functions registered below can cross
/// into the trusted host.
///
/// Host arguments and results are JSON strings. Available calls are:
/// `nulis_command`, `nulis_read_document`, `nulis_edit_document`,
/// `nulis_read_workspace`, `nulis_write_workspace`, `nulis_read_settings`,
/// `nulis_show_message`, and `nulis_print`.
///
/// # Errors
/// Returns a typed validation, permission, resource, cancellation, host, parse,
/// or evaluation error.
pub fn run_script<H: ScriptHost>(
    definition: &ScriptDefinition,
    policy: &SandboxPolicy,
    host: H,
    cancellation: &ScriptCancellationToken,
) -> Result<ScriptRunReport, AutomationError> {
    definition.validate()?;
    policy.validate()?;
    validate_script_grant(definition, policy)?;
    if cancellation.is_cancelled() {
        return Err(AutomationError::ScriptCancelled { host_calls: 0 });
    }

    let execution = ScriptExecutionGuard::new(cancellation.clone(), policy.limits.execution_millis);
    let operations = Arc::new(AtomicU64::new(0));
    let termination = Arc::new(AtomicU8::new(TERMINATION_NONE));
    let state = Arc::new(Mutex::new(ScriptRuntimeState {
        host,
        policy: policy.clone(),
        execution: execution.clone(),
        host_calls: 0,
        output: String::new(),
        failure: None,
    }));

    let mut engine = Engine::new_raw();
    configure_engine(&mut engine, policy);
    register_host_functions(&mut engine, Arc::clone(&state));

    let progress_operations = Arc::clone(&operations);
    let progress_termination = Arc::clone(&termination);
    let progress_execution = execution.clone();
    engine.on_progress(move |count| {
        progress_operations.store(count, Ordering::Relaxed);
        if progress_execution.cancellation_requested() {
            progress_termination.store(TERMINATION_CANCELLED, Ordering::Release);
            Some(Dynamic::UNIT)
        } else if progress_execution.remaining().is_zero() {
            progress_termination.store(TERMINATION_TIMED_OUT, Ordering::Release);
            Some(Dynamic::UNIT)
        } else {
            None
        }
    });

    let ast = engine.compile(&definition.source).map_err(|error| {
        AutomationError::ScriptFailed(truncate(&error.to_string(), MAX_FAILURE_BYTES))
    })?;
    execution.check(0)?;

    let evaluation = engine.run_ast(&ast);
    let (host_calls, output, host_failure) = {
        let state = lock_runtime(&state);
        (
            state.host_calls,
            state.output.clone(),
            state.failure.clone(),
        )
    };
    if let Some(error) = host_failure {
        return Err(error);
    }
    match termination.load(Ordering::Acquire) {
        TERMINATION_CANCELLED => {
            return Err(AutomationError::ScriptCancelled { host_calls });
        }
        TERMINATION_TIMED_OUT => {
            return Err(AutomationError::ScriptTimedOut {
                execution_millis: policy.limits.execution_millis,
            });
        }
        _ => {}
    }
    if let Err(error) = evaluation {
        return Err(map_evaluation_error(&error, policy));
    }
    Ok(ScriptRunReport {
        operations: operations.load(Ordering::Relaxed),
        host_calls,
        output,
    })
}

fn validate_script_grant(
    definition: &ScriptDefinition,
    policy: &SandboxPolicy,
) -> Result<(), AutomationError> {
    if let Some(capability) = policy
        .capabilities
        .difference(&definition.requested_capabilities)
        .next()
    {
        return Err(AutomationError::ScriptGrantExceedsRequest(format!(
            "capability {capability:?}"
        )));
    }
    if let Some(command) = policy
        .allowed_commands
        .difference(&definition.requested_commands)
        .next()
    {
        return Err(AutomationError::ScriptGrantExceedsRequest(format!(
            "command {command}"
        )));
    }
    Ok(())
}

fn configure_engine(engine: &mut Engine, policy: &SandboxPolicy) {
    let memory = policy.limits.memory_bytes;
    let string_bytes = bounded_usize(memory / 64, MAX_ARGUMENT_BYTES);
    engine
        .set_max_operations(policy.limits.operations)
        .set_max_variables(SCRIPT_MAX_VARIABLES)
        .set_max_expr_depths(SCRIPT_MAX_EXPR_DEPTH)
        .set_max_string_size(string_bytes)
        .set_max_strings_interned(SCRIPT_MAX_INTERNED_STRINGS);
    engine.disable_symbol("import").disable_symbol("export");
}

fn bounded_usize(value: u64, maximum: usize) -> usize {
    usize::try_from(value)
        .unwrap_or(usize::MAX)
        .clamp(1, maximum)
}

fn register_host_functions<H: ScriptHost>(
    engine: &mut Engine,
    state: Arc<Mutex<ScriptRuntimeState<H>>>,
) {
    let command_state = Arc::clone(&state);
    engine.register_fn(
        "nulis_command",
        move |command: ImmutableString,
              arguments: ImmutableString|
              -> Result<ImmutableString, Box<EvalAltResult>> {
            let arguments = parse_script_json_for_state(&command_state, &arguments)?;
            invoke_host(
                &command_state,
                ScriptHostOperation::ExecuteCommand {
                    command: command.into(),
                },
                arguments,
            )
        },
    );

    register_no_argument_host(
        engine,
        "nulis_read_document",
        Arc::clone(&state),
        ScriptHostOperation::ReadDocument,
    );
    register_json_argument_host(
        engine,
        "nulis_edit_document",
        Arc::clone(&state),
        ScriptHostOperation::EditDocument,
    );
    register_json_argument_host(
        engine,
        "nulis_read_workspace",
        Arc::clone(&state),
        ScriptHostOperation::ReadWorkspace,
    );
    register_json_argument_host(
        engine,
        "nulis_write_workspace",
        Arc::clone(&state),
        ScriptHostOperation::WriteWorkspace,
    );
    register_no_argument_host(
        engine,
        "nulis_read_settings",
        Arc::clone(&state),
        ScriptHostOperation::ReadSettings,
    );

    let message_state = Arc::clone(&state);
    engine.register_fn(
        "nulis_show_message",
        move |message: ImmutableString| -> Result<ImmutableString, Box<EvalAltResult>> {
            invoke_host(
                &message_state,
                ScriptHostOperation::ShowMessage,
                Value::String(message.into()),
            )
        },
    );

    engine.register_fn(
        "nulis_print",
        move |message: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            emit_output(&state, message.as_ref())
        },
    );
}

fn register_no_argument_host<H: ScriptHost>(
    engine: &mut Engine,
    name: &'static str,
    state: Arc<Mutex<ScriptRuntimeState<H>>>,
    operation: ScriptHostOperation,
) {
    engine.register_fn(
        name,
        move || -> Result<ImmutableString, Box<EvalAltResult>> {
            invoke_host(&state, operation.clone(), Value::Null)
        },
    );
}

fn register_json_argument_host<H: ScriptHost>(
    engine: &mut Engine,
    name: &'static str,
    state: Arc<Mutex<ScriptRuntimeState<H>>>,
    operation: ScriptHostOperation,
) {
    engine.register_fn(
        name,
        move |arguments: ImmutableString| -> Result<ImmutableString, Box<EvalAltResult>> {
            let arguments = parse_script_json_for_state(&state, &arguments)?;
            invoke_host(&state, operation.clone(), arguments)
        },
    );
}

fn parse_script_json(arguments: &str) -> Result<Value, AutomationError> {
    if arguments.len() > MAX_ARGUMENT_BYTES {
        return Err(limit("scriptHostJsonBytes", MAX_ARGUMENT_BYTES as u64));
    }
    let value = serde_json::from_str(arguments)
        .map_err(|error| AutomationError::Malformed(error.to_string()))?;
    bounded_script_json(&value)?;
    Ok(value)
}

fn parse_script_json_for_state<H>(
    state: &Arc<Mutex<ScriptRuntimeState<H>>>,
    arguments: &str,
) -> Result<Value, Box<EvalAltResult>> {
    match parse_script_json(arguments) {
        Ok(value) => Ok(value),
        Err(error) => remember_runtime_failure(&mut lock_runtime(state), error),
    }
}

fn invoke_host<H: ScriptHost>(
    shared: &Arc<Mutex<ScriptRuntimeState<H>>>,
    operation: ScriptHostOperation,
    arguments: Value,
) -> Result<ImmutableString, Box<EvalAltResult>> {
    let mut state = lock_runtime(shared);
    if let Err(error) = reserve_host_call(&mut state) {
        return remember_runtime_failure(&mut state, error);
    }
    if let Err(error) = state.policy.authorize(&operation.borrowed()) {
        return remember_runtime_failure(&mut state, error);
    }
    if let Err(error) = bounded_script_json(&arguments) {
        return remember_runtime_failure(&mut state, error);
    }
    let label = operation.label();
    let execution = state.execution.clone();
    let response = state.host.call(
        ScriptHostCall {
            operation,
            arguments,
        },
        &execution,
    );
    if let Err(error) = execution.check(state.host_calls) {
        return remember_runtime_failure(&mut state, error);
    }
    let response = match response {
        Ok(response) => response,
        Err(message) => {
            return remember_runtime_failure(
                &mut state,
                AutomationError::ScriptHostFailed {
                    operation: label,
                    message: truncate(&message, MAX_FAILURE_BYTES),
                },
            );
        }
    };
    if let Err(error) = bounded_script_json(&response) {
        return remember_runtime_failure(&mut state, error);
    }
    let encoded = match serde_json::to_string(&response) {
        Ok(encoded) => encoded,
        Err(error) => {
            return remember_runtime_failure(
                &mut state,
                AutomationError::ScriptFailed(truncate(&error.to_string(), MAX_FAILURE_BYTES)),
            );
        }
    };
    Ok(encoded.into())
}

fn emit_output<H: ScriptHost>(
    shared: &Arc<Mutex<ScriptRuntimeState<H>>>,
    message: &str,
) -> Result<(), Box<EvalAltResult>> {
    let mut state = lock_runtime(shared);
    if let Err(error) = reserve_host_call(&mut state) {
        return remember_runtime_failure(&mut state, error);
    }
    let added = message.len().saturating_add(1);
    let output_bytes = u64::try_from(state.output.len().saturating_add(added)).unwrap_or(u64::MAX);
    if output_bytes > state.policy.limits.output_bytes {
        let limit = state.policy.limits.output_bytes;
        return remember_runtime_failure(&mut state, AutomationError::ScriptOutputLimit { limit });
    }
    state.output.push_str(message);
    state.output.push('\n');
    Ok(())
}

fn reserve_host_call<H>(state: &mut ScriptRuntimeState<H>) -> Result<(), AutomationError> {
    state.execution.check(state.host_calls)?;
    if state.host_calls >= state.policy.limits.host_calls {
        return Err(AutomationError::ScriptHostCallLimit {
            limit: state.policy.limits.host_calls,
        });
    }
    state.host_calls += 1;
    Ok(())
}

fn remember_runtime_failure<T, H>(
    state: &mut ScriptRuntimeState<H>,
    error: AutomationError,
) -> Result<T, Box<EvalAltResult>> {
    let message = error.to_string();
    state.failure.get_or_insert(error);
    Err(Box::new(runtime_error(&message)))
}

fn runtime_error(message: &str) -> EvalAltResult {
    EvalAltResult::ErrorRuntime(truncate(message, MAX_FAILURE_BYTES).into(), Position::NONE)
}

fn lock_runtime<H>(
    state: &Arc<Mutex<ScriptRuntimeState<H>>>,
) -> MutexGuard<'_, ScriptRuntimeState<H>> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn bounded_script_json(value: &Value) -> Result<usize, AutomationError> {
    match bounded_json_size(value, MAX_ARGUMENT_BYTES) {
        Err(AutomationError::LimitExceeded { resource, .. }) if resource == "argumentJsonDepth" => {
            Err(limit("scriptHostJsonDepth", MAX_JSON_DEPTH as u64))
        }
        Err(AutomationError::LimitExceeded { .. }) => {
            Err(limit("scriptHostJsonBytes", MAX_ARGUMENT_BYTES as u64))
        }
        other => other,
    }
}

fn map_evaluation_error(error: &EvalAltResult, policy: &SandboxPolicy) -> AutomationError {
    match error {
        EvalAltResult::ErrorTooManyOperations(..) => AutomationError::ScriptOperationLimit {
            limit: policy.limits.operations,
        },
        EvalAltResult::ErrorDataTooLarge(..)
        | EvalAltResult::ErrorTooManyVariables(..)
        | EvalAltResult::ErrorStackOverflow(..) => AutomationError::ScriptMemoryLimit {
            limit: policy.limits.memory_bytes,
        },
        _ => AutomationError::ScriptFailed(truncate(&error.to_string(), MAX_FAILURE_BYTES)),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MacroRunReport {
    pub actions_completed: usize,
}

/// Runs a validated macro serially and stops before the next action when
/// cancellation is observed. Command results are deliberately not retained.
///
/// # Errors
/// Returns a validation, cancellation, or indexed command failure.
pub fn run_macro<Execute, Cancel>(
    definition: &MacroDefinition,
    mut execute: Execute,
    mut is_cancelled: Cancel,
) -> Result<MacroRunReport, AutomationError>
where
    Execute: FnMut(&RecordedCommand) -> Result<(), String>,
    Cancel: FnMut() -> bool,
{
    definition.validate()?;
    for (index, action) in definition.actions.iter().enumerate() {
        if is_cancelled() {
            return Err(AutomationError::Cancelled {
                actions_completed: index,
            });
        }
        execute(action).map_err(|message| AutomationError::CommandFailed {
            index,
            command: action.command.clone(),
            message: truncate(&message, MAX_FAILURE_BYTES),
        })?;
    }
    Ok(MacroRunReport {
        actions_completed: definition.actions.len(),
    })
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum AutomationError {
    #[error("malformed automation data: {0}")]
    Malformed(String),
    #[error("unsupported automation schema version: {0}")]
    UnsupportedSchema(u32),
    #[error("unsupported script API version: {0}")]
    UnsupportedScriptApi(u32),
    #[error("invalid {kind} ID: {value}")]
    InvalidId { kind: String, value: String },
    #[error("automation name must contain 1 to {MAX_NAME_BYTES} bytes")]
    InvalidName,
    #[error("a macro must contain at least one action")]
    EmptyMacro,
    #[error("a macro recording is already active")]
    RecordingActive,
    #[error("no macro recording is active")]
    RecordingInactive,
    #[error("macro ID is already present: {0}")]
    DuplicateMacro(String),
    #[error("script ID is already present: {0}")]
    DuplicateScript(String),
    #[error("unknown script: {0}")]
    UnknownScript(String),
    #[error("{resource} must be nonzero and at most {limit}")]
    LimitExceeded { resource: String, limit: u64 },
    #[error("inconsistent sandbox policy: {0}")]
    InconsistentPolicy(String),
    #[error("sandbox permission denied: {0}")]
    PermissionDenied(String),
    #[error("script grant exceeds its request: {0}")]
    ScriptGrantExceedsRequest(String),
    #[error("script cancelled after {host_calls} host calls")]
    ScriptCancelled { host_calls: u32 },
    #[error("script exceeded its {execution_millis} ms execution limit")]
    ScriptTimedOut { execution_millis: u64 },
    #[error("script exceeded its {limit}-operation limit")]
    ScriptOperationLimit { limit: u64 },
    #[error("script exceeded its {limit}-call host limit")]
    ScriptHostCallLimit { limit: u32 },
    #[error("script exceeded its {limit}-byte output limit")]
    ScriptOutputLimit { limit: u64 },
    #[error("script exceeded its {limit}-byte structural memory limit")]
    ScriptMemoryLimit { limit: u64 },
    #[error("script host operation {operation} failed: {message}")]
    ScriptHostFailed { operation: String, message: String },
    #[error("script evaluation failed: {0}")]
    ScriptFailed(String),
    #[error("macro cancelled after {actions_completed} actions")]
    Cancelled { actions_completed: usize },
    #[error("macro action {index} ({command}) failed: {message}")]
    CommandFailed {
        index: usize,
        command: String,
        message: String,
    },
    #[error("macro store path has no parent directory")]
    MissingParent,
    #[error("automation storage operation failed: {0}")]
    Io(String),
}

impl AutomationError {
    #[allow(clippy::needless_pass_by_value)]
    fn io(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

fn validate_id(value: &str, kind: &str) -> Result<(), AutomationError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.contains('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(AutomationError::InvalidId {
            kind: kind.into(),
            value: truncate(value, MAX_ID_BYTES),
        })
    }
}

fn validate_name(name: &str) -> Result<(), AutomationError> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES || name.chars().any(char::is_control) {
        Err(AutomationError::InvalidName)
    } else {
        Ok(())
    }
}

fn validate_budget(resource: &str, value: u64, maximum: u64) -> Result<(), AutomationError> {
    if value == 0 || value > maximum {
        Err(limit(resource, maximum))
    } else {
        Ok(())
    }
}

fn limit(resource: &str, maximum: u64) -> AutomationError {
    AutomationError::LimitExceeded {
        resource: resource.into(),
        limit: maximum,
    }
}

fn bounded_json_size(value: &Value, maximum: usize) -> Result<usize, AutomationError> {
    let mut total = 0_usize;
    let mut pending = vec![(value, 1_usize)];
    while let Some((current, depth)) = pending.pop() {
        if depth > MAX_JSON_DEPTH {
            return Err(limit("argumentJsonDepth", MAX_JSON_DEPTH as u64));
        }
        let increment = match current {
            Value::Null | Value::Bool(true) => 4,
            Value::Bool(false) => 5,
            Value::Number(number) => number.to_string().len(),
            Value::String(string) => string.len().saturating_mul(6).saturating_add(2),
            Value::Array(values) => {
                for child in values {
                    pending.push((child, depth + 1));
                }
                values.len().saturating_add(2)
            }
            Value::Object(entries) => {
                let mut key_bytes = 0_usize;
                for (key, child) in entries {
                    key_bytes = key_bytes
                        .saturating_add(key.len().saturating_mul(6))
                        .saturating_add(3);
                    pending.push((child, depth + 1));
                }
                key_bytes.saturating_add(entries.len()).saturating_add(2)
            }
        };
        total = total.saturating_add(increment);
        if total > maximum {
            return Err(limit("actionArgumentBytes", maximum as u64));
        }
    }
    Ok(total)
}

fn truncate(value: &str, maximum: usize) -> String {
    if value.len() <= maximum {
        return value.into();
    }
    let mut end = maximum;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].into()
}

#[cfg(test)]
mod tests {
    use super::{
        run_macro, run_script, AutomationError, HostOperation, MacroDefinition, MacroRecorder,
        MacroStore, RecordedCommand, SandboxCapability, SandboxLimits, SandboxPolicy,
        ScriptCancellationToken, ScriptDefinition, ScriptExecutionGuard, ScriptHost,
        ScriptHostCall, ScriptHostOperation, ScriptStore, AUTOMATION_SCHEMA_VERSION,
        MAX_ARGUMENT_BYTES, MAX_JSON_DEPTH, SCRIPT_API_VERSION,
    };
    use serde_json::json;
    use std::{
        cell::Cell,
        collections::BTreeSet,
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    fn sample_macro() -> MacroDefinition {
        MacroDefinition {
            schema_version: AUTOMATION_SCHEMA_VERSION,
            id: "user.format-json".into(),
            name: "Format JSON".into(),
            actions: vec![RecordedCommand {
                command: "editor.insertText".into(),
                arguments: json!({ "text": "hello" }),
            }],
        }
    }

    fn sample_script(source: &str) -> ScriptDefinition {
        ScriptDefinition {
            api_version: SCRIPT_API_VERSION,
            name: "Example".into(),
            source: source.into(),
            requested_capabilities: BTreeSet::new(),
            requested_commands: BTreeSet::new(),
        }
    }

    #[derive(Clone)]
    struct RecordingHost {
        calls: Arc<Mutex<Vec<ScriptHostCall>>>,
        response: serde_json::Value,
    }

    impl RecordingHost {
        fn new(response: serde_json::Value) -> (Self, Arc<Mutex<Vec<ScriptHostCall>>>) {
            let calls = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    calls: Arc::clone(&calls),
                    response,
                },
                calls,
            )
        }
    }

    impl ScriptHost for RecordingHost {
        fn call(
            &mut self,
            request: ScriptHostCall,
            execution: &ScriptExecutionGuard,
        ) -> Result<serde_json::Value, String> {
            execution.check(0).map_err(|error| error.to_string())?;
            self.calls.lock().expect("recording host").push(request);
            Ok(self.response.clone())
        }
    }

    struct GuardPollingHost;

    impl ScriptHost for GuardPollingHost {
        fn call(
            &mut self,
            _request: ScriptHostCall,
            execution: &ScriptExecutionGuard,
        ) -> Result<serde_json::Value, String> {
            loop {
                if let Err(error) = execution.check(1) {
                    return Err(error.to_string());
                }
                thread::yield_now();
            }
        }
    }

    #[test]
    fn macro_round_trip_is_valid_and_unknown_fields_are_rejected() {
        let encoded = serde_json::to_vec(&sample_macro()).expect("serialize");
        assert_eq!(MacroDefinition::from_json(&encoded), Ok(sample_macro()));

        let unknown = br#"{"schemaVersion":1,"id":"user.a","name":"A","actions":[],"extra":1}"#;
        assert!(matches!(
            MacroDefinition::from_json(unknown),
            Err(AutomationError::Malformed(_))
        ));
    }

    #[test]
    fn macro_arguments_have_byte_and_depth_budgets() {
        let mut oversized = sample_macro();
        oversized.actions[0].arguments = json!("x".repeat(MAX_ARGUMENT_BYTES));
        assert!(matches!(
            oversized.validate(),
            Err(AutomationError::LimitExceeded { resource, .. }) if resource == "actionArgumentBytes"
        ));

        let mut nested = ValueBuilder::null();
        for _ in 0..=MAX_JSON_DEPTH {
            nested = ValueBuilder::array(nested);
        }
        let mut too_deep = sample_macro();
        too_deep.actions[0].arguments = nested.0;
        assert!(matches!(
            too_deep.validate(),
            Err(AutomationError::LimitExceeded { resource, .. }) if resource == "argumentJsonDepth"
        ));
    }

    struct ValueBuilder(serde_json::Value);

    impl ValueBuilder {
        fn null() -> Self {
            Self(serde_json::Value::Null)
        }

        fn array(value: Self) -> Self {
            Self(serde_json::Value::Array(vec![value.0]))
        }
    }

    #[test]
    fn macro_stops_on_cancellation_and_command_failure() {
        let mut definition = sample_macro();
        definition.actions.push(RecordedCommand {
            command: "editor.deleteSelection".into(),
            arguments: serde_json::Value::Null,
        });
        let calls = Cell::new(0_usize);
        let cancelled = run_macro(
            &definition,
            |_| {
                calls.set(calls.get() + 1);
                Ok(())
            },
            || calls.get() == 1,
        );
        assert_eq!(
            cancelled,
            Err(AutomationError::Cancelled {
                actions_completed: 1
            })
        );

        let failure = run_macro(&definition, |_| Err("no access".into()), || false);
        assert!(matches!(
            failure,
            Err(AutomationError::CommandFailed { index: 0, .. })
        ));
    }

    #[test]
    fn sandbox_is_deny_by_default_and_commands_need_exact_grants() {
        let denied = SandboxPolicy::default();
        assert!(matches!(
            denied.authorize(&HostOperation::ReadDocument),
            Err(AutomationError::PermissionDenied(_))
        ));

        let allowed = SandboxPolicy {
            capabilities: BTreeSet::from([
                SandboxCapability::ExecuteCommand,
                SandboxCapability::ReadDocument,
            ]),
            allowed_commands: BTreeSet::from(["editor.insertText".into()]),
            limits: SandboxLimits::default(),
        };
        allowed.validate().expect("valid policy");
        assert_eq!(allowed.authorize(&HostOperation::ReadDocument), Ok(()));
        assert_eq!(
            allowed.authorize(&HostOperation::ExecuteCommand("editor.insertText")),
            Ok(())
        );
        assert!(allowed
            .authorize(&HostOperation::ExecuteCommand("terminal.create"))
            .is_err());
    }

    #[test]
    fn command_allowlist_without_capability_is_invalid() {
        let policy = SandboxPolicy {
            allowed_commands: BTreeSet::from(["editor.insertText".into()]),
            ..SandboxPolicy::default()
        };
        assert!(matches!(
            policy.validate(),
            Err(AutomationError::InconsistentPolicy(_))
        ));
    }

    #[test]
    fn script_requests_do_not_grant_themselves_authority() {
        let script = ScriptDefinition {
            api_version: SCRIPT_API_VERSION,
            name: "Example".into(),
            source: "nulis.showMessage('hello')".into(),
            requested_capabilities: BTreeSet::from([SandboxCapability::ShowMessage]),
            requested_commands: BTreeSet::new(),
        };
        script.validate().expect("valid request");
        assert!(SandboxPolicy::default()
            .authorize(&HostOperation::ShowMessage)
            .is_err());
    }

    #[test]
    fn script_runtime_exposes_only_capability_checked_json_host_calls() {
        let mut script = sample_script(
            r#"let result = nulis_command("editor.insertText", "{\"text\":\"hello\"}");
                nulis_print(result);"#,
        );
        script
            .requested_capabilities
            .insert(SandboxCapability::ExecuteCommand);
        script.requested_commands.insert("editor.insertText".into());
        let policy = SandboxPolicy {
            capabilities: script.requested_capabilities.clone(),
            allowed_commands: script.requested_commands.clone(),
            limits: SandboxLimits::default(),
        };
        let (host, calls) = RecordingHost::new(json!({ "ok": true }));
        let report = run_script(&script, &policy, host, &ScriptCancellationToken::new())
            .expect("run script");

        assert!(report.operations > 0);
        assert_eq!(report.host_calls, 2);
        assert_eq!(report.output, "{\"ok\":true}\n");
        assert_eq!(
            calls.lock().expect("recorded calls").as_slice(),
            &[ScriptHostCall {
                operation: ScriptHostOperation::ExecuteCommand {
                    command: "editor.insertText".into(),
                },
                arguments: json!({ "text": "hello" }),
            }]
        );
    }

    #[test]
    fn script_runtime_rejects_unrequested_and_denied_authority() {
        let script = sample_script("nulis_read_document();");
        let mut excessive = SandboxPolicy::default();
        excessive
            .capabilities
            .insert(SandboxCapability::ReadDocument);
        let (host, _) = RecordingHost::new(ValueBuilder::null().0);
        assert!(matches!(
            run_script(
                &script,
                &excessive,
                host.clone(),
                &ScriptCancellationToken::new()
            ),
            Err(AutomationError::ScriptGrantExceedsRequest(_))
        ));
        assert!(matches!(
            run_script(
                &script,
                &SandboxPolicy::default(),
                host,
                &ScriptCancellationToken::new()
            ),
            Err(AutomationError::PermissionDenied(_))
        ));
    }

    #[test]
    fn script_runtime_enforces_host_call_and_output_budgets() {
        let mut call_policy = SandboxPolicy::default();
        call_policy.limits.host_calls = 1;
        let (host, _) = RecordingHost::new(ValueBuilder::null().0);
        let call_error = run_script(
            &sample_script("nulis_print(\"one\"); nulis_print(\"two\");"),
            &call_policy,
            host.clone(),
            &ScriptCancellationToken::new(),
        );
        assert_eq!(
            call_error,
            Err(AutomationError::ScriptHostCallLimit { limit: 1 })
        );

        let mut output_policy = SandboxPolicy::default();
        output_policy.limits.output_bytes = 4;
        let output_error = run_script(
            &sample_script("nulis_print(\"four\");"),
            &output_policy,
            host,
            &ScriptCancellationToken::new(),
        );
        assert_eq!(
            output_error,
            Err(AutomationError::ScriptOutputLimit { limit: 4 })
        );
    }

    #[test]
    fn script_runtime_enforces_operation_limits_and_cancellation() {
        let mut operation_policy = SandboxPolicy::default();
        operation_policy.limits.operations = 1_000;
        let (host, _) = RecordingHost::new(ValueBuilder::null().0);
        let operation_error = run_script(
            &sample_script("loop {}"),
            &operation_policy,
            host.clone(),
            &ScriptCancellationToken::new(),
        );
        assert_eq!(
            operation_error,
            Err(AutomationError::ScriptOperationLimit { limit: 1_000 })
        );

        let mut cancellation_policy = SandboxPolicy::default();
        cancellation_policy.limits.operations = 100_000_000;
        cancellation_policy.limits.execution_millis = 10_000;
        let cancellation = ScriptCancellationToken::new();
        let trigger = cancellation.clone();
        let cancellation_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(5));
            trigger.cancel();
        });
        let cancellation_result = run_script(
            &sample_script("loop {}"),
            &cancellation_policy,
            host,
            &cancellation,
        );
        cancellation_thread.join().expect("cancellation thread");
        assert!(matches!(
            cancellation_result,
            Err(AutomationError::ScriptCancelled { host_calls: 0 })
        ));
    }

    #[test]
    fn script_cancellation_remains_typed_inside_a_host_call() {
        let mut script = sample_script("nulis_show_message(\"wait\");");
        script
            .requested_capabilities
            .insert(SandboxCapability::ShowMessage);
        let policy = SandboxPolicy {
            capabilities: script.requested_capabilities.clone(),
            ..SandboxPolicy::default()
        };
        let cancellation = ScriptCancellationToken::new();
        let trigger = cancellation.clone();
        let cancellation_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(5));
            trigger.cancel();
        });
        let result = run_script(&script, &policy, GuardPollingHost, &cancellation);
        cancellation_thread.join().expect("cancellation thread");
        assert_eq!(
            result,
            Err(AutomationError::ScriptCancelled { host_calls: 1 })
        );
    }

    #[test]
    fn script_runtime_blocks_modules_and_bounded_json_depth() {
        let (host, _) = RecordingHost::new(ValueBuilder::null().0);
        let module_error = run_script(
            &sample_script("import \"filesystem\";"),
            &SandboxPolicy::default(),
            host.clone(),
            &ScriptCancellationToken::new(),
        );
        assert!(matches!(
            module_error,
            Err(AutomationError::ScriptFailed(_))
        ));

        let nested_json = format!(
            "{}{}",
            "[".repeat(MAX_JSON_DEPTH + 1),
            "]".repeat(MAX_JSON_DEPTH + 1)
        );
        let mut script = sample_script(&format!("nulis_edit_document(\"{nested_json}\");"));
        script
            .requested_capabilities
            .insert(SandboxCapability::EditDocument);
        let policy = SandboxPolicy {
            capabilities: script.requested_capabilities.clone(),
            ..SandboxPolicy::default()
        };
        let deep_error = run_script(&script, &policy, host, &ScriptCancellationToken::new());
        assert!(
            matches!(
                &deep_error,
                Err(AutomationError::LimitExceeded { resource, .. })
                    if resource == "scriptHostJsonDepth"
            ),
            "{deep_error:?}"
        );
    }

    #[test]
    fn recorder_preserves_prior_actions_when_a_limit_is_exceeded() {
        let mut recorder = MacroRecorder::default();
        recorder
            .start("user.example".into(), "Example".into())
            .expect("start");
        recorder
            .record(RecordedCommand {
                command: "editor.insertText".into(),
                arguments: json!({ "text": "ok" }),
            })
            .expect("first action");
        let failure = recorder.record(RecordedCommand {
            command: "editor.insertText".into(),
            arguments: json!("x".repeat(MAX_ARGUMENT_BYTES)),
        });
        assert!(matches!(
            failure,
            Err(AutomationError::LimitExceeded { .. })
        ));
        let status = recorder.status();
        assert!(status.recording);
        assert_eq!(status.action_count, 1);
        assert!(status.failed.is_some());
        assert!(recorder.finish().is_err());
        assert!(recorder.cancel());
    }

    #[test]
    fn macro_store_round_trip_replaces_and_deletes_atomically() {
        let root = tempfile::tempdir().expect("macro root");
        let store = MacroStore::new(root.path().join("macros.json"));
        let mut definition = sample_macro();
        store.upsert(definition.clone()).expect("store macro");
        assert_eq!(
            store.get(&definition.id).expect("get macro"),
            Some(definition.clone())
        );
        assert_eq!(store.list().expect("list macros")[0].action_count, 1);

        definition.name = "Updated".into();
        store.upsert(definition.clone()).expect("replace macro");
        assert_eq!(store.list().expect("list macros")[0].name, "Updated");
        assert!(store.delete(&definition.id).expect("delete macro"));
        assert_eq!(store.list().expect("empty list"), Vec::new());
        assert!(!store.delete(&definition.id).expect("already deleted"));
    }

    #[test]
    fn script_store_keeps_grants_separate_and_revokes_them_on_change() {
        let root = tempfile::tempdir().expect("script root");
        let store = ScriptStore::new(root.path().join("scripts.json"));
        let mut definition = sample_script("nulis_read_document();");
        definition
            .requested_capabilities
            .insert(SandboxCapability::ReadDocument);
        store
            .upsert("user.reader".into(), definition.clone())
            .expect("store script");
        assert!(!store.list().expect("list scripts")[0].approved);

        let policy = SandboxPolicy {
            capabilities: definition.requested_capabilities.clone(),
            ..SandboxPolicy::default()
        };
        store
            .approve("user.reader", policy.clone())
            .expect("approve");
        assert_eq!(
            store
                .get("user.reader")
                .expect("get")
                .expect("script")
                .approved_policy,
            Some(policy)
        );

        store
            .upsert("user.reader".into(), definition.clone())
            .expect("identical save");
        assert!(store.list().expect("list scripts")[0].approved);
        definition.source.push_str("\nnulis_print(\"changed\");");
        store
            .upsert("user.reader".into(), definition)
            .expect("changed save");
        assert!(!store.list().expect("list scripts")[0].approved);
        assert!(store.delete("user.reader").expect("delete"));
    }
}
