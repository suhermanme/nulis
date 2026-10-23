//! Headless application core and public command boundary.

mod command;
mod count_jobs;
mod editor;
mod format_tools;
mod line_changes;
mod plist_tools;
mod print_jobs;
mod yaml_tools;

use editor::{ApplyCompareRequest, DocumentCompareRequest};

pub use command::{
    CommandContext, CommandDescriptor, CommandError, CommandRegistry, CommandResult,
};
pub use editor::{
    BookmarkDirection, ChangeDirection, ClipboardPayload, DocumentId, DocumentManager,
    DocumentMapChange, DocumentMapSample, DocumentMapSnapshot, DocumentSummary, EditorError,
    ExportObserver, ExportPhase, ExportProgress, ExportScope, ExternalChange, ExternalChangeKind,
    FileSearchScope, FileStamp, FindAllResult, FindDirection, FindResult, FoundLine, FoundMatch,
    HexCaret, HexClipboardPayload, HexCursor, HexEditResult, HexViewportRequest, HexViewportRow,
    HexViewportSnapshot, HorizontalDirection, Justify, LanguageDescriptor, LineEditResult,
    LineEnding, LineEndingConversion, LineEndingStyle, LineFormatOptions, LineOperation,
    MarkdownPreviewSource, NumberPadding, NumberSequence, NumberSum, PdfExportRequest,
    PdfExportResult, RecoveryCandidate, RectangularSelection, ReplaceAllResult, ReplaceFileOutcome,
    ReplacePreview, ReplacePreviewFile, ReplaceTarget, SearchMode, SearchOptions, Selection,
    SyntaxBracketPair, SyntaxFold, SyntaxHighlight, SyntaxSnapshot, SyntaxSymbol, TextEncoding,
    TextTransform, Unobserved, VerticalDirection, ViewportAtRequest, ViewportCursor, ViewportLine,
    ViewportLineRequest, ViewportRequest, ViewportSnapshot, WorkspaceSearchMatch,
    WorkspaceSearchResult, HIGHLIGHT_LOOKBACK_BYTES, HIGHLIGHT_WINDOW_BYTES, LARGE_EXPORT_PAGES,
    LINE_ENDING_SAMPLE_BYTES, MAX_BOOKMARKS, MAX_CLIPBOARD_BYTES, MAX_EDIT_BYTES,
    MAX_FILE_NAME_BYTES, MAX_FIND_ALL_LINES, MAX_FIND_ALL_MATCHES, MAX_HEX_CLIPBOARD_BYTES,
    MAX_HEX_EDIT_BYTES, MAX_HEX_VIEWPORT_BYTES, MAX_LINE_ENDING_CHANGES,
    MAX_LINE_ENDING_CONVERSION_BYTES, MAX_LINE_PREVIEW_BYTES, MAX_MARKDOWN_PREVIEW_BYTES,
    MAX_MERGE_BYTES, MAX_RECTANGULAR_ROWS, MAX_REPLACE_FILES, MAX_SELECTIONS, MAX_VIEWPORT_BYTES,
    MAX_WORKSPACE_SEARCH_FILES, MAX_WORKSPACE_SEARCH_RESULTS, PRINT_WINDOW_BYTES,
};
pub use format_tools::ToolKind;
pub use line_changes::{LineChange, LineChangeStatus};
pub use nulis_automation::{
    AutomationError, MacroDefinition, MacroRunReport, MacroSummary, RecordedCommand,
    RecordingStatus, SandboxCapability, SandboxLimits, SandboxPolicy, ScriptDefinition,
    ScriptRunReport, ScriptSummary, StoredScript, AUTOMATION_SCHEMA_VERSION, SCRIPT_API_VERSION,
};
pub use nulis_diff::{ChangeKind, CompareMode, DiffChange, DiffPage, DiffRange};
use nulis_filesystem::{compare_folders, fingerprint, save_atomic, SessionStore, WorkspaceService};
pub use nulis_filesystem::{
    FolderCompareEntry, FolderCompareResult, FolderCompareStatus, FolderEntryKind,
    WorkbenchSession, WorkspaceDirectory, WorkspaceEntry, WorkspaceEntryKind, WorkspaceSummary,
    MAX_FOLDER_COMPARE_ENTRIES, MAX_FOLDER_COMPARE_RESULTS, MAX_WORKSPACE_DIRECTORY_ENTRIES,
    SESSION_SCHEMA_VERSION,
};
use nulis_plugin_host::{PluginCatalog, PluginHostOperation, PluginPackage, ProcessPluginRunner};
pub use nulis_plugin_host::{
    PluginCommandContribution, PluginError, PluginGrant, PluginPermission, PluginSummary,
    PLUGIN_API_VERSION,
};
pub use nulis_settings::SettingsDocument;
pub use nulis_settings::{
    AppearanceSettings, CustomizationDocument, Keybinding, MenuEntry, CUSTOMIZATION_SCHEMA_VERSION,
};
use nulis_settings::{CustomizationStore, SettingsFileStore, SettingsLayer, SettingsStore};
use nulis_terminal::TerminalManager;
pub use nulis_terminal::{
    TerminalId, TerminalOutput, TerminalSize, TerminalSpawnRequest, TerminalSummary,
};
use nulis_theme::ThemeCatalog;
pub use nulis_theme::{Theme, ThemeKind, ThemeRecord, THEME_SCHEMA_VERSION, THEME_TOKENS};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use nulis_automation::{
    run_macro, run_script, MacroRecorder, MacroStore, ScriptCancellationToken,
    ScriptExecutionGuard, ScriptHost, ScriptHostCall, ScriptHostOperation, ScriptStore,
    CURRENT_REVISION_ARGUMENT,
};

pub const IPC_PROTOCOL_VERSION: u32 = 43;
pub const PRODUCT_NAME: &str = "Nulis";
pub const APPLICATION_ID: &str = "com.nulis.editor";
const MAX_SCRIPT_MESSAGES: usize = 100;
const MAX_SCRIPT_MESSAGE_BYTES: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationInfo {
    pub product_name: String,
    pub application_id: String,
    pub version: String,
    pub ipc_protocol_version: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MacroRunOutcome {
    pub actions_completed: usize,
    pub document: Option<DocumentSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptRunOutcome {
    pub report: ScriptRunReport,
    pub document: Option<DocumentSummary>,
    pub messages: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginRunOutcome {
    pub plugin_id: String,
    pub command: Option<String>,
    pub result: Value,
    pub messages: Vec<String>,
}

#[derive(Debug)]
struct AutomationState {
    recorder: MacroRecorder,
    macro_store: MacroStore,
    script_store: ScriptStore,
    active_script: Option<ScriptCancellationToken>,
}

#[derive(Debug)]
struct PluginState {
    catalog: PluginCatalog,
    runner: ProcessPluginRunner,
    active: Option<ScriptCancellationToken>,
}

#[derive(Debug)]
struct CustomizationState {
    store: CustomizationStore,
    document: CustomizationDocument,
    themes: ThemeCatalog,
    /// Advances on every stored change so stale editors cannot overwrite it silently.
    revision: u64,
}

/// The user's `settings.json` and the layered values derived from it.
#[derive(Debug)]
struct SettingsState {
    defaults: Map<String, Value>,
    layers: SettingsStore,
    file: SettingsFileStore,
    document: SettingsDocument,
    /// Advances on every stored change, like the customization revision.
    revision: u64,
    /// Why the file on disk could not be used, when it could not.
    load_error: Option<String>,
    /// `files.associations` patterns left out (bad pattern or unknown language).
    rejected_associations: Vec<String>,
}

impl SettingsState {
    /// Rebuilds the layers and file associations from `document`.
    fn apply(&mut self, documents: &DocumentManager) {
        let mut layers = SettingsStore::with_defaults(self.defaults.clone());
        layers.replace_layer(SettingsLayer::User, self.document.user_values());
        for (language, values) in self.document.language_overrides() {
            layers.replace_language(language, values);
        }
        self.layers = layers;
        let associations = self.document.file_associations();
        self.rejected_associations = documents.set_user_associations(
            associations
                .iter()
                .map(|(pattern, language)| (pattern.as_str(), language.as_str())),
        );
    }

    fn report(&self) -> Value {
        json!({
            "revision": self.revision,
            "document": self.document,
            "rejectedAssociations": self.rejected_associations,
            "loadError": self.load_error,
        })
    }
}

/// Owns UI-independent application services.
pub struct NulisCore {
    commands: CommandRegistry,
    settings: Mutex<SettingsState>,
    documents: Arc<DocumentManager>,
    workspace: Arc<WorkspaceService>,
    session_store: Arc<SessionStore>,
    terminals: Arc<TerminalManager>,
    automation: Mutex<AutomationState>,
    plugins: Mutex<PluginState>,
    customization: Mutex<CustomizationState>,
}

impl std::fmt::Debug for NulisCore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NulisCore")
            .field("commands", &self.commands)
            .field("settings", &self.settings)
            .field("documents", &self.documents)
            .field("workspace", &self.workspace)
            .field("session_store", &self.session_store)
            .field("terminals", &self.terminals)
            .field("automation", &self.automation)
            .field("plugins", &self.plugins)
            .field("customization", &self.customization)
            .finish()
    }
}

impl Default for NulisCore {
    fn default() -> Self {
        Self::with_session_root(std::env::temp_dir().join("nulis-sessions"))
    }
}

impl NulisCore {
    /// Creates a headless core whose disk-backed edit buffers live below the
    /// supplied application-data directory.
    ///
    /// # Panics
    /// Panics only if a statically defined built-in command ID is invalid or duplicated.
    #[must_use]
    pub fn with_session_root(session_root: std::path::PathBuf) -> Self {
        let worker = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("nulis"));
        Self::with_session_root_and_plugin_worker(session_root, worker)
    }

    /// Creates a core with an explicit isolated plugin-worker executable.
    /// Tests and alternate shells use this to retain the same process protocol.
    ///
    /// # Panics
    /// Panics only if a static built-in command is invalid/duplicated or an
    /// application-data store path cannot be constructed.
    #[must_use]
    #[allow(clippy::too_many_lines)] // Service wiring stays in one auditable constructor.
    pub fn with_session_root_and_plugin_worker(
        session_root: std::path::PathBuf,
        plugin_worker: std::path::PathBuf,
    ) -> Self {
        let mut commands = CommandRegistry::default();
        let session_store = Arc::new(SessionStore::new(session_root.join("workbench.json")));
        let automation = Mutex::new(AutomationState {
            recorder: MacroRecorder::default(),
            macro_store: MacroStore::new(session_root.join("macros.json")),
            script_store: ScriptStore::new(session_root.join("scripts.json")),
            active_script: None,
        });
        let plugins = Mutex::new(PluginState {
            catalog: PluginCatalog::new(
                session_root.join("plugins"),
                session_root.join("plugin-grants.json"),
            ),
            runner: ProcessPluginRunner::new(plugin_worker),
            active: None,
        });
        let customization_store = CustomizationStore::new(session_root.join("customization.json"));
        let customization_document = customization_store.load().unwrap_or_default();
        let customization = Mutex::new(CustomizationState {
            store: customization_store,
            document: customization_document,
            themes: ThemeCatalog::new(session_root.join("themes")),
            revision: 0,
        });
        let settings_path = session_root.join("settings.json");
        let documents = Arc::new(DocumentManager::new(session_root));
        let workspace = Arc::new(WorkspaceService::default());
        let terminals = Arc::new(TerminalManager::default());
        commands
            .register(
                CommandDescriptor::new("app.about", "About Nulis", "Application")
                    .expect("built-in command ID is valid"),
                |_context, _arguments| {
                    Ok(json!({
                        "productName": PRODUCT_NAME,
                        "version": env!("CARGO_PKG_VERSION"),
                    }))
                },
            )
            .expect("built-in command is unique");
        for (id, title) in [
            ("settings.state", "Get Settings"),
            ("settings.saveChecked", "Save Settings at Revision"),
        ] {
            commands
                .register(
                    CommandDescriptor::new(id, title, "Settings")
                        .expect("built-in command ID is valid"),
                    |_context, _arguments| Ok(Value::Null),
                )
                .expect("built-in command is unique");
        }
        for (id, title) in [
            ("customization.get", "Get Customization"),
            ("customization.state", "Get Customization Revision"),
            ("customization.save", "Save Customization"),
            (
                "customization.saveChecked",
                "Save Customization at Revision",
            ),
            ("customization.reset", "Reset Customization"),
            ("theme.list", "List Themes"),
            ("theme.install", "Install Theme"),
            ("theme.uninstall", "Uninstall Theme"),
        ] {
            commands
                .register(
                    CommandDescriptor::new(id, title, "Customization")
                        .expect("built-in command ID is valid"),
                    |_context, _arguments| Ok(Value::Null),
                )
                .expect("built-in command is unique");
        }
        commands
            .register(
                CommandDescriptor::new(
                    "workbench.commandPalette",
                    "Show Command Palette",
                    "Workbench",
                )
                .expect("built-in command ID is valid"),
                |_context, _arguments| Ok(Value::Null),
            )
            .expect("built-in command is unique");
        register_editor_commands(&mut commands, &documents);
        register_compare_commands(&mut commands, &documents);
        register_workspace_commands(&mut commands, &workspace, &documents);
        register_session_commands(&mut commands, &session_store);
        register_workbench_commands(&mut commands);
        register_terminal_commands(&mut commands, &terminals);
        register_automation_commands(&mut commands);
        register_plugin_commands(&mut commands);

        let mut defaults = Map::new();
        defaults.insert("editor".into(), json!({ "tabSize": 4, "wordWrap": false }));
        defaults.insert(
            "workbench".into(),
            json!({ "colorTheme": "nulis.ember-dark" }),
        );
        let settings_file = SettingsFileStore::new(settings_path);
        let (settings_document, load_error) = match settings_file.load() {
            Ok(document) => (document, None),
            // A broken file must not stop the editor; it is reported instead.
            Err(error) => (SettingsDocument::default(), Some(error.to_string())),
        };
        let mut settings = SettingsState {
            layers: SettingsStore::with_defaults(defaults.clone()),
            defaults,
            file: settings_file,
            document: settings_document,
            revision: 0,
            load_error,
            rejected_associations: Vec::new(),
        };
        settings.apply(&documents);

        Self {
            commands,
            settings: Mutex::new(settings),
            documents,
            workspace,
            session_store,
            terminals,
            automation,
            plugins,
            customization,
        }
    }

    #[must_use]
    pub fn application_info(&self) -> ApplicationInfo {
        ApplicationInfo {
            product_name: PRODUCT_NAME.into(),
            application_id: APPLICATION_ID.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            ipc_protocol_version: IPC_PROTOCOL_VERSION,
        }
    }

    #[must_use]
    pub fn command_descriptors(&self) -> Vec<CommandDescriptor> {
        let mut descriptors = self.commands.descriptors();
        if let Ok(plugins) = self.plugins.lock() {
            if let Ok(summaries) = plugins.catalog.list() {
                for plugin in summaries.into_iter().filter(|plugin| plugin.approved) {
                    descriptors.extend(plugin.commands.into_iter().map(|command| {
                        CommandDescriptor {
                            id: command.command,
                            title: command.title,
                            category: format!("Plugin · {}", command.category),
                        }
                    }));
                }
            }
        }
        descriptors.sort_by(|left, right| left.id.cmp(&right.id));
        descriptors
    }

    /// Executes a registered command through the sole application action boundary.
    ///
    /// # Errors
    /// Returns a typed command error for invalid, unknown, unavailable, or failed commands.
    pub fn execute_command(
        &self,
        id: &str,
        context: CommandContext,
        arguments: Value,
    ) -> CommandResult {
        if id.starts_with("customization.") || id.starts_with("theme.") {
            return self.execute_customization_command(id, arguments);
        }
        if id.starts_with("settings.") {
            return self.execute_settings_command(id, arguments);
        }
        if id.starts_with("automation.") {
            return self.execute_automation_command(id, &context, arguments);
        }
        if id.starts_with("plugin.") {
            return self.execute_plugin_lifecycle_command(id, &context, arguments);
        }
        if self.plugin_for_command(id)?.is_some() {
            return self.execute_plugin_contribution(id, arguments);
        }
        let recorded_arguments = if is_recordable_command(id) {
            let automation = self
                .automation
                .lock()
                .map_err(|_| CommandError::Unavailable("automation state".into()))?;
            automation
                .recorder
                .is_recording()
                .then(|| normalize_recorded_arguments(arguments.clone()))
        } else {
            None
        };
        let result = self.commands.execute(id, context, arguments);
        if result.is_ok() {
            if let Some(arguments) = recorded_arguments {
                if let Ok(mut automation) = self.automation.lock() {
                    let _ = automation.recorder.record(RecordedCommand {
                        command: id.into(),
                        arguments,
                    });
                }
            }
        }
        result
    }

    /// `settings.state` reports `settings.json` and any problems with it;
    /// `settings.saveChecked` validates, stores, and applies a new document
    /// against the revision the caller saw.
    fn execute_settings_command(&self, id: &str, arguments: Value) -> CommandResult {
        let mut state = self
            .settings
            .lock()
            .map_err(|_| CommandError::Unavailable("settings state".into()))?;
        match id {
            "settings.state" => Ok(state.report()),
            "settings.saveChecked" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Checked {
                    expected_revision: u64,
                    document: SettingsDocument,
                }
                let request: Checked = serde_json::from_value(arguments)
                    .map_err(|error| CommandError::Failed(format!("invalid settings: {error}")))?;
                if request.expected_revision != state.revision {
                    return Err(CommandError::Failed(format!(
                        "settings changed elsewhere (revision {} is now {})",
                        request.expected_revision, state.revision
                    )));
                }
                state
                    .file
                    .save(&request.document)
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                state.document = request.document;
                state.revision += 1;
                state.load_error = None;
                state.apply(&self.documents);
                Ok(state.report())
            }
            _ => Err(CommandError::Unknown(id.to_owned())),
        }
    }

    #[allow(clippy::too_many_lines)] // Keeps one audited dispatch boundary for the small lifecycle API.
    fn execute_customization_command(&self, id: &str, arguments: Value) -> CommandResult {
        let known_commands = self
            .command_descriptors()
            .into_iter()
            .filter(is_customizable_command)
            .map(|descriptor| descriptor.id)
            .collect::<std::collections::BTreeSet<_>>();
        let mut state = self
            .customization
            .lock()
            .map_err(|_| CommandError::Unavailable("customization state".into()))?;
        match id {
            "customization.get" => {
                validate_customization_references(&state.document, &known_commands, &state.themes)?;
                serde_json::to_value(&state.document)
                    .map_err(|error| CommandError::Failed(error.to_string()))
            }
            "customization.state" => {
                validate_customization_references(&state.document, &known_commands, &state.themes)?;
                Ok(json!({ "revision": state.revision, "document": state.document }))
            }
            "customization.save" | "customization.saveChecked" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Checked {
                    expected_revision: u64,
                    document: CustomizationDocument,
                }
                let invalid = |error: serde_json::Error| {
                    CommandError::Failed(format!("invalid customization: {error}"))
                };
                let document = if id == "customization.saveChecked" {
                    let request: Checked = serde_json::from_value(arguments).map_err(invalid)?;
                    if request.expected_revision != state.revision {
                        return Err(CommandError::Failed(format!(
                            "customization changed elsewhere (revision {} is now {})",
                            request.expected_revision, state.revision
                        )));
                    }
                    request.document
                } else {
                    serde_json::from_value(arguments).map_err(invalid)?
                };
                document
                    .validate()
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                validate_customization_references(&document, &known_commands, &state.themes)?;
                state
                    .store
                    .save(&document)
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                state.document = document;
                state.revision += 1;
                if id == "customization.saveChecked" {
                    return Ok(json!({ "revision": state.revision, "document": state.document }));
                }
                serde_json::to_value(&state.document)
                    .map_err(|error| CommandError::Failed(error.to_string()))
            }
            "customization.reset" => {
                state.document = state
                    .store
                    .reset()
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                state.revision += 1;
                serde_json::to_value(&state.document)
                    .map_err(|error| CommandError::Failed(error.to_string()))
            }
            "theme.list" => serde_json::to_value(
                state
                    .themes
                    .list()
                    .map_err(|error| CommandError::Failed(error.to_string()))?,
            )
            .map_err(|error| CommandError::Failed(error.to_string())),
            "theme.install" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Request {
                    path: String,
                }
                let request: Request = serde_json::from_value(arguments).map_err(|error| {
                    CommandError::Failed(format!("invalid theme install request: {error}"))
                })?;
                let record = state
                    .themes
                    .install(std::path::Path::new(&request.path))
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                serde_json::to_value(record)
                    .map_err(|error| CommandError::Failed(error.to_string()))
            }
            "theme.uninstall" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Request {
                    id: String,
                }
                let request: Request = serde_json::from_value(arguments).map_err(|error| {
                    CommandError::Failed(format!("invalid theme uninstall request: {error}"))
                })?;
                if state.document.appearance.theme == request.id {
                    return Err(CommandError::Failed(
                        "select another theme before uninstalling this one".into(),
                    ));
                }
                state
                    .themes
                    .uninstall(&request.id)
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                Ok(Value::Null)
            }
            _ => Err(CommandError::Unknown(id.into())),
        }
    }

    /// Replays a bounded macro through the centralized command registry.
    ///
    /// # Errors
    /// Returns a validation, cancellation, or indexed command error.
    pub fn execute_macro<Cancel>(
        &self,
        definition: &MacroDefinition,
        context: &CommandContext,
        is_cancelled: Cancel,
    ) -> Result<MacroRunReport, AutomationError>
    where
        Cancel: FnMut() -> bool,
    {
        run_macro(
            definition,
            |action| {
                if !is_recordable_command(&action.command) {
                    return Err(format!(
                        "command is not permitted in macros: {}",
                        action.command
                    ));
                }
                let arguments = self
                    .resolve_macro_arguments(action.arguments.clone(), context)
                    .map_err(|error| error.to_string())?;
                self.commands
                    .execute(&action.command, context.clone(), arguments)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            },
            is_cancelled,
        )
    }

    #[allow(clippy::too_many_lines)] // Automation lifecycle cases remain one auditable boundary.
    fn execute_automation_command(
        &self,
        id: &str,
        context: &CommandContext,
        arguments: Value,
    ) -> CommandResult {
        match id {
            "automation.listMacros" => {
                let automation = self.automation_state()?;
                let macros = automation
                    .macro_store
                    .list()
                    .map_err(automation_command_error)?;
                serde_json::to_value(macros).map_err(|error| serialization_command_error(&error))
            }
            "automation.recordingStatus" => {
                let automation = self.automation_state()?;
                serde_json::to_value(automation.recorder.status())
                    .map_err(|error| serialization_command_error(&error))
            }
            "automation.startRecording" => {
                let arguments: StartRecordingArguments = parse_arguments(arguments)?;
                let mut automation = self.automation_state()?;
                automation
                    .recorder
                    .start(arguments.id, arguments.name)
                    .map_err(automation_command_error)?;
                serde_json::to_value(automation.recorder.status())
                    .map_err(|error| serialization_command_error(&error))
            }
            "automation.stopRecording" => {
                let mut automation = self.automation_state()?;
                let definition = automation
                    .recorder
                    .snapshot()
                    .map_err(automation_command_error)?;
                automation
                    .macro_store
                    .upsert(definition.clone())
                    .map_err(automation_command_error)?;
                automation.recorder.cancel();
                serde_json::to_value(definition)
                    .map_err(|error| serialization_command_error(&error))
            }
            "automation.cancelRecording" => {
                let mut automation = self.automation_state()?;
                automation.recorder.cancel();
                serde_json::to_value(automation.recorder.status())
                    .map_err(|error| serialization_command_error(&error))
            }
            "automation.deleteMacro" => {
                let arguments: MacroIdArguments = parse_arguments(arguments)?;
                let automation = self.automation_state()?;
                automation
                    .macro_store
                    .delete(&arguments.id)
                    .map(Value::Bool)
                    .map_err(automation_command_error)
            }
            "automation.runMacro" => {
                let arguments: MacroIdArguments = parse_arguments(arguments)?;
                let definition = {
                    let automation = self.automation_state()?;
                    automation
                        .macro_store
                        .get(&arguments.id)
                        .map_err(automation_command_error)?
                        .ok_or_else(|| {
                            CommandError::Failed(format!("unknown macro: {}", arguments.id))
                        })?
                };
                let report = self
                    .execute_macro(&definition, context, || false)
                    .map_err(automation_command_error)?;
                let document = context
                    .document_id
                    .as_ref()
                    .map(|id| self.documents.summary(&DocumentId(id.clone())))
                    .transpose()
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(MacroRunOutcome {
                    actions_completed: report.actions_completed,
                    document,
                })
                .map_err(|error| serialization_command_error(&error))
            }
            "automation.listScripts" => {
                let automation = self.automation_state()?;
                let scripts = automation
                    .script_store
                    .list()
                    .map_err(automation_command_error)?;
                serde_json::to_value(scripts).map_err(|error| serialization_command_error(&error))
            }
            "automation.getScript" => {
                let arguments: ScriptIdArguments = parse_arguments(arguments)?;
                let automation = self.automation_state()?;
                let script = automation
                    .script_store
                    .get(&arguments.id)
                    .map_err(automation_command_error)?
                    .ok_or_else(|| {
                        CommandError::Failed(format!("unknown script: {}", arguments.id))
                    })?;
                serde_json::to_value(script).map_err(|error| serialization_command_error(&error))
            }
            "automation.saveScript" => {
                let arguments: SaveScriptArguments = parse_arguments(arguments)?;
                validate_script_commands(&arguments.definition)?;
                let automation = self.automation_state()?;
                automation
                    .script_store
                    .upsert(arguments.id.clone(), arguments.definition)
                    .map_err(automation_command_error)?;
                let script = automation
                    .script_store
                    .get(&arguments.id)
                    .map_err(automation_command_error)?
                    .ok_or_else(|| CommandError::Unavailable("saved script disappeared".into()))?;
                serde_json::to_value(script).map_err(|error| serialization_command_error(&error))
            }
            "automation.deleteScript" => {
                let arguments: ScriptIdArguments = parse_arguments(arguments)?;
                let automation = self.automation_state()?;
                automation
                    .script_store
                    .delete(&arguments.id)
                    .map(Value::Bool)
                    .map_err(automation_command_error)
            }
            "automation.approveScript" => {
                let arguments: ApproveScriptArguments = parse_arguments(arguments)?;
                for command in &arguments.policy.allowed_commands {
                    if !is_script_command(command) {
                        return Err(CommandError::Failed(format!(
                            "command is not permitted in scripts: {command}"
                        )));
                    }
                }
                let automation = self.automation_state()?;
                automation
                    .script_store
                    .approve(&arguments.id, arguments.policy)
                    .map_err(automation_command_error)?;
                Ok(Value::Null)
            }
            "automation.revokeScript" => {
                let arguments: ScriptIdArguments = parse_arguments(arguments)?;
                let automation = self.automation_state()?;
                automation
                    .script_store
                    .revoke(&arguments.id)
                    .map(Value::Bool)
                    .map_err(automation_command_error)
            }
            "automation.cancelScript" => {
                let automation = self.automation_state()?;
                Ok(Value::Bool(automation.active_script.as_ref().is_some_and(
                    |token| {
                        token.cancel();
                        true
                    },
                )))
            }
            "automation.runScript" => self.run_saved_script(context, arguments),
            _ => Err(CommandError::Unknown(id.into())),
        }
    }

    fn run_saved_script(&self, context: &CommandContext, arguments: Value) -> CommandResult {
        let arguments: ScriptIdArguments = parse_arguments(arguments)?;
        let (script, policy, cancellation) = {
            let mut automation = self.automation_state()?;
            if automation.active_script.is_some() {
                return Err(CommandError::Unavailable(
                    "a script is already running".into(),
                ));
            }
            let stored = automation
                .script_store
                .get(&arguments.id)
                .map_err(automation_command_error)?
                .ok_or_else(|| CommandError::Failed(format!("unknown script: {}", arguments.id)))?;
            validate_script_commands(&stored.definition)?;
            let policy = stored.approved_policy.ok_or_else(|| {
                CommandError::Failed("script requires explicit approval before it can run".into())
            })?;
            let cancellation = ScriptCancellationToken::new();
            automation.active_script = Some(cancellation.clone());
            (stored.definition, policy, cancellation)
        };

        let messages = Arc::new(Mutex::new(Vec::new()));
        let host = CoreScriptHost {
            commands: self.commands.clone(),
            documents: Arc::clone(&self.documents),
            workspace: Arc::clone(&self.workspace),
            settings: self.effective_settings(None),
            context: context.clone(),
            messages: Arc::clone(&messages),
        };
        let result = run_script(&script, &policy, host, &cancellation);
        if let Ok(mut automation) = self.automation.lock() {
            automation.active_script = None;
        }
        let report = result.map_err(automation_command_error)?;
        let document = context
            .document_id
            .as_ref()
            .map(|id| self.documents.summary(&DocumentId(id.clone())))
            .transpose()
            .map_err(|error| editor_command_error(&error))?;
        let messages = messages
            .lock()
            .map_err(|_| CommandError::Unavailable("script messages".into()))?
            .clone();
        serde_json::to_value(ScriptRunOutcome {
            report,
            document,
            messages,
        })
        .map_err(|error| serialization_command_error(&error))
    }

    #[allow(clippy::too_many_lines)] // Plugin lifecycle cases remain one auditable boundary.
    fn execute_plugin_lifecycle_command(
        &self,
        id: &str,
        _context: &CommandContext,
        arguments: Value,
    ) -> CommandResult {
        match id {
            "plugin.list" => {
                let plugins = self.plugin_state()?;
                let summaries = plugins.catalog.list().map_err(plugin_command_error)?;
                serde_json::to_value(summaries).map_err(|error| serialization_command_error(&error))
            }
            "plugin.install" => {
                let arguments: PluginInstallArguments = parse_arguments(arguments)?;
                let plugins = self.plugin_state()?;
                let summary = plugins
                    .catalog
                    .install(std::path::Path::new(&arguments.path))
                    .map_err(plugin_command_error)?;
                serde_json::to_value(summary).map_err(|error| serialization_command_error(&error))
            }
            "plugin.uninstall" => {
                let arguments: PluginIdArguments = parse_arguments(arguments)?;
                let plugins = self.plugin_state()?;
                plugins
                    .catalog
                    .uninstall(&arguments.id)
                    .map(Value::Bool)
                    .map_err(plugin_command_error)
            }
            "plugin.approve" => {
                let arguments: ApprovePluginArguments = parse_arguments(arguments)?;
                let plugins = self.plugin_state()?;
                let package = plugins
                    .catalog
                    .get(&arguments.id)
                    .map_err(plugin_command_error)?
                    .ok_or_else(|| {
                        CommandError::Failed(format!("unknown plugin: {}", arguments.id))
                    })?;
                plugins
                    .catalog
                    .approve(&package, arguments.grant)
                    .map_err(plugin_command_error)?;
                Ok(Value::Null)
            }
            "plugin.revoke" => {
                let arguments: PluginIdArguments = parse_arguments(arguments)?;
                let plugins = self.plugin_state()?;
                plugins
                    .catalog
                    .revoke(&arguments.id)
                    .map(Value::Bool)
                    .map_err(plugin_command_error)
            }
            "plugin.activateStartup" => {
                let packages = {
                    let plugins = self.plugin_state()?;
                    plugins
                        .catalog
                        .packages()
                        .map_err(plugin_command_error)?
                        .into_iter()
                        .filter(|package| package.manifest.activation_events.contains("onStartup"))
                        .collect::<Vec<_>>()
                };
                let outcomes = packages
                    .into_iter()
                    .filter_map(|package| {
                        let approved = self
                            .plugins
                            .lock()
                            .ok()
                            .and_then(|plugins| plugins.catalog.grant_for(&package).ok())
                            .flatten()
                            .is_some();
                        approved.then(|| self.run_plugin(package, None, Vec::new()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                serde_json::to_value(outcomes).map_err(|error| serialization_command_error(&error))
            }
            "plugin.cancel" => {
                let plugins = self.plugin_state()?;
                Ok(Value::Bool(plugins.active.as_ref().is_some_and(|token| {
                    token.cancel();
                    true
                })))
            }
            _ => Err(CommandError::Unknown(id.into())),
        }
    }

    fn plugin_for_command(&self, command: &str) -> Result<Option<PluginPackage>, CommandError> {
        let plugins = self.plugin_state()?;
        for package in plugins.catalog.packages().map_err(plugin_command_error)? {
            let contributes = package
                .manifest
                .contributes
                .commands
                .iter()
                .any(|contribution| contribution.command == command);
            if contributes
                && plugins
                    .catalog
                    .grant_for(&package)
                    .map_err(plugin_command_error)?
                    .is_some()
            {
                return Ok(Some(package));
            }
        }
        Ok(None)
    }

    fn execute_plugin_contribution(&self, command: &str, arguments: Value) -> CommandResult {
        let package = self
            .plugin_for_command(command)?
            .ok_or_else(|| CommandError::Unknown(command.into()))?;
        let outcome = self.run_plugin(package, Some(command.into()), vec![arguments])?;
        serde_json::to_value(outcome).map_err(|error| serialization_command_error(&error))
    }

    fn run_plugin(
        &self,
        package: PluginPackage,
        command: Option<String>,
        arguments: Vec<Value>,
    ) -> Result<PluginRunOutcome, CommandError> {
        if let Some(command) = &command {
            let event = format!("onCommand:{command}");
            if !package.manifest.activation_events.contains(&event) {
                return Err(CommandError::Failed(format!(
                    "plugin does not activate for command: {command}"
                )));
            }
        }
        let (grant, runner, cancellation) = {
            let mut plugins = self.plugin_state()?;
            if plugins.active.is_some() {
                return Err(CommandError::Unavailable(
                    "a plugin is already running".into(),
                ));
            }
            let grant = plugins
                .catalog
                .grant_for(&package)
                .map_err(plugin_command_error)?
                .ok_or_else(|| CommandError::Failed("plugin requires explicit approval".into()))?;
            let cancellation = ScriptCancellationToken::new();
            plugins.active = Some(cancellation.clone());
            (grant, plugins.runner.clone(), cancellation)
        };
        let request = package.worker_request(command.clone(), arguments);
        let response = runner.run(&request, || cancellation.is_cancelled());
        if let Ok(mut plugins) = self.plugins.lock() {
            plugins.active = None;
        }
        let response = response.map_err(plugin_command_error)?;
        let expected = package
            .manifest
            .contributes
            .commands
            .iter()
            .map(|contribution| contribution.command.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let registered = response
            .registered_commands
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if registered != expected {
            return Err(CommandError::Failed(
                "plugin runtime registrations do not match its manifest".into(),
            ));
        }
        let mut messages = Vec::new();
        let mut message_bytes = 0_usize;
        for operation in response.operations {
            grant.authorize(&operation).map_err(plugin_command_error)?;
            match operation {
                PluginHostOperation::ShowMessage { message } => {
                    message_bytes = message_bytes.saturating_add(message.len());
                    if messages.len() >= MAX_SCRIPT_MESSAGES
                        || message_bytes > MAX_SCRIPT_MESSAGE_BYTES
                    {
                        return Err(CommandError::Failed(
                            "plugin message output limit exceeded".into(),
                        ));
                    }
                    messages.push(message);
                }
            }
        }
        Ok(PluginRunOutcome {
            plugin_id: package.manifest.id,
            command,
            result: response.result,
            messages,
        })
    }

    fn plugin_state(&self) -> Result<std::sync::MutexGuard<'_, PluginState>, CommandError> {
        self.plugins
            .lock()
            .map_err(|_| CommandError::Unavailable("plugin state".into()))
    }

    fn automation_state(&self) -> Result<std::sync::MutexGuard<'_, AutomationState>, CommandError> {
        self.automation
            .lock()
            .map_err(|_| CommandError::Unavailable("automation state".into()))
    }

    fn resolve_macro_arguments(
        &self,
        mut arguments: Value,
        context: &CommandContext,
    ) -> Result<Value, CommandError> {
        let Some(object) = arguments.as_object_mut() else {
            return Ok(arguments);
        };
        let uses_current_revision = object.get("expectedRevision").and_then(Value::as_str)
            == Some(CURRENT_REVISION_ARGUMENT);
        if uses_current_revision {
            let document_id = context_document(context)?;
            let revision = self
                .documents
                .summary(&document_id)
                .map_err(|error| editor_command_error(&error))?
                .revision;
            object.insert(
                "expectedRevision".into(),
                Value::String(revision.to_string()),
            );
        }
        Ok(arguments)
    }

    #[must_use]
    pub fn effective_settings(&self, language: Option<&str>) -> Value {
        self.settings
            .lock()
            .map_or(Value::Null, |settings| settings.layers.effective(language))
    }

    /// Returns a bounded editor snapshot without exposing authoritative state.
    ///
    /// # Errors
    /// Returns a typed editor error for invalid or stale document coordinates.
    pub fn viewport(&self, request: &ViewportRequest) -> Result<ViewportSnapshot, EditorError> {
        self.documents.viewport(request)
    }

    /// Returns a bounded raw-byte hexadecimal snapshot.
    ///
    /// # Errors
    /// Returns a typed editor error for invalid layout or document coordinates.
    pub fn hex_viewport(
        &self,
        request: &HexViewportRequest,
    ) -> Result<HexViewportSnapshot, EditorError> {
        self.documents.hex_viewport(request)
    }

    /// Returns a bounded viewport revealing an arbitrary source byte.
    ///
    /// # Errors
    /// Returns a typed editor error for invalid document coordinates.
    pub fn viewport_at(
        &self,
        request: &ViewportAtRequest,
    ) -> Result<ViewportSnapshot, EditorError> {
        self.documents.viewport_at(request)
    }

    /// Returns a bounded whole-document text overview.
    ///
    /// # Errors
    /// Returns a typed editor error for an unknown document or backing-store failure.
    pub fn document_map(
        &self,
        document_id: &DocumentId,
    ) -> Result<DocumentMapSnapshot, EditorError> {
        self.documents.document_map(document_id)
    }

    /// Returns current bounded Markdown source, including unsaved edits.
    ///
    /// # Errors
    /// Returns a typed editor error for an unknown or oversized document.
    pub fn markdown_preview_source(
        &self,
        document_id: &DocumentId,
    ) -> Result<MarkdownPreviewSource, EditorError> {
        self.documents.markdown_preview_source(document_id)
    }

    /// Reads a bounded image relative to a saved Markdown document.
    ///
    /// # Errors
    /// Returns a typed editor error for an invalid image path or read failure.
    pub fn markdown_preview_image(
        &self,
        document_id: &DocumentId,
        relative_path: &str,
    ) -> Result<String, EditorError> {
        self.documents
            .markdown_preview_image(document_id, relative_path)
    }

    /// Reports whether any document has unsaved edits.
    ///
    /// # Errors
    /// Returns a typed editor error if document state is unavailable.
    pub fn has_dirty_documents(&self) -> Result<bool, EditorError> {
        self.documents.has_dirty_documents()
    }

    /// Returns a bounded viewport beginning at a one-based logical line.
    ///
    /// # Errors
    /// Returns a typed editor error for an invalid document or line number.
    pub fn viewport_line(
        &self,
        request: &ViewportLineRequest,
    ) -> Result<ViewportSnapshot, EditorError> {
        self.documents.viewport_line(request)
    }
}

fn is_customizable_command(descriptor: &CommandDescriptor) -> bool {
    descriptor.category.starts_with("Plugin ·")
        || matches!(
            descriptor.id.as_str(),
            "app.about"
                | "file.new"
                | "file.open"
                | "file.save"
                | "file.saveAs"
                | "workspace.open"
                | "workspace.close"
                | "editor.undo"
                | "editor.redo"
                | "editor.toggleReadOnly"
                | "workbench.commandPalette"
                | "tools.validate"
                | "tools.beautify"
                | "tools.minify"
                | "workbench.toggleSidebar"
                | "workbench.toggleSplit"
                | "workbench.toggleEditorMode"
        )
        || WORKBENCH_ACTIONS
            .iter()
            .any(|(id, ..)| *id == descriptor.id.as_str())
}

fn validate_customization_references(
    document: &CustomizationDocument,
    known_commands: &std::collections::BTreeSet<String>,
    themes: &ThemeCatalog,
) -> Result<(), CommandError> {
    let available_themes = themes
        .list()
        .map_err(|error| CommandError::Failed(error.to_string()))?;
    if !available_themes
        .iter()
        .any(|theme| theme.theme.id == document.appearance.theme)
    {
        return Err(CommandError::Failed(format!(
            "unknown theme: {}",
            document.appearance.theme
        )));
    }
    for command in document
        .keybindings
        .iter()
        .map(|item| item.command.as_str())
        .chain(document.menu.iter().filter_map(|item| match item {
            MenuEntry::Command { command } => Some(command.as_str()),
            MenuEntry::Separator => None,
        }))
        .chain(document.toolbar.iter().map(String::as_str))
    {
        if !known_commands.contains(command) {
            return Err(CommandError::Failed(format!(
                "command is not safe for customization: {command}"
            )));
        }
    }
    Ok(())
}

struct CoreScriptHost {
    commands: CommandRegistry,
    documents: Arc<DocumentManager>,
    workspace: Arc<WorkspaceService>,
    settings: Value,
    context: CommandContext,
    messages: Arc<Mutex<Vec<String>>>,
}

impl CoreScriptHost {
    fn document_id(&self) -> Result<DocumentId, String> {
        self.context
            .document_id
            .clone()
            .map(DocumentId)
            .ok_or_else(|| "operation requires a document context".into())
    }

    fn command_arguments(&self, mut arguments: Value) -> Result<Value, String> {
        let Some(object) = arguments.as_object_mut() else {
            return Ok(arguments);
        };
        if object.get("expectedRevision").and_then(Value::as_str) == Some(CURRENT_REVISION_ARGUMENT)
        {
            let revision = self
                .documents
                .summary(&self.document_id()?)
                .map_err(|error| error.to_string())?
                .revision;
            object.insert(
                "expectedRevision".into(),
                Value::String(revision.to_string()),
            );
        }
        Ok(arguments)
    }

    fn expected_revision(&self, value: &Value) -> Result<u64, String> {
        if value.as_str() == Some(CURRENT_REVISION_ARGUMENT) {
            return self
                .documents
                .summary(&self.document_id()?)
                .map(|document| document.revision)
                .map_err(|error| error.to_string());
        }
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|revision| revision.parse().ok()))
            .ok_or_else(|| {
                "expectedRevision must be a decimal revision or current-revision placeholder".into()
            })
    }
}

impl ScriptHost for CoreScriptHost {
    fn call(
        &mut self,
        request: ScriptHostCall,
        execution: &ScriptExecutionGuard,
    ) -> Result<Value, String> {
        execution.check(0).map_err(|error| error.to_string())?;
        match request.operation {
            ScriptHostOperation::ExecuteCommand { command } => {
                if !is_script_command(&command) {
                    return Err(format!("command is not permitted in scripts: {command}"));
                }
                let arguments = self.command_arguments(request.arguments)?;
                self.commands
                    .execute(&command, self.context.clone(), arguments)
                    .map_err(|error| error.to_string())
            }
            ScriptHostOperation::ReadDocument => {
                let document_id = self.document_id()?;
                let document = self
                    .documents
                    .summary(&document_id)
                    .map_err(|error| error.to_string())?;
                let viewport = self
                    .documents
                    .viewport(&ViewportRequest {
                        document_id,
                        start_byte: 0,
                        start_line: 1,
                        line_continuation: false,
                        max_bytes: 65_536,
                    })
                    .map_err(|error| error.to_string())?;
                Ok(json!({ "document": document, "viewport": viewport }))
            }
            ScriptHostOperation::EditDocument => {
                let arguments: ScriptEditDocumentArguments =
                    parse_host_arguments(request.arguments)?;
                let expected_revision = self.expected_revision(&arguments.expected_revision)?;
                let document = self
                    .documents
                    .replace(
                        &self.document_id()?,
                        expected_revision,
                        arguments.start_byte,
                        arguments.end_byte,
                        &arguments.text,
                    )
                    .map_err(|error| error.to_string())?;
                serde_json::to_value(document).map_err(|error| error.to_string())
            }
            ScriptHostOperation::ReadWorkspace => {
                let arguments: ScriptReadWorkspaceArguments =
                    parse_host_arguments(request.arguments)?;
                let maximum = arguments.max_entries.min(500);
                let directory = self
                    .workspace
                    .list(&arguments.path, maximum)
                    .map_err(|error| error.to_string())?;
                serde_json::to_value(directory).map_err(|error| error.to_string())
            }
            ScriptHostOperation::WriteWorkspace => {
                let arguments: ScriptWriteWorkspaceArguments =
                    parse_host_arguments(request.arguments)?;
                if arguments.contents.len() > MAX_EDIT_BYTES {
                    return Err(format!("workspace write exceeds {MAX_EDIT_BYTES} bytes"));
                }
                let path = self
                    .workspace
                    .resolve_file(&arguments.path)
                    .map_err(|error| error.to_string())?;
                let expected = fingerprint(&path).map_err(|error| error.to_string())?;
                let bytes = arguments.contents.into_bytes();
                save_atomic(&path, expected.as_ref(), |file| {
                    file.write_all(&bytes)?;
                    u64::try_from(bytes.len()).map_err(std::io::Error::other)
                })
                .map_err(|error| error.to_string())?;
                Ok(json!({ "path": arguments.path, "byteLen": bytes.len().to_string() }))
            }
            ScriptHostOperation::ReadSettings => Ok(self.settings.clone()),
            ScriptHostOperation::ShowMessage => {
                let message = request
                    .arguments
                    .as_str()
                    .ok_or_else(|| "message must be a string".to_owned())?;
                let mut messages = self
                    .messages
                    .lock()
                    .map_err(|_| "script message state is unavailable".to_owned())?;
                let total = messages
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    .saturating_add(message.len());
                if messages.len() >= MAX_SCRIPT_MESSAGES || total > MAX_SCRIPT_MESSAGE_BYTES {
                    return Err("script message output limit exceeded".into());
                }
                messages.push(message.into());
                Ok(Value::Null)
            }
        }
    }
}

fn parse_host_arguments<T: for<'de> Deserialize<'de>>(arguments: Value) -> Result<T, String> {
    serde_json::from_value(arguments).map_err(|error| format!("invalid host arguments: {error}"))
}

fn validate_script_commands(definition: &ScriptDefinition) -> Result<(), CommandError> {
    for command in &definition.requested_commands {
        if !is_script_command(command) {
            return Err(CommandError::Failed(format!(
                "command is not permitted in scripts: {command}"
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NewDocumentArguments {
    name: String,
    #[serde(default)]
    initial_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartRecordingArguments {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MacroIdArguments {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScriptIdArguments {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SaveScriptArguments {
    id: String,
    definition: ScriptDefinition,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApproveScriptArguments {
    id: String,
    policy: SandboxPolicy,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginIdArguments {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginInstallArguments {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApprovePluginArguments {
    id: String,
    grant: PluginGrant,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScriptEditDocumentArguments {
    expected_revision: Value,
    #[serde(with = "editor::decimal_u64")]
    start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    end_byte: u64,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScriptReadWorkspaceArguments {
    #[serde(default)]
    path: String,
    #[serde(default = "default_workspace_entry_limit")]
    max_entries: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScriptWriteWorkspaceArguments {
    path: String,
    contents: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenFileArguments {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SaveAsArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveryIdArguments {
    recovery_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CloseDocumentArguments {
    discard: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompareDocumentsArguments {
    left_document_id: DocumentId,
    right_document_id: DocumentId,
    mode: CompareMode,
    #[serde(default, with = "editor::decimal_u64")]
    left_start_byte: u64,
    #[serde(default, with = "editor::decimal_u64")]
    right_start_byte: u64,
    #[serde(default = "default_start_line", with = "editor::decimal_u64")]
    left_start_line: u64,
    #[serde(default = "default_start_line", with = "editor::decimal_u64")]
    right_start_line: u64,
    #[serde(default = "default_compare_bytes")]
    max_bytes: usize,
    #[serde(default = "default_compare_changes")]
    max_changes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplyCompareChangeArguments {
    source_document_id: DocumentId,
    target_document_id: DocumentId,
    #[serde(with = "editor::decimal_u64")]
    expected_target_revision: u64,
    #[serde(with = "editor::decimal_u64")]
    source_start: u64,
    #[serde(with = "editor::decimal_u64")]
    source_end: u64,
    #[serde(with = "editor::decimal_u64")]
    target_start: u64,
    #[serde(with = "editor::decimal_u64")]
    target_end: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompareFoldersArguments {
    left_root: String,
    right_root: String,
    #[serde(default = "default_folder_compare_results")]
    max_results: usize,
}

const fn default_start_line() -> u64 {
    1
}

const fn default_compare_bytes() -> usize {
    256 * 1024
}

const fn default_compare_changes() -> usize {
    500
}

const fn default_folder_compare_results() -> usize {
    1_000
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReplaceArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    #[serde(with = "editor::decimal_u64")]
    start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    end_byte: u64,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WordAtArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    #[serde(with = "editor::decimal_u64")]
    byte_offset: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MoveSelectedTextArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    #[serde(with = "editor::decimal_u64")]
    start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    end_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    destination_byte: u64,
    #[serde(default)]
    copy: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectionArguments {
    selections: Vec<Selection>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RectangularSelectionArguments {
    #[serde(with = "editor::decimal_u64")]
    anchor_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    active_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    anchor_column: u64,
    #[serde(with = "editor::decimal_u64")]
    active_column: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevisionArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FormatArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    /// The user agreed that YAML Beautify may drop comments and expand anchors.
    #[serde(default)]
    accept_loss: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InsertTextArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    text: String,
    #[serde(default)]
    overwrite: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MoveArguments {
    direction: HorizontalDirection,
    #[serde(default)]
    extend: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VerticalMoveArguments {
    direction: VerticalDirection,
    #[serde(default)]
    extend: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BookmarkMoveArguments {
    direction: BookmarkDirection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LineOperationArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    operation: LineOperation,
    #[serde(default)]
    options: LineFormatOptions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransformArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    transform: TextTransform,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EncodingArguments {
    encoding: TextEncoding,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LanguageArguments {
    language_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HexNibbleArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    high_nibble: bool,
    nibble: u8,
    #[serde(default = "default_true")]
    overwrite: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HexReplaceArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    hex: String,
    #[serde(default = "default_true")]
    overwrite: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FindArguments {
    query: String,
    #[serde(default)]
    options: SearchOptions,
    #[serde(with = "editor::decimal_u64")]
    start_byte: u64,
    direction: FindDirection,
    #[serde(default = "default_true")]
    wrap: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReplaceAllArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    query: String,
    replacement: String,
    #[serde(default)]
    options: SearchOptions,
    /// Limits the search to a byte range, for "selected text".
    #[serde(default)]
    within: Option<ByteRangeArguments>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReplaceMatchArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    query: String,
    replacement: String,
    #[serde(default)]
    options: SearchOptions,
    #[serde(with = "editor::decimal_u64")]
    start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    end_byte: u64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ByteRangeArguments {
    #[serde(with = "editor::decimal_u64")]
    start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    end_byte: u64,
}

impl ByteRangeArguments {
    fn pair(self) -> (u64, u64) {
        (self.start_byte, self.end_byte)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FindAllArguments {
    query: String,
    #[serde(default)]
    options: SearchOptions,
    #[serde(default)]
    within: Option<ByteRangeArguments>,
    #[serde(default = "default_find_all_limit")]
    max_matches: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ValidatePatternArguments {
    query: String,
    #[serde(default)]
    options: SearchOptions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MatchesInRangeArguments {
    query: String,
    #[serde(default)]
    options: SearchOptions,
    #[serde(with = "editor::decimal_u64")]
    start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    end_byte: u64,
}

fn default_find_all_limit() -> usize {
    editor::MAX_FIND_ALL_MATCHES
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreviewReplaceInFilesArguments {
    root: String,
    query: String,
    #[serde(default)]
    options: SearchOptions,
    #[serde(default)]
    scope: editor::FileSearchScope,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReplaceInFilesArguments {
    files: Vec<ReplaceTarget>,
    query: String,
    replacement: String,
    #[serde(default)]
    options: SearchOptions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChangeNearArguments {
    #[serde(with = "editor::decimal_u64")]
    from_byte: u64,
    direction: ChangeDirection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConvertLineEndingsArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    target: LineEnding,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NumberSequenceArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    sequence: NumberSequence,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JustifyArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    justify: Justify,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RenameFileArguments {
    name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReloadArguments {
    /// Reload even though the document has unsaved edits.
    #[serde(default)]
    discard: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AddBookmarksArguments {
    /// Decimal byte offsets.
    offsets: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FindInFilesArguments {
    root: String,
    query: String,
    #[serde(default)]
    options: SearchOptions,
    #[serde(default)]
    scope: editor::FileSearchScope,
    #[serde(default = "default_workspace_result_limit")]
    max_results: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenWorkspaceArguments {
    root: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ListWorkspaceArguments {
    #[serde(default)]
    path: String,
    #[serde(default = "default_workspace_entry_limit")]
    max_entries: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ListPlaceArguments {
    path: String,
    #[serde(default = "default_workspace_entry_limit")]
    max_entries: usize,
    #[serde(default)]
    show_hidden: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenWorkspaceFileArguments {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TerminalWriteArguments {
    terminal_id: TerminalId,
    bytes: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TerminalReadArguments {
    terminal_id: TerminalId,
    #[serde(with = "editor::decimal_u64")]
    cursor: u64,
    maximum: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TerminalResizeArguments {
    terminal_id: TerminalId,
    size: TerminalSize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TerminalIdArguments {
    terminal_id: TerminalId,
}

const fn default_true() -> bool {
    true
}

const fn default_workspace_result_limit() -> usize {
    500
}

const fn default_workspace_entry_limit() -> usize {
    500
}

fn register_automation_commands(commands: &mut CommandRegistry) {
    for (id, title) in [
        ("automation.listMacros", "List Macros"),
        ("automation.recordingStatus", "Macro Recording Status"),
        ("automation.startRecording", "Start Macro Recording"),
        ("automation.stopRecording", "Stop and Save Macro Recording"),
        ("automation.cancelRecording", "Cancel Macro Recording"),
        ("automation.deleteMacro", "Delete Macro"),
        ("automation.runMacro", "Run Macro"),
        ("automation.listScripts", "List Scripts"),
        ("automation.getScript", "Get Script"),
        ("automation.saveScript", "Save Script"),
        ("automation.deleteScript", "Delete Script"),
        ("automation.approveScript", "Approve Script"),
        ("automation.revokeScript", "Revoke Script Approval"),
        ("automation.runScript", "Run Script"),
        ("automation.cancelScript", "Cancel Script"),
    ] {
        commands
            .register(
                CommandDescriptor::new(id, title, "Automation")
                    .expect("built-in command ID is valid"),
                |_context, _arguments| {
                    Err(CommandError::Unavailable(
                        "automation command requires application state".into(),
                    ))
                },
            )
            .expect("built-in command is unique");
    }
}

fn register_plugin_commands(commands: &mut CommandRegistry) {
    for (id, title) in [
        ("plugin.list", "List Plugins"),
        ("plugin.install", "Install Plugin"),
        ("plugin.uninstall", "Uninstall Plugin"),
        ("plugin.approve", "Approve Plugin"),
        ("plugin.revoke", "Revoke Plugin Approval"),
        ("plugin.activateStartup", "Activate Startup Plugins"),
        ("plugin.cancel", "Cancel Plugin"),
    ] {
        commands
            .register(
                CommandDescriptor::new(id, title, "Plugins").expect("built-in command ID is valid"),
                |_context, _arguments| {
                    Err(CommandError::Unavailable(
                        "plugin command requires application state".into(),
                    ))
                },
            )
            .expect("built-in command is unique");
    }
}

fn is_recordable_command(id: &str) -> bool {
    matches!(
        id,
        "editor.deleteBackward"
            | "editor.deleteForward"
            | "editor.selectAll"
            | "editor.lineOperation"
            | "editor.deleteBookmarkedLines"
            | "editor.convertLineEndings"
            | "editor.insertNumberSequence"
            | "editor.justifyColumns"
            | "editor.replace"
            | "editor.setSelections"
            | "editor.undo"
            | "editor.redo"
            | "editor.setRectangularSelection"
            | "editor.setEncoding"
            | "editor.transformSelections"
            | "editor.toggleBookmark"
            | "editor.clearBookmarks"
            | "editor.moveHorizontal"
            | "editor.moveVertical"
            | "editor.moveToBookmark"
            | "editor.insertText"
            | "editor.deleteSelection"
            | "hex.typeNibble"
            | "hex.replaceBytes"
            | "hex.deleteBackward"
            | "language.set"
            | "search.replaceAll"
            | "search.replaceMatch"
    )
}

fn is_script_command(id: &str) -> bool {
    is_recordable_command(id)
}

fn normalize_recorded_arguments(mut arguments: Value) -> Value {
    if let Some(object) = arguments.as_object_mut() {
        if object.contains_key("expectedRevision") {
            object.insert(
                "expectedRevision".into(),
                Value::String(CURRENT_REVISION_ARGUMENT.into()),
            );
        }
    }
    arguments
}

/// Quick Open's bounded list of the workspace's files.
fn register_workspace_file_list(commands: &mut CommandRegistry, workspace: &Arc<WorkspaceService>) {
    let service = Arc::clone(workspace);
    commands
        .register(
            CommandDescriptor::new("workspace.listFiles", "List Workspace Files", "Workspace")
                .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                let result = service
                    .list_files()
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_workspace_commands(
    commands: &mut CommandRegistry,
    workspace: &Arc<WorkspaceService>,
    documents: &Arc<DocumentManager>,
) {
    let service = Arc::clone(workspace);
    commands
        .register(
            CommandDescriptor::new("workspace.open", "Open Workspace…", "Workspace")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: OpenWorkspaceArguments = parse_arguments(arguments)?;
                let result = service
                    .open(std::path::Path::new(&arguments.root))
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let service = Arc::clone(workspace);
    commands
        .register(
            CommandDescriptor::new("workspace.current", "Current Workspace", "Workspace")
                .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                let result = service
                    .current()
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let service = Arc::clone(workspace);
    commands
        .register(
            CommandDescriptor::new("workspace.list", "List Workspace Directory", "Workspace")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: ListWorkspaceArguments = parse_arguments(arguments)?;
                let result = service
                    .list(&arguments.path, arguments.max_entries)
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    register_workspace_file_list(commands, workspace);

    commands
        .register(
            CommandDescriptor::new("places.roots", "List Places", "Workspace")
                .expect("built-in command ID is valid"),
            |_context, _arguments| {
                serde_json::to_value(nulis_filesystem::places())
                    .map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
    commands
        .register(
            CommandDescriptor::new("places.list", "List Place Directory", "Workspace")
                .expect("built-in command ID is valid"),
            |_context, arguments| {
                let arguments: ListPlaceArguments = parse_arguments(arguments)?;
                let result = nulis_filesystem::list_place(
                    std::path::Path::new(&arguments.path),
                    arguments.max_entries,
                    arguments.show_hidden,
                )
                .map_err(|error| CommandError::Failed(error.to_string()))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let service = Arc::clone(workspace);
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("workspace.openFile", "Open Workspace File", "Workspace")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: OpenWorkspaceFileArguments = parse_arguments(arguments)?;
                let path = service
                    .resolve_file(&arguments.path)
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                let result = manager
                    .open_file(&path)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let service = Arc::clone(workspace);
    commands
        .register(
            CommandDescriptor::new("workspace.close", "Close Workspace", "Workspace")
                .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                service
                    .close()
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                Ok(Value::Null)
            },
        )
        .expect("built-in command is unique");
}

fn register_terminal_commands(commands: &mut CommandRegistry, terminals: &Arc<TerminalManager>) {
    let manager = Arc::clone(terminals);
    commands
        .register(
            CommandDescriptor::new("terminal.create", "Create Terminal", "Terminal")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let request: TerminalSpawnRequest = parse_arguments(arguments)?;
                let result = manager.spawn(&request).map_err(terminal_command_error)?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(terminals);
    commands
        .register(
            CommandDescriptor::new("terminal.list", "List Terminals", "Terminal")
                .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                let result = manager.list().map_err(terminal_command_error)?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(terminals);
    commands
        .register(
            CommandDescriptor::new("terminal.write", "Write Terminal", "Terminal")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: TerminalWriteArguments = parse_arguments(arguments)?;
                manager
                    .write(&arguments.terminal_id, &arguments.bytes)
                    .map_err(terminal_command_error)?;
                Ok(Value::Null)
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(terminals);
    commands
        .register(
            CommandDescriptor::new("terminal.read", "Read Terminal", "Terminal")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: TerminalReadArguments = parse_arguments(arguments)?;
                let result = manager
                    .read(&arguments.terminal_id, arguments.cursor, arguments.maximum)
                    .map_err(terminal_command_error)?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(terminals);
    commands
        .register(
            CommandDescriptor::new("terminal.resize", "Resize Terminal", "Terminal")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: TerminalResizeArguments = parse_arguments(arguments)?;
                manager
                    .resize(&arguments.terminal_id, arguments.size)
                    .map_err(terminal_command_error)?;
                Ok(Value::Null)
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(terminals);
    commands
        .register(
            CommandDescriptor::new("terminal.close", "Close Terminal", "Terminal")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: TerminalIdArguments = parse_arguments(arguments)?;
                manager
                    .close(&arguments.terminal_id)
                    .map_err(terminal_command_error)?;
                Ok(Value::Null)
            },
        )
        .expect("built-in command is unique");
}

fn register_session_commands(commands: &mut CommandRegistry, store: &Arc<SessionStore>) {
    let session_store = Arc::clone(store);
    commands
        .register(
            CommandDescriptor::new("session.load", "Restore Workbench Session", "Workspace")
                .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                let result = session_store
                    .load()
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let session_store = Arc::clone(store);
    commands
        .register(
            CommandDescriptor::new("session.save", "Save Workbench Session", "Workspace")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let session: WorkbenchSession = parse_arguments(arguments)?;
                session_store
                    .save(&session)
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                Ok(Value::Null)
            },
        )
        .expect("built-in command is unique");
}

/// Presentation-only actions. The core validates and names them so palettes,
/// shortcuts, menus, and toolbars share one dispatch path; the desktop applies them.
const WORKBENCH_ACTIONS: &[(&str, &str, &str)] = &[
    ("workbench.toggleSidebar", "Toggle Primary Side Bar", "View"),
    ("workbench.toggleSplit", "Toggle Split Editor", "View"),
    (
        "workbench.toggleEditorMode",
        "Toggle Text / Hex Editor",
        "View",
    ),
    ("workbench.splitRight", "Split Editor Right", "View"),
    ("workbench.splitDown", "Split Editor Down", "View"),
    ("workbench.closeFile", "Close File", "File"),
    ("format.insertNumbers", "Insert Number Sequence…", "Format"),
    ("format.sumNumbers", "Sum Selected Numbers", "Format"),
    ("format.justifyLeft", "Left Justify Columns", "Format"),
    ("format.justifyCenter", "Center Columns", "Format"),
    ("format.justifyRight", "Right Justify Columns", "Format"),
    (
        "format.alignColumns",
        "Convert to Fixed-Width Columns",
        "Format",
    ),
    (
        "format.unalignColumns",
        "Convert to Delimited Columns",
        "Format",
    ),
    ("edit.copyAppend", "Copy and Append", "Edit"),
    ("edit.cutAppend", "Cut and Append", "Edit"),
    ("edit.pasteCopy", "Paste and Copy", "Edit"),
    ("edit.pasteFromHistory", "Paste from History…", "Edit"),
    ("file.quickOpen", "Quick Open…", "File"),
    ("file.saveAll", "Save All", "File"),
    ("file.closeAll", "Close All Files", "File"),
    ("file.openRecent", "Open Recent…", "File"),
    ("file.clearRecent", "Clear Recent Files", "File"),
    ("file.rename", "Rename File…", "File"),
    ("file.revealInPlaces", "Show in Places", "File"),
    ("file.print", "Print…", "File"),
    ("file.pageSetup", "Page Setup…", "File"),
    ("file.exportPdf", "Export to PDF…", "File"),
    (
        "file.exportSelectionPdf",
        "Export Selection to PDF…",
        "File",
    ),
    ("workbench.closeTab", "Close Tab", "View"),
    ("workbench.openSettings", "Settings…", "Application"),
    ("workbench.openHelp", "Nulis Help", "Help"),
    ("workbench.nextTab", "Next Tab", "View"),
    ("workbench.previousTab", "Previous Tab", "View"),
    ("workbench.moveTabLeft", "Move Tab Left", "View"),
    ("workbench.moveTabRight", "Move Tab Right", "View"),
    (
        "view.toggleLineChangeIndicators",
        "Show/Hide Line Change Indicators",
        "View",
    ),
    ("view.toggleWordWrap", "Toggle Word Wrap", "View"),
    ("view.toggleWhitespace", "Show/Hide Whitespace", "View"),
    ("view.toggleLineEndings", "Show/Hide Line Endings", "View"),
    ("view.toggleLineNumbers", "Show/Hide Line Numbers", "View"),
    ("view.toggleColumnMarker", "Show/Hide Column Marker", "View"),
    ("view.zoomIn", "Zoom In", "View"),
    ("view.zoomOut", "Zoom Out", "View"),
    ("view.zoomReset", "Reset Zoom", "View"),
    ("view.toggleAlwaysOnTop", "Toggle Always on Top", "View"),
    ("workspace.rename", "Rename Workspace", "Workspace"),
    (
        "workspace.groupRight",
        "Group with Next Tab Side by Side",
        "Workspace",
    ),
    (
        "workspace.groupDown",
        "Group with Next Tab Stacked",
        "Workspace",
    ),
    ("workspace.ungroup", "Ungroup Workspace", "Workspace"),
    ("workspace.popOutPane", "Move Pane to New Tab", "Workspace"),
    ("workspace.closePane", "Close Pane", "Workspace"),
    ("workspace.focusNextPane", "Focus Next Pane", "Workspace"),
    (
        "workspace.focusPreviousPane",
        "Focus Previous Pane",
        "Workspace",
    ),
    ("workspace.growPane", "Grow Pane", "Workspace"),
    ("workspace.shrinkPane", "Shrink Pane", "Workspace"),
    // Editing and formatting commands; the focused editor carries them out.
    ("edit.selectLines", "Select Line", "Edit"),
    ("edit.duplicateLines", "Duplicate Line", "Edit"),
    ("edit.deleteLines", "Delete Line", "Edit"),
    ("edit.moveLinesUp", "Move Line Up", "Edit"),
    ("edit.moveLinesDown", "Move Line Down", "Edit"),
    ("edit.insertLineBelow", "Insert Line Below", "Edit"),
    ("format.uppercase", "To Upper Case", "Format"),
    ("format.lowercase", "To Lower Case", "Format"),
    ("format.capitalize", "Capitalize Words", "Format"),
    ("format.invertCase", "Invert Case", "Format"),
    ("format.trimSelection", "Trim Selection", "Format"),
    ("format.indent", "Indent Lines", "Format"),
    ("format.outdent", "Outdent Lines", "Format"),
    ("format.toggleComment", "Toggle Line Comment", "Format"),
    (
        "format.trimTrailingSpaces",
        "Trim Trailing Spaces",
        "Format",
    ),
    ("format.tabsToSpaces", "Tabs to Spaces", "Format"),
    ("format.spacesToTabs", "Leading Spaces to Tabs", "Format"),
    ("format.sortAscending", "Sort Lines Ascending", "Format"),
    ("format.sortDescending", "Sort Lines Descending", "Format"),
    (
        "format.sortUnique",
        "Sort Lines and Remove Duplicates",
        "Format",
    ),
    ("format.base64Encode", "Encode Base64", "Format"),
    ("format.base64Decode", "Decode Base64", "Format"),
    ("format.insertDateTime", "Insert Date and Time", "Format"),
    (
        "format.lineEndingsLf",
        "Convert Line Endings to LF",
        "Format",
    ),
    (
        "format.lineEndingsCrlf",
        "Convert Line Endings to CRLF",
        "Format",
    ),
    (
        "format.lineEndingsCr",
        "Convert Line Endings to CR",
        "Format",
    ),
    ("search.openFind", "Find…", "Search"),
    ("search.openReplace", "Replace…", "Search"),
    ("search.openFindInFiles", "Find in Files…", "Search"),
    ("search.openReplaceInFiles", "Replace in Files…", "Search"),
    ("search.findNext", "Find Next", "Search"),
    ("search.findPrevious", "Find Previous", "Search"),
    ("search.goToLine", "Go to Line…", "Search"),
    ("search.matchBrace", "Go to Matching Bracket", "Search"),
    ("search.nextChange", "Next Change", "Search"),
    ("search.previousChange", "Previous Change", "Search"),
    ("search.navigateBack", "Go Back", "Search"),
    ("search.navigateForward", "Go Forward", "Search"),
    ("search.nextBookmark", "Next Bookmark", "Search"),
    ("search.previousBookmark", "Previous Bookmark", "Search"),
    ("search.toggleBookmark", "Toggle Bookmark", "Search"),
    ("search.listBookmarks", "List Bookmarks", "Search"),
    ("search.clearBookmarks", "Clear All Bookmarks", "Search"),
    (
        "search.deleteBookmarkedLines",
        "Delete Bookmarked Lines",
        "Search",
    ),
];

fn register_workbench_commands(commands: &mut CommandRegistry) {
    for (id, title, category) in WORKBENCH_ACTIONS {
        let action = id.rsplit('.').next().unwrap_or(id);
        commands
            .register(
                CommandDescriptor::new(*id, *title, *category)
                    .expect("built-in command ID is valid"),
                move |_context, _arguments| Ok(json!({ "action": action })),
            )
            .expect("built-in command is unique");
    }
}

fn register_compare_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("compare.documents", "Compare Open Documents", "Compare")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: CompareDocumentsArguments = parse_arguments(arguments)?;
                let result = manager
                    .compare_documents(&DocumentCompareRequest {
                        left_id: arguments.left_document_id,
                        right_id: arguments.right_document_id,
                        mode: arguments.mode,
                        left_start_byte: arguments.left_start_byte,
                        right_start_byte: arguments.right_start_byte,
                        left_start_line: arguments.left_start_line,
                        right_start_line: arguments.right_start_line,
                        max_bytes: arguments.max_bytes,
                        max_changes: arguments.max_changes,
                    })
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    commands
        .register(
            CommandDescriptor::new("compare.folders", "Compare Folders…", "Compare")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: CompareFoldersArguments = parse_arguments(arguments)?;
                let result = compare_folders(
                    std::path::Path::new(&arguments.left_root),
                    std::path::Path::new(&arguments.right_root),
                    arguments.max_results,
                )
                .map_err(|error| CommandError::Failed(error.to_string()))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("compare.applyChange", "Apply Compare Change", "Compare")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: ApplyCompareChangeArguments = parse_arguments(arguments)?;
                let result = manager
                    .apply_compare_change(&ApplyCompareRequest {
                        source_id: arguments.source_document_id,
                        target_id: arguments.target_document_id,
                        expected_target_revision: arguments.expected_target_revision,
                        source_start: arguments.source_start,
                        source_end: arguments.source_end,
                        target_start: arguments.target_start,
                        target_end: arguments.target_end,
                    })
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

/// Pointer-driven word selection and drag-move, also reachable from the keyboard.
/// Built-in JSON and XML tools for the document in the command context.
fn register_tool_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "tools.validate",
                "Validate JSON / XML / YAML / plist",
                "Tools",
            )
            .expect("built-in command ID is valid"),
            move |context, _arguments| {
                let report = manager
                    .validate_document(&context_document(&context)?)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(report).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
    for (id, title, minify) in [
        (
            "tools.beautify",
            "Beautify JSON / XML / YAML / plist",
            false,
        ),
        ("tools.minify", "Minify JSON", true),
    ] {
        let manager = Arc::clone(documents);
        commands
            .register(
                CommandDescriptor::new(id, title, "Tools").expect("built-in command ID is valid"),
                move |context, arguments| {
                    let arguments: FormatArguments = parse_arguments(arguments)?;
                    let result = manager
                        .format_document(
                            &context_document(&context)?,
                            arguments.expected_revision,
                            minify,
                            arguments.accept_loss,
                        )
                        .map_err(|error| editor_command_error(&error))?;
                    serde_json::to_value(result)
                        .map_err(|error| serialization_command_error(&error))
                },
            )
            .expect("built-in command is unique");
    }
}

fn register_statistics_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.characterCount", "Count Characters", "Editor")
                .expect("built-in command ID is valid"),
            move |context, _arguments| {
                let count = manager
                    .character_count(&context_document(&context)?)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(count).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_pointer_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.selectWordAt", "Select Word", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: WordAtArguments = parse_arguments(arguments)?;
                let selection = manager
                    .select_word_at(
                        &document_id,
                        arguments.expected_revision,
                        arguments.byte_offset,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(selection).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.moveSelectedText", "Move Selected Text", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: MoveSelectedTextArguments = parse_arguments(arguments)?;
                let result = manager
                    .move_selected_text(
                        &document_id,
                        arguments.expected_revision,
                        arguments.start_byte,
                        arguments.end_byte,
                        arguments.destination_byte,
                        arguments.copy,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_editor_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    register_file_commands(commands, documents);
    print_jobs::register_print_commands(commands, documents);
    register_recovery_commands(commands, documents);
    register_text_transfer_commands(commands, documents);
    register_navigation_commands(commands, documents);
    register_transform_and_bookmark_commands(commands, documents);
    register_search_commands(commands, documents);
    count_jobs::register_count_commands(commands, documents);
    register_rectangular_selection_command(commands, documents);
    register_language_commands(commands, documents);
    register_hex_commands(commands, documents);

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.deleteBackward", "Delete Backward", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: RevisionArguments = parse_arguments(arguments)?;
                let result = manager
                    .delete_backward(&document_id, arguments.expected_revision)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    register_editing_extensions(commands, documents);

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.replace", "Replace Selection", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: ReplaceArguments = parse_arguments(arguments)?;
                let result = manager
                    .replace(
                        &document_id,
                        arguments.expected_revision,
                        arguments.start_byte,
                        arguments.end_byte,
                        &arguments.text,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.setSelections", "Set Selections", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: SelectionArguments = parse_arguments(arguments)?;
                let result = manager
                    .set_selections(&document_id, arguments.selections)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    register_pointer_commands(commands, documents);
    register_tool_commands(commands, documents);
    register_statistics_commands(commands, documents);

    for (id, title, action) in [
        (
            "editor.undo",
            "Undo",
            DocumentManager::undo
                as fn(&DocumentManager, &DocumentId) -> Result<DocumentSummary, EditorError>,
        ),
        (
            "editor.redo",
            "Redo",
            DocumentManager::redo
                as fn(&DocumentManager, &DocumentId) -> Result<DocumentSummary, EditorError>,
        ),
        (
            "editor.toggleReadOnly",
            "Toggle Read-Only Mode",
            DocumentManager::toggle_read_only
                as fn(&DocumentManager, &DocumentId) -> Result<DocumentSummary, EditorError>,
        ),
    ] {
        let manager = Arc::clone(documents);
        commands
            .register(
                CommandDescriptor::new(id, title, "Editor").expect("built-in command ID is valid"),
                move |context, _arguments| {
                    let result = action(&manager, &context_document(&context)?)
                        .map_err(|error| editor_command_error(&error))?;
                    serde_json::to_value(result)
                        .map_err(|error| serialization_command_error(&error))
                },
            )
            .expect("built-in command is unique");
    }
}

fn register_hex_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("hex.typeNibble", "Type Hexadecimal Digit", "Hex Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: HexNibbleArguments = parse_arguments(arguments)?;
                let result = manager
                    .type_hex_nibble(
                        &document_id,
                        arguments.expected_revision,
                        arguments.high_nibble,
                        arguments.nibble,
                        arguments.overwrite,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "hex.replaceBytes",
                "Replace Hexadecimal Bytes",
                "Hex Editor",
            )
            .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: HexReplaceArguments = parse_arguments(arguments)?;
                let result = manager
                    .replace_hex_bytes(
                        &document_id,
                        arguments.expected_revision,
                        &arguments.hex,
                        arguments.overwrite,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("hex.deleteBackward", "Delete Previous Byte", "Hex Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: RevisionArguments = parse_arguments(arguments)?;
                let result = manager
                    .delete_hex_backward(&document_id, arguments.expected_revision)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "hex.selection",
                "Read Selected Hexadecimal Bytes",
                "Hex Editor",
            )
            .expect("built-in command ID is valid"),
            move |context, _arguments| {
                let result = manager
                    .hex_selection(&context_document(&context)?)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_language_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("language.list", "List Languages", "Language")
                .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                serde_json::to_value(manager.language_descriptors())
                    .map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("language.set", "Set Document Language", "Language")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: LanguageArguments = parse_arguments(arguments)?;
                manager
                    .set_language(&document_id, &arguments.language_id)
                    .map(Value::String)
                    .map_err(|error| editor_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_rectangular_selection_command(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "editor.setRectangularSelection",
                "Set Rectangular Selection",
                "Editor",
            )
            .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: RectangularSelectionArguments = parse_arguments(arguments)?;
                let result = manager
                    .set_rectangular_selection(
                        &document_id,
                        arguments.anchor_byte,
                        arguments.active_byte,
                        arguments.anchor_column,
                        arguments.active_column,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_search_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("search.find", "Find", "Search")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: FindArguments = parse_arguments(arguments)?;
                let result = manager
                    .find(
                        &document_id,
                        &arguments.query,
                        arguments.options,
                        arguments.start_byte,
                        arguments.direction,
                        arguments.wrap,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("search.replaceAll", "Replace All", "Search")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: ReplaceAllArguments = parse_arguments(arguments)?;
                let result = manager
                    .replace_all(
                        &document_id,
                        arguments.expected_revision,
                        &arguments.query,
                        &arguments.replacement,
                        arguments.options,
                        arguments.within.map(ByteRangeArguments::pair),
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("search.replaceMatch", "Replace Match", "Search")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: ReplaceMatchArguments = parse_arguments(arguments)?;
                let result = manager
                    .replace_match(
                        &document_id,
                        arguments.expected_revision,
                        &arguments.query,
                        &arguments.replacement,
                        arguments.options,
                        arguments.start_byte,
                        arguments.end_byte,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("search.findInFiles", "Find in Files", "Search")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: FindInFilesArguments = parse_arguments(arguments)?;
                let result = manager
                    .find_in_files(
                        std::path::Path::new(&arguments.root),
                        &arguments.query,
                        arguments.options,
                        &arguments.scope,
                        arguments.max_results,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_navigation_commands_m26(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.changeNear", "Find Nearby Change", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: ChangeNearArguments = parse_arguments(arguments)?;
                let result = manager
                    .change_near(&document_id, arguments.from_byte, arguments.direction)
                    .map_err(|error| editor_command_error(&error))?;
                Ok(result.map_or(Value::Null, |offset| Value::String(offset.to_string())))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.bookmarkLines", "List Bookmarks", "Editor")
                .expect("built-in command ID is valid"),
            move |context, _arguments| {
                let result = manager
                    .bookmark_lines(&context_document(&context)?)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "editor.deleteBookmarkedLines",
                "Delete Bookmarked Lines",
                "Editor",
            )
            .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: RevisionArguments = parse_arguments(arguments)?;
                let result = manager
                    .delete_bookmarked_lines(&document_id, arguments.expected_revision)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_external_change_commands(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "editor.externalChanges",
                "Check for Outside Changes",
                "File",
            )
            .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                let result = manager
                    .external_changes()
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("file.reload", "Reload from Disk", "File")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: ReloadArguments = if arguments.is_null() {
                    ReloadArguments::default()
                } else {
                    parse_arguments(arguments)?
                };
                let result = manager
                    .reload(&document_id, arguments.discard)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "editor.keepExternalVersion",
                "Keep the Editor's Version",
                "File",
            )
            .expect("built-in command ID is valid"),
            move |context, _arguments| {
                let result = manager
                    .keep_external_version(&context_document(&context)?)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_replace_in_files_commands(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "search.previewReplaceInFiles",
                "Preview Replace in Files",
                "Search",
            )
            .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: PreviewReplaceInFilesArguments = parse_arguments(arguments)?;
                let result = manager
                    .preview_replace_in_files(
                        std::path::Path::new(&arguments.root),
                        &arguments.query,
                        arguments.options,
                        &arguments.scope,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("search.replaceInFiles", "Replace in Files", "Search")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: ReplaceInFilesArguments = parse_arguments(arguments)?;
                let result = manager
                    .replace_in_files(
                        &arguments.files,
                        &arguments.query,
                        &arguments.replacement,
                        arguments.options,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_find_all_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("search.findAll", "Find All", "Search")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: FindAllArguments = parse_arguments(arguments)?;
                let result = manager
                    .find_all(
                        &document_id,
                        &arguments.query,
                        arguments.options,
                        arguments.within.map(ByteRangeArguments::pair),
                        arguments.max_matches,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    commands
        .register(
            CommandDescriptor::new("search.validatePattern", "Check Search Pattern", "Search")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: ValidatePatternArguments = parse_arguments(arguments)?;
                // The search error alone reads best next to the pattern it describes.
                nulis_search::validate_pattern(arguments.query.as_bytes(), arguments.options)
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                Ok(Value::Null)
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("search.matchesInRange", "Matches in Range", "Search")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: MatchesInRangeArguments = parse_arguments(arguments)?;
                let result = manager
                    .matches_in_range(
                        &document_id,
                        &arguments.query,
                        arguments.options,
                        arguments.start_byte,
                        arguments.end_byte,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.addBookmarks", "Add Bookmarks", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: AddBookmarksArguments = parse_arguments(arguments)?;
                let offsets = arguments
                    .offsets
                    .iter()
                    .map(|offset| offset.parse::<u64>())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| {
                        CommandError::Failed("offsets must be decimal byte offsets".into())
                    })?;
                let result = manager
                    .add_bookmarks(&document_id, &offsets)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result.iter().map(u64::to_string).collect::<Vec<_>>())
                    .map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_transform_and_bookmark_commands(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.setEncoding", "Set Text Encoding", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: EncodingArguments = parse_arguments(arguments)?;
                let result = manager
                    .set_encoding(&document_id, arguments.encoding)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "editor.transformSelections",
                "Transform Selections",
                "Editor",
            )
            .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: TransformArguments = parse_arguments(arguments)?;
                let result = manager
                    .transform_selections(
                        &document_id,
                        arguments.expected_revision,
                        arguments.transform,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.toggleBookmark", "Toggle Bookmark", "Editor")
                .expect("built-in command ID is valid"),
            move |context, _arguments| {
                let result = manager
                    .toggle_bookmark(&context_document(&context)?)
                    .map_err(|error| editor_command_error(&error))?;
                Ok(Value::Array(
                    result
                        .into_iter()
                        .map(|offset| Value::String(offset.to_string()))
                        .collect(),
                ))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.clearBookmarks", "Clear Bookmarks", "Editor")
                .expect("built-in command ID is valid"),
            move |context, _arguments| {
                manager
                    .clear_bookmarks(&context_document(&context)?)
                    .map_err(|error| editor_command_error(&error))?;
                Ok(Value::Null)
            },
        )
        .expect("built-in command is unique");
}

/// Line, search, file-change, navigation, and line-ending commands added in M20–M27.
fn register_editing_extensions(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    register_line_commands(commands, documents);
    register_find_all_commands(commands, documents);
    register_replace_in_files_commands(commands, documents);
    register_external_change_commands(commands, documents);
    register_navigation_commands_m26(commands, documents);
    register_line_ending_commands(commands, documents);
    register_column_commands(commands, documents);
}

fn register_column_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "editor.insertNumberSequence",
                "Insert Number Sequence",
                "Editor",
            )
            .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: NumberSequenceArguments = parse_arguments(arguments)?;
                let result = manager
                    .insert_number_sequence(
                        &document_id,
                        arguments.expected_revision,
                        arguments.sequence,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.sumNumbers", "Sum Numbers", "Editor")
                .expect("built-in command ID is valid"),
            move |context, _arguments| {
                let result = manager
                    .sum_numbers(&context_document(&context)?)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.justifyColumns", "Justify Columns", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: JustifyArguments = parse_arguments(arguments)?;
                let result = manager
                    .justify_columns(&document_id, arguments.expected_revision, arguments.justify)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_line_ending_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "editor.convertLineEndings",
                "Convert Line Endings",
                "Editor",
            )
            .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: ConvertLineEndingsArguments = parse_arguments(arguments)?;
                let result = manager
                    .convert_line_endings(
                        &document_id,
                        arguments.expected_revision,
                        arguments.target,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_line_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.deleteForward", "Delete Forward", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: RevisionArguments = parse_arguments(arguments)?;
                let result = manager
                    .delete_forward(&document_id, arguments.expected_revision)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.selectAll", "Select All", "Editor")
                .expect("built-in command ID is valid"),
            move |context, _arguments| {
                let result = manager
                    .select_all(&context_document(&context)?)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.lineOperation", "Line Operation", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: LineOperationArguments = parse_arguments(arguments)?;
                let result = manager
                    .line_operation_with(
                        &document_id,
                        arguments.expected_revision,
                        arguments.operation,
                        arguments.options,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_navigation_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.moveHorizontal", "Move Caret Horizontally", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: MoveArguments = parse_arguments(arguments)?;
                let result = manager
                    .move_horizontal(&document_id, arguments.direction, arguments.extend)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.moveVertical", "Move Caret Vertically", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: VerticalMoveArguments = parse_arguments(arguments)?;
                let result = manager
                    .move_vertical(&document_id, arguments.direction, arguments.extend)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.moveToBookmark", "Move to Bookmark", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: BookmarkMoveArguments = parse_arguments(arguments)?;
                let result = manager
                    .move_to_bookmark(&document_id, arguments.direction)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_text_transfer_commands(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.insertText", "Insert Text", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: InsertTextArguments = parse_arguments(arguments)?;
                let result = manager
                    .insert_text(
                        &document_id,
                        arguments.expected_revision,
                        &arguments.text,
                        arguments.overwrite,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.selectionText", "Read Selected Text", "Editor")
                .expect("built-in command ID is valid"),
            move |context, _arguments| {
                let result = manager
                    .selection_text(&context_document(&context)?)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("editor.deleteSelection", "Delete Selection", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: RevisionArguments = parse_arguments(arguments)?;
                let result = manager
                    .delete_selection(&document_id, arguments.expected_revision)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_file_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("file.open", "Open File…", "File")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: OpenFileArguments = parse_arguments(arguments)?;
                let result = manager
                    .open_file(std::path::Path::new(&arguments.path))
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("file.new", "New Text File", "File")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: NewDocumentArguments = parse_arguments(arguments)?;
                let result = manager
                    .create_untitled(arguments.name, arguments.initial_text)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("file.save", "Save", "File")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: RevisionArguments = parse_arguments(arguments)?;
                let result = manager
                    .save(&document_id, arguments.expected_revision)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("file.renameTo", "Rename File To", "File")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: RenameFileArguments = parse_arguments(arguments)?;
                let result = manager
                    .rename_file(&document_id, &arguments.name)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("file.saveAs", "Save As…", "File")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: SaveAsArguments = parse_arguments(arguments)?;
                let result = manager
                    .save_as(
                        &document_id,
                        arguments.expected_revision,
                        std::path::Path::new(&arguments.path),
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

fn register_recovery_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("file.close", "Close Document", "File")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: CloseDocumentArguments = parse_arguments(arguments)?;
                manager
                    .close_document(&document_id, arguments.discard)
                    .map_err(|error| editor_command_error(&error))?;
                Ok(Value::Null)
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("recovery.checkpoint", "Checkpoint Document", "File")
                .expect("built-in command ID is valid"),
            move |context, _arguments| {
                manager
                    .checkpoint_recovery(&context_document(&context)?)
                    .map_err(|error| editor_command_error(&error))?;
                Ok(Value::Null)
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("recovery.list", "List Recoverable Documents", "File")
                .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                serde_json::to_value(
                    manager
                        .recovery_candidates()
                        .map_err(|error| editor_command_error(&error))?,
                )
                .map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("recovery.restore", "Restore Document", "File")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: RecoveryIdArguments = parse_arguments(arguments)?;
                serde_json::to_value(
                    manager
                        .restore_recovery(&arguments.recovery_id)
                        .map_err(|error| editor_command_error(&error))?,
                )
                .map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("recovery.discard", "Discard Recovery", "File")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: RecoveryIdArguments = parse_arguments(arguments)?;
                manager
                    .discard_recovery(&arguments.recovery_id)
                    .map_err(|error| editor_command_error(&error))?;
                Ok(Value::Null)
            },
        )
        .expect("built-in command is unique");
}

fn parse_arguments<T: for<'de> Deserialize<'de>>(arguments: Value) -> Result<T, CommandError> {
    serde_json::from_value(arguments)
        .map_err(|error| CommandError::Failed(format!("invalid command arguments: {error}")))
}

fn context_document(context: &CommandContext) -> Result<DocumentId, CommandError> {
    context
        .document_id
        .clone()
        .map(DocumentId)
        .ok_or_else(|| CommandError::Failed("command requires a document context".into()))
}

fn editor_command_error(error: &EditorError) -> CommandError {
    CommandError::Failed(error.to_string())
}

#[allow(clippy::needless_pass_by_value)] // `Result::map_err` transfers ownership.
fn terminal_command_error(error: nulis_terminal::TerminalError) -> CommandError {
    CommandError::Failed(error.to_string())
}

#[allow(clippy::needless_pass_by_value)] // `Result::map_err` transfers ownership.
fn automation_command_error(error: AutomationError) -> CommandError {
    CommandError::Failed(error.to_string())
}

#[allow(clippy::needless_pass_by_value)] // `Result::map_err` transfers ownership.
fn plugin_command_error(error: PluginError) -> CommandError {
    CommandError::Failed(error.to_string())
}

fn serialization_command_error(error: &serde_json::Error) -> CommandError {
    CommandError::Failed(format!("command result serialization failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{CommandContext, NulisCore, APPLICATION_ID, IPC_PROTOCOL_VERSION};
    use serde_json::Value;
    use std::{sync::Arc, thread, time::Duration};

    #[test]
    fn exposes_centralized_identity_and_protocol() {
        let info = NulisCore::default().application_info();
        assert_eq!(info.application_id, APPLICATION_ID);
        assert_eq!(info.ipc_protocol_version, IPC_PROTOCOL_VERSION);
    }

    #[test]
    fn built_in_command_executes_headlessly() {
        let result = NulisCore::default()
            .execute_command("app.about", CommandContext::default(), Value::Null)
            .expect("command executes");
        assert_eq!(result["productName"], "Nulis");
    }

    #[test]
    fn export_pdf_reads_every_window_and_refuses_unsafe_destinations() {
        let application_data = tempfile::tempdir().expect("application data");
        let folder = tempfile::tempdir().expect("folder");
        let core = NulisCore::with_session_root(application_data.path().into());
        let open = |name: &str, text: &str| {
            let path = folder.path().join(name);
            std::fs::write(&path, text).expect("write source");
            let opened = core
                .execute_command(
                    "file.open",
                    CommandContext::default(),
                    serde_json::json!({ "path": path.display().to_string() }),
                )
                .expect("open");
            (
                path,
                CommandContext {
                    window_id: Some("main".into()),
                    document_id: Some(opened["documentId"].as_str().expect("document ID").into()),
                },
            )
        };
        let export = |context: &CommandContext, arguments: serde_json::Value| {
            core.execute_command("file.exportPdfTo", context.clone(), arguments)
        };

        // 40,000 short lines (about 430 KB) take two reading windows; the
        // default A4 page holds 61 rows.
        let mut text = String::new();
        for line in 1..=40_000 {
            text.push_str("line ");
            text.push_str(&line.to_string());
            text.push('\n');
        }
        let (source, context) = open("lines.txt", &text);
        let pdf = folder.path().join("lines.pdf");
        let result = export(
            &context,
            serde_json::json!({ "expectedRevision": "0", "path": pdf.display().to_string(), "date": "today" }),
        )
        .expect("export");
        // 40,001 lines: the newline at the end starts an empty last line.
        assert_eq!(result["pages"], "656");
        assert_eq!(result["missingGlyphs"], "0");
        let bytes = std::fs::read(&pdf).expect("read PDF");
        assert!(bytes.starts_with(b"%PDF-1.7"));
        assert_eq!(result["bytes"], bytes.len().to_string());
        let leftovers = std::fs::read_dir(folder.path())
            .expect("list")
            .filter(|entry| {
                entry.as_ref().is_ok_and(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".nulis-export-")
                })
            })
            .count();
        assert_eq!(leftovers, 0, "the temporary file is moved into place");

        // Refusals: the document's own file, a stale revision, a relative path.
        for (path, revision) in [
            (source.display().to_string(), "0"),
            (pdf.display().to_string(), "7"),
            ("relative.pdf".to_owned(), "0"),
        ] {
            assert!(
                export(
                    &context,
                    serde_json::json!({ "expectedRevision": revision, "path": path })
                )
                .is_err(),
                "{path} at {revision}"
            );
        }
        assert_eq!(std::fs::read_to_string(&source).expect("source"), text);

        // One 700 KiB line with no line break is read in three windows that
        // continue each other: 716,800 characters over 90 columns is 7,965
        // rows, 131 pages, with no page count in the footer.
        let (_, long_context) = open("long.txt", &"x".repeat(700 * 1024));
        let result = export(
            &long_context,
            serde_json::json!({
                "expectedRevision": "0",
                "path": folder.path().join("long.pdf").display().to_string(),
                "setup": { "syntaxHighlighting": false, "footer": { "center": "{page}" } },
            }),
        )
        .expect("export long line");
        assert_eq!(result["pages"], "131");
    }

    /// Opens a document of `rows` numbered lines in `folder`.
    fn open_rows(core: &NulisCore, folder: &std::path::Path, rows: u32) -> CommandContext {
        let source = folder.join("rows.txt");
        let mut text = String::new();
        for line in 1..=rows {
            text.push_str("row ");
            text.push_str(&line.to_string());
            text.push('\n');
        }
        std::fs::write(&source, &text).expect("write source");
        let opened = core
            .execute_command(
                "file.open",
                CommandContext::default(),
                serde_json::json!({ "path": source.display().to_string() }),
            )
            .expect("open");
        CommandContext {
            window_id: Some("main".into()),
            document_id: Some(opened["documentId"].as_str().expect("document ID").into()),
        }
    }

    /// Cancels at its first progress report.
    #[derive(Default)]
    struct CancelAtOnce(std::sync::atomic::AtomicBool);

    impl crate::editor::ExportObserver for CancelAtOnce {
        fn cancelled(&self) -> bool {
            self.0.load(std::sync::atomic::Ordering::Relaxed)
        }

        fn report(&self, _progress: crate::editor::ExportProgress) {
            self.0.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    #[test]
    fn export_jobs_ask_before_very_large_pdfs() {
        let application_data = tempfile::tempdir().expect("application data");
        let folder = tempfile::tempdir().expect("folder");
        let core = NulisCore::with_session_root(application_data.path().into());
        let context = open_rows(&core, folder.path(), 70_000);
        let pdf = folder.path().join("big.pdf");
        let run = |confirmed: bool| {
            let started = core
                .execute_command(
                    "file.exportPdfStart",
                    context.clone(),
                    serde_json::json!({
                        "expectedRevision": "0",
                        "path": pdf.display().to_string(),
                        "confirmed": confirmed,
                        "setup": { "syntaxHighlighting": false },
                    }),
                )
                .expect("start");
            let job = serde_json::json!({ "jobId": started["jobId"] });
            for _ in 0..6_000 {
                let report = core
                    .execute_command(
                        "file.exportPdfProgress",
                        CommandContext::default(),
                        job.clone(),
                    )
                    .expect("progress");
                if report["done"] == true {
                    return report;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("the export did not finish");
        };
        // 70,001 lines at 61 rows a page: at least 1,148 pages.
        let asked = run(false);
        assert_eq!(asked["needsConfirmation"], "1148");
        assert!(!pdf.exists(), "nothing is written before the user agrees");
        let done = run(true);
        assert_eq!(done["result"]["pages"], "1148");
        assert_eq!(done["progress"]["phase"], "writing");
        assert_eq!(done["progress"]["done"], done["progress"]["total"]);
        assert!(pdf.exists());
    }

    /// Starts a Print and waits for its report.
    fn run_print(core: &NulisCore, context: &CommandContext, confirmed: bool) -> serde_json::Value {
        let started = core
            .execute_command(
                "file.printStart",
                context.clone(),
                serde_json::json!({
                    "expectedRevision": "0",
                    "confirmed": confirmed,
                    "date": "today",
                    "setup": { "syntaxHighlighting": false },
                }),
            )
            .expect("start");
        let job = serde_json::json!({ "jobId": started["jobId"] });
        for _ in 0..6_000 {
            let report = core
                .execute_command(
                    "file.exportPdfProgress",
                    CommandContext::default(),
                    job.clone(),
                )
                .expect("progress");
            if report["done"] == true {
                return report;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the print layout did not finish");
    }

    #[test]
    fn print_lays_out_the_same_pages_as_the_pdf_and_hands_them_out_in_batches() {
        let application_data = tempfile::tempdir().expect("application data");
        let folder = tempfile::tempdir().expect("folder");
        let core = NulisCore::with_session_root(application_data.path().into());
        let context = open_rows(&core, folder.path(), 200);
        let pdf = folder.path().join("rows.pdf");
        let exported = core
            .execute_command(
                "file.exportPdfTo",
                context.clone(),
                serde_json::json!({
                    "expectedRevision": "0",
                    "path": pdf.display().to_string(),
                    "date": "today",
                    "setup": { "syntaxHighlighting": false },
                }),
            )
            .expect("export");
        let report = run_print(&core, &context, false);
        assert_eq!(report["printedPages"], exported["pages"]);
        assert_eq!(report["printedPages"], "4");

        let read = |start: usize, count: usize| {
            core.execute_command(
                "print.pages",
                CommandContext::default(),
                serde_json::json!({ "start": start, "count": count }),
            )
        };
        let batch = read(1, 2).expect("pages");
        assert_eq!(batch["total"], 4);
        assert_eq!(batch["start"], 1);
        assert_eq!(batch["title"], "rows.txt");
        assert!((batch["frame"]["widthMm"].as_f64().expect("width") - 210.0).abs() < 0.01);
        let pages = batch["pages"].as_array().expect("pages");
        assert_eq!(pages.len(), 2);
        let second: serde_json::Value =
            serde_json::from_str(pages[0].as_str().expect("page JSON")).expect("page");
        assert_eq!(second["number"], 2);
        // 61 rows a page: page 2 starts with line 62.
        assert_eq!(second["rows"][0][0][3], " 62");
        assert_eq!(second["rows"][0][1][3], "row 62");
        assert_eq!(second["header"][0][3], "rows.txt");
        assert_eq!(second["footer"][0][3], "Page 2 of 4");
        // Reading past the end returns no pages.
        assert_eq!(
            read(9, 5).expect("past the end")["pages"],
            serde_json::json!([])
        );

        core.execute_command(
            "print.release",
            CommandContext::default(),
            serde_json::Value::Null,
        )
        .expect("release");
        assert!(read(0, 1).is_err(), "released pages are gone");
    }

    #[test]
    fn the_page_setup_preview_lays_out_only_the_first_page() {
        let application_data = tempfile::tempdir().expect("application data");
        let folder = tempfile::tempdir().expect("folder");
        let core = NulisCore::with_session_root(application_data.path().into());
        let context = open_rows(&core, folder.path(), 70_000);
        let preview = core
            .execute_command(
                "file.printPreview",
                context.clone(),
                serde_json::json!({
                    "expectedRevision": "0",
                    "date": "today",
                    "setup": { "orientation": "landscape", "fontSize": 12 },
                }),
            )
            .expect("preview");
        let frame = &preview["frame"];
        assert!((frame["widthMm"].as_f64().expect("width") - 297.0).abs() < 0.01);
        assert_eq!(frame["landscape"], true);
        assert_eq!(preview["page"]["number"], 1);
        assert_eq!(preview["page"]["rows"][0][1][3], "row 1");
        // The page count is estimated, not counted: no pass over 70,000 lines.
        let footer = preview["page"]["footer"][0][3].as_str().expect("footer");
        assert!(footer.starts_with("Page 1 of "), "{footer}");
        // A setup the core refuses is reported, not shown.
        assert!(core
            .execute_command(
                "file.printPreview",
                context,
                serde_json::json!({ "expectedRevision": "0", "setup": { "fontSize": 99 } }),
            )
            .is_err());
    }

    #[test]
    fn print_asks_above_a_thousand_pages_and_refuses_above_five_thousand() {
        let application_data = tempfile::tempdir().expect("application data");
        let folder = tempfile::tempdir().expect("folder");
        let core = NulisCore::with_session_root(application_data.path().into());
        let context = open_rows(&core, folder.path(), 70_000);
        assert_eq!(
            run_print(&core, &context, false)["needsConfirmation"],
            "1148"
        );
        let confirmed = run_print(&core, &context, true);
        assert_eq!(confirmed["printedPages"], "1148");

        let context = open_rows(&core, folder.path(), 400_000);
        let refused = run_print(&core, &context, true);
        let error = refused["error"].as_str().expect("refused");
        assert!(
            error.starts_with("Could not print: this needs at least 6,558 pages"),
            "{error}"
        );
        assert!(error.contains("Print takes at most 5,000"), "{error}");
    }

    #[test]
    fn a_comment_across_a_highlight_window_edge_keeps_its_color() {
        let application_data = tempfile::tempdir().expect("application data");
        let folder = tempfile::tempdir().expect("folder");
        let core = NulisCore::with_session_root(application_data.path().into());
        // Filler up to 300 bytes before the first window edge, then a block
        // comment that runs well past it.
        let edge = usize::try_from(crate::HIGHLIGHT_WINDOW_BYTES).expect("small");
        let mut text = String::new();
        while text.len() < edge - 300 {
            text.push_str("int x;\n");
        }
        text.push_str("/* a comment that starts before the edge\n");
        for _ in 0..20 {
            text.push_str("   still inside the comment\n");
        }
        text.push_str("*/\nint after;\n");
        let source = folder.path().join("edge.c");
        std::fs::write(&source, &text).expect("write source");
        let opened = core
            .execute_command(
                "file.open",
                CommandContext::default(),
                serde_json::json!({ "path": source.display().to_string() }),
            )
            .expect("open");
        let document_id =
            crate::editor::DocumentId(opened["documentId"].as_str().expect("id").into());
        let mut cursor = Some(crate::editor::ViewportCursor {
            byte_offset: 0,
            line: 1,
            line_continuation: false,
        });
        let mut windows = Vec::new();
        while let Some(at) = cursor {
            let (lines, next) = core
                .documents
                .print_window(&document_id, 0, &at, true)
                .expect("window");
            windows.push(lines);
            cursor = next;
        }
        assert!(windows.len() >= 2, "the comment crosses a window edge");
        let second = &windows[1];
        let inside = second
            .iter()
            .find(|line| line.text.contains("still inside"))
            .expect("comment line in the second window");
        assert!(
            inside
                .spans
                .iter()
                .any(|span| span.token == nulis_syntax::SyntaxToken::Comment && span.end > 6),
            "{inside:?}"
        );
        let after = second
            .iter()
            .find(|line| line.text == "int after;")
            .expect("code after the comment");
        assert!(after
            .spans
            .iter()
            .all(|span| span.token != nulis_syntax::SyntaxToken::Comment));
    }

    #[test]
    fn a_cancelled_export_writes_nothing() {
        let application_data = tempfile::tempdir().expect("application data");
        let folder = tempfile::tempdir().expect("folder");
        let core = NulisCore::with_session_root(application_data.path().into());
        let context = open_rows(&core, folder.path(), 5_000);
        let pdf = folder.path().join("cancelled.pdf");
        let outcome = core.documents.export_pdf(
            &crate::editor::DocumentId(context.document_id.clone().expect("document")),
            &crate::editor::PdfExportRequest {
                path: pdf.clone(),
                expected_revision: 0,
                setup: nulis_print::PageSetup::default(),
                colors: nulis_print::ColorChoice::Print,
                date: String::new(),
                confirm_above: None,
                scope: crate::editor::ExportScope::Document,
            },
            &CancelAtOnce::default(),
        );
        assert_eq!(outcome, Err(crate::EditorError::ExportCancelled));
        assert!(!pdf.exists());
        let leftovers = std::fs::read_dir(folder.path())
            .expect("list")
            .filter(|entry| {
                entry.as_ref().is_ok_and(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".nulis-export-")
                })
            })
            .count();
        assert_eq!(leftovers, 0, "a cancelled export leaves no temporary file");
    }

    #[test]
    fn file_save_commands_execute_headlessly() {
        let application_data = tempfile::tempdir().expect("application data");
        let destination = tempfile::tempdir().expect("destination");
        let path = destination.path().join("saved.txt");
        let core = NulisCore::with_session_root(application_data.path().into());
        let created = core
            .execute_command(
                "file.new",
                CommandContext::default(),
                serde_json::json!({ "name": "Untitled", "initialText": "alpha" }),
            )
            .expect("create document");
        let context = CommandContext {
            window_id: Some("main".into()),
            document_id: Some(created["documentId"].as_str().expect("document ID").into()),
        };

        let saved = core
            .execute_command(
                "file.saveAs",
                context.clone(),
                serde_json::json!({ "expectedRevision": "0", "path": path }),
            )
            .expect("save as");
        assert_eq!(saved["name"], "saved.txt");
        assert_eq!(
            std::fs::read_to_string(&path).expect("saved content"),
            "alpha"
        );

        core.execute_command(
            "editor.insertText",
            context.clone(),
            serde_json::json!({
                "expectedRevision": "0", "text": "beta", "overwrite": false
            }),
        )
        .expect("edit saved document");
        core.execute_command(
            "file.save",
            context,
            serde_json::json!({ "expectedRevision": "1" }),
        )
        .expect("save existing destination");
        assert_eq!(
            std::fs::read_to_string(path).expect("updated content"),
            "betaalpha"
        );
    }

    #[test]
    fn recorded_macros_persist_and_resolve_current_document_revisions() {
        let application_data = tempfile::tempdir().expect("application data");
        let core = NulisCore::with_session_root(application_data.path().into());
        let created = core
            .execute_command(
                "file.new",
                CommandContext::default(),
                serde_json::json!({ "name": "Macro target", "initialText": "" }),
            )
            .expect("create document");
        let document_id = created["documentId"].as_str().expect("document ID");
        let context = CommandContext {
            window_id: Some("main".into()),
            document_id: Some(document_id.into()),
        };
        core.execute_command(
            "automation.startRecording",
            context.clone(),
            serde_json::json!({ "id": "user.insert-x", "name": "Insert X" }),
        )
        .expect("start recording");
        core.execute_command(
            "editor.insertText",
            context.clone(),
            serde_json::json!({
                "expectedRevision": "0", "text": "x", "overwrite": false
            }),
        )
        .expect("recorded insert");
        let definition = core
            .execute_command("automation.stopRecording", context.clone(), Value::Null)
            .expect("stop recording");
        assert_eq!(
            definition["actions"][0]["arguments"]["expectedRevision"],
            "$nulis.currentRevision"
        );

        core.execute_command("editor.undo", context.clone(), Value::Null)
            .expect("undo recorded edit");
        let outcome = core
            .execute_command(
                "automation.runMacro",
                context,
                serde_json::json!({ "id": "user.insert-x" }),
            )
            .expect("run macro against current revision");
        assert_eq!(outcome["actionsCompleted"], 1);
        assert_eq!(outcome["document"]["byteLen"], "1");

        let reopened = NulisCore::with_session_root(application_data.path().into());
        let listed = reopened
            .execute_command(
                "automation.listMacros",
                CommandContext::default(),
                Value::Null,
            )
            .expect("list persisted macros");
        assert_eq!(listed[0]["id"], "user.insert-x");
        assert_eq!(listed[0]["actionCount"], 1);
    }

    #[test]
    fn scripts_require_persisted_approval_and_use_bounded_core_adapters() {
        let application_data = tempfile::tempdir().expect("application data");
        let core = NulisCore::with_session_root(application_data.path().into());
        let created = core
            .execute_command(
                "file.new",
                CommandContext::default(),
                serde_json::json!({ "name": "Script target", "initialText": "" }),
            )
            .expect("create document");
        let context = CommandContext {
            window_id: Some("main".into()),
            document_id: Some(created["documentId"].as_str().expect("document ID").into()),
        };
        core.execute_command(
            "automation.saveScript",
            context.clone(),
            serde_json::json!({
                "id": "user.edit",
                "definition": {
                    "apiVersion": 1,
                    "name": "Edit",
                    "source": r#"nulis_read_document(); nulis_edit_document("{\"expectedRevision\":\"$nulis.currentRevision\",\"startByte\":\"0\",\"endByte\":\"0\",\"text\":\"x\"}"); nulis_show_message("done"); nulis_print("ok");"#,
                    "requestedCapabilities": ["readDocument", "editDocument", "showMessage"],
                    "requestedCommands": []
                }
            }),
        )
        .expect("save script");
        assert!(core
            .execute_command(
                "automation.runScript",
                context.clone(),
                serde_json::json!({ "id": "user.edit" }),
            )
            .is_err());

        core.execute_command(
            "automation.approveScript",
            context.clone(),
            serde_json::json!({
                "id": "user.edit",
                "policy": {
                    "capabilities": ["readDocument", "editDocument", "showMessage"],
                    "allowedCommands": [],
                    "limits": {
                        "memoryBytes": 16_777_216,
                        "executionMillis": 1000,
                        "operations": 5_000_000,
                        "hostCalls": 1000,
                        "outputBytes": 65536
                    }
                }
            }),
        )
        .expect("approve script");
        let outcome = core
            .execute_command(
                "automation.runScript",
                context,
                serde_json::json!({ "id": "user.edit" }),
            )
            .expect("run script");
        assert_eq!(outcome["document"]["revision"], "1");
        assert_eq!(outcome["report"]["output"], "ok\n");
        assert_eq!(outcome["messages"], serde_json::json!(["done"]));

        let reopened = NulisCore::with_session_root(application_data.path().into());
        let listed = reopened
            .execute_command(
                "automation.listScripts",
                CommandContext::default(),
                Value::Null,
            )
            .expect("list scripts");
        assert_eq!(listed[0]["id"], "user.edit");
        assert_eq!(listed[0]["approved"], true);
    }

    #[test]
    fn scripts_use_symlink_safe_workspace_and_settings_adapters() {
        let application_data = tempfile::tempdir().expect("application data");
        let workspace = tempfile::tempdir().expect("workspace");
        let path = workspace.path().join("note.txt");
        std::fs::write(&path, "before").expect("workspace file");
        let core = NulisCore::with_session_root(application_data.path().into());
        core.execute_command(
            "workspace.open",
            CommandContext::default(),
            serde_json::json!({ "root": workspace.path() }),
        )
        .expect("open workspace");
        core.execute_command(
            "automation.saveScript",
            CommandContext::default(),
            serde_json::json!({
                "id": "user.workspace",
                "definition": {
                    "apiVersion": 1,
                    "name": "Workspace",
                    "source": r#"nulis_read_workspace("{\"path\":\"\",\"maxEntries\":10}"); nulis_write_workspace("{\"path\":\"note.txt\",\"contents\":\"after\"}"); nulis_print(nulis_read_settings());"#,
                    "requestedCapabilities": ["readWorkspace", "writeWorkspace", "readSettings"],
                    "requestedCommands": []
                }
            }),
        )
        .expect("save script");
        core.execute_command(
            "automation.approveScript",
            CommandContext::default(),
            serde_json::json!({
                "id": "user.workspace",
                "policy": {
                    "capabilities": ["readWorkspace", "writeWorkspace", "readSettings"],
                    "allowedCommands": [],
                    "limits": {
                        "memoryBytes": 16_777_216,
                        "executionMillis": 1000,
                        "operations": 5_000_000,
                        "hostCalls": 1000,
                        "outputBytes": 65536
                    }
                }
            }),
        )
        .expect("approve script");
        let outcome = core
            .execute_command(
                "automation.runScript",
                CommandContext::default(),
                serde_json::json!({ "id": "user.workspace" }),
            )
            .expect("run script");
        assert_eq!(
            std::fs::read_to_string(path).expect("updated file"),
            "after"
        );
        assert!(outcome["report"]["output"]
            .as_str()
            .expect("settings output")
            .contains("workbench"));
    }

    #[test]
    fn scripts_cannot_request_terminal_commands() {
        let core = NulisCore::default();
        let result = core.execute_command(
            "automation.saveScript",
            CommandContext::default(),
            serde_json::json!({
                "id": "user.unsafe",
                "definition": {
                    "apiVersion": 1,
                    "name": "Unsafe",
                    "source": "",
                    "requestedCapabilities": ["executeCommand"],
                    "requestedCommands": ["terminal.create"]
                }
            }),
        );
        assert!(result.is_err());
    }

    #[test]
    fn active_scripts_can_be_cancelled_through_the_command_boundary() {
        let core = Arc::new(NulisCore::default());
        core.execute_command(
            "automation.saveScript",
            CommandContext::default(),
            serde_json::json!({
                "id": "user.loop",
                "definition": {
                    "apiVersion": 1,
                    "name": "Loop",
                    "source": "loop {}",
                    "requestedCapabilities": [],
                    "requestedCommands": []
                }
            }),
        )
        .expect("save script");
        core.execute_command(
            "automation.approveScript",
            CommandContext::default(),
            serde_json::json!({
                "id": "user.loop",
                "policy": {
                    "capabilities": [],
                    "allowedCommands": [],
                    "limits": {
                        "memoryBytes": 16_777_216,
                        "executionMillis": 10_000,
                        "operations": 100_000_000,
                        "hostCalls": 1000,
                        "outputBytes": 65_536
                    }
                }
            }),
        )
        .expect("approve script");
        let runner = Arc::clone(&core);
        let running = thread::spawn(move || {
            runner.execute_command(
                "automation.runScript",
                CommandContext::default(),
                serde_json::json!({ "id": "user.loop" }),
            )
        });
        let mut cancelled = false;
        for _ in 0..100 {
            let result = core
                .execute_command(
                    "automation.cancelScript",
                    CommandContext::default(),
                    Value::Null,
                )
                .expect("cancel command");
            if result == Value::Bool(true) {
                cancelled = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(cancelled, "script did not become active");
        let result = running.join().expect("script thread");
        assert!(result
            .expect_err("script should be cancelled")
            .to_string()
            .contains("cancelled"));
    }

    #[test]
    fn recorder_excludes_non_editor_capability_commands() {
        let core = NulisCore::default();
        core.execute_command(
            "automation.startRecording",
            CommandContext::default(),
            serde_json::json!({ "id": "user.safe", "name": "Safe" }),
        )
        .expect("start recording");
        core.execute_command("app.about", CommandContext::default(), Value::Null)
            .expect("non-recordable command");
        assert!(core
            .execute_command(
                "automation.stopRecording",
                CommandContext::default(),
                Value::Null,
            )
            .is_err());
        let status = core
            .execute_command(
                "automation.recordingStatus",
                CommandContext::default(),
                Value::Null,
            )
            .expect("recording status");
        assert_eq!(status["actionCount"], 0);
        core.execute_command(
            "automation.cancelRecording",
            CommandContext::default(),
            Value::Null,
        )
        .expect("cancel recording");

        let unsafe_definition = super::MacroDefinition {
            schema_version: super::AUTOMATION_SCHEMA_VERSION,
            id: "user.unsafe".into(),
            name: "Unsafe".into(),
            actions: vec![super::RecordedCommand {
                command: "terminal.create".into(),
                arguments: serde_json::json!({}),
            }],
        };
        assert!(core
            .execute_macro(&unsafe_definition, &CommandContext::default(), || false,)
            .is_err());
    }

    #[test]
    fn workspace_and_session_commands_share_validated_core_state() {
        let application_data = tempfile::tempdir().expect("application data");
        let workspace = tempfile::tempdir().expect("workspace");
        std::fs::create_dir(workspace.path().join("src")).expect("source directory");
        std::fs::write(workspace.path().join("src/main.txt"), "hello").expect("source file");
        let core = NulisCore::with_session_root(application_data.path().into());

        let opened = core
            .execute_command(
                "workspace.open",
                CommandContext::default(),
                serde_json::json!({ "root": workspace.path() }),
            )
            .expect("open workspace");
        assert_eq!(
            opened["name"],
            workspace
                .path()
                .file_name()
                .expect("name")
                .to_string_lossy()
                .as_ref()
        );
        let listing = core
            .execute_command(
                "workspace.list",
                CommandContext::default(),
                serde_json::json!({ "path": "", "maxEntries": 10 }),
            )
            .expect("list workspace");
        assert_eq!(listing["entries"][0]["relativePath"], "src");
        let document = core
            .execute_command(
                "workspace.openFile",
                CommandContext::default(),
                serde_json::json!({ "path": "src/main.txt" }),
            )
            .expect("open workspace file");
        assert_eq!(document["byteLen"], "5");

        let source_path = document["sourcePath"].as_str().expect("source path");
        let reference = format!("path:{source_path}");
        let mut session = serde_json::json!({
            "schemaVersion": 2,
            "workspaceRoot": opened["root"],
            "documents": [reference],
            "layouts": [{
                "id": "layout-1",
                "name": "Writing",
                "root": { "kind": "pane", "id": "pane-1", "tabs": [reference], "activeTab": reference },
                "activePane": "pane-1"
            }],
            "activeLayout": "layout-1",
            "sidebarVisible": true,
            "sidebarWidth": 300,
            "activePanel": "files",
            "viewModes": {}
        });
        session["viewModes"][&reference] = serde_json::json!("text");
        core.execute_command("session.save", CommandContext::default(), session.clone())
            .expect("save session");
        let restored = core
            .execute_command("session.load", CommandContext::default(), Value::Null)
            .expect("load session");
        assert_eq!(restored, session);
    }

    #[test]
    fn workbench_layout_actions_are_centralized_commands() {
        let core = NulisCore::default();
        let command_ids = core
            .command_descriptors()
            .into_iter()
            .map(|command| command.id)
            .collect::<Vec<_>>();
        assert!(command_ids.contains(&"workbench.toggleSidebar".into()));
        assert!(command_ids.contains(&"workbench.toggleSplit".into()));
        assert!(command_ids.contains(&"workbench.toggleEditorMode".into()));
        let result = core
            .execute_command(
                "workbench.toggleSidebar",
                CommandContext::default(),
                Value::Null,
            )
            .expect("layout command");
        assert_eq!(result["action"], "toggleSidebar");
        for id in [
            "workspace.rename",
            "workspace.groupRight",
            "workspace.groupDown",
            "workspace.ungroup",
            "workspace.popOutPane",
            "workbench.moveTabLeft",
            "workbench.moveTabRight",
            "view.toggleLineChangeIndicators",
            "workbench.openSettings",
            "workbench.openHelp",
        ] {
            assert!(command_ids.contains(&id.into()), "{id} is registered");
        }
    }

    #[test]
    fn document_commands_and_viewports_share_headless_state() {
        let core = NulisCore::default();
        let created = core
            .execute_command(
                "file.new",
                CommandContext::default(),
                serde_json::json!({ "name": "Test", "initialText": "one\ntwo" }),
            )
            .expect("create document");
        let document_id = created["documentId"].as_str().expect("document ID");
        let snapshot = core
            .viewport(&super::ViewportRequest {
                document_id: super::DocumentId(document_id.into()),
                start_byte: 0,
                start_line: 1,
                line_continuation: false,
                max_bytes: 1024,
            })
            .expect("viewport");
        assert_eq!(snapshot.lines.len(), 2);
        assert_eq!(snapshot.lines[1].text, "two");

        let moved = core
            .execute_command(
                "editor.moveVertical",
                CommandContext {
                    window_id: None,
                    document_id: Some(document_id.into()),
                },
                serde_json::json!({ "direction": "down", "extend": false }),
            )
            .expect("move vertically through command boundary");
        assert_eq!(moved[0]["activeByte"], "4");
    }

    #[test]
    fn language_commands_list_modules_and_override_detection() {
        let core = NulisCore::default();
        let created = core
            .execute_command(
                "file.new",
                CommandContext::default(),
                serde_json::json!({ "name": "data.json", "initialText": "{\"ok\":true}" }),
            )
            .expect("create JSON document");
        let document_id = created["documentId"].as_str().expect("document ID");
        let languages = core
            .execute_command("language.list", CommandContext::default(), Value::Null)
            .expect("list languages");
        assert!(languages
            .as_array()
            .expect("language array")
            .iter()
            .any(|language| language["id"] == "json"));
        let selected = core
            .execute_command(
                "language.set",
                CommandContext {
                    window_id: None,
                    document_id: Some(document_id.into()),
                },
                serde_json::json!({ "languageId": "plaintext" }),
            )
            .expect("set language");
        assert_eq!(selected, "plaintext");
    }

    #[test]
    fn hexadecimal_commands_share_revision_checked_document_state() {
        let core = NulisCore::default();
        let created = core
            .execute_command(
                "file.new",
                CommandContext::default(),
                serde_json::json!({ "name": "bytes.bin", "initialText": "A" }),
            )
            .expect("create document");
        let document_id = created["documentId"].as_str().expect("document ID");
        let context = CommandContext {
            window_id: None,
            document_id: Some(document_id.into()),
        };
        let edit = core
            .execute_command(
                "hex.typeNibble",
                context.clone(),
                serde_json::json!({
                    "expectedRevision": "0",
                    "highNibble": true,
                    "nibble": 15,
                    "overwrite": true
                }),
            )
            .expect("hex command");
        assert_eq!(edit["document"]["revision"], "1");
        assert_eq!(edit["caret"]["byteOffset"], "0");
        assert!(core
            .execute_command(
                "hex.deleteBackward",
                context,
                serde_json::json!({ "expectedRevision": "0" }),
            )
            .is_err());
    }

    #[test]
    fn plugin_install_approval_and_revision_changes_control_command_visibility() {
        let application_data = tempfile::tempdir().expect("application data");
        let source = tempfile::tempdir().expect("plugin source");
        std::fs::write(
            source.path().join("plugin.json"),
            r#"{
              "id":"nulis.example",
              "name":"Example",
              "version":"1.0.0",
              "engines":{"nulis":">=0.1.0"},
              "permissions":["showMessage"],
              "activationEvents":["onCommand:nulis.example.hello"],
              "main":"index.js",
              "contributes":{"commands":[{"command":"nulis.example.hello","title":"Hello","category":"Examples"}]}
            }"#,
        )
        .expect("manifest");
        std::fs::write(source.path().join("index.js"), "function activate() {}").expect("source");
        let core = NulisCore::with_session_root(application_data.path().into());

        let installed = core
            .execute_command(
                "plugin.install",
                CommandContext::default(),
                serde_json::json!({ "path": source.path() }),
            )
            .expect("install plugin");
        assert_eq!(installed["approved"], false);
        assert!(!core
            .command_descriptors()
            .iter()
            .any(|command| command.id == "nulis.example.hello"));

        core.execute_command(
            "plugin.approve",
            CommandContext::default(),
            serde_json::json!({ "id": "nulis.example", "grant": { "permissions": ["showMessage"] } }),
        )
        .expect("approve plugin");
        let descriptor = core
            .command_descriptors()
            .into_iter()
            .find(|command| command.id == "nulis.example.hello")
            .expect("approved command");
        assert_eq!(descriptor.category, "Plugin · Examples");

        std::fs::write(
            application_data
                .path()
                .join("plugins/nulis.example/index.js"),
            "function activate() { return 1; }",
        )
        .expect("change installed source");
        let listed = core
            .execute_command("plugin.list", CommandContext::default(), Value::Null)
            .expect("list changed plugin");
        assert_eq!(listed[0]["approved"], false);
        assert!(!core
            .command_descriptors()
            .iter()
            .any(|command| command.id == "nulis.example.hello"));
        assert_eq!(
            core.execute_command(
                "plugin.uninstall",
                CommandContext::default(),
                serde_json::json!({ "id": "nulis.example" }),
            )
            .expect("uninstall plugin"),
            Value::Bool(true)
        );
    }

    #[cfg(unix)]
    #[test]
    fn customization_and_theme_lifecycle_persist_through_core_commands() {
        let directory = tempfile::tempdir().expect("tempdir");
        let core = NulisCore::with_session_root(directory.path().into());
        let themes = core
            .execute_command("theme.list", CommandContext::default(), Value::Null)
            .expect("list themes");
        assert_eq!(themes.as_array().expect("theme array").len(), 99);

        let mut customization = core
            .execute_command("customization.get", CommandContext::default(), Value::Null)
            .expect("get customization");
        let mut unsafe_customization = customization.clone();
        unsafe_customization["keybindings"][0]["command"] = serde_json::json!("terminal.close");
        assert!(core
            .execute_command(
                "customization.save",
                CommandContext::default(),
                unsafe_customization,
            )
            .is_err());
        customization["appearance"]["fontSize"] = serde_json::json!(17.0);
        core.execute_command(
            "customization.save",
            CommandContext::default(),
            customization,
        )
        .expect("save customization");
        drop(core);

        let reopened = NulisCore::with_session_root(directory.path().into());
        let loaded = reopened
            .execute_command("customization.get", CommandContext::default(), Value::Null)
            .expect("reload customization");
        assert_eq!(loaded["appearance"]["fontSize"], 17.0);

        // Two editors of the same revision: the second save is rejected, not merged.
        let state = reopened
            .execute_command(
                "customization.state",
                CommandContext::default(),
                Value::Null,
            )
            .expect("state");
        let mut first = state["document"].clone();
        first["appearance"]["fontSize"] = serde_json::json!(15.0);
        let saved = reopened
            .execute_command(
                "customization.saveChecked",
                CommandContext::default(),
                serde_json::json!({ "expectedRevision": state["revision"], "document": first }),
            )
            .expect("checked save");
        assert_eq!(
            saved["revision"],
            state["revision"].as_u64().expect("revision") + 1
        );
        let mut second = state["document"].clone();
        second["appearance"]["fontSize"] = serde_json::json!(21.0);
        assert!(reopened
            .execute_command(
                "customization.saveChecked",
                CommandContext::default(),
                serde_json::json!({ "expectedRevision": state["revision"], "document": second }),
            )
            .is_err());
        let current = reopened
            .execute_command("customization.get", CommandContext::default(), Value::Null)
            .expect("current");
        assert_eq!(current["appearance"]["fontSize"], 15.0);
    }

    #[cfg(unix)]
    #[test]
    fn terminal_lifecycle_is_exposed_through_capability_commands() {
        let directory = tempfile::tempdir().expect("tempdir");
        let core = NulisCore::with_session_root(directory.path().into());
        let created = core
            .execute_command(
                "terminal.create",
                CommandContext::default(),
                serde_json::json!({
                    "program": "/bin/sh",
                    "arguments": ["-c", "printf command-terminal"],
                    "environment": {},
                    "size": { "columns": 80, "rows": 24 }
                }),
            )
            .expect("create terminal");
        let terminal_id = created["terminalId"].as_str().expect("terminal ID");
        let output = core
            .execute_command(
                "terminal.read",
                CommandContext::default(),
                serde_json::json!({
                    "terminalId": terminal_id,
                    "cursor": "0",
                    "maximum": 1024
                }),
            )
            .expect("read terminal");
        assert_eq!(output["terminalId"], terminal_id);
        core.execute_command(
            "terminal.close",
            CommandContext::default(),
            serde_json::json!({ "terminalId": terminal_id }),
        )
        .expect("close terminal");
    }
}

#[cfg(test)]
mod settings_tests {
    use super::{CommandContext, NulisCore};
    use serde_json::{json, Value};

    fn language_of_new(core: &NulisCore, name: &str) -> String {
        core.execute_command(
            "file.new",
            CommandContext::default(),
            json!({ "name": name, "initialText": "" }),
        )
        .expect("create document")["languageId"]
            .as_str()
            .expect("language")
            .to_owned()
    }

    fn state(core: &NulisCore) -> Value {
        core.execute_command("settings.state", CommandContext::default(), Value::Null)
            .expect("settings state")
    }

    #[test]
    fn file_associations_in_settings_json_apply_at_start() {
        let root = tempfile::tempdir().expect("application data");
        std::fs::write(
            root.path().join("settings.json"),
            r#"{"schemaVersion":1,"values":{
                "files":{"associations":{"*.h":"cpp","*.foo":"no-such","a/b":"rust"}},
                "editor":{"tabSize":2},
                "[python]":{"editor":{"tabSize":8}}
            }}"#,
        )
        .expect("write settings");
        let core = NulisCore::with_session_root(root.path().into());
        assert_eq!(language_of_new(&core, "x.h"), "cpp");
        assert_eq!(language_of_new(&core, "x.c"), "c");
        let report = state(&core);
        assert_eq!(report["rejectedAssociations"], json!(["*.foo", "a/b"]));
        assert_eq!(report["loadError"], Value::Null);
        assert_eq!(core.effective_settings(None)["editor"]["tabSize"], 2);
        assert_eq!(
            core.effective_settings(Some("python"))["editor"]["tabSize"],
            8
        );
        // Defaults still fill what the file leaves out.
        assert_eq!(core.effective_settings(None)["editor"]["wordWrap"], false);
    }

    #[test]
    fn saving_settings_applies_them_and_a_stale_save_is_refused() {
        let root = tempfile::tempdir().expect("application data");
        let core = NulisCore::with_session_root(root.path().into());
        assert_eq!(language_of_new(&core, "x.h"), "c");
        let document = json!({ "schemaVersion": 1, "values": {
            "files": { "associations": { "*.h": "objective-c" } }
        }});
        let saved = core
            .execute_command(
                "settings.saveChecked",
                CommandContext::default(),
                json!({ "expectedRevision": 0, "document": document }),
            )
            .expect("save");
        assert_eq!(saved["revision"], 1);
        assert_eq!(language_of_new(&core, "x.h"), "objective-c");
        assert!(root.path().join("settings.json").is_file());

        let stale = core.execute_command(
            "settings.saveChecked",
            CommandContext::default(),
            json!({ "expectedRevision": 0, "document": { "schemaVersion": 1, "values": {} } }),
        );
        assert!(stale.is_err());
        assert_eq!(language_of_new(&core, "x.h"), "objective-c");

        // Removing the association restores the built-in mapping, and a new
        // core reads the saved file.
        core.execute_command(
            "settings.saveChecked",
            CommandContext::default(),
            json!({ "expectedRevision": 1, "document": { "schemaVersion": 1, "values": {} } }),
        )
        .expect("save");
        assert_eq!(language_of_new(&core, "x.h"), "c");
        let invalid = core.execute_command(
            "settings.saveChecked",
            CommandContext::default(),
            json!({ "expectedRevision": 2, "document": { "schemaVersion": 1, "values": {
                "files": { "associations": ["*.h"] }
            }}}),
        );
        assert!(invalid.is_err());
        assert_eq!(state(&core)["revision"], 2);
    }

    #[test]
    fn a_broken_settings_file_is_reported_and_does_not_stop_the_editor() {
        let root = tempfile::tempdir().expect("application data");
        std::fs::write(root.path().join("settings.json"), b"{ broken").expect("write");
        let core = NulisCore::with_session_root(root.path().into());
        let report = state(&core);
        assert!(report["loadError"]
            .as_str()
            .is_some_and(|error| error.contains("not valid JSON")));
        assert_eq!(language_of_new(&core, "x.h"), "c");
        assert_eq!(core.effective_settings(None)["editor"]["tabSize"], 4);
    }
}
