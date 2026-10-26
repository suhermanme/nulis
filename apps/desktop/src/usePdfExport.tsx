import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import type { DocumentSummary, PdfExportReport, PrintPageFrame, PrintedPage, ThemeRecord } from "./contracts";
import type { RemoteImagePolicy } from "./contracts";
import { keepFocusInDialog } from "./dialogFocus";
import { errorMessage } from "./errors";
import { executeCommand, openPrintWindow } from "./ipc";
import { remoteImagesAllowed } from "./MarkdownPreview";
import { colorChoice, type PrintSettings } from "./printSettings";

/** How often a running export is asked for its progress. */
const EXPORT_POLL_MS = 150;

/** What an export covers; `hexDump` is the hexadecimal editor's view of the bytes. */
type ExportScope = "document" | "selection" | "hexDump";

/**
 * The document an export or a print reads. An export writes `path`; a print
 * (`kind: "print"`) lays out pages for the Print Preview window.
 */
interface ExportTarget { kind: "pdf" | "print"; documentId: string; name: string; revision: string; path: string; scope: ExportScope }

/** Export to PDF and Print: the running job, its progress, and the question asked before a very large one. */
export function usePdfExport({ activeDocument, viewModes, printSettings, activeTheme, markdownViewing, remoteImagePolicy, setStatus }: {
  activeDocument: DocumentSummary | undefined;
  viewModes: Record<string, "text" | "hex">;
  printSettings: PrintSettings;
  activeTheme: ThemeRecord;
  /** The document whose Markdown preview is showing in the active pane. */
  markdownViewing: string | null;
  remoteImagePolicy: RemoteImagePolicy;
  setStatus(message: string): void;
}) {
  const [exportJob, setExportJob] = useState<{ jobId: number; kind: ExportTarget["kind"]; name: string; percent: number } | null>(null);
  const [largeExport, setLargeExport] = useState<(ExportTarget & { estimatedPages: string }) | null>(null);

  /** Asks where to save, then exports the document as it is now, unsaved edits included. */
  const exportPdf = async (requested: "document" | "selection") => {
    const document = activeDocument;
    if (!document) return;
    // In the hexadecimal editor the PDF is a dump of the bytes.
    const scope: ExportScope = viewModes[document.documentId] === "hex" ? "hexDump" : requested;
    if (exportJob) { setStatus(exportJob.kind === "print" ? "Pages are being prepared for printing" : "An export to PDF is already running"); return; }
    const suffix = scope === "selection" ? " (selection)" : scope === "hexDump" ? " (hex)" : "";
    const stem = `${document.name.replace(/\.[^.\\/]+$/, "") || "Untitled"}${suffix}`;
    const folder = document.sourcePath ? document.sourcePath.slice(0, Math.max(document.sourcePath.lastIndexOf("/"), document.sourcePath.lastIndexOf("\\")) + 1) : "";
    try {
      const destination = await saveDialog({
        title: "Export to PDF",
        defaultPath: `${folder}${stem}.pdf`,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!destination) { setStatus("Export cancelled"); return; }
      await runExport({ kind: "pdf", documentId: document.documentId, name: document.name, revision: document.revision, path: destination, scope }, false);
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not export to PDF"));
    }
  };

  /** The page setup and colors the core lays pages out with. */
  const printArguments = (settings: PrintSettings = printSettings) => ({
    setup: settings.setup,
    colors: colorChoice(settings.colors, activeTheme),
    date: new Date().toLocaleDateString(undefined, { dateStyle: "long" }),
  });

  /** Prints the active document: its Markdown preview when that is showing, else pages laid out by the core. */
  const printDocument = async () => {
    const document = activeDocument;
    if (!document) { setStatus("Open a document to print"); return; }
    if (exportJob) { setStatus(exportJob.kind === "print" ? "Pages are being prepared for printing" : "An export to PDF is already running"); return; }
    try {
      if (markdownViewing === document.documentId && viewModes[document.documentId] !== "hex") {
        await openPrintWindow({
          kind: "markdown", documentId: document.documentId, title: document.name,
          remoteImages: remoteImagesAllowed(remoteImagePolicy, document.documentId),
        });
        setStatus(`Print Preview shows ${document.name}`);
        return;
      }
      const scope: ExportScope = viewModes[document.documentId] === "hex" ? "hexDump" : "document";
      await runExport({ kind: "print", documentId: document.documentId, name: document.name, revision: document.revision, path: "", scope }, false);
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not print"));
    }
  };

  /** The first page of the active document for the Page Setup preview. */
  const previewFirstPage = async (settings: PrintSettings) => {
    const document = activeDocument;
    if (!document) throw new Error("Open a document to preview its first page");
    return executeCommand<{ frame: PrintPageFrame; page: PrintedPage }>({
      id: "file.printPreview",
      context: { windowId: "main", documentId: document.documentId },
      arguments: {
        expectedRevision: document.revision,
        ...printArguments(settings),
        scope: viewModes[document.documentId] === "hex" ? "hexDump" : "document",
      },
    });
  };

  /** Runs an export or print job, showing its progress, and reports how it ended. */
  const runExport = async (target: ExportTarget, confirmed: boolean) => {
    const printing = target.kind === "print";
    const { jobId } = await executeCommand<{ jobId: number }>({
      id: printing ? "file.printStart" : "file.exportPdfStart",
      context: { windowId: "main", documentId: target.documentId },
      arguments: {
        expectedRevision: target.revision,
        ...(printing ? {} : { path: target.path }),
        ...printArguments(),
        confirmed,
        scope: target.scope,
      },
    });
    setExportJob({ jobId, kind: target.kind, name: target.name, percent: 0 });
    setStatus(printing ? `Preparing ${target.name} for printing…` : `Exporting ${target.name} to PDF…`);
    let report: PdfExportReport;
    try {
      for (;;) {
        await new Promise((resolve) => setTimeout(resolve, EXPORT_POLL_MS));
        report = await executeCommand<PdfExportReport>({
          id: "file.exportPdfProgress", context: { windowId: "main" }, arguments: { jobId },
        });
        if (report.done) break;
        const { done, total } = report.progress;
        const percent = BigInt(total) > 0n ? Number((BigInt(done) * 100n) / BigInt(total)) : 0;
        setExportJob({ jobId, kind: target.kind, name: target.name, percent });
      }
    } finally {
      setExportJob(null);
    }
    if (report.needsConfirmation) {
      setLargeExport({ ...target, estimatedPages: report.needsConfirmation });
      setStatus(printing ? "Print waiting for your answer" : "Export waiting for your answer");
    } else if (report.cancelled) {
      setStatus(printing ? "Print cancelled" : "Export to PDF cancelled");
    } else if (report.error) {
      setStatus(report.error);
    } else if (report.printedPages) {
      const pages = report.printedPages === "1" ? "1 page" : `${Number(report.printedPages).toLocaleString()} pages`;
      await openPrintWindow({ kind: "pages" });
      setStatus(`${target.name}: ${pages} ready to print`);
    } else if (report.result) {
      const { result } = report;
      const name = result.path.split(/[\\/]/).pop() ?? result.path;
      const pages = result.pages === "1" ? "1 page" : `${Number(result.pages).toLocaleString()} pages`;
      const missing = result.missingGlyphs === "0" ? ""
        : `; ${Number(result.missingGlyphs).toLocaleString()} characters the print font lacks are left blank`;
      setStatus(`Exported ${pages} to ${name}${missing}`);
    }
  };

  const cancelExport = () => {
    if (!exportJob) return;
    void executeCommand({ id: "file.exportPdfCancel", context: { windowId: "main" }, arguments: { jobId: exportJob.jobId } })
      .catch(() => undefined);
  };

  return { exportJob, largeExport, setLargeExport, exportPdf, printDocument, previewFirstPage, runExport, cancelExport };
}

/** Asks before an export or a print of very many pages. */
export function LargeExportDialog({ exporter, onStatus: setStatus }: {
  exporter: Pick<ReturnType<typeof usePdfExport>, "largeExport" | "setLargeExport" | "runExport">;
  onStatus(message: string): void;
}) {
  const { largeExport, setLargeExport, runExport } = exporter;
  if (!largeExport) return null;
  return <div className="decision-backdrop">
      <section className="decision-dialog" role="dialog" aria-modal="true" aria-labelledby="large-export-title"
        onKeyDown={(event) => {
          keepFocusInDialog(event);
          if (event.key === "Escape") { setLargeExport(null); setStatus(largeExport.kind === "print" ? "Print cancelled" : "Export cancelled"); }
        }}>
        <h2 id="large-export-title">{largeExport.kind === "print" ? "Print" : "Export"} {Number(largeExport.estimatedPages).toLocaleString()} pages?</h2>
        <p>{largeExport.kind === "print"
          ? <>{largeExport.name} prints on at least {Number(largeExport.estimatedPages).toLocaleString()} pages,
            more if long lines wrap. Preparing this many pages takes a while.</>
          : <>{largeExport.name} makes a PDF of at least {Number(largeExport.estimatedPages).toLocaleString()} pages,
            more if long lines wrap. A PDF this large takes a while to write.</>}</p>
        <div className="decision-actions">
          <button autoFocus type="button" onClick={() => { setLargeExport(null); setStatus(largeExport.kind === "print" ? "Print cancelled" : "Export cancelled"); }}>Cancel</button>
          <button className="primary" type="button" onClick={() => {
            const target = largeExport;
            setLargeExport(null);
            void runExport(target, true).catch((error: unknown) => setStatus(errorMessage(error, target.kind === "print" ? "Could not print" : "Could not export to PDF")));
          }}>{largeExport.kind === "print" ? "Print" : "Export"}</button>
        </div>
      </section>
    </div>;
}
