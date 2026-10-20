import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type {
  ByteRange,
  ColumnRange,
  DocumentSummary,
  FileSearchScope,
  FindAllResult,
  ReplaceAllResult,
  ReplaceFileOutcome,
  ReplacePreview,
  SearchOptions,
  SearchRequest,
  WorkspaceSearchResult,
} from "./contracts";
import { executeCommand, type HelpTopic } from "./ipc";
import { IconButton } from "./icons";
import { ContextMenu } from "./Toolbar";

export type FindTab = "find" | "findInFiles" | "replace" | "replaceInFiles";
type DocumentScope = "document" | "selection" | "openFiles";
type FileScope = "folder" | "openFiles";

/** A place to show: an open document or a file on disk, and a byte range in it. */
export interface MatchTarget {
  documentId?: string;
  path?: string;
  startByte: string;
  endByte: string;
}

export interface ResultRow extends MatchTarget {
  key: string;
  /** File name, shown when results come from several files. */
  source?: string;
  line: string;
  text: string;
  /** The start of the line, for Bookmark All. */
  lineStartByte?: string;
}

export interface ResultList {
  title: string;
  rows: ResultRow[];
  truncated: boolean;
  multipleSources: boolean;
}

/** What the editor window tells the separate Find window. */
export interface FindContext {
  activeDocument: DocumentSummary | null;
  textDocuments: DocumentSummary[];
  selection?: ByteRange;
  defaultFolder?: string;
}

/** Events between the editor window and the separate Find and results windows. */
export const FIND_EVENTS = {
  /** Editor → Find window: a {@link FindContext}. */
  context: "nulis://find-context",
  /** Editor → Find window: `{ tab, seed? }`. */
  open: "nulis://find-open",
  /** Editor → results window: a {@link ResultList} with `openDocumentIds`. */
  results: "nulis://find-results",
  /** Find or results window → editor (relayed by the adapter): a {@link FindMessage}. */
  message: "nulis://find-message",
} as const;

export type FindMessage =
  | { kind: "ready" }
  | { kind: "search"; request: Omit<SearchRequest, "id"> }
  | { kind: "openMatch"; target: MatchTarget }
  | { kind: "documentChange"; document: DocumentSummary }
  | { kind: "status"; message: string }
  | { kind: "results"; list: ResultList }
  | { kind: "resultsReady" };

/** The window chrome: a floating panel in the browser shell, nothing in a desktop window. */
function Frame({ framed, title, label, initial, width, onClose, className, children }: {
  framed: boolean; title: string; label: string; initial: { x: number; y: number }; width: number;
  onClose(): void; className: string; children: ReactNode;
}) {
  if (framed) {
    return <FloatingWindow title={title} label={label} initial={initial} width={width} onClose={onClose} className={className}>
      {children}
    </FloatingWindow>;
  }
  return <main className={`aux-window ${className}`} aria-label={label}
    onKeyDown={(event) => { if (event.key === "Escape") { event.stopPropagation(); onClose(); } }}>
    {children}
  </main>;
}

/** How often Count All asks the core how far it has got. */
const COUNT_POLL_MS = 200;

/** A Count All report from the core; byte and match counts are decimal strings. */
export interface CountReport {
  matches: string;
  lines: string;
  scannedBytes: string;
  totalBytes: string;
  documents: number;
  documentsWithMatches: number;
  done: boolean;
  cancelled: boolean;
  error?: string;
}

const grouped = (value: string | number) => Number(value).toLocaleString("en-US");
const plural = (count: string | number, word: string) => `${grouped(count)} ${word}${Number(count) === 1 ? "" : word.endsWith("ch") ? "es" : "s"}`;

/** Bytes as KB, MB, or GB with one decimal. */
export function sizeText(bytes: string | number): string {
  const value = Number(bytes);
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let index = 0;
  let scaled = value;
  while (scaled >= 1024 && index < units.length - 1) { scaled /= 1024; index += 1; }
  return index === 0 ? `${value} bytes` : `${scaled.toFixed(1)} ${units[index]}`;
}

function percent(report: CountReport): number {
  const total = Number(report.totalBytes);
  return total === 0 ? 100 : Math.floor((Number(report.scannedBytes) / total) * 100);
}

/** "Counting… 41% (13.0 GB of 31.8 GB) · 1,234,567 matches so far" */
export function countingText(report: CountReport): string {
  return `Counting… ${percent(report)}% (${sizeText(report.scannedBytes)} of ${sizeText(report.totalBytes)}) · ${plural(report.matches, "match")} so far`;
}

/** The final Count All result, or where a cancelled count stopped. */
export function countResultText(report: CountReport): string {
  const found = `${plural(report.matches, "match")} on ${plural(report.lines, "line")}`;
  const files = report.documents > 1 ? ` in ${plural(report.documentsWithMatches, "file")}` : "";
  return report.cancelled ? `Stopped at ${percent(report)}%: ${found} so far${files}` : `${found}${files}`;
}

const HISTORY_LIMIT = 20;
const HISTORY_KEYS = { find: "nulis.find.history", replace: "nulis.replace.history" } as const;
const TABS: Array<{ id: FindTab; label: string }> = [
  { id: "find", label: "Find" },
  { id: "findInFiles", label: "Find in Files" },
  { id: "replace", label: "Replace" },
  { id: "replaceInFiles", label: "Replace in Files" },
];

function readHistory(kind: keyof typeof HISTORY_KEYS): string[] {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(HISTORY_KEYS[kind]) ?? "[]");
    return Array.isArray(stored) ? stored.filter((entry): entry is string => typeof entry === "string").slice(0, HISTORY_LIMIT) : [];
  } catch {
    return [];
  }
}

function remember(kind: keyof typeof HISTORY_KEYS, value: string): string[] {
  const next = [value, ...readHistory(kind).filter((entry) => entry !== value)].slice(0, HISTORY_LIMIT);
  try { localStorage.setItem(HISTORY_KEYS[kind], JSON.stringify(next)); } catch { /* history is a convenience */ }
  return next;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function errorText(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : typeof error === "string" ? error : fallback;
}

/** A movable, non-modal window drawn inside the main window. */
export function FloatingWindow({ title, label, initial, width, onClose, children, className }: {
  title: string; label: string; initial: { x: number; y: number }; width: number;
  onClose(): void; children: ReactNode; className?: string;
}) {
  const [position, setPosition] = useState(initial);
  const drag = useRef<{ dx: number; dy: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const clamp = (x: number, y: number) => {
    const box = ref.current;
    const maxX = Math.max(0, window.innerWidth - (box?.offsetWidth ?? width));
    const maxY = Math.max(0, window.innerHeight - 40);
    return { x: Math.min(Math.max(0, x), maxX), y: Math.min(Math.max(0, y), maxY) };
  };
  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || (event.target as HTMLElement).closest("button")) return;
    drag.current = { dx: event.clientX - position.x, dy: event.clientY - position.y };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };
  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    setPosition(clamp(event.clientX - drag.current.dx, event.clientY - drag.current.dy));
  };
  const onKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") { event.stopPropagation(); onClose(); }
  };
  return <div ref={ref} className={`floating-window${className ? ` ${className}` : ""}`} role="dialog" aria-modal="false"
    aria-label={label} style={{ left: position.x, top: position.y, width }} onKeyDown={onKeyDown}>
    <div className="floating-title" onPointerDown={onPointerDown} onPointerMove={onPointerMove}
      onPointerUp={() => { drag.current = null; }} onPointerCancel={() => { drag.current = null; }}>
      <span>{title}</span>
      <IconButton icon="close" label={`Close ${label}`} shortcut="Escape" onClick={onClose} />
    </div>
    <div className="floating-body">{children}</div>
  </div>;
}

function Check({ label, checked, onChange, disabled }: {
  label: string; checked: boolean; onChange(value: boolean): void; disabled?: boolean;
}) {
  return <label className="find-check">
    <input type="checkbox" checked={checked} disabled={disabled} onChange={(event) => onChange(event.currentTarget.checked)} />
    <span>{label}</span>
  </label>;
}

function HistoryField({ label, value, onChange, history, onEnter, autoFocus }: {
  label: string; value: string; onChange(value: string): void; history: string[];
  onEnter?(shift: boolean): void; autoFocus?: boolean;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  return <div className="find-field">
    <span className="find-label">{label}</span>
    <div className="find-field-row">
      <textarea aria-label={label.replace(/:$/, "")} value={value} rows={3} spellCheck={false} autoFocus={autoFocus}
        onChange={(event) => onChange(event.currentTarget.value)}
        onKeyDown={(event) => {
          // Enter runs the main action; Ctrl/⌘ Enter adds a line break for multi-line searches.
          if (event.key === "Enter" && !event.ctrlKey && !event.metaKey && onEnter) {
            event.preventDefault();
            onEnter(event.shiftKey);
          }
        }} />
      <IconButton icon="history" label={`Recent ${label.replace(/:$/, "").toLowerCase()} entries`} disabled={history.length === 0}
        onClick={() => {
          const button = document.activeElement as HTMLElement | null;
          const bounds = button?.getBoundingClientRect();
          setMenu({ x: bounds?.left ?? 0, y: bounds?.bottom ?? 0 });
        }} />
    </div>
    {menu ? <ContextMenu x={menu.x} y={menu.y} label={`Recent ${label.replace(/:$/, "").toLowerCase()} entries`}
      items={history.map((entry) => ({ label: entry.length > 60 ? `${entry.slice(0, 60)}…` : entry, onSelect: () => onChange(entry) }))}
      onClose={() => setMenu(null)} /> : null}
  </div>;
}

export interface FindReplacePanelProps {
  tab: FindTab;
  onTab(tab: FindTab): void;
  onClose(): void;
  /** Text to put in "Find what"; a new `id` applies it again. */
  seed?: { id: number; text: string };
  activeDocument: DocumentSummary | null;
  /** Open documents in the text editor (hex views are left out). */
  textDocuments: DocumentSummary[];
  /** The active document's primary selection, if any. */
  selection?: ByteRange;
  /** Runs a find or replace action in the focused editor. */
  onEditorSearch(request: Omit<SearchRequest, "id">): void;
  onOpenMatch(target: MatchTarget): void;
  onDocumentChange(document: DocumentSummary): void;
  onResults(list: ResultList): void;
  onChooseFolder(): Promise<string | null>;
  /** The open workspace folder, used until another folder is chosen. */
  defaultFolder?: string;
  onStatus(message: string): void;
  /** False inside its own desktop window, which supplies the title bar. */
  framed?: boolean;
  /** Opens Nulis Help at a topic; without it the "Regex help" link is not shown. */
  onHelp?(topic: HelpTopic): void;
}

/** Where the regular-expression syntax is explained in Nulis Help. */
export const REGEX_HELP: HelpTopic = { section: "search", heading: "regular-expressions" };

const OPTIONS_KEY = "nulis.find.options.v1";

/** Find options remembered between sessions. */
interface StoredOptions {
  caseSensitive: boolean;
  wholeWord: boolean;
  regex: boolean;
  highlight: boolean;
  listLines: boolean;
  closeAfter: boolean;
  optionsOpen: boolean;
  columnSearch: boolean;
  columnFrom: string;
  columnTo: string;
}

const DEFAULT_OPTIONS: StoredOptions = {
  caseSensitive: false, wholeWord: false, regex: false, highlight: false, listLines: false, closeAfter: false,
  optionsOpen: true, columnSearch: false, columnFrom: "", columnTo: "",
};

function readOptions(): StoredOptions {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(OPTIONS_KEY) ?? "{}");
    if (!stored || typeof stored !== "object") return DEFAULT_OPTIONS;
    const merged = { ...DEFAULT_OPTIONS };
    for (const [key, fallback] of Object.entries(DEFAULT_OPTIONS) as Array<[keyof StoredOptions, boolean | string]>) {
      const value = (stored as Record<string, unknown>)[key];
      if (typeof value === typeof fallback) (merged as Record<string, unknown>)[key] = value;
    }
    return merged;
  } catch {
    return DEFAULT_OPTIONS;
  }
}

/** A column field: empty, or a whole number from 1. */
function columnNumber(text: string): number | undefined | null {
  const trimmed = text.trim();
  if (!trimmed) return undefined;
  return /^\d+$/.test(trimmed) && Number(trimmed) >= 1 ? Number(trimmed) : null;
}

/** The column range the fields describe, or why they do not describe one. */
export function columnRange(from: string, to: string): { columns?: ColumnRange; error?: string } {
  const start = columnNumber(from);
  const end = columnNumber(to);
  if (start === null || end === null) return { error: "Columns are whole numbers from 1" };
  if (start === undefined && end === undefined) return { error: "Enter a From or To column" };
  if (start !== undefined && end !== undefined && end < start) return { error: `To (${end}) is before From (${start})` };
  return { columns: { ...(start !== undefined ? { from: start } : {}), ...(end !== undefined ? { to: end } : {}) } };
}

/** The Find and Replace panel: Find, Find in Files, and Replace tabs. */
export function FindReplacePanel(props: FindReplacePanelProps) {
  const { tab, onTab, onClose, seed, activeDocument, textDocuments, selection, onEditorSearch, onOpenMatch,
    onDocumentChange, onResults, onChooseFolder, defaultFolder, onStatus, onHelp, framed = true } = props;
  const [stored] = useState(readOptions);
  const [query, setQuery] = useState(seed?.text ?? "");
  const [replacement, setReplacement] = useState("");
  const [findHistory, setFindHistory] = useState(() => readHistory("find"));
  const [replaceHistory, setReplaceHistory] = useState(() => readHistory("replace"));
  const [documentScope, setDocumentScope] = useState<DocumentScope>("document");
  const [fileScope, setFileScope] = useState<FileScope>("folder");
  const [include, setInclude] = useState("");
  const [folder, setFolder] = useState(defaultFolder ?? "");
  const [recursive, setRecursive] = useState(true);
  const [optionsOpen, setOptionsOpen] = useState(stored.optionsOpen);
  const [caseSensitive, setCaseSensitive] = useState(stored.caseSensitive);
  const [wholeWord, setWholeWord] = useState(stored.wholeWord);
  const [regex, setRegex] = useState(stored.regex);
  const [highlight, setHighlight] = useState(stored.highlight);
  const [listLines, setListLines] = useState(stored.listLines);
  const [closeAfter, setCloseAfter] = useState(stored.closeAfter);
  const [columnSearch, setColumnSearch] = useState(stored.columnSearch);
  const [columnFrom, setColumnFrom] = useState(stored.columnFrom);
  const [columnTo, setColumnTo] = useState(stored.columnTo);
  const [patternError, setPatternError] = useState<string | null>(null);
  const [summary, setSummary] = useState("");
  const [busy, setBusy] = useState(false);
  // Replace in Files: the preview, what it was made for, and whether it awaits confirmation.
  const [preview, setPreview] = useState<{ key: string; result: ReplacePreview } | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [failures, setFailures] = useState<ReplaceFileOutcome[]>([]);
  // Matches in "selected text" or "all open files", stepped through by Next and Previous.
  const stepping = useRef<{ key: string; targets: MatchTarget[]; index: number } | null>(null);

  useEffect(() => {
    if (seed) setQuery(seed.text);
  }, [seed?.id]);
  // The workspace folder may arrive after the window opened.
  useEffect(() => {
    if (defaultFolder) setFolder((current) => current || defaultFolder);
  }, [defaultFolder]);

  useEffect(() => {
    const options: StoredOptions = {
      caseSensitive, wholeWord, regex, highlight, listLines, closeAfter, optionsOpen, columnSearch, columnFrom, columnTo,
    };
    try { localStorage.setItem(OPTIONS_KEY, JSON.stringify(options)); } catch { /* options are a convenience */ }
  }, [caseSensitive, wholeWord, regex, highlight, listLines, closeAfter, optionsOpen, columnSearch, columnFrom, columnTo]);

  // Column search applies where one document is searched: the Find and Replace tabs.
  const columnsApply = columnSearch && (tab === "find" || tab === "replace");
  const column = columnsApply ? columnRange(columnFrom, columnTo) : {};
  const columns = column.columns;
  const options: SearchOptions = { mode: regex ? "regex" : "literal", caseSensitive, wholeWord, ...(columns ? { columns } : {}) };
  const editorRequest = (action: SearchRequest["action"], within?: ByteRange): Omit<SearchRequest, "id"> => ({
    action, query, replacement, regex, caseSensitive, wholeWord, highlight, within, ...(columns ? { columns } : {}),
  });

  // A regular expression is checked as it is typed, by the same engine that searches.
  useEffect(() => {
    if (!regex || !query) { setPatternError(null); return; }
    let current = true;
    const timer = setTimeout(() => {
      executeCommand({
        id: "search.validatePattern", context: { windowId: "main" },
        arguments: { query, options: { mode: "regex", caseSensitive, wholeWord } },
      }).then(() => { if (current) setPatternError(null); })
        .catch((error: unknown) => {
          const message = errorText(error, "Invalid regular expression");
          if (current) setPatternError(message.charAt(0).toUpperCase() + message.slice(1));
        });
    }, 200);
    return () => { current = false; clearTimeout(timer); };
  }, [query, regex, caseSensitive, wholeWord]);
  const inputError = patternError ?? column.error ?? null;

  // Highlighting follows the checkbox, and while it is on, the query and options as they change.
  const highlightKey = JSON.stringify([query, regex, caseSensitive, wholeWord, columns ?? null]);
  const sentHighlight = useRef(false);
  useEffect(() => {
    if (!query || inputError || (!highlight && !sentHighlight.current)) return;
    const timer = setTimeout(() => {
      sentHighlight.current = highlight;
      onEditorSearch(editorRequest("highlight"));
    }, 250);
    return () => clearTimeout(timer);
  }, [highlight, highlightKey, inputError]);

  const rememberQuery = () => { if (query) setFindHistory(remember("find", query)); };
  const rememberReplacement = () => { if (replacement) setReplaceHistory(remember("replace", replacement)); };

  const findAllIn = async (document: DocumentSummary, within?: ByteRange) => executeCommand<FindAllResult>({
    id: "search.findAll",
    context: { windowId: "main", documentId: document.documentId },
    arguments: { query, options, ...(within ? { within } : {}) },
  });

  /** Every match in the chosen scope, document by document. */
  const collect = async (): Promise<Array<{ document: DocumentSummary; result: FindAllResult }>> => {
    if (documentScope === "openFiles") {
      const found = [];
      for (const document of textDocuments) found.push({ document, result: await findAllIn(document) });
      return found;
    }
    if (!activeDocument) throw new Error("Open a text document to search in");
    if (documentScope === "selection") {
      if (!selection || selection.startByte === selection.endByte) throw new Error("Select the text to search in first");
      return [{ document: activeDocument, result: await findAllIn(activeDocument, selection) }];
    }
    return [{ document: activeDocument, result: await findAllIn(activeDocument) }];
  };

  const showLines = (found: Array<{ document: DocumentSummary; result: FindAllResult }>) => {
    const multipleSources = found.length > 1;
    onResults({
      title: `Lines containing “${query.length > 40 ? `${query.slice(0, 40)}…` : query}”`,
      multipleSources,
      truncated: found.some(({ result }) => result.truncated),
      rows: found.flatMap(({ document, result }) => {
        const firstOnLine = new Map<string, FindAllResult["matches"][number]>();
        for (const match of result.matches) if (!firstOnLine.has(match.line)) firstOnLine.set(match.line, match);
        return result.lines.map((line) => {
        const first = firstOnLine.get(line.line);
        return {
          key: `${document.documentId}:${line.startByte}`,
          documentId: document.documentId,
          source: multipleSources ? document.name : undefined,
          line: line.line,
          text: line.truncated ? `${line.text}…` : line.text,
          startByte: first?.startByte ?? line.startByte,
          endByte: first?.endByte ?? line.startByte,
          lineStartByte: line.startByte,
        };
        });
      }),
    });
  };

  const run = async (action: () => Promise<void>) => {
    if (!query) { onStatus("Enter text to find"); return; }
    if (inputError) { onStatus(inputError); return; }
    setBusy(true);
    try { await action(); } catch (error: unknown) { onStatus(errorText(error, "Search failed")); } finally { setBusy(false); }
  };

  // Count All runs as a background job in the core: every match is counted,
  // however many, with progress and a Cancel button.
  const [countJob, setCountJob] = useState<number | null>(null);
  const mounted = useRef(true);
  useEffect(() => () => { mounted.current = false; }, []);
  const cancelCount = () => {
    if (countJob === null) return;
    void executeCommand({ id: "search.countCancel", context: { windowId: "main" }, arguments: { jobId: countJob } })
      .catch(() => undefined);
  };
  const countAll = () => run(async () => {
    rememberQuery();
    let targets: Array<{ documentId: string; within?: ByteRange }>;
    if (documentScope === "openFiles") {
      targets = textDocuments.map((document) => ({ documentId: document.documentId }));
    } else {
      if (!activeDocument) throw new Error("Open a text document to search in");
      if (documentScope === "selection" && (!selection || selection.startByte === selection.endByte)) {
        throw new Error("Select the text to search in first");
      }
      targets = [{ documentId: activeDocument.documentId, ...(documentScope === "selection" && selection ? { within: selection } : {}) }];
    }
    if (targets.length === 0) throw new Error("Open a text document to search in");
    const { jobId } = await executeCommand<{ jobId: number }>({
      id: "search.countStart", context: { windowId: "main" }, arguments: { query, options, targets },
    });
    setCountJob(jobId);
    let report: CountReport;
    try {
      for (;;) {
        await new Promise((resolve) => setTimeout(resolve, COUNT_POLL_MS));
        if (!mounted.current) {
          void executeCommand({ id: "search.countCancel", context: { windowId: "main" }, arguments: { jobId } }).catch(() => undefined);
          return;
        }
        report = await executeCommand<CountReport>({ id: "search.countProgress", context: { windowId: "main" }, arguments: { jobId } });
        if (report.done) break;
        setSummary(countingText(report));
      }
    } finally {
      if (mounted.current) setCountJob(null);
    }
    if (report.error) throw new Error(report.error);
    const message = countResultText(report);
    setSummary(message);
    onStatus(message);
    if (report.cancelled) return;
    if (listLines) showLines(await collect());
    if (closeAfter) onClose();
  });

  const step = (direction: "next" | "previous") => run(async () => {
    rememberQuery();
    if (documentScope === "document") {
      onEditorSearch(editorRequest(direction));
      if (listLines) showLines(await collect());
      if (closeAfter) onClose();
      return;
    }
    const key = JSON.stringify([query, options, documentScope, selection, textDocuments.map((document) => document.revision)]);
    if (stepping.current?.key !== key) {
      const found = await collect();
      stepping.current = {
        key, index: -1,
        targets: found.flatMap(({ document, result }) => result.matches.map((match) => ({
          documentId: document.documentId, startByte: match.startByte, endByte: match.endByte,
        }))),
      };
      if (listLines) showLines(found);
    }
    const { targets } = stepping.current;
    if (targets.length === 0) { setSummary("No matches"); onStatus(`No matches for “${query}”`); return; }
    const count = targets.length;
    stepping.current.index = direction === "next"
      ? (stepping.current.index + 1) % count
      : (stepping.current.index - 1 + count) % count;
    const target = targets[stepping.current.index];
    if (target) onOpenMatch(target);
    setSummary(`Match ${stepping.current.index + 1} of ${count}`);
    if (closeAfter) onClose();
  });

  const replaceOne = () => run(async () => {
    rememberQuery();
    rememberReplacement();
    onEditorSearch(editorRequest("replace"));
  });

  const replaceAll = () => run(async () => {
    rememberQuery();
    rememberReplacement();
    if (documentScope === "document") {
      onEditorSearch(editorRequest("replaceAll"));
    } else if (documentScope === "selection") {
      if (!selection || selection.startByte === selection.endByte) throw new Error("Select the text to replace in first");
      onEditorSearch(editorRequest("replaceAll", selection));
    } else {
      await replaceAllOpenFiles();
    }
    if (closeAfter) onClose();
  });

  const replaceAllOpenFiles = async () => {
    {
      let total = 0;
      let changed = 0;
      const skipped: string[] = [];
      for (const document of textDocuments) {
        if (document.readOnly) { skipped.push(document.name); continue; }
        const result = await executeCommand<ReplaceAllResult>({
          id: "search.replaceAll",
          context: { windowId: "main", documentId: document.documentId },
          arguments: { expectedRevision: document.revision, query, replacement, options },
        });
        if (result.replacements > 0) { total += result.replacements; changed += 1; onDocumentChange(result.document); }
      }
      const message = `Replaced ${total} match${total === 1 ? "" : "es"} in ${changed} file${changed === 1 ? "" : "s"}${
        skipped.length ? `; skipped read-only ${skipped.join(", ")}` : ""}`;
      setSummary(message);
      onStatus(message);
    }
  };

  const fileScopeArgument = (): FileSearchScope => ({
    include: include.split(/[;,]/).map((pattern) => pattern.trim()).filter(Boolean),
    recursive,
  });
  // A preview is valid only for the exact search it was made for.
  const previewKey = JSON.stringify([query, replacement, options, folder, include, recursive]);
  const previewIsCurrent = preview?.key === previewKey;
  const previewMatches = preview ? preview.result.files.reduce((total, file) => total + file.matches, 0) : 0;

  const previewReplaceInFiles = () => run(async () => {
    rememberQuery();
    rememberReplacement();
    if (!folder) throw new Error("Choose a folder to replace in");
    setConfirming(false);
    setFailures([]);
    onStatus("Looking for files to change…");
    const result = await executeCommand<ReplacePreview>({
      id: "search.previewReplaceInFiles", context: { windowId: "main" },
      arguments: { root: folder, query, options, scope: fileScopeArgument() },
    });
    setPreview({ key: previewKey, result });
    const matches = result.files.reduce((total, file) => total + file.matches, 0);
    const message = result.files.length === 0
      ? `No matches in ${result.filesScanned} files`
      : `${matches} match${matches === 1 ? "" : "es"} in ${result.files.length} file${result.files.length === 1 ? "" : "s"}${
        result.truncated ? " (limit reached: only these files would change)" : ""}`;
    setSummary(message);
    onStatus(message);
  });

  const replaceInFiles = () => run(async () => {
    if (fileScope === "openFiles") { await replaceAllOpenFiles(); return; }
    if (!preview || !previewIsCurrent) throw new Error("Preview the files first");
    setConfirming(false);
    const outcomes = await executeCommand<ReplaceFileOutcome[]>({
      id: "search.replaceInFiles", context: { windowId: "main" },
      arguments: {
        files: preview.result.files.map((file) => ({ path: file.path, stamp: file.stamp })),
        query, replacement, options,
      },
    });
    for (const outcome of outcomes) if (outcome.document) onDocumentChange(outcome.document);
    const failed = outcomes.filter((outcome) => outcome.error);
    const changed = outcomes.filter((outcome) => !outcome.error && outcome.replacements > 0);
    const total = changed.reduce((sum, outcome) => sum + outcome.replacements, 0);
    const message = `Replaced ${total} match${total === 1 ? "" : "es"} in ${changed.length} file${changed.length === 1 ? "" : "s"}${
      failed.length ? `; ${failed.length} file${failed.length === 1 ? "" : "s"} not changed` : ""}`;
    setFailures(failed);
    setPreview(null);
    setSummary(message);
    onStatus(message);
    if (closeAfter) onClose();
  });

  const findInFiles = () => run(async () => {
    rememberQuery();
    if (fileScope === "openFiles") {
      const found = [];
      for (const document of textDocuments) found.push({ document, result: await findAllIn(document) });
      showLines(found);
      const matches = found.reduce((total, { result }) => total + result.matches.length, 0);
      setSummary(`${matches} matches in ${found.filter(({ result }) => result.matches.length > 0).length} open files`);
      return;
    }
    if (!folder) throw new Error("Choose a folder to search in");
    const scope = fileScopeArgument();
    onStatus("Searching files…");
    const result = await executeCommand<WorkspaceSearchResult>({
      id: "search.findInFiles", context: { windowId: "main" },
      arguments: { root: folder, query, options, scope, maxResults: 2000 },
    });
    const files = new Set(result.matches.map((match) => match.path)).size;
    const message = `${result.matches.length} matches in ${files} of ${result.filesScanned} files${result.truncated ? " (limit reached)" : ""}`;
    setSummary(message);
    onStatus(message);
    onResults({
      title: `Find in Files: “${query.length > 40 ? `${query.slice(0, 40)}…` : query}”`,
      multipleSources: true,
      truncated: result.truncated,
      rows: result.matches.map((match, index) => ({
        key: `${match.path}:${match.startByte}:${index}`,
        path: match.path,
        source: fileName(match.path),
        line: match.line,
        text: match.lineText,
        startByte: match.startByte,
        endByte: match.endByte,
      })),
    });
    if (closeAfter) onClose();
  });

  const regexCheck = <div className="find-regex">
    <Check label="Use regular expressions" checked={regex} onChange={setRegex} />
    {onHelp ? <IconButton icon="help" label="Regex help" className="find-help" onClick={() => onHelp(REGEX_HELP)} /> : null}
  </div>;
  const columnRow = <div className="find-columns" role="group" aria-label="Search in column">
    <Check label="Search in column:" checked={columnSearch} onChange={setColumnSearch} />
    <label>From <input aria-label="From column" inputMode="numeric" value={columnFrom} disabled={!columnSearch}
      onChange={(event) => setColumnFrom(event.currentTarget.value)} /></label>
    <label>To <input aria-label="To column" inputMode="numeric" value={columnTo} disabled={!columnSearch}
      onChange={(event) => setColumnTo(event.currentTarget.value)} /></label>
  </div>;
  // Mistakes show in the status line above the buttons, which keeps its place, so nothing moves while typing.
  const statusLine = <p className={`find-summary${inputError ? " find-error" : ""}`} role="status">{inputError ?? summary}</p>;
  const scopeSelect = (label: string) => <label className="find-where">
    <span className="find-label">{label}</span>
    <select aria-label={label.replace(/:$/, "")} value={documentScope}
      onChange={(event) => setDocumentScope(event.currentTarget.value as DocumentScope)}>
      <option value="document">Current document</option>
      <option value="selection">Selected text</option>
      <option value="openFiles">All open files</option>
    </select>
  </label>;
  const optionsToggle = (label: string) => <button type="button" className="find-options-toggle" aria-expanded={optionsOpen}
    onClick={() => setOptionsOpen((open) => !open)}><span aria-hidden="true">{optionsOpen ? "▾" : "▸"}</span> {label}</button>;

  return <Frame framed={framed} title="Find and Replace" label="Find and Replace" width={460}
    initial={{ x: Math.max(16, window.innerWidth - 500), y: 90 }} onClose={onClose} className="find-window">
    <div className="find-tabs" role="tablist" aria-label="Find and Replace mode">
      {TABS.map((item) => <button type="button" role="tab" key={item.id} aria-selected={tab === item.id}
        className={tab === item.id ? "active" : ""} onClick={() => onTab(item.id)}>{item.label}</button>)}
    </div>
    <div className="find-content" role="tabpanel" aria-label={TABS.find((item) => item.id === tab)?.label}>
      <HistoryField label="Find what:" value={query} onChange={setQuery} history={findHistory} autoFocus
        onEnter={(shift) => {
          if (tab === "findInFiles") void findInFiles();
          else if (tab === "replaceInFiles") void previewReplaceInFiles();
          else void step(shift ? "previous" : "next");
        }} />
      {tab === "replace" || tab === "replaceInFiles" ? <HistoryField label="Replace with:" value={replacement}
        onChange={setReplacement} history={replaceHistory}
        onEnter={() => void (tab === "replace" ? replaceOne() : previewReplaceInFiles())} /> : null}

      {tab === "findInFiles" || tab === "replaceInFiles" ? <>
        <label className="find-where">
          <span className="find-label">{tab === "findInFiles" ? "Find where:" : "Replace where:"}</span>
          <select aria-label={tab === "findInFiles" ? "Find where" : "Replace where"} value={fileScope} onChange={(event) => setFileScope(event.currentTarget.value as FileScope)}>
            <option value="folder">Files in a folder</option>
            <option value="openFiles">All open files</option>
          </select>
        </label>
        {fileScope === "folder" ? <>
          <label className="find-where">
            <span className="find-label">In files/types:</span>
            <input aria-label="In files or types" placeholder="*.rs; *.md (empty: all files)" value={include}
              onChange={(event) => setInclude(event.currentTarget.value)} />
          </label>
          <div className="find-where">
            <span className="find-label">In directory:</span>
            <div className="find-field-row">
              <input aria-label="In directory" value={folder} placeholder="Choose a folder" onChange={(event) => setFolder(event.currentTarget.value)} />
              <button type="button" onClick={() => void onChooseFolder().then((chosen) => { if (chosen) setFolder(chosen); })}>Browse…</button>
            </div>
          </div>
          <Check label="Include subdirectories" checked={recursive} onChange={setRecursive} />
        </> : null}
        {optionsToggle("Find options")}
        {optionsOpen ? <div className="find-options">
          <Check label="Match case" checked={caseSensitive} onChange={setCaseSensitive} />
          <Check label="Match whole word" checked={wholeWord} onChange={setWholeWord} />
          {regexCheck}
          {tab === "replaceInFiles" ? <Check label="Close after replace all" checked={closeAfter} onChange={setCloseAfter} /> : null}
        </div> : null}
        {statusLine}
        {tab === "replaceInFiles" && fileScope === "folder" && preview ? <ul className="replace-preview" aria-label="Files to change">
          {preview.result.files.map((file) => <li key={file.path} title={file.path}>
            <span className="result-source">{fileName(file.path)}</span>
            <span>{file.matches} match{file.matches === 1 ? "" : "es"}</span>
            {file.open ? <span className="replace-open">open: changed in the editor, not saved</span> : null}
          </li>)}
        </ul> : null}
        {failures.length ? <ul className="replace-failures" aria-label="Files not changed">
          {failures.map((failure) => <li key={failure.path} title={failure.path}>
            <span className="result-source">{fileName(failure.path)}</span><span>{failure.error}</span>
          </li>)}
        </ul> : null}
        {tab === "findInFiles" ? <div className="find-actions">
          <span />
          <button type="button" className="primary" disabled={busy || inputError !== null} onClick={() => void findInFiles()}>Find</button>
        </div> : fileScope === "openFiles" ? <div className="find-actions">
          <span />
          <button type="button" className="primary" disabled={busy || inputError !== null} onClick={() => void replaceInFiles()}>Replace All</button>
        </div> : confirming && previewIsCurrent ? <div className="find-actions find-confirm" role="group" aria-label="Confirm Replace in Files">
          <span>Replace {previewMatches} match{previewMatches === 1 ? "" : "es"} in {preview?.result.files.length} files?
            Closed files are saved right away.</span>
          <button type="button" onClick={() => setConfirming(false)}>Cancel</button>
          <button type="button" className="primary" disabled={busy || inputError !== null} onClick={() => void replaceInFiles()}>Replace</button>
        </div> : <div className="find-actions">
          <span />
          <button type="button" disabled={busy || inputError !== null} onClick={() => void previewReplaceInFiles()}>Preview</button>
          <button type="button" className="primary" disabled={busy || !previewIsCurrent || previewMatches === 0}
            title={previewIsCurrent ? undefined : "Preview the files first"} onClick={() => setConfirming(true)}>Replace All</button>
        </div>}
      </> : <>
        {scopeSelect(tab === "replace" ? "Replace where:" : "Find where:")}
        {optionsToggle("Options")}
        {optionsOpen ? <div className="find-options">
          <Check label="Match case" checked={caseSensitive} onChange={setCaseSensitive} />
          {tab === "find" ? <>
            <Check label="Highlight all items found" checked={highlight} onChange={setHighlight} />
            <Check label="Match whole word" checked={wholeWord} onChange={setWholeWord} />
            <Check label="List lines containing find string" checked={listLines} onChange={setListLines} />
          </> : <Check label="Match whole word" checked={wholeWord} onChange={setWholeWord} />}
          {columnRow}
          {regexCheck}
          <Check label={tab === "find" ? "Close after find" : "Close after replace all"} checked={closeAfter} onChange={setCloseAfter} />
        </div> : null}
        {statusLine}
        {tab === "find" ? <div className="find-actions">
          {countJob !== null
            ? <button type="button" onClick={cancelCount}>Cancel</button>
            : <button type="button" disabled={busy || inputError !== null} onClick={() => void countAll()}>Count All</button>}
          <span />
          <button type="button" disabled={busy || inputError !== null} onClick={() => void step("previous")}>Previous</button>
          <button type="button" className="primary" disabled={busy || inputError !== null} onClick={() => void step("next")}>Next</button>
        </div> : <div className="find-actions">
          <span />
          <button type="button" className="primary" disabled={busy || inputError !== null} onClick={() => void step("next")}>Next</button>
          <button type="button" disabled={busy || inputError !== null || documentScope !== "document"} title={documentScope !== "document"
            ? "Replace one match works in the current document" : undefined} onClick={() => void replaceOne()}>Replace</button>
          <button type="button" disabled={busy || inputError !== null} onClick={() => void replaceAll()}>Replace All</button>
        </div>}
      </>}
    </div>
  </Frame>;
}

/** The list of matching lines; choosing a row goes to it. */
export function ResultsWindow({ list, onClose, onOpenMatch, onStatus, openDocumentIds, framed = true }: {
  list: ResultList; onClose(): void; onOpenMatch(target: MatchTarget): void; onStatus(message: string): void;
  openDocumentIds: ReadonlySet<string>;
  /** False inside its own desktop window. */
  framed?: boolean;
}) {
  const [selected, setSelected] = useState(0);
  const [showLineNumbers, setShowLineNumbers] = useState(true);
  useEffect(() => setSelected(0), [list]);
  const current = list.rows[selected];
  const rowText = (row: ResultRow) => `${row.source ? `${row.source}:` : ""}${showLineNumbers ? `${row.line}: ` : ""}${row.text}`;

  const bookmarkAll = async () => {
    const byDocument = new Map<string, string[]>();
    for (const row of list.rows) {
      if (!row.documentId || !openDocumentIds.has(row.documentId)) continue;
      byDocument.set(row.documentId, [...(byDocument.get(row.documentId) ?? []), row.lineStartByte ?? row.startByte]);
    }
    if (byDocument.size === 0) { onStatus("Bookmark All works on lines in open documents"); return; }
    try {
      for (const [documentId, offsets] of byDocument) {
        await executeCommand({ id: "editor.addBookmarks", context: { windowId: "main", documentId }, arguments: { offsets } });
      }
      const count = [...byDocument.values()].reduce((total, offsets) => total + offsets.length, 0);
      onStatus(`Bookmarked ${count} line${count === 1 ? "" : "s"}`);
    } catch (error: unknown) {
      onStatus(errorText(error, "Could not add bookmarks"));
    }
  };

  const copy = () => {
    void writeText(list.rows.map(rowText).join("\n"))
      .then(() => onStatus(`Copied ${list.rows.length} line${list.rows.length === 1 ? "" : "s"}`))
      .catch((error: unknown) => onStatus(errorText(error, "Could not copy")));
  };

  return <Frame framed={framed} title={list.title} label="Search results" width={640} className="results-window"
    initial={{ x: 60, y: Math.max(80, window.innerHeight - 380) }} onClose={onClose}>
    <div className="results-layout">
      <div className="results-list" role="listbox" aria-label="Matching lines" tabIndex={0}
        aria-activedescendant={current ? `result-${selected}` : undefined}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown") { event.preventDefault(); setSelected((index) => Math.min(list.rows.length - 1, index + 1)); }
          if (event.key === "ArrowUp") { event.preventDefault(); setSelected((index) => Math.max(0, index - 1)); }
          if (event.key === "Enter" && current) { event.preventDefault(); onOpenMatch(current); }
        }}>
        {list.rows.length === 0 ? <p className="results-empty">No matching lines.</p> : list.rows.map((row, index) =>
          <div role="option" id={`result-${index}`} key={row.key} aria-selected={index === selected}
            className={index === selected ? "selected" : ""}
            onClick={() => { setSelected(index); onOpenMatch(row); }}>
            {row.source ? <span className="result-source">{row.source}</span> : null}
            {showLineNumbers ? <span className="result-line">{row.line}</span> : null}
            <span className="result-text">{row.text}</span>
          </div>)}
      </div>
      <div className="results-actions">
        <button type="button" disabled={!current} onClick={() => { if (current) onOpenMatch(current); }}>Go To</button>
        <button type="button" disabled={list.rows.length === 0} onClick={() => void bookmarkAll()}>Bookmark All</button>
        <button type="button" disabled={list.rows.length === 0} onClick={copy}>Copy</button>
      </div>
    </div>
    <div className="results-footer">
      <label className="find-check"><input type="checkbox" checked={showLineNumbers}
        onChange={(event) => setShowLineNumbers(event.currentTarget.checked)} /><span>Show line numbers</span></label>
      <span className="find-summary">{list.rows.length} line{list.rows.length === 1 ? "" : "s"}{list.truncated ? " (limit reached)" : ""}</span>
      <button type="button" onClick={onClose}>Close</button>
    </div>
  </Frame>;
}
