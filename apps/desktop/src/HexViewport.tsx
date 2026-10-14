import {
  type ClipboardEvent,
  type KeyboardEvent,
  type UIEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type {
  DocumentSummary,
  HexCaret,
  HexClipboardPayload,
  HexEditResult,
  HexViewportSnapshot,
  Selection,
} from "./contracts";
import { executeCommand, getHexViewport } from "./ipc";

const BYTES_PER_ROW = 16;
const PAGE_BYTES = 4 * 1024;
const ROW_HEIGHT = 24;
const OVERSCAN_ROWS = 8;
const textEncoder = new TextEncoder();

interface HexViewportProps {
  document: DocumentSummary;
  onDocumentChange(document: DocumentSummary): void;
  onStatus(message: string): void;
  viewId?: string;
}

function messageFor(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

function byteHex(byte: number): string {
  return byte.toString(16).padStart(2, "0").toUpperCase();
}

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes, byteHex).join("");
}

function asciiByte(byte: number): string {
  return byte >= 0x20 && byte <= 0x7e ? String.fromCharCode(byte) : ".";
}

function normalizedSelection(selection: Selection | undefined): { start: bigint; end: bigint } {
  if (!selection) return { start: 0n, end: 0n };
  const anchor = BigInt(selection.anchorByte);
  const active = BigInt(selection.activeByte);
  return anchor <= active ? { start: anchor, end: active } : { start: active, end: anchor };
}

export function HexViewport({
  document,
  onDocumentChange,
  onStatus,
  viewId = "primary",
}: HexViewportProps) {
  const [snapshot, setSnapshot] = useState<HexViewportSnapshot | null>(null);
  const [pageStart, setPageStart] = useState("0");
  const [pageHistory, setPageHistory] = useState<string[]>([]);
  const [caret, setCaret] = useState<HexCaret>({ byteOffset: "0", highNibble: true });
  const [activeColumn, setActiveColumn] = useState<"hex" | "ascii">("hex");
  const [overwrite, setOverwrite] = useState(true);
  const [gotoValue, setGotoValue] = useState("0x0");
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(500);
  const surfaceRef = useRef<HTMLDivElement>(null);

  const load = useCallback(async (startByte: string, resetCaret = false) => {
    const next = await getHexViewport({
      documentId: document.documentId,
      startByte,
      maxBytes: PAGE_BYTES,
      bytesPerRow: BYTES_PER_ROW,
    });
    const actualStart = next.rows[0]?.startByte ?? startByte;
    setSnapshot(next);
    setPageStart(actualStart);
    setScrollTop(0);
    if (surfaceRef.current) surfaceRef.current.scrollTop = 0;
    if (resetCaret) {
      const activeByte = next.selections[0]?.activeByte ?? actualStart;
      setCaret({ byteOffset: activeByte, highNibble: true });
    }
    return next;
  }, [document.documentId]);

  useEffect(() => {
    setPageHistory([]);
    void load("0", true).catch((error: unknown) => {
      onStatus(messageFor(error, "Could not load hexadecimal viewport"));
    });
  }, [document.documentId, load, onStatus]);

  useEffect(() => {
    if (!snapshot || snapshot.revision === document.revision) return;
    void load(pageStart).catch((error: unknown) => {
      onStatus(messageFor(error, "Could not refresh hexadecimal viewport"));
    });
  }, [document.revision, load, onStatus, pageStart, snapshot]);

  useEffect(() => {
    const element = surfaceRef.current;
    if (!element || typeof ResizeObserver === "undefined") return undefined;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setViewportHeight(entry.contentRect.height);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const primarySelection = snapshot?.selections[0];
  const selection = useMemo(() => normalizedSelection(primarySelection), [primarySelection]);
  const offsetWidth = Math.max(
    8,
    Math.min(16, Math.max(1, BigInt(snapshot?.documentByteLen ?? "0").toString(16).length)),
  );
  const firstVisible = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN_ROWS);
  const visibleCount = Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN_ROWS * 2;
  const visibleRows = snapshot?.rows.slice(firstVisible, firstVisible + visibleCount) ?? [];

  const setCoreSelection = useCallback(async (next: Selection) => {
    await executeCommand<Selection[]>({
      id: "editor.setSelections",
      context: { windowId: "main", documentId: document.documentId },
      arguments: { selections: [next] },
    });
    setSnapshot((current) => current ? { ...current, selections: [next] } : current);
  }, [document.documentId]);

  const chooseByte = async (offset: bigint, highNibble: boolean, extend: boolean) => {
    const anchor = extend && primarySelection
      ? BigInt(primarySelection.anchorByte)
      : offset;
    const active = extend ? (offset >= anchor ? offset + 1n : offset) : offset;
    const next = { anchorByte: anchor.toString(), activeByte: active.toString() };
    setCaret({ byteOffset: offset.toString(), highNibble });
    try {
      await setCoreSelection(next);
      surfaceRef.current?.focus();
    } catch (error: unknown) {
      onStatus(messageFor(error, "Could not update byte selection"));
    }
  };

  const refreshAfterEdit = async (result: HexEditResult) => {
    setCaret(result.caret);
    onDocumentChange(result.document);
    const target = BigInt(result.caret.byteOffset);
    const first = snapshot?.rows[0] ? BigInt(snapshot.rows[0].startByte) : 0n;
    const lastRow = snapshot?.rows.at(-1);
    const end = lastRow
      ? BigInt(lastRow.startByte) + BigInt(lastRow.bytes.length)
      : first;
    const documentLength = BigInt(result.document.byteLen);
    const outsidePage = target < first || (target >= end && target < documentLength);
    await load(outsidePage ? result.caret.byteOffset : pageStart);
    onStatus(`Edited byte ${result.caret.byteOffset}`);
  };

  const typeNibble = async (key: string) => {
    if (!snapshot) return;
    try {
      const result = await executeCommand<HexEditResult>({
        id: "hex.typeNibble",
        context: { windowId: "main", documentId: document.documentId },
        arguments: {
          expectedRevision: snapshot.revision,
          highNibble: caret.highNibble,
          nibble: Number.parseInt(key, 16),
          overwrite,
        },
      });
      await refreshAfterEdit(result);
    } catch (error: unknown) {
      onStatus(messageFor(error, "Hexadecimal edit failed"));
    }
  };

  const replaceBytes = async (hex: string) => {
    if (!snapshot) return;
    try {
      const result = await executeCommand<HexEditResult>({
        id: "hex.replaceBytes",
        context: { windowId: "main", documentId: document.documentId },
        arguments: { expectedRevision: snapshot.revision, hex, overwrite },
      });
      await refreshAfterEdit(result);
    } catch (error: unknown) {
      onStatus(messageFor(error, "Byte replacement failed"));
    }
  };

  const deleteBackward = async () => {
    if (!snapshot) return;
    try {
      const result = await executeCommand<HexEditResult>({
        id: "hex.deleteBackward",
        context: { windowId: "main", documentId: document.documentId },
        arguments: { expectedRevision: snapshot.revision },
      });
      await refreshAfterEdit(result);
    } catch (error: unknown) {
      onStatus(messageFor(error, "Byte deletion failed"));
    }
  };

  const deleteForward = async () => {
    if (!snapshot) return;
    try {
      let activeSelection = primarySelection;
      if (!activeSelection || activeSelection.anchorByte === activeSelection.activeByte) {
        const offset = BigInt(caret.byteOffset);
        const documentLength = BigInt(snapshot.documentByteLen);
        if (offset >= documentLength) return;
        activeSelection = { anchorByte: offset.toString(), activeByte: (offset + 1n).toString() };
        await setCoreSelection(activeSelection);
      }
      const updated = await executeCommand<DocumentSummary>({
        id: "editor.deleteSelection",
        context: { windowId: "main", documentId: document.documentId },
        arguments: { expectedRevision: snapshot.revision },
      });
      const nextOffset = normalizedSelection(activeSelection).start.toString();
      setCaret({ byteOffset: nextOffset, highNibble: true });
      onDocumentChange(updated);
      await load(pageStart);
    } catch (error: unknown) {
      onStatus(messageFor(error, "Byte deletion failed"));
    }
  };

  const moveCaret = async (key: string, extend: boolean) => {
    if (!snapshot) return;
    const length = BigInt(snapshot.documentByteLen);
    let offset = BigInt(caret.byteOffset);
    let highNibble = caret.highNibble;
    if (key === "ArrowLeft") {
      if (activeColumn === "hex" && !highNibble && !extend) {
        highNibble = true;
      } else if (offset > 0n) {
        offset -= 1n;
        highNibble = activeColumn === "hex" && !extend ? false : true;
      }
    } else if (key === "ArrowRight") {
      if (activeColumn === "hex" && highNibble && offset < length && !extend) {
        highNibble = false;
      } else if (offset < length) {
        offset += 1n;
        highNibble = true;
      }
    } else if (key === "ArrowUp") {
      offset = offset >= BigInt(BYTES_PER_ROW) ? offset - BigInt(BYTES_PER_ROW) : 0n;
    } else if (key === "ArrowDown") {
      offset = (offset + BigInt(BYTES_PER_ROW)) > length
        ? length
        : offset + BigInt(BYTES_PER_ROW);
    } else if (key === "Home") {
      offset -= offset % BigInt(BYTES_PER_ROW);
      highNibble = true;
    } else if (key === "End") {
      const rowEnd = offset - offset % BigInt(BYTES_PER_ROW) + BigInt(BYTES_PER_ROW);
      offset = rowEnd > length ? length : rowEnd;
      highNibble = true;
    }
    const previousOffset = BigInt(caret.byteOffset);
    setCaret({ byteOffset: offset.toString(), highNibble });
    if (offset !== previousOffset || extend) {
      const anchor = extend && primarySelection
        ? primarySelection.anchorByte
        : offset.toString();
      await setCoreSelection({ anchorByte: anchor, activeByte: offset.toString() });
      const first = snapshot.rows[0] ? BigInt(snapshot.rows[0].startByte) : 0n;
      const lastRow = snapshot.rows.at(-1);
      const end = lastRow
        ? BigInt(lastRow.startByte) + BigInt(lastRow.bytes.length)
        : first;
      if (offset < first || (offset >= end && offset < length)) {
        setPageHistory((current) => [...current.slice(-255), pageStart]);
        await load(offset.toString());
      }
    }
  };

  const undoOrRedo = async (redo: boolean) => {
    try {
      const updated = await executeCommand<DocumentSummary>({
        id: redo ? "editor.redo" : "editor.undo",
        context: { windowId: "main", documentId: document.documentId },
        arguments: {},
      });
      onDocumentChange(updated);
      await load(pageStart, true);
      onStatus(redo ? "Redid byte edit" : "Undid byte edit");
    } catch (error: unknown) {
      onStatus(messageFor(error, redo ? "Redo failed" : "Undo failed"));
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const command = event.metaKey || event.ctrlKey;
    if (command && event.key.toLowerCase() === "z") {
      event.preventDefault();
      void undoOrRedo(event.shiftKey);
      return;
    }
    if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      void moveCaret(event.key, event.shiftKey).catch((error: unknown) => {
        onStatus(messageFor(error, "Could not move byte caret"));
      });
      return;
    }
    if (event.key === "Backspace") {
      event.preventDefault();
      void deleteBackward();
      return;
    }
    if (event.key === "Delete") {
      event.preventDefault();
      void deleteForward();
      return;
    }
    if (event.key === "Insert") {
      event.preventDefault();
      setOverwrite((enabled) => !enabled);
      return;
    }
    if (!command && activeColumn === "hex" && /^[0-9a-f]$/i.test(event.key)) {
      event.preventDefault();
      void typeNibble(event.key);
      return;
    }
    if (!command && activeColumn === "ascii" && event.key.length === 1) {
      event.preventDefault();
      void replaceBytes(bytesToHex(textEncoder.encode(event.key)));
    }
  };

  const copySelection = async (cut: boolean) => {
    if (!snapshot) return;
    try {
      const payload = await executeCommand<HexClipboardPayload>({
        id: "hex.selection",
        context: { windowId: "main", documentId: document.documentId },
        arguments: {},
      });
      await writeText(payload.hex);
      if (cut && payload.hex) {
        const updated = await executeCommand<DocumentSummary>({
          id: "editor.deleteSelection",
          context: { windowId: "main", documentId: document.documentId },
          arguments: { expectedRevision: payload.revision },
        });
        onDocumentChange(updated);
        await load(pageStart, true);
      }
      onStatus(payload.hex ? `Copied ${payload.hex.split(" ").length} bytes` : "No bytes selected");
    } catch (error: unknown) {
      onStatus(messageFor(error, cut ? "Byte cut failed" : "Byte copy failed"));
    }
  };

  const handlePaste = (event: ClipboardEvent<HTMLDivElement>) => {
    event.preventDefault();
    const text = event.clipboardData.getData("text/plain");
    const hex = activeColumn === "hex" ? text : bytesToHex(textEncoder.encode(text));
    void replaceBytes(hex);
  };

  const gotoOffset = async () => {
    if (!snapshot) return;
    try {
      const trimmed = gotoValue.trim();
      const offset = trimmed.toLowerCase().startsWith("0x")
        ? BigInt(trimmed)
        : BigInt(trimmed || "0");
      if (offset < 0n || offset > BigInt(snapshot.documentByteLen)) {
        throw new Error(`Offset must be between 0 and ${snapshot.documentByteLen}`);
      }
      setPageHistory((current) => [...current.slice(-255), pageStart]);
      await load(offset.toString());
      setCaret({ byteOffset: offset.toString(), highNibble: true });
      await setCoreSelection({ anchorByte: offset.toString(), activeByte: offset.toString() });
    } catch (error: unknown) {
      onStatus(messageFor(error, "Invalid byte offset"));
    }
  };

  if (!snapshot) return <div className="editor-loading">Loading hexadecimal viewport…</div>;

  return (
    <div className="hex-stage">
      <div className="hex-toolbar">
        <span>BYTES</span>
        <button type="button" aria-pressed={activeColumn === "hex"} onClick={() => setActiveColumn("hex")}>Hex</button>
        <button type="button" aria-pressed={activeColumn === "ascii"} onClick={() => setActiveColumn("ascii")}>ASCII</button>
        <label>
          <span>Go to</span>
          <input
            aria-label="Go to byte offset"
            value={gotoValue}
            onChange={(event) => setGotoValue(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void gotoOffset();
            }}
          />
        </label>
        <button type="button" onClick={() => void gotoOffset()}>Go</button>
        <button type="button" aria-pressed={overwrite} onClick={() => setOverwrite((enabled) => !enabled)}>
          {overwrite ? "Overwrite" : "Insert"}
        </button>
        <span className="hex-position">0x{BigInt(caret.byteOffset).toString(16).toUpperCase().padStart(offsetWidth, "0")} · {caret.highNibble ? "high" : "low"}</span>
      </div>
      <div className="hex-column-header" aria-hidden="true">
        <span className="hex-offset-heading">OFFSET</span>
        <span className="hex-byte-headings">
          {Array.from({ length: BYTES_PER_ROW }, (_, index) => <i key={index}>{index.toString(16).toUpperCase()}</i>)}
        </span>
        <span className="hex-ascii-heading">ASCII</span>
      </div>
      <div
        ref={surfaceRef}
        id={`hex-viewport-${viewId}`}
        className="hex-surface"
        role="grid"
        tabIndex={0}
        aria-label={`${document.name} hexadecimal editor`}
        aria-rowcount={snapshot.rows.length}
        onKeyDown={handleKeyDown}
        onCopy={(event) => { event.preventDefault(); void copySelection(false); }}
        onCut={(event) => { event.preventDefault(); void copySelection(true); }}
        onPaste={handlePaste}
        onScroll={(event: UIEvent<HTMLDivElement>) => setScrollTop(event.currentTarget.scrollTop)}
      >
        <p className="screen-reader-only">
          Hexadecimal editor. Arrow keys move by nibble or byte. Type hexadecimal digits in Hex mode, or characters in ASCII mode.
        </p>
        <div className="hex-virtual-rows" style={{ height: snapshot.rows.length * ROW_HEIGHT }}>
          <div style={{ transform: `translateY(${firstVisible * ROW_HEIGHT}px)` }}>
            {visibleRows.map((row) => (
              <div className="hex-row" role="row" key={row.startByte} style={{ height: ROW_HEIGHT }}>
                <span className="hex-offset">{BigInt(row.startByte).toString(16).toUpperCase().padStart(offsetWidth, "0")}</span>
                <span className="hex-bytes" role="gridcell">
                  {row.bytes.map((byte, index) => {
                    const offset = BigInt(row.startByte) + BigInt(index);
                    const selected = offset >= selection.start && offset < selection.end;
                    const current = offset === BigInt(caret.byteOffset);
                    const hex = byteHex(byte);
                    return <span className={`hex-byte${selected ? " selected" : ""}${current ? " current" : ""}`} key={offset.toString()}>
                      <button
                        type="button"
                        className={current && caret.highNibble && activeColumn === "hex" ? "active-nibble" : ""}
                        aria-label={`Byte ${offset.toString()} high nibble ${hex[0]}`}
                        onPointerDown={(event) => {
                          event.preventDefault();
                          setActiveColumn("hex");
                          void chooseByte(offset, true, event.shiftKey);
                        }}
                      >{hex[0]}</button>
                      <button
                        type="button"
                        className={current && !caret.highNibble && activeColumn === "hex" ? "active-nibble" : ""}
                        aria-label={`Byte ${offset.toString()} low nibble ${hex[1]}`}
                        onPointerDown={(event) => {
                          event.preventDefault();
                          setActiveColumn("hex");
                          void chooseByte(offset, false, event.shiftKey);
                        }}
                      >{hex[1]}</button>
                    </span>;
                  })}
                </span>
                <span className="hex-ascii" role="gridcell">
                  {row.bytes.map((byte, index) => {
                    const offset = BigInt(row.startByte) + BigInt(index);
                    const selected = offset >= selection.start && offset < selection.end;
                    const current = offset === BigInt(caret.byteOffset) && activeColumn === "ascii";
                    return <button
                      type="button"
                      className={`${selected ? "selected" : ""}${current ? " current" : ""}`}
                      aria-label={`Byte ${offset.toString()} ASCII ${asciiByte(byte)}`}
                      key={offset.toString()}
                      onPointerDown={(event) => {
                        event.preventDefault();
                        setActiveColumn("ascii");
                        void chooseByte(offset, true, event.shiftKey);
                      }}
                    >{asciiByte(byte)}</button>;
                  })}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
      {pageHistory.length > 0 || snapshot.next ? <div className="viewport-navigation">
        <button
          type="button"
          disabled={pageHistory.length === 0}
          onClick={() => {
            const previous = pageHistory.at(-1);
            if (!previous) return;
            setPageHistory((current) => current.slice(0, -1));
            void load(previous);
          }}
        >Previous bytes</button>
        <button
          type="button"
          disabled={!snapshot.next}
          onClick={() => {
            if (!snapshot.next) return;
            setPageHistory((current) => [...current.slice(-255), pageStart]);
            void load(snapshot.next.byteOffset);
          }}
        >Next bytes</button>
      </div> : null}
    </div>
  );
}
