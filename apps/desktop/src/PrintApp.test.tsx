import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, expect, test, vi } from "vitest";
import type { PrintPageFrame, PrintedPage } from "./contracts";
import type { PrintWindowContent } from "./ipc";

const printThisWindow = vi.fn<(paper: unknown) => Promise<void>>(() => Promise.resolve());
const executeCommand = vi.fn<(request: { id: string; arguments: { start: number; count: number } }) => Promise<unknown>>();
let content: PrintWindowContent = { kind: "pages" };
vi.mock("./ipc", () => ({
  printThisWindow: (paper: unknown) => printThisWindow(paper),
  executeCommand: (request: { id: string; arguments: { start: number; count: number } }) => executeCommand(request),
  printWindowContent: () => Promise.resolve(content),
  printWindowMarkdownImage: () => Promise.reject(new Error("no images")),
}));
vi.mock("./windowTheme", () => ({ useWindowTheme: () => ({}) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => undefined) }));

const { PrintApp, PAGE_BATCH, pageSizeRule } = await import("./PrintApp");

afterEach(() => {
  cleanup();
  printThisWindow.mockClear();
  executeCommand.mockReset();
  content = { kind: "pages" };
  window.localStorage.clear();
});

const frame: PrintPageFrame = {
  widthMm: 297, heightMm: 210, paperWidthMm: 210, paperHeightMm: 297, landscape: true,
  marginLeftPt: 42.5, headerTopPt: 42.5, bodyTopPt: 60, footerTopPt: 550,
  fontSizePt: 9, lineHeightPt: 11.7, cellPt: 5.4, columns: 140, rows: 40,
  hasHeader: true, hasFooter: true, background: null, ruleColor: "#7b858c",
  styles: [{ color: "#1f2328", bold: false }, { color: "#8c959f", bold: false }, { color: "#7b858c", bold: false },
    { color: "#57606a", bold: false }, { color: "#5c7369", bold: false }, { color: "#9a3b2b", bold: true }],
};

function page(number: number): PrintedPage {
  return {
    number,
    header: [[0, 8, 3, "notes.md"]],
    footer: [[60, 6, 3, `Page ${number}`]],
    rows: [[[0, 3, 2, `  ${number}`], [4, 31, 0, "<script>alert(1)</script></section>"]], [], [[4, 2, 5, "fn"]]],
    rules: [1],
  };
}

/** `print.pages` over `total` pages, in the batches the window asks for. */
function servePages(total: number) {
  executeCommand.mockImplementation((request) => {
    const { start, count } = request.arguments;
    const pages = Array.from({ length: Math.max(0, Math.min(count, total - start)) }, (_, index) => JSON.stringify(page(start + index + 1)));
    return Promise.resolve({ printId: 1, title: "notes.md", frame, total, start, pages });
  });
}

test("Print Preview reads the prepared pages in batches and draws each piece at its cell", async () => {
  servePages(PAGE_BATCH + 3);
  render(<PrintApp />);
  await waitFor(() => expect(screen.getAllByRole("region", { name: /^Page \d+ of 53$/ })).toHaveLength(53));
  expect(executeCommand).toHaveBeenCalledTimes(2);
  expect(executeCommand.mock.calls[1][0].arguments).toEqual({ start: PAGE_BATCH, count: PAGE_BATCH });
  const first = screen.getByRole("region", { name: "Page 1 of 53" });
  expect(first).toHaveStyle({ width: "297mm", "--page-height": "210mm" });
  // Document text that looks like HTML stays text.
  expect(first).toHaveTextContent("<script>alert(1)</script></section>");
  expect(first.querySelector("script")).toBeNull();
  const keyword = screen.getAllByText("fn")[0];
  expect(keyword).toHaveStyle({ left: `${4 * 5.4}pt`, fontWeight: "700", color: "#9a3b2b" });
  expect(first.querySelector(".print-rule")).toHaveStyle({ top: `${60 + 1.5 * 11.7}pt` });
  expect(screen.getByRole("status")).toHaveTextContent("53 pages");

  fireEvent.click(screen.getByRole("button", { name: "Print…" }));
  // The dialog takes the paper upright and the orientation separately.
  expect(printThisWindow).toHaveBeenCalledWith({ widthMm: 210, heightMm: 297, landscape: true });
});

test("Print waits until every page is in", async () => {
  servePages(3);
  executeCommand.mockImplementationOnce(() => new Promise(() => undefined));
  render(<PrintApp />);
  expect(screen.getByRole("button", { name: "Print…" })).toBeDisabled();
});

test("a Markdown document prints its formatted preview on the Page Setup paper", async () => {
  window.localStorage.setItem("nulis.printSettings.v1", JSON.stringify({ setup: { paper: "letter", margins: { top: 20, right: 20, bottom: 20, left: 20 } } }));
  content = { kind: "markdown", title: "notes.md", source: { documentId: "doc-1", revision: "3", text: "# Title\n\nSome *text* and <b>html</b>." } };
  render(<PrintApp />);
  expect(await screen.findByRole("heading", { name: "Title" })).toBeInTheDocument();
  expect(screen.getByLabelText("Markdown preview")).toHaveClass("print-markdown");
  fireEvent.click(screen.getByRole("button", { name: "Print…" }));
  expect(printThisWindow).toHaveBeenCalledWith({ widthMm: 215.9, heightMm: 279.4, landscape: false });
});

test("where the engine repeats padding on each page, Markdown margins leave no room for the browser's header", async () => {
  vi.stubGlobal("CSS", { supports: (property: string, value: string) => property === "box-decoration-break" && value === "clone" });
  try {
    window.localStorage.setItem("nulis.printSettings.v1", JSON.stringify({ setup: { margins: { top: 20, right: 20, bottom: 20, left: 20 } } }));
    content = { kind: "markdown", title: "notes.md", source: { documentId: "doc-1", revision: "3", text: "# Title" } };
    const { container } = render(<PrintApp />);
    await screen.findByRole("heading", { name: "Title" });
    const sheet = container.querySelector(".print-markdown-sheet");
    expect(sheet).toHaveClass("repeat-margins");
    expect((sheet as HTMLElement).style.getPropertyValue("--sheet-margins")).toBe("20mm 20mm 20mm 20mm");
  } finally {
    vi.unstubAllGlobals();
  }
});

test("with nothing prepared the window says how to print", async () => {
  content = { kind: "empty" };
  render(<PrintApp />);
  expect(await screen.findByText(/File ▸ Print… in the editor window/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Print…" })).toBeDisabled();
});

test("the page rule sets the paper and margins", () => {
  expect(pageSizeRule(297, 210)).toBe("@page { size: 297mm 210mm; margin: 0; }");
  expect(pageSizeRule(210, 297, { top: 10, right: 12, bottom: 14, left: 16 }))
    .toBe("@page { size: 210mm 297mm; margin: 10mm 12mm 14mm 16mm; }");
});
