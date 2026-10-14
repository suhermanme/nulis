import { useCallback, useEffect, useState } from "react";
import type { CompareMode, DiffChange, DiffPage, DocumentSummary } from "./contracts";
import { executeCommand } from "./ipc";

interface CompareViewProps {
  left: DocumentSummary;
  right: DocumentSummary;
  onDocumentChange(document: DocumentSummary): void;
  onStatus(message: string): void;
}

interface CompareCursor {
  leftByte: string;
  rightByte: string;
  leftLine: string;
  rightLine: string;
}

const FIRST_CURSOR: CompareCursor = {
  leftByte: "0",
  rightByte: "0",
  leftLine: "1",
  rightLine: "1",
};
const MAX_COMPARE_HISTORY = 256;

function byteCount(range: { startByte: string; endByte: string }): bigint {
  return BigInt(range.endByte) - BigInt(range.startByte);
}

export function CompareView({ left, right, onDocumentChange, onStatus }: CompareViewProps) {
  const [mode, setMode] = useState<CompareMode>("text");
  const [cursor, setCursor] = useState<CompareCursor>(FIRST_CURSOR);
  const [history, setHistory] = useState<CompareCursor[]>([]);
  const [page, setPage] = useState<DiffPage | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async (nextCursor: CompareCursor) => {
    setBusy(true);
    try {
      const result = await executeCommand<DiffPage>({
        id: "compare.documents",
        context: { windowId: "main" },
        arguments: {
          leftDocumentId: left.documentId,
          rightDocumentId: right.documentId,
          mode,
          leftStartByte: nextCursor.leftByte,
          rightStartByte: mode === "binary" ? nextCursor.leftByte : nextCursor.rightByte,
          leftStartLine: nextCursor.leftLine,
          rightStartLine: nextCursor.rightLine,
          maxBytes: 256 * 1024,
          maxChanges: 500,
        },
      });
      setPage(result);
      setCursor(nextCursor);
      onStatus(result.changes.length === 0 ? "No differences in this window" : `${result.changes.length} differences`);
    } catch (error: unknown) {
      onStatus(error instanceof Error ? error.message : "Comparison failed");
    } finally {
      setBusy(false);
    }
  }, [left.documentId, mode, onStatus, right.documentId]);

  useEffect(() => {
    setHistory([]);
    void load(FIRST_CURSOR);
  }, [left.documentId, right.documentId, mode, left.revision, right.revision, load]);

  const apply = async (change: DiffChange, direction: "left" | "right") => {
    const source = direction === "right" ? left : right;
    const target = direction === "right" ? right : left;
    const sourceRange = direction === "right" ? change.left : change.right;
    const targetRange = direction === "right" ? change.right : change.left;
    setBusy(true);
    try {
      const updated = await executeCommand<DocumentSummary>({
        id: "compare.applyChange",
        context: { windowId: "main", documentId: target.documentId },
        arguments: {
          sourceDocumentId: source.documentId,
          targetDocumentId: target.documentId,
          expectedTargetRevision: target.revision,
          sourceStart: sourceRange.startByte,
          sourceEnd: sourceRange.endByte,
          targetStart: targetRange.startByte,
          targetEnd: targetRange.endByte,
        },
      });
      onDocumentChange(updated);
      onStatus(`Applied change to ${target.name}`);
    } catch (error: unknown) {
      onStatus(error instanceof Error ? error.message : "Could not apply comparison change");
      setBusy(false);
    }
  };

  const nextPage = () => {
    if (!page || (!page.nextLeftByte && !page.nextRightByte)) return;
    setHistory((current) => [...current, cursor].slice(-MAX_COMPARE_HISTORY));
    void load({
      leftByte: page.nextLeftByte ?? left.byteLen,
      rightByte: page.nextRightByte ?? right.byteLen,
      leftLine: page.nextLeftLine ?? cursor.leftLine,
      rightLine: page.nextRightLine ?? cursor.rightLine,
    });
  };

  const previousPage = () => {
    const previous = history.at(-1);
    if (!previous) return;
    setHistory((current) => current.slice(0, -1));
    void load(previous);
  };

  return <section className="compare-view" aria-label={`Compare ${left.name} and ${right.name}`}>
    <header className="compare-header">
      <div><strong>{left.name}</strong><small>{left.byteLen} bytes</small></div>
      <span aria-hidden="true">⇄</span>
      <div><strong>{right.name}</strong><small>{right.byteLen} bytes</small></div>
      <label>Mode
        <select aria-label="Comparison mode" value={mode} disabled={busy}
          onChange={(event) => setMode(event.target.value as CompareMode)}>
          <option value="text">Text</option>
          <option value="binary">Binary</option>
        </select>
      </label>
    </header>
    <div className="compare-column-headings" aria-hidden="true">
      <span>{left.name}</span><span>Merge</span><span>{right.name}</span>
    </div>
    <div className="compare-changes" aria-live="polite">
      {busy && !page ? <p className="compare-empty">Comparing bounded windows…</p> : null}
      {!busy && page?.changes.length === 0 ? <p className="compare-empty">No differences in this window.</p> : null}
      {page?.changes.map((change, index) => <article className={`compare-change ${change.kind}`}
        key={`${change.left.startByte}-${change.right.startByte}-${index}`}>
        <div>
          <strong>{change.kind}</strong>
          <span>bytes {change.left.startByte}–{change.left.endByte}</span>
          <small>{byteCount(change.left).toString()} bytes{mode === "text" ? ` · line ${change.leftStartLine}` : ""}</small>
        </div>
        <div className="compare-actions">
          <button type="button" disabled={busy} aria-label={`Apply change to ${left.name}`}
            title={`Copy from ${right.name} to ${left.name}`} onClick={() => void apply(change, "left")}>←</button>
          <button type="button" disabled={busy} aria-label={`Apply change to ${right.name}`}
            title={`Copy from ${left.name} to ${right.name}`} onClick={() => void apply(change, "right")}>→</button>
        </div>
        <div>
          <strong>{change.kind}</strong>
          <span>bytes {change.right.startByte}–{change.right.endByte}</span>
          <small>{byteCount(change.right).toString()} bytes{mode === "text" ? ` · line ${change.rightStartLine}` : ""}</small>
        </div>
      </article>)}
    </div>
    <footer className="compare-navigation">
      <button type="button" disabled={busy || history.length === 0} onClick={previousPage}>Previous window</button>
      <span>left byte {cursor.leftByte} · right byte {cursor.rightByte}</span>
      <button type="button" disabled={busy || (!page?.nextLeftByte && !page?.nextRightByte)} onClick={nextPage}>Next window</button>
    </footer>
  </section>;
}
