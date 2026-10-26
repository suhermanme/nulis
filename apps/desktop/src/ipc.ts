import { invoke } from "@tauri-apps/api/core";
import {
  IPC_PROTOCOL_VERSION,
  type ApplicationInfo,
  type CommandDescriptor,
  type CommandFailure,
  type DocumentMapSnapshot,
  type EditorPreferences,
  type MarkdownPreviewSource,
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

const FAILURE_PREFIX: Record<CommandFailure["kind"], string> = {
  invalidId: "Invalid command", duplicate: "Command already registered", unknown: "Unknown command",
  unavailable: "Command unavailable", failed: "",
};

/** A failed command's error as an `Error` with the core's own message, so callers can show it. */
export function commandError(error: unknown): unknown {
  if (error instanceof Error || typeof error === "string") return typeof error === "string" ? new Error(error) : error;
  if (error && typeof error === "object" && "kind" in error && "detail" in error) {
    const { kind, detail } = error as CommandFailure;
    const prefix = FAILURE_PREFIX[kind] ?? "";
    return new Error(prefix ? `${prefix}: ${detail}` : String(detail));
  }
  return error;
}

export function executeCommand<T = unknown>(request: ExecuteCommandRequest): Promise<T> {
  return invoke<T>("execute_command", { request }).catch((error: unknown) => { throw commandError(error); });
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
/** A place in Nulis Help: a section and a heading in it, by their slugs. */
export interface HelpTopic {
  section: string;
  heading?: string;
}

/** Opens Nulis Help, at `topic` when given. */
export function openHelpWindow(topic?: HelpTopic): Promise<void> {
  return invoke<void>("open_help_window", { topic: topic ? { section: topic.section, heading: topic.heading ?? "" } : null });
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
/** Names and enables the native Tools items: each label ("JSON") the focused document has. */
export function setToolsMenu(tools: { validate?: string; beautify?: string; minify?: string }): Promise<void> {
  return invoke<void>("set_tools_menu", { tools });
}

/** Opens the separate Find and Replace window, or brings it forward. */
/** Tells the shell this secondary window has drawn its themed page, so it may be shown. */
export function windowReady(): Promise<void> {
  return invoke<void>("window_ready");
}

export function openFindWindow(): Promise<void> {
  return invoke<void>("open_find_window");
}

/** What Print Preview shows: the pages laid out by `file.printStart`, or a Markdown preview. */
/** `remoteImages`: the editor window allows the preview to fetch `https:` images for this document. */
export type PrintContent = { kind: "pages" } | { kind: "markdown"; documentId: string; title: string; remoteImages: boolean };

/** Opens the Print Preview window on `content`, or brings it forward and shows `content`. */
export function openPrintWindow(content: PrintContent): Promise<void> {
  return invoke<void>("open_print_window", { content });
}

/** What the calling Print Preview window should show. */
export type PrintWindowContent =
  | { kind: "pages" }
  | { kind: "markdown"; title: string; source: MarkdownPreviewSource; remoteImages: boolean }
  | { kind: "empty" };

export function printWindowContent(): Promise<PrintWindowContent> {
  return invoke<PrintWindowContent>("print_window_content");
}

/** An image of the Markdown document being printed, as a data URL. */
export function printWindowMarkdownImage(relativePath: string): Promise<string> {
  return invoke<string>("print_window_markdown_image", { relativePath });
}

/** Portrait paper size in millimetres, and whether pages are landscape. */
export interface PrintPaper { widthMm: number; heightMm: number; landscape: boolean }

/** Opens the system print dialog for the calling Print Preview window, on this paper. */
export function printThisWindow(paper: PrintPaper): Promise<void> {
  return invoke<void>("print_window_print", { paper });
}

/** Opens the separate list of matching lines, or brings it forward. */
export function openResultsWindow(): Promise<void> {
  return invoke<void>("open_results_window");
}

/** Closes the calling Find or results window. */
export function closeFindWindow(): Promise<void> {
  return invoke<void>("close_find_window");
}

/** Resizes the Find window so its page shows `content` points; it now shows `viewport`. The width is kept. */
export function fitFindWindow(content: number, viewport: number): Promise<void> {
  return invoke<void>("fit_find_window", { content, viewport });
}

/** Sends a message from the Find or results window to the editor window. */
export function sendFindMessage(message: { kind: string } & Record<string, unknown>): Promise<void> {
  return invoke<void>("find_message", { message });
}
