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
  "workspace.rename", "workspace.groupRight", "workspace.groupDown", "workspace.ungroup",
  "workspace.popOutPane", "workspace.closePane", "workspace.focusNextPane", "workspace.focusPreviousPane",
  "workspace.growPane", "workspace.shrinkPane",
] as const;

/** JSON and XML tools; offered only while such a document is focused. */
export const TOOL_COMMANDS = ["tools.validate", "tools.beautify", "tools.minify"] as const;

const FILE_COMMANDS = [
  "app.about", "file.new", "file.open", "file.save", "file.saveAs",
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
