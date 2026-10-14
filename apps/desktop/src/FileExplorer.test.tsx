import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { ExecuteCommandRequest } from "./contracts";
import { FileExplorer, fileFilter, pathAncestors, type ExplorerTab } from "./FileExplorer";
import { executeCommand } from "./ipc";

vi.mock("./ipc", () => ({ executeCommand: vi.fn() }));

const dir = (name: string, parent: string) => ({ name, path: `${parent === "/" ? "" : parent}/${name}`, kind: "directory" });
const file = (name: string, parent: string) => ({ name, path: `${parent}/${name}`, kind: "file", byteLen: "3" });
const TREE: Record<string, unknown[]> = {
  "/": [dir("Applications", "/"), dir("Users", "/"), dir("etc", "/")],
  "/Users": [dir("medys", "/Users"), dir("Shared", "/Users")],
  "/Users/medys": [dir("Documents", "/Users/medys"), file("notes.md", "/Users/medys"), file("todo.txt", "/Users/medys"),
    { name: "broken", path: "/Users/medys/broken", kind: "other" }],
  "/Users/medys/Documents": [],
  "/Volumes/DATABANK": [file("backup.sql", "/Volumes/DATABANK")],
};

beforeEach(() => {
  vi.mocked(executeCommand).mockReset().mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "places.roots") return [
      { name: "WD_MACOS", path: "/", kind: "computer" },
      { name: "DATABANK", path: "/Volumes/DATABANK", kind: "volume" },
      { name: "medys", path: "/Users/medys", kind: "home" },
    ] as never;
    const { path, showHidden } = request.arguments as { path: string; showHidden: boolean };
    if (path === "/etc") throw new Error("Permission denied");
    const entries = [...(TREE[path] ?? [])];
    if (showHidden && path === "/Users/medys") entries.unshift(file(".zshrc", path));
    return { path, entries, truncated: false } as never;
  });
});

afterEach(cleanup);

function renderExplorer(tab: ExplorerTab = "places") {
  const props = { onTab: vi.fn(), onShowDocument: vi.fn(), onOpenPath: vi.fn(), onStatus: vi.fn() };
  const view = render(<FileExplorer tab={tab} {...props} activeDocumentId="document-2" folder={<p>folder tree</p>}
    documents={[
      { documentId: "document-1", name: "a.txt", byteLen: "1", revision: "0", encoding: "utf8", sourcePath: "/tmp/a.txt" },
      { documentId: "document-2", name: "draft.md", byteLen: "1", revision: "1", encoding: "utf8", dirty: true },
    ]} />);
  return { ...view, ...props };
}

const rowNames = () => within(screen.getByRole("tree", { name: "Places" })).getAllByRole("button").map((row) => row.textContent);

test("Places lists the computer and volumes with the home folder already open", async () => {
  const { onOpenPath } = renderExplorer();
  await screen.findByRole("button", { name: /notes\.md/ });
  expect(rowNames()).toEqual([
    "▾WD_MACOS", "▸Applications", "▾Users", "▾medys", "▸Documents", "notes.md", "todo.txt", "broken",
    "▸Shared", "▸etc", "▸DATABANK",
  ]);
  // Only the chain down to home was listed, one bounded directory at a time.
  expect(vi.mocked(executeCommand).mock.calls.filter(([request]) => request.id === "places.list")
    .map(([request]) => request.arguments)).toEqual([
    { path: "/", maxEntries: 500, showHidden: false },
    { path: "/Users", maxEntries: 500, showHidden: false },
    { path: "/Users/medys", maxEntries: 500, showHidden: false },
  ]);
  expect(screen.getByRole("button", { name: /medys/ })).toHaveClass("home");
  expect(screen.getByRole("button", { name: /broken/ })).toBeDisabled();

  fireEvent.click(screen.getByRole("button", { name: /notes\.md/ }));
  expect(onOpenPath).toHaveBeenCalledWith("/Users/medys/notes.md");
});

test("folders load lazily, collapse, and report a folder that cannot be read", async () => {
  const { onStatus } = renderExplorer();
  await screen.findByRole("button", { name: /notes\.md/ });
  fireEvent.click(screen.getByRole("button", { name: /DATABANK/ }));
  expect(await screen.findByRole("button", { name: /backup\.sql/ })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: /Documents/ }));
  expect(await screen.findByText("Empty")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: /etc/ }));
  await waitFor(() => expect(onStatus).toHaveBeenCalledWith("Permission denied"));
  expect(screen.getByRole("button", { name: /etc/ })).toHaveAttribute("aria-expanded", "false");

  // Collapsing keeps the listing; reopening does not ask the core again.
  const calls = vi.mocked(executeCommand).mock.calls.length;
  fireEvent.click(screen.getByRole("button", { name: "Users" }));
  expect(screen.getByRole("button", { name: "Users" })).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByRole("button", { name: /notes\.md/ })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Users" }));
  expect(screen.getByRole("button", { name: /notes\.md/ })).toBeInTheDocument();
  expect(vi.mocked(executeCommand).mock.calls).toHaveLength(calls);
});

test("the filter narrows files but never folders, and hidden files are opt-in", async () => {
  renderExplorer();
  await screen.findByRole("button", { name: /notes\.md/ });
  fireEvent.change(screen.getByRole("searchbox", { name: "File filter" }), { target: { value: "*.md; *.rs" } });
  expect(screen.getByRole("button", { name: /notes\.md/ })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /todo\.txt/ })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: /Documents/ })).toBeInTheDocument();
  fireEvent.change(screen.getByRole("searchbox", { name: "File filter" }), { target: { value: "" } });
  expect(screen.getByRole("button", { name: /todo\.txt/ })).toBeInTheDocument();

  expect(screen.queryByRole("button", { name: /\.zshrc/ })).not.toBeInTheDocument();
  const hidden = screen.getByRole("button", { name: "Show hidden files" });
  fireEvent.click(hidden);
  expect(await screen.findByRole("button", { name: /\.zshrc/ })).toBeInTheDocument();
  expect(hidden).toHaveAttribute("aria-pressed", "true");
});

test("the Open tab lists open files and the Folder tab shows the opened folder", async () => {
  const { onShowDocument, onTab, rerender } = renderExplorer("open");
  expect(executeCommand).not.toHaveBeenCalled();
  const open = screen.getByRole("tree", { name: "Open files" });
  expect(within(open).getAllByRole("button").map((row) => row.textContent)).toEqual(["a.txt", "draft.md●"]);
  expect(within(open).getByRole("button", { name: /draft\.md/ })).toHaveClass("home");
  fireEvent.click(within(open).getByRole("button", { name: /a\.txt/ }));
  expect(onShowDocument).toHaveBeenCalledWith("document-1");

  expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Open", "Places", "Folder"]);
  fireEvent.click(screen.getByRole("tab", { name: "Folder" }));
  expect(onTab).toHaveBeenCalledWith("folder");
  rerender(<FileExplorer tab="folder" onTab={onTab} onShowDocument={onShowDocument} onOpenPath={vi.fn()} onStatus={vi.fn()}
    activeDocumentId={null} documents={[]} folder={<p>folder tree</p>} />);
  expect(screen.getByText("folder tree")).toBeInTheDocument();
});

test("path and filter helpers handle both path styles", () => {
  expect(pathAncestors("/Users/medys")).toEqual(["/", "/Users", "/Users/medys"]);
  expect(pathAncestors("/")).toEqual(["/"]);
  expect(pathAncestors("C:\\Users\\me")).toEqual(["C:\\", "C:\\Users", "C:\\Users\\me"]);
  expect(fileFilter("")("anything")).toBe(true);
  expect(fileFilter("*")("anything")).toBe(true);
  const filter = fileFilter("*.MD, notes?.txt");
  expect(filter("README.md")).toBe(true);
  expect(filter("notes1.txt")).toBe(true);
  expect(filter("notes10.txt")).toBe(false);
  expect(filter("a.mdx")).toBe(false);
  // Regex metacharacters in a pattern are literal.
  expect(fileFilter("a+b(1).txt")("a+b(1).txt")).toBe(true);
});
