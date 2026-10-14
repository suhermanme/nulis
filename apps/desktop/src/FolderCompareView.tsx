import type { FolderCompareResult } from "./contracts";

interface FolderCompareViewProps {
  result: FolderCompareResult;
  onClose(): void;
}

function rootName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

export function FolderCompareView({ result, onClose }: FolderCompareViewProps) {
  return <section className="folder-compare-view" aria-label="Folder comparison">
    <header className="folder-compare-header">
      <div title={result.leftRoot}><strong>{rootName(result.leftRoot)}</strong><small>{result.leftRoot}</small></div>
      <span aria-hidden="true">⇄</span>
      <div title={result.rightRoot}><strong>{rootName(result.rightRoot)}</strong><small>{result.rightRoot}</small></div>
      <button type="button" onClick={onClose} aria-label="Close folder comparison">×</button>
    </header>
    <p className="folder-compare-summary">
      {result.entries.length} differences · {result.identicalFiles} identical files · {result.comparedFiles} files compared
      {result.skippedSymlinks ? ` · ${result.skippedSymlinks} symlinks skipped` : ""}
      {result.truncated ? " · result limited" : ""}
    </p>
    <div className="folder-compare-list" role="list" aria-label="Folder differences">
      {result.entries.length === 0 ? <p className="compare-empty">The folders have identical file content.</p> : null}
      {result.entries.map((entry) => <div className={`folder-compare-entry ${entry.status}`}
        role="listitem" key={entry.relativePath}>
        <span className="folder-status">{entry.status === "leftOnly" ? "←" : entry.status === "rightOnly" ? "→" : "≠"}</span>
        <strong>{entry.relativePath}</strong>
        <span>{entry.leftKind ?? "—"}{entry.leftByteLen ? ` · ${entry.leftByteLen} B` : ""}</span>
        <span>{entry.rightKind ?? "—"}{entry.rightByteLen ? ` · ${entry.rightByteLen} B` : ""}</span>
      </div>)}
    </div>
  </section>;
}
