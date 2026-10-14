/**
 * Pure model for the single tab strip and its workspaces.
 *
 * Every top-level tab holds a tree of panes, and every pane shows exactly one
 * document. A tab with one pane is an ordinary document tab; dropping another
 * tab into its content turns it into a named workspace of split panes.
 *
 * Trees hold transient document IDs while the application runs. Sessions store
 * stable references instead (`path:` or `recovery:`), converted at the edges.
 */
import type {
  DocumentSummary,
  LayoutGroup,
  LayoutNode,
  PaneNode,
  SplitDirection,
  WorkbenchSession,
} from "./contracts";

export const LAYOUT_LIMITS = { tabs: 128, panes: 16, depth: 6, nameChars: 80 } as const;
export const DEFAULT_WORKSPACE_NAME = "Workspace";
export const MIN_RATIO = 100;
export const MAX_RATIO = 900;
/** Placeholder ID for an untitled tab whose recovery checkpoint is not restored yet. */
export const PENDING_PREFIX = "pending:";

/** A top-level tab: one document, or a workspace of split panes. */
export type WorkTab = LayoutGroup;
export type DropZone = "left" | "right" | "top" | "bottom";
export type LayoutResult = { tab: WorkTab; error?: undefined } | { tab?: undefined; error: string };

export function listPanes(node: LayoutNode): PaneNode[] {
  return node.kind === "pane" ? [node] : [...listPanes(node.first), ...listPanes(node.second)];
}

export function treeDepth(node: LayoutNode): number {
  return node.kind === "pane" ? 1 : 1 + Math.max(treeDepth(node.first), treeDepth(node.second));
}

export function paneDocument(pane: PaneNode): string | null {
  return pane.activeTab ?? pane.tabs[0] ?? null;
}

export function activePane(tab: WorkTab): PaneNode {
  const panes = listPanes(tab.root);
  return panes.find((pane) => pane.id === tab.activePane) ?? panes[0];
}

/** The document shown in the focused pane of a tab. */
export function activeDocumentId(tab: WorkTab): string | null {
  return paneDocument(activePane(tab));
}

export function tabDocuments(tab: WorkTab): string[] {
  return [...new Set(listPanes(tab.root).flatMap((pane) => paneDocument(pane) ?? []))];
}

/** A workspace is a tab that groups two or more panes. */
export function isWorkspace(tab: WorkTab): boolean {
  return tab.root.kind === "split";
}

export function viewCount(tabs: readonly WorkTab[], documentId: string): number {
  return tabs.reduce((total, tab) => total
    + listPanes(tab.root).filter((pane) => paneDocument(pane) === documentId).length, 0);
}

/** Trims and validates a workspace name; null when it is unusable. */
export function sanitizeName(value: string): string | null {
  const name = value.trim();
  const hasControl = Array.from(name).some((character) => {
    const code = character.codePointAt(0) ?? 0;
    return code < 32 || (code >= 127 && code < 160);
  });
  if (!name || hasControl || Array.from(name).length > LAYOUT_LIMITS.nameChars) return null;
  return name;
}

function mapPanes(node: LayoutNode, change: (pane: PaneNode) => PaneNode): LayoutNode {
  if (node.kind === "pane") return change(node);
  return { ...node, first: mapPanes(node.first, change), second: mapPanes(node.second, change) };
}

/** Removes the panes matching `drop`, collapsing each branch left with one child. */
function withoutPanes(node: LayoutNode, drop: (pane: PaneNode) => boolean): LayoutNode | null {
  if (node.kind === "pane") return drop(node) ? null : node;
  const first = withoutPanes(node.first, drop);
  const second = withoutPanes(node.second, drop);
  if (!first) return second;
  if (!second) return first;
  return { ...node, first, second };
}

function withRoot(tab: WorkTab, root: LayoutNode | null): WorkTab | null {
  if (!root) return null;
  const panes = listPanes(root);
  return { ...tab, root, activePane: panes.some((pane) => pane.id === tab.activePane) ? tab.activePane : panes[0].id };
}

function nextId(prefix: string, taken: readonly string[]): string {
  const highest = taken.reduce((maximum, id) => {
    const match = new RegExp(`^${prefix}-(\\d+)$`).exec(id);
    return match ? Math.max(maximum, Number(match[1])) : maximum;
  }, 0);
  return `${prefix}-${highest + 1}`;
}

export function nextPaneId(tabs: readonly WorkTab[], reserved: readonly string[] = []): string {
  return nextId("pane", [...reserved, ...tabs.flatMap((tab) => listPanes(tab.root).map((pane) => pane.id))]);
}

export function nextTabId(tabs: readonly WorkTab[], reserved: readonly string[] = []): string {
  return nextId("tab", [...reserved, ...tabs.map((tab) => tab.id)]);
}

/** "Workspace", then "Workspace 2", … so new workspaces start distinguishable. */
export function nextWorkspaceName(tabs: readonly WorkTab[]): string {
  const names = new Set(tabs.filter(isWorkspace).map((tab) => tab.name));
  if (!names.has(DEFAULT_WORKSPACE_NAME)) return DEFAULT_WORKSPACE_NAME;
  let ordinal = 2;
  while (names.has(`${DEFAULT_WORKSPACE_NAME} ${ordinal}`)) ordinal += 1;
  return `${DEFAULT_WORKSPACE_NAME} ${ordinal}`;
}

export function documentTab(id: string, paneId: string, documentId: string): WorkTab {
  return {
    id, name: DEFAULT_WORKSPACE_NAME, activePane: paneId,
    root: { kind: "pane", id: paneId, tabs: [documentId], activeTab: documentId },
  };
}

function splitAt(node: LayoutNode, paneId: string, zone: DropZone, inserted: LayoutNode): LayoutNode {
  if (node.kind === "split") {
    return { ...node, first: splitAt(node.first, paneId, zone, inserted), second: splitAt(node.second, paneId, zone, inserted) };
  }
  if (node.id !== paneId) return node;
  const direction: SplitDirection = zone === "left" || zone === "right" ? "horizontal" : "vertical";
  const insertedFirst = zone === "left" || zone === "top";
  return { kind: "split", direction, ratio: 500, first: insertedFirst ? inserted : node, second: insertedFirst ? node : inserted };
}

function checkLimits(tab: WorkTab): LayoutResult {
  if (listPanes(tab.root).length > LAYOUT_LIMITS.panes) {
    return { error: `A workspace supports at most ${LAYOUT_LIMITS.panes} panes` };
  }
  if (treeDepth(tab.root) > LAYOUT_LIMITS.depth) {
    return { error: `Panes can be nested at most ${LAYOUT_LIMITS.depth} levels deep` };
  }
  return { tab };
}

/**
 * Drops `source` into `target` beside one of its panes. The two tabs become
 * one workspace; `name` is used only when the target was a plain document tab.
 */
export function groupTabs(target: WorkTab, paneId: string, zone: DropZone, source: WorkTab, name: string): LayoutResult {
  if (!listPanes(target.root).some((pane) => pane.id === paneId)) return { error: "That pane is no longer open" };
  return checkLimits({
    ...target,
    name: isWorkspace(target) ? target.name : name,
    root: splitAt(target.root, paneId, zone, source.root),
    activePane: source.activePane,
  });
}

/** Opens another view of a document in a new pane beside `paneId`. */
export function splitPane(tab: WorkTab, paneId: string, zone: DropZone, documentId: string, newPaneId: string, name: string): LayoutResult {
  return groupTabs(tab, paneId, zone, documentTab("unused", newPaneId, documentId), name);
}

/** Removes one pane; null when it was the tab's last. */
export function closePane(tab: WorkTab, paneId: string): WorkTab | null {
  return withRoot(tab, withoutPanes(tab.root, (pane) => pane.id === paneId));
}

/** Closes every view of a document in this tab; null when nothing is left. */
export function removeDocument(tab: WorkTab, documentId: string): WorkTab | null {
  return withRoot(tab, withoutPanes(tab.root, (pane) => paneDocument(pane) === documentId));
}

export function replaceDocument(tab: WorkTab, from: string, to: string): WorkTab {
  const root = mapPanes(tab.root, (pane) => paneDocument(pane) === from ? { ...pane, tabs: [to], activeTab: to } : pane);
  return { ...tab, root };
}

/** Lifts one pane out of a workspace into its own document tab. */
export function extractPane(tab: WorkTab, paneId: string, newTabId: string): { remaining: WorkTab | null; extracted: WorkTab } | null {
  const pane = listPanes(tab.root).find((item) => item.id === paneId);
  if (!pane) return null;
  return {
    remaining: closePane(tab, paneId),
    extracted: { id: newTabId, name: DEFAULT_WORKSPACE_NAME, root: pane, activePane: pane.id },
  };
}

/** Moves a pane beside another pane of the same workspace. */
export function movePane(tab: WorkTab, paneId: string, targetPaneId: string, zone: DropZone): LayoutResult {
  if (paneId === targetPaneId) return { tab };
  const lifted = extractPane(tab, paneId, "unused");
  if (!lifted?.remaining) return { tab };
  return groupTabs(lifted.remaining, targetPaneId, zone, lifted.extracted, tab.name);
}

/** Dissolves a workspace into one document tab per pane. */
export function ungroupTab(tab: WorkTab, tabs: readonly WorkTab[]): WorkTab[] {
  const ids: string[] = [];
  return listPanes(tab.root).map((pane, index) => {
    const id = index === 0 ? tab.id : nextTabId(tabs, ids);
    ids.push(id);
    return { id, name: DEFAULT_WORKSPACE_NAME, root: pane, activePane: pane.id };
  });
}

export function clampRatio(ratio: number): number {
  return Math.round(Math.max(MIN_RATIO, Math.min(MAX_RATIO, ratio)));
}

/** Sets the ratio of the branch addressed by a path of `0`/`1` child steps. */
export function setRatio(tab: WorkTab, path: string, ratio: number): WorkTab {
  const walk = (node: LayoutNode, steps: string[]): LayoutNode => {
    if (node.kind === "pane") return node;
    if (steps.length === 0) return { ...node, ratio: clampRatio(ratio) };
    const [step, ...rest] = steps;
    return step === "0" ? { ...node, first: walk(node.first, rest) } : { ...node, second: walk(node.second, rest) };
  };
  return { ...tab, root: walk(tab.root, path ? path.split("/") : []) };
}

function panePath(node: LayoutNode, paneId: string, path: string[] = []): string[] | null {
  if (node.kind === "pane") return node.id === paneId ? path : null;
  return panePath(node.first, paneId, [...path, "0"]) ?? panePath(node.second, paneId, [...path, "1"]);
}

/** Grows or shrinks the innermost branch holding the focused pane. */
export function resizeActivePane(tab: WorkTab, delta: number): WorkTab {
  const steps = panePath(tab.root, tab.activePane);
  if (!steps || steps.length === 0) return tab;
  const last = steps[steps.length - 1];
  const parent = steps.slice(0, -1);
  let node = tab.root;
  for (const step of parent) if (node.kind === "split") node = step === "0" ? node.first : node.second;
  if (node.kind !== "split") return tab;
  return setRatio(tab, parent.join("/"), node.ratio + (last === "0" ? delta : -delta));
}

/** Stable session identity for a document, or null when it cannot be restored. */
export function documentReference(document: DocumentSummary): string | null {
  if (document.sourcePath) return `path:${document.sourcePath}`;
  if (!document.remoteUri && document.recoveryId) return `recovery:${document.recoveryId}`;
  return null;
}

function referenceFor(tab: string, documents: readonly DocumentSummary[]): string | null {
  if (tab.startsWith(PENDING_PREFIX)) return `recovery:${tab.slice(PENDING_PREFIX.length)}`;
  const document = documents.find((item) => item.documentId === tab);
  return document ? documentReference(document) : null;
}

/** Converts live tabs to session layouts, dropping panes that cannot be restored. */
export function tabsToSession(
  tabs: readonly WorkTab[], documents: readonly DocumentSummary[],
): Pick<WorkbenchSession, "documents" | "layouts"> {
  const references = new Set<string>();
  const layouts = tabs.flatMap((tab) => {
    const mapped = mapPanes(tab.root, (pane) => {
      const reference = referenceFor(paneDocument(pane) ?? "", documents);
      return { ...pane, tabs: reference ? [reference] : [], activeTab: reference };
    });
    const stored = withRoot(tab, withoutPanes(mapped, (pane) => pane.tabs.length === 0));
    if (!stored) return [];
    listPanes(stored.root).forEach((pane) => references.add(pane.tabs[0]));
    return [{ ...stored, name: sanitizeName(stored.name) ?? DEFAULT_WORKSPACE_NAME }];
  });
  return { documents: [...references], layouts };
}

/**
 * Rebuilds live tabs from a session. `resolve` maps each reference to a
 * document ID (or a pending placeholder), or null when it is unavailable; only
 * those panes are removed. A pane that stored several tabs (an older session)
 * keeps its active one, and the others return as tabs of their own.
 */
export function tabsFromSession(
  session: Pick<WorkbenchSession, "layouts">, resolve: (reference: string) => string | null,
): WorkTab[] {
  const tabs: WorkTab[] = [];
  const loose: string[] = [];
  const paneIds: string[] = [];
  for (const group of session.layouts.slice(0, LAYOUT_LIMITS.tabs)) {
    const mapped = mapPanes(group.root, (pane) => {
      const resolved = pane.tabs.flatMap((reference) => resolve(reference) ?? []);
      const active = (pane.activeTab ? resolve(pane.activeTab) : null) ?? resolved[0] ?? null;
      loose.push(...resolved.filter((id) => id !== active));
      // Pane identities must be unique across tabs so tabs can be grouped later.
      const id = paneIds.includes(pane.id) ? nextId("pane", paneIds) : pane.id;
      paneIds.push(id);
      return { ...pane, id, tabs: active ? [active] : [], activeTab: active };
    });
    const tab = withRoot({ ...group, name: sanitizeName(group.name) ?? DEFAULT_WORKSPACE_NAME },
      withoutPanes(mapped, (pane) => pane.tabs.length === 0));
    if (tab && !tabs.some((item) => item.id === tab.id)) tabs.push(tab);
  }
  for (const documentId of new Set(loose)) {
    if (viewCount(tabs, documentId) > 0) continue;
    tabs.push(documentTab(nextTabId(tabs), nextPaneId(tabs), documentId));
  }
  return tabs;
}
