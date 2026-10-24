import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { open, save } from "@tauri-apps/plugin-dialog";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { emitTo, listen } from "@tauri-apps/api/event";
import { App } from "./App";
import type { CustomizationDocument, ExecuteCommandRequest, WorkbenchSession } from "./contracts";
import {
  closeWindowAfterSave, executeCommand, listCommands, openFindWindow, openHelpWindow, openPrintWindow, openResultsWindow, openSettingsWindow,
  setToolsMenu, takeLaunchPaths,
} from "./ipc";
import { DEFAULT_EDITOR_PREFERENCES } from "./preferences";

const DEFAULT_SESSION: WorkbenchSession = {
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

vi.mock("./ipc", () => ({
  getApplicationInfo: vi.fn().mockResolvedValue({
    productName: "Nulis",
    applicationId: "com.nulis.editor",
    version: "0.1.0",
    ipcProtocolVersion: 26,
  }),
  listCommands: vi.fn().mockResolvedValue([
    { id: "app.about", title: "About Nulis", category: "Application" },
    { id: "file.new", title: "New Text File", category: "File" },
    { id: "file.open", title: "Open File…", category: "File" },
    { id: "file.save", title: "Save", category: "File" },
    { id: "file.saveAs", title: "Save As…", category: "File" },
    { id: "workbench.toggleSidebar", title: "Toggle Primary Side Bar", category: "View" },
  ]),
  executeCommand: vi.fn(),
  closeWindowAfterSave: vi.fn().mockResolvedValue(undefined),
  configureNativeMenu: vi.fn().mockResolvedValue(undefined),
  openSettingsWindow: vi.fn().mockResolvedValue(undefined),
  openHelpWindow: vi.fn().mockResolvedValue(undefined),
  openFindWindow: vi.fn().mockResolvedValue(undefined),
  openResultsWindow: vi.fn().mockResolvedValue(undefined),
  openPrintWindow: vi.fn().mockResolvedValue(undefined),
  takeLaunchPaths: vi.fn().mockResolvedValue([]),
  setToolsMenu: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: vi.fn(() => false) }));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: vi.fn(() => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => undefined) })),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
  emitTo: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: vi.fn(() => ({
  onCloseRequested: vi.fn().mockResolvedValue(() => undefined), destroy: vi.fn().mockResolvedValue(undefined),
})) }));

vi.mock("./EditorViewport", () => ({
  EditorViewport: ({ document, showLineChanges, searchRequest, onCaret, onToggleReadOnly, onOpenFind, editorAction,
    wordWrap, showWhitespace, showLineEndings, showLineNumbers, columnMarker, lineHeight, onClipboardText, insertRequest }: {
    document: { name: string; readOnly?: boolean }; showLineChanges?: boolean; searchRequest?: object;
    editorAction?: { action: string }; wordWrap?: boolean; showWhitespace?: boolean; showLineEndings?: boolean;
    showLineNumbers?: boolean; columnMarker?: number; lineHeight?: number;
    onClipboardText?: (text: string) => void; insertRequest?: { text: string };
    onCaret?: (position: object | null) => void; onToggleReadOnly?: () => void;
    onOpenFind?: (tab: "find" | "replace", seed?: string) => void;
  }) => (
    <div>
      <div data-line-changes={String(showLineChanges)} data-search={JSON.stringify(searchRequest ?? null)}
        data-action={editorAction?.action ?? ""}
        data-view={JSON.stringify({ wordWrap, showWhitespace, showLineEndings, showLineNumbers, columnMarker, lineHeight })}
        data-inserted={insertRequest?.text ?? ""}>
        {document.name} editor</div>
      <button type="button" onClick={() => onCaret?.({ line: "1234", column: 7, carets: 1, selectedBytes: "0", codePoint: 0xe9 })}>Move caret in {document.name}</button>
      <button type="button" onClick={() => onToggleReadOnly?.()}>Editor mode of {document.name}: {document.readOnly ? "read-only" : "edit"}</button>
      <button type="button" onClick={() => onClipboardText?.(`copied from ${document.name}\nsecond line`)}>Copy in {document.name}</button>
      <textarea aria-label={`Keys in ${document.name}`} onKeyDown={(event) => {
        // Like the real editor: it undoes on Primary Z itself and marks the key handled.
        if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") event.preventDefault();
      }} />
      {onOpenFind ? <>
        <button type="button" onClick={() => onOpenFind("find", "seeded")}>Find in {document.name}</button>
        <button type="button" onClick={() => onOpenFind("replace")}>Replace in {document.name}</button>
      </> : null}
    </div>
  ),
}));

vi.mock("./HexViewport", () => ({
  HexViewport: ({ document }: { document: { name: string } }) => <div>{document.name} hexadecimal editor</div>,
}));

let responses: Record<string, unknown[]>;
let storedCustomization: { revision: number; document: CustomizationDocument };

class PointerEventStub extends MouseEvent {
  readonly pointerId: number;

  constructor(type: string, eventInit: PointerEventInit = {}) {
    super(type, eventInit);
    this.pointerId = eventInit.pointerId ?? 0;
  }
}

function saveRequests(): WorkbenchSession[] {
  return vi.mocked(executeCommand).mock.calls
    .filter(([request]) => request.id === "session.save")
    .map(([request]) => request.arguments as WorkbenchSession);
}

function tabNames(): Array<string | null> {
  return within(screen.getByRole("tablist", { name: "Open documents" })).queryAllByRole("tab").map((tab) => tab.textContent);
}

function panes(): HTMLElement[] {
  return screen.getAllByRole("group", { name: /^Pane \d+ of \d+|^Editor$/ });
}

function paneNames(): Array<string | null> {
  return panes().map((pane) => pane.getAttribute("aria-label"));
}

/** Drags a strip tab to a point inside a pane of the active tab. */
function dragTabTo(name: string, pane: HTMLElement, x: number, y: number, release = true) {
  pane.getBoundingClientRect = () => ({ left: 0, top: 0, width: 800, height: 600 }) as DOMRect;
  // With two tabs of the same document, the later one is dragged.
  fireEvent.pointerDown(screen.getAllByRole("tab", { name }).at(-1)!.parentElement!, { button: 0, clientX: 40, clientY: 10 });
  fireEvent.pointerMove(pane.querySelector(".editor-pane")!, { clientX: x, clientY: y });
  if (release) fireEvent.pointerUp(window);
}

async function openDocuments(...names: string[]) {
  responses["file.new"] = names.map((name, index) => ({
    documentId: `document-${index + 1}`, name, byteLen: "0", revision: "0", encoding: "utf8",
    sourcePath: `/tmp/${name}`,
  }));
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText(`${names[0]} editor`);
  for (const name of names.slice(1)) {
    fireEvent.click(screen.getByRole("button", { name: "New tab" }));
    await screen.findByText(`${name} editor`);
  }
}

/** Lets a command dispatched through the (mocked) core settle. */
async function settle() {
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
}

async function chooseMenu(menu: string, item: string | RegExp) {
  fireEvent.click(screen.getByRole("button", { name: `Open ${menu} menu` }));
  const entries = [...screen.getByRole("menu", { name: menu }).querySelectorAll<HTMLElement>("button")];
  const entry = entries.find((button) => {
    const label = button.querySelector("span")?.textContent ?? "";
    return typeof item === "string" ? label === item : item.test(label);
  });
  if (!entry) throw new Error(`No ${menu} menu entry matches ${String(item)}`);
  fireEvent.click(entry);
  await settle();
}

beforeEach(() => {
  responses = {};
  vi.stubGlobal("PointerEvent", PointerEventStub);
  storedCustomization = {
    revision: 0,
    document: {
      schemaVersion: 1,
      appearance: DEFAULT_EDITOR_PREFERENCES,
      keybindings: [{ key: "Primary+S", command: "file.save" }],
      menu: [{ kind: "command", command: "file.save" }],
      toolbar: ["file.new", "file.save"],
    },
  };
  vi.mocked(isTauri).mockReturnValue(false);
  vi.mocked(getCurrentWindow).mockImplementation(() => ({
    onCloseRequested: vi.fn().mockResolvedValue(() => undefined), destroy: vi.fn().mockResolvedValue(undefined),
  }) as unknown as ReturnType<typeof getCurrentWindow>);
  vi.mocked(listen).mockReset().mockResolvedValue(() => undefined);
  window.localStorage.clear();
  vi.mocked(open).mockReset().mockResolvedValue(null);
  vi.mocked(save).mockReset().mockResolvedValue(null);
  vi.mocked(executeCommand).mockReset().mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "session.load") return DEFAULT_SESSION as never;
    if (request.id === "session.save") return null as never;
    if (request.id === "plugin.activateStartup") return [] as never;
    if (request.id === "customization.state") return storedCustomization as never;
    if (request.id === "customization.saveChecked") {
      const { expectedRevision, document } = request.arguments as { expectedRevision: number; document: CustomizationDocument };
      if (expectedRevision !== storedCustomization.revision) {
        throw new Error(`customization changed elsewhere (revision ${expectedRevision} is now ${storedCustomization.revision})`);
      }
      storedCustomization = { revision: expectedRevision + 1, document };
      return storedCustomization as never;
    }
    const queue = responses[request.id];
    if (!queue?.length) return null as never;
    return queue.shift() as never;
  });
  vi.mocked(closeWindowAfterSave).mockReset().mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

test("renders the independently branded workbench", async () => {
  render(<App />);
  expect(screen.getByRole("heading", { name: /write, inspect/i })).toBeInTheDocument();
  expect(await screen.findByText("Nulis 0.1.0")).toBeInTheDocument();
});

test("activates approved startup plugins and runs contributed palette commands", async () => {
  vi.mocked(listCommands).mockResolvedValueOnce([
    { id: "app.about", title: "About Nulis", category: "Application" },
    { id: "nulis.hello-example.hello", title: "Hello from Plugin", category: "Plugin · Examples" },
  ]);
  responses["nulis.hello-example.hello"] = [{
    pluginId: "nulis.hello-example", command: "nulis.hello-example.hello",
    result: null, messages: ["Hello from an isolated plugin"],
  }];

  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "plugin.activateStartup", context: { windowId: "main" }, arguments: {},
  }));
  fireEvent.click(screen.getByRole("button", { name: "Open command palette" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Search commands" }), {
    target: { value: "Hello from Plugin" },
  });
  fireEvent.click(await screen.findByRole("option", { name: /Hello from Plugin/ }));

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "nulis.hello-example.hello",
    context: { windowId: "main", documentId: undefined }, arguments: {},
  }));
  expect(await screen.findByRole("status")).toHaveTextContent("Hello from an isolated plugin");
});

test("opens a native picker selection through the file command", async () => {
  vi.mocked(open).mockResolvedValueOnce("/tmp/example.txt");
  responses["file.open"] = [{
    documentId: "document-1", name: "example.txt", byteLen: "12", revision: "0",
    encoding: "utf8", sourcePath: "/tmp/example.txt",
  }];

  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "Open a file" }));

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "file.open", context: { windowId: "main" }, arguments: { path: "/tmp/example.txt" },
  }));
  expect(await screen.findByText("example.txt editor")).toBeInTheDocument();
});

test("opens native dropped files through the file command", async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  const onDragDropEvent = vi.fn().mockResolvedValue(() => undefined);
  vi.mocked(getCurrentWebview).mockReturnValue({ onDragDropEvent } as never);
  responses["file.open"] = [{
    documentId: "dropped-document", name: "example.py", byteLen: "12", revision: "0",
    encoding: "utf8", sourcePath: "/tmp/example.py",
  }];

  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  await waitFor(() => expect(onDragDropEvent).toHaveBeenCalled());
  const handler = onDragDropEvent.mock.calls[0]?.[0];
  await act(async () => handler({ payload: { type: "enter", paths: ["/tmp/example.py"] } }));
  expect(screen.getByText("Drop files to open")).toBeInTheDocument();
  await act(async () => handler({ payload: { type: "drop", paths: ["/tmp/example.py"] } }));

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "file.open", context: { windowId: "main" }, arguments: { path: "/tmp/example.py" },
  }));
  expect(await screen.findByText("example.py editor")).toBeInTheDocument();
  expect(screen.queryByText("Drop files to open")).not.toBeInTheDocument();
});

test("saves an existing file from the visible File menu", async () => {
  vi.mocked(open).mockResolvedValueOnce("/tmp/example.txt");
  const opened = {
    documentId: "document-1", name: "example.txt", byteLen: "12", revision: "3",
    encoding: "utf8", sourcePath: "/tmp/example.txt",
  };
  responses["file.open"] = [opened];
  responses["file.save"] = [opened];

  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "Open a file" }));
  await screen.findByText("example.txt editor");
  fireEvent.click(screen.getByRole("button", { name: "Open File menu" }));
  fireEvent.click(screen.getByRole("menuitem", { name: /^Save (?:⌘|Ctrl\+)S$/ }));

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "file.save", context: { windowId: "main", documentId: "document-1" },
    arguments: { expectedRevision: "3" },
  }));
  expect(save).not.toHaveBeenCalled();
});

test("uses the native File menu in the desktop shell without rendering a duplicate", async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  responses["file.new"] = [{
    documentId: "document-1", name: "Untitled 1", byteLen: "0", revision: "0", encoding: "utf8",
  }];

  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  expect(screen.queryByRole("button", { name: "Open File menu" })).not.toBeInTheDocument();
  await waitFor(() => expect(listen).toHaveBeenCalledWith(
    "nulis://menu-command", expect.any(Function),
  ));

  const listener = vi.mocked(listen).mock.calls[0]?.[1];
  listener?.({ event: "nulis://menu-command", id: 1, payload: "file.new" });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "file.new", context: { windowId: "main" },
    arguments: { name: "Untitled 1", initialText: "" },
  }));
});

test("dispatches a persisted portable shortcut through the command boundary", async () => {
  responses["customization.get"] = [{
    schemaVersion: 1,
    appearance: {
      theme: "nulis.ember-dark",
      fontFamily: 'ui-monospace, "SFMono-Regular", Consolas, "Liberation Mono", monospace',
      fontSize: 13,
      lineHeight: 1.7,
      fontLigatures: true,
    },
    keybindings: [{ key: "Primary+Alt+1", command: "file.new" }],
    menu: [{ kind: "command", command: "file.new" }],
    toolbar: [],
  }];
  responses["file.new"] = [{
    documentId: "document-1", name: "Shortcut", byteLen: "0", revision: "0", encoding: "utf8",
  }];

  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.keyDown(window, { key: "1", metaKey: true, altKey: true });
  expect(await screen.findByText("Shortcut editor")).toBeInTheDocument();
});

test("uses the native destination picker when saving an untitled file", async () => {
  responses["file.new"] = [{
    documentId: "document-1", name: "Untitled 1", byteLen: "0", revision: "0", encoding: "utf8",
  }];
  responses["file.saveAs"] = [{
    documentId: "document-1", name: "note.txt", byteLen: "0", revision: "0",
    encoding: "utf8", sourcePath: "/tmp/note.txt",
  }];
  vi.mocked(save).mockResolvedValueOnce("/tmp/note.txt");

  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("Untitled 1 editor");
  fireEvent.click(screen.getByRole("button", { name: "Open File menu" }));
  fireEvent.click(screen.getByRole("menuitem", { name: /^Save As/ }));

  await waitFor(() => expect(save).toHaveBeenCalledWith({
    title: "Save As", defaultPath: "Untitled 1",
  }));
  expect(executeCommand).toHaveBeenCalledWith({
    id: "file.saveAs", context: { windowId: "main", documentId: "document-1" },
    arguments: { expectedRevision: "0", path: "/tmp/note.txt" },
  });
});

function exportReport(overrides: Record<string, unknown>) {
  return {
    progress: { phase: "writing", done: "10", total: "10", pagesWritten: "0", estimatedPages: "0" },
    done: true, cancelled: false, ...overrides,
  };
}

async function startExportFromMenu(name: string) {
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText(`${name} editor`);
  fireEvent.click(screen.getByRole("button", { name: "Open File menu" }));
  fireEvent.click(screen.getByRole("menuitem", { name: /^Export to PDF/ }));
}

test("Export to PDF suggests a .pdf name and reports the pages written", async () => {
  responses["file.new"] = [{
    documentId: "document-1", name: "notes.md", byteLen: "5", revision: "3", encoding: "utf8", dirty: true,
  }];
  responses["file.exportPdfStart"] = [{ jobId: 7 }];
  responses["file.exportPdfProgress"] = [exportReport({
    result: { path: "/tmp/notes.pdf", pages: "12", missingGlyphs: "2", bytes: "300000" },
  })];
  vi.mocked(save).mockResolvedValueOnce("/tmp/notes.pdf");

  await startExportFromMenu("notes.md");

  await waitFor(() => expect(save).toHaveBeenCalledWith({
    title: "Export to PDF", defaultPath: "notes.pdf", filters: [{ name: "PDF", extensions: ["pdf"] }],
  }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "file.exportPdfStart", context: { windowId: "main", documentId: "document-1" },
    arguments: {
      expectedRevision: "3", path: "/tmp/notes.pdf", date: expect.any(String) as string, confirmed: false, scope: "document",
      setup: expect.objectContaining({ fontSize: 9, lineNumbers: true }) as unknown, colors: { kind: "print" },
    },
  }));
  expect(await screen.findByText(/Exported 12 pages to notes\.pdf; 2 characters the print font lacks are left blank/))
    .toBeInTheDocument();
  expect(executeCommand).toHaveBeenCalledWith({
    id: "file.exportPdfProgress", context: { windowId: "main" }, arguments: { jobId: 7 },
  });
});

test("a very large export asks first and runs again once confirmed", async () => {
  responses["file.new"] = [{
    documentId: "document-1", name: "server.log", byteLen: "5", revision: "0", encoding: "utf8",
  }];
  responses["file.exportPdfStart"] = [{ jobId: 1 }, { jobId: 2 }];
  responses["file.exportPdfProgress"] = [
    exportReport({ needsConfirmation: "3100" }),
    exportReport({ result: { path: "/tmp/server.pdf", pages: "3150", missingGlyphs: "0", bytes: "9000000" } }),
  ];
  vi.mocked(save).mockResolvedValueOnce("/tmp/server.pdf");

  await startExportFromMenu("server.log");

  const dialog = await screen.findByRole("dialog", { name: "Export 3,100 pages?" });
  expect(dialog).toHaveTextContent("at least 3,100 pages");
  fireEvent.click(within(dialog).getByRole("button", { name: "Export" }));
  expect(await screen.findByText("Exported 3,150 pages to server.pdf")).toBeInTheDocument();
  const starts = vi.mocked(executeCommand).mock.calls
    .filter(([request]) => request.id === "file.exportPdfStart")
    .map(([request]) => (request.arguments as { confirmed: boolean }).confirmed);
  expect(starts).toEqual([false, true]);
});

async function chooseFileMenuItem(name: RegExp) {
  fireEvent.click(screen.getByRole("button", { name: "Open File menu" }));
  fireEvent.click(screen.getByRole("menuitem", { name }));
}

async function openNewDocument(name: string, revision = "0") {
  window.localStorage.removeItem("nulis.printSettings.v1");
  responses["file.new"] = [{ documentId: "document-1", name, byteLen: "5", revision, encoding: "utf8" }];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText(`${name} editor`);
}

test("Print lays out pages in the core, then opens Print Preview on them", async () => {
  await openNewDocument("notes.md", "4");
  responses["file.printStart"] = [{ jobId: 5 }];
  responses["file.exportPdfProgress"] = [exportReport({ printedPages: "3" })];

  await chooseFileMenuItem(/^Print…/);

  expect(await screen.findByText("notes.md: 3 pages ready to print")).toBeInTheDocument();
  expect(executeCommand).toHaveBeenCalledWith({
    id: "file.printStart", context: { windowId: "main", documentId: "document-1" },
    arguments: {
      expectedRevision: "4", date: expect.any(String) as string, confirmed: false, scope: "document",
      setup: expect.objectContaining({ fontSize: 9 }) as unknown, colors: { kind: "print" },
    },
  });
  expect(openPrintWindow).toHaveBeenCalledWith({ kind: "pages" });
});

test("Print says when a document is too long to print", async () => {
  await openNewDocument("huge.log");
  responses["file.printStart"] = [{ jobId: 1 }];
  responses["file.exportPdfProgress"] = [exportReport({
    error: "Could not print: this needs at least 6,558 pages, and Print takes at most 5,000; use Export to PDF instead",
  })];

  await chooseFileMenuItem(/^Print…/);

  expect(await screen.findByText(/^Could not print: this needs at least 6,558 pages.*use Export to PDF instead$/)).toBeInTheDocument();
  expect(openPrintWindow).not.toHaveBeenCalled();
});

test("Page Setup previews the first page, saves the settings, and exports with them", async () => {
  await openNewDocument("notes.md");
  const frame = {
    widthMm: 210, heightMm: 297, paperWidthMm: 210, paperHeightMm: 297, landscape: false, marginLeftPt: 42, headerTopPt: 42,
    bodyTopPt: 60, footerTopPt: 800, fontSizePt: 9, lineHeightPt: 11.7, cellPt: 5.4, columns: 94, rows: 60,
    hasHeader: true, hasFooter: true, background: null, ruleColor: "#777777", styles: [{ color: "#111111", bold: false }],
  };
  responses["file.printPreview"] = Array.from({ length: 10 }, () => ({
    frame, page: { number: 1, header: [], footer: [], rows: [[[4, 11, 0, "first line"]]], rules: [] },
  }));

  await chooseFileMenuItem(/^Page Setup…/);
  const dialog = await screen.findByRole("dialog", { name: "Page Setup" });
  expect(await within(dialog).findByText("first line")).toBeInTheDocument();
  fireEvent.change(within(dialog).getByLabelText("Orientation"), { target: { value: "landscape" } });
  fireEvent.change(within(dialog).getByLabelText("Colors"), { target: { value: "blackAndWhite" } });
  fireEvent.change(within(dialog).getByLabelText("Margins"), { target: { value: "custom" } });
  fireEvent.change(within(dialog).getByRole("spinbutton", { name: "Left" }), { target: { value: "30" } });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "file.printPreview",
    arguments: expect.objectContaining({
      setup: expect.objectContaining({ orientation: "landscape" }) as unknown,
      colors: { kind: "blackAndWhite" },
    }) as unknown,
  })));
  fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
  expect(screen.queryByRole("dialog", { name: "Page Setup" })).not.toBeInTheDocument();
  const saved = JSON.parse(window.localStorage.getItem("nulis.printSettings.v1") ?? "{}") as {
    setup: { orientation: string; margins: { left: number; top: number } }; colors: string;
  };
  expect(saved.setup.orientation).toBe("landscape");
  expect(saved.setup.margins).toMatchObject({ left: 30, top: 15 });
  expect(saved.colors).toBe("blackAndWhite");

  responses["file.exportPdfStart"] = [{ jobId: 3 }];
  responses["file.exportPdfProgress"] = [exportReport({ result: { path: "/tmp/notes.pdf", pages: "1", missingGlyphs: "0", bytes: "9" } })];
  vi.mocked(save).mockResolvedValueOnce("/tmp/notes.pdf");
  await chooseFileMenuItem(/^Export to PDF/);
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "file.exportPdfStart",
    arguments: expect.objectContaining({
      setup: expect.objectContaining({ orientation: "landscape" }) as unknown,
      colors: { kind: "blackAndWhite" },
    }) as unknown,
  })));
});

test("Page Setup reports a setup the core refuses and will not save it", async () => {
  await openNewDocument("notes.md");
  vi.mocked(executeCommand).mockImplementation((request: ExecuteCommandRequest) => {
    if (request.id === "file.printPreview") return Promise.reject(new Error("the margins leave too little room for text"));
    const queue = responses[request.id];
    return Promise.resolve(queue?.length ? queue.shift() : null) as never;
  });
  await chooseFileMenuItem(/^Page Setup…/);
  const dialog = await screen.findByRole("dialog", { name: "Page Setup" });
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("too little room");
  expect(within(dialog).getByRole("button", { name: "Save" })).toBeDisabled();
  fireEvent.keyDown(dialog, { key: "Escape" });
  expect(screen.queryByRole("dialog", { name: "Page Setup" })).not.toBeInTheDocument();
});

test("asks before closing an unsaved document and honors cancel or discard", async () => {
  responses["file.new"] = [{
    documentId: "document-1", name: "Draft", byteLen: "5", revision: "1", encoding: "utf8", dirty: true,
  }];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByRole("tab", { name: /Draft/ });
  expect(tabNames()).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "Close Draft" }));
  expect(screen.getByRole("dialog", { name: "Save changes to Draft?" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.getByRole("tab", { name: /Draft/ })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Close Draft" }));
  fireEvent.click(screen.getByRole("button", { name: "Discard" }));
  await waitFor(() => expect(screen.queryByRole("tab", { name: /Draft/ })).not.toBeInTheDocument());
  expect(executeCommand).toHaveBeenCalledWith({
    id: "file.close", context: { windowId: "main", documentId: "document-1" },
    arguments: { discard: true },
  });
});

test("offers saved recovery checkpoints on startup", async () => {
  responses["recovery.list"] = [[{
    recoveryId: "document-old", name: "Lost.md", sourcePath: null, byteLen: "7",
  }]];
  responses["recovery.restore"] = [{
    documentId: "document-7", name: "Lost.md", byteLen: "7", revision: "1", encoding: "utf8", dirty: true,
  }];
  render(<App />);
  expect(await screen.findByRole("dialog", { name: "Recover unsaved work?" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Restore" }));
  expect(await screen.findByText("Lost.md editor")).toBeInTheDocument();
  expect(executeCommand).toHaveBeenCalledWith({
    id: "recovery.restore", context: { windowId: "main" }, arguments: { recoveryId: "document-old" },
  });
});

test("shows a changed-source recovery conflict without offering an unsafe restore", async () => {
  responses["recovery.list"] = [[{
    recoveryId: "document-old", name: "Changed.md", sourcePath: "/tmp/Changed.md",
    sourceChanged: true, byteLen: "7",
  }]];
  render(<App />);
  expect(await screen.findByText(/original file changed/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Restore" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Discard" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Recover unsaved work?" })).not.toBeInTheDocument());
  expect(executeCommand).toHaveBeenCalledWith({
    id: "recovery.discard", context: { windowId: "main" }, arguments: { recoveryId: "document-old" },
  });
});

test("guards window close until unsaved work is resolved", async () => {
  let requestClose: ((event: { preventDefault: () => void }) => void) | undefined;
  const onCloseRequested = vi.fn().mockImplementation(async (handler) => {
    requestClose = handler;
    return () => undefined;
  });
  vi.mocked(getCurrentWindow).mockReturnValue({
    onCloseRequested,
  } as unknown as ReturnType<typeof getCurrentWindow>);
  vi.mocked(isTauri).mockReturnValue(true);
  responses["file.new"] = [{
    documentId: "document-1", name: "Unsaved", byteLen: "3", revision: "1", encoding: "utf8", dirty: true,
  }];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByRole("tab", { name: /Unsaved/ });
  await waitFor(() => expect(onCloseRequested).toHaveBeenCalledTimes(2));
  const preventDefault = vi.fn();
  act(() => requestClose?.({ preventDefault }));
  expect(preventDefault).toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Discard" }));
  await waitFor(() => expect(closeWindowAfterSave).toHaveBeenCalledOnce());
});

test("switches an open document between text and hexadecimal views", async () => {
  responses["file.new"] = [{
    documentId: "document-1", name: "bytes.bin", byteLen: "4", revision: "0", encoding: "utf8",
  }];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  expect(await screen.findByText("bytes.bin editor")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Show hexadecimal editor" }));
  expect(screen.getByText("bytes.bin hexadecimal editor")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Show text editor" }));
  expect(screen.getByText("bytes.bin editor")).toBeInTheDocument();
});

test("Find in Files searches a filtered folder from the floating panel and opens a result at its line", async () => {
  await openDocuments("Second");
  expect(screen.queryByRole("button", { name: "Search" })).not.toBeInTheDocument();
  vi.mocked(open).mockResolvedValueOnce("/tmp/project");
  responses["search.findInFiles"] = [{
    matches: [{ path: "/tmp/project/a.txt", startByte: "5", endByte: "11", line: "3", lineText: "  needle here" }],
    filesScanned: 2, skippedFiles: 0, truncated: false,
  }];
  responses["file.open"] = [{ documentId: "document-9", name: "a.txt", byteLen: "20", revision: "0", encoding: "utf8", sourcePath: "/tmp/project/a.txt" }];

  fireEvent.click(screen.getByRole("button", { name: "Find in Second" }));
  const panel = screen.getByRole("dialog", { name: "Find and Replace" });
  fireEvent.click(within(panel).getByRole("tab", { name: "Find in Files" }));
  fireEvent.change(within(panel).getByRole("textbox", { name: "Find what" }), { target: { value: "needle" } });
  fireEvent.change(within(panel).getByRole("textbox", { name: "In files or types" }), { target: { value: "*.txt; *.md" } });
  fireEvent.click(within(panel).getByRole("button", { name: "Browse…" }));
  await waitFor(() => expect(within(panel).getByRole("textbox", { name: "In directory" })).toHaveValue("/tmp/project"));
  fireEvent.click(within(panel).getByRole("checkbox", { name: "Include subdirectories" }));
  fireEvent.click(within(panel).getByRole("button", { name: "Find" }));

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "search.findInFiles",
    context: { windowId: "main" },
    arguments: {
      root: "/tmp/project", query: "needle",
      options: { mode: "literal", caseSensitive: false, wholeWord: false },
      scope: { include: ["*.txt", "*.md"], recursive: false }, maxResults: 2000,
    },
  }));
  const results = await screen.findByRole("dialog", { name: "Search results" });
  const row = within(results).getByRole("option", { name: /a\.txt\s*3\s*needle here/ });
  fireEvent.click(row);
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "editor.setSelections", context: { windowId: "main", documentId: "document-9" },
    arguments: { selections: [{ anchorByte: "5", activeByte: "11" }] },
  }));
});

test("Replace in Files previews, asks for confirmation, then reports each file", async () => {
  await openDocuments("Second");
  vi.mocked(open).mockResolvedValueOnce("/tmp/project");
  const stamp = { size: "3", modified: "1" };
  responses["search.previewReplaceInFiles"] = [{
    files: [
      { path: "/tmp/project/a.txt", matches: 2, stamp, open: false },
      { path: "/tmp/Second", matches: 1, stamp, open: true },
      { path: "/tmp/project/c.txt", matches: 1, stamp, open: false },
    ],
    filesScanned: 4, truncated: false,
  }];
  responses["search.replaceInFiles"] = [[
    { path: "/tmp/project/a.txt", replacements: 2, error: null, document: null },
    { path: "/tmp/Second", replacements: 1, error: null,
      document: { documentId: "document-1", name: "Second", byteLen: "3", revision: "1", encoding: "utf8", dirty: true } },
    { path: "/tmp/project/c.txt", replacements: 0, error: "the file changed on disk after the preview", document: null },
  ]];

  fireEvent.click(screen.getByRole("button", { name: "Replace in Second" }));
  const panel = screen.getByRole("dialog", { name: "Find and Replace" });
  fireEvent.click(within(panel).getByRole("tab", { name: "Replace in Files" }));
  fireEvent.change(within(panel).getByRole("textbox", { name: "Find what" }), { target: { value: "cat" } });
  fireEvent.change(within(panel).getByRole("textbox", { name: "Replace with" }), { target: { value: "dog" } });
  fireEvent.click(within(panel).getByRole("button", { name: "Browse…" }));
  await waitFor(() => expect(within(panel).getByRole("textbox", { name: "In directory" })).toHaveValue("/tmp/project"));
  expect(within(panel).getByRole("button", { name: "Replace All" })).toBeDisabled();

  fireEvent.click(within(panel).getByRole("button", { name: "Preview" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "search.previewReplaceInFiles", context: { windowId: "main" },
    arguments: { root: "/tmp/project", query: "cat", options: { mode: "literal", caseSensitive: false, wholeWord: false },
      scope: { include: [], recursive: true } },
  }));
  const files = await within(panel).findByRole("list", { name: "Files to change" });
  expect(within(files).getAllByRole("listitem")).toHaveLength(3);
  expect(within(files).getByText("open: changed in the editor, not saved")).toBeInTheDocument();
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "search.replaceInFiles" }));

  // Changing the replacement makes the preview stale.
  fireEvent.change(within(panel).getByRole("textbox", { name: "Replace with" }), { target: { value: "dogs" } });
  expect(within(panel).getByRole("button", { name: "Replace All" })).toBeDisabled();
  fireEvent.change(within(panel).getByRole("textbox", { name: "Replace with" }), { target: { value: "dog" } });

  fireEvent.click(within(panel).getByRole("button", { name: "Replace All" }));
  const confirm = within(panel).getByRole("group", { name: "Confirm Replace in Files" });
  expect(confirm).toHaveTextContent("Replace 4 matches in 3 files?");
  fireEvent.click(within(confirm).getByRole("button", { name: "Replace" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "search.replaceInFiles", context: { windowId: "main" },
    arguments: {
      files: ["/tmp/project/a.txt", "/tmp/Second", "/tmp/project/c.txt"].map((path) => ({ path, stamp })),
      query: "cat", replacement: "dog", options: { mode: "literal", caseSensitive: false, wholeWord: false },
    },
  }));
  expect(await within(panel).findByText("Replaced 3 matches in 2 files; 1 file not changed")).toBeInTheDocument();
  const failures = within(panel).getByRole("list", { name: "Files not changed" });
  expect(failures).toHaveTextContent("c.txt");
  expect(failures).toHaveTextContent("the file changed on disk after the preview");
  // The open document's new state reached the editor: its tab is marked unsaved.
  expect(screen.getByRole("tab", { name: /Second/ })).toHaveAccessibleName(/unsaved|modified|•/i);
});

test("opens a bounded workspace tree lazily and routes files through the workspace command", async () => {
  vi.mocked(open).mockResolvedValueOnce("/tmp/project");
  responses["workspace.open"] = [{ root: "/tmp/project", name: "project" }];
  responses["workspace.list"] = [
    { relativePath: "", entries: [{ name: "src", relativePath: "src", kind: "directory" }], truncated: false },
    { relativePath: "src", entries: [{ name: "main.rs", relativePath: "src/main.rs", kind: "file", byteLen: "25" }], truncated: false },
  ];
  responses["workspace.openFile"] = [{
    documentId: "document-1", name: "main.rs", byteLen: "25", revision: "0",
    encoding: "utf8", sourcePath: "/tmp/project/src/main.rs",
  }];

  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "Open a folder" }));
  expect(await screen.findByRole("tree", { name: "project files" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: /src/ }));
  expect(await screen.findByRole("button", { name: /main.rs/ })).toBeInTheDocument();
  expect(executeCommand).toHaveBeenCalledWith({
    id: "workspace.list", context: { windowId: "main" },
    arguments: { path: "src", maxEntries: 500 },
  });
  fireEvent.click(screen.getByRole("button", { name: /main.rs/ }));
  expect(await screen.findByText("main.rs editor")).toBeInTheDocument();
  expect(executeCommand).toHaveBeenCalledWith({
    id: "workspace.openFile", context: { windowId: "main" }, arguments: { path: "src/main.rs" },
  });
});

test("filters centralized commands in the palette and executes a layout command", async () => {
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "Open command palette" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Search commands" }), {
    target: { value: "sidebar" },
  });
  fireEvent.click(screen.getByRole("option", { name: /Toggle Primary Side Bar/ }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "workbench.toggleSidebar", context: { windowId: "main" }, arguments: {},
  }));
  expect(document.querySelector("main")).toHaveClass("sidebar-hidden");
});


async function openSettings() {
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
  const dialog = await screen.findByRole("dialog", { name: "Settings" });
  await within(dialog).findByRole("tablist", { name: "Settings sections" });
  return dialog;
}

test("settings open as one sectioned surface with keyboard navigation", async () => {
  const dialog = await openSettings();
  const sections = within(dialog).getAllByRole("tab").map((tab) => tab.textContent);
  expect(sections).toEqual([
    "General", "Editor", "Appearance & Themes", "Languages", "Keybindings", "Menus & Toolbar",
    "Workspace", "Extensions & Security",
  ]);
  // Only the selected section is shown.
  expect(within(dialog).getByRole("tabpanel", { name: "General" })).toBeInTheDocument();
  expect(within(dialog).queryByRole("combobox", { name: "Color theme" })).not.toBeInTheDocument();

  const general = within(dialog).getByRole("tab", { name: "General" });
  fireEvent.keyDown(general, { key: "ArrowDown" });
  expect(within(dialog).getByRole("tab", { name: "Editor" })).toHaveAttribute("aria-selected", "true");
  expect(within(dialog).getByRole("tab", { name: "Editor" })).toHaveFocus();
  fireEvent.keyDown(within(dialog).getByRole("tab", { name: "Editor" }), { key: "End" });
  expect(within(dialog).getByRole("tabpanel", { name: "Extensions & Security" })).toHaveTextContent(/no access to open documents/);

  // The last section is remembered and settings stay a singleton surface.
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog", { name: "Settings" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
  const reopened = await screen.findByRole("dialog", { name: "Settings" });
  expect(await within(reopened).findByRole("tab", { name: "Extensions & Security" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getAllByRole("dialog", { name: "Settings" })).toHaveLength(1);
});

test("language features can be switched off per language and are saved to settings.json", async () => {
  const rows = [
    { id: "json", displayName: "JSON", capabilities: [
      { kind: "diagnostics", label: "JSON", enabled: true },
      { kind: "format", label: "JSON", enabled: true },
      { kind: "compact", label: "JSON", enabled: true },
    ] },
    { id: "markdown", displayName: "Markdown", capabilities: [{ kind: "preview", label: "Markdown", enabled: true }] },
  ];
  const settingsAtStart = {
    revision: 3, loadError: null, rejectedAssociations: ["src/*.x"],
    document: { schemaVersion: 1, values: {
      editor: { tabSize: 2 },
      files: { associations: { "*.h": "cpp", "src/*.x": "rust" } },
      "[json]": { disabledCapabilities: ["someOtherKind"], note: "kept" },
    } },
  };
  // The editor window reads settings at start, then the Settings window does.
  responses["settings.state"] = [settingsAtStart, settingsAtStart];
  responses["language.capabilities"] = [rows];
  const dialog = await openSettings();
  fireEvent.click(within(dialog).getByRole("tab", { name: "Languages" }));
  const associations = await within(dialog).findByRole("list", { name: "Your file associations" });
  expect(associations).toHaveTextContent("*.hcpp");
  expect(associations).toHaveTextContent("src/*.xrustignored");

  fireEvent.click(within(dialog).getByRole("checkbox", { name: "Beautify for JSON" }));
  fireEvent.click(within(dialog).getByRole("checkbox", { name: "Minify for JSON" }));
  expect(within(within(dialog).getByRole("tab", { name: /^Languages/ })).getByRole("img", { name: "unapplied changes" })).toBeInTheDocument();
  responses["settings.state"] = [{ revision: 4, loadError: null, rejectedAssociations: [], document: { schemaVersion: 1, values: {} } }];
  responses["language.capabilities"] = [rows.map((row) => row.id !== "json" ? row : {
    ...row, capabilities: row.capabilities.map((capability) => ({ ...capability, enabled: capability.kind === "diagnostics" })),
  })];
  fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({ id: "settings.saveChecked" })));
  const saved = vi.mocked(executeCommand).mock.calls.find(([request]) => request.id === "settings.saveChecked")?.[0];
  expect(saved?.arguments).toEqual({
    expectedRevision: 3,
    document: { schemaVersion: 1, values: {
      editor: { tabSize: 2 },
      files: { associations: { "*.h": "cpp", "src/*.x": "rust" } },
      "[json]": { disabledCapabilities: ["someOtherKind", "format", "compact"], note: "kept" },
    } },
  });
  await waitFor(() => expect(within(dialog).getByRole("checkbox", { name: "Beautify for JSON" })).not.toBeChecked());
  expect(within(dialog).queryByRole("img", { name: "unapplied changes" })).not.toBeInTheDocument();
});

test("indentation settings are saved to settings.json and keep other editor values", async () => {
  const settingsAtStart = {
    revision: 1, loadError: null, rejectedAssociations: [],
    document: { schemaVersion: 1, values: { editor: { tabSize: 2, wordWrap: true } } },
  };
  // The editor window reads settings at start, then the Settings window does.
  responses["settings.state"] = [settingsAtStart, settingsAtStart];
  responses["language.capabilities"] = [[]];
  const dialog = await openSettings();
  fireEvent.click(within(dialog).getByRole("tab", { name: "Editor" }));
  await waitFor(() => expect(within(dialog).getByRole("spinbutton", { name: "Tab size" })).toHaveValue(2));
  expect(within(dialog).getByRole("combobox", { name: "Auto-indent" })).toHaveValue("full");
  expect(within(dialog).getByRole("checkbox", { name: "Detect indentation" })).toBeChecked();
  fireEvent.change(within(dialog).getByRole("combobox", { name: "Auto-indent" }), { target: { value: "none" } });
  fireEvent.click(within(dialog).getByRole("checkbox", { name: "Insert spaces" }));
  expect(within(within(dialog).getByRole("tab", { name: /^Editor/ })).getByRole("img", { name: "unapplied changes" })).toBeInTheDocument();
  responses["settings.state"] = [{ revision: 2, loadError: null, rejectedAssociations: [], document: { schemaVersion: 1, values: {} } }];
  responses["language.capabilities"] = [[]];
  fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({ id: "settings.saveChecked" })));
  const saved = vi.mocked(executeCommand).mock.calls.find(([request]) => request.id === "settings.saveChecked")?.[0];
  expect(saved?.arguments).toEqual({
    expectedRevision: 1,
    document: { schemaVersion: 1, values: { editor: {
      tabSize: 2, wordWrap: true, autoIndent: "none", insertSpaces: true, detectIndentation: true,
      suggest: { auto: true, acceptOnEnter: true },
    } } },
  });
});

test("applies and persists editor appearance settings at the observed revision", async () => {
  const dialog = await openSettings();
  fireEvent.click(within(dialog).getByRole("tab", { name: "Appearance & Themes" }));
  const themes = within(dialog).getByRole("combobox", { name: "Color theme" });
  expect(themes.querySelectorAll("option")).toHaveLength(99);
  expect(within(dialog).getByRole("option", { name: "NvChad · Radium" })).toBeInTheDocument();
  fireEvent.change(themes, { target: { value: "nulis.paper-light" } });
  fireEvent.click(within(dialog).getByRole("tab", { name: "Editor" }));
  fireEvent.change(within(dialog).getByRole("spinbutton", { name: "Editor font size" }), { target: { value: "18" } });
  fireEvent.change(within(dialog).getByRole("combobox", { name: "Editor font" }), {
    target: { value: '"JetBrains Mono", monospace' },
  });

  // Previewed immediately and marked as pending in each edited section.
  expect(document.querySelector("main")).toHaveStyle({
    "--editor-bg": "#FBFAF7",
    "--editor-font-size": "18px",
    "--editor-font-family": '"JetBrains Mono", monospace',
  });
  for (const name of [/^Editor/, /^Appearance & Themes/]) {
    expect(within(within(dialog).getByRole("tab", { name })).getByRole("img", { name: "unapplied changes" })).toBeInTheDocument();
  }
  expect(within(dialog).getByRole("status")).toHaveTextContent("Unapplied changes");

  fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "customization.saveChecked",
    arguments: expect.objectContaining({ expectedRevision: 0 }),
  })));
  await waitFor(() => expect(within(dialog).getByRole("status")).toHaveTextContent("Settings saved"));
  expect(within(dialog).queryByRole("img", { name: "unapplied changes" })).not.toBeInTheDocument();
  expect(storedCustomization.document.appearance).toMatchObject({ theme: "nulis.paper-light", fontSize: 18 });
  await waitFor(() => expect(JSON.parse(window.localStorage.getItem("nulis.editorPreferences.v1") ?? "{}"))
    .toMatchObject({ theme: "nulis.paper-light", fontSize: 18 }));
});

test("cancel and discard revert an unapplied preview, and closing with changes asks first", async () => {
  const dialog = await openSettings();
  fireEvent.click(within(dialog).getByRole("tab", { name: "Appearance & Themes" }));
  fireEvent.change(within(dialog).getByRole("combobox", { name: "Color theme" }), { target: { value: "nvchad.radium" } });
  expect(document.querySelector("main")).toHaveStyle({ "--editor-bg": "#101317", "--syntax-keyword": "#c397d8" });

  fireEvent.click(within(dialog).getByRole("button", { name: "Close settings" }));
  const confirm = within(dialog).getByRole("alertdialog", { name: "Apply your changes to Settings?" });
  fireEvent.click(within(confirm).getByRole("button", { name: "Continue Editing" }));
  expect(within(dialog).getByRole("combobox", { name: "Color theme" })).toHaveValue("nvchad.radium");

  fireEvent.keyDown(within(dialog).getByRole("combobox", { name: "Color theme" }), { key: "Escape" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Discard" }));
  expect(screen.queryByRole("dialog", { name: "Settings" })).not.toBeInTheDocument();
  expect(document.querySelector("main")).not.toHaveStyle({ "--editor-bg": "#101317" });
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "customization.saveChecked" }));
});

test("a stale settings revision is reported and never overwrites the newer one", async () => {
  const dialog = await openSettings();
  fireEvent.click(within(dialog).getByRole("tab", { name: "Editor" }));
  fireEvent.change(within(dialog).getByRole("spinbutton", { name: "Editor font size" }), { target: { value: "20" } });
  // Another window stores a newer customization first.
  storedCustomization = {
    revision: 1,
    document: { ...storedCustomization.document, toolbar: ["file.open"] },
  };
  fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("changed in another window");
  expect(storedCustomization.document.appearance.fontSize).toBe(DEFAULT_EDITOR_PREFERENCES.fontSize);
  expect(within(dialog).getByRole("button", { name: "Apply" })).toBeDisabled();

  fireEvent.click(within(dialog).getByRole("button", { name: "Reload" }));
  await waitFor(() => expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument());
  expect(within(dialog).getByRole("spinbutton", { name: "Editor font size" })).toHaveValue(DEFAULT_EDITOR_PREFERENCES.fontSize);
  fireEvent.click(within(dialog).getByRole("tab", { name: "Menus & Toolbar" }));
  expect(within(dialog).getByRole("button", { name: "Remove Open File… from toolbar" })).toBeInTheDocument();
});

test("a customized toolbar choice applies to the icon toolbar with command parity", async () => {
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "session.load") return DEFAULT_SESSION as never;
    if (request.id === "customization.get") return storedCustomization.document as never;
    if (request.id === "customization.state") return storedCustomization as never;
    if (request.id === "customization.saveChecked") {
      storedCustomization = { revision: 1, document: (request.arguments as { document: CustomizationDocument }).document };
      return storedCustomization as never;
    }
    if (request.id === "plugin.activateStartup") return [] as never;
    return null as never;
  });
  const dialog = await openSettings();
  const toolbar = screen.getByRole("toolbar", { name: "Custom toolbar" });
  const save = within(toolbar).getByRole("button", { name: "Toolbar: Save" });
  expect(save).toHaveAttribute("title", expect.stringMatching(/^Save \((?:⌘|Ctrl\+)S\)$/));
  expect(save.querySelector('svg[data-icon="save"]')).not.toBeNull();
  expect(save).toBeDisabled();
  expect(within(toolbar).queryByRole("button", { name: "Toolbar: Open File…" })).not.toBeInTheDocument();

  fireEvent.click(within(dialog).getByRole("tab", { name: "Menus & Toolbar" }));
  fireEvent.change(within(dialog).getByRole("combobox", { name: "Add toolbar command" }), { target: { value: "file.open" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
  const added = await within(toolbar).findByRole("button", { name: "Toolbar: Open File…" });
  expect(added.querySelector('svg[data-icon="open"]')).not.toBeNull();
  vi.mocked(open).mockResolvedValueOnce(null);
  fireEvent.click(added);
  await waitFor(() => expect(open).toHaveBeenCalledWith({ multiple: false, directory: false }));
});

test("files the system asks Nulis to open are opened at launch and while running", async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(takeLaunchPaths).mockResolvedValueOnce(["/Users/me/notes.md"]);
  const summary = (id: string, name: string) => ({
    documentId: id, name, byteLen: "4", revision: "0", encoding: "utf8", sourcePath: `/Users/me/${name}`,
  });
  responses["file.open"] = [summary("document-1", "notes.md"), summary("document-2", "main.rs")];
  render(<App />);
  // Opened from Finder before the window was ready: it opens after the session is restored.
  expect(await screen.findByText("notes.md editor")).toBeInTheDocument();
  const opens = () => vi.mocked(executeCommand).mock.calls
    .filter(([request]) => request.id === "file.open").map(([request]) => (request.arguments as { path: string }).path);
  expect(opens()).toEqual(["/Users/me/notes.md"]);

  // Sent while running (Open With, the Dock icon, or a second launch).
  const listener = vi.mocked(listen).mock.calls.find(([event]) => event === "nulis://open-paths")?.[1];
  expect(listener).toBeDefined();
  await act(async () => { listener?.({ event: "nulis://open-paths", id: 2, payload: ["/Users/me/main.rs"] }); });
  expect(await screen.findByText("main.rs editor")).toBeInTheDocument();
  expect(opens()).toEqual(["/Users/me/notes.md", "/Users/me/main.rs"]);
});

test("the desktop shell opens Help as its own window instead of an overlay", async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(openHelpWindow).mockClear();
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  const menuListener = vi.mocked(listen).mock.calls.find(([event]) => event === "nulis://menu-command")?.[1];
  expect(menuListener).toBeDefined();
  await act(async () => { menuListener?.({ event: "nulis://menu-command", id: 1, payload: "workbench.openHelp" }); });
  await waitFor(() => expect(openHelpWindow).toHaveBeenCalledTimes(1));
  expect(screen.queryByRole("dialog", { name: "Nulis Help" })).not.toBeInTheDocument();
});

test("the desktop shell opens Settings as its own window instead of an overlay", async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
  fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
  expect(openSettingsWindow).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole("dialog", { name: "Settings" })).not.toBeInTheDocument();
  expect(listen).toHaveBeenCalledWith("nulis://customization-changed", expect.any(Function));
  expect(listen).toHaveBeenCalledWith("nulis://appearance-preview", expect.any(Function));
});

test("the desktop shell opens Find and Replace and its results as their own windows", async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  await openDocuments("Second");
  fireEvent.click(screen.getByRole("button", { name: "Find in Second" }));
  expect(openFindWindow).toHaveBeenCalledOnce();
  expect(screen.queryByRole("dialog", { name: "Find and Replace" })).not.toBeInTheDocument();
  expect(emitTo).toHaveBeenCalledWith("find", "nulis://find-open", { tab: "find", seed: "seeded" });

  const handler = vi.mocked(listen).mock.calls.find(([event]) => event === "nulis://find-message")?.[1];
  expect(handler).toBeDefined();
  const deliver = (payload: object) => act(() => { handler!({ event: "nulis://find-message", id: 1, payload } as never); });
  vi.mocked(emitTo).mockClear();
  deliver({ kind: "ready" });
  expect(emitTo).toHaveBeenCalledWith("find", "nulis://find-context", expect.objectContaining({
    activeDocument: expect.objectContaining({ documentId: "document-1" }),
  }));
  expect(emitTo).toHaveBeenCalledWith("find", "nulis://find-open", { tab: "find", seed: "seeded" });

  deliver({ kind: "search", request: { action: "next", query: "x", replacement: "", regex: false, caseSensitive: true } });
  expect(JSON.parse(screen.getByText("Second editor").getAttribute("data-search") ?? "null")).toMatchObject({ action: "next", query: "x" });

  const list = { title: "Lines containing “x”", rows: [], truncated: false, multipleSources: false };
  deliver({ kind: "results", list });
  expect(openResultsWindow).toHaveBeenCalledOnce();
  expect(emitTo).toHaveBeenCalledWith("find-results", "nulis://find-results", { ...list, openDocumentIds: ["document-1"] });

  deliver({ kind: "openMatch", target: { documentId: "document-1", startByte: "2", endByte: "3" } });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "editor.setSelections", context: { windowId: "main", documentId: "document-1" },
    arguments: { selections: [{ anchorByte: "2", activeByte: "3" }] },
  }));
  deliver({ kind: "status", message: "3 matches on 2 lines" });
  expect(screen.getByText("3 matches on 2 lines")).toBeInTheDocument();
});

test("a file changed by another program is reported once and can be reloaded or kept", async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  await openDocuments("First", "Second");
  const focus = async () => { act(() => { window.dispatchEvent(new Event("focus")); }); await settle(); };
  const modifiedSecond = [{ documentId: "document-2", path: "/tmp/Second", kind: "modified", dirty: true }];

  responses["editor.externalChanges"] = [modifiedSecond];
  responses["editor.keepExternalVersion"] = [{ documentId: "document-2", name: "Second", byteLen: "0", revision: "0", encoding: "utf8", sourcePath: "/tmp/Second" }];
  await focus();
  const changed = await screen.findByRole("dialog", { name: "Second was changed by another program" });
  expect(changed).toHaveTextContent("discards your unsaved edits");
  fireEvent.click(within(changed).getByRole("button", { name: "Keep Mine" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "editor.keepExternalVersion", context: { windowId: "main", documentId: "document-2" }, arguments: {},
  }));

  // The same change is not asked about twice.
  responses["editor.externalChanges"] = [modifiedSecond];
  await focus();
  expect(screen.queryByRole("dialog", { name: "Second was changed by another program" })).not.toBeInTheDocument();

  responses["editor.externalChanges"] = [[{ documentId: "document-1", path: "/tmp/First", kind: "modified", dirty: false }]];
  responses["file.reload"] = [{ documentId: "document-1", name: "First", byteLen: "9", revision: "0", encoding: "utf8", sourcePath: "/tmp/First" }];
  await focus();
  const first = await screen.findByRole("dialog", { name: "First was changed by another program" });
  fireEvent.click(within(first).getByRole("button", { name: "Reload" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "file.reload", context: { windowId: "main", documentId: "document-1" }, arguments: { discard: true },
  }));
  expect(await screen.findByText("Reloaded First from disk")).toBeInTheDocument();

  responses["editor.externalChanges"] = [[{ documentId: "document-1", path: "/tmp/First", kind: "deleted", dirty: false }]];
  await focus();
  const deleted = await screen.findByRole("dialog", { name: "First was deleted or moved" });
  fireEvent.click(within(deleted).getByRole("button", { name: "Close" }));
  await waitFor(() => expect(screen.queryByText("First editor")).not.toBeInTheDocument());
});

test("Reload from Disk asks first when there are unsaved edits", async () => {
  responses["file.new"] = [{ documentId: "document-1", name: "Draft", byteLen: "3", revision: "1", encoding: "utf8", sourcePath: "/tmp/Draft", dirty: true }];
  responses["file.reload"] = [{ documentId: "document-1", name: "Draft", byteLen: "1", revision: "0", encoding: "utf8", sourcePath: "/tmp/Draft" }];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("Draft editor");
  await chooseMenu("File", "Reload from Disk");
  const dialog = screen.getByRole("dialog", { name: "Reload Draft from disk?" });
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "file.reload" }));
  fireEvent.click(within(dialog).getByRole("button", { name: "Discard Edits and Reload" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "file.reload", context: { windowId: "main", documentId: "document-1" }, arguments: { discard: true },
  }));
});

test("Format commands from the palette reach the focused editor", async () => {
  vi.mocked(listCommands).mockResolvedValueOnce([
    { id: "file.new", title: "New Text File", category: "File" },
    { id: "format.sortAscending", title: "Sort Lines Ascending", category: "Format" },
  ]);
  await openDocuments("First");
  fireEvent.click(screen.getByRole("button", { name: "Open command palette" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Search commands" }), { target: { value: "sort lines" } });
  fireEvent.click(await screen.findByRole("option", { name: /Sort Lines Ascending/ }));
  await waitFor(() => expect(screen.getByText("First editor")).toHaveAttribute("data-action", "sortAscending"));
});

test("the View menu toggles line change indicators with a persistent default", async () => {
  await openDocuments("First");
  expect(screen.getByText("First editor")).toHaveAttribute("data-line-changes", "true");
  await chooseMenu("View", /Line Change Indicators/);
  expect(screen.getByText("First editor")).toHaveAttribute("data-line-changes", "false");
  expect(window.localStorage.getItem("nulis.lineChangeIndicators.v1")).toBe("false");
  expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({ id: "view.toggleLineChangeIndicators" }));
  fireEvent.click(screen.getByRole("button", { name: "Open View menu" }));
  expect(screen.getByRole("menuitemcheckbox", { name: /Line Change Indicators/ })).toHaveAttribute("aria-checked", "false");
});


test("keeps documents in one tab strip and shows another view of a document in a split pane", async () => {
  await openDocuments("First", "Second");
  expect(tabNames()).toEqual(["First", "Second"]);
  // One strip: only the active tab's document is shown.
  expect(screen.queryByText("First editor")).not.toBeInTheDocument();
  expect(paneNames()).toEqual(["Editor"]);
  expect(screen.queryByRole("tablist", { name: "Layout groups" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Split editor" }));
  expect(paneNames()).toEqual(["Pane 1 of 2: Second", "Pane 2 of 2: Second"]);
  expect(tabNames()).toEqual(["First", "Workspace"]);

  // The divider resizes from the keyboard within its bounds.
  const divider = screen.getByRole("separator", { name: "Resize panes horizontally" });
  expect(divider).toHaveAttribute("aria-valuenow", "50");
  fireEvent.keyDown(divider, { key: "ArrowLeft" });
  expect(divider).toHaveAttribute("aria-valuenow", "48");
  fireEvent.keyDown(divider, { key: "Home" });
  expect(divider).toHaveAttribute("aria-valuenow", "10");

  // Closing one of two views keeps the document open and dissolves the workspace.
  fireEvent.click(within(panes()[1]).getByRole("button", { name: "Close pane Second" }));
  expect(paneNames()).toEqual(["Editor"]);
  expect(tabNames()).toEqual(["First", "Second"]);
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "file.close" }));

  fireEvent.click(screen.getByRole("button", { name: "Close Second" }));
  await waitFor(() => expect(tabNames()).toEqual(["First"]));
  expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({ id: "file.close" }));
  // The neighbouring tab takes over.
  expect(await screen.findByText("First editor")).toBeInTheDocument();
});

test("dragging a tab into the content groups both tabs into one renamable workspace tab", async () => {
  await openDocuments("First", "Second", "Third");

  // Dragging within the threshold, or dragging the active tab onto itself, does nothing.
  dragTabTo("Third", panes()[0], 780, 300, false);
  expect(document.querySelector(".drop-overlay")).toBeNull();
  fireEvent.pointerUp(window);
  expect(tabNames()).toEqual(["First", "Second", "Third"]);

  // The overlay previews the half the tab will occupy; Escape cancels.
  dragTabTo("First", panes()[0], 400, 590, false);
  expect(document.querySelector('[data-drop-zone="bottom"]')).not.toBeNull();
  fireEvent.pointerMove(panes()[0].querySelector(".editor-pane")!, { clientX: 780, clientY: 300 });
  expect(document.querySelector('[data-drop-zone="right"]')).not.toBeNull();
  fireEvent.keyDown(window, { key: "Escape" });
  fireEvent.pointerUp(window);
  expect(tabNames()).toEqual(["First", "Second", "Third"]);

  // Drop: First joins the active tab on the right and both leave the strip as one tab.
  dragTabTo("First", panes()[0], 780, 300);
  expect(paneNames()).toEqual(["Pane 1 of 2: Third", "Pane 2 of 2: First"]);
  // While its name is being typed, the workspace tab shows the name field.
  expect(tabNames()).toEqual(["Second"]);
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "file.close" }));

  // The new workspace's name is selected for typing, as in the reference.
  const input = screen.getByRole("textbox", { name: "Workspace name" });
  expect(input).toHaveValue("Workspace");
  fireEvent.change(input, { target: { value: "  monitoring  " } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(tabNames()).toEqual(["Second", "monitoring"]);
  expect(screen.getByRole("status")).toHaveTextContent("Workspace renamed to monitoring");

  // Dropping onto one pane of the workspace splits only that pane.
  dragTabTo("Second", panes()[1], 400, 20);
  expect(paneNames()).toEqual(["Pane 1 of 3: Third", "Pane 2 of 3: Second", "Pane 3 of 3: First"]);
  expect(document.querySelector(".layout-split.horizontal .layout-split.vertical")).not.toBeNull();
  expect(tabNames()).toEqual(["monitoring"]);
  expect(screen.queryByRole("textbox", { name: "Workspace name" })).not.toBeInTheDocument();

  await waitFor(() => {
    const saved = saveRequests().at(-1);
    expect(saved?.layouts).toHaveLength(1);
    expect(saved?.layouts[0]).toMatchObject({ name: "monitoring", root: { kind: "split", direction: "horizontal" } });
    expect(saved?.documents).toEqual(["path:/tmp/Third", "path:/tmp/Second", "path:/tmp/First"]);
    expect(JSON.stringify(saved)).not.toContain("document-1");
  });

  // Renaming again: double-click, Escape cancels, an invalid name is rejected with feedback.
  fireEvent.doubleClick(screen.getByRole("tab", { name: "monitoring" }));
  const again = screen.getByRole("textbox", { name: "Workspace name" });
  fireEvent.change(again, { target: { value: "discarded" } });
  fireEvent.keyDown(again, { key: "Escape" });
  expect(tabNames()).toEqual(["monitoring"]);
  fireEvent.doubleClick(screen.getByRole("tab", { name: "monitoring" }));
  const invalid = screen.getByRole("textbox", { name: "Workspace name" });
  fireEvent.change(invalid, { target: { value: "x".repeat(81) } });
  fireEvent.keyDown(invalid, { key: "Enter" });
  expect(tabNames()).toEqual(["monitoring"]);
  expect(screen.getByRole("status")).toHaveTextContent("A name needs 1 to 80 visible characters");
});

test("the welcome screen can be turned off, and startup then opens an empty document", async () => {
  responses["file.new"] = [{ documentId: "document-1", name: "Untitled 1", byteLen: "0", revision: "0", encoding: "utf8" }];
  const first = render(<App />);
  await screen.findByText("Nulis 0.1.0");
  expect(screen.getByRole("heading", { name: /Write, inspect/ })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("checkbox", { name: "Do not show this on startup" }));
  expect(window.localStorage.getItem("nulis.welcomeOnStartup.v1")).toBe("false");
  // It stays for this session so the choice can be reversed.
  expect(screen.getByRole("checkbox", { name: "Do not show this on startup" })).toBeChecked();
  expect(tabNames()).toEqual([]);
  first.unmount();

  render(<App />);
  expect(await screen.findByText("Untitled 1 editor")).toBeInTheDocument();
  expect(tabNames()).toEqual(["Untitled 1"]);
  expect(screen.queryByRole("heading", { name: /Write, inspect/ })).toBeNull();
});

test("the About page shows the product identity without the welcome actions", async () => {
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "About Nulis" }));
  const about = screen.getByRole("dialog", { name: "Nulis" });
  expect(within(about).getByText("Version 0.1.0")).toBeInTheDocument();
  expect(within(about).getByRole("heading", { name: /Write, inspect/ })).toBeInTheDocument();
  expect(within(about).queryByRole("button", { name: "New text file" })).toBeNull();
  // The name's origin and the license are part of the page.
  expect(within(about).getByText(/Javanese word that means/)).toBeInTheDocument();
  expect(within(about).getByText(/either version 3 of the License/)).toBeInTheDocument();
  fireEvent.click(within(about).getByRole("button", { name: "View License" }));
  expect(within(about).getByLabelText("GNU General Public License, version 3")).toHaveTextContent("Version 3, 29 June 2007");
  fireEvent.click(within(about).getByRole("button", { name: "Back" }));
  fireEvent.keyDown(about, { key: "Escape" });
  expect(screen.queryByRole("dialog", { name: "Nulis" })).toBeNull();
});

test("the Help menu opens sectioned help and the About page", async () => {
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  await chooseMenu("Help", "Nulis Help");
  const help = await screen.findByRole("dialog", { name: "Nulis Help" });
  const sections = within(help).getAllByRole("tab").map((tab) => tab.textContent);
  expect(sections.slice(0, 3)).toEqual(["Getting Started", "Files", "Tabs and Workspaces"]);
  expect(sections).toContain("Building Plugins");
  expect(within(help).getByRole("heading", { name: "Getting Started", level: 2 })).toBeInTheDocument();

  // A reference to another chapter is a link to that section, never a file name.
  fireEvent.click(within(help).getByRole("tab", { name: "Plugins" }));
  expect(help).not.toHaveTextContent("PLUGIN_GUIDE.md");
  fireEvent.click(within(help).getByRole("button", { name: "Building Plugins" }));
  expect(within(help).getByRole("tab", { name: "Building Plugins" })).toHaveAttribute("aria-selected", "true");
  expect(within(help).getByRole("heading", { name: "Quick start: your first plugin" })).toBeInTheDocument();
  expect(within(help).queryByRole("heading", { name: "The window at a glance" })).toBeNull();

  // Search narrows the sections to those that mention the text.
  fireEvent.change(within(help).getByRole("searchbox", { name: "Search help" }), { target: { value: "esbuild" } });
  expect(within(help).getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Building Plugins"]);
  fireEvent.keyDown(help, { key: "Escape" });
  expect(screen.queryByRole("dialog", { name: "Nulis Help" })).toBeNull();

  fireEvent.keyDown(window, { key: "F1" });
  expect(await screen.findByRole("dialog", { name: "Nulis Help" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Close" }));

  await chooseMenu("Help", "About Nulis");
  const about = await screen.findByRole("dialog", { name: "Nulis" });
  // The default theme is dark, so the About window uses the dark batik.
  expect(about).toHaveAttribute("data-batik", "dark");
  expect(about.style.getPropertyValue("--about-batik")).toMatch(/about-batik-dark/);
});

test("the status bar shows the caret position, the size in bytes and characters, and the mode", async () => {
  const first = { documentId: "document-1", name: "First", byteLen: "1234567", revision: "0", encoding: "utf8", readOnly: false };
  responses["file.new"] = [first];
  responses["editor.characterCount"] = [{ revision: "0", characters: "1200000", words: "180000", lines: "1" }];
  responses["editor.toggleReadOnly"] = [{ ...first, readOnly: true }, { ...first, readOnly: false }];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  const footer = document.querySelector<HTMLElement>(".statusbar")!;
  expect(within(footer).queryByRole("button", { name: /mode/ })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("First editor");

  // Size: bytes at once, characters when the count arrives.
  expect(within(footer).getByText("1,234,567 bytes")).toBeInTheDocument();
  expect(await within(footer).findByText("1,234,567 bytes · 1,200,000 characters · 180,000 words · 1 line")).toBeInTheDocument();

  // Caret position follows the focused editor.
  expect(within(footer).queryByText(/^Ln /)).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Move caret in First" }));
  expect(within(footer).getByText("Ln 1,234, Col 7")).toBeInTheDocument();
  expect(within(footer).getByText("U+00E9")).toHaveAttribute("title", "Character after the caret (Unicode code point)");

  // Mode: the badge names the state and toggles it; the tab gains a lock.
  const mode = within(footer).getByRole("button", { name: "Edit mode: switch to Read-Only mode" });
  expect(mode).toHaveTextContent("EDIT");
  expect(mode).toHaveAttribute("aria-pressed", "false");
  fireEvent.click(mode);
  const locked = await within(footer).findByRole("button", { name: "Read-only mode: switch to Edit mode" });
  expect(locked).toHaveTextContent("READ ONLY");
  expect(locked).toHaveAttribute("aria-pressed", "true");
  expect(within(screen.getByRole("tab", { name: /First/ })).getByRole("img", { name: "read-only" })).toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent("First is read-only");
  expect(screen.getByRole("button", { name: "Editor mode of First: read-only" })).toBeInTheDocument();

  // The editor's own toggle switches back.
  fireEvent.click(screen.getByRole("button", { name: "Editor mode of First: read-only" }));
  expect(await within(footer).findByRole("button", { name: "Edit mode: switch to Read-Only mode" })).toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent("First can be edited");
});

test("the status bar names the line endings and converts them from a menu", async () => {
  const first = { documentId: "document-1", name: "First", byteLen: "12", revision: "0", encoding: "utf8", readOnly: false };
  responses["file.new"] = [first];
  responses["editor.characterCount"] = [{ revision: "0", characters: "12", words: "2", lines: "3", lineEnding: "mixed" }];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("First editor");
  const footer = document.querySelector<HTMLElement>(".statusbar")!;
  const button = await within(footer).findByRole("button", { name: "Line endings: Mixed. Convert line endings" });
  expect(button).toHaveAttribute("title", expect.stringContaining("mixes line endings"));

  fireEvent.click(button);
  const menu = screen.getByRole("menu", { name: "Line endings" });
  fireEvent.click(within(menu).getByRole("menuitem", { name: /CRLF \(Windows\)/ }));
  await waitFor(() => expect(screen.getByText("First editor")).toHaveAttribute("data-action", "lineEndingsCrlf"));
});

test("View options apply to every editor, are remembered, and zoom has keys and a status badge", async () => {
  await openDocuments("First");
  const view = () => JSON.parse(screen.getByText("First editor").getAttribute("data-view") ?? "{}") as Record<string, unknown>;
  expect(view()).toMatchObject({ wordWrap: false, showWhitespace: false, showLineEndings: false, showLineNumbers: true });
  expect(view().columnMarker).toBeUndefined();
  const baseLineHeight = view().lineHeight as number;

  fireEvent.keyDown(window, { key: "Ω", code: "KeyZ", altKey: true });
  await waitFor(() => expect(view().wordWrap).toBe(true));
  expect(screen.getByRole("status")).toHaveTextContent("Word wrap on");
  await chooseMenu("View", "Show Whitespace");
  await chooseMenu("View", "Show Line Endings");
  await chooseMenu("View", /Line Numbers$/);
  await chooseMenu("View", "Column Marker at 80");
  expect(view()).toMatchObject({ showWhitespace: true, showLineEndings: true, showLineNumbers: false, columnMarker: 80 });
  expect(JSON.parse(window.localStorage.getItem("nulis.viewOptions.v1") ?? "{}")).toMatchObject({
    wordWrap: true, showWhitespace: true, lineNumbers: false, columnMarker: true,
  });

  const footer = document.querySelector<HTMLElement>(".statusbar")!;
  expect(within(footer).queryByRole("button", { name: /Zoom/ })).toBeNull();
  fireEvent.keyDown(window, { key: "=", code: "Equal", ctrlKey: true });
  fireEvent.keyDown(window, { key: "=", code: "Equal", ctrlKey: true });
  const badge = await within(footer).findByRole("button", { name: "Zoom 125%. Reset zoom" });
  expect(view().lineHeight).toBeCloseTo(baseLineHeight * 1.25);
  fireEvent.keyDown(window, { key: "-", code: "Minus", ctrlKey: true });
  expect(await within(footer).findByRole("button", { name: "Zoom 110%. Reset zoom" })).toBeInTheDocument();
  fireEvent.click(within(footer).getByRole("button", { name: "Zoom 110%. Reset zoom" }));
  await waitFor(() => expect(badge).not.toBeInTheDocument());
  expect(within(footer).queryByRole("button", { name: /Zoom/ })).toBeNull();
});

test("Open Recent lists opened files, filters them, opens one, and drops a file that is gone", async () => {
  window.localStorage.setItem("nulis.recentFiles.v1", JSON.stringify(["/work/old.txt", "/work/gone.txt"]));
  await openDocuments("First");
  expect(JSON.parse(window.localStorage.getItem("nulis.recentFiles.v1") ?? "[]"))
    .toEqual(["/tmp/First", "/work/old.txt", "/work/gone.txt"]);

  await chooseMenu("File", "Open Recent…");
  const picker = screen.getByRole("dialog", { name: "Open Recent" });
  expect(within(picker).getAllByRole("option").map((option) => option.textContent))
    .toEqual(["First/tmp", "old.txt/work", "gone.txt/work"]);
  const search = within(picker).getByRole("textbox", { name: "Search recent files" });
  fireEvent.change(search, { target: { value: "OLD" } });
  expect(within(picker).getAllByRole("option")).toHaveLength(1);
  responses["file.open"] = [{ documentId: "document-9", name: "old.txt", byteLen: "1", revision: "0", encoding: "utf8", sourcePath: "/work/old.txt" }];
  fireEvent.keyDown(search, { key: "Enter" });
  expect(await screen.findByText("old.txt editor")).toBeInTheDocument();
  expect(screen.queryByRole("dialog", { name: "Open Recent" })).toBeNull();

  // A file that can no longer be opened leaves the list.
  responses["file.open"] = [{ then: (_resolve: unknown, reject: (error: Error) => void) => reject(new Error("No such file")) }];
  await chooseMenu("File", "Open Recent…");
  fireEvent.click(within(screen.getByRole("dialog", { name: "Open Recent" })).getByRole("option", { name: /gone\.txt/ }));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Could not open gone.txt; it was removed from Recent Files"));
  expect(JSON.parse(window.localStorage.getItem("nulis.recentFiles.v1") ?? "[]")).toEqual(["/work/old.txt", "/tmp/First"]);
});

test("Save All saves every changed file, Rename renames the active one, and Close All closes everything", async () => {
  const make = (index: number, name: string) => ({
    documentId: `document-${index}`, name, byteLen: "1", revision: "1", encoding: "utf8", sourcePath: `/tmp/${name}`, dirty: true,
  });
  responses["file.new"] = [make(1, "First"), make(2, "Second")];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("First editor");
  fireEvent.click(screen.getByRole("button", { name: "New tab" }));
  await screen.findByText("Second editor");

  responses["file.save"] = [{ ...make(1, "First"), dirty: false }, { ...make(2, "Second"), dirty: false }];
  await chooseMenu("File", "Save All");
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Saved 2 files"));
  expect(vi.mocked(executeCommand).mock.calls.filter(([request]) => request.id === "file.save")
    .map(([request]) => request.context.documentId)).toEqual(["document-1", "document-2"]);

  await chooseMenu("File", "Rename File…");
  const dialog = screen.getByRole("dialog", { name: "Rename Second" });
  const field = within(dialog).getByRole("textbox", { name: "New file name" });
  expect(field).toHaveValue("Second");
  fireEvent.change(field, { target: { value: "Renamed.md" } });
  responses["file.renameTo"] = [{ ...make(2, "Renamed.md"), dirty: false, sourcePath: "/tmp/Renamed.md" }];
  fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));
  expect(await screen.findByText("Renamed.md editor")).toBeInTheDocument();
  expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "file.renameTo", context: expect.objectContaining({ documentId: "document-2" }), arguments: { name: "Renamed.md" },
  }));
  expect(JSON.parse(window.localStorage.getItem("nulis.recentFiles.v1") ?? "[]")).toEqual(["/tmp/Renamed.md", "/tmp/First"]);

  await chooseMenu("File", "Show in Places");
  expect(screen.getByRole("tab", { name: "Places" })).toHaveAttribute("aria-selected", "true");

  await chooseMenu("File", "Close All Files");
  await waitFor(() => expect(screen.queryByText(/ editor$/)).toBeNull());
});

test("Quick Open finds open files, folder files, and recent files", async () => {
  window.localStorage.setItem("nulis.recentFiles.v1", JSON.stringify(["/elsewhere/notes.md"]));
  vi.mocked(open).mockResolvedValueOnce("/tmp/project");
  responses["workspace.open"] = [{ root: "/tmp/project", name: "project" }];
  responses["workspace.list"] = [{ relativePath: "", entries: [], truncated: false }];
  responses["workspace.listFiles"] = [{ files: ["README.md", "src/main.rs", "src/notes.rs"], truncated: false }];
  responses["workspace.openFile"] = [{
    documentId: "document-7", name: "main.rs", byteLen: "1", revision: "0", encoding: "utf8", sourcePath: "/tmp/project/src/main.rs",
  }];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "Open a folder" }));
  await screen.findByRole("tree", { name: "project files" });

  fireEvent.keyDown(window, { key: "p", code: "KeyP", ctrlKey: true });
  const picker = await screen.findByRole("dialog", { name: "Quick Open" });
  await waitFor(() => expect(within(picker).getAllByRole("option")).toHaveLength(4));
  const search = within(picker).getByRole("textbox", { name: "Search files" });
  fireEvent.change(search, { target: { value: "notes" } });
  // The folder's notes.rs and the recent notes.md both match by name.
  expect(within(picker).getAllByRole("option").map((option) => option.textContent))
    .toEqual(["notes.rssrc", "notes.mdRecent · /elsewhere"]);
  fireEvent.change(search, { target: { value: "main" } });
  fireEvent.keyDown(search, { key: "Enter" });
  expect(await screen.findByText("main.rs editor")).toBeInTheDocument();
  expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({ id: "workspace.openFile", arguments: { path: "src/main.rs" } }));
  expect(screen.queryByRole("dialog", { name: "Quick Open" })).toBeNull();
});

test("Paste from History lists this session's copies and pastes the chosen one", async () => {
  vi.mocked(listCommands).mockResolvedValueOnce([
    { id: "edit.pasteFromHistory", title: "Paste from History…", category: "Edit" },
  ]);
  await openDocuments("First");
  fireEvent.click(screen.getByRole("button", { name: "Copy in First" }));
  fireEvent.click(screen.getByRole("button", { name: "Open command palette" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Search commands" }), { target: { value: "paste from history" } });
  fireEvent.click(await screen.findByRole("option", { name: /Paste from History/ }));
  const picker = await screen.findByRole("dialog", { name: "Paste from History" });
  const option = within(picker).getByRole("option");
  expect(option).toHaveTextContent("copied from First");
  expect(option).toHaveTextContent("29 characters");
  fireEvent.click(option);
  await waitFor(() => expect(screen.getByText("First editor"))
    .toHaveAttribute("data-inserted", "copied from First\nsecond line"));
});

test("the About window uses the light batik with a light theme", async () => {
  storedCustomization.document.appearance.theme = "nulis.paper-light";
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  await chooseMenu("Help", "About Nulis");
  const about = await screen.findByRole("dialog", { name: "Nulis" });
  expect(about).toHaveAttribute("data-batik", "light");
  expect(about.style.getPropertyValue("--about-batik")).toMatch(/about-batik-light/);
});

test("a key the editor handled is not run again by the window's shortcuts", async () => {
  // The real defaults bind Primary Z to undo, which the editor also handles itself.
  storedCustomization.document.keybindings = [{ key: "Primary+Z", command: "editor.undo" }];
  vi.mocked(listCommands).mockResolvedValueOnce([
    { id: "file.new", title: "New Text File", category: "File" },
    { id: "editor.undo", title: "Undo", category: "Editor" },
  ]);
  await openDocuments("First");
  const undone = { documentId: "document-1", name: "First", byteLen: "0", revision: "1", encoding: "utf8", sourcePath: "/tmp/First" };
  responses["editor.undo"] = [undone, undone];
  vi.mocked(executeCommand).mockClear();
  fireEvent.keyDown(screen.getByRole("textbox", { name: "Keys in First" }), { key: "z", ctrlKey: true });
  await settle();
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "editor.undo" }));
  // Outside the editor the shortcut still works.
  fireEvent.keyDown(window, { key: "z", ctrlKey: true });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({ id: "editor.undo" })));
});

test("closing a picker or the Rename dialog puts focus back where it was", async () => {
  await openDocuments("First");
  const keys = screen.getByRole("textbox", { name: "Keys in First" });
  keys.focus();
  fireEvent.keyDown(keys, { key: "p", code: "KeyP", ctrlKey: true });
  const search = await screen.findByRole("textbox", { name: "Search files" });
  expect(search).toHaveFocus();
  fireEvent.keyDown(search, { key: "Escape" });
  await waitFor(() => expect(keys).toHaveFocus());

  await chooseMenu("File", "Rename File…");
  const field = screen.getByRole("textbox", { name: "New file name" });
  // Tab from the last button wraps to the field, Shift Tab from the field stays inside.
  const dialog = screen.getByRole("dialog", { name: "Rename First" });
  within(dialog).getByRole("button", { name: "Rename" }).focus();
  fireEvent.keyDown(dialog, { key: "Tab" });
  expect(field).toHaveFocus();
  fireEvent.keyDown(dialog, { key: "Tab", shiftKey: true });
  expect(within(dialog).getByRole("button", { name: "Rename" })).toHaveFocus();
  keys.focus();
  fireEvent.keyDown(dialog, { key: "Escape" });
  expect(screen.queryByRole("dialog", { name: "Rename First" })).toBeNull();
});

test("the tab strip offers scroll arrows only when its tabs overflow", async () => {
  await openDocuments("First", "Second", "Third");
  const strip = screen.getByRole("tablist", { name: "Open documents" });
  expect(screen.queryByRole("button", { name: "Scroll tabs right" })).toBeNull();
  expect(strip.querySelector(".welcome-dot")).toBeNull();

  Object.defineProperty(strip, "clientWidth", { configurable: true, value: 200 });
  Object.defineProperty(strip, "scrollWidth", { configurable: true, value: 600 });
  fireEvent.scroll(strip);
  expect(screen.getByRole("button", { name: "Scroll tabs left" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Scroll tabs right" }));
  expect(strip.scrollLeft).toBe(120);
  fireEvent.scroll(strip);
  expect(screen.getByRole("button", { name: "Scroll tabs left" })).toBeEnabled();
  // The new-tab button stays reachable beside the arrows.
  expect(strip.contains(screen.getByRole("button", { name: "New tab" }))).toBe(false);
});

test("panes leave a workspace by drag or button, and tabs reorder in the strip", async () => {
  await openDocuments("First", "Second", "Third");
  dragTabTo("First", panes()[0], 780, 300);
  fireEvent.keyDown(screen.getByRole("textbox", { name: "Workspace name" }), { key: "Enter" });
  expect(tabNames()).toEqual(["Second", "Workspace"]);

  // Drag a pane header onto the tab strip: it becomes a tab of its own again.
  const strip = screen.getByRole("tablist", { name: "Open documents" });
  for (const [index, item] of [...strip.querySelectorAll<HTMLElement>("[data-tab-id]")].entries()) {
    item.getBoundingClientRect = () => ({ left: index * 100, width: 100 }) as DOMRect;
  }
  fireEvent.pointerDown(panes()[1].querySelector(".pane-header")!, { button: 0, clientX: 600, clientY: 60 });
  fireEvent.pointerMove(strip, { clientX: 20, clientY: 10 });
  fireEvent.pointerUp(window);
  expect(tabNames()).toEqual(["First", "Second", "Third"]);
  expect(paneNames()).toEqual(["Editor"]);
  expect(screen.getByText("First editor")).toBeInTheDocument();

  // Regroup, then rearrange panes inside the workspace by dragging a header.
  fireEvent.click(screen.getByRole("tab", { name: "Third" }));
  dragTabTo("First", panes()[0], 780, 300);
  fireEvent.keyDown(screen.getByRole("textbox", { name: "Workspace name" }), { key: "Enter" });
  expect(paneNames()).toEqual(["Pane 1 of 2: Third", "Pane 2 of 2: First"]);
  panes()[0].getBoundingClientRect = () => ({ left: 0, top: 0, width: 400, height: 600 }) as DOMRect;
  fireEvent.pointerDown(panes()[1].querySelector(".pane-header")!, { button: 0, clientX: 600, clientY: 60 });
  fireEvent.pointerMove(panes()[0].querySelector(".editor-pane")!, { clientX: 10, clientY: 300 });
  expect(document.querySelector('[data-drop-zone="left"]')).not.toBeNull();
  fireEvent.pointerUp(window);
  expect(paneNames()).toEqual(["Pane 1 of 2: First", "Pane 2 of 2: Third"]);

  // The pane's own control pops it out next to the workspace.
  fireEvent.click(screen.getByRole("button", { name: "Move Third to its own tab" }));
  expect(tabNames()).toEqual(["Second", "First", "Third"]);

  // Dragging a tab along the strip reorders it.
  for (const [index, item] of [...strip.querySelectorAll<HTMLElement>("[data-tab-id]")].entries()) {
    item.getBoundingClientRect = () => ({ left: index * 100, width: 100 }) as DOMRect;
  }
  fireEvent.pointerDown(screen.getByRole("tab", { name: "Third" }).parentElement!, { button: 0, clientX: 250, clientY: 10 });
  fireEvent.pointerMove(strip, { clientX: 20, clientY: 10 });
  fireEvent.pointerUp(window);
  expect(tabNames()).toEqual(["Third", "Second", "First"]);
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "file.close" }));
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "file.save" }));
});

test("every workspace operation is also a keyboard-reachable command", async () => {
  await openDocuments("First", "Second", "Third");
  fireEvent.click(screen.getByRole("tab", { name: "First" }));
  fireEvent.click(screen.getByRole("button", { name: "Open Workspace menu" }));
  expect(within(screen.getByRole("menu", { name: "Workspace" })).getByRole("menuitem", { name: "Rename Workspace" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Open Workspace menu" }));

  await chooseMenu("Workspace", "Group with Next Tab Side by Side");
  expect(paneNames()).toEqual(["Pane 1 of 2: First", "Pane 2 of 2: Second"]);
  fireEvent.keyDown(screen.getByRole("textbox", { name: "Workspace name" }), { key: "Escape" });
  await chooseMenu("Workspace", "Group with Next Tab Stacked");
  expect(paneNames()).toEqual(["Pane 1 of 3: First", "Pane 2 of 3: Second", "Pane 3 of 3: Third"]);
  expect(screen.getByRole("separator", { name: "Resize panes vertically" })).toBeInTheDocument();
  expect(tabNames()).toEqual(["Workspace"]);
  // With no tab left to group, the command is unavailable.
  fireEvent.click(screen.getByRole("button", { name: "Open Workspace menu" }));
  expect(within(screen.getByRole("menu", { name: "Workspace" })).getByRole("menuitem", { name: "Group with Next Tab Stacked" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Open Workspace menu" }));

  await chooseMenu("Workspace", "Rename Workspace");
  const input = screen.getByRole("textbox", { name: "Workspace name" });
  fireEvent.change(input, { target: { value: "Release" } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(tabNames()).toEqual(["Release"]);

  await chooseMenu("Workspace", "Move Pane to New Tab");
  expect(tabNames()).toEqual(["Release", "Third"]);
  await chooseMenu("Workspace", "Move Tab Left");
  expect(tabNames()).toEqual(["Third", "Release"]);
  await chooseMenu("Workspace", "Next Tab");
  expect(screen.getByRole("tab", { name: "Release" })).toHaveAttribute("aria-selected", "true");
  await chooseMenu("Workspace", "Ungroup Workspace");
  expect(tabNames()).toEqual(["Third", "First", "Second"]);
  expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({ id: "workspace.ungroup" }));
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "file.close" }));
});

test("closing a workspace tab closes its panes and guards the last view of a dirty document", async () => {
  responses["file.new"] = [
    { documentId: "document-1", name: "Shared", byteLen: "0", revision: "0", encoding: "utf8", sourcePath: "/tmp/Shared" },
    { documentId: "document-2", name: "Draft", byteLen: "3", revision: "1", encoding: "utf8", dirty: true },
  ];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("Shared editor");
  // A second view of Shared, popped out, leaves Shared open in two tabs.
  fireEvent.click(screen.getByRole("button", { name: "Split editor" }));
  fireEvent.click(within(panes()[1]).getByRole("button", { name: "Move Shared to its own tab" }));
  fireEvent.click(screen.getByRole("button", { name: "New tab" }));
  await screen.findByText("Draft editor");
  expect(tabNames()).toEqual(["Shared", "Shared", "●Draft"]);
  // Workspace: Draft with the second Shared view.
  dragTabTo("Shared", panes()[0], 780, 300);
  fireEvent.keyDown(screen.getByRole("textbox", { name: "Workspace name" }), { key: "Enter" });
  expect(tabNames()).toEqual(["Shared", "●Workspace"]);

  fireEvent.click(screen.getByRole("button", { name: "Close Workspace" }));
  const prompt = screen.getByRole("dialog", { name: "Save changes to Draft?" });
  fireEvent.click(within(prompt).getByRole("button", { name: "Cancel" }));
  expect(tabNames()).toEqual(["Shared", "●Workspace"]);
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "file.close" }));

  fireEvent.click(screen.getByRole("button", { name: "Close Workspace" }));
  fireEvent.click(screen.getByRole("button", { name: "Discard" }));
  await waitFor(() => expect(tabNames()).toEqual(["Shared"]));
  expect(executeCommand).toHaveBeenCalledWith({
    id: "file.close", context: { windowId: "main", documentId: "document-2" }, arguments: { discard: true },
  });
  // Shared still has a view in the remaining tab, so it was never closed.
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({
    id: "file.close", context: { windowId: "main", documentId: "document-1" },
  }));
  expect(await screen.findByText("Shared editor")).toBeInTheDocument();
});

test("restores tabs, named workspaces, and hexadecimal mode, removing only missing panes", async () => {
  const bytes = "path:/tmp/bytes.bin";
  const notes = "path:/tmp/notes.md";
  const missing = "path:/tmp/missing.txt";
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "session.load") return {
      ...DEFAULT_SESSION,
      documents: [bytes, notes, missing],
      layouts: [{
        id: "tab-1", name: "Workspace", activePane: "pane-1",
        root: { kind: "pane", id: "pane-1", tabs: [missing], activeTab: missing },
      }, {
        id: "tab-2", name: "Inspect", activePane: "pane-3",
        root: {
          kind: "split", direction: "horizontal", ratio: 300,
          first: { kind: "pane", id: "pane-2", tabs: [bytes], activeTab: bytes },
          second: {
            kind: "split", direction: "vertical", ratio: 500,
            first: { kind: "pane", id: "pane-3", tabs: [notes], activeTab: notes },
            second: { kind: "pane", id: "pane-4", tabs: [missing], activeTab: missing },
          },
        },
      }],
      activeLayout: "tab-2",
      viewModes: { [bytes]: "hex" },
    } as never;
    if (request.id === "file.open") {
      const path = (request.arguments as { path: string }).path;
      if (path === "/tmp/missing.txt") throw new Error("not found");
      return {
        documentId: path, name: path.split("/").pop(), byteLen: "4", revision: "0", encoding: "utf8", sourcePath: path,
      } as never;
    }
    if (request.id === "plugin.activateStartup") return [] as never;
    return null as never;
  });

  render(<App />);
  expect(await screen.findByText("bytes.bin hexadecimal editor")).toBeInTheDocument();
  expect(screen.getByText("notes.md editor")).toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent("Restored 2 files; 1 unavailable");
  // The tab and the pane that only held the missing file are gone; the workspace stays usable.
  expect(tabNames()).toEqual(["Inspect"]);
  expect(paneNames()).toEqual(["Pane 1 of 2: bytes.bin", "Pane 2 of 2: notes.md"]);
  expect(screen.getByRole("separator", { name: "Resize panes horizontally" })).toHaveAttribute("aria-valuenow", "30");
  expect(screen.getByRole("button", { name: "Close split view" })).toBeInTheDocument();
});

test("languages chosen with View As are restored and saved again; an unavailable one is skipped", async () => {
  const notes = "path:/tmp/notes.txt";
  const script = "path:/tmp/script.nim";
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "session.load") return {
      ...DEFAULT_SESSION,
      documents: [notes, script],
      layouts: [{
        id: "tab-1", name: "Workspace", activePane: "pane-1",
        root: { kind: "pane", id: "pane-1", tabs: [notes, script], activeTab: notes },
      }],
      activeLayout: "tab-1",
      languages: { [notes]: "markdown", [script]: "nim" },
    } as never;
    if (request.id === "file.open") {
      const path = (request.arguments as { path: string }).path;
      return {
        documentId: path, name: path.split("/").pop(), byteLen: "4", revision: "0", encoding: "utf8",
        sourcePath: path, languageId: "plaintext",
      } as never;
    }
    if (request.id === "language.set") {
      const { languageId } = request.arguments as { languageId: string };
      if (languageId === "nim") throw new Error("unknown language");
      return {
        documentId: request.context.documentId, name: "notes.txt", byteLen: "4", revision: "0", encoding: "utf8",
        sourcePath: "/tmp/notes.txt", languageId, languageOverride: true,
      } as never;
    }
    if (request.id === "plugin.activateStartup") return [] as never;
    return null as never;
  });

  render(<App />);
  expect(await screen.findByText("notes.txt editor")).toBeInTheDocument();
  expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "language.set", context: { windowId: "main", documentId: "/tmp/notes.txt" }, arguments: { languageId: "markdown" },
  }));
  await waitFor(() => expect(saveRequests().length).toBeGreaterThan(0));
  const saved = saveRequests().at(-1);
  expect(saved?.languages).toEqual({ [notes]: "markdown" });
});

test("a recovered untitled document returns to the pane it had before the restart", async () => {
  const draft = "recovery:document-old";
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "session.load") return {
      ...DEFAULT_SESSION,
      documents: ["path:/tmp/a.txt", draft],
      layouts: [{
        id: "tab-1", name: "Notes", activePane: "pane-1",
        root: {
          kind: "split", direction: "horizontal", ratio: 500,
          first: { kind: "pane", id: "pane-1", tabs: ["path:/tmp/a.txt"], activeTab: "path:/tmp/a.txt" },
          second: { kind: "pane", id: "pane-2", tabs: [draft], activeTab: draft },
        },
      }],
      activeLayout: "tab-1",
    } as never;
    if (request.id === "file.open") return {
      documentId: "document-1", name: "a.txt", byteLen: "1", revision: "0", encoding: "utf8", sourcePath: "/tmp/a.txt",
    } as never;
    if (request.id === "recovery.list") return [{ recoveryId: "document-old", name: "Lost.md", sourcePath: null, byteLen: "7" }] as never;
    if (request.id === "recovery.restore") return {
      documentId: "document-7", name: "Lost.md", byteLen: "7", revision: "1", encoding: "utf8", dirty: true,
      recoveryId: "document-old",
    } as never;
    if (request.id === "plugin.activateStartup") return [] as never;
    return null as never;
  });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Restore" }));
  await waitFor(() => expect(paneNames()).toEqual(["Pane 1 of 2: a.txt", "Pane 2 of 2: Lost.md"]));
  expect(within(panes()[1]).getByText("Lost.md editor")).toBeInTheDocument();
  expect(tabNames()).toEqual(["●Notes"]);
  await waitFor(() => expect(saveRequests().at(-1)?.documents).toEqual(["path:/tmp/a.txt", draft]));
});

test("the window close button closes a clean window through the native command", async () => {
  let requestClose: ((event: { preventDefault: () => void }) => void) | undefined;
  vi.mocked(getCurrentWindow).mockReturnValue({
    onCloseRequested: vi.fn().mockImplementation(async (handler) => { requestClose = handler; return () => undefined; }),
  } as unknown as ReturnType<typeof getCurrentWindow>);
  vi.mocked(isTauri).mockReturnValue(true);
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  await waitFor(() => expect(requestClose).toBeDefined());
  const preventDefault = vi.fn();
  act(() => requestClose?.({ preventDefault }));
  // The webview cannot destroy its own window; the native command does it.
  expect(preventDefault).toHaveBeenCalled();
  await waitFor(() => expect(closeWindowAfterSave).toHaveBeenCalledOnce());
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

test("the title bar has left-aligned captioned tools and a command button at the far right", async () => {
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  expect(screen.queryByText("Untitled Workspace")).not.toBeInTheDocument();
  const toolbar = screen.getByRole("toolbar", { name: "Custom toolbar" });
  const save = within(toolbar).getByRole("button", { name: "Toolbar: Save" });
  expect(save).toHaveTextContent("Save");
  expect(save).toHaveAttribute("title", expect.stringMatching(/^Save \(/));
  expect(within(toolbar).getByRole("button", { name: "Open settings" })).toHaveTextContent("Settings");
  const palette = screen.getByRole("button", { name: "Open command palette" });
  expect(palette).toHaveTextContent("Commands");
  expect(palette).toHaveAttribute("title", expect.stringMatching(/^Search commands/));
  // Order in the bar: tools first, the command button last.
  expect(toolbar.compareDocumentPosition(palette) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(palette.parentElement?.lastElementChild).toBe(palette);
  fireEvent.click(palette);
  expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
});

test("Close File is in the File menu and on the toolbar, and closes the focused file", async () => {
  storedCustomization.document.toolbar = ["file.new", "workbench.closeFile"];
  storedCustomization.document.menu = [{ kind: "command", command: "file.save" }];
  vi.mocked(listCommands).mockResolvedValueOnce([
    { id: "file.new", title: "New Text File", category: "File" },
    { id: "file.save", title: "Save", category: "File" },
    { id: "workbench.closeFile", title: "Close File", category: "File" },
  ]);
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "session.load") return DEFAULT_SESSION as never;
    if (request.id === "customization.get") return storedCustomization.document as never;
    if (request.id === "plugin.activateStartup") return [] as never;
    const queue = responses[request.id];
    return (queue?.length ? queue.shift() : null) as never;
  });
  responses["file.new"] = [
    { documentId: "document-1", name: "First", byteLen: "0", revision: "0", encoding: "utf8" },
    { documentId: "document-2", name: "Second", byteLen: "0", revision: "0", encoding: "utf8" },
  ];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  const close = screen.getByRole("button", { name: "Toolbar: Close File" });
  expect(close).toBeDisabled();
  expect(close).toHaveTextContent("Close");

  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("First editor");
  fireEvent.click(screen.getByRole("button", { name: "New tab" }));
  await screen.findByText("Second editor");
  fireEvent.click(close);
  await waitFor(() => expect(tabNames()).toEqual(["First"]));
  expect(executeCommand).toHaveBeenCalledWith({
    id: "file.close", context: { windowId: "main", documentId: "document-2" }, arguments: { discard: false },
  });

  // The menu offers it even though this customized File menu does not list it.
  await chooseMenu("File", "Close File");
  await waitFor(() => expect(screen.queryByRole("tablist", { name: "Open documents" })?.querySelector('[role="tab"]')).toBeNull());
  expect(screen.getByRole("heading", { name: /write, inspect/i })).toBeInTheDocument();
});

test("a file chosen in Places opens through the file command, and an open one is focused", async () => {
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "session.load") return DEFAULT_SESSION as never;
    if (request.id === "plugin.activateStartup") return [] as never;
    if (request.id === "places.roots") return [{ name: "Disk", path: "/", kind: "computer" }] as never;
    if (request.id === "places.list") return {
      path: "/", truncated: false,
      entries: [{ name: "main.rs", path: "/main.rs", kind: "file" }, { name: "lib.rs", path: "/lib.rs", kind: "file" }],
    } as never;
    if (request.id === "file.open") {
      const path = (request.arguments as { path: string }).path;
      return { documentId: path, name: path.slice(1), byteLen: "1", revision: "0", encoding: "utf8", sourcePath: path } as never;
    }
    return null as never;
  });
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  // Places is the default view; no folder has to be opened first.
  expect(screen.getByRole("tab", { name: "Places" })).toHaveAttribute("aria-selected", "true");
  const places = screen.getByRole("tree", { name: "Places" });
  fireEvent.click(await within(places).findByRole("button", { name: /Disk/ }));
  fireEvent.click(await within(places).findByRole("button", { name: /main\.rs/ }));
  expect(await screen.findByText("main.rs editor")).toBeInTheDocument();
  fireEvent.click(within(places).getByRole("button", { name: /lib\.rs/ }));
  expect(await screen.findByText("lib.rs editor")).toBeInTheDocument();
  const opens = () => vi.mocked(executeCommand).mock.calls.filter(([request]) => request.id === "file.open").length;
  expect(opens()).toBe(2);
  fireEvent.click(within(places).getByRole("button", { name: /main\.rs/ }));
  expect(await screen.findByText("main.rs editor")).toBeInTheDocument();
  expect(opens()).toBe(2);
  fireEvent.click(screen.getByRole("tab", { name: "Open" }));
  expect(within(screen.getByRole("tree", { name: "Open files" })).getAllByRole("button")).toHaveLength(2);
});

test("the Find and Replace panel drives find, replace, and replace all in the current editor", async () => {
  await openDocuments("First", "Second");
  const sent = () => JSON.parse(screen.getByText("Second editor").getAttribute("data-search") ?? "null");
  fireEvent.click(screen.getByRole("button", { name: "Replace in Second" }));
  const panel = screen.getByRole("dialog", { name: "Find and Replace" });
  expect(within(panel).getByRole("tab", { name: "Replace" })).toHaveAttribute("aria-selected", "true");

  fireEvent.click(within(panel).getByRole("button", { name: "Next" }));
  expect(screen.getByText("Enter text to find")).toBeInTheDocument();
  expect(sent()).toBeNull();

  const find = within(panel).getByRole("textbox", { name: "Find what" });
  fireEvent.change(find, { target: { value: "l(a|i)ne" } });
  fireEvent.click(within(panel).getByRole("checkbox", { name: "Use regular expressions" }));
  fireEvent.keyDown(find, { key: "Enter" });
  await waitFor(() => expect(sent()).toMatchObject({
    id: 1, action: "next", query: "l(a|i)ne", regex: true, caseSensitive: false, wholeWord: false,
  }));
  fireEvent.keyDown(find, { key: "Enter", shiftKey: true });
  await waitFor(() => expect(sent()).toMatchObject({ id: 2, action: "previous" }));

  fireEvent.change(within(panel).getByRole("textbox", { name: "Replace with" }), { target: { value: "row" } });
  fireEvent.click(within(panel).getByRole("checkbox", { name: "Match case" }));
  fireEvent.click(within(panel).getByRole("checkbox", { name: "Match whole word" }));
  fireEvent.click(within(panel).getByRole("button", { name: "Replace" }));
  await waitFor(() => expect(sent()).toMatchObject({
    id: 3, action: "replace", replacement: "row", caseSensitive: true, wholeWord: true,
  }));
  fireEvent.click(within(panel).getByRole("button", { name: "Replace All" }));
  await waitFor(() => expect(sent()).toMatchObject({ id: 4, action: "replaceAll" }));
  expect(sent().within).toBeUndefined();

  // The panel is not modal and closes with Escape.
  fireEvent.keyDown(find, { key: "Escape" });
  expect(screen.queryByRole("dialog", { name: "Find and Replace" })).not.toBeInTheDocument();
});

test("Count All and the lines list go to the chosen line and can bookmark every line", async () => {
  await openDocuments("Second");
  responses["search.findAll"] = [{
    revision: "0", truncated: false,
    matches: [
      { startByte: "0", endByte: "3", line: "1" },
      { startByte: "10", endByte: "13", line: "2" },
      { startByte: "14", endByte: "17", line: "2" },
    ],
    lines: [
      { line: "1", startByte: "0", text: "let a", truncated: false, matches: 1 },
      { line: "2", startByte: "6", text: "  let b let", truncated: false, matches: 2 },
    ],
  }];
  responses["editor.addBookmarks"] = [["0", "6"]];
  // Count All counts in the core; the lines list still comes from Find All.
  responses["search.countStart"] = [{ jobId: 1 }];
  responses["search.countProgress"] = [{
    matches: "3", lines: "2", scannedBytes: "20", totalBytes: "20", documents: 1, documentsWithMatches: 1, done: true, cancelled: false,
  }];
  fireEvent.click(screen.getByRole("button", { name: "Find in Second" }));
  const panel = screen.getByRole("dialog", { name: "Find and Replace" });
  expect(within(panel).getByRole("textbox", { name: "Find what" })).toHaveValue("seeded");
  fireEvent.change(within(panel).getByRole("textbox", { name: "Find what" }), { target: { value: "let" } });
  fireEvent.click(within(panel).getByRole("checkbox", { name: "List lines containing find string" }));
  fireEvent.click(within(panel).getByRole("button", { name: "Count All" }));

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "search.findAll", context: { windowId: "main", documentId: "document-1" },
    arguments: { query: "let", options: { mode: "literal", caseSensitive: false, wholeWord: false } },
  }));
  expect(await within(panel).findByText("3 matches on 2 lines")).toBeInTheDocument();
  const results = screen.getByRole("dialog", { name: "Search results" });
  const options = within(results).getAllByRole("option");
  expect(options.map((option) => option.textContent)).toEqual(["1let a", "2  let b let"]);
  fireEvent.click(options[1]!);
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "editor.setSelections", context: { windowId: "main", documentId: "document-1" },
    arguments: { selections: [{ anchorByte: "10", activeByte: "13" }] },
  }));
  fireEvent.click(within(results).getByRole("button", { name: "Bookmark All" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "editor.addBookmarks", context: { windowId: "main", documentId: "document-1" },
    arguments: { offsets: ["0", "6"] },
  }));
  fireEvent.click(within(results).getByRole("checkbox", { name: "Show line numbers" }));
  expect(within(results).getAllByRole("option")[0]).toHaveTextContent(/^let a$/);
});

test("Replace All can cover every open file and refuses an empty selection scope", async () => {
  await openDocuments("First", "Second");
  responses["search.replaceAll"] = [
    { document: { documentId: "document-1", name: "First", byteLen: "3", revision: "1", encoding: "utf8" }, replacements: 2 },
    { document: { documentId: "document-2", name: "Second", byteLen: "3", revision: "1", encoding: "utf8" }, replacements: 0 },
  ];
  fireEvent.click(screen.getByRole("button", { name: "Replace in Second" }));
  const panel = screen.getByRole("dialog", { name: "Find and Replace" });
  fireEvent.change(within(panel).getByRole("textbox", { name: "Find what" }), { target: { value: "a" } });
  fireEvent.change(within(panel).getByRole("textbox", { name: "Replace with" }), { target: { value: "b" } });

  fireEvent.change(within(panel).getByRole("combobox", { name: "Replace where" }), { target: { value: "selection" } });
  fireEvent.click(within(panel).getByRole("button", { name: "Replace All" }));
  expect(await screen.findByText("Select the text to replace in first")).toBeInTheDocument();

  fireEvent.change(within(panel).getByRole("combobox", { name: "Replace where" }), { target: { value: "openFiles" } });
  expect(within(panel).getByRole("button", { name: "Replace" })).toBeDisabled();
  fireEvent.click(within(panel).getByRole("button", { name: "Replace All" }));
  await waitFor(() => expect(within(panel).getByText("Replaced 2 matches in 1 file")).toBeInTheDocument());
  for (const documentId of ["document-1", "document-2"]) {
    expect(executeCommand).toHaveBeenCalledWith({
      id: "search.replaceAll", context: { windowId: "main", documentId },
      arguments: { expectedRevision: "0", query: "a", replacement: "b",
        options: { mode: "literal", caseSensitive: false, wholeWord: false } },
    });
  }
});

test("right-clicking a tab offers close, close others, save, and save as", async () => {
  responses["file.new"] = [
    { documentId: "document-1", name: "First", byteLen: "0", revision: "0", encoding: "utf8", sourcePath: "/tmp/First" },
    { documentId: "document-2", name: "Draft", byteLen: "3", revision: "1", encoding: "utf8", dirty: true },
    { documentId: "document-3", name: "Third", byteLen: "0", revision: "0", encoding: "utf8", sourcePath: "/tmp/Third" },
  ];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("First editor");
  for (const name of ["Draft", "Third"]) {
    fireEvent.click(screen.getByRole("button", { name: "New tab" }));
    await screen.findByText(`${name} editor`);
  }
  const menuFor = (name: string | RegExp) => {
    fireEvent.contextMenu(screen.getByRole("tab", { name }), { clientX: 120, clientY: 40 });
    return screen.getByRole("menu");
  };

  const first = menuFor("First");
  expect(within(first).getAllByRole("menuitem").map((item) => item.querySelector("span")?.textContent)).toEqual([
    "Close", "Close All Files Except This", "Save", "Save As…", "Copy File Path",
  ]);
  // A saved, unchanged file has nothing to save.
  expect(within(first).getByRole("menuitem", { name: /^Save(?! As)/ })).toBeDisabled();
  fireEvent.keyDown(window, { key: "Escape" });
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();

  // Save As on a background tab saves that file, not the active one.
  vi.mocked(save).mockResolvedValueOnce("/tmp/draft.md");
  responses["file.saveAs"] = [{
    documentId: "document-2", name: "draft.md", byteLen: "3", revision: "1", encoding: "utf8", sourcePath: "/tmp/draft.md",
  }];
  fireEvent.click(within(menuFor(/Draft/)).getByRole("menuitem", { name: /Save As/ }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "file.saveAs", context: { windowId: "main", documentId: "document-2" },
    arguments: { expectedRevision: "1", path: "/tmp/draft.md" },
  }));
  expect(await screen.findByRole("tab", { name: "draft.md" })).toBeInTheDocument();

  fireEvent.click(within(menuFor("draft.md")).getByRole("menuitem", { name: "Close All Files Except This" }));
  await waitFor(() => expect(tabNames()).toEqual(["draft.md"]));
  expect(screen.getByText("draft.md editor")).toBeInTheDocument();
  expect(vi.mocked(executeCommand).mock.calls.filter(([request]) => request.id === "file.close")
    .map(([request]) => request.context?.documentId)).toEqual(["document-1", "document-3"]);

  fireEvent.click(within(menuFor("draft.md")).getByRole("menuitem", { name: /^Close(?! All)/ }));
  await waitFor(() => expect(tabNames()).toEqual([]));
});

test("closing all other tabs stops at an unsaved file until it is resolved", async () => {
  responses["file.new"] = [
    { documentId: "document-1", name: "Keep", byteLen: "0", revision: "0", encoding: "utf8" },
    { documentId: "document-2", name: "Draft", byteLen: "3", revision: "1", encoding: "utf8", dirty: true },
    { documentId: "document-3", name: "Clean", byteLen: "0", revision: "0", encoding: "utf8" },
  ];
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("Keep editor");
  for (const name of ["Draft", "Clean"]) {
    fireEvent.click(screen.getByRole("button", { name: "New tab" }));
    await screen.findByText(`${name} editor`);
  }
  fireEvent.contextMenu(screen.getByRole("tab", { name: "Keep" }), { clientX: 10, clientY: 10 });
  fireEvent.click(screen.getByRole("menuitem", { name: "Close All Files Except This" }));
  const prompt = await screen.findByRole("dialog", { name: "Save changes to Draft?" });
  fireEvent.click(within(prompt).getByRole("button", { name: "Cancel" }));
  expect(tabNames()).toEqual(["Keep", "●Draft", "Clean"]);

  fireEvent.contextMenu(screen.getByRole("tab", { name: "Keep" }), { clientX: 10, clientY: 10 });
  fireEvent.click(screen.getByRole("menuitem", { name: "Close All Files Except This" }));
  fireEvent.click(within(await screen.findByRole("dialog", { name: "Save changes to Draft?" })).getByRole("button", { name: "Discard" }));
  await waitFor(() => expect(tabNames()).toEqual(["Keep"]));
});

test("JSON and XML tools appear only for those documents and validate, beautify, and minify", async () => {
  responses["file.new"] = [
    { documentId: "document-1", name: "notes.txt", byteLen: "2", revision: "0", encoding: "utf8", languageId: "plaintext" },
    { documentId: "document-2", name: "data.json", byteLen: "9", revision: "3", encoding: "utf8", languageId: "json", capabilities: { validate: "JSON", format: "JSON", compact: "JSON" } },
    { documentId: "document-3", name: "feed.xml", byteLen: "9", revision: "0", encoding: "utf8", languageId: "xml", capabilities: { validate: "XML", format: "XML" } },
  ];
  vi.mocked(isTauri).mockReturnValue(false);
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  const toolbar = screen.getByRole("toolbar", { name: "Custom toolbar" });
  const tool = (name: string) => within(toolbar).queryByRole("button", { name });

  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("notes.txt editor");
  // A plain text document has no tools: no buttons, no Tools menu, no palette entries.
  expect(tool("Validate JSON") ?? tool("Validate XML")).toBeNull();
  expect(screen.queryByRole("button", { name: "Open Tools menu" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "New tab" }));
  await screen.findByText("data.json editor");
  expect(tool("Validate JSON")).toHaveTextContent("Validate");
  expect(tool("Beautify JSON")).toHaveTextContent("Beautify");
  expect(tool("Minify JSON")).toHaveTextContent("Minify");

  // Validate reports the problem and moves the caret to it.
  responses["tools.validate"] = [{
    valid: false, message: "Invalid JSON: expected value (line 2, column 10)", byteOffset: "17", line: 2, column: 10,
  }];
  fireEvent.click(tool("Validate JSON")!);
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Invalid JSON: expected value (line 2, column 10)"));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "editor.setSelections", context: { windowId: "main", documentId: "document-2" },
    arguments: { selections: [{ anchorByte: "17", activeByte: "17" }] },
  }));

  // Beautify runs against the observed revision and reports the change.
  responses["tools.beautify"] = [
    { documentId: "document-2", name: "data.json", byteLen: "30", revision: "4", encoding: "utf8", languageId: "json", capabilities: { validate: "JSON", format: "JSON", compact: "JSON" }, dirty: true },
    { documentId: "document-2", name: "data.json", byteLen: "30", revision: "4", encoding: "utf8", languageId: "json", capabilities: { validate: "JSON", format: "JSON", compact: "JSON" }, dirty: true },
  ];
  fireEvent.click(tool("Beautify JSON")!);
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Beautified JSON"));
  expect(executeCommand).toHaveBeenCalledWith({
    id: "tools.beautify", context: { windowId: "main", documentId: "document-2" }, arguments: { expectedRevision: "3" },
  });
  await chooseMenu("Tools", "Beautify JSON");
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("JSON is already formatted"));
  expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "tools.beautify", arguments: { expectedRevision: "4" },
  }));

  // XML has Validate and Beautify, but no Minify.
  fireEvent.click(screen.getByRole("button", { name: "New tab" }));
  await screen.findByText("feed.xml editor");
  expect(tool("Validate XML")).not.toBeNull();
  expect(tool("Beautify XML")).not.toBeNull();
  expect(tool("Minify JSON")).toBeNull();
  responses["tools.validate"] = [{ valid: true, message: "Valid XML" }];
  await chooseMenu("Tools", "Validate XML");
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Valid XML"));

  // Back on the text file the tools are gone again.
  fireEvent.click(screen.getByRole("tab", { name: "notes.txt" }));
  expect(tool("Validate XML") ?? tool("Validate JSON")).toBeNull();
});

test("YAML Beautify asks before losing comments, and plist has Validate and Beautify", async () => {
  responses["file.new"] = [
    { documentId: "document-1", name: "ci.yml", byteLen: "40", revision: "2", encoding: "utf8", languageId: "yaml", capabilities: { validate: "YAML", format: "YAML", formatMayLoseData: true } },
    { documentId: "document-2", name: "Info.plist", byteLen: "60", revision: "0", encoding: "utf8", languageId: "plist", capabilities: { validate: "plist", format: "plist" } },
  ];
  vi.mocked(isTauri).mockReturnValue(false);
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  const toolbar = screen.getByRole("toolbar", { name: "Custom toolbar" });
  const tool = (name: string) => within(toolbar).queryByRole("button", { name });

  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("ci.yml editor");
  expect(tool("Validate YAML")).not.toBeNull();
  expect(tool("Minify JSON")).toBeNull();

  // Beautify checks first; with comments it asks, and Cancel changes nothing.
  const note = "Beautify removes comments; values and key order are kept, but quoting and number spelling may change.";
  responses["tools.validate"] = [{ valid: true, message: "Valid YAML", beautifyNote: note }];
  fireEvent.click(tool("Beautify YAML")!);
  const dialog = await screen.findByRole("dialog", { name: "Beautify ci.yml?" });
  expect(dialog).toHaveTextContent("removes comments");
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog", { name: "Beautify ci.yml?" })).not.toBeInTheDocument();
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "tools.beautify" }));

  // Agreeing runs Beautify with the loss accepted.
  responses["tools.validate"] = [{ valid: true, message: "Valid YAML", beautifyNote: note }];
  responses["tools.beautify"] = [
    { documentId: "document-1", name: "ci.yml", byteLen: "30", revision: "3", encoding: "utf8", languageId: "yaml", capabilities: { validate: "YAML", format: "YAML", formatMayLoseData: true }, dirty: true },
  ];
  fireEvent.click(tool("Beautify YAML")!);
  fireEvent.click(within(await screen.findByRole("dialog", { name: "Beautify ci.yml?" })).getByRole("button", { name: "Beautify" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "tools.beautify", context: { windowId: "main", documentId: "document-1" },
    arguments: { expectedRevision: "2", acceptLoss: true },
  }));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Beautified YAML"));

  // Tags make Beautify unavailable, with the reason.
  responses["tools.validate"] = [{
    valid: true, message: "Valid YAML", beautifyUnavailable: true,
    beautifyNote: "Beautify cannot keep YAML tags such as !Ref, so it is not available for this document.",
  }];
  fireEvent.click(tool("Beautify YAML")!);
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("cannot keep YAML tags"));

  fireEvent.click(screen.getByRole("button", { name: "New tab" }));
  await screen.findByText("Info.plist editor");
  expect(tool("Validate plist")).not.toBeNull();
  expect(tool("Beautify plist")).not.toBeNull();
  expect(tool("Minify JSON")).toBeNull();
});

test("a binary property list opens in the hexadecimal editor", async () => {
  responses["file.new"] = [{
    documentId: "document-1", name: "Preferences.plist", byteLen: "40", revision: "0", encoding: "utf8",
    languageId: "plist", readOnly: true, binaryFormat: "binary property list",
  }];
  vi.mocked(isTauri).mockReturnValue(false);
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await waitFor(() => expect(screen.getByRole("status"))
    .toHaveTextContent("Preferences.plist is a binary property list: shown read-only in the hexadecimal editor"));
  expect(screen.getByRole("button", { name: "Show text editor" })).toBeInTheDocument();
  // No text tools for bytes.
  expect(within(screen.getByRole("toolbar", { name: "Custom toolbar" })).queryByRole("button", { name: "Validate plist" })).toBeNull();
});

test("an invalid document is not reformatted, and the native Tools menu follows the focused document", async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "session.load") return DEFAULT_SESSION as never;
    if (request.id === "plugin.activateStartup") return [] as never;
    if (request.id === "file.new") return {
      documentId: "document-1", name: "feed.xml", byteLen: "9", revision: "0", encoding: "utf8", languageId: "xml", capabilities: { validate: "XML", format: "XML" },
    } as never;
    if (request.id === "tools.beautify") throw new Error("Invalid XML: <a> is never closed (line 1, column 1)");
    if (request.id === "tools.validate") return {
      valid: false, message: "Invalid XML: <a> is never closed (line 1, column 1)", byteOffset: "0", line: 1, column: 1,
    } as never;
    return null as never;
  });
  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  await waitFor(() => expect(setToolsMenu).toHaveBeenLastCalledWith({ validate: undefined, beautify: undefined, minify: undefined }));
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("feed.xml editor");
  await waitFor(() => expect(setToolsMenu).toHaveBeenLastCalledWith({ validate: "XML", beautify: "XML", minify: undefined }));

  fireEvent.click(screen.getByRole("button", { name: "Beautify XML" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({ id: "editor.setSelections" })));
  expect(screen.getByRole("status")).toHaveTextContent("Invalid XML: <a> is never closed (line 1, column 1)");
  // Hexadecimal view is not text: the tools step aside.
  fireEvent.click(screen.getByRole("button", { name: "Show hexadecimal editor" }));
  await waitFor(() => expect(setToolsMenu).toHaveBeenLastCalledWith({ validate: undefined, beautify: undefined, minify: undefined }));
  expect(screen.queryByRole("button", { name: "Beautify XML" })).not.toBeInTheDocument();
});

test("shows language-server problems in the status bar and jumps to one", async () => {
  responses["file.new"] = [{
    documentId: "document-1", name: "main.rs", byteLen: "10", revision: "1", encoding: "utf8",
    capabilities: { languageServer: "fake-lsp" },
  }];
  responses["lsp.diagnostics"] = [{
    server: "fake-lsp", revision: "1", problems: [
      { startByte: "4", endByte: "8", line: 2, severity: "error", message: "mismatched types", source: "fake-lsp" },
    ],
  }];

  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("main.rs editor");

  // Problems arrive shortly after the text, so the status bar fills in later.
  const status = await screen.findByRole("button", { name: "1 problem in main.rs" });
  expect(status).toHaveClass("error");
  fireEvent.click(status);
  const panel = await screen.findByRole("region", { name: "Problems in main.rs" });
  expect(within(panel).getByText("mismatched types")).toBeInTheDocument();
  expect(within(panel).getByText("fake-lsp", { selector: ".problems-server" })).toBeInTheDocument();

  fireEvent.click(within(panel).getByRole("button", { name: "Error on line 2: mismatched types" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.setSelections",
    context: { windowId: "main", documentId: "document-1" },
    arguments: { selections: [{ anchorByte: "4", activeByte: "8" }] },
  })));
  fireEvent.click(within(panel).getByRole("button", { name: "Close problems" }));
  expect(screen.queryByRole("region", { name: "Problems in main.rs" })).not.toBeInTheDocument();
});

test("asks no language server for a document without one", async () => {
  responses["file.new"] = [{
    documentId: "document-1", name: "notes.txt", byteLen: "0", revision: "1", encoding: "utf8",
  }];

  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("notes.txt editor");
  await settle();
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 600)); });
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "lsp.diagnostics" }));
  expect(screen.queryByRole("button", { name: /problem.* in notes.txt/ })).not.toBeInTheDocument();
});

test("ignores problems placed in a different revision of the text", async () => {
  responses["file.new"] = [{
    documentId: "document-1", name: "main.rs", byteLen: "10", revision: "2", encoding: "utf8",
    capabilities: { languageServer: "fake-lsp" },
  }];
  // The server answered for the text as it was before the latest edit.
  responses["lsp.diagnostics"] = [{
    server: "fake-lsp", revision: "1", problems: [
      { startByte: "0", endByte: "1", line: 1, severity: "error", message: "stale", source: "fake-lsp" },
    ],
  }];

  render(<App />);
  await screen.findByText("Nulis 0.1.0");
  fireEvent.click(screen.getByRole("button", { name: "New text file" }));
  await screen.findByText("main.rs editor");
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({ id: "lsp.diagnostics" })));
  await settle();
  // A server is set up, so the button exists, but the stale answer is not shown.
  expect(screen.getByRole("button", { name: "No problems in main.rs" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "No problems in main.rs" }));
  const panel = await screen.findByRole("region", { name: "Problems in main.rs" });
  expect(within(panel).getByText("fake-lsp reports no problems in this document.")).toBeInTheDocument();
  expect(within(panel).queryByText("stale")).not.toBeInTheDocument();
});

test("approves a language server in Settings", async () => {
  const server = {
    id: "rust", command: ["rust-analyzer"], languages: ["rust"],
    approved: false, running: [], stopped: false,
  };
  responses["lsp.state"] = [{ servers: [server], configError: null }];
  responses["lsp.approve"] = [{ servers: [{ ...server, approved: true }], configError: null }];

  const dialog = await openSettings();
  fireEvent.click(within(dialog).getByRole("tab", { name: "Languages" }));
  expect(await within(dialog).findByText("rust-analyzer")).toBeInTheDocument();
  expect(within(dialog).getByText("Not approved")).toBeInTheDocument();

  fireEvent.click(within(dialog).getByRole("button", { name: "Approve" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "lsp.approve", context: { windowId: "main" }, arguments: { id: "rust" },
  }));
  expect(await within(dialog).findByText("Approved")).toBeInTheDocument();
  expect(within(dialog).getByRole("button", { name: "Revoke" })).toBeInTheDocument();
});
