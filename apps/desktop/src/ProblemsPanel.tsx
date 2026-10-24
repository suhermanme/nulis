import type { DocumentProblem, DocumentSummary } from "./contracts";
import { IconButton } from "./icons";

/** The problems panel's names for each severity, and the order they are listed in. */
const SEVERITY_LABELS: Record<DocumentProblem["severity"], string> = {
  error: "Error", warning: "Warning", information: "Information", hint: "Hint",
};

const SEVERITY_ORDER: DocumentProblem["severity"][] = ["error", "warning", "information", "hint"];

/** How many problems of each severity, in the panel's order. */
export function problemCounts(problems: DocumentProblem[]): Array<{ severity: DocumentProblem["severity"]; count: number }> {
  return SEVERITY_ORDER
    .map((severity) => ({ severity, count: problems.filter((problem) => problem.severity === severity).length }))
    .filter(({ count }) => count > 0);
}

/** The most serious severity among `problems`, or null when there are none. */
export function worstSeverity(items: ReadonlyArray<{ severity: DocumentProblem["severity"] }>): DocumentProblem["severity"] | null {
  return SEVERITY_ORDER.find((severity) => items.some((item) => item.severity === severity)) ?? null;
}

interface ProblemsPanelProps {
  document: DocumentSummary;
  problems: DocumentProblem[];
  /** The language server that reported them, when one is set up. */
  server?: string;
  /** Why the last request for problems failed, when it did. */
  error?: string | null;
  onSelect(problem: DocumentProblem): void;
  onClose(): void;
  onOpenSettings?(): void;
}

/**
 * Problems for one document, from a language server or a validator. Rows
 * are buttons so the whole list can be reached with the keyboard; choosing
 * one puts the caret on the problem.
 */
export function ProblemsPanel({
  document, problems, server, error, onSelect, onClose, onOpenSettings,
}: ProblemsPanelProps) {
  const counts = problemCounts(problems);
  return <section className="problems-panel" id="problems-panel" aria-label={`Problems in ${document.name}`}>
    <header>
      <strong>Problems</strong>
      <span className="problems-summary">
        {problems.length === 0 ? "No problems"
          : counts.map(({ severity, count }) => (
            <span className={`problem-count ${severity}`} key={severity}>{count} {SEVERITY_LABELS[severity].toLocaleLowerCase()}</span>
          ))}
      </span>
      {server ? <span className="problems-server" title="The language server that reports these problems">{server}</span> : null}
      <span className="problems-spacer" />
      {onOpenSettings ? <button type="button" className="problems-settings" onClick={onOpenSettings}>Language Servers…</button> : null}
      <IconButton icon="close" label="Close problems" onClick={onClose} />
    </header>
    {error ? <p className="problems-error" role="alert">{error}</p> : null}
    {problems.length === 0
      ? <p className="problems-empty">
        {server
          ? `${server} reports no problems in this document.`
          : "No language server is set up for this document. Add one under Settings ▸ Languages ▸ Language servers."}
      </p>
      : <ul className="problems-list" aria-label="Problems">
        {problems.map((problem, index) => <li key={`${problem.startByte}-${index}`}>
          <button type="button" className={`problem-row ${problem.severity}`}
            aria-label={`${SEVERITY_LABELS[problem.severity]} on line ${problem.line}: ${problem.message}`}
            onClick={() => onSelect(problem)}>
            <span className={`problem-severity ${problem.severity}`} aria-hidden="true">
              {problem.severity === "error" || problem.severity === "warning" ? "!" : "i"}
            </span>
            <span className="problem-message">{problem.message}</span>
            <span className="problem-source">
              {problem.source ? <>{problem.source} · </> : null}Ln {problem.line}
            </span>
          </button>
        </li>)}
      </ul>}
  </section>;
}
