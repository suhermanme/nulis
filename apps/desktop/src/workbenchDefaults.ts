/** What an empty workbench starts from, and small path helpers the workbench shares. */
import type { CustomizationDocument, WorkbenchSession } from "./contracts";
import { removeDocument, type WorkTab } from "./layout";
import { loadEditorPreferences } from "./preferences";

/** Closes every view of a document, dropping tabs that end up empty. */
export function dropDocument(tabs: WorkTab[], documentId: string): WorkTab[] {
  return tabs.flatMap((tab) => removeDocument(tab, documentId) ?? []);
}

export function defaultCustomization(): CustomizationDocument {
  return {
    schemaVersion: 1,
    appearance: loadEditorPreferences(),
    keybindings: [
      { key: "Primary+N", command: "file.new" }, { key: "Primary+O", command: "file.open" },
      { key: "Primary+S", command: "file.save" }, { key: "Primary+Shift+S", command: "file.saveAs" },
      { key: "Primary+W", command: "workbench.closeFile" },
      { key: "Primary+K", command: "workbench.commandPalette" },
      { key: "Primary+Shift+L", command: "editor.toggleReadOnly" },
      { key: "Primary+Z", command: "editor.undo" }, { key: "Primary+Shift+Z", command: "editor.redo" },
    ],
    menu: [
      { kind: "command", command: "file.new" }, { kind: "command", command: "file.open" },
      { kind: "separator" }, { kind: "command", command: "file.save" },
      { kind: "command", command: "file.saveAs" },
      { kind: "separator" }, { kind: "command", command: "workbench.closeFile" },
    ],
    toolbar: ["file.new", "file.open", "file.save", "workbench.closeFile", "editor.undo", "editor.redo"],
  };
}

export function defaultSession(): WorkbenchSession {
  return {
    schemaVersion: 2,
    workspaceRoot: null,
    documents: [],
    layouts: [],
    activeLayout: null,
    sidebarVisible: true,
    sidebarWidth: 260,
    activePanel: "files",
    viewModes: {},
  };
}

/** The core migrates older schemas; anything else falls back to an empty session. */
export function usableSession(value: WorkbenchSession | null | undefined): WorkbenchSession {
  return value && value.schemaVersion === 2 && Array.isArray(value.layouts)
    && Array.isArray(value.documents) ? value : defaultSession();
}

/** The last part of a path. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

export function folderName(path: string): string {
  const name = baseName(path);
  return path.slice(0, Math.max(0, path.length - name.length)).replace(/[\\/]$/, "") || path;
}

export function comparablePath(path: string): string {
  return path.replaceAll("\\", "/").replace(/\/+$/, "");
}
