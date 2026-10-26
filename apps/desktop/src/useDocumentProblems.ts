import { useEffect, useState } from "react";
import type { DocumentProblems, DocumentSummary } from "./contracts";
import { errorMessage } from "./errors";
import { executeCommand } from "./ipc";
import { worstSeverity } from "./ProblemsPanel";

/** Language-server problems by document, asked for after each change of the active document. */
export function useDocumentProblems({ activeDocument, viewModes, documents }: {
  activeDocument: DocumentSummary | undefined;
  viewModes: Record<string, "text" | "hex">;
  documents: DocumentSummary[];
}) {
  const activeDocumentId = activeDocument?.documentId ?? null;
  // Language-server problems by document, and why the last request failed.
  const [problemsByDocument, setProblemsByDocument] = useState<Record<string, DocumentProblems>>({});
  const [problemsErrors, setProblemsErrors] = useState<Record<string, string>>({});
  const activeProblems = activeDocument ? problemsByDocument[activeDocument.documentId] : undefined;
  const activeProblemsError = activeDocument ? problemsErrors[activeDocument.documentId] : undefined;
  const activeProblemCount = activeProblems?.problems.length ?? 0;
  const activeProblemSeverity = worstSeverity(activeProblems?.problems ?? []);

  // Language-server problems arrive a little after the text; ask shortly after a
  // change and once more after that, never while typing. The panel stays open
  // with the last answer while the next one is on its way.
  useEffect(() => {
    const documentId = activeDocumentId;
    const revision = activeDocument?.revision;
    const server = activeDocument?.capabilities?.languageServer;
    if (!documentId || !revision || viewModes[documentId] === "hex") return;
    if (!server) {
      setProblemsByDocument((current) => {
        const existing = current[documentId];
        if (!existing || existing.problems.length === 0) return current;
        return { ...current, [documentId]: { revision, problems: [] } };
      });
      return;
    }
    let cancelled = false;
    const ask = () => {
      void executeCommand<DocumentProblems | null>({
        id: "lsp.diagnostics",
        context: { windowId: "main", documentId },
        arguments: {},
      }).then((result) => {
        if (cancelled || !result || typeof result.revision !== "string" || !Array.isArray(result.problems)) return;
        // Problems placed in another revision would underline the wrong bytes.
        if (result.revision !== revision) return;
        setProblemsByDocument((current) => ({ ...current, [documentId]: result }));
        setProblemsErrors((current) => {
          if (!(documentId in current)) return current;
          const next = { ...current };
          delete next[documentId];
          return next;
        });
      }).catch((error: unknown) => {
        if (cancelled) return;
        setProblemsErrors((current) => ({
          ...current, [documentId]: errorMessage(error, "Could not ask the language server for problems"),
        }));
      });
    };
    const first = window.setTimeout(ask, 500);
    const second = window.setTimeout(ask, 2500);
    return () => { cancelled = true; window.clearTimeout(first); window.clearTimeout(second); };
  }, [activeDocumentId, activeDocument?.revision, activeDocument?.capabilities?.languageServer, viewModes]);

  // Forget problems of documents that are no longer open.
  useEffect(() => {
    const open = new Set(documents.map((document) => document.documentId));
    setProblemsByDocument((current) => {
      const kept = Object.entries(current).filter(([documentId]) => open.has(documentId));
      return kept.length === Object.keys(current).length ? current : Object.fromEntries(kept);
    });
    setProblemsErrors((current) => {
      const kept = Object.entries(current).filter(([documentId]) => open.has(documentId));
      return kept.length === Object.keys(current).length ? current : Object.fromEntries(kept);
    });
  }, [documents]);

  return { problemsByDocument, activeProblems, activeProblemsError, activeProblemCount, activeProblemSeverity };
}
