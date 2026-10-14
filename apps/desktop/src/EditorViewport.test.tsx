import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeAll, expect, test, vi } from "vitest";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { EditorViewport } from "./EditorViewport";
import { executeCommand, getDocumentMap, getMarkdownPreviewSource, getViewport, getViewportAt, getViewportLine } from "./ipc";

vi.mock("./ipc", () => ({
  getViewport: vi.fn().mockResolvedValue({
    documentId: "document-1",
    revision: "0",
    documentByteLen: "7",
    encoding: "utf8",
    lines: [
      { line: "1", startByte: "0", endByte: "4", text: "one", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false },
      { line: "2", startByte: "4", endByte: "7", text: "two", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false },
    ],
    selections: [{ anchorByte: "0", activeByte: "0" }],
    bookmarks: [],
  }),
  executeCommand: vi.fn().mockResolvedValue({
    documentId: "document-1", name: "Untitled", byteLen: "8", revision: "1", encoding: "utf8",
  }),
  getViewportAt: vi.fn(),
  getViewportLine: vi.fn(),
  getDocumentMap: vi.fn().mockResolvedValue({ documentId: "document-1", revision: "0", documentByteLen: "7", totalLines: 2, samples: [] }),
  getMarkdownPreviewSource: vi.fn().mockResolvedValue({ documentId: "document-1", revision: "0", text: "# Preview\n\n**bold**" }),
}));

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn().mockResolvedValue(undefined),
}));

const resizeObserverCallbacks: ResizeObserverCallback[] = [];

beforeAll(() => {
  class ResizeObserverStub {
    constructor(callback: ResizeObserverCallback) {
      resizeObserverCallbacks.push(callback);
    }

    observe() {}
    disconnect() {}
  }
  class PointerEventStub extends MouseEvent {
    readonly pointerId: number;

    constructor(type: string, eventInit: PointerEventInit = {}) {
      super(type, eventInit);
      this.pointerId = eventInit.pointerId ?? 0;
    }
  }
  vi.stubGlobal("ResizeObserver", ResizeObserverStub);
  vi.stubGlobal("PointerEvent", PointerEventStub);
  Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
    configurable: true,
    value: vi.fn(() => ({
      font: "",
      measureText: () => ({ width: 8 }),
    })),
  });
});

afterEach(() => {
  cleanup();
  resizeObserverCallbacks.length = 0;
  vi.clearAllMocks();
});

test("renders line numbers from a bounded native viewport and dispatches typing", async () => {
  render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });
  expect(screen.getByText("one")).toBeInTheDocument();
  expect(screen.getByText("2")).toBeInTheDocument();
  await waitFor(() => expect(editor).toHaveFocus());

  fireEvent.keyDown(editor, { key: "x" });
  await waitFor(() => {
    expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
      id: "editor.insertText",
      arguments: expect.objectContaining({
        text: "x", expectedRevision: "0", overwrite: false,
      }),
    }));
  });
});

test("shows a compact document map for a short file and exposes its visibility toggle", async () => {
  render(<EditorViewport document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
    onDocumentChange={() => undefined} onStatus={() => undefined} />);
  await screen.findByRole("textbox", { name: "Untitled editor" });
  const map = screen.getByRole("button", { name: "Document map" });
  act(() => resizeObserverCallbacks[0]?.([{ contentRect: { height: 500 } } as ResizeObserverEntry], {} as ResizeObserver));
  const marker = map.querySelector<HTMLElement>(".document-map-viewport");
  expect(Number.parseFloat(marker?.style.height ?? "0")).toBeGreaterThan(50);
  expect(Number.parseFloat(marker?.style.height ?? "0")).toBeLessThan(200);
  expect(screen.getByRole("button", { name: "Toggle document map" })).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("button", { name: "Toggle document map" })).toHaveAttribute("title", "Hide document map");
  fireEvent.click(screen.getByRole("button", { name: "Toggle document map" }));
  expect(screen.queryByRole("button", { name: "Document map" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Toggle document map" })).toHaveAttribute("aria-pressed", "false");
  expect(screen.getByRole("button", { name: "Toggle document map" })).toHaveAttribute("title", "Show document map");
  fireEvent.click(screen.getByRole("button", { name: "Toggle document map" }));
  expect(screen.getByRole("button", { name: "Document map" })).toBeInTheDocument();
  await waitFor(() => expect(getDocumentMap).toHaveBeenCalledWith("document-1"));
});

test("switches Markdown between editable text and a formatted safe preview", async () => {
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-1", revision: "0", documentByteLen: "12", encoding: "utf8",
    lines: [{ line: "1", startByte: "0", endByte: "12", text: "# Preview", sourceByteWidths: Array(9).fill(1), continuation: false, truncated: false }],
    selections: [{ anchorByte: "0", activeByte: "0" }], bookmarks: [],
    syntax: { languageId: "markdown", parsedBytes: 12, highlights: [], folds: [], symbols: [],
      bracketPairs: [], truncated: false, timedOut: false },
  });
  vi.mocked(getMarkdownPreviewSource).mockResolvedValueOnce({ documentId: "document-1", revision: "0",
    text: "# Preview\n\n**bold**\n\n| A | B |\n| - | - |\n| one | two |\n\n<script>bad()</script>" });
  render(<EditorViewport document={{ documentId: "document-1", name: "notes.md", byteLen: "12", revision: "0", encoding: "utf8" }}
    onDocumentChange={() => undefined} onStatus={() => undefined} />);
  expect(await screen.findByRole("textbox", { name: "notes.md editor" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Edit" })).toHaveAttribute("aria-pressed", "true");
  fireEvent.click(screen.getByRole("button", { name: "View" }));
  expect(await screen.findByRole("heading", { name: "Preview" })).toBeInTheDocument();
  expect(screen.getByText("bold").tagName).toBe("STRONG");
  expect(screen.getByRole("table")).toHaveTextContent("one");
  expect(screen.queryByRole("textbox", { name: "notes.md editor" })).not.toBeInTheDocument();
  expect(screen.queryByText("bad()")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Edit" }));
  expect(screen.getByRole("textbox", { name: "notes.md editor" })).toBeInTheDocument();
});

test("document map tracks visible lines and scrolls within a loaded document", async () => {
  const lines = Array.from({ length: 60 }, (_, index) => ({
    line: String(index + 1), startByte: String(index * 2), endByte: String(index * 2 + 2),
    text: "x", sourceByteWidths: [1], continuation: false, truncated: false,
  }));
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-1", revision: "0", documentByteLen: "120", encoding: "utf8",
    lines, selections: [{ anchorByte: "0", activeByte: "0" }], bookmarks: [],
  });
  vi.mocked(getDocumentMap).mockResolvedValueOnce({
    documentId: "document-1", revision: "0", documentByteLen: "120", totalLines: 60,
    samples: lines.map((line) => ({ byteOffset: line.startByte, line: line.line, text: line.text })),
  });
  const { container } = render(<EditorViewport document={{ documentId: "document-1", name: "Untitled",
    byteLen: "120", revision: "0", encoding: "utf8" }} onDocumentChange={() => undefined} onStatus={() => undefined} />);
  const map = await screen.findByRole("button", { name: "Document map" });
  await waitFor(() => expect(map.querySelectorAll(".document-map-line")).toHaveLength(60));
  await waitFor(() => expect(getDocumentMap).toHaveBeenCalledWith("document-1"));
  await act(async () => { await new Promise((resolve) => window.setTimeout(resolve, 0)); });
  const surface = container.querySelector<HTMLElement>(".editor-surface");
  expect(surface).not.toBeNull();
  act(() => resizeObserverCallbacks[0]?.([{ contentRect: { height: 220 } } as ResizeObserverEntry], {} as ResizeObserver));
  fireEvent.scroll(surface!, { target: { scrollTop: 440 } });
  const marker = map.querySelector<HTMLElement>(".document-map-viewport");
  expect(Number.parseFloat(marker?.style.top ?? "0")).toBeGreaterThan(30);
  map.getBoundingClientRect = () => ({ top: 0, height: 800 }) as DOMRect;
  fireEvent.pointerDown(map, { clientY: 160, pointerId: 1 });
  expect(surface!.scrollTop).toBeGreaterThan(440);
  expect(getViewportAt).not.toHaveBeenCalled();
});

test("focuses the input when the active tab changes to a newly loaded document", async () => {
  const { rerender } = render(
    <EditorViewport
      document={{ documentId: "document-1", name: "First", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  const firstEditor = await screen.findByRole("textbox", { name: "First editor" });
  await waitFor(() => expect(firstEditor).toHaveFocus());
  screen.getByRole("button", { name: "INS" }).focus();

  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-2",
    revision: "0",
    documentByteLen: "0",
    encoding: "utf8",
    lines: [
      { line: "1", startByte: "0", endByte: "0", text: "", sourceByteWidths: [], continuation: false, truncated: false },
    ],
    selections: [{ anchorByte: "0", activeByte: "0" }],
    bookmarks: [],
  });
  rerender(
    <EditorViewport
      document={{ documentId: "document-2", name: "Second", byteLen: "0", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );

  const secondEditor = await screen.findByRole("textbox", { name: "Second editor" });
  await waitFor(() => expect(secondEditor).toHaveFocus());
  fireEvent.keyDown(secondEditor, { key: "x" });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.insertText",
    context: { windowId: "main", documentId: "document-2" },
  })));
});

test("focuses the input when any blank part of the editor surface is clicked", async () => {
  const { container } = render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });
  await waitFor(() => expect(editor).toHaveFocus());
  screen.getByRole("button", { name: "INS" }).focus();
  expect(editor).not.toHaveFocus();

  const surface = container.querySelector<HTMLElement>(".editor-surface");
  expect(surface).not.toBeNull();
  fireEvent.pointerDown(surface!, { button: 0, pointerId: 3 });
  expect(editor).toHaveFocus();

  fireEvent.keyDown(editor, { key: "x" });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.insertText",
    arguments: expect.objectContaining({ text: "x" }),
  })));
});

test("reveals a retained caret outside the first bounded viewport when a tab opens", async () => {
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-deep",
    revision: "4",
    documentByteLen: "1003",
    encoding: "utf8",
    lines: [
      { line: "1", startByte: "0", endByte: "4", text: "top", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false },
    ],
    selections: [{ anchorByte: "1000", activeByte: "1000" }],
    bookmarks: [],
  });
  vi.mocked(getViewportAt).mockResolvedValueOnce({
    documentId: "document-deep",
    revision: "4",
    documentByteLen: "1003",
    encoding: "utf8",
    lines: [
      { line: "100", startByte: "1000", endByte: "1003", text: "end", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false },
    ],
    selections: [{ anchorByte: "1000", activeByte: "1000" }],
    bookmarks: [],
  });

  const { container } = render(
    <EditorViewport
      document={{ documentId: "document-deep", name: "Deep", byteLen: "1003", revision: "4", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );

  expect(await screen.findByText("end")).toBeInTheDocument();
  expect(getViewportAt).toHaveBeenCalledWith({
    documentId: "document-deep", byteOffset: "1000", maxBytes: 32 * 1024,
  });
  expect(container.querySelector(".caret")).toBeInTheDocument();
  expect(screen.getByRole("textbox", { name: "Deep editor" })).toHaveFocus();
});

test("does not reset the viewport merely because the document revision changes", async () => {
  const document = {
    documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" as const,
  };
  const onStatus = () => undefined;
  const { rerender } = render(
    <EditorViewport document={document} onDocumentChange={() => undefined} onStatus={onStatus} />,
  );
  await screen.findByRole("textbox", { name: "Untitled editor" });
  await waitFor(() => expect(getViewport).toHaveBeenCalledTimes(1));

  rerender(
    <EditorViewport
      document={{ ...document, revision: "1" }}
      onDocumentChange={() => undefined}
      onStatus={onStatus}
    />,
  );
  await act(async () => undefined);
  expect(getViewport).toHaveBeenCalledTimes(1);
});

test("keeps the new-line caret visible when Enter refreshes a scrolled viewport", async () => {
  const beforeLines = Array.from({ length: 30 }, (_, index) => ({
    line: String(index + 1),
    startByte: String(index * 2),
    endByte: String(index * 2 + 2),
    text: "x",
    sourceByteWidths: [1],
    continuation: false,
    truncated: false,
  }));
  const afterLines = [
    ...beforeLines.slice(0, 20),
    {
      line: "21", startByte: "40", endByte: "41", text: "", sourceByteWidths: [],
      continuation: false, truncated: false,
    },
    ...beforeLines.slice(20).map((line) => ({
      ...line,
      line: String(Number(line.line) + 1),
      startByte: String(Number(line.startByte) + 1),
      endByte: String(Number(line.endByte) + 1),
    })),
  ];
  vi.mocked(getViewport)
    .mockResolvedValueOnce({
      documentId: "document-1", revision: "0", documentByteLen: "60", encoding: "utf8",
      lines: beforeLines, selections: [{ anchorByte: "39", activeByte: "39" }], bookmarks: [],
    })
    .mockResolvedValueOnce({
      documentId: "document-1", revision: "1", documentByteLen: "61", encoding: "utf8",
      lines: afterLines, selections: [{ anchorByte: "40", activeByte: "40" }], bookmarks: [],
    });
  vi.mocked(executeCommand).mockResolvedValueOnce({
    documentId: "document-1", name: "Untitled", byteLen: "61", revision: "1", encoding: "utf8",
  });

  const { container } = render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "60", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });
  const surface = container.querySelector<HTMLElement>(".editor-surface");
  expect(surface).not.toBeNull();
  Object.defineProperties(surface, {
    clientHeight: { configurable: true, value: 44 },
    scrollHeight: { configurable: true, value: 682 },
  });
  act(() => {
    resizeObserverCallbacks.at(-1)?.(
      [{ contentRect: { height: 44 } } as ResizeObserverEntry],
      {} as ResizeObserver,
    );
  });
  surface!.scrollTop = 396;
  fireEvent.scroll(surface!);

  fireEvent.keyDown(editor, { key: "Enter" });
  await waitFor(() => expect(getViewport).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(container.querySelector(".caret")).toBeInTheDocument());
  expect(surface!.scrollTop).toBeGreaterThan(0);
  expect(editor).toHaveFocus();
});

test("resizes the rendered text viewport after its async surface mounts", async () => {
  const lines = Array.from({ length: 60 }, (_, index) => ({
    line: String(index + 1),
    startByte: String(index * 2),
    endByte: String(index * 2 + 2),
    text: "x",
    sourceByteWidths: [1],
    continuation: false,
    truncated: false,
  }));
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-1",
    revision: "0",
    documentByteLen: "120",
    encoding: "utf8",
    lines,
    selections: [{ anchorByte: "0", activeByte: "0" }],
    bookmarks: [],
  });
  render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "120", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );

  await screen.findByRole("textbox", { name: "Untitled editor" });
  await waitFor(() => expect(resizeObserverCallbacks.length).toBeGreaterThanOrEqual(1));
  const resize = resizeObserverCallbacks[0];
  expect(resize).toBeDefined();

  act(() => {
    resize?.(
      [{ contentRect: { height: 44 } } as ResizeObserverEntry],
      {} as ResizeObserver,
    );
  });
  expect(screen.queryByText("30")).not.toBeInTheDocument();

  act(() => {
    resize?.(
      [{ contentRect: { height: 660 } } as ResizeObserverEntry],
      {} as ResizeObserver,
    );
  });
  expect(screen.getByText("30")).toBeInTheDocument();
});

test("copies bounded core selection text through the native clipboard", async () => {
  vi.mocked(executeCommand).mockResolvedValueOnce({
    text: "selected text",
    revision: "0",
    hadInvalidUtf8: false,
  });
  render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });

  fireEvent.copy(editor);
  await waitFor(() => expect(writeText).toHaveBeenCalledWith("selected text"));
  expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.selectionText",
  }));
});

test("writes the clipboard before dispatching revision-checked cut deletion", async () => {
  const callOrder: string[] = [];
  vi.mocked(executeCommand)
    .mockImplementationOnce(async () => ({
      text: "selected text",
      revision: "4",
      hadInvalidUtf8: false,
    }))
    .mockImplementationOnce(async () => {
      callOrder.push("delete");
      return { documentId: "document-1", name: "Untitled", byteLen: "0", revision: "5" };
    });
  vi.mocked(writeText).mockImplementationOnce(async () => {
    callOrder.push("clipboard");
  });
  const onDocumentChange = vi.fn();
  render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "13", revision: "4", encoding: "utf8" }}
      onDocumentChange={onDocumentChange}
      onStatus={() => undefined}
    />,
  );
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });

  fireEvent.cut(editor);
  await waitFor(() => expect(onDocumentChange).toHaveBeenCalled());
  expect(callOrder).toEqual(["clipboard", "delete"]);
  expect(executeCommand).toHaveBeenNthCalledWith(2, expect.objectContaining({
    id: "editor.deleteSelection",
    arguments: { expectedRevision: "4" },
  }));
});

test("toggles overwrite mode with the Insert key", async () => {
  render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });

  fireEvent.keyDown(editor, { key: "Insert" });
  expect(screen.getByRole("button", { name: "OVR" })).toHaveAttribute("aria-pressed", "true");
  fireEvent.keyDown(editor, { key: "x" });
  await waitFor(() => {
    expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
      id: "editor.insertText",
      arguments: expect.objectContaining({ overwrite: true }),
    }));
  });
});

test("dispatches vertical movement with selection extension", async () => {
  vi.mocked(executeCommand).mockResolvedValueOnce([
    { anchorByte: "0", activeByte: "4" },
  ]);
  render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });

  fireEvent.keyDown(editor, { key: "ArrowDown", shiftKey: true });
  await waitFor(() => {
    expect(executeCommand).toHaveBeenCalledWith({
      id: "editor.moveVertical",
      context: { windowId: "main", documentId: "document-1" },
      arguments: { direction: "down", extend: true },
    });
  });
});

test("reveals a caret that moves outside the bounded snapshot", async () => {
  vi.mocked(executeCommand).mockResolvedValueOnce([
    { anchorByte: "100", activeByte: "100" },
  ]);
  vi.mocked(getViewportAt).mockResolvedValueOnce({
    documentId: "document-1",
    revision: "0",
    documentByteLen: "200",
    encoding: "utf8",
    lines: [
      { line: "25", startByte: "96", endByte: "104", text: "revealed", sourceByteWidths: [1, 1, 1, 1, 1, 1, 1, 1], continuation: false, truncated: false },
    ],
    selections: [{ anchorByte: "100", activeByte: "100" }],
    bookmarks: [],
  });
  render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "200", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });

  fireEvent.keyDown(editor, { key: "ArrowDown" });
  await waitFor(() => expect(getViewportAt).toHaveBeenCalledWith({
    documentId: "document-1",
    byteOffset: "100",
    maxBytes: 32 * 1024,
  }));
  expect(await screen.findByText("revealed")).toBeInTheDocument();
});

test("scrolls the virtualized window when the caret moves below the visible rows", async () => {
  const lines = Array.from({ length: 30 }, (_, index) => ({
    line: String(index + 1),
    startByte: String(index * 2),
    endByte: String(index * 2 + 2),
    text: "x",
    sourceByteWidths: [1],
    continuation: false,
    truncated: false,
  }));
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-1",
    revision: "0",
    documentByteLen: "60",
    encoding: "utf8",
    lines,
    selections: [{ anchorByte: "0", activeByte: "0" }],
    bookmarks: [],
  });
  vi.mocked(executeCommand).mockResolvedValueOnce([
    { anchorByte: "48", activeByte: "48" },
  ]);
  const { container } = render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "60", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });
  const surface = container.querySelector<HTMLElement>(".editor-surface");
  expect(surface).not.toBeNull();
  Object.defineProperties(surface!, {
    clientHeight: { configurable: true, value: 44 },
    scrollHeight: { configurable: true, value: 660 },
  });

  fireEvent.keyDown(editor, { key: "ArrowDown" });

  await waitFor(() => expect(surface!.scrollTop).toBe(506));
  expect(getViewportAt).not.toHaveBeenCalled();
  expect(screen.getByText("25")).toBeInTheDocument();
});

test("commits an IME composition exactly once and exposes editor semantics", async () => {
  render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });
  expect(document.getElementById(editor.getAttribute("aria-describedby") ?? "")).toHaveTextContent(/Text editor\./);
  expect(document.getElementById(editor.getAttribute("aria-controls") ?? "")).toHaveClass("virtual-lines");

  fireEvent.compositionStart(editor);
  fireEvent.keyDown(editor, { key: "Process", keyCode: 229 });
  fireEvent.compositionUpdate(editor, { data: "文" });
  fireEvent.compositionEnd(editor, { data: "文" });
  fireEvent.input(editor, { target: { value: "文" } });

  await waitFor(() => expect(executeCommand).toHaveBeenCalledTimes(1));
  expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.insertText",
    arguments: expect.objectContaining({ text: "文" }),
  }));
  expect(screen.getByRole("status")).toHaveTextContent(/Line 1, column 1/);
});

test("keeps the rendered DOM bounded for a 10 GB logical document", async () => {
  const lines = Array.from({ length: 20_000 }, (_, index) => ({
    line: String(index + 1),
    startByte: String(index),
    endByte: String(index + 1),
    text: "",
    sourceByteWidths: [],
    continuation: false,
    truncated: false,
  }));
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-1",
    revision: "0",
    documentByteLen: String(10 * 1024 * 1024 * 1024),
    encoding: "utf8",
    lines,
    selections: [{ anchorByte: "0", activeByte: "0" }],
    bookmarks: [],
    next: { byteOffset: "20000", line: "20001", lineContinuation: false },
  });
  const { container } = render(
    <EditorViewport
      document={{
        documentId: "document-1",
        name: "huge.log",
        byteLen: String(10 * 1024 * 1024 * 1024),
        revision: "0",
        encoding: "utf8",
      }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  await screen.findByRole("textbox", { name: "huge.log editor" });
  await waitFor(() => expect(container.querySelectorAll(".editor-line").length).toBeGreaterThan(0));
  expect(container.querySelectorAll(".editor-line").length).toBeLessThanOrEqual(40);
  expect(container.querySelector(".virtual-lines")).toHaveStyle({ height: `${20_000 * 22}px` });
});

test("renders byte-ranged selection text and crossed line endings", async () => {
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-1",
    revision: "0",
    documentByteLen: "7",
    encoding: "utf8",
    lines: [
      { line: "1", startByte: "0", endByte: "4", text: "one", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false },
      { line: "2", startByte: "4", endByte: "7", text: "two", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false },
    ],
    selections: [{ anchorByte: "1", activeByte: "6" }],
    bookmarks: [],
  });
  const { container } = render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  await screen.findByRole("textbox", { name: "Untitled editor" });
  await waitFor(() => expect(container.querySelectorAll(".selection")).toHaveLength(3));

  const selections = [...container.querySelectorAll(".selection")];
  expect(selections.map((selection) => selection.textContent)).toEqual(["ne", " ", "tw"]);
  expect(container.querySelector(".selection.line-ending")).toBeInTheDocument();
});

test("previews a drag locally and commits one core selection on release", async () => {
  const { container } = render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  await screen.findByRole("textbox", { name: "Untitled editor" });
  const lines = [...container.querySelectorAll<HTMLElement>(".editor-line")];
  const codes = [...container.querySelectorAll<HTMLElement>(".editor-line code")];
  for (const code of codes) {
    code.getBoundingClientRect = () => ({ left: 64 }) as DOMRect;
  }

  fireEvent.pointerDown(lines[0], { button: 0, pointerId: 7, clientX: 72 });
  fireEvent.pointerMove(lines[1], { buttons: 1, pointerId: 7, clientX: 80 });
  fireEvent.pointerUp(window, { pointerId: 7 });

  await waitFor(() => {
    expect(executeCommand).toHaveBeenCalledTimes(1);
    expect(executeCommand).toHaveBeenCalledWith({
      id: "editor.setSelections",
      context: { windowId: "main", documentId: "document-1" },
      arguments: { selections: [{ anchorByte: "1", activeByte: "6" }] },
    });
  });
});

test("continues a drag selection across bounded viewport pages", async () => {
  vi.mocked(getViewport)
    .mockResolvedValueOnce({
      documentId: "document-1",
      revision: "0",
      documentByteLen: "12",
    encoding: "utf8",
      lines: [
        { line: "1", startByte: "0", endByte: "4", text: "one", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false },
        { line: "2", startByte: "4", endByte: "7", text: "two", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false },
      ],
      selections: [{ anchorByte: "0", activeByte: "0" }],
      bookmarks: [],
      next: { byteOffset: "7", line: "3", lineContinuation: false },
    })
    .mockResolvedValueOnce({
      documentId: "document-1",
      revision: "0",
      documentByteLen: "12",
    encoding: "utf8",
      lines: [
        { line: "3", startByte: "7", endByte: "12", text: "next", sourceByteWidths: [1, 1, 1, 1], continuation: false, truncated: false },
      ],
      selections: [{ anchorByte: "0", activeByte: "0" }],
      bookmarks: [],
    });
  const { container } = render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "12", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  await screen.findByRole("textbox", { name: "Untitled editor" });
  const surface = container.querySelector<HTMLElement>(".editor-surface");
  const lines = [...container.querySelectorAll<HTMLElement>(".editor-line")];
  const codes = [...container.querySelectorAll<HTMLElement>(".editor-line code")];
  expect(surface).not.toBeNull();
  for (const code of codes) code.getBoundingClientRect = () => ({ left: 64 }) as DOMRect;
  Object.defineProperties(surface, {
    clientHeight: { configurable: true, value: 44 },
    scrollHeight: { configurable: true, value: 44 },
  });
  if (surface) surface.getBoundingClientRect = () => ({ top: 0, bottom: 100 }) as DOMRect;

  fireEvent.pointerDown(lines[0], { button: 0, pointerId: 9, clientX: 72, clientY: 10 });
  fireEvent.pointerMove(lines[1], {
    buttons: 1, pointerId: 9, clientX: 80, clientY: 95,
  });
  await waitFor(() => expect(getViewport).toHaveBeenCalledTimes(2));
  await screen.findByText("next");
  fireEvent.pointerUp(window, { pointerId: 9 });

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "editor.setSelections",
    context: { windowId: "main", documentId: "document-1" },
    arguments: { selections: [{ anchorByte: "1", activeByte: "11" }] },
  }));
});

test("adds and commits an independent caret with Alt-click", async () => {
  const { container } = render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  await screen.findByRole("textbox", { name: "Untitled editor" });
  const lines = [...container.querySelectorAll<HTMLElement>(".editor-line")];
  const codes = [...container.querySelectorAll<HTMLElement>(".editor-line code")];
  for (const code of codes) code.getBoundingClientRect = () => ({ left: 64 }) as DOMRect;

  fireEvent.pointerDown(lines[1], {
    button: 0,
    pointerId: 17,
    clientX: 72,
    altKey: true,
  });
  fireEvent.pointerUp(window, { pointerId: 17 });

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "editor.setSelections",
    context: { windowId: "main", documentId: "document-1" },
    arguments: {
      selections: [
        { anchorByte: "0", activeByte: "0" },
        { anchorByte: "5", activeByte: "5" },
      ],
    },
  }));
  expect(screen.getByText("2 carets")).toBeInTheDocument();
  expect(container.querySelectorAll(".caret")).toHaveLength(2);
});

test("previews and commits a first-class rectangular selection with Alt-Shift drag", async () => {
  vi.mocked(executeCommand).mockResolvedValueOnce({
    anchorByte: "1",
    activeByte: "6",
    anchorLine: "1",
    activeLine: "2",
    anchorColumn: "1",
    activeColumn: "2",
  });
  const { container } = render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  await screen.findByRole("textbox", { name: "Untitled editor" });
  const lines = [...container.querySelectorAll<HTMLElement>(".editor-line")];
  const codes = [...container.querySelectorAll<HTMLElement>(".editor-line code")];
  for (const code of codes) code.getBoundingClientRect = () => ({ left: 64 }) as DOMRect;

  fireEvent.pointerDown(lines[0], {
    button: 0,
    pointerId: 23,
    clientX: 72,
    altKey: true,
    shiftKey: true,
  });
  fireEvent.pointerMove(lines[1], {
    buttons: 1,
    pointerId: 23,
    clientX: 80,
    altKey: true,
    shiftKey: true,
  });
  expect(container.querySelectorAll(".selection")).toHaveLength(2);
  fireEvent.pointerUp(window, { pointerId: 23 });

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "editor.setRectangularSelection",
    context: { windowId: "main", documentId: "document-1" },
    arguments: {
      anchorByte: "1",
      activeByte: "6",
      anchorColumn: "1",
      activeColumn: "2",
    },
  }));
});

test("renders bounded syntax features, bracket matching, folding, symbols, and language override", async () => {
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-1",
    revision: "0",
    documentByteLen: "11",
    encoding: "utf8",
    lines: [
      { line: "1", startByte: "0", endByte: "10", text: "{\"x\": 1,", sourceByteWidths: [1, 1, 1, 1, 1, 1, 1, 1], continuation: false, truncated: false },
      { line: "2", startByte: "10", endByte: "11", text: "}", sourceByteWidths: [1], continuation: false, truncated: false },
    ],
    selections: [{ anchorByte: "0", activeByte: "0" }],
    bookmarks: [],
    syntax: {
      languageId: "json",
      parsedBytes: 11,
      highlights: [
        { startByte: "1", endByte: "4", token: "property" },
        { startByte: "6", endByte: "7", token: "number" },
      ],
      folds: [{ startByte: "0", endByte: "11", startLine: "1", endLine: "2" }],
      symbols: [{ name: "x", kind: "property", startByte: "1", line: "1" }],
      bracketPairs: [{ openByte: "0", closeByte: "10" }],
      truncated: false,
      timedOut: false,
    },
  });
  const { container } = render(
    <EditorViewport
      document={{ documentId: "document-1", name: "settings.json", byteLen: "11", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  await screen.findByRole("textbox", { name: "settings.json editor" });
  expect(container.querySelector(".syntax-property")).toHaveTextContent("\"x\"");
  expect(container.querySelectorAll(".bracket-match")).toHaveLength(2);
  expect(screen.getByRole("combobox", { name: "Document symbols" })).toHaveTextContent("x · line 1");

  const foldToggle = container.querySelector<HTMLButtonElement>(".fold-toggle");
  expect(foldToggle).not.toBeNull();
  if (foldToggle) fireEvent.click(foldToggle);
  expect(container.querySelectorAll(".editor-line")).toHaveLength(1);
  expect(container.querySelector(".fold-placeholder")).toBeInTheDocument();

  expect(screen.getByText("View As")).toBeVisible();
  fireEvent.change(screen.getByRole("combobox", { name: "View As language" }), {
    target: { value: "plaintext" },
  });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "language.set",
    context: { windowId: "main", documentId: "document-1" },
    arguments: { languageId: "plaintext" },
  }));
});

test("exposes wrapping, transforms, bookmarks, and encoding controls", async () => {
  vi.mocked(executeCommand)
    .mockResolvedValueOnce(["0"])
    .mockResolvedValueOnce({
      documentId: "document-1",
      name: "Untitled",
      byteLen: "7",
      revision: "0",
      encoding: "windows1252",
    });
  const { container } = render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );
  await screen.findByRole("textbox", { name: "Untitled editor" });

  const wrap = screen.getByRole("button", { name: "Wrap" });
  fireEvent.click(wrap);
  expect(wrap).toHaveAttribute("aria-pressed", "true");
  expect(container.querySelector(".editor-surface")).toHaveClass("word-wrap");

  fireEvent.click(screen.getByTitle(/Toggle bookmark/));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.toggleBookmark",
  })));

  fireEvent.change(screen.getByRole("combobox", { name: "Text encoding" }), {
    target: { value: "windows1252" },
  });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.setEncoding",
    arguments: { encoding: "windows1252" },
  })));
});

test("opens bounded find with the keyboard and reveals the returned match", async () => {
  vi.mocked(executeCommand).mockResolvedValueOnce({
    revision: "0",
    selection: { anchorByte: "4", activeByte: "7" },
    wrapped: false,
  });
  const onStatus = vi.fn();
  render(
    <EditorViewport
      document={{ documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" }}
      onDocumentChange={() => undefined}
      onStatus={onStatus}
    />,
  );
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });

  fireEvent.keyDown(editor, { key: "f", ctrlKey: true });
  fireEvent.change(screen.getByRole("textbox", { name: "Find text" }), {
    target: { value: "two" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Next match" }));

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "search.find",
    context: { windowId: "main", documentId: "document-1" },
    arguments: {
      query: "two",
      options: { mode: "literal", caseSensitive: true },
      startByte: "0",
      direction: "next",
      wrap: true,
    },
  }));
  expect(onStatus).toHaveBeenCalledWith("next match");
});

const untitled = { documentId: "document-1", name: "Untitled", byteLen: "7", revision: "0", encoding: "utf8" } as const;

function selectedFirstWordViewport() {
  return {
    documentId: "document-1", revision: "0", documentByteLen: "7", encoding: "utf8" as const,
    lines: [
      { line: "1", startByte: "0", endByte: "4", text: "one", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false },
      { line: "2", startByte: "4", endByte: "7", text: "two", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false },
    ],
    selections: [{ anchorByte: "0", activeByte: "3" }],
    bookmarks: [],
  };
}

async function renderPointerEditor(onStatus: (message: string) => void = () => undefined) {
  const view = render(<EditorViewport document={untitled} onDocumentChange={() => undefined} onStatus={onStatus} />);
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });
  const lines = [...view.container.querySelectorAll<HTMLElement>(".editor-line")];
  const codes = [...view.container.querySelectorAll<HTMLElement>(".editor-line code")];
  for (const code of codes) code.getBoundingClientRect = () => ({ left: 64 }) as DOMRect;
  return { ...view, editor, lines, codes };
}

test("double-click commits a core word selection and triple-click selects the logical line", async () => {
  const { lines } = await renderPointerEditor();
  vi.mocked(executeCommand).mockResolvedValueOnce({ anchorByte: "0", activeByte: "3" });
  fireEvent.pointerDown(lines[0], { button: 0, pointerId: 1, clientX: 72, detail: 2 });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "editor.selectWordAt",
    context: { windowId: "main", documentId: "document-1" },
    arguments: { expectedRevision: "0", byteOffset: "1" },
  }));
  expect(await screen.findByText("one")).toHaveClass("selection");

  fireEvent.pointerDown(lines[1], { button: 0, pointerId: 1, clientX: 72, detail: 3 });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.setSelections",
    arguments: { selections: [{ anchorByte: "4", activeByte: "7" }] },
  })));
});

test("keyboard word selection uses the same core boundary rule", async () => {
  const { editor } = await renderPointerEditor();
  vi.mocked(executeCommand).mockResolvedValueOnce({ anchorByte: "0", activeByte: "3" });
  fireEvent.keyDown(editor, { key: "d", ctrlKey: true });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.selectWordAt",
    arguments: { expectedRevision: "0", byteOffset: "0" },
  })));
  expect(await screen.findByText("one")).toHaveClass("selection");
});

test("dragging a selection past the threshold moves it as one revision-checked command", async () => {
  vi.mocked(getViewport).mockResolvedValueOnce(selectedFirstWordViewport());
  const { container, lines, codes } = await renderPointerEditor();
  fireEvent.pointerDown(codes[0], { button: 0, pointerId: 3, clientX: 72, clientY: 10 });
  // Movement below the threshold neither previews nor commits a move.
  fireEvent.pointerMove(lines[0], { buttons: 1, pointerId: 3, clientX: 73, clientY: 10 });
  expect(container.querySelector(".drop-caret")).toBeNull();
  fireEvent.pointerMove(lines[1], { buttons: 1, pointerId: 3, clientX: 80, clientY: 32 });
  expect(container.querySelector(".drop-caret")).not.toBeNull();
  fireEvent.pointerUp(window, { pointerId: 3 });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "editor.moveSelectedText",
    context: { windowId: "main", documentId: "document-1" },
    arguments: { expectedRevision: "0", startByte: "0", endByte: "3", destinationByte: "6", copy: false },
  }));
  expect(container.querySelector(".drop-caret")).toBeNull();
  expect(executeCommand).not.toHaveBeenCalledWith(expect.objectContaining({ id: "editor.setSelections" }));
});

test("the platform copy modifier copies a dragged selection", async () => {
  vi.mocked(getViewport).mockResolvedValueOnce(selectedFirstWordViewport());
  const { lines, codes } = await renderPointerEditor();
  fireEvent.pointerDown(codes[0], { button: 0, pointerId: 3, clientX: 72, clientY: 10 });
  fireEvent.pointerMove(lines[1], { buttons: 1, pointerId: 3, clientX: 80, clientY: 32 });
  fireEvent.pointerUp(window, { pointerId: 3, ctrlKey: true });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.moveSelectedText", arguments: expect.objectContaining({ copy: true }),
  })));
});

test("a click inside a selection places the caret, and Escape or an outside drop cancels a move", async () => {
  vi.mocked(getViewport)
    .mockResolvedValueOnce(selectedFirstWordViewport())
    .mockResolvedValueOnce(selectedFirstWordViewport());
  const onStatus = vi.fn();
  const { codes } = await renderPointerEditor(onStatus);

  fireEvent.pointerDown(codes[0], { button: 0, pointerId: 4, clientX: 72, clientY: 10 });
  fireEvent.pointerUp(window, { pointerId: 4 });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.setSelections", arguments: { selections: [{ anchorByte: "1", activeByte: "1" }] },
  })));

  cleanup();
  vi.mocked(executeCommand).mockClear();
  const second = await renderPointerEditor(onStatus);
  fireEvent.pointerDown(second.codes[0], { button: 0, pointerId: 5, clientX: 72, clientY: 10 });
  fireEvent.pointerMove(second.lines[1], { buttons: 1, pointerId: 5, clientX: 80, clientY: 32 });
  fireEvent.keyDown(second.editor, { key: "Escape" });
  fireEvent.pointerUp(window, { pointerId: 5 });
  expect(onStatus).toHaveBeenCalledWith("Text move canceled");

  fireEvent.pointerDown(second.codes[0], { button: 0, pointerId: 6, clientX: 72, clientY: 10 });
  fireEvent.pointerMove(second.lines[1], { buttons: 1, pointerId: 6, clientX: 80, clientY: 32 });
  fireEvent.pointerMove(document.body, { buttons: 1, pointerId: 6, clientX: 900, clientY: 900 });
  fireEvent.pointerUp(window, { pointerId: 6 });
  await act(async () => { await Promise.resolve(); });
  expect(executeCommand).not.toHaveBeenCalled();
});

test("a stale drop reports the rejection and refreshes without a local edit", async () => {
  vi.mocked(getViewport).mockResolvedValueOnce(selectedFirstWordViewport());
  const onStatus = vi.fn();
  const { lines, codes } = await renderPointerEditor(onStatus);
  vi.mocked(executeCommand).mockRejectedValueOnce(new Error("stale revision: expected 0, actual 1"));
  fireEvent.pointerDown(codes[0], { button: 0, pointerId: 3, clientX: 72, clientY: 10 });
  fireEvent.pointerMove(lines[1], { buttons: 1, pointerId: 3, clientX: 80, clientY: 32 });
  fireEvent.pointerUp(window, { pointerId: 3 });
  await waitFor(() => expect(onStatus).toHaveBeenCalledWith("stale revision: expected 0, actual 1"));
  await waitFor(() => expect(getViewport).toHaveBeenCalledTimes(2));
});

test("read-only mode refuses typing with a clear message and reports the caret position", async () => {
  vi.mocked(getViewport).mockResolvedValueOnce(selectedFirstWordViewport());
  const onStatus = vi.fn();
  const onCaret = vi.fn();
  const onToggle = vi.fn();
  const onChange = vi.fn();
  const { container } = render(<EditorViewport document={{ ...untitled, readOnly: true }} onDocumentChange={onChange}
    onStatus={onStatus} onCaret={onCaret} onToggleReadOnly={onToggle} />);
  const input = await screen.findByRole("textbox", { name: "Untitled editor" });
  await waitFor(() => expect(onCaret).toHaveBeenCalledWith(expect.objectContaining({ line: "1", carets: 1 })));
  expect(container.querySelector(".editor-stage")).toHaveClass("read-only");

  vi.mocked(executeCommand).mockClear();
  fireEvent.keyDown(input, { key: "x" });
  fireEvent.keyDown(input, { key: "Backspace" });
  fireEvent.keyDown(input, { key: "Enter" });
  fireEvent.keyDown(input, { key: "z", ctrlKey: true });
  fireEvent.paste(input, { clipboardData: { getData: () => "pasted" } });
  expect(onStatus).toHaveBeenLastCalledWith("Read-only: switch to Edit mode to change this document");
  expect(onStatus).toHaveBeenCalledTimes(5);
  expect(vi.mocked(executeCommand).mock.calls.map(([request]) => request.id)).toEqual([]);
  expect(onChange).not.toHaveBeenCalled();

  const mode = screen.getByRole("button", { name: "Read-only mode: switch to Edit mode" });
  expect(mode).toHaveTextContent("READ ONLY");
  fireEvent.click(mode);
  expect(onToggle).toHaveBeenCalledTimes(1);
});

test("saving reloads the visible lines so unsaved markers become saved markers", async () => {
  const line = { line: "1", startByte: "0", endByte: "4", text: "one", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false };
  const viewport = {
    documentId: "document-1", revision: "3", documentByteLen: "4", encoding: "utf8" as const,
    selections: [{ anchorByte: "0", activeByte: "0" }], bookmarks: [],
    lineChanges: { available: true, approximate: false },
  };
  vi.mocked(getViewport).mockResolvedValueOnce({ ...viewport, lines: [{ ...line, change: "unsaved" }] });
  const edited = { ...untitled, revision: "3", byteLen: "4", dirty: true };
  const onChange = vi.fn();
  const onStatus = vi.fn();
  const { container, rerender } = render(<EditorViewport document={edited} onDocumentChange={onChange}
    onStatus={onStatus} showLineChanges />);
  await waitFor(() => expect(container.querySelectorAll(".line-change.unsaved")).toHaveLength(1));
  vi.mocked(getViewport).mockResolvedValueOnce({ ...viewport, lines: [{ ...line, change: "saved" }] });
  rerender(<EditorViewport document={{ ...edited, dirty: false }} onDocumentChange={onChange}
    onStatus={onStatus} showLineChanges />);
  await waitFor(() => expect(container.querySelectorAll(".line-change.saved")).toHaveLength(1));
  expect(container.querySelectorAll(".line-change.unsaved")).toHaveLength(0);
  expect(getViewport).toHaveBeenCalledTimes(2);
});

test("line change indicators distinguish unsaved, saved, and deleted lines without relying on color", async () => {
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-1", revision: "0", documentByteLen: "11", encoding: "utf8",
    lines: [
      { line: "1", startByte: "0", endByte: "4", text: "one", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false, change: "unsaved" },
      { line: "2", startByte: "4", endByte: "8", text: "two", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false, change: "saved" },
      { line: "3", startByte: "8", endByte: "11", text: "six", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false, change: "unsavedDeletion" },
    ],
    selections: [{ anchorByte: "0", activeByte: "0" }],
    bookmarks: [],
    lineChanges: { available: true, approximate: false },
  });
  vi.mocked(getDocumentMap).mockResolvedValueOnce({
    documentId: "document-1", revision: "0", documentByteLen: "11", totalLines: 3, samples: [],
    changes: [{ byteOffset: "0", line: "1", change: "unsaved" }, { byteOffset: "4", line: "2", change: "saved" }],
    lineChanges: { available: true, approximate: false },
  });
  const onToggle = vi.fn();
  const { container, rerender } = render(<EditorViewport document={untitled} onDocumentChange={() => undefined}
    onStatus={() => undefined} showLineChanges onToggleLineChanges={onToggle} />);
  await screen.findByRole("textbox", { name: "Untitled editor" });
  const markers = [...container.querySelectorAll<HTMLElement>(".line-change")];
  expect(markers.map((marker) => marker.dataset.lineChange)).toEqual(["unsaved", "saved", "unsavedDeletion"]);
  expect(markers.map((marker) => marker.title)).toEqual([
    "Unsaved change", "Changed and saved in this session", "Unsaved deletion",
  ]);
  expect(screen.getByRole("status")).toHaveTextContent("Line 1, column 1, unsaved change");
  await waitFor(() => expect(container.querySelectorAll(".document-map-change")).toHaveLength(2));

  const toggle = screen.getByRole("button", { name: "Hide line change indicators" });
  expect(toggle).toHaveAttribute("aria-pressed", "true");
  fireEvent.click(toggle);
  expect(onToggle).toHaveBeenCalledTimes(1);

  rerender(<EditorViewport document={untitled} onDocumentChange={() => undefined}
    onStatus={() => undefined} showLineChanges={false} onToggleLineChanges={onToggle} />);
  expect(container.querySelectorAll(".line-change")).toHaveLength(0);
  expect(container.querySelectorAll(".document-map-change")).toHaveLength(0);
  expect(screen.getByRole("button", { name: "Show line change indicators" })).toHaveAttribute("aria-pressed", "false");
});

test("reports unavailable line change markers instead of showing a clean gutter", async () => {
  vi.mocked(getViewport).mockResolvedValueOnce({
    ...selectedFirstWordViewport(), lineChanges: { available: false, approximate: false },
  });
  const { container } = render(<EditorViewport document={untitled} onDocumentChange={() => undefined} onStatus={() => undefined} />);
  await screen.findByRole("textbox", { name: "Untitled editor" });
  expect(container.querySelectorAll(".line-change")).toHaveLength(0);
  expect(screen.getByText(/line changes unavailable until saved/)).toBeInTheDocument();
});

test("editor tools are icon controls with names, shortcut tooltips, and pressed states", async () => {
  await renderPointerEditor();
  const tools = screen.getByRole("toolbar", { name: "Editor tools" });
  for (const [name, title] of [
    ["Find", "Find and replace (Ctrl/⌘ F)"], ["Go to line", "Go to line (Ctrl/⌘ G)"],
    ["Toggle bookmark", "Toggle bookmark (Ctrl/⌘ F2)"], ["Wrap", "Word wrap"],
    ["Hide line change indicators", "Hide Line Change Indicators"], ["Toggle document map", "Hide document map"],
  ]) {
    const button = screen.getByRole("button", { name });
    expect(tools).toContainElement(button);
    expect(button).toHaveAttribute("title", title);
    expect(button.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
  }
  const wrap = screen.getByRole("button", { name: "Wrap" });
  expect(wrap).toHaveAttribute("aria-pressed", "false");
  fireEvent.click(wrap);
  expect(wrap).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("button", { name: "Fold" })).toBeDisabled();
});

test("goes to a line through a bounded core query and places the caret there", async () => {
  const onStatus = vi.fn();
  const { editor } = await renderPointerEditor(onStatus);
  vi.mocked(getViewportLine).mockResolvedValueOnce({
    documentId: "document-1", revision: "0", documentByteLen: "900", encoding: "utf8",
    lines: [{ line: "40", startByte: "400", endByte: "406", text: "forty", sourceByteWidths: [1, 1, 1, 1, 1], continuation: false, truncated: false }],
    selections: [{ anchorByte: "0", activeByte: "0" }], bookmarks: [],
  });
  fireEvent.keyDown(editor, { key: "g", ctrlKey: true });
  const input = screen.getByRole("textbox", { name: "Line number" });
  fireEvent.change(input, { target: { value: "x" } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(onStatus).toHaveBeenCalledWith("Enter a line number");
  expect(getViewportLine).not.toHaveBeenCalled();

  fireEvent.change(input, { target: { value: "40" } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(await screen.findByText("forty")).toBeInTheDocument();
  expect(getViewportLine).toHaveBeenCalledWith({ documentId: "document-1", line: "40", maxBytes: 32 * 1024 });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.setSelections", arguments: { selections: [{ anchorByte: "400", activeByte: "400" }] },
  })));
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Line number" })).not.toBeInTheDocument());
});

test("highlights every visible occurrence of the find query when enabled", async () => {
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-1", revision: "0", documentByteLen: "22", encoding: "utf8",
    lines: [
      { line: "1", startByte: "0", endByte: "12", text: "one One one", sourceByteWidths: Array(11).fill(1), continuation: false, truncated: false },
      { line: "2", startByte: "12", endByte: "22", text: "none at all", sourceByteWidths: Array(11).fill(1), continuation: false, truncated: false },
    ],
    selections: [{ anchorByte: "21", activeByte: "21" }], bookmarks: [],
  });
  const { container } = render(<EditorViewport document={untitled} onDocumentChange={() => undefined} onStatus={() => undefined} />);
  await screen.findByRole("textbox", { name: "Untitled editor" });
  fireEvent.click(screen.getByRole("button", { name: "Find" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Find text" }), { target: { value: "one" } });
  expect(container.querySelectorAll(".find-highlight")).toHaveLength(0);
  const highlight = screen.getByRole("button", { name: "Highlight matches" });
  fireEvent.click(highlight);
  expect(highlight).toHaveAttribute("aria-pressed", "true");
  expect([...container.querySelectorAll(".find-highlight")].map((item) => item.textContent)).toEqual(["one", "one", "one"]);
  fireEvent.click(screen.getByRole("button", { name: "Match case" }));
  expect(container.querySelectorAll(".find-highlight")).toHaveLength(4);
  // An invalid pattern highlights nothing instead of failing.
  fireEvent.click(screen.getByRole("button", { name: "Regular expression" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Find text" }), { target: { value: "(" } });
  expect(container.querySelectorAll(".find-highlight")).toHaveLength(0);
});

test("column editing mode makes a plain drag select a rectangle", async () => {
  const { lines } = await renderPointerEditor();
  const column = screen.getByRole("button", { name: "Column editing" });
  fireEvent.click(column);
  expect(column).toHaveAttribute("aria-pressed", "true");
  vi.mocked(executeCommand).mockResolvedValueOnce({
    anchorByte: "1", activeByte: "6", anchorLine: "1", activeLine: "2", anchorColumn: "1", activeColumn: "2",
  });
  fireEvent.pointerDown(lines[0], { button: 0, pointerId: 2, clientX: 72 });
  fireEvent.pointerMove(lines[1], { buttons: 1, pointerId: 2, clientX: 80 });
  fireEvent.pointerUp(window, { pointerId: 2 });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.setRectangularSelection",
    arguments: { anchorByte: "1", activeByte: "6", anchorColumn: "1", activeColumn: "2" },
  })));
  expect(await screen.findByText("Rectangular selection")).toBeInTheDocument();
});

test("a second view reloads in place when another pane edits the same document", async () => {
  const { rerender } = render(<EditorViewport document={untitled} onDocumentChange={() => undefined}
    onStatus={() => undefined} autoFocus={false} />);
  const editor = await screen.findByRole("textbox", { name: "Untitled editor" });
  // A view in an inactive pane does not take focus.
  expect(editor).not.toHaveFocus();
  expect(getViewport).toHaveBeenCalledTimes(1);
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-1", revision: "1", documentByteLen: "8", encoding: "utf8",
    lines: [
      { line: "1", startByte: "0", endByte: "5", text: "one!", sourceByteWidths: [1, 1, 1, 1], continuation: false, truncated: false, change: "unsaved" },
      { line: "2", startByte: "5", endByte: "8", text: "two", sourceByteWidths: [1, 1, 1], continuation: false, truncated: false },
    ],
    selections: [{ anchorByte: "4", activeByte: "4" }], bookmarks: [],
    lineChanges: { available: true, approximate: false },
  });
  rerender(<EditorViewport document={{ ...untitled, revision: "1", byteLen: "8" }} onDocumentChange={() => undefined}
    onStatus={() => undefined} autoFocus={false} />);
  expect(await screen.findByText("one!")).toBeInTheDocument();
  expect(getViewport).toHaveBeenCalledTimes(2);
  expect(document.querySelectorAll(".line-change.unsaved")).toHaveLength(1);
  // Once current, no further reloads are issued for the same revision.
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 120)); });
  expect(getViewport).toHaveBeenCalledTimes(2);
});

test("a zero-width column shows a caret on every row and stays active while typing", async () => {
  const column = { anchorByte: "1", activeByte: "5", anchorLine: "1", activeLine: "2", anchorColumn: "1", activeColumn: "1" };
  vi.mocked(getViewport).mockResolvedValueOnce({
    ...selectedFirstWordViewport(), selections: [{ anchorByte: "5", activeByte: "5" }], rectangularSelection: column,
  });
  const { container, editor } = await renderPointerEditor();
  // One caret per row of the column, not just on the active row.
  expect(container.querySelectorAll(".editor-line .caret")).toHaveLength(2);
  expect(screen.getByText("Rectangular selection")).toBeInTheDocument();

  // The core keeps the column after an insert, so the refreshed view still has both carets.
  vi.mocked(getViewport).mockResolvedValueOnce({
    documentId: "document-1", revision: "1", documentByteLen: "9", encoding: "utf8",
    lines: [
      { line: "1", startByte: "0", endByte: "5", text: "oxne", sourceByteWidths: [1, 1, 1, 1], continuation: false, truncated: false },
      { line: "2", startByte: "5", endByte: "9", text: "txwo", sourceByteWidths: [1, 1, 1, 1], continuation: false, truncated: false },
    ],
    selections: [{ anchorByte: "7", activeByte: "7" }], bookmarks: [],
    rectangularSelection: { ...column, anchorByte: "2", activeByte: "7", anchorColumn: "2", activeColumn: "2" },
  });
  fireEvent.keyDown(editor, { key: "x" });
  expect(await screen.findByText("oxne")).toBeInTheDocument();
  expect(container.querySelectorAll(".editor-line .caret")).toHaveLength(2);
  expect(screen.getByText("Rectangular selection")).toBeInTheDocument();
});

test("a Search panel request runs find, replace, and replace all with its own options", async () => {
  const props = { document: untitled, onDocumentChange: () => undefined, onStatus: vi.fn() };
  const request = { query: "t.o", replacement: "2", regex: true, caseSensitive: false };
  // A request that already existed when the view mounted is not replayed.
  const { rerender } = render(<EditorViewport {...props} searchRequest={{ id: 4, action: "next", ...request }} />);
  await screen.findByRole("textbox", { name: "Untitled editor" });
  expect(executeCommand).not.toHaveBeenCalled();

  vi.mocked(executeCommand).mockResolvedValueOnce({ selection: { anchorByte: "4", activeByte: "7" }, wrapped: false });
  rerender(<EditorViewport {...props} searchRequest={{ id: 5, action: "next", ...request }} />);
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "search.find", context: { windowId: "main", documentId: "document-1" },
    arguments: { query: "t.o", options: { mode: "regex", caseSensitive: false }, startByte: "0", direction: "next", wrap: true },
  }));
  expect(await screen.findByText("two")).toHaveClass("selection");

  // Replace acts on the match that was just found.
  vi.mocked(executeCommand).mockResolvedValueOnce({ documentId: "document-1", name: "Untitled", byteLen: "5", revision: "1", encoding: "utf8" });
  rerender(<EditorViewport {...props} searchRequest={{ id: 6, action: "replace", ...request }} />);
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "editor.replace", arguments: { expectedRevision: "0", startByte: "4", endByte: "7", text: "2" },
  })));

  vi.mocked(executeCommand).mockResolvedValueOnce({
    document: { documentId: "document-1", name: "Untitled", byteLen: "5", revision: "2", encoding: "utf8" }, replacements: 3,
  });
  rerender(<EditorViewport {...props} searchRequest={{ id: 7, action: "replaceAll", ...request, query: "o" }} />);
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "search.replaceAll",
    arguments: expect.objectContaining({ query: "o", replacement: "2", options: { mode: "regex", caseSensitive: false } }),
  })));
  await waitFor(() => expect(props.onStatus).toHaveBeenCalledWith("Replaced 3 matches"));
});
