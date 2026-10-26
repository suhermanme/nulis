//! Headless application core and public command boundary.

mod arguments;
mod builtin_commands;
mod builtin_providers;
mod command;
mod count_jobs;
mod editor;
mod format_tools;
mod language_servers;
mod line_changes;
mod plist_tools;
mod plugin_languages;
mod print_jobs;
mod yaml_tools;
use arguments::{
    AcceptCompletionArguments, AddBookmarksArguments, ApplyCompareChangeArguments,
    ApprovePluginArguments, ApproveScriptArguments, BookmarkMoveArguments, ByteRangeArguments,
    ChangeNearArguments, CloseDocumentArguments, CompareDocumentsArguments,
    CompareFoldersArguments, CompletionArguments, ConvertLineEndingsArguments, EncodingArguments,
    FindAllArguments, FindArguments, FindInFilesArguments, FormatArguments, HexNibbleArguments,
    HexReplaceArguments, InsertTextArguments, JustifyArguments, LanguageArguments,
    LineOperationArguments, ListPlaceArguments, ListWorkspaceArguments, MacroIdArguments,
    MatchesInRangeArguments, MoveArguments, MoveSelectedTextArguments, NewDocumentArguments,
    NumberSequenceArguments, OpenFileArguments, OpenWorkspaceArguments, OpenWorkspaceFileArguments,
    PluginCapabilityArguments, PluginEnabledArguments, PluginIdArguments, PluginInstallArguments,
    PreviewReplaceInFilesArguments, RecoveryIdArguments, RectangularSelectionArguments,
    ReloadArguments, RenameFileArguments, ReplaceAllArguments, ReplaceArguments,
    ReplaceInFilesArguments, ReplaceMatchArguments, RevisionArguments, SaveAsArguments,
    SaveScriptArguments, ScriptEditDocumentArguments, ScriptIdArguments,
    ScriptReadWorkspaceArguments, ScriptWriteWorkspaceArguments, SelectionArguments,
    StartRecordingArguments, TerminalIdArguments, TerminalReadArguments, TerminalResizeArguments,
    TerminalWriteArguments, TransformArguments, ValidatePatternArguments, VerticalMoveArguments,
    WordAtArguments,
};
use builtin_commands::{
    register_automation_commands, register_compare_commands, register_editor_commands,
    register_plugin_commands, register_session_commands, register_terminal_commands,
    register_workbench_commands, register_workspace_commands, WORKBENCH_ACTIONS,
};

use editor::{ApplyCompareRequest, DocumentCompareRequest};

pub use command::{
    CommandContext, CommandDescriptor, CommandError, CommandRegistry, CommandResult,
};
pub use editor::{
    AssociationClaim, AutoIndent, BookmarkDirection, CapabilitySwitch, ChangeDirection,
    ClipboardPayload, CompletionList, DocumentCapabilities, DocumentId, DocumentManager,
    DocumentMapChange, DocumentMapSample, DocumentMapSnapshot, DocumentSummary, EditorError,
    ExportObserver, ExportPhase, ExportProgress, ExportScope, ExternalChange, ExternalChangeKind,
    FileSearchScope, FileStamp, FindAllResult, FindDirection, FindResult, FoundLine, FoundMatch,
    HexCaret, HexClipboardPayload, HexCursor, HexEditResult, HexViewportRequest, HexViewportRow,
    HexViewportSnapshot, HorizontalDirection, IndentSettings, IndentSettingsTable,
    IndentationSummary, Justify, LanguageCapabilities, LanguageConflict, LanguageDescriptor,
    LineEditResult, LineEnding, LineEndingConversion, LineEndingStyle, LineFormatOptions,
    LineOperation, MarkdownPreviewSource, NumberPadding, NumberSequence, NumberSum,
    PdfExportRequest, PdfExportResult, RecoveryCandidate, RectangularSelection, ReplaceAllResult,
    ReplaceFileOutcome, ReplacePreview, ReplacePreviewFile, ReplaceTarget, SearchMode,
    SearchOptions, Selection, SyntaxBracketPair, SyntaxFold, SyntaxHighlight, SyntaxSnapshot,
    SyntaxSymbol, TextEncoding, TextTransform, Unobserved, VerticalDirection, ViewportAtRequest,
    ViewportCursor, ViewportLine, ViewportLineRequest, ViewportRequest, ViewportSnapshot,
    WorkspaceSearchMatch, WorkspaceSearchResult, COMPLETION_WINDOW_BYTES, HIGHLIGHT_LOOKBACK_BYTES,
    HIGHLIGHT_WINDOW_BYTES, LARGE_EXPORT_PAGES, LINE_ENDING_SAMPLE_BYTES, MAX_BOOKMARKS,
    MAX_CLIPBOARD_BYTES, MAX_EDIT_BYTES, MAX_FILE_NAME_BYTES, MAX_FIND_ALL_LINES,
    MAX_FIND_ALL_MATCHES, MAX_HEX_CLIPBOARD_BYTES, MAX_HEX_EDIT_BYTES, MAX_HEX_VIEWPORT_BYTES,
    MAX_LINE_ENDING_CHANGES, MAX_LINE_ENDING_CONVERSION_BYTES, MAX_LINE_PREVIEW_BYTES,
    MAX_MARKDOWN_PREVIEW_BYTES, MAX_MERGE_BYTES, MAX_RECTANGULAR_ROWS, MAX_REPLACE_FILES,
    MAX_SELECTIONS, MAX_VIEWPORT_BYTES, MAX_WORKSPACE_SEARCH_FILES, MAX_WORKSPACE_SEARCH_RESULTS,
    PRINT_WINDOW_BYTES,
};
pub use line_changes::{LineChange, LineChangeStatus};
pub use nulis_automation::{
    AutomationError, MacroDefinition, MacroRunReport, MacroSummary, RecordedCommand,
    RecordingStatus, SandboxCapability, SandboxLimits, SandboxPolicy, ScriptDefinition,
    ScriptRunReport, ScriptSummary, StoredScript, AUTOMATION_SCHEMA_VERSION, SCRIPT_API_VERSION,
};
pub use nulis_diff::{ChangeKind, CompareMode, DiffChange, DiffPage, DiffRange};
use nulis_filesystem::{fingerprint, save_atomic, SessionStore, WorkspaceService};
pub use nulis_filesystem::{
    FolderCompareEntry, FolderCompareResult, FolderCompareStatus, FolderEntryKind,
    WorkbenchSession, WorkspaceDirectory, WorkspaceEntry, WorkspaceEntryKind, WorkspaceSummary,
    MAX_FOLDER_COMPARE_ENTRIES, MAX_FOLDER_COMPARE_RESULTS, MAX_WORKSPACE_DIRECTORY_ENTRIES,
    SESSION_SCHEMA_VERSION,
};
use nulis_plugin_host::{
    PluginCapability, PluginCatalog, PluginHostOperation, PluginPackage, ProcessPluginRunner,
};
pub use nulis_plugin_host::{
    PluginCommandContribution, PluginError, PluginGrant, PluginLanguageSummary, PluginPermission,
    PluginSummary, PLUGIN_API_VERSION,
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

pub const IPC_PROTOCOL_VERSION: u32 = 49;
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
        let disabled = self
            .document
            .language_overrides()
            .into_iter()
            .filter_map(|(language, values)| {
                let language = nulis_syntax::LanguageId::new(language).ok()?;
                let kinds = values.get("disabledCapabilities")?.as_array()?.clone();
                Some(kinds.into_iter().filter_map(move |kind| {
                    let kind: nulis_language::CapabilityKind = serde_json::from_value(kind).ok()?;
                    Some((language.clone(), kind))
                }))
            })
            .flatten()
            .collect::<Vec<_>>();
        documents.set_disabled_capabilities(disabled);
        let servers = documents.servers().configure(&self.layers.effective(None));
        documents.set_language_servers(servers);
        documents.set_indent_settings(IndentSettingsTable {
            default: IndentSettings::from_settings(&self.layers.effective(None)),
            languages: self
                .document
                .language_overrides()
                .into_keys()
                .map(|language| {
                    let settings =
                        IndentSettings::from_settings(&self.layers.effective(Some(&language)));
                    (language, settings)
                })
                .collect(),
        });
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
    /// Why a plugin's declarative language could not be applied, as of the
    /// last rebuild.
    plugin_language_errors: Mutex<Vec<String>>,
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
            .field("plugin_language_errors", &self.plugin_language_errors)
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
            ("theme.save", "Save Theme"),
            ("theme.readable", "Readable Theme Colors"),
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

        let core = Self {
            commands,
            settings: Mutex::new(settings),
            documents,
            workspace,
            session_store,
            terminals,
            automation,
            plugins,
            plugin_language_errors: Mutex::new(Vec::new()),
            customization,
        };
        core.refresh_plugin_languages();
        core
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
                for plugin in summaries.into_iter().filter(|plugin| {
                    plugin.approved
                        && plugin.enabled
                        && !plugin
                            .disabled_capabilities
                            .contains(&PluginCapability::Commands)
                }) {
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
            "theme.save" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Request {
                    theme: Theme,
                }
                let request: Request = serde_json::from_value(arguments).map_err(|error| {
                    CommandError::Failed(format!("invalid theme save request: {error}"))
                })?;
                let record = state
                    .themes
                    .save(&request.theme)
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                serde_json::to_value(record)
                    .map_err(|error| CommandError::Failed(error.to_string()))
            }
            // Answers what the theme would show, without storing or reading anything: the
            // panel asks this of a draft so it can say which colors Nulis will strengthen.
            "theme.readable" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Request {
                    theme: Theme,
                }
                let request: Request = serde_json::from_value(arguments).map_err(|error| {
                    CommandError::Failed(format!("invalid theme readability request: {error}"))
                })?;
                let authored = request.theme.colors.clone();
                let mut theme = request.theme;
                theme
                    .validate()
                    .map_err(|error| CommandError::Failed(error.to_string()))?;
                let changes = theme.ensure_readable();
                Ok(json!({
                    "colors": theme.colors,
                    "changes": changes
                        .iter()
                        .map(|(token, to)| json!({
                            "token": token,
                            "from": authored.get(token).cloned().unwrap_or_default(),
                            "to": to,
                        }))
                        .collect::<Vec<_>>(),
                }))
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
                let summary = {
                    let plugins = self.plugin_state()?;
                    plugins
                        .catalog
                        .install(std::path::Path::new(&arguments.path))
                        .map_err(plugin_command_error)?
                };
                self.refresh_plugin_languages();
                serde_json::to_value(summary).map_err(|error| serialization_command_error(&error))
            }
            "plugin.uninstall" => {
                let arguments: PluginIdArguments = parse_arguments(arguments)?;
                let removed = {
                    let plugins = self.plugin_state()?;
                    plugins
                        .catalog
                        .uninstall(&arguments.id)
                        .map_err(plugin_command_error)?
                };
                self.refresh_plugin_languages();
                Ok(Value::Bool(removed))
            }
            "plugin.approve" => {
                let arguments: ApprovePluginArguments = parse_arguments(arguments)?;
                {
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
                }
                self.refresh_plugin_languages();
                Ok(Value::Null)
            }
            "plugin.revoke" => {
                let arguments: PluginIdArguments = parse_arguments(arguments)?;
                let revoked = {
                    let plugins = self.plugin_state()?;
                    plugins
                        .catalog
                        .revoke(&arguments.id)
                        .map_err(plugin_command_error)?
                };
                self.refresh_plugin_languages();
                Ok(Value::Bool(revoked))
            }
            "plugin.setEnabled" => {
                let arguments: PluginEnabledArguments = parse_arguments(arguments)?;
                let changed = {
                    let plugins = self.plugin_state()?;
                    plugins
                        .catalog
                        .set_enabled(&arguments.id, arguments.enabled)
                        .map_err(plugin_command_error)?
                };
                self.refresh_plugin_languages();
                Ok(Value::Bool(changed))
            }
            "plugin.setCapabilityEnabled" => {
                let arguments: PluginCapabilityArguments = parse_arguments(arguments)?;
                let changed = {
                    let plugins = self.plugin_state()?;
                    plugins
                        .catalog
                        .set_capability_enabled(
                            &arguments.id,
                            arguments.capability,
                            arguments.enabled,
                        )
                        .map_err(plugin_command_error)?
                };
                self.refresh_plugin_languages();
                Ok(Value::Bool(changed))
            }
            "plugin.languageErrors" => {
                let errors = self
                    .plugin_language_errors
                    .lock()
                    .map_err(|_| CommandError::Unavailable("plugin language errors".into()))?
                    .clone();
                serde_json::to_value(errors).map_err(|error| serialization_command_error(&error))
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
                        let active = self
                            .plugins
                            .lock()
                            .ok()
                            .and_then(|plugins| plugins.catalog.activation_for(&package).ok())
                            .flatten()
                            .is_some_and(|activation| {
                                activation.capability_enabled(PluginCapability::Commands)
                            });
                        active.then(|| self.run_plugin(package, None, Vec::new()))
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
                    .activation_for(&package)
                    .map_err(plugin_command_error)?
                    .is_some_and(|activation| {
                        activation.capability_enabled(PluginCapability::Commands)
                    })
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
            let activation = plugins
                .catalog
                .activation_for(&package)
                .map_err(plugin_command_error)?
                .ok_or_else(|| CommandError::Failed("plugin requires explicit approval".into()))?;
            if !activation.capability_enabled(PluginCapability::Commands) {
                return Err(CommandError::Failed("plugin commands are disabled".into()));
            }
            let grant = activation.grant;
            let cancellation = ScriptCancellationToken::new();
            plugins.active = Some(cancellation.clone());
            (grant, plugins.runner.clone(), cancellation)
        };
        let request = package
            .worker_request(command.clone(), arguments)
            .map_err(plugin_command_error)?;
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

    /// Rebuilds declarative plugin languages, associations, and providers
    /// from the current installation and approval state. Called after every
    /// action that changes which plugins or capabilities are active.
    fn refresh_plugin_languages(&self) {
        let sync = {
            let Ok(plugins) = self.plugins.lock() else {
                return;
            };
            let Ok(packages) = plugins.catalog.packages() else {
                return;
            };
            crate::plugin_languages::build(&packages, |package| {
                plugins.catalog.activation_for(package).ok().flatten()
            })
        };
        let errors = self.documents.apply_plugin_languages(sync);
        if let Ok(mut stored) = self.plugin_language_errors.lock() {
            *stored = errors;
        }
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

fn is_recordable_command(id: &str) -> bool {
    matches!(
        id,
        "editor.deleteBackward"
            | "completion.accept"
            | "editor.newline"
            | "editor.tab"
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
mod tests;

#[cfg(test)]
mod settings_tests;
