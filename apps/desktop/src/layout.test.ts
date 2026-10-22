import { expect, test } from "vitest";
import type { DocumentSummary, LayoutNode } from "./contracts";
import {
  LAYOUT_LIMITS, PENDING_PREFIX, type LayoutResult, type WorkTab, activeDocumentId, closePane, documentTab,
  extractPane, groupTabs, isWorkspace, listPanes, movePane, nextPaneId, nextTabId, nextWorkspaceName,
  removeDocument, replaceDocument, resizeActivePane, sanitizeName, setRatio, splitPane, tabDocuments,
  tabsFromSession, tabsToSession, treeDepth, ungroupTab, viewCount,
} from "./layout";

const tab = (n: number, documentId: string) => documentTab(`tab-${n}`, `pane-${n}`, documentId);

function must(result: LayoutResult): WorkTab {
  if (result.error !== undefined) throw new Error(result.error);
  return result.tab;
}

function shape(node: LayoutNode): unknown {
  return node.kind === "pane" ? node.tabs[0] : { [node.direction]: [shape(node.first), shape(node.second)] };
}

test("dropping a tab into another tab's content makes one named workspace", () => {
  const a = tab(1, "a");
  expect(isWorkspace(a)).toBe(false);
  const grouped = must(groupTabs(a, "pane-1", "right", tab(2, "b"), "Workspace"));
  expect(isWorkspace(grouped)).toBe(true);
  expect(grouped).toMatchObject({ id: "tab-1", name: "Workspace", activePane: "pane-2" });
  expect(shape(grouped.root)).toEqual({ horizontal: ["a", "b"] });

  // Dropping onto one pane of a workspace splits only that pane and keeps its name.
  const named = { ...grouped, name: "monitoring" };
  const nested = must(groupTabs(named, "pane-2", "top", tab(3, "c"), "ignored"));
  expect(nested.name).toBe("monitoring");
  expect(shape(nested.root)).toEqual({ horizontal: ["a", { vertical: ["c", "b"] }] });
  expect(must(groupTabs(a, "pane-1", "left", tab(2, "b"), "W")).root).toMatchObject({ first: { id: "pane-2" } });
  expect(must(groupTabs(a, "pane-1", "bottom", tab(2, "b"), "W")).root).toMatchObject({ direction: "vertical", second: { id: "pane-2" } });
  expect(groupTabs(a, "missing", "left", tab(2, "b"), "W").error).toBeDefined();

  // A whole workspace can be dropped into another tab as a subtree.
  const merged = must(groupTabs(tab(4, "d"), "pane-4", "bottom", nested, "Workspace 2"));
  expect(shape(merged.root)).toEqual({ vertical: ["d", { horizontal: ["a", { vertical: ["c", "b"] }] }] });
  expect(tabDocuments(merged)).toEqual(["d", "a", "c", "b"]);
});

test("closing, extracting, moving, and ungrouping panes collapse predictably", () => {
  const workspace = must(groupTabs(must(groupTabs(tab(1, "a"), "pane-1", "right", tab(2, "b"), "W")), "pane-2", "bottom", tab(3, "c"), "W"));
  expect(shape(workspace.root)).toEqual({ horizontal: ["a", { vertical: ["b", "c"] }] });

  const closed = closePane(workspace, "pane-3");
  expect(closed && shape(closed.root)).toEqual({ horizontal: ["a", "b"] });
  expect(closed?.activePane).toBe("pane-1");
  expect(closePane(tab(1, "a"), "pane-1")).toBeNull();

  const lifted = extractPane(workspace, "pane-1", "tab-9");
  expect(lifted?.extracted).toMatchObject({ id: "tab-9", activePane: "pane-1" });
  expect(lifted?.remaining && shape(lifted.remaining.root)).toEqual({ vertical: ["b", "c"] });
  expect(extractPane(workspace, "missing", "tab-9")).toBeNull();

  const moved = must(movePane(workspace, "pane-3", "pane-1", "left"));
  expect(shape(moved.root)).toEqual({ horizontal: [{ horizontal: ["c", "a"] }, "b"] });
  expect(must(movePane(workspace, "pane-1", "pane-1", "left"))).toBe(workspace);

  const loose = ungroupTab(workspace, [workspace]);
  expect(loose.map((item) => [item.id, shape(item.root)])).toEqual([["tab-1", "a"], ["tab-2", "b"], ["tab-3", "c"]]);
  expect(loose.every((item) => !isWorkspace(item))).toBe(true);

  // A second view of one document counts separately and closes independently.
  const twice = must(splitPane(tab(1, "a"), "pane-1", "right", "a", "pane-2", "Workspace"));
  expect(viewCount([twice], "a")).toBe(2);
  expect(removeDocument(twice, "a")).toBeNull();
  expect(shape(replaceDocument(workspace, "b", "z").root)).toEqual({ horizontal: ["a", { vertical: ["z", "c"] }] });
  expect(activeDocumentId(workspace)).toBe("c");
});

test("workspace size is bounded by pane count and nesting depth", () => {
  let wide = tab(1, "a");
  let lastError: string | undefined;
  for (let index = 2; index <= LAYOUT_LIMITS.panes + 1; index += 1) {
    const panes = listPanes(wide.root);
    const target = panes[Math.floor((index - 2) / 2) % panes.length].id;
    const result = splitPane(wide, target, index % 2 ? "right" : "bottom", "a", `pane-${index}`, "W");
    if (result.error !== undefined) { lastError = result.error; break; }
    wide = result.tab;
  }
  expect(listPanes(wide.root).length).toBeLessThanOrEqual(LAYOUT_LIMITS.panes);
  expect(lastError).toBeDefined();

  let deep = tab(1, "a");
  let depthError: string | undefined;
  for (let index = 2; index < 12; index += 1) {
    const result = splitPane(deep, `pane-${index - 1}`, "right", "a", `pane-${index}`, "W");
    if (result.error !== undefined) { depthError = result.error; break; }
    deep = result.tab;
  }
  expect(treeDepth(deep.root)).toBe(LAYOUT_LIMITS.depth);
  expect(depthError).toMatch(/nested at most 6 levels/);
});

test("ratios stay bounded and names and identities stay unique", () => {
  const pair = must(groupTabs(tab(1, "a"), "pane-1", "right", tab(2, "b"), "Workspace"));
  expect(setRatio(pair, "", 5).root).toMatchObject({ ratio: 100 });
  expect(setRatio(pair, "", 5000).root).toMatchObject({ ratio: 900 });
  expect(resizeActivePane(pair, 50).root).toMatchObject({ ratio: 450 });
  expect(resizeActivePane({ ...pair, activePane: "pane-1" }, 50).root).toMatchObject({ ratio: 550 });
  expect(resizeActivePane(tab(1, "a"), 50)).toEqual(tab(1, "a"));

  expect(sanitizeName("  Review  ")).toBe("Review");
  expect(sanitizeName("   ")).toBeNull();
  expect(sanitizeName("tab\there")).toBeNull();
  expect(sanitizeName("é".repeat(80))).toHaveLength(80);
  expect(sanitizeName("x".repeat(81))).toBeNull();
  expect(nextWorkspaceName([tab(1, "a")])).toBe("Workspace");
  expect(nextWorkspaceName([pair, { ...pair, id: "tab-5", name: "Workspace 2" }])).toBe("Workspace 3");
  expect(nextPaneId([pair])).toBe("pane-3");
  expect(nextTabId([pair, tab(7, "x")])).toBe("tab-8");
});

test("sessions store stable references and restore independently of missing documents", () => {
  const documents: DocumentSummary[] = [
    { documentId: "document-1", name: "a.txt", byteLen: "0", revision: "0", encoding: "utf8", sourcePath: "/p/a.txt" },
    { documentId: "document-2", name: "Untitled 2", byteLen: "3", revision: "1", encoding: "utf8", recoveryId: "document-abc" },
    { documentId: "document-3", name: "Untitled 3", byteLen: "3", revision: "0", encoding: "utf8" },
  ];
  const workspace = { ...must(groupTabs(tab(1, "document-1"), "pane-1", "right", tab(2, "document-2"), "Workspace")), name: "monitoring" };
  const stored = tabsToSession([workspace, tab(3, "document-3"), tab(4, `${PENDING_PREFIX}document-xyz`)], documents);
  // A document with neither a file nor a recovery copy cannot be restored and never reaches the session.
  expect(stored.documents).toEqual(["path:/p/a.txt", "recovery:document-abc", "recovery:document-xyz"]);
  expect(stored.layouts.map((item) => item.id)).toEqual(["tab-1", "tab-4"]);
  expect(stored.layouts[0]).toMatchObject({
    name: "monitoring", root: { first: { tabs: ["path:/p/a.txt"] }, second: { tabs: ["recovery:document-abc"] } },
  });
  expect(JSON.stringify(stored)).not.toContain("document-1");

  // Only the unavailable pane is removed; the workspace collapses to a document tab.
  const restored = tabsFromSession(stored, (reference) => reference === "path:/p/a.txt" ? "fresh-1" : null);
  expect(restored.map((item) => [item.id, shape(item.root), isWorkspace(item)])).toEqual([["tab-1", "fresh-1", false]]);
  expect(tabsFromSession({ layouts: [] }, () => null)).toEqual([]);
});

test("an older session with several tabs in one pane restores as one tab per document", () => {
  const restored = tabsFromSession({ layouts: [{
    id: "layout-1", name: "Layout 1", activePane: "pane-1",
    root: { kind: "split", direction: "horizontal", ratio: 500,
      first: { kind: "pane", id: "pane-1", tabs: ["a", "b", "c"], activeTab: "b" },
      second: { kind: "pane", id: "pane-2", tabs: ["a"], activeTab: "a" } },
  }, {
    id: "layout-2", name: "Layout 2", activePane: "pane-1",
    root: { kind: "pane", id: "pane-1", tabs: ["d"], activeTab: "d" },
  }] }, (reference) => reference);
  expect(restored.map((item) => shape(item.root))).toEqual([{ horizontal: ["b", "a"] }, "d", "c"]);
  // Pane identities are made unique across tabs so tabs can be grouped later.
  const ids = restored.flatMap((item) => listPanes(item.root).map((pane) => pane.id));
  expect(new Set(ids).size).toBe(ids.length);
});
