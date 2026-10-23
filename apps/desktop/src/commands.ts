import type { LineEnding } from "./contracts";

export const LINE_ENDING_LABELS: Record<LineEnding, string> = { lf: "LF", crlf: "CRLF", cr: "CR" };

/** Editing and formatting commands carried out by the focused editor. */
export const EDITOR_ACTIONS = {
  "edit.selectLines": "selectLines",
  "edit.duplicateLines": "duplicateLines",
  "edit.deleteLines": "deleteLines",
  "edit.moveLinesUp": "moveLinesUp",
  "edit.moveLinesDown": "moveLinesDown",
  "edit.insertLineBelow": "insertLineBelow",
  "edit.triggerSuggest": "triggerSuggest",
  "format.uppercase": "uppercase",
  "format.lowercase": "lowercase",
  "format.capitalize": "capitalize",
  "format.invertCase": "invertCase",
  "format.trimSelection": "trimSelection",
  "format.indent": "indent",
  "format.outdent": "outdent",
  "format.toggleComment": "toggleComment",
  "format.trimTrailingSpaces": "trimTrailingSpaces",
  "format.tabsToSpaces": "tabsToSpaces",
  "format.spacesToTabs": "spacesToTabs",
  "format.sortAscending": "sortAscending",
  "format.sortDescending": "sortDescending",
  "format.sortUnique": "sortUnique",
  "format.base64Encode": "base64Encode",
  "format.base64Decode": "base64Decode",
  "format.insertDateTime": "insertDateTime",
  "format.lineEndingsLf": "lineEndingsLf",
  "format.lineEndingsCrlf": "lineEndingsCrlf",
  "format.lineEndingsCr": "lineEndingsCr",
  "format.insertNumbers": "insertNumbers",
  "format.sumNumbers": "sumNumbers",
  "format.justifyLeft": "justifyLeft",
  "format.justifyCenter": "justifyCenter",
  "format.justifyRight": "justifyRight",
  "format.alignColumns": "alignColumns",
  "format.unalignColumns": "unalignColumns",
  "edit.copyAppend": "copyAppend",
  "edit.cutAppend": "cutAppend",
  "edit.pasteCopy": "pasteCopy",
  "search.findNext": "findNext",
  "search.findPrevious": "findPrevious",
  "search.goToLine": "goToLine",
  "search.matchBrace": "matchBrace",
  "search.nextChange": "nextChange",
  "search.previousChange": "previousChange",
  "search.navigateBack": "navigateBack",
  "search.navigateForward": "navigateForward",
  "search.nextBookmark": "nextBookmark",
  "search.previousBookmark": "previousBookmark",
  "search.toggleBookmark": "toggleBookmark",
  "search.listBookmarks": "listBookmarks",
  "search.clearBookmarks": "clearBookmarks",
  "search.deleteBookmarkedLines": "deleteBookmarkedLines",
} as const;

/** Commands that open the Find and Replace window on a tab. */
export const FIND_WINDOW_COMMANDS = {
  "search.openFind": "find",
  "search.openReplace": "replace",
  "search.openFindInFiles": "findInFiles",
  "search.openReplaceInFiles": "replaceInFiles",
} as const;
export type EditorActionName = typeof EDITOR_ACTIONS[keyof typeof EDITOR_ACTIONS];
const EDITOR_ACTION_IDS = Object.keys(EDITOR_ACTIONS) as Array<keyof typeof EDITOR_ACTIONS>;

/**
 * Presentation-only commands the core names and validates and the desktop
 * applies. Mirrors `WORKBENCH_ACTIONS` in nulis-core.
 */
export const WORKBENCH_ACTIONS = [
  "workbench.toggleSidebar", "workbench.toggleSplit", "workbench.toggleEditorMode",
  "workbench.splitRight", "workbench.splitDown", "workbench.closeFile", "workbench.closeTab", "workbench.openSettings",
  "workbench.openHelp",
  "workbench.nextTab", "workbench.previousTab", "workbench.moveTabLeft", "workbench.moveTabRight",
  "view.toggleLineChangeIndicators",
  "file.saveAll", "file.closeAll", "file.openRecent", "file.clearRecent", "file.rename", "file.revealInPlaces",
  "file.quickOpen", "file.print", "file.pageSetup", "file.exportPdf", "file.exportSelectionPdf", "edit.pasteFromHistory",
  "view.toggleWordWrap", "view.toggleWhitespace", "view.toggleLineEndings", "view.toggleLineNumbers",
  "view.toggleColumnMarker", "view.zoomIn", "view.zoomOut", "view.zoomReset", "view.toggleAlwaysOnTop",
  "workspace.rename", "workspace.groupRight", "workspace.groupDown", "workspace.ungroup",
  "workspace.popOutPane", "workspace.closePane", "workspace.focusNextPane", "workspace.focusPreviousPane",
  "workspace.growPane", "workspace.shrinkPane",
  ...EDITOR_ACTION_IDS,
  ...(Object.keys(FIND_WINDOW_COMMANDS) as Array<keyof typeof FIND_WINDOW_COMMANDS>),
] as const;

/** JSON, XML, YAML, and plist tools; offered only while such a document is focused. */
export const TOOL_COMMANDS = ["tools.validate", "tools.beautify", "tools.minify"] as const;

const FILE_COMMANDS = [
  "app.about", "file.new", "file.open", "file.save", "file.saveAs", "file.reload",
  "workspace.open", "workspace.close", "editor.undo", "editor.redo", "editor.toggleReadOnly",
] as const;

/** Zero-argument commands offered in the command palette. */
export const PALETTE_COMMANDS: ReadonlySet<string> = new Set([...FILE_COMMANDS, ...TOOL_COMMANDS, ...WORKBENCH_ACTIONS]);

/** Commands a shortcut, File-menu entry, or toolbar button may reference. */
export const CUSTOMIZABLE_COMMANDS: ReadonlySet<string> = new Set([
  ...FILE_COMMANDS, ...TOOL_COMMANDS, "workbench.commandPalette", ...WORKBENCH_ACTIONS,
]);

export function shortcutLabel(chord: string | undefined): string {
  if (!chord) return "";
  const parts = chord.split("+");
  const key = parts.at(-1) ?? "";
  const apple = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);
  if (apple) return `${parts.includes("Shift") ? "⇧" : ""}${parts.includes("Alt") ? "⌥" : ""}${parts.includes("Primary") ? "⌘" : ""}${key}`;
  return `${parts.includes("Primary") ? "Ctrl+" : ""}${parts.includes("Shift") ? "Shift+" : ""}${parts.includes("Alt") ? "Alt+" : ""}${key}`;
}
