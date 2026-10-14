import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import type { DocumentSummary, LayoutNode, PaneNode } from "./contracts";
import { Icon, IconButton } from "./icons";
import {
  type DropZone, type WorkTab, clampRatio, isWorkspace, listPanes, paneDocument, sanitizeName, tabDocuments,
} from "./layout";

const DRAG_THRESHOLD = 6;
/** Smallest distance, in pixels, one press of a tab scroll arrow moves the strip. */
const TAB_SCROLL_MIN = 120;
const RATIO_KEY_STEP = 20;

function NameInput({ value, label, onCommit, onCancel, onInvalid }: {
  value: string;
  label: string;
  onCommit(name: string): void;
  onCancel(): void;
  onInvalid?(): void;
}) {
  const [draft, setDraft] = useState(value);
  const settled = useRef(false);
  const commit = () => {
    if (settled.current) return;
    settled.current = true;
    const name = sanitizeName(draft);
    if (!name) { onInvalid?.(); onCancel(); } else if (name === value) onCancel(); else onCommit(name);
  };
  return <input className="editable-name-input" autoFocus aria-label={label}
    value={draft} maxLength={200} onFocus={(event) => event.currentTarget.select()}
    onChange={(event) => setDraft(event.currentTarget.value)} onBlur={commit}
    onPointerDown={(event) => event.stopPropagation()}
    onKeyDown={(event) => {
      event.stopPropagation();
      if (event.key === "Enter") { event.preventDefault(); commit(); }
      if (event.key === "Escape") { event.preventDefault(); settled.current = true; onCancel(); }
    }} />;
}

type Drag =
  | { kind: "tab"; tabId: string; x: number; y: number; started: boolean }
  | { kind: "pane"; paneId: string; x: number; y: number; started: boolean };
type Preview = { kind: "pane"; paneId: string; zone: DropZone } | { kind: "strip"; index: number };

interface WorkTabsProps {
  tabs: WorkTab[];
  activeTabId: string | null;
  documents: DocumentSummary[];
  /** Renders the document of one pane. */
  renderView(document: DocumentSummary, pane: PaneNode, active: boolean, ordinal: number, total: number): ReactNode;
  /** Shown when no tab is open. */
  emptyState: ReactNode;
  /** Replaces the panes of the active tab (comparison views). */
  content?: ReactNode;
  /** Tab whose name should enter edit mode; bump `signal` to trigger it again. */
  rename: { tabId: string | null; signal: number };
  /** Toolbar controls shown at the end of the tab strip. */
  children?: ReactNode;
  onSelectTab(tabId: string): void;
  onCloseTab(tabId: string): void;
  onNewTab(): void;
  onReorderTab(tabId: string, index: number): void;
  onRenameTab(tabId: string, name: string): void;
  onInvalidName(): void;
  /** A tab was dropped into the active tab's content beside a pane. */
  onGroupTab(tabId: string, targetPaneId: string, zone: DropZone): void;
  onMovePane(paneId: string, targetPaneId: string, zone: DropZone): void;
  /** A pane was dragged to the tab strip, or its pop-out control was used. */
  onPopOutPane(paneId: string, index?: number): void;
  onClosePane(paneId: string): void;
  onFocusPane(paneId: string): void;
  onRatio(path: string, ratio: number): void;
  /** Right-click on a tab or a pane header, at window coordinates. */
  onTabMenu(tabId: string, x: number, y: number): void;
  onPaneMenu(paneId: string, x: number, y: number): void;
}

const ZONE_HINT: Record<DropZone, string> = {
  left: "Release to place it on the left",
  right: "Release to place it on the right",
  top: "Release to place it above",
  bottom: "Release to place it below",
};

/**
 * The single tab strip and the content of its active tab. Dragging a tab into
 * the content groups it with the active tab as a workspace of split panes.
 */
export function WorkTabs({
  tabs, activeTabId, documents, renderView, emptyState, content, rename, children,
  onSelectTab, onCloseTab, onNewTab, onReorderTab, onRenameTab, onInvalidName,
  onGroupTab, onMovePane, onPopOutPane, onClosePane, onFocusPane, onRatio, onTabMenu, onPaneMenu,
}: WorkTabsProps) {
  const [preview, setPreview] = useState<Preview | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [overflow, setOverflow] = useState({ start: false, end: false });
  const scrollRef = useRef<HTMLDivElement>(null);
  const previewRef = useRef<Preview | null>(null);
  const dragRef = useRef<Drag | null>(null);
  const resizeRef = useRef<{ path: string; horizontal: boolean; bounds: DOMRect } | null>(null);
  const lastRename = useRef(rename.signal);
  const handlers = useRef({ onGroupTab, onMovePane, onPopOutPane, onReorderTab, onRatio });
  handlers.current = { onGroupTab, onMovePane, onPopOutPane, onReorderTab, onRatio };
  const activeTab = tabs.find((tab) => tab.id === activeTabId);
  const activeTabRef = useRef(activeTab);
  activeTabRef.current = activeTab;
  const byId = new Map(documents.map((document) => [document.documentId, document]));

  // Arrow buttons appear once the tabs no longer fit the strip.
  const measureStrip = useCallback(() => {
    const strip = scrollRef.current;
    if (!strip) return;
    const start = strip.scrollLeft > 1;
    const end = strip.scrollLeft + strip.clientWidth < strip.scrollWidth - 1;
    setOverflow((current) => current.start === start && current.end === end ? current : { start, end });
  }, []);
  useEffect(measureStrip);
  useEffect(() => {
    const strip = scrollRef.current;
    if (!strip || typeof ResizeObserver === "undefined") return undefined;
    const observer = new ResizeObserver(measureStrip);
    observer.observe(strip);
    return () => observer.disconnect();
  }, [measureStrip]);
  useEffect(() => {
    const strip = scrollRef.current;
    const tab = [...strip?.querySelectorAll<HTMLElement>("[data-tab-id]") ?? []]
      .find((item) => item.dataset.tabId === activeTabId);
    if (!strip || !tab) return;
    const bounds = tab.getBoundingClientRect();
    const view = strip.getBoundingClientRect();
    if (bounds.left < view.left) strip.scrollLeft -= view.left - bounds.left;
    else if (bounds.right > view.right) strip.scrollLeft += bounds.right - view.right;
    measureStrip();
  }, [activeTabId, measureStrip, tabs.length]);
  const scrollTabs = (direction: -1 | 1) => {
    const strip = scrollRef.current;
    if (!strip) return;
    const left = strip.scrollLeft + direction * Math.max(TAB_SCROLL_MIN, strip.clientWidth * 0.6);
    if (typeof strip.scrollTo === "function") strip.scrollTo({ left, behavior: "smooth" });
    else strip.scrollLeft = left;
  };

  useEffect(() => {
    if (rename.signal !== lastRename.current) {
      lastRename.current = rename.signal;
      setRenaming(rename.tabId);
    }
  }, [rename]);

  useEffect(() => {
    const show = (next: Preview | null) => { previewRef.current = next; setPreview(next); };
    const cancel = () => { dragRef.current = null; resizeRef.current = null; show(null); };
    const onMove = (event: PointerEvent) => {
      const resize = resizeRef.current;
      if (resize) {
        const { bounds } = resize;
        const share = resize.horizontal
          ? (event.clientX - bounds.left) / Math.max(1, bounds.width)
          : (event.clientY - bounds.top) / Math.max(1, bounds.height);
        handlers.current.onRatio(resize.path, clampRatio(share * 1000));
        return;
      }
      const drag = dragRef.current;
      if (!drag) return;
      if (!drag.started && Math.hypot(event.clientX - drag.x, event.clientY - drag.y) < DRAG_THRESHOLD) return;
      drag.started = true;
      const target = event.target instanceof Element ? event.target : null;
      const strip = target?.closest<HTMLElement>("[data-tabstrip]");
      if (strip) {
        const others = [...strip.querySelectorAll<HTMLElement>("[data-tab-id]")]
          .filter((item) => !(drag.kind === "tab" && item.dataset.tabId === drag.tabId));
        const index = others.filter((item) => {
          const bounds = item.getBoundingClientRect();
          return event.clientX > bounds.left + bounds.width / 2;
        }).length;
        show({ kind: "strip", index });
        return;
      }
      const paneElement = target?.closest<HTMLElement>("[data-pane-id]");
      const paneId = paneElement?.dataset.paneId;
      // A tab cannot be grouped with itself, and a pane cannot be dropped onto itself.
      if (!paneElement || !paneId
        || (drag.kind === "tab" && drag.tabId === activeTabRef.current?.id)
        || (drag.kind === "pane" && drag.paneId === paneId)) { show(null); return; }
      const bounds = paneElement.getBoundingClientRect();
      const x = (event.clientX - bounds.left) / Math.max(1, bounds.width);
      const y = (event.clientY - bounds.top) / Math.max(1, bounds.height);
      const distances: Array<[DropZone, number]> = [["left", x], ["right", 1 - x], ["top", y], ["bottom", 1 - y]];
      const [zone] = distances.reduce((nearest, item) => item[1] < nearest[1] ? item : nearest);
      show({ kind: "pane", paneId, zone });
    };
    const onUp = () => {
      resizeRef.current = null;
      const drag = dragRef.current;
      const target = previewRef.current;
      dragRef.current = null;
      show(null);
      if (!drag?.started || !target) return;
      if (drag.kind === "tab") {
        if (target.kind === "strip") handlers.current.onReorderTab(drag.tabId, target.index);
        else handlers.current.onGroupTab(drag.tabId, target.paneId, target.zone);
      } else if (target.kind === "strip") handlers.current.onPopOutPane(drag.paneId, target.index);
      else handlers.current.onMovePane(drag.paneId, target.paneId, target.zone);
    };
    const onKey = (event: globalThis.KeyboardEvent) => { if (event.key === "Escape" && dragRef.current) cancel(); };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const tabLabel = (tab: WorkTab) => isWorkspace(tab) ? tab.name
    : byId.get(tabDocuments(tab)[0] ?? "")?.name ?? tab.name;
  const panes = activeTab ? listPanes(activeTab.root) : [];

  const renderPane = (pane: PaneNode) => {
    const ordinal = panes.findIndex((item) => item.id === pane.id) + 1;
    const active = pane.id === activeTab?.activePane;
    const document = byId.get(paneDocument(pane) ?? "");
    const drop = preview?.kind === "pane" && preview.paneId === pane.id ? preview.zone : null;
    const grouped = panes.length > 1;
    return <section className={`layout-pane${active ? " active" : ""}${grouped ? " grouped" : ""}`} role="group"
      data-pane-id={pane.id} key={pane.id}
      aria-label={grouped ? `Pane ${ordinal} of ${panes.length}: ${document?.name ?? "empty"}` : "Editor"}
      onPointerDownCapture={() => { if (!active) onFocusPane(pane.id); }}
      onFocusCapture={() => { if (!active) onFocusPane(pane.id); }}>
      {grouped ? <header className="pane-header" title="Drag to rearrange, or to the tab strip to ungroup"
        onContextMenu={(event) => { event.preventDefault(); onPaneMenu(pane.id, event.clientX, event.clientY); }}
        onPointerDown={(event) => {
          if (event.button !== 0 || (event.target instanceof Element && event.target.closest("button"))) return;
          dragRef.current = { kind: "pane", paneId: pane.id, x: event.clientX, y: event.clientY, started: false };
        }}>
        <span className="pane-title">{document?.readOnly
          ? <span className="tab-lock" role="img" aria-label="read-only" title="Read-only"><Icon name="lock" size={12} /></span> : null}
          {document?.name ?? "Unavailable"}
          {document?.dirty ? <span className="tab-dirty" aria-label="unsaved changes">●</span> : null}</span>
        <IconButton icon="popOut" label={`Move ${document?.name ?? "pane"} to its own tab`} onClick={() => onPopOutPane(pane.id)} />
        <IconButton icon="close" label={`Close pane ${document?.name ?? ordinal}`} onClick={() => onClosePane(pane.id)} />
      </header> : null}
      <div className="editor-pane">{document ? renderView(document, pane, active, ordinal, panes.length) : null}</div>
      {drop ? <div className={`drop-overlay ${drop}`} data-drop-zone={drop} aria-hidden="true" /> : null}
    </section>;
  };

  const renderNode = (node: LayoutNode, path: string): ReactNode => {
    if (node.kind === "pane") return renderPane(node);
    const horizontal = node.direction === "horizontal";
    const child = (step: string) => path ? `${path}/${step}` : step;
    return <div className={`layout-split ${node.direction}`} key={`split-${path}`}>
      <div className="layout-child" style={{ flex: `${node.ratio} 1 0` }}>{renderNode(node.first, child("0"))}</div>
      <div className={`layout-divider ${node.direction}`} role="separator" tabIndex={0}
        aria-label={horizontal ? "Resize panes horizontally" : "Resize panes vertically"}
        aria-orientation={horizontal ? "vertical" : "horizontal"}
        aria-valuemin={10} aria-valuemax={90} aria-valuenow={Math.round(node.ratio / 10)}
        onPointerDown={(event) => {
          const bounds = event.currentTarget.parentElement?.getBoundingClientRect();
          if (!bounds || event.button !== 0) return;
          event.preventDefault();
          resizeRef.current = { path, horizontal, bounds };
        }}
        onKeyDown={(event) => {
          const smaller = horizontal ? "ArrowLeft" : "ArrowUp";
          const larger = horizontal ? "ArrowRight" : "ArrowDown";
          const ratio = event.key === smaller ? node.ratio - RATIO_KEY_STEP
            : event.key === larger ? node.ratio + RATIO_KEY_STEP
              : event.key === "Home" ? 0 : event.key === "End" ? 1000 : null;
          if (ratio === null) return;
          event.preventDefault();
          onRatio(path, clampRatio(ratio));
        }} />
      <div className="layout-child" style={{ flex: `${1000 - node.ratio} 1 0` }}>{renderNode(node.second, child("1"))}</div>
    </div>;
  };

  const onTabKey = (event: KeyboardEvent<HTMLButtonElement>, index: number, tab: WorkTab) => {
    if (event.key === "F2" && isWorkspace(tab)) { event.preventDefault(); setRenaming(tab.id); return; }
    const offset = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    const next = tabs[index + offset];
    if (!offset || !next) return;
    event.preventDefault();
    onSelectTab(next.id);
  };
  const marker = (index: number) => preview?.kind === "strip" && preview.index === index
    ? <i className="tab-drop-marker" aria-hidden="true" /> : null;
  const visibleIndex = (tab: WorkTab) => {
    const drag = dragRef.current;
    const others = tabs.filter((item) => !(drag?.kind === "tab" && item.id === drag.tabId));
    return others.indexOf(tab);
  };

  const scrollable = overflow.start || overflow.end;

  return <>
    <div className="tabstrip">
      <div className="tabs-area" data-tabstrip>
      {scrollable ? <IconButton icon="scrollLeft" label="Scroll tabs left" className="tab-scroll"
        disabled={!overflow.start} onClick={() => scrollTabs(-1)} /> : null}
      <div className="tabs-scroll" role="tablist" aria-label="Open documents" ref={scrollRef} onScroll={measureStrip}>
        {tabs.map((tab, index) => {
          const selected = tab.id === activeTabId;
          const workspace = isWorkspace(tab);
          const label = tabLabel(tab);
          const dirty = tabDocuments(tab).some((id) => byId.get(id)?.dirty);
          const readOnly = !workspace && tabDocuments(tab).every((id) => byId.get(id)?.readOnly);
          return <div className={`tab${selected ? " active" : ""}${workspace ? " workspace" : ""}`} data-tab-id={tab.id} key={tab.id}
            onContextMenu={(event) => { event.preventDefault(); onTabMenu(tab.id, event.clientX, event.clientY); }}
            onPointerDown={(event) => {
              if (event.button !== 0 || (event.target instanceof Element && event.target.closest(".tab-close, input"))) return;
              dragRef.current = { kind: "tab", tabId: tab.id, x: event.clientX, y: event.clientY, started: false };
            }}>
            {marker(visibleIndex(tab))}
            {renaming === tab.id && workspace
              ? <><Icon name="workspace" size={14} /><NameInput value={tab.name} label="Workspace name" onInvalid={onInvalidName}
                  onCancel={() => setRenaming(null)}
                  onCommit={(name) => { setRenaming(null); onRenameTab(tab.id, name); }} /></>
              : <button className="tab-select" type="button" role="tab" aria-selected={selected} tabIndex={selected ? 0 : -1}
                  title={workspace ? `${label} — double-click to rename` : label}
                  onKeyDown={(event) => onTabKey(event, index, tab)}
                  onClick={() => onSelectTab(tab.id)}
                  onDoubleClick={() => { if (workspace) setRenaming(tab.id); }}
                  >
                  {workspace ? <Icon name="workspace" size={14} /> : null}
                  {dirty ? <span className="tab-dirty" aria-label="unsaved changes">●</span> : null}
                  {readOnly ? <span className="tab-lock" role="img" aria-label="read-only" title="Read-only"><Icon name="lock" size={12} /></span> : null}
                  <span className="tab-label">{label}</span>
                </button>}
            <IconButton icon="close" label={`Close ${label}`} className="tab-close" onClick={() => onCloseTab(tab.id)} />
          </div>;
        })}
        {preview?.kind === "strip" && preview.index >= tabs.length - (dragRef.current?.kind === "tab" ? 1 : 0)
          ? <i className="tab-drop-marker" aria-hidden="true" /> : null}
      </div>
      {scrollable ? <IconButton icon="scrollRight" label="Scroll tabs right" className="tab-scroll"
        disabled={!overflow.end} onClick={() => scrollTabs(1)} /> : null}
      <IconButton icon="plus" label="New tab" className="tab-new" onClick={onNewTab} />
      </div>
      <div className="layout-actions">{children}</div>
    </div>
    {content ?? <div className={`editor-layout${preview ? " dragging-tab" : ""}`}>
      {activeTab ? renderNode(activeTab.root, "") : <div className="editor-pane">{emptyState}</div>}
    </div>}
    <div className="screen-reader-only" aria-live="polite">
      {preview?.kind === "pane" ? ZONE_HINT[preview.zone] : preview?.kind === "strip" ? "Release to place the tab here" : ""}
    </div>
  </>;
}
