import {
  type ClipboardEvent,
  type CompositionEvent,
  type FormEvent,
  type KeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  type UIEvent,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type {
  CaretPosition,
  ClipboardPayload,
  DocumentMapSnapshot,
  DocumentSummary,
  FindResult,
  LanguageDescriptor,
  LineChange,
  MarkdownPreviewSource,
  RectangularSelection,
  ReplaceAllResult,
  SearchOptions,
  SearchRequest,
  Selection,
  SyntaxToken,
  TextEncoding,
  ViewportCursor,
  ViewportSnapshot,
} from "./contracts";
import { Icon, IconButton } from "./icons";
import {
  executeCommand, getDocumentMap, getMarkdownPreviewSource, getViewport, getViewportAt, getViewportLine,
} from "./ipc";
import { MarkdownPreview } from "./MarkdownPreview";

const VIEWPORT_BYTES = 32 * 1024;
export const READ_ONLY_MESSAGE = "Read-only: switch to Edit mode to change this document";
const LINE_HEIGHT = 22;
const OVERSCAN_LINES = 8;
const MAX_PAGE_HISTORY = 256;
const AUTO_SCROLL_EDGE = 36;
const MAP_ROW_HEIGHT = 4;
const MAX_HIGHLIGHTS_PER_LINE = 200;
const LINE_CHANGE_LABELS: Record<LineChange, string> = {
  unsaved: "Unsaved change",
  saved: "Changed and saved in this session",
  unsavedDeletion: "Unsaved deletion",
  savedDeletion: "Deleted and saved in this session",
};
const textEncoder = new TextEncoder();

/** Option copies a dragged selection on Apple platforms; Control does elsewhere. */
function copyModifierHeld(event: { altKey: boolean; ctrlKey: boolean }): boolean {
  return /Mac|iPhone|iPad/.test(navigator.platform) ? event.altKey : event.ctrlKey;
}
const FIRST_CURSOR: ViewportCursor = {
  byteOffset: "0",
  line: "1",
  lineContinuation: false,
};

function utf16IndexForByte(text: string, sourceByteWidths: number[], byteOffset: number): number {
  let bytes = 0;
  let utf16 = 0;
  let scalarIndex = 0;
  for (const scalar of text) {
    const width = sourceByteWidths[scalarIndex] ?? textEncoder.encode(scalar).length;
    if (bytes + width > byteOffset) break;
    bytes += width;
    utf16 += scalar.length;
    scalarIndex += 1;
  }
  return utf16;
}

function utf16IndexForScalarColumn(text: string, column: bigint): number {
  let utf16 = 0;
  let scalarColumn = 0n;
  for (const scalar of text) {
    if (scalarColumn >= column) break;
    utf16 += scalar.length;
    scalarColumn += 1n;
  }
  return utf16;
}

function scalarColumnForByte(
  text: string,
  sourceByteWidths: number[],
  byteOffset: number,
): string {
  const utf16 = utf16IndexForByte(text, sourceByteWidths, byteOffset);
  return String(Array.from(text.slice(0, utf16)).length);
}

function byteOffsetForClientX(
  text: string,
  sourceByteWidths: number[],
  clientX: number,
  element: HTMLElement,
): number {
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");
  if (!context) return 0;
  context.font = getComputedStyle(element).font;
  const localX = Math.max(0, clientX - element.getBoundingClientRect().left);
  let width = 0;
  let sourceBytes = 0;
  let scalarIndex = 0;
  for (const scalar of text) {
    const scalarWidth = context.measureText(scalar === "\t" ? "    " : scalar).width;
    if (localX < width + scalarWidth / 2) break;
    width += scalarWidth;
    sourceBytes += sourceByteWidths[scalarIndex] ?? textEncoder.encode(scalar).length;
    scalarIndex += 1;
  }
  return sourceBytes;
}

function scalarColumnForClientX(text: string, clientX: number, element: HTMLElement): string {
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");
  if (!context) return "0";
  context.font = getComputedStyle(element).font;
  const localX = Math.max(0, clientX - element.getBoundingClientRect().left);
  let width = 0;
  let column = 0;
  for (const scalar of text) {
    const scalarWidth = context.measureText(scalar === "\t" ? "    " : scalar).width;
    if (localX < width + scalarWidth / 2) return String(column);
    width += scalarWidth;
    column += 1;
  }
  const spaceWidth = Math.max(1, context.measureText(" ").width);
  return String(column + Math.max(0, Math.round((localX - width) / spaceWidth)));
}

function viewportContainsByte(snapshot: ViewportSnapshot, byteOffset: bigint): boolean {
  return snapshot.lines.some((line) => {
    const start = BigInt(line.startByte);
    const textEnd = start + BigInt(
      line.sourceByteWidths.reduce((total, width) => total + width, 0),
    );
    return byteOffset >= start && byteOffset <= textEnd;
  });
}

function renderLineText(
  text: string,
  selectionRanges: Array<{ start: number; end: number }>,
  caretIndices: number[],
  syntaxRanges: Array<{ start: number; end: number; token: SyntaxToken }>,
  bracketRanges: Array<{ start: number; end: number }>,
  dropIndex?: number,
  highlightRanges: Array<{ start: number; end: number }> = [],
): ReactNode[] {
  const boundaries = [...new Set([
    0,
    text.length,
    ...selectionRanges.flatMap((range) => [range.start, range.end]),
    ...highlightRanges.flatMap((range) => [range.start, range.end]),
    ...syntaxRanges.flatMap((range) => [range.start, range.end]),
    ...bracketRanges.flatMap((range) => [range.start, range.end]),
    ...caretIndices,
    ...(dropIndex === undefined ? [] : [dropIndex]),
  ])].sort((left, right) => left - right);
  const output: ReactNode[] = [];
  for (let index = 0; index < boundaries.length; index += 1) {
    const start = boundaries[index];
    caretIndices.forEach((caretIndex, caretOrdinal) => {
      if (start === caretIndex) {
        output.push(<i className="caret" key={`caret-${start}-${caretOrdinal}`} />);
      }
    });
    if (start === dropIndex) output.push(<i className="drop-caret" key={`drop-${start}`} />);
    const end = boundaries[index + 1];
    if (end === undefined || end === start) continue;
    const segment = text.slice(start, end);
    const selected = selectionRanges.some((range) => start >= range.start && end <= range.end);
    const syntax = syntaxRanges
      .filter((range) => start >= range.start && end <= range.end)
      .sort((left, right) => (left.end - left.start) - (right.end - right.start))[0];
    const bracket = bracketRanges.some((range) => start >= range.start && end <= range.end);
    const highlighted = highlightRanges.some((range) => start >= range.start && end <= range.end);
    const className = [
      selected ? "selection" : syntax ? `syntax syntax-${syntax.token}` : "",
      bracket ? "bracket-match" : "",
      highlighted && !selected ? "find-highlight" : "",
    ].filter(Boolean).join(" ");
    output.push(className
      ? <span className={className} key={`text-${start}`}>{segment}</span>
      : segment);
  }
  if (output.length === 0) output.push(" ");
  return output;
}

/** UTF-16 ranges of every visible occurrence of the find query in one rendered line. */
function findHighlights(text: string, query: string, regex: boolean, caseSensitive: boolean): Array<{ start: number; end: number }> {
  if (!query || !text) return [];
  const ranges: Array<{ start: number; end: number }> = [];
  if (regex) {
    let pattern: RegExp;
    try { pattern = new RegExp(query, caseSensitive ? "gu" : "giu"); } catch { return []; }
    for (const match of text.matchAll(pattern)) {
      if (match[0].length > 0) ranges.push({ start: match.index, end: match.index + match[0].length });
      if (ranges.length >= MAX_HIGHLIGHTS_PER_LINE) break;
    }
    return ranges;
  }
  const haystack = caseSensitive ? text : text.toLocaleLowerCase();
  const needle = caseSensitive ? query : query.toLocaleLowerCase();
  if (haystack.length !== text.length) return [];
  for (let from = haystack.indexOf(needle); from >= 0 && ranges.length < MAX_HIGHLIGHTS_PER_LINE;
    from = haystack.indexOf(needle, from + needle.length)) {
    ranges.push({ start: from, end: from + needle.length });
  }
  return ranges;
}

interface DragSelection {
  pointerId: number;
  selections: Selection[];
  targetIndex: number;
  rectangularSelection?: RectangularSelection;
  move?: {
    startByte: string;
    endByte: string;
    pressByte: string;
    pressX: number;
    pressY: number;
    destinationByte: string;
    started: boolean;
    validDrop: boolean;
  };
}

interface EditorViewportProps {
  document: DocumentSummary;
  languages?: LanguageDescriptor[];
  onDocumentChange(document: DocumentSummary): void;
  onStatus(message: string): void;
  viewId?: string;
  revealOffset?: string;
  lineHeight?: number;
  /** False for a view in an inactive pane, which must not take focus. */
  autoFocus?: boolean;
  /** Whether the line change indicator lane is shown for this document. */
  showLineChanges?: boolean;
  onToggleLineChanges?(): void;
  /** The latest Search panel action; only the focused editor receives one. */
  searchRequest?: SearchRequest;
  /** Reports the caret position whenever it changes; `null` when it is unknown. */
  onCaret?(position: CaretPosition | null): void;
  /** Switches the document between Edit and Read-Only mode. */
  onToggleReadOnly?(): void;
}

export function EditorViewport({
  document,
  languages = [
    { id: "plaintext", displayName: "Plain Text" },
    { id: "json", displayName: "JSON" },
    { id: "markdown", displayName: "Markdown" },
    { id: "python", displayName: "Python" },
  ],
  onDocumentChange,
  onStatus,
  viewId,
  revealOffset,
  lineHeight = LINE_HEIGHT,
  autoFocus = true,
  showLineChanges = true,
  onToggleLineChanges,
  searchRequest,
  onCaret,
  onToggleReadOnly,
}: EditorViewportProps) {
  const rowHeight = Math.max(12, Math.min(100, lineHeight));
  // Several panes can show views at once, so element IDs are per view.
  const viewDomId = useId();
  const instructionsId = `${viewDomId}-instructions`;
  const linesId = `${viewDomId}-lines`;
  const [snapshot, setSnapshot] = useState<ViewportSnapshot | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(500);
  const [compositionText, setCompositionText] = useState("");
  const [overwrite, setOverwrite] = useState(false);
  const [wordWrap, setWordWrap] = useState(false);
  const [showDocumentMap, setShowDocumentMap] = useState(true);
  const [documentMap, setDocumentMap] = useState<DocumentMapSnapshot | null>(null);
  const [markdownMode, setMarkdownMode] = useState<"edit" | "view">("edit");
  const [previewSource, setPreviewSource] = useState<MarkdownPreviewSource | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [mapHeight, setMapHeight] = useState(800);
  const [findOpen, setFindOpen] = useState(false);
  const [findQuery, setFindQuery] = useState("");
  const [replacement, setReplacement] = useState("");
  const [regexSearch, setRegexSearch] = useState(false);
  const [caseSensitive, setCaseSensitive] = useState(true);
  const [collapsedFolds, setCollapsedFolds] = useState<string[]>([]);
  const [currentFindMatch, setCurrentFindMatch] = useState<Selection | null>(null);
  const [currentCursor, setCurrentCursor] = useState<ViewportCursor>(FIRST_CURSOR);
  const [pageHistory, setPageHistory] = useState<ViewportCursor[]>([]);
  const [dropTargetByte, setDropTargetByte] = useState<string | null>(null);
  const [highlightMatches, setHighlightMatches] = useState(false);
  const [columnMode, setColumnMode] = useState(false);
  const [goToOpen, setGoToOpen] = useState(false);
  const [goToLine, setGoToLine] = useState("");
  const snapshotRef = useRef<ViewportSnapshot | null>(null);
  snapshotRef.current = snapshot;
  const surfaceRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const dragSelectionRef = useRef<DragSelection | null>(null);
  const composingRef = useRef(false);
  const suppressedCompositionRef = useRef<string | null>(null);
  const dragPageLoadRef = useRef(false);
  const mapNavigationGenerationRef = useRef(0);
  const loadGenerationRef = useRef(0);

  const load = useCallback(
    async (cursor: ViewportCursor = FIRST_CURSOR, resetScroll = true) => {
      const generation = ++loadGenerationRef.current;
      const next = await getViewport({
        documentId: document.documentId,
        startByte: cursor.byteOffset,
        startLine: cursor.line,
        lineContinuation: cursor.lineContinuation,
        maxBytes: VIEWPORT_BYTES,
      });
      if (generation !== loadGenerationRef.current) return next;
      setSnapshot(next);
      setCurrentCursor(cursor);
      if (resetScroll) {
        setScrollTop(0);
        if (surfaceRef.current) surfaceRef.current.scrollTop = 0;
      }
      return next;
    },
    [document.documentId],
  );

  useEffect(() => {
    let cancelled = false;
    setPageHistory([]);
    void load(FIRST_CURSOR).then(async (initial) => {
      const activeByte = BigInt(initial.selections[0]?.activeByte ?? "0");
      if (cancelled || viewportContainsByte(initial, activeByte)) return;
      const revealed = await getViewportAt({
        documentId: document.documentId,
        byteOffset: activeByte.toString(),
        maxBytes: VIEWPORT_BYTES,
      });
      if (cancelled) return;
      const first = revealed.lines[0];
      setSnapshot(revealed);
      setCurrentCursor({
        byteOffset: first?.startByte ?? activeByte.toString(),
        line: first?.line ?? "1",
        lineContinuation: first?.continuation ?? false,
      });
      setScrollTop(0);
      if (surfaceRef.current) surfaceRef.current.scrollTop = 0;
    }).catch((error: unknown) => {
      if (!cancelled) {
        onStatus(error instanceof Error ? error.message : "Viewport unavailable");
      }
    });
    return () => {
      cancelled = true;
    };
  }, [load, onStatus]);

  useEffect(() => {
    if (revealOffset === undefined) return;
    void getViewportAt({
      documentId: document.documentId,
      byteOffset: revealOffset,
      maxBytes: VIEWPORT_BYTES,
    }).then((revealed) => {
      const first = revealed.lines[0];
      setSnapshot(revealed);
      setCurrentCursor({
        byteOffset: first?.startByte ?? revealOffset,
        line: first?.line ?? "1",
        lineContinuation: first?.continuation ?? false,
      });
      setPageHistory([]);
      setScrollTop(0);
      if (surfaceRef.current) surfaceRef.current.scrollTop = 0;
    }).catch((error: unknown) => {
      onStatus(error instanceof Error ? error.message : "Could not reveal search result");
    });
  }, [document.documentId, onStatus, revealOffset]);

  useEffect(() => {
    setCurrentFindMatch(null);
  }, [caseSensitive, document.documentId, findQuery, regexSearch]);

  useEffect(() => {
    setCollapsedFolds([]);
  }, [document.documentId]);

  useEffect(() => {
    setMarkdownMode("edit");
    setPreviewSource(null);
    setPreviewError(null);
  }, [document.documentId]);

  useEffect(() => {
    if (snapshot?.syntax?.languageId && snapshot.syntax.languageId !== "markdown") {
      setMarkdownMode("edit");
    }
  }, [snapshot?.syntax?.languageId]);

  useEffect(() => {
    if (markdownMode !== "view") return;
    let cancelled = false;
    setPreviewError(null);
    const timer = window.setTimeout(() => {
      void getMarkdownPreviewSource(document.documentId).then((source) => {
        if (!cancelled) {
          setPreviewSource(source);
          setPreviewError(null);
        }
      }).catch((error: unknown) => {
        if (!cancelled) setPreviewError(error instanceof Error ? error.message : "Markdown preview unavailable");
      });
    }, 120);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [document.documentId, document.revision, markdownMode]);

  useEffect(() => {
    if (!showDocumentMap) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void getDocumentMap(document.documentId).then((next) => {
        if (!cancelled) setDocumentMap(next);
      }).catch((error: unknown) => {
        if (!cancelled) onStatus(error instanceof Error ? error.message : "Document map unavailable");
      });
    }, 200);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [document.dirty, document.documentId, document.revision, onStatus, showDocumentMap]);

  useEffect(() => {
    const surface = surfaceRef.current;
    if (!surface) return;
    setViewportHeight(surface.clientHeight);
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setViewportHeight(entry.contentRect.height);
    });
    observer.observe(surface);
    return () => observer.disconnect();
  }, [document.documentId, markdownMode, snapshot?.documentId]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;
    if (map.clientHeight > 0) setMapHeight(map.clientHeight);
    const observer = new ResizeObserver(([entry]) => {
      if (entry && entry.contentRect.height > 0) setMapHeight(entry.contentRect.height);
    });
    observer.observe(map);
    return () => observer.disconnect();
  }, [showDocumentMap, snapshot?.documentId]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || map.clientHeight <= 0 || !snapshot || !showDocumentMap) return;
    const complete = !snapshot.next && snapshot.lines[0]?.startByte === "0";
    const lineCount = documentMap?.documentId === document.documentId
      && documentMap.revision === snapshot.revision ? documentMap.totalLines : null;
    if (lineCount === null && !complete) return;
    const row = Math.min(snapshot.lines.length - 1, Math.floor(scrollTop / rowHeight));
    const top = (Number(snapshot.lines[row]?.line ?? "1") - 1) * MAP_ROW_HEIGHT;
    const height = viewportHeight / rowHeight * MAP_ROW_HEIGHT;
    if (top < map.scrollTop) map.scrollTop = top;
    else if (top + height > map.scrollTop + map.clientHeight) {
      map.scrollTop = top + height - map.clientHeight;
    }
  }, [document.documentId, documentMap, rowHeight, scrollTop, showDocumentMap, snapshot, viewportHeight]);

  useEffect(() => {
    if (markdownMode === "view" || viewId === "secondary" || !autoFocus
      || snapshot?.documentId !== document.documentId) return;
    inputRef.current?.focus();
  }, [autoFocus, document.documentId, markdownMode, snapshot?.documentId, viewId]);

  const scrollCaretIntoView = useCallback((active: bigint, current: ViewportSnapshot) => {
    const surface = surfaceRef.current;
    if (!surface) return;
    const targetIndex = current.lines.findIndex((line) => {
      const start = BigInt(line.startByte);
      const textEnd = start + BigInt(
        line.sourceByteWidths.reduce((total, width) => total + width, 0),
      );
      return active >= start && active <= textEnd;
    });
    if (targetIndex < 0) return;
    const targetLine = current.lines[targetIndex];
    if (!targetLine) return;
    let displayedIndex = 0;
    for (let index = 0; index < targetIndex; index += 1) {
      const line = current.lines[index];
      if (!line) continue;
      const lineNumber = BigInt(line.line);
      const hidden = current.syntax?.folds.some((fold) => (
        collapsedFolds.includes(fold.startByte)
        && lineNumber > BigInt(fold.startLine)
        && lineNumber <= BigInt(fold.endLine)
      ));
      if (!hidden) displayedIndex += 1;
    }
    if (wordWrap) {
      window.requestAnimationFrame(() => {
        const row = surface.querySelector<HTMLElement>(`[data-line-start="${targetLine.startByte}"]`);
        if (!row) return;
        const rowTop = row.offsetTop;
        const rowBottom = rowTop + row.offsetHeight;
        if (rowTop < surface.scrollTop) surface.scrollTop = rowTop;
        else if (rowBottom > surface.scrollTop + surface.clientHeight) {
          surface.scrollTop = rowBottom - surface.clientHeight;
        }
        setScrollTop(surface.scrollTop);
      });
      return;
    }
    const rowTop = displayedIndex * rowHeight;
    const rowBottom = rowTop + rowHeight;
    let nextScrollTop = surface.scrollTop;
    if (rowTop < surface.scrollTop) nextScrollTop = rowTop;
    else if (rowBottom > surface.scrollTop + surface.clientHeight) {
      nextScrollTop = rowBottom - surface.clientHeight;
    }
    if (nextScrollTop !== surface.scrollTop) surface.scrollTop = nextScrollTop;
    setScrollTop(nextScrollTop);
  }, [collapsedFolds, rowHeight, wordWrap]);

  const refresh = useCallback(async () => {
    const refreshed = await load(currentCursor, false);
    const active = BigInt(refreshed.selections[0]?.activeByte ?? "0");
    if (viewportContainsByte(refreshed, active)) {
      scrollCaretIntoView(active, refreshed);
    } else {
      const revealed = await getViewportAt({
        documentId: document.documentId,
        byteOffset: active.toString(),
        maxBytes: VIEWPORT_BYTES,
      });
      const first = revealed.lines[0];
      setSnapshot(revealed);
      setCurrentCursor({
        byteOffset: first?.startByte ?? active.toString(),
        line: first?.line ?? "1",
        lineContinuation: first?.continuation ?? false,
      });
      setScrollTop(0);
      if (surfaceRef.current) surfaceRef.current.scrollTop = 0;
    }
    inputRef.current?.focus();
  }, [currentCursor, document.documentId, load, scrollCaretIntoView]);

  // Another view of the same document (a second pane) advanced the
  // revision: reload this view in place so text and line markers stay current.
  useEffect(() => {
    const timer = window.setTimeout(() => {
      const current = snapshotRef.current;
      if (!current || current.documentId !== document.documentId || current.revision === document.revision) return;
      void load(currentCursor, false).catch(() => undefined);
    }, 80);
    return () => window.clearTimeout(timer);
  }, [currentCursor, document.documentId, document.revision, load]);

  // Saving keeps the revision but turns unsaved line markers into saved
  // ones, so the visible lines are fetched again when the document is clean.
  const savedStateRef = useRef({ documentId: document.documentId, dirty: document.dirty });
  useEffect(() => {
    const previous = savedStateRef.current;
    savedStateRef.current = { documentId: document.documentId, dirty: document.dirty };
    if (previous.documentId !== document.documentId || !previous.dirty || document.dirty) return;
    void load(currentCursor, false).catch(() => undefined);
  }, [currentCursor, document.dirty, document.documentId, load]);

  const readOnly = document.readOnly === true;
  /** Says why nothing happened when an edit is attempted in Read-Only mode. */
  const refuseEdit = useCallback(() => {
    if (!readOnly) return false;
    onStatus(READ_ONLY_MESSAGE);
    return true;
  }, [onStatus, readOnly]);

  // Typed edits run strictly one after another. Each must name the revision
  // it follows, and a key pressed before the previous edit has returned would
  // otherwise name a stale one and be refused, dropping the character.
  const editChainRef = useRef<Promise<unknown>>(Promise.resolve());
  const editRevisionRef = useRef<string | null>(null);
  useEffect(() => { editRevisionRef.current = snapshot?.revision ?? null; }, [snapshot]);
  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;
  const queueEdit = useCallback(
    (id: "editor.insertText" | "editor.deleteBackward", details: Record<string, unknown>) => {
      const run = async () => {
        const revision = editRevisionRef.current;
        if (revision === null) return false;
        const updated = await executeCommand<DocumentSummary>({
          id,
          context: { windowId: "main", documentId: document.documentId },
          arguments: { expectedRevision: revision, ...details },
        });
        editRevisionRef.current = updated.revision;
        onDocumentChange(updated);
        await refreshRef.current();
        return true;
      };
      const result = editChainRef.current.then(run, run);
      editChainRef.current = result.catch(() => undefined);
      return result;
    },
    [document.documentId, onDocumentChange],
  );

  const insertText = useCallback(
    async (text: string) => {
      if (refuseEdit()) return;
      try {
        if (await queueEdit("editor.insertText", { text, overwrite })) onStatus("Edited");
      } catch (error: unknown) {
        onStatus(error instanceof Error ? error.message : "Could not insert text");
      }
    },
    [onStatus, overwrite, queueEdit, refuseEdit],
  );

  const transferSelection = useCallback(
    async (cut: boolean) => {
      if (cut && refuseEdit()) return;
      try {
        const payload = await executeCommand<ClipboardPayload>({
          id: "editor.selectionText",
          context: { windowId: "main", documentId: document.documentId },
          arguments: null,
        });
        if (payload.text.length === 0) {
          onStatus("No text selected");
          return;
        }
        await writeText(payload.text);
        if (cut) {
          const updated = await executeCommand<DocumentSummary>({
            id: "editor.deleteSelection",
            context: { windowId: "main", documentId: document.documentId },
            arguments: { expectedRevision: payload.revision },
          });
          onDocumentChange(updated);
          await refresh();
        }
        const qualifier = payload.hadInvalidUtf8 ? " with replacement characters" : "";
        onStatus(`${cut ? "Cut" : "Copied"}${qualifier}`);
      } catch (error: unknown) {
        onStatus(error instanceof Error ? error.message : "Clipboard operation failed");
      }
    },
    [document.documentId, onDocumentChange, onStatus, refresh, refuseEdit],
  );

  const runHistory = useCallback(
    async (command: "editor.undo" | "editor.redo") => {
      if (refuseEdit()) return;
      const updated = await executeCommand<DocumentSummary>({
        id: command,
        context: { windowId: "main", documentId: document.documentId },
        arguments: null,
      });
      onDocumentChange(updated);
      await refresh();
      onStatus(command === "editor.undo" ? "Undo" : "Redo");
    },
    [document.documentId, onDocumentChange, onStatus, refresh, refuseEdit],
  );

  const commitSelections = useCallback(
    async (selections: Selection[]) => {
      await executeCommand<Selection[]>({
        id: "editor.setSelections",
        context: { windowId: "main", documentId: document.documentId },
        arguments: { selections },
      });
      setSnapshot((current) => current ? { ...current, selections } : current);
    },
    [document.documentId],
  );

  const commitRectangularSelection = useCallback(
    async (selection: RectangularSelection) => {
      const committed = await executeCommand<RectangularSelection>({
        id: "editor.setRectangularSelection",
        context: { windowId: "main", documentId: document.documentId },
        arguments: {
          anchorByte: selection.anchorByte,
          activeByte: selection.activeByte,
          anchorColumn: selection.anchorColumn,
          activeColumn: selection.activeColumn,
        },
      });
      setSnapshot((current) => current
        ? {
            ...current,
            selections: [{ anchorByte: committed.activeByte, activeByte: committed.activeByte }],
            rectangularSelection: committed,
          }
        : current);
    },
    [document.documentId],
  );

  const revealSelections = useCallback(
    async (selections: Selection[]) => {
      const active = BigInt(selections[0]?.activeByte ?? "0");
      const caretIsVisible = snapshot ? viewportContainsByte(snapshot, active) : false;
      if (caretIsVisible) {
        setSnapshot((current) => current ? { ...current, selections } : current);
        if (snapshot) scrollCaretIntoView(active, snapshot);
        return;
      }
      const revealed = await getViewportAt({
        documentId: document.documentId,
        byteOffset: active.toString(),
        maxBytes: VIEWPORT_BYTES,
      });
      const first = revealed.lines[0];
      setPageHistory((history) => [...history.slice(-(MAX_PAGE_HISTORY - 1)), currentCursor]);
      setCurrentCursor({
        byteOffset: first?.startByte ?? active.toString(),
        line: first?.line ?? "1",
        lineContinuation: first?.continuation ?? false,
      });
      setSnapshot({ ...revealed, selections });
      setScrollTop(0);
      if (surfaceRef.current) surfaceRef.current.scrollTop = 0;
    },
    [currentCursor, document.documentId, scrollCaretIntoView, snapshot],
  );

  useEffect(() => {
    const finishDrag = (event: PointerEvent) => {
      const drag = dragSelectionRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      dragSelectionRef.current = null;
      setDropTargetByte(null);
      if (drag.move) {
        const move = drag.move;
        if (event.type === "pointercancel") return;
        if (!move.started) {
          void commitSelections([{ anchorByte: move.pressByte, activeByte: move.pressByte }])
            .catch((error: unknown) => onStatus(error instanceof Error ? error.message : "Could not place caret"));
          return;
        }
        if (!move.validDrop || !snapshot) return;
        const copy = copyModifierHeld(event);
        void executeCommand<DocumentSummary>({
          id: "editor.moveSelectedText",
          context: { windowId: "main", documentId: document.documentId },
          arguments: {
            expectedRevision: snapshot.revision,
            startByte: move.startByte,
            endByte: move.endByte,
            destinationByte: move.destinationByte,
            copy,
          },
        }).then(async (updated) => {
          onDocumentChange(updated);
          await refresh();
          onStatus(copy ? "Copied selected text" : "Moved selected text");
        }).catch((error: unknown) => {
          onStatus(error instanceof Error ? error.message : "Could not move selected text");
          void refresh();
        });
        return;
      }
      if (event.type === "pointercancel") return;
      const commit = drag.rectangularSelection
        ? commitRectangularSelection(drag.rectangularSelection)
        : commitSelections(drag.selections);
      void commit.catch((error: unknown) => {
        onStatus(error instanceof Error ? error.message : "Could not update selection");
      });
    };
    const trackOutside = (event: PointerEvent) => {
      const move = dragSelectionRef.current?.move;
      if (!move?.started) return;
      if (event.target instanceof Node && !surfaceRef.current?.contains(event.target)) {
        move.validDrop = false;
        setDropTargetByte(null);
      }
    };
    window.addEventListener("pointermove", trackOutside);
    window.addEventListener("pointerup", finishDrag);
    window.addEventListener("pointercancel", finishDrag);
    return () => {
      window.removeEventListener("pointermove", trackOutside);
      window.removeEventListener("pointerup", finishDrag);
      window.removeEventListener("pointercancel", finishDrag);
    };
  }, [commitRectangularSelection, commitSelections, document.documentId, onDocumentChange, onStatus, refresh, snapshot]);

  const moveHorizontal = useCallback(
    async (direction: "left" | "right", extend: boolean) => {
      const selections = await executeCommand<Selection[]>({
        id: "editor.moveHorizontal",
        context: { windowId: "main", documentId: document.documentId },
        arguments: { direction, extend },
      });
      await revealSelections(selections);
    },
    [document.documentId, revealSelections],
  );

  const moveVertical = useCallback(
    async (direction: "up" | "down", extend: boolean) => {
      const selections = await executeCommand<Selection[]>({
        id: "editor.moveVertical",
        context: { windowId: "main", documentId: document.documentId },
        arguments: { direction, extend },
      });
      await revealSelections(selections);
    },
    [document.documentId, revealSelections],
  );

  const transformSelections = useCallback(
    async (transform: "uppercase" | "lowercase" | "trimWhitespace") => {
      if (!snapshot) return;
      try {
        const updated = await executeCommand<DocumentSummary>({
          id: "editor.transformSelections",
          context: { windowId: "main", documentId: document.documentId },
          arguments: { expectedRevision: snapshot.revision, transform },
        });
        onDocumentChange(updated);
        await refresh();
        onStatus(`Applied ${transform}`);
      } catch (error: unknown) {
        onStatus(error instanceof Error ? error.message : "Text transform failed");
      }
    },
    [document.documentId, onDocumentChange, onStatus, refresh, snapshot],
  );

  const toggleBookmark = useCallback(async () => {
    await executeCommand({
      id: "editor.toggleBookmark",
      context: { windowId: "main", documentId: document.documentId },
      arguments: null,
    });
    await refresh();
    onStatus("Bookmark toggled");
  }, [document.documentId, onStatus, refresh]);

  const moveToBookmark = useCallback(
    async (direction: "previous" | "next") => {
      const selections = await executeCommand<Selection[]>({
        id: "editor.moveToBookmark",
        context: { windowId: "main", documentId: document.documentId },
        arguments: { direction },
      });
      await revealSelections(selections);
      onStatus(`${direction === "next" ? "Next" : "Previous"} bookmark`);
    },
    [document.documentId, onStatus, revealSelections],
  );

  const clearBookmarks = useCallback(async () => {
    await executeCommand({
      id: "editor.clearBookmarks",
      context: { windowId: "main", documentId: document.documentId },
      arguments: null,
    });
    await refresh();
    onStatus("Bookmarks cleared");
  }, [document.documentId, onStatus, refresh]);

  const setEncoding = useCallback(
    async (encoding: TextEncoding) => {
      const updated = await executeCommand<DocumentSummary>({
        id: "editor.setEncoding",
        context: { windowId: "main", documentId: document.documentId },
        arguments: { encoding },
      });
      onDocumentChange(updated);
      await load(currentCursor);
      onStatus(encoding === "utf8" ? "Encoding: UTF-8" : "Encoding: Windows-1252");
    },
    [currentCursor, document.documentId, load, onDocumentChange, onStatus],
  );

  const setLanguage = useCallback(
    async (languageId: string) => {
      await executeCommand<string>({
        id: "language.set",
        context: { windowId: "main", documentId: document.documentId },
        arguments: { languageId },
      });
      setCollapsedFolds([]);
      // The workbench offers language-specific tools from the document summary.
      onDocumentChange({ ...document, languageId });
      await load(currentCursor);
      onStatus(`Language: ${languages.find((language) => language.id === languageId)?.displayName ?? languageId}`);
    },
    [currentCursor, document, languages, load, onDocumentChange, onStatus],
  );

  const searchOptions = useCallback((): SearchOptions => ({
    mode: regexSearch ? "regex" : "literal",
    caseSensitive,
  }), [caseSensitive, regexSearch]);

  const runFind = useCallback(async (direction: "previous" | "next") => {
    if (!snapshot || findQuery.length === 0) {
      onStatus("Enter text to find");
      return;
    }
    const primary = snapshot.selections[0] ?? { anchorByte: "0", activeByte: "0" };
    const anchor = BigInt(primary.anchorByte);
    const active = BigInt(primary.activeByte);
    const startByte = direction === "next"
      ? (anchor > active ? anchor : active)
      : (anchor < active ? anchor : active);
    try {
      const result = await executeCommand<FindResult>({
        id: "search.find",
        context: { windowId: "main", documentId: document.documentId },
        arguments: {
          query: findQuery,
          options: searchOptions(),
          startByte: startByte.toString(),
          direction,
          wrap: true,
        },
      });
      if (!result.selection) {
        setCurrentFindMatch(null);
        onStatus(`No matches for “${findQuery}”`);
        return;
      }
      setCurrentFindMatch(result.selection);
      await revealSelections([result.selection]);
      onStatus(`${result.wrapped ? "Wrapped to " : ""}${direction} match`);
    } catch (error: unknown) {
      onStatus(error instanceof Error ? error.message : "Search failed");
    }
  }, [document.documentId, findQuery, onStatus, revealSelections, searchOptions, snapshot]);

  const replaceCurrent = useCallback(async () => {
    if (!snapshot) return;
    const primary = snapshot.selections[0];
    if (
      !primary
      || !currentFindMatch
      || primary.anchorByte !== currentFindMatch.anchorByte
      || primary.activeByte !== currentFindMatch.activeByte
    ) {
      await runFind("next");
      return;
    }
    const start = BigInt(primary.anchorByte) < BigInt(primary.activeByte)
      ? primary.anchorByte
      : primary.activeByte;
    const end = BigInt(primary.anchorByte) > BigInt(primary.activeByte)
      ? primary.anchorByte
      : primary.activeByte;
    try {
      const updated = await executeCommand<DocumentSummary>({
        id: "editor.replace",
        context: { windowId: "main", documentId: document.documentId },
        arguments: {
          expectedRevision: snapshot.revision,
          startByte: start,
          endByte: end,
          text: replacement,
        },
      });
      onDocumentChange(updated);
      setCurrentFindMatch(null);
      await refresh();
      onStatus("Replaced match");
    } catch (error: unknown) {
      onStatus(error instanceof Error ? error.message : "Replace failed");
    }
  }, [currentFindMatch, document.documentId, onDocumentChange, onStatus, refresh, replacement, runFind, snapshot]);

  const replaceAll = useCallback(async () => {
    if (!snapshot || findQuery.length === 0) {
      onStatus("Enter text to replace");
      return;
    }
    try {
      const result = await executeCommand<ReplaceAllResult>({
        id: "search.replaceAll",
        context: { windowId: "main", documentId: document.documentId },
        arguments: {
          expectedRevision: snapshot.revision,
          query: findQuery,
          replacement,
          options: searchOptions(),
        },
      });
      onDocumentChange(result.document);
      setCurrentFindMatch(null);
      await refresh();
      onStatus(`Replaced ${result.replacements} match${result.replacements === 1 ? "" : "es"}`);
    } catch (error: unknown) {
      onStatus(error instanceof Error ? error.message : "Replace all failed");
    }
  }, [document.documentId, findQuery, onDocumentChange, onStatus, refresh, replacement, searchOptions, snapshot]);

  const runGoToLine = () => {
    const line = goToLine.trim();
    if (!/^[1-9]\d{0,18}$/.test(line)) { onStatus("Enter a line number"); return; }
    void getViewportLine({ documentId: document.documentId, line, maxBytes: VIEWPORT_BYTES }).then(async (revealed) => {
      const first = revealed.lines[0];
      const caret = first?.startByte ?? "0";
      setPageHistory((history) => [...history.slice(-(MAX_PAGE_HISTORY - 1)), currentCursor]);
      setCurrentCursor({ byteOffset: caret, line: first?.line ?? "1", lineContinuation: first?.continuation ?? false });
      setSnapshot(revealed);
      setScrollTop(0);
      if (surfaceRef.current) surfaceRef.current.scrollTop = 0;
      await commitSelections([{ anchorByte: caret, activeByte: caret }]);
      setGoToOpen(false);
      inputRef.current?.focus({ preventScroll: true });
      onStatus(first?.line === line ? `Line ${line}` : `Line ${line} is past the end; showing line ${first?.line ?? "1"}`);
    }).catch((error: unknown) => onStatus(error instanceof Error ? error.message : "Could not go to line"));
  };

  // A Search panel action: adopt its query and options, then run it once the
  // find functions have picked them up.
  const handledSearch = useRef(searchRequest?.id ?? 0);
  const pendingSearch = useRef<SearchRequest["action"] | null>(null);
  const [searchTick, setSearchTick] = useState(0);
  useEffect(() => {
    if (!searchRequest || searchRequest.id === handledSearch.current) return;
    handledSearch.current = searchRequest.id;
    setFindQuery(searchRequest.query);
    setReplacement(searchRequest.replacement);
    setRegexSearch(searchRequest.regex);
    setCaseSensitive(searchRequest.caseSensitive);
    pendingSearch.current = searchRequest.action;
    setSearchTick((tick) => tick + 1);
  }, [searchRequest]);
  useEffect(() => {
    const action = pendingSearch.current;
    if (!action || !snapshot) return;
    pendingSearch.current = null;
    if (action === "replace") void replaceCurrent();
    else if (action === "replaceAll") void replaceAll();
    else void runFind(action);
    // Runs once per request; the functions above are current for this render.
  }, [searchTick, snapshot === null]);

  const loadNext = () => {
    if (!snapshot?.next) return;
    setPageHistory((history) => [...history.slice(-(MAX_PAGE_HISTORY - 1)), currentCursor]);
    void load(snapshot.next).catch((error: unknown) => {
      onStatus(error instanceof Error ? error.message : "Could not load next viewport");
    });
  };

  const loadPrevious = () => {
    const previous = pageHistory[pageHistory.length - 1];
    if (!previous) return;
    setPageHistory((history) => history.slice(0, -1));
    void load(previous).catch((error: unknown) => {
      onStatus(error instanceof Error ? error.message : "Could not load previous viewport");
    });
  };

  const loadDragPage = async (direction: "previous" | "next") => {
    if (!snapshot || dragPageLoadRef.current) return;
    const previous = pageHistory[pageHistory.length - 1];
    const cursor = direction === "next" ? snapshot.next : previous;
    if (!cursor) return;
    dragPageLoadRef.current = true;
    try {
      if (direction === "next") {
        setPageHistory((history) => [...history.slice(-(MAX_PAGE_HISTORY - 1)), currentCursor]);
      } else {
        setPageHistory((history) => history.slice(0, -1));
      }
      const loaded = await load(cursor);
      const boundaryLine = direction === "next"
        ? loaded.lines[0]
        : loaded.lines[loaded.lines.length - 1];
      const boundaryOffset = boundaryLine
        ? (BigInt(boundaryLine.startByte)
          + BigInt(boundaryLine.sourceByteWidths.reduce(
            (total, width) => total + width,
            0,
          ))).toString()
        : cursor.byteOffset;
      const drag = dragSelectionRef.current;
      if (drag && !drag.move) {
        const target = drag.selections[drag.targetIndex];
        if (target) {
          drag.selections[drag.targetIndex] = drag.rectangularSelection
            ? { anchorByte: boundaryOffset, activeByte: boundaryOffset }
            : { ...target, activeByte: boundaryOffset };
        }
        if (drag.rectangularSelection && boundaryLine) {
          const byteWithinLine = Number(BigInt(boundaryOffset) - BigInt(boundaryLine.startByte));
          drag.rectangularSelection = {
            ...drag.rectangularSelection,
            activeByte: boundaryOffset,
            activeLine: boundaryLine.line,
            activeColumn: scalarColumnForByte(
              boundaryLine.text,
              boundaryLine.sourceByteWidths,
              byteWithinLine,
            ),
          };
        }
        setSnapshot({
          ...loaded,
          selections: drag.selections,
          rectangularSelection: drag.rectangularSelection,
        });
      }
      if (direction === "previous") {
        requestAnimationFrame(() => {
          const surface = surfaceRef.current;
          if (!surface) return;
          surface.scrollTop = Math.max(0, surface.scrollHeight - surface.clientHeight);
          setScrollTop(surface.scrollTop);
        });
      }
    } catch (error: unknown) {
      onStatus(error instanceof Error ? error.message : "Could not continue drag selection");
    } finally {
      dragPageLoadRef.current = false;
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    const modifier = event.metaKey || event.ctrlKey;
    if (modifier && event.key.toLowerCase() === "f") {
      event.preventDefault();
      setFindOpen(true);
      return;
    }
    if (modifier && event.key.toLowerCase() === "h") {
      event.preventDefault();
      setFindOpen(true);
      return;
    }
    if (modifier && event.key.toLowerCase() === "g") {
      event.preventDefault();
      setGoToOpen(true);
      return;
    }
    if (modifier && event.key.toLowerCase() === "d") {
      event.preventDefault();
      if (!snapshot) return;
      void executeCommand<Selection>({
        id: "editor.selectWordAt",
        context: { windowId: "main", documentId: document.documentId },
        arguments: {
          expectedRevision: snapshot.revision,
          byteOffset: snapshot.selections[0]?.activeByte ?? "0",
        },
      }).then((selected) => revealSelections([selected])).catch((error: unknown) => onStatus(
        error instanceof Error ? error.message : "Could not select word",
      ));
      return;
    }
    if (modifier && event.key.toLowerCase() === "z") {
      event.preventDefault();
      void runHistory(event.shiftKey ? "editor.redo" : "editor.undo");
      return;
    }
    if (modifier && event.key === "F2") {
      event.preventDefault();
      void toggleBookmark();
      return;
    }
    if (event.key === "F2") {
      event.preventDefault();
      void moveToBookmark(event.shiftKey ? "previous" : "next");
      return;
    }
    if (event.key === "Escape" && findOpen) {
      event.preventDefault();
      setFindOpen(false);
      return;
    }
    if (event.key === "Escape" && dragSelectionRef.current?.move) {
      event.preventDefault();
      dragSelectionRef.current = null;
      setDropTargetByte(null);
      onStatus("Text move canceled");
      return;
    }
    if (
      event.key === "Escape"
      && snapshot
      && (snapshot.selections.length > 1 || snapshot.rectangularSelection)
    ) {
      event.preventDefault();
      const primary = snapshot.selections[0];
      if (primary) void commitSelections([primary]);
      return;
    }
    if (
      composingRef.current
      || event.nativeEvent.isComposing
      || event.nativeEvent.keyCode === 229
      || modifier
      || event.altKey
    ) return;
    if (event.key === "Insert") {
      event.preventDefault();
      setOverwrite((enabled) => {
        onStatus(enabled ? "Insert mode" : "Overwrite mode");
        return !enabled;
      });
    } else if (event.key === "Enter") {
      event.preventDefault();
      void insertText("\n");
    } else if (event.key === "Tab") {
      event.preventDefault();
      void insertText("\t");
    } else if (event.key === "Backspace") {
      event.preventDefault();
      if (refuseEdit()) return;
      void queueEdit("editor.deleteBackward", {}).catch((error: unknown) => onStatus(
        error instanceof Error ? error.message : "Could not delete"));
    } else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      event.preventDefault();
      void moveHorizontal(event.key === "ArrowLeft" ? "left" : "right", event.shiftKey);
    } else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      void moveVertical(event.key === "ArrowUp" ? "up" : "down", event.shiftKey);
    } else if (event.key === "PageDown") {
      event.preventDefault();
      loadNext();
    } else if (event.key === "PageUp") {
      event.preventDefault();
      loadPrevious();
    } else if (event.key.length === 1) {
      event.preventDefault();
      void insertText(event.key);
    }
  };

  const onPaste = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    event.preventDefault();
    void insertText(event.clipboardData.getData("text/plain"));
  };

  const onCompositionEnd = (event: CompositionEvent<HTMLTextAreaElement>) => {
    composingRef.current = false;
    suppressedCompositionRef.current = event.data || null;
    setCompositionText("");
    event.currentTarget.value = "";
    if (event.data) void insertText(event.data);
  };

  const onInput = (event: FormEvent<HTMLTextAreaElement>) => {
    if (composingRef.current) return;
    const input = event.currentTarget;
    if (suppressedCompositionRef.current === input.value) {
      suppressedCompositionRef.current = null;
      input.value = "";
      return;
    }
    suppressedCompositionRef.current = null;
    const text = input.value;
    input.value = "";
    if (text) void insertText(text);
  };

  const onScroll = (event: UIEvent<HTMLDivElement>) => {
    setScrollTop(event.currentTarget.scrollTop);
  };

  // The caret is worked out below, once a viewport exists; this reports it
  // after each render in which it changed.
  const caretRef = useRef<CaretPosition | null>(null);
  const reportedCaretRef = useRef<string | null>(null);
  caretRef.current = null;
  useEffect(() => {
    if (!onCaret) return;
    const key = JSON.stringify(caretRef.current);
    if (key === reportedCaretRef.current) return;
    reportedCaretRef.current = key;
    onCaret(caretRef.current);
  });

  if (!snapshot) {
    return <div className="editor-loading" role="status">Loading bounded viewport…</div>;
  }

  const syntax = snapshot.syntax ?? {
    languageId: "plaintext",
    parsedBytes: 0,
    highlights: [],
    folds: [],
    symbols: [],
    bracketPairs: [],
    truncated: false,
    timedOut: false,
  };
  const isMarkdownView = syntax.languageId === "markdown" && markdownMode === "view";
  const currentPreview = previewSource?.documentId === document.documentId
    && previewSource.revision === document.revision ? previewSource : null;
  const currentMap = documentMap?.documentId === document.documentId
    && documentMap.revision === snapshot.revision
    ? documentMap : null;
  const mapSamples: DocumentMapSnapshot["samples"] = currentMap?.samples
    ?? snapshot.lines.map((line) => ({ byteOffset: line.startByte, line: line.line, text: line.text }));
  const mapTotalLines = currentMap?.totalLines
    ?? (!snapshot.next && snapshot.lines[0]?.startByte === "0"
      ? Number(snapshot.lines.at(-1)?.line ?? "1") : null);
  const activeFolds = syntax.folds.filter((fold) => collapsedFolds.includes(fold.startByte));
  const displayedLines = snapshot.lines
    .map((line, lineIndex) => ({ line, lineIndex }))
    .filter(({ line }) => !activeFolds.some((fold) => {
      const lineNumber = BigInt(line.line);
      return lineNumber > BigInt(fold.startLine) && lineNumber <= BigInt(fold.endLine);
    }));
  const visibleCount = Math.ceil(viewportHeight / rowHeight);
  const firstVisible = wordWrap
    ? 0
    : Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN_LINES);
  const lastVisible = wordWrap
    ? displayedLines.length
    : Math.min(displayedLines.length, firstVisible + visibleCount + OVERSCAN_LINES * 2);
  const visibleLines = displayedLines.slice(firstVisible, lastVisible);
  const mapPoint = (line: string, byteOffset: string) => mapTotalLines !== null
    ? Math.max(0, Number(line) - 1) * MAP_ROW_HEIGHT
    : Number(BigInt(byteOffset) * BigInt(mapHeight) / (BigInt(snapshot.documentByteLen) || 1n));
  const firstOnScreen = displayedLines[Math.min(displayedLines.length - 1, Math.floor(scrollTop / rowHeight))]?.line.line;
  const lastOnScreen = displayedLines[Math.min(displayedLines.length - 1,
    Math.floor((scrollTop + viewportHeight) / rowHeight))]?.line.line;
  const markerStart = mapPoint(firstOnScreen ?? snapshot.lines[0]?.line ?? "1",
    displayedLines[Math.min(displayedLines.length - 1, Math.floor(scrollTop / rowHeight))]?.line.startByte
      ?? snapshot.lines[0]?.startByte ?? "0");
  const markerEnd = mapPoint(lastOnScreen ?? snapshot.lines.at(-1)?.line ?? "1",
    displayedLines[Math.min(displayedLines.length - 1, Math.floor((scrollTop + viewportHeight) / rowHeight))]?.line.endByte
      ?? snapshot.lines.at(-1)?.endByte ?? "0");
  const markerHeight = mapTotalLines !== null
    ? Math.max(8, viewportHeight / rowHeight * MAP_ROW_HEIGHT)
    : Math.max(8, markerEnd - markerStart);
  const mapContentHeight = mapTotalLines !== null
    ? Math.max(mapHeight, mapTotalLines * MAP_ROW_HEIGHT + markerHeight)
    : mapHeight;
  const mapStep = mapTotalLines !== null ? 1
    : Math.max(1, Math.ceil(mapSamples.length * MAP_ROW_HEIGHT / Math.max(1, mapHeight)));
  let insideCodeBlock = false;
  const previewSamples = mapSamples.map((sample, index) => {
    const text = sample.text;
    let tone = "";
    if (syntax.languageId === "markdown") {
      if (/^\s*(```|~~~)/.test(text)) {
        tone = "code";
        insideCodeBlock = !insideCodeBlock;
      } else if (insideCodeBlock) tone = "code";
      else if (/^\s*#{1,6}\s/.test(text)) tone = "heading";
      else if (/^\s*>/.test(text)) tone = "quote";
      else if (/^\s*([-*+]\s|\d+\.\s)/.test(text)) tone = "bullet";
    } else if (/^\s*(#|\/\/)/.test(text)) tone = "comment";
    return { ...sample, tone, index };
  }).filter(({ index }) => index % mapStep === 0);
  const navigateMap = (position: number) => {
    const clamped = Math.max(0, Math.min(mapContentHeight, position));
    if (mapSamples.length === 0) return;
    const selected = mapSamples.reduce((nearest, sample) => {
      const samplePosition = mapPoint(sample.line, sample.byteOffset);
      return Math.abs(samplePosition - clamped) < Math.abs(mapPoint(nearest.line, nearest.byteOffset) - clamped)
        ? sample : nearest;
    });
    if (mapTotalLines !== null && !snapshot.next && snapshot.lines[0]?.startByte === "0") {
      const index = displayedLines.findIndex(({ line }) => Number(line.line) >= Number(selected.line));
      const surface = surfaceRef.current;
      if (index >= 0 && surface) {
        const nextTop = Math.max(0, index * rowHeight - viewportHeight / 2);
        surface.scrollTop = nextTop;
        setScrollTop(surface.scrollTop);
        inputRef.current?.focus({ preventScroll: true });
        return;
      }
    }
    const generation = ++mapNavigationGenerationRef.current;
    void getViewportAt({ documentId: document.documentId, byteOffset: selected.byteOffset, maxBytes: VIEWPORT_BYTES })
      .then((revealed) => {
        if (generation !== mapNavigationGenerationRef.current) return;
        const first = revealed.lines[0];
        setPageHistory((history) => [...history.slice(-(MAX_PAGE_HISTORY - 1)), currentCursor]);
        setCurrentCursor({ byteOffset: first?.startByte ?? selected.byteOffset, line: first?.line ?? "1",
          lineContinuation: first?.continuation ?? false });
        setSnapshot(revealed);
        setScrollTop(0);
        if (surfaceRef.current) surfaceRef.current.scrollTop = 0;
        inputRef.current?.focus({ preventScroll: true });
      }).catch((error: unknown) => onStatus(error instanceof Error ? error.message : "Could not navigate document map"));
  };
  const lineChangesAvailable = snapshot.lineChanges?.available ?? true;
  const lineChangesApproximate = snapshot.lineChanges?.approximate ?? false;
  const primarySelection = snapshot.selections[0] ?? { anchorByte: "0", activeByte: "0" };
  const activeByte = BigInt(primarySelection.activeByte);
  const matchedBracketOffsets = syntax.bracketPairs.flatMap((pair) => {
    const open = BigInt(pair.openByte);
    const close = BigInt(pair.closeByte);
    return activeByte === open
      || activeByte === open + 1n
      || activeByte === close
      || activeByte === close + 1n
      ? [open, close]
      : [];
  });
  const caretLine = snapshot.lines.find((line) => {
    const start = BigInt(line.startByte);
    const textEnd = start + BigInt(
      line.sourceByteWidths.reduce((total, width) => total + width, 0),
    );
    return activeByte >= start && activeByte <= textEnd;
  });
  const caretFold = caretLine
    ? syntax.folds.find((fold) => fold.startLine === caretLine.line)
    : undefined;
  const caretColumn = caretLine
    ? Array.from(caretLine.text.slice(
      0,
      utf16IndexForByte(
        caretLine.text,
        caretLine.sourceByteWidths,
        Number(activeByte - BigInt(caretLine.startByte)),
      ),
    )).length + 1
    : 1;
  const selectionBytes = snapshot.selections.reduce((total, selection) => {
    const anchor = BigInt(selection.anchorByte);
    const active = BigInt(selection.activeByte);
    return total + (anchor > active ? anchor - active : active - anchor);
  }, 0n);
  caretRef.current = caretLine ? {
    line: caretLine.line, column: caretColumn, carets: snapshot.selections.length,
    selectedBytes: selectionBytes.toString(),
  } : null;

  const accessibilityStatus = compositionText
    ? `Composing ${compositionText}`
    : `Line ${caretLine?.line ?? "unknown"}, column ${caretColumn}${
      snapshot.rectangularSelection
        ? `, rectangular selection across ${(
            BigInt(snapshot.rectangularSelection.anchorLine)
            - BigInt(snapshot.rectangularSelection.activeLine)
          < 0n
            ? BigInt(snapshot.rectangularSelection.activeLine)
              - BigInt(snapshot.rectangularSelection.anchorLine)
            : BigInt(snapshot.rectangularSelection.anchorLine)
              - BigInt(snapshot.rectangularSelection.activeLine)) + 1n} lines`
        : selectionBytes > 0 ? `, ${selectionBytes.toString()} bytes selected` : ""
    }${showLineChanges && lineChangesAvailable && caretLine?.change
      ? `, ${LINE_CHANGE_LABELS[caretLine.change].toLowerCase()}` : ""}`;

  const pointerOffset = (
    event: ReactPointerEvent<HTMLDivElement>,
    lineIndex: number,
  ): string => {
    const line = snapshot.lines[lineIndex];
    const code = event.currentTarget.querySelector("code");
    const relative = line && code
      ? byteOffsetForClientX(line.text, line.sourceByteWidths, event.clientX, code)
      : 0;
    return (BigInt(line?.startByte ?? "0") + BigInt(relative)).toString();
  };

  return (
    <div className={`editor-stage${readOnly ? " read-only" : ""}`}>
      <p className="screen-reader-only" id={instructionsId}>
        Text editor. Use arrow keys to move, Shift plus arrows to select, and Page Up or Page Down
        to move between bounded viewports. Double-click, or press Control or Command plus D, to
        select a word. Drag selected text to move it; hold Option on macOS or Control elsewhere
        while dropping to copy it. Hold Alt plus Shift while dragging for a rectangular selection.
      </p>
      <div className="screen-reader-only" role="status" aria-live="polite" aria-atomic="true">
        {accessibilityStatus}
      </div>
      {findOpen ? <div className="findbar" role="search" aria-label="Find and replace">
        <input
          autoFocus
          aria-label="Find text"
          value={findQuery}
          onChange={(event) => setFindQuery(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              void runFind(event.shiftKey ? "previous" : "next");
            } else if (event.key === "Escape") {
              setFindOpen(false);
              inputRef.current?.focus();
            }
          }}
          placeholder="Find"
        />
        <input
          aria-label="Replace text"
          value={replacement}
          onChange={(event) => setReplacement(event.currentTarget.value)}
          placeholder="Replace"
        />
        <IconButton icon="previous" label="Previous match" shortcut="Shift Enter" onClick={() => void runFind("previous")} />
        <IconButton icon="next" label="Next match" shortcut="Enter" onClick={() => void runFind("next")} />
        <IconButton icon="caseSensitive" label="Match case" pressed={caseSensitive} onClick={() => setCaseSensitive((enabled) => !enabled)} />
        <IconButton icon="regex" label="Regular expression" pressed={regexSearch} onClick={() => setRegexSearch((enabled) => !enabled)} />
        <IconButton icon="highlight" label="Highlight matches" pressed={highlightMatches} onClick={() => setHighlightMatches((enabled) => !enabled)} />
        <IconButton icon="replace" label="Replace" onClick={() => void replaceCurrent()} />
        <button type="button" onClick={() => void replaceAll()}>All</button>
        <IconButton icon="close" label="Close find" shortcut="Escape" onClick={() => setFindOpen(false)} />
      </div> : null}
      {goToOpen ? <div className="findbar gotobar" role="search" aria-label="Go to line">
        <input autoFocus aria-label="Line number" inputMode="numeric" placeholder="Line number" value={goToLine}
          onChange={(event) => setGoToLine(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") { event.preventDefault(); runGoToLine(); }
            if (event.key === "Escape") { setGoToOpen(false); inputRef.current?.focus(); }
          }} />
        <button type="button" onClick={runGoToLine}>Go</button>
        <IconButton icon="close" label="Close go to line" shortcut="Escape" onClick={() => setGoToOpen(false)} />
      </div> : null}
      <div className="editor-toolbar" role="toolbar" aria-label="Editor tools">
        {syntax.languageId === "markdown" ? <div className="markdown-mode-switch" role="group" aria-label="Markdown mode">
          <IconButton icon="edit" label="Edit" tooltip="Edit Markdown source" pressed={!isMarkdownView} onClick={() => setMarkdownMode("edit")} />
          <IconButton icon="preview" label="View" tooltip="Preview formatted Markdown" pressed={isMarkdownView} onClick={() => setMarkdownMode("view")} />
        </div> : null}
        {!isMarkdownView ? <>
        <IconButton icon="find" label="Find" tooltip="Find and replace" shortcut="Ctrl/⌘ F" pressed={findOpen} onClick={() => setFindOpen((shown) => !shown)} />
        <IconButton icon="goTo" label="Go to line" shortcut="Ctrl/⌘ G" pressed={goToOpen} onClick={() => setGoToOpen((shown) => !shown)} />
        <IconButton icon="bookmark" label="Toggle bookmark" shortcut="Ctrl/⌘ F2" onClick={() => void toggleBookmark()} />
        <IconButton icon="bookmarkPrevious" label="Previous bookmark" shortcut="Shift F2" onClick={() => void moveToBookmark("previous")} />
        <IconButton icon="bookmarkNext" label="Next bookmark" shortcut="F2" onClick={() => void moveToBookmark("next")} />
        <IconButton icon="bookmarkClear" label="Clear bookmarks" onClick={() => void clearBookmarks()} />
        <select
          aria-label="Transform selections"
          defaultValue=""
          onChange={(event) => {
            const transform = event.currentTarget.value as "uppercase" | "lowercase" | "trimWhitespace";
            event.currentTarget.value = "";
            if (transform) void transformSelections(transform);
          }}
        >
          <option value="" disabled>Transform</option>
          <option value="uppercase">Uppercase</option>
          <option value="lowercase">Lowercase</option>
          <option value="trimWhitespace">Trim whitespace</option>
        </select>
        <select
          aria-label="Text encoding"
          value={snapshot.encoding}
          onChange={(event) => void setEncoding(event.currentTarget.value as TextEncoding)}
        >
          <option value="utf8">UTF-8</option>
          <option value="windows1252">Windows-1252</option>
        </select>
        <label className="language-picker">
          <span>View As</span>
          <select
            aria-label="View As language"
            value={syntax.languageId}
            onChange={(event) => void setLanguage(event.currentTarget.value)}
          >
            {languages.map((language) => <option value={language.id} key={language.id}>{language.displayName}</option>)}
          </select>
        </label>
        <select
          aria-label="Document symbols"
          value=""
          disabled={syntax.symbols.length === 0}
          onChange={(event) => {
            const offset = event.currentTarget.value;
            event.currentTarget.value = "";
            if (!offset) return;
            const selection = { anchorByte: offset, activeByte: offset };
            void commitSelections([selection])
              .then(() => revealSelections([selection]))
              .catch((error: unknown) => {
                onStatus(error instanceof Error ? error.message : "Could not reveal symbol");
              });
          }}
        >
          <option value="">{syntax.symbols.length} symbols</option>
          {syntax.symbols.map((symbol) => (
            <option value={symbol.startByte} key={`${symbol.startByte}-${symbol.name}`}>
              {symbol.name} · line {symbol.line}
            </option>
          ))}
        </select>
        <IconButton icon="wrap" label="Wrap" tooltip="Word wrap" pressed={wordWrap} onClick={() => setWordWrap((enabled) => !enabled)} />
        <IconButton icon="column" label="Column editing" tooltip="Column editing: drag selects a rectangle (or hold Alt Shift)"
          pressed={columnMode} onClick={() => setColumnMode((enabled) => !enabled)} />
        <IconButton icon="fold" label="Fold" tooltip="Fold region at caret" disabled={!caretFold}
          pressed={caretFold ? collapsedFolds.includes(caretFold.startByte) : false}
          onClick={() => {
            if (!caretFold) return;
            setCollapsedFolds((current) => current.includes(caretFold.startByte)
              ? current.filter((start) => start !== caretFold.startByte)
              : [...current, caretFold.startByte]);
          }} />
        <IconButton icon="lineChanges" pressed={showLineChanges} disabled={!onToggleLineChanges}
          label={showLineChanges ? "Hide line change indicators" : "Show line change indicators"}
          tooltip={showLineChanges ? "Hide Line Change Indicators" : "Show Line Change Indicators"}
          onClick={() => onToggleLineChanges?.()} />
        <IconButton icon="map" label="Toggle document map" pressed={showDocumentMap}
          tooltip={showDocumentMap ? "Hide document map" : "Show document map"}
          onClick={() => setShowDocumentMap((shown) => !shown)} />
        <button className={`editor-mode access-mode${readOnly ? " read-only" : ""}`} type="button" aria-pressed={readOnly}
          disabled={!onToggleReadOnly} onClick={() => onToggleReadOnly?.()}
          aria-label={readOnly ? "Read-only mode: switch to Edit mode" : "Edit mode: switch to Read-Only mode"}
          title={readOnly ? "Read-Only mode — click to allow editing" : "Edit mode — click to make this document read-only"}>
          <Icon name={readOnly ? "lock" : "edit"} size={14} />{readOnly ? "READ ONLY" : "EDIT"}</button>
        <button className="editor-mode" type="button" aria-pressed={overwrite}
          onClick={() => setOverwrite((enabled) => !enabled)}
          title="Toggle insert/overwrite mode (Insert)">{overwrite ? "OVR" : "INS"}</button>
        <span className="selection-count">
          {snapshot.rectangularSelection
            ? "Rectangular selection"
            : `${snapshot.selections.length} caret${snapshot.selections.length === 1 ? "" : "s"}`}
          {syntax.truncated ? " · syntax capped" : ""}
          {showLineChanges && !lineChangesAvailable ? " · line changes unavailable until saved" : ""}
          {showLineChanges && lineChangesAvailable && lineChangesApproximate ? " · line changes approximate" : ""}
        </span>
        </> : <span className="markdown-view-label">Formatted Markdown preview</span>}
      </div>
      <div className="editor-content">
      {isMarkdownView ? previewError
        ? <div className="markdown-preview-message" role="alert">{previewError}</div>
        : currentPreview
          ? <MarkdownPreview documentId={document.documentId} source={currentPreview.text} />
          : <div className="markdown-preview-message" role="status">Rendering Markdown preview…</div>
      : <>
      <div
        ref={surfaceRef}
        className={`editor-surface${wordWrap ? " word-wrap" : ""}`}
        onScroll={onScroll}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          inputRef.current?.focus({ preventScroll: true });
        }}
        onPointerMove={(event) => {
          const drag = dragSelectionRef.current;
          if (!drag || (event.buttons & 1) === 0) return;
          const surface = event.currentTarget;
          const bounds = surface.getBoundingClientRect();
          const maximum = Math.max(0, surface.scrollHeight - surface.clientHeight);
          if (event.clientY >= bounds.bottom - AUTO_SCROLL_EDGE) {
            if (surface.scrollTop < maximum) {
              surface.scrollTop = Math.min(maximum, surface.scrollTop + rowHeight);
              setScrollTop(surface.scrollTop);
            } else {
              void loadDragPage("next");
            }
          } else if (event.clientY <= bounds.top + AUTO_SCROLL_EDGE) {
            if (surface.scrollTop > 0) {
              surface.scrollTop = Math.max(0, surface.scrollTop - rowHeight);
              setScrollTop(surface.scrollTop);
            } else {
              void loadDragPage("previous");
            }
          }
        }}
      >
        <textarea
          ref={inputRef}
          className="editor-input-capture"
          aria-label={`${document.name} editor${viewId ? ` ${viewId}` : ""}`}
          aria-multiline="true"
          aria-describedby={instructionsId}
          aria-controls={linesId}
          aria-keyshortcuts="Control+F Meta+F Control+H Meta+H Control+D Meta+D Control+G Meta+G ArrowLeft ArrowRight ArrowUp ArrowDown Shift+ArrowLeft Shift+ArrowRight Shift+ArrowUp Shift+ArrowDown PageUp PageDown"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onInput={onInput}
          onKeyDown={onKeyDown}
          onCopy={(event) => {
            event.preventDefault();
            void transferSelection(false);
          }}
          onCut={(event) => {
            event.preventDefault();
            void transferSelection(true);
          }}
          onPaste={onPaste}
          onCompositionStart={() => {
            composingRef.current = true;
            suppressedCompositionRef.current = null;
            setCompositionText("");
          }}
          onCompositionUpdate={(event) => setCompositionText(event.data)}
          onCompositionEnd={onCompositionEnd}
        />
        <div
          aria-hidden="true"
          className="virtual-lines"
          id={linesId}
          style={wordWrap ? undefined : { height: displayedLines.length * rowHeight }}
        >
          <div style={wordWrap ? undefined : { transform: `translateY(${firstVisible * rowHeight}px)` }}>
            {visibleLines.map(({ line, lineIndex }) => {
              const lineStartByte = BigInt(line.startByte);
              const textEndByte = lineStartByte + BigInt(
                line.sourceByteWidths.reduce((total, width) => total + width, 0),
              );
              const lineEndByte = BigInt(line.endByte);
              const caretIndices = snapshot.selections.flatMap((selection) => {
                const active = BigInt(selection.activeByte);
                if (active < lineStartByte || active > textEndByte) return [];
                return [utf16IndexForByte(
                  line.text,
                  line.sourceByteWidths,
                  Number(active - lineStartByte),
                )];
              });
              const selectionRanges = snapshot.selections.flatMap((selection) => {
                const anchor = BigInt(selection.anchorByte);
                const active = BigInt(selection.activeByte);
                const start = anchor < active ? anchor : active;
                const end = anchor > active ? anchor : active;
                const selectedStart = start > lineStartByte ? start : lineStartByte;
                const selectedEnd = end < textEndByte ? end : textEndByte;
                if (selectedStart >= selectedEnd) return [];
                return [{
                  start: utf16IndexForByte(
                    line.text,
                    line.sourceByteWidths,
                    Number(selectedStart - lineStartByte),
                  ),
                  end: utf16IndexForByte(
                    line.text,
                    line.sourceByteWidths,
                    Number(selectedEnd - lineStartByte),
                  ),
                }];
              });
              const rectangle = snapshot.rectangularSelection;
              if (rectangle) {
                const lineNumber = BigInt(line.line);
                const firstLine = BigInt(rectangle.anchorLine) < BigInt(rectangle.activeLine)
                  ? BigInt(rectangle.anchorLine)
                  : BigInt(rectangle.activeLine);
                const lastLine = BigInt(rectangle.anchorLine) > BigInt(rectangle.activeLine)
                  ? BigInt(rectangle.anchorLine)
                  : BigInt(rectangle.activeLine);
                if (lineNumber >= firstLine && lineNumber <= lastLine) {
                  const startColumn = BigInt(rectangle.anchorColumn) < BigInt(rectangle.activeColumn)
                    ? BigInt(rectangle.anchorColumn)
                    : BigInt(rectangle.activeColumn);
                  const endColumn = BigInt(rectangle.anchorColumn) > BigInt(rectangle.activeColumn)
                    ? BigInt(rectangle.anchorColumn)
                    : BigInt(rectangle.activeColumn);
                  const start = utf16IndexForScalarColumn(line.text, startColumn);
                  const end = utf16IndexForScalarColumn(line.text, endColumn);
                  if (start < end) selectionRanges.push({ start, end });
                  // A zero-width column shows a caret on every row it spans.
                  else if (!caretIndices.includes(start)) caretIndices.push(start);
                }
              }
              const syntaxRanges = syntax.highlights.flatMap((highlight) => {
                const startByte = BigInt(highlight.startByte);
                const endByte = BigInt(highlight.endByte);
                const clippedStart = startByte > lineStartByte ? startByte : lineStartByte;
                const clippedEnd = endByte < textEndByte ? endByte : textEndByte;
                if (clippedStart >= clippedEnd) return [];
                return [{
                  start: utf16IndexForByte(
                    line.text,
                    line.sourceByteWidths,
                    Number(clippedStart - lineStartByte),
                  ),
                  end: utf16IndexForByte(
                    line.text,
                    line.sourceByteWidths,
                    Number(clippedEnd - lineStartByte),
                  ),
                  token: highlight.token,
                }];
              });
              const bracketRanges = matchedBracketOffsets.flatMap((offset) => {
                if (offset < lineStartByte || offset >= textEndByte) return [];
                const relative = Number(offset - lineStartByte);
                return [{
                  start: utf16IndexForByte(line.text, line.sourceByteWidths, relative),
                  end: utf16IndexForByte(line.text, line.sourceByteWidths, relative + 1),
                }];
              });
              const fold = syntax.folds.find((candidate) => candidate.startLine === line.line);
              const foldCollapsed = fold ? collapsedFolds.includes(fold.startByte) : false;
              const selectsLineEnding = lineEndByte > textEndByte
                && snapshot.selections.some((selection) => {
                  const anchor = BigInt(selection.anchorByte);
                  const active = BigInt(selection.activeByte);
                  const start = anchor < active ? anchor : active;
                  const end = anchor > active ? anchor : active;
                  return start < lineEndByte && end > textEndByte;
                });
              const hasBookmark = snapshot.bookmarks.some((bookmark) => {
                const offset = BigInt(bookmark);
                return offset >= lineStartByte && offset <= lineEndByte;
              });
              return (
                <div
                  className={`editor-line${caretIndices.length > 0 ? " current" : ""}`}
                  data-line-start={line.startByte}
                  style={wordWrap ? undefined : { height: rowHeight }}
                  key={`${line.startByte}-${lineIndex}`}
                  onPointerDown={(event) => {
                    if (event.button !== 0) return;
                    event.preventDefault();
                    const offset = pointerOffset(event, lineIndex);
                    inputRef.current?.focus({ preventScroll: true });
                    if (event.detail === 2 && !event.altKey && !event.shiftKey) {
                      dragSelectionRef.current = null;
                      void executeCommand<Selection>({
                        id: "editor.selectWordAt",
                        context: { windowId: "main", documentId: document.documentId },
                        arguments: { expectedRevision: snapshot.revision, byteOffset: offset },
                      }).then((selected) => {
                        setSnapshot((current) => current
                          ? { ...current, selections: [selected], rectangularSelection: undefined }
                          : current);
                      }).catch((error: unknown) => onStatus(
                        error instanceof Error ? error.message : "Could not select word",
                      ));
                      return;
                    }
                    if (event.detail >= 3 && !event.altKey && !event.shiftKey) {
                      dragSelectionRef.current = null;
                      void commitSelections([{ anchorByte: line.startByte, activeByte: line.endByte }])
                        .catch((error: unknown) => onStatus(
                          error instanceof Error ? error.message : "Could not select line",
                        ));
                      return;
                    }
                    const code = event.currentTarget.querySelector("code");
                    const primaryStart = BigInt(primarySelection.anchorByte) < BigInt(primarySelection.activeByte)
                      ? BigInt(primarySelection.anchorByte) : BigInt(primarySelection.activeByte);
                    const primaryEnd = BigInt(primarySelection.anchorByte) > BigInt(primarySelection.activeByte)
                      ? BigInt(primarySelection.anchorByte) : BigInt(primarySelection.activeByte);
                    const rectangular = (event.altKey && event.shiftKey)
                      || (columnMode && !event.altKey && !event.shiftKey);
                    if (!event.altKey && !event.shiftKey && !columnMode && !snapshot.rectangularSelection
                      && snapshot.selections.length === 1 && primaryStart < primaryEnd
                      && BigInt(offset) >= primaryStart && BigInt(offset) < primaryEnd
                      && event.target instanceof Node && code?.contains(event.target)) {
                      dragSelectionRef.current = {
                        pointerId: event.pointerId,
                        selections: snapshot.selections,
                        targetIndex: 0,
                        move: {
                          startByte: primaryStart.toString(), endByte: primaryEnd.toString(),
                          pressByte: offset, pressX: event.clientX, pressY: event.clientY,
                          destinationByte: offset, started: false, validDrop: false,
                        },
                      };
                      return;
                    }
                    const relativeByte = Number(BigInt(offset) - BigInt(line.startByte));
                    const column = rectangular && code
                      ? scalarColumnForClientX(line.text, event.clientX, code)
                      : scalarColumnForByte(
                          line.text,
                          line.sourceByteWidths,
                          relativeByte,
                        );
                    const rectangularSelection = rectangular
                      ? {
                          anchorByte: offset,
                          activeByte: offset,
                          anchorLine: line.line,
                          activeLine: line.line,
                          anchorColumn: column,
                          activeColumn: column,
                        }
                      : undefined;
                    const selection: Selection = {
                      anchorByte: event.shiftKey && !event.altKey
                        ? primarySelection.anchorByte
                        : offset,
                      activeByte: offset,
                    };
                    const selections = event.altKey && !event.shiftKey
                      ? [...snapshot.selections, selection]
                      : [selection];
                    const targetIndex = selections.length - 1;
                    dragSelectionRef.current = {
                      pointerId: event.pointerId,
                      selections,
                      targetIndex,
                      rectangularSelection,
                    };
                    setSnapshot((current) => current
                      ? { ...current, selections, rectangularSelection }
                      : current);
                  }}
                  onPointerMove={(event) => {
                    const drag = dragSelectionRef.current;
                    if (!drag || drag.pointerId !== event.pointerId || (event.buttons & 1) === 0) {
                      return;
                    }
                    event.preventDefault();
                    if (drag.move) {
                      const move = drag.move;
                      if (!move.started && Math.hypot(
                        event.clientX - move.pressX,
                        event.clientY - move.pressY,
                      ) < 4) return;
                      move.started = true;
                      move.validDrop = true;
                      move.destinationByte = pointerOffset(event, lineIndex);
                      setDropTargetByte(move.destinationByte);
                      return;
                    }
                    const target = drag.selections[drag.targetIndex];
                    if (!target) return;
                    const activeByte = pointerOffset(event, lineIndex);
                    const selection: Selection = drag.rectangularSelection
                      ? { anchorByte: activeByte, activeByte }
                      : { anchorByte: target.anchorByte, activeByte };
                    drag.selections[drag.targetIndex] = selection;
                    if (drag.rectangularSelection) {
                      const relativeByte = Number(
                        BigInt(selection.activeByte) - BigInt(line.startByte),
                      );
                      const code = event.currentTarget.querySelector("code");
                      drag.rectangularSelection = {
                        ...drag.rectangularSelection,
                        activeByte: selection.activeByte,
                        activeLine: line.line,
                        activeColumn: code
                          ? scalarColumnForClientX(line.text, event.clientX, code)
                          : scalarColumnForByte(
                              line.text,
                              line.sourceByteWidths,
                              relativeByte,
                            ),
                      };
                    }
                    setSnapshot((current) => current
                      ? {
                          ...current,
                          selections: [...drag.selections],
                          rectangularSelection: drag.rectangularSelection,
                        }
                      : current);
                  }}
                >
                  <span className={`line-number${hasBookmark ? " bookmarked" : ""}`} aria-hidden="true">
                    {fold ? <button
                      type="button"
                      className="fold-toggle"
                      title={foldCollapsed ? "Expand region" : "Collapse region"}
                      onPointerDown={(event) => event.stopPropagation()}
                      onClick={(event) => {
                        event.stopPropagation();
                        setCollapsedFolds((current) => foldCollapsed
                          ? current.filter((start) => start !== fold.startByte)
                          : [...current, fold.startByte]);
                      }}
                    >{foldCollapsed ? "▸" : "▾"}</button> : null}
                    {line.continuation ? "·" : line.line}
                    {showLineChanges && lineChangesAvailable && line.change
                      ? <i className={`line-change ${line.change}`} data-line-change={line.change}
                          title={LINE_CHANGE_LABELS[line.change]} />
                      : null}
                  </span>
                  <code>
                    {renderLineText(
                      line.text,
                      selectionRanges,
                      caretIndices,
                      syntaxRanges,
                      bracketRanges,
                      dropTargetByte !== null
                        && BigInt(dropTargetByte) >= lineStartByte
                        && BigInt(dropTargetByte) <= textEndByte
                        ? utf16IndexForByte(
                            line.text,
                            line.sourceByteWidths,
                            Number(BigInt(dropTargetByte) - lineStartByte),
                          )
                        : undefined,
                      highlightMatches ? findHighlights(line.text, findQuery, regexSearch, caseSensitive) : [],
                    )}
                    {foldCollapsed ? <span className="fold-placeholder"> … </span> : null}
                    {selectsLineEnding ? <span className="selection line-ending"> </span> : null}
                  </code>
                </div>
              );
            })}
          </div>
        </div>
      </div>
      {showDocumentMap ? <div className="document-map" ref={mapRef} role="button" tabIndex={0}
        aria-label="Document map" title="Click to navigate the document"
        onPointerDown={(event) => {
          event.currentTarget.setPointerCapture?.(event.pointerId);
          const bounds = event.currentTarget.getBoundingClientRect();
          if (bounds.height <= 0) return;
          navigateMap(event.clientY - bounds.top + event.currentTarget.scrollTop);
        }}
        onPointerMove={(event) => {
          if (!event.currentTarget.hasPointerCapture?.(event.pointerId) || (event.buttons & 1) === 0) return;
          const bounds = event.currentTarget.getBoundingClientRect();
          if (bounds.height > 0) navigateMap(event.clientY - bounds.top + event.currentTarget.scrollTop);
        }}
        onPointerUp={(event) => {
          if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
        }}
        onKeyDown={(event) => {
          if (event.key !== "Enter" && event.key !== " ") return;
          event.preventDefault();
          navigateMap(event.currentTarget.scrollTop + mapHeight / 2);
        }}>
        <div className="document-map-content" style={{ height: mapContentHeight }}>
          <div className="document-map-viewport" style={{
            top: markerStart,
            height: markerHeight,
          }} />
          {previewSamples
            .map((sample) => <span className={`document-map-line${sample.tone ? ` ${sample.tone}` : ""}`} aria-hidden="true" data-preview={sample.text.slice(0, 90) || " "} key={sample.byteOffset} style={{
              top: mapPoint(sample.line, sample.byteOffset),
            }} />)}
          {showLineChanges && (currentMap?.lineChanges?.available ?? true)
            ? (currentMap?.changes ?? []).map((tick) => <i className={`document-map-change ${tick.change}`}
              aria-hidden="true" key={`${tick.byteOffset}-${tick.change}`}
              style={{ top: mapPoint(tick.line, tick.byteOffset) }} />)
            : null}
        </div>
      </div> : null}
      </>}
      </div>
      {!isMarkdownView && (pageHistory.length > 0 || snapshot.next) ? <div className="viewport-navigation"
        style={{ right: showDocumentMap ? 130 : 18 }}>
        <button type="button" disabled={pageHistory.length === 0} onClick={loadPrevious}>
          Previous viewport
        </button>
        <button type="button" disabled={!snapshot.next} onClick={loadNext}>
          Next viewport
        </button>
      </div> : null}
    </div>
  );
}
