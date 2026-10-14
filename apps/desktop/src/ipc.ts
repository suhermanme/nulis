import { invoke } from "@tauri-apps/api/core";
import {
  IPC_PROTOCOL_VERSION,
  type ApplicationInfo,
  type CommandDescriptor,
  type DocumentMapSnapshot,
  type EditorPreferences,
  type MarkdownPreviewSource,
  type ToolKind,
  type ExecuteCommandRequest,
  type HexViewportRequest,
  type HexViewportSnapshot,
  type ViewportRequest,
  type ViewportAtRequest,
  type ViewportLineRequest,
  type ViewportSnapshot,
} from "./contracts";

export async function getApplicationInfo(): Promise<ApplicationInfo> {
  const info = await invoke<ApplicationInfo>("application_info");
  if (info.ipcProtocolVersion !== IPC_PROTOCOL_VERSION) {
    throw new Error(
      `Unsupported native protocol ${info.ipcProtocolVersion}; UI expects ${IPC_PROTOCOL_VERSION}`,
    );
  }
  return info;
}

export function listCommands(): Promise<CommandDescriptor[]> {
  return invoke<CommandDescriptor[]>("list_commands");
}

export function executeCommand<T = unknown>(request: ExecuteCommandRequest): Promise<T> {
  return invoke<T>("execute_command", { request });
}

export interface NativeMenuEntry {
  kind: "command" | "separator";
  command?: string;
  title?: string;
  accelerator?: string;
}

export function configureNativeMenu(entries: NativeMenuEntry[]): Promise<void> {
  return invoke<void>("configure_native_menu", { entries });
}

export function closeWindowAfterSave(): Promise<void> {
  return invoke<void>("close_window_after_save");
}

export function getViewport(request: ViewportRequest): Promise<ViewportSnapshot> {
  return invoke<ViewportSnapshot>("editor_viewport", { request });
}

export function getViewportAt(request: ViewportAtRequest): Promise<ViewportSnapshot> {
  return invoke<ViewportSnapshot>("editor_viewport_at", { request });
}

export function getDocumentMap(documentId: string): Promise<DocumentMapSnapshot> {
  return invoke<DocumentMapSnapshot>("editor_document_map", { documentId });
}

export function getMarkdownPreviewSource(documentId: string): Promise<MarkdownPreviewSource> {
  return invoke<MarkdownPreviewSource>("editor_markdown_preview_source", { documentId });
}

export function getMarkdownPreviewImage(documentId: string, relativePath: string): Promise<string> {
  return invoke<string>("editor_markdown_preview_image", { documentId, relativePath });
}

export function getViewportLine(request: ViewportLineRequest): Promise<ViewportSnapshot> {
  return invoke<ViewportSnapshot>("editor_viewport_line", { request });
}

export function getHexViewport(request: HexViewportRequest): Promise<HexViewportSnapshot> {
  return invoke<HexViewportSnapshot>("editor_hex_viewport", { request });
}

/** Files the system asked Nulis to open at launch; later ones arrive as an event. */
export function takeLaunchPaths(): Promise<string[]> {
  return invoke<string[]>("take_launch_paths");
}

/** Opens the singleton Help window, or brings the existing one forward. */
export function openHelpWindow(): Promise<void> {
  return invoke<void>("open_help_window");
}

export function closeHelpWindow(): Promise<void> {
  return invoke<void>("close_help_window");
}

/** Opens the singleton Settings window, or brings the existing one forward. */
export function openSettingsWindow(): Promise<void> {
  return invoke<void>("open_settings_window");
}

export function closeSettingsWindow(): Promise<void> {
  return invoke<void>("close_settings_window");
}

/** Relays an unapplied appearance preview to the editor window; null reverts it. */
export function previewAppearance(appearance: EditorPreferences | null): Promise<void> {
  return invoke<void>("settings_preview", { appearance });
}

export function notifySettingsChanged(): Promise<void> {
  return invoke<void>("settings_changed");
}

export function requestMainAction(action: string): Promise<void> {
  return invoke<void>("settings_main_action", { action });
}

/** Names and enables the native Tools menu for the focused document. */
export function setToolsMenu(kind: ToolKind | null): Promise<void> {
  return invoke<void>("set_tools_menu", { kind });
}
