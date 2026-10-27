import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeAll, beforeEach, expect, test, vi } from "vitest";
import type { ExecuteCommandRequest, TerminalOutput, TerminalSummary } from "./contracts";
import { executeCommand } from "./ipc";
import { TerminalPanel, type TerminalFont } from "./TerminalPanel";

// xterm measures glyphs and jsdom has no layout, and the panel reaches the shell only
// through commands. Both get stand-ins that record what the panel asked for.
const grids = vi.hoisted(() => {
  interface Grid {
    cols: number;
    rows: number;
    options: Record<string, unknown>;
    input: HTMLTextAreaElement | null;
    dataHandler: ((data: string) => void) | null;
    written: string[];
    focusCalls: number;
    disposed: boolean;
    selection: string;
    pasted: string[];
    actions: string[];
  }
  const created: Grid[] = [];
  const fits: number[] = [];
  const observerCallbacks: ResizeObserverCallback[] = [];

  class FakeFitAddon {
    private terminal: { cols: number; rows: number } | null = null;

    activate(terminal: { cols: number; rows: number }) { this.terminal = terminal; }
    fit() {
      const terminal = this.terminal;
      if (!terminal) return;
      terminal.cols = 90;
      terminal.rows = 24;
      fits.push(1);
    }
    dispose() {}
  }

  class FakeTerminal {
    options: Record<string, unknown>;
    cols = 80;
    rows = 20;
    input: HTMLTextAreaElement | null = null;
    dataHandler: ((data: string) => void) | null = null;
    written: string[] = [];
    focusCalls = 0;
    disposed = false;
    selection = "";
    pasted: string[] = [];
    actions: string[] = [];

    constructor(options: Record<string, unknown>) {
      this.options = { ...options };
      created.push(this);
    }

    // The real grid takes keyboard input through a hidden textarea inside its element.
    open(host: HTMLElement) {
      const grid = document.createElement("div");
      grid.className = "xterm";
      this.input = document.createElement("textarea");
      grid.append(this.input);
      host.append(grid);
    }

    loadAddon(addon: FakeFitAddon) { addon.activate(this); }

    focus() {
      this.focusCalls += 1;
      this.input?.focus();
    }

    onData(callback: (data: string) => void) {
      this.dataHandler = callback;
      return { dispose: () => { this.dataHandler = null; } };
    }

    write(data: Uint8Array | string) {
      this.written.push(typeof data === "string" ? data : new TextDecoder().decode(data));
    }

    getSelection() { return this.selection; }
    paste(text: string) { this.pasted.push(text); }
    selectAll() { this.actions.push("selectAll"); }
    clear() { this.actions.push("clear"); }

    dispose() {
      this.disposed = true;
      this.input?.remove();
    }
  }

  class ResizeObserverStub {
    constructor(callback: ResizeObserverCallback) { observerCallbacks.push(callback); }
    observe() {}
    unobserve() {}
    disconnect() {}
  }

  return { FakeFitAddon, FakeTerminal, ResizeObserverStub, created, fits, observerCallbacks };
});

vi.mock("@xterm/xterm", () => ({ Terminal: grids.FakeTerminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: grids.FakeFitAddon }));
vi.mock("./ipc", () => ({ executeCommand: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  readText: vi.fn(),
  writeText: vi.fn(),
}));

beforeAll(() => { vi.stubGlobal("ResizeObserver", grids.ResizeObserverStub); });
afterEach(() => {
  cleanup();
  (document.activeElement as HTMLElement | null)?.blur();
});

beforeEach(() => {
  grids.created.length = 0;
  grids.fits.length = 0;
  grids.observerCallbacks.length = 0;
  vi.mocked(executeCommand).mockReset();
  vi.mocked(readText).mockReset().mockResolvedValue("echo pasted");
  vi.mocked(writeText).mockReset().mockResolvedValue(undefined);
  onStatus.mockReset();
  vi.mocked(executeCommand).mockImplementation((request: ExecuteCommandRequest) => {
    if (request.id === "terminal.create") {
      return Promise.resolve({
        terminalId: "term-1", title: "zsh", size: { columns: 90, rows: 24 }, running: true, exitCode: null,
      } satisfies TerminalSummary);
    }
    if (request.id === "terminal.read") {
      return Promise.resolve({
        terminalId: "term-1", bytes: [], startOffset: "0", nextOffset: "0", dropped: false, running: true, exitCode: null,
      } satisfies TerminalOutput);
    }
    return Promise.resolve(undefined);
  });
});

const theme = {
  "terminal.background": "#101418",
  "terminal.foreground": "#e7e4df",
  "terminal.cursor": "#e88974",
  panel: "#151920",
  muted: "#969da7",
};
const face: TerminalFont = { family: "ui-monospace, Menlo, monospace", size: 13, lineHeight: 1.1 };
const onStatus = vi.fn();
const onClose = vi.fn();

interface PanelProps {
  theme?: Record<string, string>;
  font?: TerminalFont;
  hidden?: boolean;
  focusSignal?: number;
}

function element(overrides: PanelProps = {}) {
  return <TerminalPanel theme={overrides.theme ?? theme} font={overrides.font ?? face} height={230}
    hidden={overrides.hidden ?? false} focusSignal={overrides.focusSignal ?? 0}
    onClose={onClose} onStatus={onStatus} />;
}

// A hidden panel leaves the accessibility tree, and its hidden state is exactly what
// these layout tests read, so they go straight to the element.
function panel() {
  const node = document.querySelector<HTMLElement>(".terminal-panel");
  if (!node) throw new Error("no terminal panel in the tree");
  return node;
}
function grid() { return grids.created[grids.created.length - 1]; }

/** jsdom lays out nothing, so a panel meant to be visible gets a box measured here. */
function box(host: HTMLElement, width: number, height: number) {
  Object.defineProperty(host, "clientWidth", { configurable: true, value: width });
  Object.defineProperty(host, "clientHeight", { configurable: true, value: height });
}

async function started() {
  await waitFor(() => expect(vi.mocked(executeCommand)).toHaveBeenCalledWith(expect.objectContaining({ id: "terminal.create" })));
  return grid();
}

test("starts a shell in a grid drawn with the app's monospace face", async () => {
  render(element());
  const created = await started();

  expect(onStatus).toHaveBeenCalledWith("Started zsh");
  // The divider's height lands on the panel itself, which is what the grid is fitted to.
  expect(panel()).toHaveStyle({ height: "230px" });
  expect(created.options.fontFamily).toBe(face.family);
  expect(created.options.fontSize).toBe(face.size);
  expect(created.options.letterSpacing).toBe(0);
  expect(created.options.theme).toMatchObject({
    background: theme["terminal.background"],
    foreground: theme["terminal.foreground"],
    cursor: theme["terminal.cursor"],
    // The glyph under a block cursor has to read on the panel's own ground colour.
    cursorAccent: theme["terminal.background"],
  });
  expect(vi.mocked(executeCommand)).toHaveBeenCalledWith(expect.objectContaining({
    id: "terminal.create",
    arguments: expect.objectContaining({ size: { columns: 90, rows: 24 } }),
  }));
});

test("keeps the grid's line spacing inside the band a terminal can use", async () => {
  const { rerender } = render(element({ font: { ...face, lineHeight: 2 } }));
  expect((await started()).options.lineHeight).toBe(1.25);

  rerender(element({ font: { ...face, lineHeight: 0.8 } }));
  expect(grid().options.lineHeight).toBe(1);

  rerender(element({ font: { ...face, lineHeight: Number.NaN } }));
  expect(grid().options.lineHeight).toBe(1);
});

test("follows the font when the setting changes", async () => {
  const { rerender } = render(element());
  await started();

  rerender(element({ font: { family: "Menlo, monospace", size: 16, lineHeight: 1.4 } }));
  expect(grid().options.fontFamily).toBe("Menlo, monospace");
  expect(grid().options.fontSize).toBe(16);
  expect(grid().options.lineHeight).toBe(1.25);
  expect(grids.created).toHaveLength(1);
});

test("routes keystrokes to the shell and its output back to the grid", async () => {
  render(element());
  const created = await started();
  vi.mocked(executeCommand).mockImplementation((request: ExecuteCommandRequest) => {
    if (request.id === "terminal.read") {
      return Promise.resolve({
        terminalId: "term-1", bytes: [13, 10, 104, 105, 13, 10], startOffset: "0", nextOffset: "6",
        dropped: false, running: true, exitCode: null,
      } satisfies TerminalOutput);
    }
    return Promise.resolve(undefined);
  });

  created.dataHandler?.("ls -l\r");
  await waitFor(() => expect(vi.mocked(executeCommand)).toHaveBeenCalledWith(expect.objectContaining({
    id: "terminal.write",
    arguments: { terminalId: "term-1", bytes: Array.from(new TextEncoder().encode("ls -l\r")) },
  })));
  await waitFor(() => expect(created.written.join("")).toContain("\r\nhi\r\n"));
});

test("a click anywhere in the panel puts the keyboard in the grid", async () => {
  render(element());
  const created = await started();

  expect(screen.getByRole("region", { name: "Integrated terminal" })).toBeInTheDocument();
  fireEvent.pointerDown(panel().querySelector(".terminal-body") as HTMLElement);
  expect(created.focusCalls).toBeGreaterThan(0);
  expect(panel()).toHaveClass("focused");
});

test("the grid takes the keyboard when the shell opens and lets go when it loses it", async () => {
  const { rerender } = render(element());
  const created = await started();
  expect(created.focusCalls).toBeGreaterThan(0);
  await waitFor(() => expect(panel()).toHaveClass("focused"));

  created.input?.blur();
  await waitFor(() => expect(panel()).not.toHaveClass("focused"));

  // A focus request from the workbench reaches the grid, which is what puts the caret there.
  rerender(element({ focusSignal: 1 }));
  await waitFor(() => expect(panel()).toHaveClass("focused"));
});

test("hiding the panel keeps the shell running and refits it on the way back", async () => {
  const { rerender } = render(element());
  const created = await started();
  box(panel().querySelector(".terminal-host") as HTMLElement, 700, 220);
  const fitsWhileVisible = grids.fits.length;

  rerender(element({ hidden: true }));
  expect(panel()).toHaveAttribute("hidden");
  expect(grids.fits).toHaveLength(fitsWhileVisible);
  expect(created.disposed).toBe(false);
  expect(vi.mocked(executeCommand)).not.toHaveBeenCalledWith(expect.objectContaining({ id: "terminal.close" }));

  rerender(element());
  expect(panel()).not.toHaveAttribute("hidden");
  expect(grids.fits.length).toBeGreaterThan(fitsWhileVisible);
  expect(vi.mocked(executeCommand)).toHaveBeenCalledWith(expect.objectContaining({
    id: "terminal.resize",
    arguments: { terminalId: "term-1", size: { columns: 90, rows: 24 } },
  }));
});

test("refuses to fit a panel that has no box, which would squeeze the live shell", async () => {
  render(element());
  const created = await started();
  const fits = grids.fits.length;
  // jsdom reports no box for any element, and a hidden panel has none for real.
  grids.observerCallbacks.forEach((callback) =>
    callback([{ contentRect: { width: 0, height: 0 } } as ResizeObserverEntry], {} as ResizeObserver));

  expect(grids.fits).toHaveLength(fits);
  expect(vi.mocked(executeCommand)).not.toHaveBeenCalledWith(expect.objectContaining({ id: "terminal.resize" }));
  expect(created.disposed).toBe(false);
});

test("closing the panel ends the shell", async () => {
  const { unmount } = render(element());
  const created = await started();

  fireEvent.click(screen.getByRole("button", { name: "Close terminal" }));
  expect(onClose).toHaveBeenCalled();
  unmount();

  expect(created.disposed).toBe(true);
  expect(vi.mocked(executeCommand)).toHaveBeenCalledWith(expect.objectContaining({
    id: "terminal.close",
    arguments: { terminalId: "term-1" },
  }));
});

function openMenu() {
  fireEvent.contextMenu(panel().querySelector(".terminal-body") as HTMLElement, { clientX: 40, clientY: 50 });
  return screen.getByRole("menu", { name: "Terminal" });
}

test("the right-click menu copies the terminal's selected text", async () => {
  render(element());
  const created = await started();
  created.selection = "total 48";

  fireEvent.click(within(openMenu()).getByRole("menuitem", { name: "Copy" }));

  await waitFor(() => expect(writeText).toHaveBeenCalledWith("total 48"));
  expect(screen.queryByRole("menu", { name: "Terminal" })).not.toBeInTheDocument();
  expect(onStatus).toHaveBeenCalledWith("Copied terminal text");
});

test("Copy is off when nothing in the terminal is selected", async () => {
  render(element());
  await started();

  expect(within(openMenu()).getByRole("menuitem", { name: "Copy" })).toBeDisabled();
});

test("the right-click menu pastes the clipboard into the shell", async () => {
  render(element());
  const created = await started();

  fireEvent.click(within(openMenu()).getByRole("menuitem", { name: "Paste" }));

  await waitFor(() => expect(created.pasted).toEqual(["echo pasted"]));
});

test("the right-click menu selects all and clears the terminal", async () => {
  render(element());
  const created = await started();

  fireEvent.click(within(openMenu()).getByRole("menuitem", { name: "Select All" }));
  fireEvent.click(within(openMenu()).getByRole("menuitem", { name: "Clear" }));

  expect(created.actions).toEqual(["selectAll", "clear"]);
  expect(created.focusCalls).toBeGreaterThan(1);
});
