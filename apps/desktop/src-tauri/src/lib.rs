use nulis_core::{
    ApplicationInfo, CommandContext, CommandDescriptor, CommandError, DocumentId,
    DocumentMapSnapshot, EditorError, HexViewportRequest, HexViewportSnapshot,
    MarkdownPreviewSource, NulisCore, ViewportAtRequest, ViewportLineRequest, ViewportRequest,
    ViewportSnapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

#[cfg(desktop)]
use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem, Submenu, SubmenuBuilder};

const MAX_COMMAND_ID_BYTES: usize = 256;
const MAX_COMMAND_ARGUMENT_BYTES: usize = 1024 * 1024;
const NATIVE_MENU_EVENT: &str = "nulis://menu-command";
const FILE_NEW_MENU_ID: &str = "file.new";
const FILE_OPEN_MENU_ID: &str = "file.open";
const FILE_SAVE_MENU_ID: &str = "file.save";
const FILE_SAVE_AS_MENU_ID: &str = "file.saveAs";
const CLOSE_FILE_MENU_ID: &str = "workbench.closeFile";
const RELOAD_MENU_ID: &str = "file.reload";
/// File commands always offered after the customizable part of the File menu.
const FILE_EXTRA_MENU_COMMANDS: &[(&str, &str)] = &[
    ("file.quickOpen", "Quick Open…"),
    ("file.openRecent", "Open Recent…"),
    ("file.saveAll", "Save All"),
    ("file.rename", "Rename File…"),
    ("file.revealInPlaces", "Show in Places"),
];
const CLOSE_WINDOW_MENU_ID: &str = "window.close";
const QUIT_MENU_ID: &str = "app.quit";
const MAX_NATIVE_MENU_ENTRIES: usize = 64;
const MAIN_WINDOW_LABEL: &str = "main";
const SETTINGS_WINDOW_LABEL: &str = "settings";
const HELP_WINDOW_LABEL: &str = "help";
const FIND_WINDOW_LABEL: &str = "find";
const RESULTS_WINDOW_LABEL: &str = "find-results";
const PRINT_WINDOW_LABEL: &str = "print";
/// Find and Replace searches and replaces (Replace in Files saves the files it
/// changes, after a preview); it cannot open, show, or type into documents.
const FIND_COMMANDS: &[&str] = &[
    "customization.state",
    "theme.list",
    "search.findAll",
    "search.validatePattern",
    "search.countStart",
    "search.countProgress",
    "search.countCancel",
    "search.replaceAll",
    "search.findInFiles",
    "search.previewReplaceInFiles",
    "search.replaceInFiles",
];
/// The list of matching lines can only bookmark lines.
const RESULTS_COMMANDS: &[&str] = &["customization.state", "theme.list", "editor.addBookmarks"];
/// Messages from the Find and results windows to the editor window.
const FIND_MESSAGE_EVENT: &str = "nulis://find-message";
/// Asks an open Help window to show a topic.
const HELP_TOPIC_EVENT: &str = "nulis://help-topic";
const FIND_MESSAGE_KINDS: &[&str] = &[
    "ready",
    "search",
    "openMatch",
    "documentChange",
    "status",
    "results",
    "resultsReady",
];
/// A list of matching lines is at most 5,000 lines of 512 bytes, plus names.
const MAX_FIND_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const FIND_MIN_SIZE: (u32, u32) = (420, 360);
const RESULTS_MIN_SIZE: (u32, u32) = (420, 220);
/// Files the operating system asks Nulis to open while it is running.
const OPEN_PATHS_EVENT: &str = "nulis://open-paths";
/// Help only reads what it needs to follow the chosen theme and font.
const HELP_COMMANDS: &[&str] = &["customization.state", "theme.list"];
const HELP_MIN_SIZE: (u32, u32) = (640, 420);
/// Print Preview reads the theme and the pages the editor window prepared
/// for it (`print.pages`). It cannot choose a document or run editor or
/// file commands.
const PRINT_COMMANDS: &[&str] = &["customization.state", "theme.list", "print.pages"];
/// Tells an open Print Preview window to show new content.
const PRINT_CONTENT_EVENT: &str = "nulis://print-content";
/// Longest document name accepted as a print title, in bytes.
const MAX_PRINT_TITLE_BYTES: usize = 512;
const PRINT_MIN_SIZE: (u32, u32) = (520, 420);
const OPEN_SETTINGS_MENU_ID: &str = "workbench.openSettings";
const ABOUT_MENU_ID: &str = "app.about";
const HELP_MENU_ID: &str = "workbench.openHelp";
const CUSTOMIZATION_CHANGED_EVENT: &str = "nulis://customization-changed";
const APPEARANCE_PREVIEW_EVENT: &str = "nulis://appearance-preview";
const MAX_APPEARANCE_PREVIEW_BYTES: usize = 4096;
const MAX_WINDOW_FRAME_BYTES: u64 = 1024;
const SETTINGS_MIN_SIZE: (u32, u32) = (640, 420);
/// A restored window must keep at least this much of itself on one display.
const MIN_VISIBLE_FRAME: (i64, i64) = (160, 120);

/// Everything the Settings webview may ask the core to do. It receives no
/// document, workspace, file system, terminal, or automation authority.
const SETTINGS_COMMANDS: &[&str] = &[
    "settings.state",
    "settings.saveChecked",
    "language.capabilities",
    "lsp.state",
    "lsp.approve",
    "lsp.revoke",
    "lsp.restart",
    "customization.state",
    "customization.saveChecked",
    "customization.reset",
    "theme.list",
    "theme.install",
    "theme.uninstall",
    "theme.save",
    // Read-only: answers what a draft palette would show, so it mutates nothing.
    "theme.readable",
    "language.list",
    "language.conflicts",
    "plugin.list",
];
const CUSTOMIZATION_MUTATIONS: &[&str] = &[
    "settings.saveChecked",
    "customization.save",
    "customization.saveChecked",
    "customization.reset",
    "theme.install",
    "theme.uninstall",
    "theme.save",
    // Approving, revoking, or restarting a server changes open documents' capabilities.
    "lsp.approve",
    "lsp.revoke",
    "lsp.restart",
];
/// Presentation actions Settings may hand to the editor window.
const SETTINGS_MAIN_ACTIONS: &[&str] = &["workspace.rename", "workbench.openPlugins"];
const VIEW_MENU_COMMANDS: &[(&str, &str)] = &[
    (
        "view.toggleLineChangeIndicators",
        "Show/Hide Line Change Indicators",
    ),
    ("view.toggleWordWrap", "Toggle Word Wrap"),
    ("view.toggleWhitespace", "Show/Hide Whitespace"),
    ("view.toggleLineEndings", "Show/Hide Line Endings"),
    ("view.toggleLineNumbers", "Show/Hide Line Numbers"),
    ("view.toggleColumnMarker", "Show/Hide Column Marker"),
    ("view.zoomIn", "Zoom In"),
    ("view.zoomOut", "Zoom Out"),
    ("view.zoomReset", "Reset Zoom"),
    ("view.toggleAlwaysOnTop", "Toggle Always on Top"),
    ("workbench.toggleSidebar", "Toggle Primary Side Bar"),
    ("editor.toggleReadOnly", "Toggle Read-Only Mode"),
    ("workbench.splitRight", "Split Editor Right"),
    ("workbench.splitDown", "Split Editor Down"),
];
/// Line commands added to the platform Edit menu.
const EDIT_MENU_COMMANDS: &[(&str, &str)] = &[
    ("edit.selectLines", "Select Line"),
    ("edit.duplicateLines", "Duplicate Line"),
    ("edit.deleteLines", "Delete Line"),
    ("edit.moveLinesUp", "Move Line Up"),
    ("edit.moveLinesDown", "Move Line Down"),
    ("edit.insertLineBelow", "Insert Line Below"),
    ("edit.copyAppend", "Copy and Append"),
    ("edit.cutAppend", "Cut and Append"),
    ("edit.pasteCopy", "Paste and Copy"),
    ("edit.pasteFromHistory", "Paste from History…"),
];
const SEARCH_MENU_COMMANDS: &[&[(&str, &str)]] = &[
    &[
        ("search.openFind", "Find…"),
        ("search.openReplace", "Replace…"),
        ("search.findNext", "Find Next"),
        ("search.findPrevious", "Find Previous"),
    ],
    &[
        ("search.openFindInFiles", "Find in Files…"),
        ("search.openReplaceInFiles", "Replace in Files…"),
    ],
    &[
        ("search.goToLine", "Go to Line…"),
        ("search.matchBrace", "Go to Matching Bracket"),
        ("search.navigateBack", "Go Back"),
        ("search.navigateForward", "Go Forward"),
        ("search.nextChange", "Next Change"),
        ("search.previousChange", "Previous Change"),
    ],
    &[
        ("search.toggleBookmark", "Toggle Bookmark"),
        ("search.nextBookmark", "Next Bookmark"),
        ("search.previousBookmark", "Previous Bookmark"),
        ("search.listBookmarks", "List Bookmarks…"),
        ("search.clearBookmarks", "Clear All Bookmarks"),
        ("search.deleteBookmarkedLines", "Delete Bookmarked Lines"),
    ],
];
const FORMAT_MENU_COMMANDS: &[&[(&str, &str)]] = &[
    &[
        ("format.uppercase", "To Upper Case"),
        ("format.lowercase", "To Lower Case"),
        ("format.capitalize", "Capitalize Words"),
        ("format.invertCase", "Invert Case"),
    ],
    &[
        ("format.indent", "Indent Lines"),
        ("format.outdent", "Outdent Lines"),
        ("format.toggleComment", "Toggle Line Comment"),
    ],
    &[
        ("format.trimSelection", "Trim Selection"),
        ("format.trimTrailingSpaces", "Trim Trailing Spaces"),
        ("format.tabsToSpaces", "Tabs to Spaces"),
        ("format.spacesToTabs", "Leading Spaces to Tabs"),
    ],
    &[
        ("format.sortAscending", "Sort Lines Ascending"),
        ("format.sortDescending", "Sort Lines Descending"),
        ("format.sortUnique", "Sort Lines and Remove Duplicates"),
    ],
    &[
        ("format.base64Encode", "Encode Base64"),
        ("format.base64Decode", "Decode Base64"),
        ("format.insertDateTime", "Insert Date and Time"),
    ],
    &[
        ("format.insertNumbers", "Insert Number Sequence…"),
        ("format.sumNumbers", "Sum Selected Numbers"),
        ("format.justifyLeft", "Left Justify Columns"),
        ("format.justifyCenter", "Center Columns"),
        ("format.justifyRight", "Right Justify Columns"),
        ("format.alignColumns", "Convert to Fixed-Width Columns"),
        ("format.unalignColumns", "Convert to Delimited Columns"),
    ],
    &[
        ("format.lineEndingsLf", "Convert Line Endings to LF"),
        ("format.lineEndingsCrlf", "Convert Line Endings to CRLF"),
        ("format.lineEndingsCr", "Convert Line Endings to CR"),
    ],
];
const WORKSPACE_MENU_COMMANDS: &[&[(&str, &str)]] = &[
    &[
        ("workspace.rename", "Rename Workspace…"),
        ("workspace.groupRight", "Group with Next Tab Side by Side"),
        ("workspace.groupDown", "Group with Next Tab Stacked"),
        ("workspace.ungroup", "Ungroup Workspace"),
    ],
    &[
        ("workspace.popOutPane", "Move Pane to New Tab"),
        ("workspace.closePane", "Close Pane"),
        ("workspace.focusNextPane", "Focus Next Pane"),
    ],
    &[
        ("workbench.nextTab", "Next Tab"),
        ("workbench.previousTab", "Previous Tab"),
        ("workbench.moveTabLeft", "Move Tab Left"),
        ("workbench.moveTabRight", "Move Tab Right"),
    ],
];

/// Outer position and inner size of a window in physical pixels.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct WindowFrame {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Default)]
struct SettingsFrame(Mutex<Option<WindowFrame>>);

/// Tools menu items: command ID and title.
const TOOLS_MENU_COMMANDS: &[(&str, &str)] = &[
    ("tools.validate", "Validate"),
    ("tools.beautify", "Beautify"),
    ("tools.minify", "Minify"),
];
const MAX_TOOL_LABEL_CHARS: usize = 64;

/// What the focused document offers in the Tools menu: the label of each
/// tool it has ("JSON"), in `TOOLS_MENU_COMMANDS` order.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ToolsMenuState {
    validate: Option<String>,
    beautify: Option<String>,
    minify: Option<String>,
}

impl ToolsMenuState {
    fn labels(&self) -> [Option<&str>; 3] {
        [
            self.validate.as_deref(),
            self.beautify.as_deref(),
            self.minify.as_deref(),
        ]
    }
}

/// The Tools menu items and what the focused document offers.
#[cfg(desktop)]
#[derive(Default)]
struct ToolsMenu(Mutex<(Vec<tauri::menu::MenuItem<tauri::Wry>>, ToolsMenuState)>);

/// Names and enables each Tools item: "Validate JSON" when the document has
/// it, a plain disabled "Validate" when not.
#[cfg(desktop)]
fn apply_tools_menu(items: &[tauri::menu::MenuItem<tauri::Wry>], tools: &ToolsMenuState) {
    for ((item, (_, title)), label) in items.iter().zip(TOOLS_MENU_COMMANDS).zip(tools.labels()) {
        let _ = item.set_text(
            label.map_or_else(|| (*title).to_owned(), |label| format!("{title} {label}")),
        );
        let _ = item.set_enabled(label.is_some());
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExecuteCommandRequest {
    id: String,
    #[serde(default)]
    context: CommandContext,
    #[serde(default)]
    arguments: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum NativeMenuEntry {
    Command {
        command: String,
        title: String,
        accelerator: Option<String>,
    },
    Separator,
}

fn default_native_menu() -> Vec<NativeMenuEntry> {
    vec![
        NativeMenuEntry::Command {
            command: FILE_NEW_MENU_ID.into(),
            title: "New File".into(),
            accelerator: Some("Primary+N".into()),
        },
        NativeMenuEntry::Command {
            command: FILE_OPEN_MENU_ID.into(),
            title: "Open…".into(),
            accelerator: Some("Primary+O".into()),
        },
        NativeMenuEntry::Separator,
        NativeMenuEntry::Command {
            command: FILE_SAVE_MENU_ID.into(),
            title: "Save".into(),
            accelerator: Some("Primary+S".into()),
        },
        NativeMenuEntry::Command {
            command: FILE_SAVE_AS_MENU_ID.into(),
            title: "Save As…".into(),
            accelerator: Some("Primary+Shift+S".into()),
        },
    ]
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn application_info(core: State<'_, NulisCore>) -> ApplicationInfo {
    core.application_info()
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State and WebviewWindow by value.
fn close_window_after_save(
    core: State<'_, NulisCore>,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if core
        .has_dirty_documents()
        .map_err(|error| error.to_string())?
    {
        return Err("Unsaved documents remain".into());
    }
    window.destroy().map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn list_commands(core: State<'_, NulisCore>) -> Vec<CommandDescriptor> {
    core.command_descriptors()
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
#[allow(clippy::unused_async)] // Async commands keep bounded script runs off the UI event loop.
async fn execute_command(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    core: State<'_, NulisCore>,
    request: ExecuteCommandRequest,
) -> Result<Value, CommandError> {
    validate_request(&request)?;
    authorize_window_command(window.label(), &request.id)?;
    let result = core.execute_command(&request.id, request.context, request.arguments)?;
    if CUSTOMIZATION_MUTATIONS.contains(&request.id.as_str()) {
        // Every other window refreshes its settings, toolbar, and native menu.
        for label in [
            MAIN_WINDOW_LABEL,
            SETTINGS_WINDOW_LABEL,
            HELP_WINDOW_LABEL,
            FIND_WINDOW_LABEL,
            RESULTS_WINDOW_LABEL,
            PRINT_WINDOW_LABEL,
        ] {
            if label != window.label() {
                let _ = app.emit_to(label, CUSTOMIZATION_CHANGED_EVENT, ());
            }
        }
    }
    Ok(result)
}

/// The editor window has the full command boundary; Settings and Help have narrow allowlists.
fn authorize_window_command(label: &str, id: &str) -> Result<(), CommandError> {
    if label == MAIN_WINDOW_LABEL
        || (label == SETTINGS_WINDOW_LABEL && SETTINGS_COMMANDS.contains(&id))
        || (label == HELP_WINDOW_LABEL && HELP_COMMANDS.contains(&id))
        || (label == FIND_WINDOW_LABEL && FIND_COMMANDS.contains(&id))
        || (label == RESULTS_WINDOW_LABEL && RESULTS_COMMANDS.contains(&id))
        || (label == PRINT_WINDOW_LABEL && PRINT_COMMANDS.contains(&id))
    {
        return Ok(());
    }
    Err(CommandError::Failed(format!(
        "command {id} is not available to the {label} window"
    )))
}

/// Document bytes cross IPC only to the editor window.
fn require_editor_window(window: &tauri::WebviewWindow) -> Result<(), EditorError> {
    if window.label() == MAIN_WINDOW_LABEL {
        Ok(())
    } else {
        Err(EditorError::StateUnavailable)
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State and WebviewWindow by value.
fn editor_viewport(
    core: State<'_, NulisCore>,
    window: tauri::WebviewWindow,
    request: ViewportRequest,
) -> Result<ViewportSnapshot, EditorError> {
    require_editor_window(&window)?;
    core.viewport(&request)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State and WebviewWindow by value.
fn editor_hex_viewport(
    core: State<'_, NulisCore>,
    window: tauri::WebviewWindow,
    request: HexViewportRequest,
) -> Result<HexViewportSnapshot, EditorError> {
    require_editor_window(&window)?;
    core.hex_viewport(&request)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State and WebviewWindow by value.
fn editor_viewport_at(
    core: State<'_, NulisCore>,
    window: tauri::WebviewWindow,
    request: ViewportAtRequest,
) -> Result<ViewportSnapshot, EditorError> {
    require_editor_window(&window)?;
    core.viewport_at(&request)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State and WebviewWindow by value.
fn editor_document_map(
    core: State<'_, NulisCore>,
    window: tauri::WebviewWindow,
    document_id: DocumentId,
) -> Result<DocumentMapSnapshot, EditorError> {
    require_editor_window(&window)?;
    core.document_map(&document_id)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State and WebviewWindow by value.
fn editor_markdown_preview_source(
    core: State<'_, NulisCore>,
    window: tauri::WebviewWindow,
    document_id: DocumentId,
) -> Result<MarkdownPreviewSource, EditorError> {
    require_editor_window(&window)?;
    core.markdown_preview_source(&document_id)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State and WebviewWindow by value.
fn editor_markdown_preview_image(
    core: State<'_, NulisCore>,
    window: tauri::WebviewWindow,
    document_id: DocumentId,
    relative_path: String,
) -> Result<String, EditorError> {
    require_editor_window(&window)?;
    core.markdown_preview_image(&document_id, &relative_path)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State and WebviewWindow by value.
fn editor_viewport_line(
    core: State<'_, NulisCore>,
    window: tauri::WebviewWindow,
    request: ViewportLineRequest,
) -> Result<ViewportSnapshot, EditorError> {
    require_editor_window(&window)?;
    core.viewport_line(&request)
}

fn validate_request(request: &ExecuteCommandRequest) -> Result<(), CommandError> {
    if request.id.len() > MAX_COMMAND_ID_BYTES {
        return Err(CommandError::InvalidId(request.id.clone()));
    }
    let argument_size = serde_json::to_vec(&request.arguments)
        .map_err(|error| CommandError::Failed(format!("invalid command arguments: {error}")))?
        .len();
    if argument_size > MAX_COMMAND_ARGUMENT_BYTES {
        return Err(CommandError::Failed(
            "command arguments exceed the 1 MiB IPC limit".into(),
        ));
    }
    Ok(())
}

fn validate_native_menu(entries: &[NativeMenuEntry]) -> Result<(), String> {
    if entries.len() > MAX_NATIVE_MENU_ENTRIES
        || matches!(entries.first(), Some(NativeMenuEntry::Separator))
        || matches!(entries.last(), Some(NativeMenuEntry::Separator))
        || entries.windows(2).any(|pair| {
            matches!(
                pair,
                [NativeMenuEntry::Separator, NativeMenuEntry::Separator]
            )
        })
    {
        return Err("native menu layout is invalid".into());
    }
    for entry in entries {
        if let NativeMenuEntry::Command {
            command,
            title,
            accelerator,
        } = entry
        {
            if command.is_empty()
                || command.len() > MAX_COMMAND_ID_BYTES
                || !command.contains('.')
                || !command
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
                || title.trim().is_empty()
                || title.len() > 128
                || accelerator
                    .as_deref()
                    .is_some_and(|value| portable_accelerator(value).is_none())
            {
                return Err("native menu command is invalid".into());
            }
        }
    }
    Ok(())
}

fn portable_accelerator(value: &str) -> Option<String> {
    let parts = value.split('+').collect::<Vec<_>>();
    if !(2..=4).contains(&parts.len()) {
        return None;
    }
    let modifiers = &parts[..parts.len() - 1];
    let canonical = ["Primary", "Shift", "Alt"];
    let mut previous = None;
    for modifier in modifiers {
        let index = canonical
            .iter()
            .position(|candidate| candidate == modifier)?;
        if previous.is_some_and(|last| index <= last) {
            return None;
        }
        previous = Some(index);
    }
    if !modifiers.contains(&"Primary") {
        return None;
    }
    let mut translated = Vec::new();
    for part in parts {
        translated.push(match part {
            "Primary" => "CmdOrCtrl",
            "Shift" => "Shift",
            "Alt" => "Alt",
            key if key.len() == 1 && key.bytes().all(|byte| byte.is_ascii_alphanumeric()) => key,
            "Enter" => "Enter",
            "Escape" => "Escape",
            "Tab" => "Tab",
            "Backspace" => "Backspace",
            "Delete" => "Delete",
            "ArrowUp" => "Up",
            "ArrowDown" => "Down",
            "ArrowLeft" => "Left",
            "ArrowRight" => "Right",
            _ => return None,
        });
    }
    Some(translated.join("+"))
}

/// Export to PDF, Page Setup, and Print.
#[cfg(desktop)]
fn append_print_menu_items<'a>(
    app: &'a tauri::AppHandle,
    mut file_builder: SubmenuBuilder<'a, tauri::Wry, tauri::AppHandle>,
    entries: &[NativeMenuEntry],
) -> tauri::Result<SubmenuBuilder<'a, tauri::Wry, tauri::AppHandle>> {
    file_builder = file_builder.separator();
    for (command, title) in [
        ("file.exportPdf", "Export to PDF…"),
        ("file.exportSelectionPdf", "Export Selection to PDF…"),
        ("file.pageSetup", "Page Setup…"),
        ("file.print", "Print…"),
    ] {
        if native_menu_has(entries, command) {
            continue;
        }
        let mut item = MenuItemBuilder::with_id(command, title);
        if command == "file.print" {
            // Primary+P stays Quick Open (IMPROVEMENT_PLAN_3.md §7).
            item = item.accelerator("CmdOrCtrl+Alt+P");
        }
        file_builder = file_builder.item(&item.build(app)?);
    }
    Ok(file_builder.separator())
}

#[cfg(desktop)]
fn install_native_menu(app: &tauri::AppHandle, entries: &[NativeMenuEntry]) -> tauri::Result<()> {
    // Primary+W closes the file, as in other editors; the window takes Shift.
    let close_window = MenuItemBuilder::with_id(CLOSE_WINDOW_MENU_ID, "Close Window")
        .accelerator("CmdOrCtrl+Shift+W")
        .build(app)?;
    #[cfg(not(target_os = "macos"))]
    let quit = MenuItemBuilder::with_id(QUIT_MENU_ID, "Quit Nulis")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let mut file_builder = SubmenuBuilder::with_id(app, "nulis.file", "File");
    for entry in entries {
        match entry {
            NativeMenuEntry::Separator => file_builder = file_builder.separator(),
            NativeMenuEntry::Command {
                command,
                title,
                accelerator,
            } => {
                let mut item = MenuItemBuilder::with_id(command, title);
                if let Some(accelerator) = accelerator.as_deref().and_then(portable_accelerator) {
                    item = item.accelerator(accelerator);
                }
                file_builder = file_builder.item(&item.build(app)?);
            }
        }
    }
    if !entries.is_empty() {
        file_builder = file_builder.separator();
    }
    // Reload and Close File are always offered, even in a customized menu that omits them.
    if !native_menu_has(entries, RELOAD_MENU_ID) {
        file_builder = file_builder
            .item(&MenuItemBuilder::with_id(RELOAD_MENU_ID, "Reload from Disk").build(app)?);
    }
    for (command, title) in FILE_EXTRA_MENU_COMMANDS {
        if !native_menu_has(entries, command) {
            file_builder =
                file_builder.item(&MenuItemBuilder::with_id(*command, *title).build(app)?);
        }
    }
    let mut file_builder = append_print_menu_items(app, file_builder, entries)?;
    if !native_menu_has(entries, CLOSE_FILE_MENU_ID) {
        file_builder = file_builder.item(
            &MenuItemBuilder::with_id(CLOSE_FILE_MENU_ID, "Close File")
                .accelerator("CmdOrCtrl+W")
                .build(app)?,
        );
    }
    if !native_menu_has(entries, "file.closeAll") {
        file_builder = file_builder
            .item(&MenuItemBuilder::with_id("file.closeAll", "Close All Files").build(app)?);
    }
    // macOS places Settings in the application menu; other platforms use File.
    #[cfg(not(target_os = "macos"))]
    let file_builder = if native_menu_has(entries, OPEN_SETTINGS_MENU_ID) {
        file_builder
    } else {
        file_builder.item(&settings_item(app)?).separator()
    };
    let file_builder = file_builder.item(&close_window);
    #[cfg(not(target_os = "macos"))]
    let file_builder = file_builder.item(&quit);
    let file_menu = file_builder.build()?;

    let menu = Menu::default(app)?;
    #[cfg(target_os = "macos")]
    {
        // The default macOS menu places File after the application menu.
        let _ = menu.remove_at(1)?;
        menu.insert(&file_menu, 1)?;
        // Standard placement: Nulis ▸ Settings…, directly below About.
        if let Some(application) = menu.items()?.first().and_then(|item| item.as_submenu()) {
            // The application's own About page replaces the system panel.
            let _ = application.remove_at(0)?;
            application.insert(
                &MenuItemBuilder::with_id(ABOUT_MENU_ID, "About Nulis").build(app)?,
                0,
            )?;
            application.insert(&PredefinedMenuItem::separator(app)?, 1)?;
            application.insert(&settings_item(app)?, 2)?;
        }
    }
    #[cfg(all(
        not(target_os = "macos"),
        not(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))
    ))]
    {
        // Windows' default menu starts with File, so replace that submenu.
        let _ = menu.remove_at(0)?;
        menu.insert(&file_menu, 0)?;
    }
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    menu.insert(&file_menu, 0)?;

    add_command_menus(app, &menu)?;
    attach_menu(app, menu)
}

/// macOS has one menu bar for the whole application. Elsewhere an
/// application menu is drawn inside every window, including Settings and
/// Find, and comes back each time it is rebuilt; so it belongs to the editor
/// window alone. Menu clicks reach the global handler either way.
#[cfg(desktop)]
fn attach_menu(app: &tauri::AppHandle, menu: Menu<tauri::Wry>) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    app.set_menu(menu)?;
    #[cfg(not(target_os = "macos"))]
    if let Some(main) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        main.set_menu(menu)?;
    }
    Ok(())
}

/// Adds the View commands and the Tools and Workspace menus.
#[cfg(desktop)]
fn add_command_menus(app: &tauri::AppHandle, menu: &Menu<tauri::Wry>) -> tauri::Result<()> {
    let view = find_or_add_submenu(app, menu, "View")?;
    if !view.items()?.is_empty() {
        view.append(&PredefinedMenuItem::separator(app)?)?;
    }
    append_commands(app, &view, VIEW_MENU_COMMANDS)?;
    let edit = find_or_add_submenu(app, menu, "Edit")?;
    if !edit.items()?.is_empty() {
        edit.append(&PredefinedMenuItem::separator(app)?)?;
    }
    append_commands(app, &edit, EDIT_MENU_COMMANDS)?;
    let search = find_or_add_submenu(app, menu, "Search")?;
    for (index, section) in SEARCH_MENU_COMMANDS.iter().enumerate() {
        if index > 0 {
            search.append(&PredefinedMenuItem::separator(app)?)?;
        }
        append_commands(app, &search, section)?;
    }
    let format = find_or_add_submenu(app, menu, "Format")?;
    for (index, section) in FORMAT_MENU_COMMANDS.iter().enumerate() {
        if index > 0 {
            format.append(&PredefinedMenuItem::separator(app)?)?;
        }
        append_commands(app, &format, section)?;
    }
    // Tools are named and enabled for the focused document's language.
    let tools = find_or_add_submenu(app, menu, "Tools")?;
    let mut tool_items = Vec::new();
    for (id, title) in TOOLS_MENU_COMMANDS {
        let item = MenuItemBuilder::with_id(*id, *title).build(app)?;
        tools.append(&item)?;
        tool_items.push(item);
    }
    if let Ok(mut state) = app.state::<ToolsMenu>().0.lock() {
        apply_tools_menu(&tool_items, &state.1);
        state.0 = tool_items;
    }
    let workspace = find_or_add_submenu(app, menu, "Workspace")?;
    for (index, section) in WORKSPACE_MENU_COMMANDS.iter().enumerate() {
        if index > 0 {
            workspace.append(&PredefinedMenuItem::separator(app)?)?;
        }
        append_commands(app, &workspace, section)?;
    }
    // Help holds the user guide and the About page on every platform; the
    // default menu's own entries (a system About panel) are replaced.
    let help = find_or_add_submenu(app, menu, "Help")?;
    while !help.items()?.is_empty() {
        let _ = help.remove_at(0)?;
    }
    help.append(
        &MenuItemBuilder::with_id(HELP_MENU_ID, "Nulis Help")
            .accelerator("F1")
            .build(app)?,
    )?;
    help.append(&PredefinedMenuItem::separator(app)?)?;
    help.append(&MenuItemBuilder::with_id(ABOUT_MENU_ID, "About Nulis").build(app)?)?;
    Ok(())
}

#[cfg(desktop)]
fn settings_item(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::MenuItem<tauri::Wry>> {
    MenuItemBuilder::with_id(OPEN_SETTINGS_MENU_ID, "Settings…")
        .accelerator("CmdOrCtrl+,")
        .build(app)
}

#[cfg(desktop)]
fn native_menu_has(entries: &[NativeMenuEntry], id: &str) -> bool {
    entries
        .iter()
        .any(|entry| matches!(entry, NativeMenuEntry::Command { command, .. } if command == id))
}

/// Returns the named top-level submenu, creating it before Window/Help when absent.
#[cfg(desktop)]
fn find_or_add_submenu(
    app: &tauri::AppHandle,
    menu: &Menu<tauri::Wry>,
    name: &str,
) -> tauri::Result<Submenu<tauri::Wry>> {
    let items = menu.items()?;
    let titled = |title: &str| {
        items.iter().position(|item| {
            item.as_submenu()
                .and_then(|submenu| submenu.text().ok())
                .is_some_and(|text| text == title)
        })
    };
    if let Some(existing) = titled(name).and_then(|index| items[index].as_submenu().cloned()) {
        return Ok(existing);
    }
    let created = SubmenuBuilder::new(app, name).build()?;
    match titled("Window").or_else(|| titled("Help")) {
        Some(index) => menu.insert(&created, index)?,
        None => menu.append(&created)?,
    }
    Ok(created)
}

#[cfg(desktop)]
fn append_commands(
    app: &tauri::AppHandle,
    submenu: &Submenu<tauri::Wry>,
    commands: &[(&str, &str)],
) -> tauri::Result<()> {
    for (id, title) in commands {
        submenu.append(&MenuItemBuilder::with_id(*id, *title).build(app)?)?;
    }
    Ok(())
}

/// Keeps a remembered frame only if enough of it is still on a connected
/// display, shrinking it to that display when the screen got smaller.
fn visible_frame(frame: WindowFrame, monitors: &[WindowFrame]) -> Option<WindowFrame> {
    let width = frame.width.max(SETTINGS_MIN_SIZE.0);
    let height = frame.height.max(SETTINGS_MIN_SIZE.1);
    monitors.iter().find_map(|monitor| {
        let width = width.min(monitor.width);
        let height = height.min(monitor.height);
        let overlap = |start: i32, len: u32, other_start: i32, other_len: u32| {
            let end = i64::from(start) + i64::from(len);
            let other_end = i64::from(other_start) + i64::from(other_len);
            end.min(other_end) - i64::from(start.max(other_start))
        };
        (overlap(frame.x, width, monitor.x, monitor.width) >= MIN_VISIBLE_FRAME.0
            && overlap(frame.y, height, monitor.y, monitor.height) >= MIN_VISIBLE_FRAME.1)
            .then_some(WindowFrame {
                x: frame.x,
                y: frame.y,
                width,
                height,
            })
    })
}

fn settings_frame_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|directory| directory.join("settings-window.json"))
}

fn load_settings_frame(app: &tauri::AppHandle) -> Option<WindowFrame> {
    let path = settings_frame_path(app)?;
    if std::fs::metadata(&path).ok()?.len() > MAX_WINDOW_FRAME_BYTES {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn remember_settings_frame(window: &tauri::Window) {
    if window.is_minimized().unwrap_or(true) {
        return;
    }
    let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    if let Ok(mut frame) = window.state::<SettingsFrame>().0.lock() {
        *frame = Some(WindowFrame {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        });
    }
}

fn persist_settings_frame(app: &tauri::AppHandle) {
    let frame = app
        .state::<SettingsFrame>()
        .0
        .lock()
        .ok()
        .and_then(|frame| *frame);
    if let (Some(frame), Some(path)) = (frame, settings_frame_path(app)) {
        if let Ok(bytes) = serde_json::to_vec(&frame) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

/// Opens the singleton Settings window or brings the existing one forward.
fn show_settings_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW_LABEL,
        tauri::WebviewUrl::App("index.html?window=settings".into()),
    )
    .title("Settings")
    .inner_size(880.0, 620.0)
    .min_inner_size(
        f64::from(SETTINGS_MIN_SIZE.0),
        f64::from(SETTINGS_MIN_SIZE.1),
    )
    .resizable(true)
    .visible(false)
    .build()?;
    detach_application_menu(&window)?;
    show_when_ready(&window);
    let monitors = window
        .available_monitors()?
        .iter()
        .map(|monitor| WindowFrame {
            x: monitor.position().x,
            y: monitor.position().y,
            width: monitor.size().width,
            height: monitor.size().height,
        })
        .collect::<Vec<_>>();
    if let Some(frame) = load_settings_frame(app).and_then(|frame| visible_frame(frame, &monitors))
    {
        window.set_size(tauri::PhysicalSize::new(frame.width, frame.height))?;
        window.set_position(tauri::PhysicalPosition::new(frame.x, frame.y))?;
    }
    Ok(())
}

/// Files the operating system asked Nulis to open: from "Open With", a
/// double-click, the Dock, or the command line. They wait here until the
/// page is ready to open them, and are sent to it directly afterwards.
#[derive(Default)]
struct LaunchPaths(Mutex<(bool, Vec<String>)>);

/// Keeps the arguments that name existing files, resolved against `directory`.
fn file_arguments<I: IntoIterator<Item = String>>(arguments: I, directory: &Path) -> Vec<String> {
    arguments
        .into_iter()
        .filter(|argument| !argument.starts_with('-'))
        .map(|argument| directory.join(argument))
        .filter(|path| path.is_file())
        .map(|path| path.canonicalize().unwrap_or(path).display().to_string())
        .collect()
}

/// Opens `paths` in the running window, or keeps them until it is ready.
fn open_launch_paths(app: &tauri::AppHandle, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    let Some(state) = app.try_state::<LaunchPaths>() else {
        return;
    };
    let Ok(mut pending) = state.0.lock() else {
        return;
    };
    if pending.0 {
        drop(pending);
        let _ = app.emit_to(MAIN_WINDOW_LABEL, OPEN_PATHS_EVENT, paths);
        if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    } else {
        pending.1.extend(paths);
    }
}

/// Hands the page the files Nulis was launched with; later ones arrive as events.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn take_launch_paths(
    state: State<'_, LaunchPaths>,
    window: tauri::WebviewWindow,
) -> Result<Vec<String>, String> {
    if window.label() != MAIN_WINDOW_LABEL {
        return Err("only the editor window opens files".into());
    }
    let mut pending = state.0.lock().map_err(|_| "launch state unavailable")?;
    pending.0 = true;
    Ok(std::mem::take(&mut pending.1))
}

/// A second launch focuses the running window and opens any files it names
/// ("Open with" on Windows and Linux starts the application again).
#[allow(clippy::needless_pass_by_value)] // The plugin's callback signature passes ownership.
fn on_second_launch(app: &tauri::AppHandle, arguments: Vec<String>, directory: String) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    let paths = file_arguments(arguments.into_iter().skip(1), Path::new(&directory));
    open_launch_paths(app, paths);
}

/// Handles application lifecycle events, including files the system asks Nulis to open.
// Tauri passes the event by value; only macOS consumes it (its file list).
#[allow(clippy::needless_pass_by_value)]
fn handle_run_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    match event {
        tauri::RunEvent::Ready => {
            // Files named on the command line, which is how Windows and
            // Linux pass the file chosen with "Open with".
            let directory = std::env::current_dir().unwrap_or_default();
            let paths = file_arguments(std::env::args().skip(1), &directory);
            open_launch_paths(app, paths);
        }
        // macOS sends files from Finder's Open With, a double-click, or
        // the Dock icon as an event rather than as arguments.
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Opened { urls } => {
            let paths = urls
                .into_iter()
                .filter_map(|url| url.to_file_path().ok())
                .map(|path| path.display().to_string())
                .collect();
            open_launch_paths(app, paths);
        }
        _ => {}
    }
}

/// Opens the singleton Help window or brings the existing one forward.
fn show_help_window(app: &tauri::AppHandle, topic: Option<&HelpTopic>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(HELP_WINDOW_LABEL) {
        if let Some(topic) = topic {
            let _ = app.emit_to(HELP_WINDOW_LABEL, HELP_TOPIC_EVENT, topic);
        }
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    let query = topic.map_or_else(String::new, |topic| {
        format!("&section={}&heading={}", topic.section, topic.heading)
    });
    let window = tauri::WebviewWindowBuilder::new(
        app,
        HELP_WINDOW_LABEL,
        tauri::WebviewUrl::App(format!("index.html?window=help{query}").into()),
    )
    .title("Nulis Help")
    .inner_size(980.0, 700.0)
    .min_inner_size(f64::from(HELP_MIN_SIZE.0), f64::from(HELP_MIN_SIZE.1))
    .resizable(true)
    .visible(false)
    .build()?;
    detach_application_menu(&window)?;
    show_when_ready(&window);
    Ok(())
}

/// On Windows and Linux an application-wide menu would be drawn inside every
/// window. The menu is attached to the editor window only (see
/// `install_native_menu`); this also clears one a platform may add by default.
/// macOS has one global menu bar and needs nothing.
#[cfg_attr(target_os = "macos", allow(clippy::unnecessary_wraps))]
fn detach_application_menu(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    #[cfg(not(target_os = "macos"))]
    window.remove_menu()?;
    #[cfg(target_os = "macos")]
    let _ = window;
    Ok(())
}

/// Opens a singleton auxiliary window or brings the existing one forward.
fn show_aux_window(
    app: &tauri::AppHandle,
    label: &str,
    title: &str,
    size: (f64, f64),
    min_size: (u32, u32),
) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(label) {
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App(format!("index.html?window={label}").into()),
    )
    .title(title)
    .inner_size(size.0, size.1)
    .min_inner_size(f64::from(min_size.0), f64::from(min_size.1))
    .resizable(true)
    .visible(false)
    .build()?;
    detach_application_menu(&window)?;
    show_when_ready(&window);
    Ok(())
}

/// Longest a new window stays hidden waiting for its page to say it is ready.
const WINDOW_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);
const SECONDARY_WINDOW_LABELS: &[&str] = &[
    SETTINGS_WINDOW_LABEL,
    HELP_WINDOW_LABEL,
    FIND_WINDOW_LABEL,
    RESULTS_WINDOW_LABEL,
    PRINT_WINDOW_LABEL,
];

/// A new window is created hidden, so it never shows as an empty white or
/// black rectangle while its web view starts. Its page calls `window_ready`
/// once it has drawn; if that never comes, the window appears anyway.
fn show_when_ready(window: &tauri::WebviewWindow) {
    let window = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(WINDOW_READY_TIMEOUT);
        reveal(&window);
    });
}

fn reveal(window: &tauri::WebviewWindow) {
    if !window.is_visible().unwrap_or(true) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Called by a secondary window's page after its first themed paint.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts WebviewWindow by value.
fn window_ready(window: tauri::WebviewWindow) -> Result<(), String> {
    if !SECONDARY_WINDOW_LABELS.contains(&window.label()) {
        return Err("only a secondary window reports that it is ready".into());
    }
    reveal(&window);
    Ok(())
}

// Async so the window is built off the main thread (see open_settings_window).
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts AppHandle by value.
async fn open_find_window(app: tauri::AppHandle) -> Result<(), String> {
    show_aux_window(
        &app,
        FIND_WINDOW_LABEL,
        "Find and Replace",
        (480.0, 460.0),
        FIND_MIN_SIZE,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts AppHandle by value.
async fn open_results_window(app: tauri::AppHandle) -> Result<(), String> {
    show_aux_window(
        &app,
        RESULTS_WINDOW_LABEL,
        "Lines Containing",
        (720.0, 360.0),
        RESULTS_MIN_SIZE,
    )
    .map_err(|error| error.to_string())
}

/// What the Print Preview window shows: the pages the core laid out
/// (`file.printStart`), or a document's rendered Markdown preview.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum PrintContent {
    Pages,
    #[serde(rename_all = "camelCase")]
    Markdown {
        document_id: DocumentId,
        title: String,
    },
}

/// The content the editor window last chose for Print Preview.
#[derive(Default)]
struct PrintContentState(Mutex<Option<PrintContent>>);

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts AppHandle and WebviewWindow by value.
async fn open_print_window(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    content: PrintContent,
) -> Result<(), String> {
    if window.label() != MAIN_WINDOW_LABEL {
        return Err("only the editor window chooses what to print".into());
    }
    if let PrintContent::Markdown { title, .. } = &content {
        if title.len() > MAX_PRINT_TITLE_BYTES {
            return Err("the document name is too long".into());
        }
    }
    let state = app.state::<PrintContentState>();
    *state.0.lock().map_err(|_| "print state is unavailable")? = Some(content);
    // An open window loads the new content; a new one loads it as it starts.
    let _ = app.emit_to(PRINT_WINDOW_LABEL, PRINT_CONTENT_EVENT, ());
    show_aux_window(
        &app,
        PRINT_WINDOW_LABEL,
        "Print Preview",
        (900.0, 760.0),
        PRINT_MIN_SIZE,
    )
    .map_err(|error| error.to_string())
}

/// What Print Preview shows, as the window reads it.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum PrintWindowContent {
    /// Read the pages with `print.pages`.
    Pages,
    Markdown {
        title: String,
        source: MarkdownPreviewSource,
    },
    Empty,
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State and WebviewWindow by value.
fn print_window_content(
    core: State<'_, NulisCore>,
    print: State<'_, PrintContentState>,
    window: tauri::WebviewWindow,
) -> Result<PrintWindowContent, String> {
    require_print_window(&window)?;
    let content = print
        .0
        .lock()
        .map_err(|_| "print state is unavailable")?
        .clone();
    Ok(match content {
        None => PrintWindowContent::Empty,
        Some(PrintContent::Pages) => PrintWindowContent::Pages,
        Some(PrintContent::Markdown { document_id, title }) => PrintWindowContent::Markdown {
            title,
            source: core
                .markdown_preview_source(&document_id)
                .map_err(|error| error.to_string())?,
        },
    })
}

/// An image in the Markdown document being printed, by its relative path.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State and WebviewWindow by value.
fn print_window_markdown_image(
    core: State<'_, NulisCore>,
    print: State<'_, PrintContentState>,
    window: tauri::WebviewWindow,
    relative_path: String,
) -> Result<String, String> {
    require_print_window(&window)?;
    let content = print
        .0
        .lock()
        .map_err(|_| "print state is unavailable")?
        .clone();
    let Some(PrintContent::Markdown { document_id, .. }) = content else {
        return Err("no Markdown document is being printed".into());
    };
    core.markdown_preview_image(&document_id, &relative_path)
        .map_err(|error| error.to_string())
}

fn require_print_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == PRINT_WINDOW_LABEL {
        Ok(())
    } else {
        Err("only the Print Preview window reads what is printed".into())
    }
}

/// The paper the Print Preview laid its pages out for, portrait size in millimetres.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PrintPaper {
    width_mm: f64,
    height_mm: f64,
    landscape: bool,
}

impl PrintPaper {
    /// Smallest and largest paper side accepted, in millimetres (A10 to A0 and a bit more).
    const SIDE_MM: std::ops::RangeInclusive<f64> = 20.0..=1500.0;

    fn validate(&self) -> Result<(), String> {
        if [self.width_mm, self.height_mm]
            .iter()
            .all(|side| side.is_finite() && Self::SIDE_MM.contains(side))
        {
            Ok(())
        } else {
            Err("paper size must be between 20 and 1500 mm on each side".into())
        }
    }
}

/// Opens the system print dialog for the Print Preview window's pages.
/// On macOS the dialog is a sheet on this window, so the window must be
/// visible; Windows and Linux show their own dialog.
///
/// Not async: Tauri runs it on the main thread, which `AppKit` requires.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts WebviewWindow by value.
fn print_window_print(window: tauri::WebviewWindow, paper: PrintPaper) -> Result<(), String> {
    if window.label() != PRINT_WINDOW_LABEL {
        return Err("only the Print Preview window prints".into());
    }
    paper.validate()?;
    #[cfg(target_os = "macos")]
    set_macos_print_paper(paper);
    open_print_dialog(&window, paper)
}

#[cfg(not(target_os = "linux"))]
#[allow(clippy::unnecessary_wraps)] // Same signature as the Linux version.
fn open_print_dialog(window: &tauri::WebviewWindow, _paper: PrintPaper) -> Result<(), String> {
    window.print().map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
fn open_print_dialog(window: &tauri::WebviewWindow, paper: PrintPaper) -> Result<(), String> {
    print_on_linux(window, paper)
}

/// `WebKitGTK` opens the print dialog on the default paper, portrait, and
/// shrinks a landscape page onto it. This opens the same dialog with the
/// Print Preview's paper already chosen.
///
/// A landscape orientation in the page setup makes `WebKitGTK` print blank
/// pages (seen with 2.50 on Ubuntu 26.04), so a landscape page is given as
/// an upright custom paper of the turned size. The page margins are 0: the
/// Print Preview's pages carry their own.
#[cfg(target_os = "linux")]
fn print_on_linux(window: &tauri::WebviewWindow, paper: PrintPaper) -> Result<(), String> {
    window
        .with_webview(move |webview| {
            use gtk::prelude::{Cast, WidgetExt};
            use webkit2gtk::PrintOperationExt;
            let view = webview.inner();
            let size = linux_paper_size(paper);
            let setup = gtk::PageSetup::new();
            setup.set_paper_size(&size);
            setup.set_orientation(gtk::PageOrientation::Portrait);
            setup.set_top_margin(0.0, gtk::Unit::Mm);
            setup.set_bottom_margin(0.0, gtk::Unit::Mm);
            setup.set_left_margin(0.0, gtk::Unit::Mm);
            setup.set_right_margin(0.0, gtk::Unit::Mm);
            let settings = gtk::PrintSettings::new();
            settings.set_paper_size(&size);
            settings.set_orientation(gtk::PageOrientation::Portrait);
            let operation = webkit2gtk::PrintOperation::new(&view);
            operation.set_page_setup(&setup);
            operation.set_print_settings(&settings);
            let parent = view
                .toplevel()
                .and_then(|widget| widget.downcast::<gtk::Window>().ok());
            operation.run_dialog(parent.as_ref());
        })
        .map_err(|error| error.to_string())
}

/// The paper for a page as the Print Preview laid it out: the named GTK
/// paper for an upright page of a size Page Setup offers, so the dialog and
/// the printer recognize it, and otherwise a custom paper of the page's
/// size (a landscape page turned upright).
#[cfg(target_os = "linux")]
fn linux_paper_size(paper: PrintPaper) -> gtk::PaperSize {
    const NAMED: [(&str, &str, f64, f64); 5] = [
        ("iso_a4", "A4", 210.0, 297.0),
        ("na_letter", "Letter", 215.9, 279.4),
        ("na_legal", "Legal", 215.9, 355.6),
        ("iso_a3", "A3", 297.0, 420.0),
        ("iso_a5", "A5", 148.0, 210.0),
    ];
    let known = NAMED.iter().find(|&&(_, _, width, height)| {
        (paper.width_mm - width).abs() < 0.5 && (paper.height_mm - height).abs() < 0.5
    });
    match (known, paper.landscape) {
        (Some(&(name, _, _, _)), false) => gtk::PaperSize::new(Some(name)),
        (known, landscape) => {
            let label = known.map_or("Page Setup", |&(_, label, _, _)| label);
            let (width, height) = if landscape {
                (paper.height_mm, paper.width_mm)
            } else {
                (paper.width_mm, paper.height_mm)
            };
            let display = if landscape {
                format!("{label} landscape")
            } else {
                label.to_owned()
            };
            gtk::PaperSize::new_custom("nulis-page", &display, width, height, gtk::Unit::Mm)
        }
    }
}

/// macOS prints on the paper of the last print job and ignores the page's
/// `@page` size, shrinking the pages to fit. wry prints with the shared print
/// info, so setting its paper first makes the dialog open on the right paper.
#[cfg(target_os = "macos")]
fn set_macos_print_paper(paper: PrintPaper) {
    use objc2_app_kit::{NSPaperOrientation, NSPrintInfo};
    use objc2_foundation::NSSize;
    const POINTS_PER_MM: f64 = 72.0 / 25.4;
    let (width, height) = if paper.landscape {
        (paper.height_mm, paper.width_mm)
    } else {
        (paper.width_mm, paper.height_mm)
    };
    let info = NSPrintInfo::sharedPrintInfo();
    info.setOrientation(if paper.landscape {
        NSPaperOrientation::Landscape
    } else {
        NSPaperOrientation::Portrait
    });
    info.setPaperSize(NSSize::new(width * POINTS_PER_MM, height * POINTS_PER_MM));
}

/// Closes the calling Find or results window.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts WebviewWindow by value.
fn close_find_window(window: tauri::WebviewWindow) -> Result<(), String> {
    require_find_window(&window)?;
    window.hide().map_err(|error| error.to_string())
}

/// Room kept free above and below the Find window when it grows to fit, for
/// its title bar and so it never touches the screen edges.
const FIND_FIT_MARGIN: f64 = 64.0;

/// The height the Find window takes for content `requested` points tall:
/// at least its minimum, at most `max` (the screen's usable height).
fn fitted_find_height(requested: f64, max: f64) -> Option<f64> {
    if !requested.is_finite() || requested <= 0.0 {
        return None;
    }
    let min = f64::from(FIND_MIN_SIZE.1);
    Some(requested.ceil().clamp(min, max.max(min)))
}

/// Sets the Find window's height so its page shows `content` points, keeping
/// its width. The page reports what it currently shows as `viewport`; the
/// window grows or shrinks by the difference, so the result is right whatever
/// the platform counts as the window's own height (a title bar or not).
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts WebviewWindow by value.
fn fit_find_window(
    window: tauri::WebviewWindow,
    content: f64,
    viewport: f64,
) -> Result<(), String> {
    if window.label() != FIND_WINDOW_LABEL {
        return Err("only the Find window may do this".into());
    }
    if !viewport.is_finite() || viewport <= 0.0 {
        return Err("the viewport height must be a positive number".into());
    }
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let size = window
        .inner_size()
        .map_err(|error| error.to_string())?
        .to_logical::<f64>(scale);
    let max = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .map_or(f64::MAX, |monitor| {
            f64::from(monitor.work_area().size.height) / monitor.scale_factor() - FIND_FIT_MARGIN
        });
    let height = fitted_find_height(size.height + (content - viewport), max)
        .ok_or("the content height must be a positive number")?;
    if (height - size.height).abs() < 1.0 {
        return Ok(());
    }
    window
        .set_size(tauri::LogicalSize::new(size.width, height))
        .map_err(|error| error.to_string())
}

/// Relays a message from the Find or results window to the editor window,
/// which validates and acts on it. Going to a match brings the editor forward.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts its arguments by value.
fn find_message(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    message: Value,
) -> Result<(), String> {
    require_find_window(&window)?;
    let kind = message
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("a find message needs a kind")?;
    if !FIND_MESSAGE_KINDS.contains(&kind) {
        return Err(format!("unknown find message: {kind}"));
    }
    let size = serde_json::to_vec(&message)
        .map_err(|error| error.to_string())?
        .len();
    if size > MAX_FIND_MESSAGE_BYTES {
        return Err("the find message is too large".into());
    }
    if kind == "openMatch" {
        if let Some(main) = app.get_webview_window(MAIN_WINDOW_LABEL) {
            let _ = main.set_focus();
        }
    }
    app.emit_to(MAIN_WINDOW_LABEL, FIND_MESSAGE_EVENT, message)
        .map_err(|error| error.to_string())
}

fn require_find_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == FIND_WINDOW_LABEL || window.label() == RESULTS_WINDOW_LABEL {
        Ok(())
    } else {
        Err("only the Find and results windows may do this".into())
    }
}

fn require_settings_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == SETTINGS_WINDOW_LABEL {
        Ok(())
    } else {
        Err("only the Settings window may do this".into())
    }
}

// Async so the window is built off the main thread: WebView2 deadlocks, and
// the window stays blank, when a window is built in a synchronous command.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts AppHandle by value.
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    show_settings_window(&app).map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts AppHandle by value.
fn close_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    match app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        Some(window) => window.destroy().map_err(|error| error.to_string()),
        None => Ok(()),
    }
}

// Async so the window is built off the main thread: WebView2 deadlocks, and
// the window stays blank, when a window is built in a synchronous command.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts AppHandle by value.
async fn open_help_window(app: tauri::AppHandle, topic: Option<HelpTopic>) -> Result<(), String> {
    if let Some(topic) = &topic {
        topic.validate()?;
    }
    show_help_window(&app, topic.as_ref()).map_err(|error| error.to_string())
}

/// A place in Nulis Help: a section and a heading in it, by their slugs.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct HelpTopic {
    section: String,
    #[serde(default)]
    heading: String,
}

impl HelpTopic {
    fn validate(&self) -> Result<(), String> {
        let slug = |text: &str| {
            text.len() <= 64
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        };
        if !self.section.is_empty() && slug(&self.section) && slug(&self.heading) {
            Ok(())
        } else {
            Err("a help topic is a section and a heading written as slugs".into())
        }
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts AppHandle by value.
fn close_help_window(app: tauri::AppHandle) -> Result<(), String> {
    match app.get_webview_window(HELP_WINDOW_LABEL) {
        Some(window) => window.hide().map_err(|error| error.to_string()),
        None => Ok(()),
    }
}

/// Relays an unapplied appearance preview to the editor window; `null` reverts it.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts its arguments by value.
fn settings_preview(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    appearance: Option<Value>,
) -> Result<(), String> {
    require_settings_window(&window)?;
    let size = serde_json::to_vec(&appearance)
        .map_err(|error| error.to_string())?
        .len();
    if size > MAX_APPEARANCE_PREVIEW_BYTES {
        return Err("appearance preview is too large".into());
    }
    app.emit_to(MAIN_WINDOW_LABEL, APPEARANCE_PREVIEW_EVENT, appearance)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts its arguments by value.
fn settings_changed(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    require_settings_window(&window)?;
    // Help follows the theme too; it is simply absent when not open.
    for label in [
        HELP_WINDOW_LABEL,
        FIND_WINDOW_LABEL,
        RESULTS_WINDOW_LABEL,
        PRINT_WINDOW_LABEL,
    ] {
        let _ = app.emit_to(label, CUSTOMIZATION_CHANGED_EVENT, ());
    }
    app.emit_to(MAIN_WINDOW_LABEL, CUSTOMIZATION_CHANGED_EVENT, ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts its arguments by value.
fn settings_main_action(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    action: String,
) -> Result<(), String> {
    require_settings_window(&window)?;
    if !SETTINGS_MAIN_ACTIONS.contains(&action.as_str()) {
        return Err("that action is not available from Settings".into());
    }
    if let Some(main) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = main.set_focus();
    }
    app.emit_to(MAIN_WINDOW_LABEL, NATIVE_MENU_EVENT, action)
        .map_err(|error| error.to_string())
}

/// Tells the native Tools menu which tool set the focused document has.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts its arguments by value.
fn set_tools_menu(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    tools: ToolsMenuState,
) -> Result<(), String> {
    if window.label() != MAIN_WINDOW_LABEL {
        return Err("only the editor window sets the Tools menu".into());
    }
    let valid = |label: &str| {
        let count = label.chars().count();
        (1..=MAX_TOOL_LABEL_CHARS).contains(&count) && !label.chars().any(char::is_control)
    };
    if tools
        .labels()
        .into_iter()
        .flatten()
        .any(|label| !valid(label))
    {
        return Err("invalid tool label".into());
    }
    #[cfg(desktop)]
    if let Ok(mut state) = app.state::<ToolsMenu>().0.lock() {
        apply_tools_menu(&state.0, &tools);
        state.1 = tools;
    }
    #[cfg(not(desktop))]
    let _ = (app, tools);
    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn configure_native_menu(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    entries: Vec<NativeMenuEntry>,
) -> Result<(), String> {
    if window.label() != MAIN_WINDOW_LABEL {
        return Err("only the editor window configures the native menu".into());
    }
    validate_native_menu(&entries)?;
    #[cfg(desktop)]
    install_native_menu(&app, &entries).map_err(|error| error.to_string())?;
    Ok(())
}

/// macOS adds an `AutoFill` item to the Edit menu of any application whose
/// Edit menu has the standard cut, copy, and paste actions. Nulis is an
/// editor, not a form, so the item is turned off before `AppKit` builds the
/// menu. The key was confirmed against a minimal `AppKit` menu on macOS 26.
#[cfg(target_os = "macos")]
fn disable_webview_autofill_menu() {
    use objc2_foundation::{NSString, NSUserDefaults};
    NSUserDefaults::standardUserDefaults().setBool_forKey(
        false,
        &NSString::from_str("NSAutoFillSystemInsertMenuEnabled"),
    );
}

fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    match (window.label(), event) {
        (SETTINGS_WINDOW_LABEL, tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)) => {
            remember_settings_frame(window);
        }
        // Find, results, and Help are hidden rather than closed, so opening
        // them again is instant (a new web view takes a moment, most of all
        // on Windows) and Find keeps its text. They close with the editor.
        (
            FIND_WINDOW_LABEL | RESULTS_WINDOW_LABEL | HELP_WINDOW_LABEL,
            tauri::WindowEvent::CloseRequested { api, .. },
        ) => {
            api.prevent_close();
            let _ = window.hide();
        }
        // The prepared pages can be large; they go with the window.
        (PRINT_WINDOW_LABEL, tauri::WindowEvent::Destroyed) => {
            let app = window.app_handle();
            if let Some(state) = app.try_state::<PrintContentState>() {
                if let Ok(mut content) = state.0.lock() {
                    *content = None;
                }
            }
            if let Some(core) = app.try_state::<NulisCore>() {
                let _ = core.execute_command(
                    "print.release",
                    nulis_core::CommandContext::default(),
                    Value::Null,
                );
            }
        }
        (SETTINGS_WINDOW_LABEL, tauri::WindowEvent::Destroyed) => {
            let app = window.app_handle();
            persist_settings_frame(app);
            // Any unapplied preview ends with the window that showed it.
            let _ = app.emit_to(MAIN_WINDOW_LABEL, APPEARANCE_PREVIEW_EVENT, Value::Null);
        }
        // With nothing unsaved the window closes at once. This does not
        // wait for the page, so the application can always be closed even
        // if the page has stopped answering. With unsaved documents the
        // page asks what to do with them, as before.
        (MAIN_WINDOW_LABEL, tauri::WindowEvent::CloseRequested { .. }) => {
            let clean = window
                .app_handle()
                .try_state::<NulisCore>()
                .is_some_and(|core| matches!(core.has_dirty_documents(), Ok(false)));
            if clean {
                let _ = window.destroy();
            }
        }
        (MAIN_WINDOW_LABEL, tauri::WindowEvent::Destroyed) => {
            for label in [
                SETTINGS_WINDOW_LABEL,
                HELP_WINDOW_LABEL,
                FIND_WINDOW_LABEL,
                RESULTS_WINDOW_LABEL,
                PRINT_WINDOW_LABEL,
            ] {
                if let Some(secondary) = window.app_handle().get_webview_window(label) {
                    let _ = secondary.destroy();
                }
            }
        }
        _ => {}
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the native Nulis desktop event loop.
///
/// # Panics
/// Panics if Tauri cannot initialize or the platform event loop exits with an error.
pub fn run() {
    #[cfg(target_os = "macos")]
    disable_webview_autofill_menu();
    tauri::Builder::default()
        // Registered first, as the plugin requires. Two copies of Nulis would
        // share one session file and one set of recovery checkpoints, and
        // each would overwrite the other's. A second launch brings the
        // running window forward instead.
        .plugin(tauri_plugin_single_instance::init(on_second_launch))
        .manage(LaunchPaths::default())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .on_menu_event(|app, event| match event.id().as_ref() {
            CLOSE_WINDOW_MENU_ID | QUIT_MENU_ID => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.close();
                }
            }
            // Opened natively so it works whichever window has focus.
            OPEN_SETTINGS_MENU_ID => {
                let app = app.clone();
                // Never build a window inside a menu handler (WebView2 deadlock).
                tauri::async_runtime::spawn(async move {
                    let _ = show_settings_window(&app);
                });
            }
            // Opened natively so it works whichever window has focus.
            HELP_MENU_ID => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = show_help_window(&app, None);
                });
            }
            // The About page lives in the main window, which may be behind another one.
            ABOUT_MENU_ID => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_focus();
                }
                let _ = app.emit_to("main", NATIVE_MENU_EVENT, ABOUT_MENU_ID);
            }
            command if command.contains('.') => {
                let _ = app.emit_to("main", NATIVE_MENU_EVENT, command);
            }
            _ => {}
        })
        .on_window_event(handle_window_event)
        .setup(|app| {
            let session_root = app.path().app_data_dir()?.join("sessions");
            app.manage(NulisCore::with_session_root(session_root));
            app.manage(SettingsFrame::default());
            app.manage(PrintContentState::default());
            #[cfg(desktop)]
            app.manage(ToolsMenu::default());
            #[cfg(desktop)]
            install_native_menu(app.handle(), &default_native_menu())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            close_window_after_save,
            application_info,
            list_commands,
            execute_command,
            editor_viewport,
            editor_hex_viewport,
            editor_viewport_at,
            editor_document_map,
            editor_markdown_preview_source,
            editor_markdown_preview_image,
            editor_viewport_line,
            configure_native_menu,
            set_tools_menu,
            open_settings_window,
            close_settings_window,
            open_help_window,
            close_help_window,
            open_find_window,
            open_print_window,
            print_window_content,
            print_window_markdown_image,
            print_window_print,
            open_results_window,
            close_find_window,
            fit_find_window,
            window_ready,
            find_message,
            take_launch_paths,
            settings_preview,
            settings_changed,
            settings_main_action
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Nulis desktop application")
        .run(handle_run_event);
}

#[cfg(test)]
mod tests {
    use super::{
        authorize_window_command, fitted_find_height, portable_accelerator, validate_native_menu,
        validate_request, visible_frame, CommandContext, ExecuteCommandRequest, HelpTopic,
        NativeMenuEntry, PrintContent, PrintPaper, WindowFrame,
    };
    use serde_json::{json, Value};

    #[test]
    fn help_topics_are_slugs_only() {
        let topic = |section: &str, heading: &str| HelpTopic {
            section: section.into(),
            heading: heading.into(),
        };
        assert!(topic("search", "regular-expressions").validate().is_ok());
        assert!(topic("search", "").validate().is_ok());
        assert!(topic("", "x").validate().is_err());
        assert!(topic("search&x=1", "").validate().is_err());
        assert!(topic("search", "a b").validate().is_err());
    }

    #[test]
    fn find_window_height_stays_between_its_minimum_and_the_screen() {
        assert_eq!(fitted_find_height(431.2, 900.0), Some(432.0));
        assert_eq!(fitted_find_height(120.0, 900.0), Some(360.0));
        assert_eq!(fitted_find_height(2_000.0, 900.0), Some(900.0));
        // A screen shorter than the minimum still gets the minimum.
        assert_eq!(fitted_find_height(500.0, 200.0), Some(360.0));
        assert_eq!(fitted_find_height(f64::NAN, 900.0), None);
        assert_eq!(fitted_find_height(0.0, 900.0), None);
    }

    #[test]
    fn rejects_oversized_command_ids() {
        let request = ExecuteCommandRequest {
            id: format!("test.{}", "x".repeat(300)),
            context: CommandContext::default(),
            arguments: Value::Null,
        };
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn rejects_oversized_argument_payloads() {
        let request = ExecuteCommandRequest {
            id: "test.payload".into(),
            context: CommandContext::default(),
            arguments: json!("x".repeat(1024 * 1024)),
        };
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn portable_shortcuts_are_translated_for_tauri() {
        assert_eq!(
            portable_accelerator("Primary+Shift+S").as_deref(),
            Some("CmdOrCtrl+Shift+S")
        );
        assert!(portable_accelerator("Control+S").is_none());
        assert!(portable_accelerator("Shift+Primary+S").is_none());
    }

    #[test]
    fn settings_window_is_limited_to_customization_commands() {
        for id in [
            "file.open",
            "editor.insertText",
            "terminal.create",
            "session.load",
        ] {
            assert!(authorize_window_command("main", id).is_ok());
            assert!(authorize_window_command("settings", id).is_err(), "{id}");
        }
        for id in [
            "customization.saveChecked",
            "theme.install",
            "theme.save",
            "plugin.list",
        ] {
            assert!(authorize_window_command("settings", id).is_ok(), "{id}");
        }
        // Help can read the theme and nothing else.
        for id in ["customization.state", "theme.list"] {
            assert!(authorize_window_command("help", id).is_ok(), "{id}");
        }
        for id in ["file.open", "customization.saveChecked", "plugin.list"] {
            assert!(authorize_window_command("help", id).is_err(), "{id}");
        }
        assert!(authorize_window_command("other", "theme.list").is_err());
        let a4 = PrintPaper {
            width_mm: 210.0,
            height_mm: 297.0,
            landscape: false,
        };
        assert!(a4.validate().is_ok());
        for (width_mm, height_mm) in [(f64::NAN, 297.0), (210.0, 0.0), (5000.0, 297.0)] {
            let paper = PrintPaper {
                width_mm,
                height_mm,
                ..a4
            };
            assert!(paper.validate().is_err(), "{width_mm} × {height_mm}");
        }
        // Print Preview can read the theme and the prepared pages, nothing else:
        // it cannot lay out a document itself or release the pages.
        for id in ["customization.state", "theme.list", "print.pages"] {
            assert!(authorize_window_command("print", id).is_ok(), "{id}");
        }
        for id in [
            "file.open",
            "file.save",
            "file.printStart",
            "file.exportPdfStart",
            "print.release",
            "editor.insertText",
            "plugin.list",
        ] {
            assert!(authorize_window_command("print", id).is_err(), "{id}");
        }
        // Markdown content names its document; unknown kinds are refused.
        let markdown: PrintContent = serde_json::from_value(serde_json::json!({
            "kind": "markdown", "documentId": "doc-1", "title": "notes.md"
        }))
        .expect("markdown content");
        assert!(matches!(markdown, PrintContent::Markdown { .. }));
        assert!(
            serde_json::from_value::<PrintContent>(serde_json::json!({ "kind": "file" })).is_err()
        );
    }

    #[test]
    fn find_windows_may_search_but_not_open_save_or_edit_directly() {
        for id in [
            "search.findAll",
            "search.validatePattern",
            "search.countStart",
            "search.countProgress",
            "search.countCancel",
            "search.replaceAll",
            "search.findInFiles",
            "search.previewReplaceInFiles",
            "search.replaceInFiles",
            "theme.list",
        ] {
            assert!(authorize_window_command("find", id).is_ok(), "{id}");
        }
        for id in [
            "file.open",
            "file.save",
            "editor.insertText",
            "editor.addBookmarks",
            "terminal.create",
            "customization.saveChecked",
        ] {
            assert!(authorize_window_command("find", id).is_err(), "{id}");
        }
        assert!(authorize_window_command("find-results", "editor.addBookmarks").is_ok());
        for id in ["search.replaceAll", "file.open", "editor.replace"] {
            assert!(
                authorize_window_command("find-results", id).is_err(),
                "{id}"
            );
        }
    }

    #[test]
    fn remembered_settings_frame_is_restored_only_within_visible_bounds() {
        let frame = |x, y, width, height| WindowFrame {
            x,
            y,
            width,
            height,
        };
        let displays = [frame(0, 0, 1920, 1080), frame(1920, 0, 1280, 1024)];
        // On the second display.
        assert_eq!(
            visible_frame(frame(2000, 100, 900, 600), &displays),
            Some(frame(2000, 100, 900, 600))
        );
        // The display it was on is gone.
        assert_eq!(
            visible_frame(frame(4000, 100, 900, 600), &displays[..1]),
            None
        );
        // Mostly off-screen is not usable.
        assert_eq!(visible_frame(frame(-850, 100, 900, 600), &displays), None);
        // A window larger than the remaining display shrinks to fit it.
        assert_eq!(
            visible_frame(frame(100, 50, 3000, 2000), &displays[..1]),
            Some(frame(100, 50, 1920, 1080))
        );
        // A corrupt tiny size is raised to the minimum.
        assert_eq!(
            visible_frame(frame(10, 10, 1, 1), &displays),
            Some(frame(10, 10, 640, 420))
        );
    }

    #[test]
    fn native_menu_rejects_edge_separators() {
        assert!(validate_native_menu(&[NativeMenuEntry::Separator]).is_err());
    }
}
