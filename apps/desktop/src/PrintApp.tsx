import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { PrintPageFrame, PrintPagesBatch, PrintedPage } from "./contracts";
import { executeCommand, printThisWindow, printWindowContent, printWindowMarkdownImage, type PrintWindowContent } from "./ipc";
import { MarkdownPreview, type ImageLoader } from "./MarkdownPreview";
import { PrintedPageView } from "./PrintedPageView";
import { PAPER_SIZES, loadPrintSettings } from "./printSettings";
import { useWindowTheme } from "./windowTheme";
import "./print.css";

/**
 * Print Preview window (`IMPROVEMENT_PLAN_3.md` §3.6). It shows the pages
 * the core laid out for the editor window's Print (read with `print.pages`,
 * the only document data this window may ask for), or the rendered Markdown
 * preview of the document being printed, and opens the system print dialog.
 */

const PRINT_CONTENT_EVENT = "nulis://print-content";
/** Pages read per `print.pages` call. */
export const PAGE_BATCH = 50;

/** The paper size as `@page` takes it, after orientation. */
export function pageSizeRule(widthMm: number, heightMm: number, margins?: { top: number; right: number; bottom: number; left: number }): string {
  const margin = margins ? `${margins.top}mm ${margins.right}mm ${margins.bottom}mm ${margins.left}mm` : "0";
  return `@page { size: ${widthMm}mm ${heightMm}mm; margin: ${margin}; }`;
}

/**
 * Whether the engine repeats a box's padding on every printed page. Chromium
 * does; WebKit (macOS, Linux) does not, and adds no header of its own there.
 */
export function repeatsPagePadding(): boolean {
  try {
    return typeof CSS !== "undefined" && typeof CSS.supports === "function" && CSS.supports("box-decoration-break", "clone");
  } catch {
    return false;
  }
}

/** Puts `rule` in the print style sheet while mounted; the dialog may still change the paper. */
function usePageRule(rule: string | null) {
  useEffect(() => {
    if (!rule) return undefined;
    const sheet = Array.from(document.styleSheets).find((candidate) => {
      try { return Array.from(candidate.cssRules).some((cssRule) => cssRule.cssText.includes("--print-sheet")); } catch { return false; }
    });
    if (!sheet) return undefined;
    let index = -1;
    try {
      index = sheet.insertRule(rule, sheet.cssRules.length);
    } catch {
      return undefined;
    }
    return () => { try { sheet.deleteRule(index); } catch { /* already gone */ } };
  }, [rule]);
}

type Shown =
  | { kind: "loading" }
  | { kind: "empty" }
  | { kind: "pages"; printId: number; title: string; frame: PrintPageFrame; total: number; pages: PrintedPage[] }
  | { kind: "markdown"; title: string; documentId: string; text: string; remoteImages: boolean };

function errorText(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  if (typeof reason === "string") return reason;
  if (reason && typeof reason === "object" && "message" in reason) return String((reason as { message: unknown }).message);
  return String(reason);
}

/** Reads every prepared page in batches, showing them as they arrive. */
async function readPages(update: (shown: Shown) => void, stale: () => boolean): Promise<void> {
  let printId: number | null = null;
  let pages: PrintedPage[] = [];
  let start = 0;
  for (;;) {
    const batch = await executeCommand<PrintPagesBatch>({
      id: "print.pages", context: { windowId: "print" }, arguments: { start, count: PAGE_BATCH },
    });
    if (stale()) return;
    if (batch.printId !== printId) {
      printId = batch.printId;
      // A newer Print replaced the pages: start again from its first page.
      if (start > 0) { pages = []; start = 0; continue; }
    }
    pages = [...pages, ...batch.pages.map((page) => JSON.parse(page) as PrintedPage)];
    update({ kind: "pages", printId, title: batch.title, frame: batch.frame, total: batch.total, pages });
    start += batch.pages.length;
    if (batch.pages.length === 0 || start >= batch.total) return;
  }
}

function sheetMargins(margins: { top: number; right: number; bottom: number; left: number }): string {
  return `${margins.top}mm ${margins.right}mm ${margins.bottom}mm ${margins.left}mm`;
}

const loadMarkdownImage: ImageLoader = (_documentId, relativePath) => printWindowMarkdownImage(relativePath);

/** Root of the Print Preview window. */
export function PrintApp() {
  const style = useWindowTheme("print");
  const [shown, setShown] = useState<Shown>({ kind: "loading" });
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);

  const load = useCallback(() => {
    const current = ++generation.current;
    const stale = () => current !== generation.current;
    setError(null);
    setShown({ kind: "loading" });
    void printWindowContent().then(async (content: PrintWindowContent) => {
      if (stale()) return;
      if (content.kind === "markdown") {
        setShown({ kind: "markdown", title: content.title, documentId: content.source.documentId, text: content.source.text, remoteImages: content.remoteImages });
      } else if (content.kind === "pages") {
        await readPages((next) => { if (!stale()) setShown(next); }, stale);
      } else {
        setShown({ kind: "empty" });
      }
    }).catch((reason: unknown) => { if (!stale()) setError(errorText(reason)); });
  }, []);

  useEffect(() => {
    document.title = "Print Preview";
    // Lets print.css release the window-height layout from styles.css when printing.
    document.documentElement.classList.add("print-window");
    load();
    let stop: (() => void) | undefined;
    let disposed = false;
    void listen(PRINT_CONTENT_EVENT, load).then((unlisten) => {
      if (disposed) unlisten(); else stop = unlisten;
    }).catch(() => undefined);
    return () => {
      disposed = true;
      stop?.();
      document.documentElement.classList.remove("print-window");
    };
  }, [load]);

  // Markdown flows onto the paper chosen in Page Setup, within its margins.
  const markdownSetup = shown.kind === "markdown" ? loadPrintSettings().setup : null;
  const markdownPaper = markdownSetup ? PAPER_SIZES.find(({ id }) => id === markdownSetup.paper) ?? PAPER_SIZES[0] : null;
  const landscape = shown.kind === "pages" ? shown.frame.landscape : markdownSetup?.orientation === "landscape";
  const paper = shown.kind === "pages"
    ? { widthMm: shown.frame.paperWidthMm, heightMm: shown.frame.paperHeightMm }
    : markdownPaper ? { widthMm: markdownPaper.widthMm, heightMm: markdownPaper.heightMm } : null;
  const pageWidthMm = paper ? (landscape ? paper.heightMm : paper.widthMm) : 0;
  const pageHeightMm = paper ? (landscape ? paper.widthMm : paper.heightMm) : 0;
  const repeatMargins = repeatsPagePadding();
  usePageRule(paper ? pageSizeRule(pageWidthMm, pageHeightMm, repeatMargins ? undefined : markdownSetup?.margins) : null);

  const title = shown.kind === "pages" || shown.kind === "markdown" ? shown.title : "";
  useEffect(() => {
    // macOS suggests the page title as the name in Save as PDF.
    document.title = title || "Print Preview";
  }, [title]);

  const ready = (shown.kind === "pages" && shown.pages.length >= shown.total) || shown.kind === "markdown";
  const print = () => {
    if (!paper) return;
    setError(null);
    void printThisWindow({ ...paper, landscape: Boolean(landscape) }).catch((reason: unknown) => setError(errorText(reason)));
  };

  const status = shown.kind === "pages"
    ? shown.pages.length < shown.total
      ? `Preparing ${shown.pages.length.toLocaleString()} of ${shown.total.toLocaleString()} pages…`
      : shown.total === 1 ? "1 page" : `${shown.total.toLocaleString()} pages`
    : shown.kind === "markdown" ? "Formatted Markdown" : "";

  return (
    <div className="print-root" style={style}>
      <header className="print-toolbar">
        <span className="print-toolbar-title">{title || "Print Preview"}</span>
        <span className="print-hint" role="status">{status}</span>
        <button type="button" className="primary" onClick={print} disabled={!ready}>Print…</button>
      </header>
      {error ? <p role="alert" className="print-error">{error}</p> : null}
      {shown.kind === "loading" ? <div className="print-message" role="status">Preparing pages…</div> : null}
      {shown.kind === "empty" ? <div className="print-message">Choose File ▸ Print… in the editor window.</div> : null}
      {shown.kind === "pages" ? (
        <main className="print-pages">
          {shown.pages.map((page) => <PrintedPageView key={page.number} page={page} frame={shown.frame} total={shown.total} />)}
        </main>
      ) : null}
      {shown.kind === "markdown" && markdownSetup ? (
        <main className="print-pages">
          <div className={repeatMargins ? "print-markdown-sheet repeat-margins" : "print-markdown-sheet"} style={{
            width: `${pageWidthMm}mm`,
            padding: sheetMargins(markdownSetup.margins),
            ["--sheet-margins" as string]: sheetMargins(markdownSetup.margins),
          }}>
            <MarkdownPreview documentId={shown.documentId} source={shown.text} loadImage={loadMarkdownImage} className="print-markdown"
              remoteImages={shown.remoteImages ? "allow" : "block"} />
          </div>
        </main>
      ) : null}
    </div>
  );
}
