import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { DocumentSummary, Place, PlaceDirectory, PlaceEntry } from "./contracts";
import { Icon, type IconName } from "./icons";
import { executeCommand } from "./ipc";

export const EXPLORER_TABS = [
  { id: "open", title: "Open" },
  { id: "places", title: "Places" },
  { id: "folder", title: "Folder" },
] as const;
export type ExplorerTab = typeof EXPLORER_TABS[number]["id"];

const TAB_KEY = "nulis.explorerTab.v1";
const ENTRY_LIMIT = 500;
const MAX_LOADED_DIRECTORIES = 256;
const CONTEXT = { windowId: "main" } as const;

export function loadExplorerTab(): ExplorerTab {
  try {
    const stored = window.localStorage.getItem(TAB_KEY);
    return EXPLORER_TABS.find((tab) => tab.id === stored)?.id ?? "places";
  } catch {
    return "places";
  }
}

export function saveExplorerTab(tab: ExplorerTab): void {
  try { window.localStorage.setItem(TAB_KEY, tab); } catch { /* The choice still holds for this session. */ }
}

/** Every directory from the volume root down to `path`, in order. */
export function pathAncestors(path: string): string[] {
  const separator = path.includes("\\") ? "\\" : "/";
  const parts = path.split(separator).filter(Boolean);
  const drive = /^[A-Za-z]:$/.test(parts[0] ?? "");
  const root = drive ? `${parts[0]}\\` : "/";
  const chain = [root];
  let current = drive ? parts[0] : "";
  for (const part of parts.slice(drive ? 1 : 0)) {
    current = `${current}${separator}${part}`;
    chain.push(current);
  }
  return chain;
}

/**
 * Turns a filter such as `*.md; *.txt` into a file-name test. An empty filter
 * or `*` matches everything; folders are never filtered out.
 */
export function fileFilter(pattern: string): (name: string) => boolean {
  const globs = pattern.split(/[;,\s]+/).filter((item) => item && item !== "*" && item !== "*.*");
  if (globs.length === 0) return () => true;
  const tests = globs.map((glob) => new RegExp(`^${glob.replace(/[.+^${}()|[\]\\]/g, "\\$&")
    .replace(/\*/g, ".*").replace(/\?/g, ".")}$`, "i"));
  return (name) => tests.some((test) => test.test(name));
}

interface FileExplorerProps {
  tab: ExplorerTab;
  onTab(tab: ExplorerTab): void;
  documents: DocumentSummary[];
  activeDocumentId: string | null;
  onShowDocument(documentId: string): void;
  /** Opens a file by absolute path, or focuses it when it is already open. */
  onOpenPath(path: string): void;
  onStatus(message: string): void;
  /** Right-click on an open file, at window coordinates. */
  onDocumentMenu?(documentId: string, x: number, y: number): void;
  /** The opened-folder tree. */
  folder: ReactNode;
  /** A file to show in Places; a new `id` shows it again. */
  reveal?: { path: string; id: number };
}

/** File View: open documents, the whole machine (Places), and the opened folder. */
export function FileExplorer({
  tab, onTab, documents, activeDocumentId, onShowDocument, onOpenPath, onStatus, onDocumentMenu, folder, reveal,
}: FileExplorerProps) {
  const [revealed, setRevealed] = useState<string | null>(null);
  const treeRef = useRef<HTMLDivElement>(null);
  const [places, setPlaces] = useState<Place[] | null>(null);
  const [directories, setDirectories] = useState<Record<string, PlaceDirectory>>({});
  const [expanded, setExpanded] = useState<string[]>([]);
  const [filter, setFilter] = useState("");
  const [showHidden, setShowHidden] = useState(false);
  const [home, setHome] = useState<string | null>(null);
  const directoriesRef = useRef(directories);
  directoriesRef.current = directories;
  const matches = useMemo(() => fileFilter(filter), [filter]);

  const list = useCallback((path: string, hidden: boolean) => executeCommand<PlaceDirectory>({
    id: "places.list", context: CONTEXT, arguments: { path, maxEntries: ENTRY_LIMIT, showHidden: hidden },
  }), []);

  // Places load once, on first use: the roots, then the path down to the home folder.
  const started = useRef(false);
  const alive = useRef(true);
  useEffect(() => () => { alive.current = false; }, []);
  useEffect(() => {
    if (tab !== "places" || started.current) return;
    started.current = true;
    void (async () => {
      const roots = await executeCommand<Place[]>({ id: "places.roots", context: CONTEXT, arguments: {} });
      if (!alive.current) return;
      const found = Array.isArray(roots) ? roots : [];
      setPlaces(found);
      const homePlace = found.find((place) => place.kind === "home");
      const chain = homePlace ? pathAncestors(homePlace.path) : [];
      if (!homePlace || !found.some((place) => place.kind !== "home" && place.path === chain[0])) return;
      setHome(homePlace.path);
      const loaded: Record<string, PlaceDirectory> = {};
      for (const path of chain) {
        try { loaded[path] = await list(path, false); } catch { break; }
      }
      if (!alive.current) return;
      setDirectories((current) => ({ ...current, ...loaded }));
      // Keep folders a "Show in Places" may have opened meanwhile.
      setExpanded((current) => [...new Set([...current, ...chain.filter((path) => loaded[path])])]);
    })().catch((error: unknown) => {
      if (!alive.current) return;
      setPlaces([]);
      onStatus(error instanceof Error ? error.message : "Could not list places");
    });
  }, [list, onStatus, tab]);

  // Show in Places: open every folder above the file, then highlight it.
  const showHiddenRef = useRef(showHidden);
  showHiddenRef.current = showHidden;
  const matchesRef = useRef(matches);
  matchesRef.current = matches;
  const revealId = reveal?.id;
  useEffect(() => {
    if (!reveal || tab !== "places") return undefined;
    let cancelled = false;
    const target = reveal.path;
    const chain = pathAncestors(target).slice(0, -1);
    void (async () => {
      const loaded: Record<string, PlaceDirectory> = {};
      for (const path of chain) {
        if (directoriesRef.current[path]) continue;
        try {
          loaded[path] = await list(path, showHiddenRef.current);
        } catch (error: unknown) {
          if (!cancelled) onStatus(error instanceof Error ? error.message : "Could not show the file in Places");
          return;
        }
      }
      if (cancelled) return;
      setDirectories((current) => ({ ...current, ...loaded }));
      setExpanded((current) => [...new Set([...current, ...chain])]);
      setRevealed(target);
      const parent = loaded[chain.at(-1) ?? ""] ?? directoriesRef.current[chain.at(-1) ?? ""];
      const entry = parent?.entries.find((item) => item.path === target);
      if (!entry) onStatus("The file is not listed in Places; it may be hidden");
      else if (!matchesRef.current(entry.name)) setFilter("");
    })();
    return () => { cancelled = true; };
  }, [revealId, tab]);
  useEffect(() => {
    if (!revealed) return;
    const row = [...(treeRef.current?.querySelectorAll<HTMLElement>("[data-path]") ?? [])]
      .find((element) => element.dataset.path === revealed);
    row?.scrollIntoView?.({ block: "nearest" });
    row?.focus({ preventScroll: true });
  }, [directories, expanded, revealed]);

  const toggle = async (path: string) => {
    if (expanded.includes(path)) {
      setExpanded((current) => current.filter((item) => item !== path));
      return;
    }
    if (!directoriesRef.current[path]) {
      if (Object.keys(directoriesRef.current).length >= MAX_LOADED_DIRECTORIES) {
        onStatus(`Explorer directory limit (${MAX_LOADED_DIRECTORIES}) reached; collapse a folder first`);
        return;
      }
      try {
        const listing = await list(path, showHidden);
        setDirectories((current) => ({ ...current, [path]: listing }));
      } catch (error: unknown) {
        onStatus(error instanceof Error ? error.message : "Could not list folder");
        return;
      }
    }
    setExpanded((current) => [...current, path]);
  };

  const toggleHidden = async () => {
    const next = !showHidden;
    setShowHidden(next);
    // Reload what is on screen; collapsed folders reload when reopened.
    const reloaded: Record<string, PlaceDirectory> = {};
    for (const path of expanded) {
      try { reloaded[path] = await list(path, next); } catch { /* Keeps showing nothing for that folder. */ }
    }
    setDirectories(reloaded);
    setExpanded((current) => current.filter((path) => reloaded[path]));
  };

  const renderEntries = (path: string, depth: number): ReactNode => {
    const directory = directories[path];
    if (!directory) return null;
    const visible = directory.entries.filter((entry) => entry.kind === "directory" || matches(entry.name));
    return <>
      {visible.map((entry) => renderRow(entry, depth))}
      {visible.length === 0 ? <p className="tree-limit" style={{ paddingLeft: 12 + depth * 14 }}>
        {directory.entries.length === 0 ? "Empty" : "No files match the filter"}</p> : null}
      {directory.truncated ? <p className="tree-limit" style={{ paddingLeft: 12 + depth * 14 }}>First {ENTRY_LIMIT} entries shown</p> : null}
    </>;
  };

  const renderRow = (entry: Pick<PlaceEntry, "name" | "path" | "kind">, depth: number, icon?: IconName) => {
    const isDirectory = entry.kind === "directory";
    const open = isDirectory && expanded.includes(entry.path);
    return <div key={entry.path}>
      <button type="button" data-path={entry.path}
        className={`tree-row ${entry.kind}${entry.path === home ? " home" : ""}${entry.path === revealed ? " revealed" : ""}`}
        style={{ paddingLeft: 8 + depth * 14 }} disabled={entry.kind === "other"} title={entry.path}
        aria-expanded={isDirectory ? open : undefined}
        onClick={() => { if (isDirectory) void toggle(entry.path); else onOpenPath(entry.path); }}>
        <span className="tree-icon" aria-hidden="true">{isDirectory ? (open ? "▾" : "▸") : ""}</span>
        <Icon name={icon ?? (isDirectory ? (open ? "open" : "folder") : "document")} size={15} />
        <span className="tree-name">{entry.name}</span>
      </button>
      {open ? renderEntries(entry.path, depth + 1) : null}
    </div>;
  };

  const roots = (places ?? []).filter((place) => place.kind !== "home");
  return <div className="explorer file-view">
    <div className="panel-heading"><span>FILE VIEW</span></div>
    <div className="explorer-tabs" role="tablist" aria-label="File view">
      {EXPLORER_TABS.map((item) => <button type="button" role="tab" key={item.id} aria-selected={item.id === tab}
        className={item.id === tab ? "active" : ""} onClick={() => onTab(item.id)}>{item.title}</button>)}
    </div>
    {tab === "open" ? <div className="workspace-tree" role="tree" aria-label="Open files">
      {documents.length === 0 ? <p className="tree-limit">No files are open.</p> : documents.map((document) => (
        <button type="button" key={document.documentId} title={document.sourcePath ?? document.name}
          className={`tree-row file${document.documentId === activeDocumentId ? " home" : ""}`}
          onContextMenu={(event) => { event.preventDefault(); onDocumentMenu?.(document.documentId, event.clientX, event.clientY); }}
          onClick={() => onShowDocument(document.documentId)}>
          <span className="tree-icon" aria-hidden="true" /><Icon name="document" size={15} />
          <span className="tree-name">{document.name}</span>
          {document.dirty ? <span className="tab-dirty" aria-label="unsaved changes">●</span> : null}
        </button>))}
    </div> : tab === "places" ? <>
      <div className="explorer-filter">
        <input type="search" aria-label="File filter" placeholder="All files (*)" title="Show only matching files, for example *.md; *.txt"
          value={filter} onChange={(event) => setFilter(event.currentTarget.value)} />
        <button type="button" aria-pressed={showHidden} title="Show hidden files and folders"
          aria-label="Show hidden files" onClick={() => void toggleHidden()}>.*</button>
      </div>
      <div className="workspace-tree" role="tree" aria-label="Places" ref={treeRef}>
        {places === null ? <div className="tree-loading">Loading…</div>
          : roots.length === 0 ? <p className="tree-limit">No places are available.</p>
            : roots.map((place) => renderRow({ name: place.name, path: place.path, kind: "directory" }, 0, "drive"))}
      </div>
    </> : folder}
  </div>;
}
