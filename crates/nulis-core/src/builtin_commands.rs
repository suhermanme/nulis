//! The built-in commands: each one's ID, name and menu category, bound to the
//! core service that runs it. Palettes, menus, shortcuts, the toolbar and
//! macros all reach the core through these.

use super::{
    context_document, count_jobs, editor_command_error, parse_arguments, print_jobs,
    serialization_command_error, terminal_command_error, AcceptCompletionArguments,
    AddBookmarksArguments, ApplyCompareChangeArguments, ApplyCompareRequest, BookmarkMoveArguments,
    ByteRangeArguments, ChangeNearArguments, CloseDocumentArguments, CommandDescriptor,
    CommandError, CommandRegistry, CompareDocumentsArguments, CompareFoldersArguments,
    CompletionArguments, ConvertLineEndingsArguments, DocumentCompareRequest, DocumentId,
    DocumentManager, DocumentSummary, EditorError, EncodingArguments, FindAllArguments,
    FindArguments, FindInFilesArguments, FormatArguments, HexNibbleArguments, HexReplaceArguments,
    InsertTextArguments, JustifyArguments, LanguageArguments, LineOperationArguments,
    ListPlaceArguments, ListWorkspaceArguments, MatchesInRangeArguments, MoveArguments,
    MoveSelectedTextArguments, NewDocumentArguments, NumberSequenceArguments, OpenFileArguments,
    OpenWorkspaceArguments, OpenWorkspaceFileArguments, PreviewReplaceInFilesArguments,
    RecoveryIdArguments, RectangularSelectionArguments, ReloadArguments, RenameFileArguments,
    ReplaceAllArguments, ReplaceArguments, ReplaceInFilesArguments, ReplaceMatchArguments,
    RevisionArguments, SaveAsArguments, SelectionArguments, TerminalIdArguments,
    TerminalReadArguments, TerminalResizeArguments, TerminalWriteArguments, TransformArguments,
    ValidatePatternArguments, VerticalMoveArguments, WordAtArguments,
};
use nulis_filesystem::{compare_folders, SessionStore, WorkbenchSession, WorkspaceService};
use nulis_terminal::{TerminalManager, TerminalSpawnRequest};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub(super) fn register_automation_commands(commands: &mut CommandRegistry) {
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

pub(super) fn register_plugin_commands(commands: &mut CommandRegistry) {
    for (id, title) in [
        ("plugin.list", "List Plugins"),
        ("plugin.install", "Install Plugin"),
        ("plugin.uninstall", "Uninstall Plugin"),
        ("plugin.approve", "Approve Plugin"),
        ("plugin.revoke", "Revoke Plugin Approval"),
        ("plugin.activateStartup", "Activate Startup Plugins"),
        ("plugin.cancel", "Cancel Plugin"),
        ("plugin.setEnabled", "Enable or Disable Plugin"),
        (
            "plugin.setCapabilityEnabled",
            "Enable or Disable Plugin Capability",
        ),
        ("plugin.languageErrors", "Plugin Language Errors"),
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

pub(super) fn register_workspace_commands(
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

pub(super) fn register_terminal_commands(
    commands: &mut CommandRegistry,
    terminals: &Arc<TerminalManager>,
) {
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

pub(super) fn register_session_commands(commands: &mut CommandRegistry, store: &Arc<SessionStore>) {
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
pub(super) const WORKBENCH_ACTIONS: &[(&str, &str, &str)] = &[
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
    ("edit.triggerSuggest", "Show Suggestions", "Edit"),
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

pub(super) fn register_workbench_commands(commands: &mut CommandRegistry) {
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

pub(super) fn register_compare_commands(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
) {
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
                    let document_id = context_document(&context)?;
                    // Beautify without a built-in formatter asks the language server.
                    if !minify && !manager.has_builtin_formatter(&document_id, false) {
                        let options = manager
                            .line_format_defaults(&document_id)
                            .map_err(|error| editor_command_error(&error))?;
                        if let Some(summary) = manager
                            .servers()
                            .format(
                                &manager,
                                &document_id,
                                options.tab_width,
                                options.indent_with_spaces,
                            )
                            .map_err(CommandError::Failed)?
                        {
                            return serde_json::to_value(summary)
                                .map_err(|error| serialization_command_error(&error));
                        }
                    }
                    let result = manager
                        .format_document(
                            &document_id,
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
            CommandDescriptor::new("editor.summaries", "List Open Documents", "Editor")
                .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                let summaries = manager
                    .summaries()
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(summaries).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
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

pub(super) fn register_editor_commands(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
) {
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
            CommandDescriptor::new(
                "language.capabilities",
                "List Language Capabilities",
                "Language",
            )
            .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                serde_json::to_value(manager.language_capabilities())
                    .map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("language.conflicts", "List Language Conflicts", "Language")
                .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                serde_json::to_value(manager.language_conflicts())
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
                    .and_then(|_| manager.summary(&document_id))
                    .map_err(|error| editor_command_error(&error))
                    .and_then(|summary| {
                        serde_json::to_value(summary)
                            .map_err(|error| serialization_command_error(&error))
                    })
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
                let options = match arguments.options {
                    Some(options) => options,
                    None => manager
                        .line_format_defaults(&document_id)
                        .map_err(|error| editor_command_error(&error))?,
                };
                let result = manager
                    .line_operation_with(
                        &document_id,
                        arguments.expected_revision,
                        arguments.operation,
                        options,
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

/// Settings' view of language servers, approving and restarting them, and
/// the problems a server reports for a document.
fn register_language_server_commands(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
) {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ServerArguments {
        id: String,
    }
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("lsp.state", "List Language Servers", "Language")
                .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                let (servers, config_error) = manager.servers().status();
                Ok(json!({ "servers": servers, "configError": config_error }))
            },
        )
        .expect("built-in command is unique");
    for (id, title) in [
        ("lsp.approve", "Approve Language Server"),
        ("lsp.revoke", "Revoke Language Server"),
        ("lsp.restart", "Restart Language Server"),
    ] {
        let manager = Arc::clone(documents);
        commands
            .register(
                CommandDescriptor::new(id, title, "Language")
                    .expect("built-in command ID is valid"),
                move |_context, arguments| {
                    let arguments: ServerArguments = parse_arguments(arguments)?;
                    let servers = manager.servers();
                    if id == "lsp.restart" {
                        servers.restart(&arguments.id);
                    } else {
                        let map = servers
                            .set_approved(&arguments.id, id == "lsp.approve")
                            .map_err(CommandError::Failed)?;
                        manager.set_language_servers(map);
                    }
                    let (servers, config_error) = servers.status();
                    Ok(json!({ "servers": servers, "configError": config_error }))
                },
            )
            .expect("built-in command is unique");
    }
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("lsp.diagnostics", "Language Server Problems", "Language")
                .expect("built-in command ID is valid"),
            move |context, _arguments| {
                let document_id = context_document(&context)?;
                let problems = manager
                    .servers()
                    .problems(&manager, &document_id)
                    .map_err(CommandError::Failed)?;
                serde_json::to_value(problems).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
}

/// Suggestions for the word at the caret, and accepting one.
fn register_completion_commands(commands: &mut CommandRegistry, documents: &Arc<DocumentManager>) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("completion.request", "Suggest Completions", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: CompletionArguments = parse_arguments(arguments)?;
                let mut list = manager
                    .completions(
                        &document_id,
                        arguments.expected_revision,
                        arguments.request_id,
                        arguments.explicit,
                        arguments.min_prefix,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                // A language server's suggestions join the built-in ones;
                // a failing or late server just adds none.
                let asked = arguments.explicit
                    || list.prefix.chars().count() >= arguments.min_prefix.max(1);
                if asked {
                    if let Ok(items) = manager.servers().completions(&manager, &document_id) {
                        if !items.is_empty() {
                            let builtin = std::mem::take(&mut list.items);
                            list.items = nulis_language::completion::rank(
                                &list.prefix,
                                vec![items, builtin],
                            );
                        }
                    }
                }
                serde_json::to_value(list).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("completion.accept", "Accept Completion", "Editor")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: AcceptCompletionArguments = parse_arguments(arguments)?;
                let summary = manager
                    .accept_completion(
                        &document_id,
                        arguments.expected_revision,
                        arguments.replace_start,
                        arguments.replace_end,
                        &arguments.text,
                        arguments.snippet,
                    )
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(summary).map_err(|error| serialization_command_error(&error))
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
                let insert = if arguments.typed {
                    DocumentManager::insert_typed_text
                } else {
                    DocumentManager::insert_text
                };
                let result = insert(
                    &manager,
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

    register_completion_commands(commands, documents);
    register_language_server_commands(commands, documents);

    for (id, title, tab) in [
        ("editor.newline", "Insert Line Break", false),
        ("editor.tab", "Insert Tab", true),
    ] {
        let manager = Arc::clone(documents);
        commands
            .register(
                CommandDescriptor::new(id, title, "Editor").expect("built-in command ID is valid"),
                move |context, arguments| {
                    let document_id = context_document(&context)?;
                    let arguments: RevisionArguments = parse_arguments(arguments)?;
                    let result = if tab {
                        manager.insert_tab(&document_id, arguments.expected_revision)
                    } else {
                        manager.newline(&document_id, arguments.expected_revision)
                    }
                    .map_err(|error| editor_command_error(&error))?;
                    serde_json::to_value(result)
                        .map_err(|error| serialization_command_error(&error))
                },
            )
            .expect("built-in command is unique");
    }

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
