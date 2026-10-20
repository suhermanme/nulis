import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { columnRange, FindReplacePanel, REGEX_HELP, type FindTab } from "./FindReplacePanel";
import { executeCommand } from "./ipc";
import type { DocumentSummary } from "./contracts";

vi.mock("./ipc", () => ({ executeCommand: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: vi.fn() }));

const document: DocumentSummary = { documentId: "document-1", name: "notes.txt", byteLen: "10", revision: "0", encoding: "utf8" };

function panel(tab: FindTab, extra: Partial<Parameters<typeof FindReplacePanel>[0]> = {}) {
  const props = {
    tab, onTab: vi.fn(), onClose: vi.fn(), activeDocument: document, textDocuments: [document],
    onEditorSearch: vi.fn(), onOpenMatch: vi.fn(), onDocumentChange: vi.fn(), onResults: vi.fn(),
    onChooseFolder: vi.fn(async () => null), onStatus: vi.fn(), framed: false, ...extra,
  };
  return { ...render(<FindReplacePanel {...props} />), props };
}

function optionLabels() {
  return within(screen.getByRole("tabpanel")).getAllByRole("checkbox").map((box) => box.closest("label")?.textContent);
}

beforeEach(() => {
  localStorage.clear();
  vi.mocked(executeCommand).mockReset();
  vi.mocked(executeCommand).mockResolvedValue(null);
});

afterEach(cleanup);

it("lays the options out per tab, with column search only where one document is searched", () => {
  const { unmount } = panel("find");
  expect(optionLabels()).toEqual([
    "Match case", "Highlight all items found", "Match whole word", "List lines containing find string",
    "Search in column:", "Use regular expressions", "Close after find",
  ]);
  unmount();
  const replace = panel("replace");
  expect(optionLabels()).toEqual(["Match case", "Match whole word", "Search in column:", "Use regular expressions", "Close after replace all"]);
  replace.unmount();
  panel("findInFiles");
  expect(optionLabels()).toEqual(["Include subdirectories", "Match case", "Match whole word", "Use regular expressions"]);
});

it("sends the column range with the search and keeps the fields off until the box is ticked", () => {
  const { props } = panel("find");
  const from = screen.getByRole("textbox", { name: "From column" });
  expect(from).toBeDisabled();
  fireEvent.click(screen.getByRole("checkbox", { name: "Search in column:" }));
  fireEvent.change(from, { target: { value: "3" } });
  fireEvent.change(screen.getByRole("textbox", { name: "To column" }), { target: { value: "8" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Find what" }), { target: { value: "id" } });
  fireEvent.click(screen.getByRole("button", { name: "Next" }));
  return waitFor(() => expect(props.onEditorSearch).toHaveBeenCalledWith(expect.objectContaining({
    action: "next", query: "id", columns: { from: 3, to: 8 },
  })));
});

it("explains a bad column range and holds the search back", () => {
  panel("find");
  fireEvent.click(screen.getByRole("checkbox", { name: "Search in column:" }));
  fireEvent.change(screen.getByRole("textbox", { name: "From column" }), { target: { value: "9" } });
  fireEvent.change(screen.getByRole("textbox", { name: "To column" }), { target: { value: "2" } });
  expect(screen.getByRole("status")).toHaveTextContent("To (2) is before From (9)");
  expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
});

it("checks a regular expression as it is typed and shows the engine's message", async () => {
  vi.mocked(executeCommand).mockRejectedValue(new Error(
    "invalid regular expression: lookahead and lookbehind ((?=…), (?!…), (?<=…), (?<!…)) are not supported (at character 1)",
  ));
  panel("find");
  fireEvent.click(screen.getByRole("checkbox", { name: "Use regular expressions" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Find what" }), { target: { value: "(?<=id=)\\d+" } });
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("lookbehind"));
  expect(executeCommand).toHaveBeenCalledWith({
    id: "search.validatePattern", context: { windowId: "main" },
    arguments: { query: "(?<=id=)\\d+", options: { mode: "regex", caseSensitive: false, wholeWord: false } },
  });
  expect(screen.getByRole("button", { name: "Count All" })).toBeDisabled();

  vi.mocked(executeCommand).mockResolvedValue(null);
  fireEvent.change(screen.getByRole("textbox", { name: "Find what" }), { target: { value: "id=\\d+" } });
  await waitFor(() => expect(screen.getByRole("status")).toBeEmptyDOMElement());
  expect(screen.getByRole("status")).not.toHaveClass("find-error");
  expect(screen.getByRole("button", { name: "Count All" })).toBeEnabled();
});

it("keeps highlights in step with the query and options while Highlight all is on", async () => {
  const { props } = panel("find");
  fireEvent.change(screen.getByRole("textbox", { name: "Find what" }), { target: { value: "id" } });
  // Off: typing sends nothing.
  await new Promise((resolve) => setTimeout(resolve, 300));
  expect(props.onEditorSearch).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("checkbox", { name: "Highlight all items found" }));
  await waitFor(() => expect(props.onEditorSearch).toHaveBeenLastCalledWith(expect.objectContaining({
    action: "highlight", query: "id", highlight: true,
  })));
  fireEvent.click(screen.getByRole("checkbox", { name: "Match case" }));
  await waitFor(() => expect(props.onEditorSearch).toHaveBeenLastCalledWith(expect.objectContaining({
    action: "highlight", query: "id", caseSensitive: true,
  })));
  // Turning it off clears the highlights once.
  fireEvent.click(screen.getByRole("checkbox", { name: "Highlight all items found" }));
  await waitFor(() => expect(props.onEditorSearch).toHaveBeenLastCalledWith(expect.objectContaining({ highlight: false })));
});

it("remembers the options for the next time the window opens", () => {
  const first = panel("find");
  fireEvent.click(screen.getByRole("checkbox", { name: "Use regular expressions" }));
  fireEvent.click(screen.getByRole("checkbox", { name: "Match case" }));
  fireEvent.click(screen.getByRole("checkbox", { name: "Search in column:" }));
  fireEvent.change(screen.getByRole("textbox", { name: "From column" }), { target: { value: "4" } });
  first.unmount();
  panel("find");
  expect(screen.getByRole("checkbox", { name: "Use regular expressions" })).toBeChecked();
  expect(screen.getByRole("checkbox", { name: "Match case" })).toBeChecked();
  expect(screen.getByRole("textbox", { name: "From column" })).toHaveValue("4");
});

it("opens the regex section of Nulis Help from the link", () => {
  const onHelp = vi.fn();
  panel("find", { onHelp });
  fireEvent.click(screen.getByRole("button", { name: "Regex help" }));
  expect(onHelp).toHaveBeenCalledWith(REGEX_HELP);
});

it("reads column fields as whole numbers from 1", () => {
  expect(columnRange("2", "")).toEqual({ columns: { from: 2 } });
  expect(columnRange("", "7")).toEqual({ columns: { to: 7 } });
  expect(columnRange(" 3 ", "3")).toEqual({ columns: { from: 3, to: 3 } });
  expect(columnRange("0", "")).toEqual({ error: "Columns are whole numbers from 1" });
  expect(columnRange("1.5", "")).toEqual({ error: "Columns are whole numbers from 1" });
  expect(columnRange("", "")).toEqual({ error: "Enter a From or To column" });
});

it("counts every match in the background, shows progress, and can be cancelled", async () => {
  const reports = [
    { matches: "1234567", lines: "900000", scannedBytes: String(13 * 1024 ** 3), totalBytes: String(31.8 * 1024 ** 3), documents: 1, documentsWithMatches: 0, done: false, cancelled: false },
    { matches: "2500000", lines: "1800000", scannedBytes: String(31.8 * 1024 ** 3), totalBytes: String(31.8 * 1024 ** 3), documents: 1, documentsWithMatches: 1, done: true, cancelled: false },
  ];
  vi.mocked(executeCommand).mockImplementation(async (request) => {
    if (request.id === "search.countStart") return { jobId: 7 };
    if (request.id === "search.countProgress") return reports.shift();
    return null;
  });
  panel("find");
  fireEvent.change(screen.getByRole("textbox", { name: "Find what" }), { target: { value: "aadmin1" } });
  fireEvent.click(screen.getByRole("button", { name: "Count All" }));
  expect(await screen.findByRole("button", { name: "Cancel" })).toBeEnabled();
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Counting… 40% (13.0 GB of 31.8 GB) · 1,234,567 matches so far"));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("2,500,000 matches on 1,800,000 lines"));
  expect(screen.getByRole("status")).not.toHaveTextContent("limit");
  expect(screen.getByRole("button", { name: "Count All" })).toBeEnabled();
  expect(executeCommand).toHaveBeenCalledWith({
    id: "search.countStart", context: { windowId: "main" },
    arguments: { query: "aadmin1", options: { mode: "literal", caseSensitive: false, wholeWord: false }, targets: [{ documentId: "document-1" }] },
  });

  // Cancel asks the core to stop; the result says where it stopped.
  let cancelled = false;
  vi.mocked(executeCommand).mockImplementation(async (request) => {
    if (request.id === "search.countStart") return { jobId: 8 };
    if (request.id === "search.countCancel") { cancelled = true; return null; }
    if (request.id === "search.countProgress") {
      return { matches: "10", lines: "4", scannedBytes: "500", totalBytes: "1000", documents: 1, documentsWithMatches: 1, done: cancelled, cancelled };
    }
    return null;
  });
  fireEvent.click(screen.getByRole("button", { name: "Count All" }));
  fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Stopped at 50%: 10 matches on 4 lines so far"));
  expect(executeCommand).toHaveBeenCalledWith({ id: "search.countCancel", context: { windowId: "main" }, arguments: { jobId: 8 } });
});
