import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { printThisWindow } from "./ipc";
import { useWindowTheme } from "./windowTheme";
import "./print.css";

/**
 * Print Preview window. In M36 (`IMPROVEMENT_PLAN_3.md`) it shows calibration
 * pages instead of a document: printing them on each platform shows whether
 * page size, margins, and page breaks come through the native print dialog
 * unchanged. M37–M39 replace them with pages laid out by the core.
 */

export type PaperSize = "a4" | "letter";
export type Orientation = "portrait" | "landscape";

/** Paper sizes in millimetres, portrait. */
export const PAPER_MM: Record<PaperSize, { width: number; height: number; label: string; css: string }> = {
  a4: { width: 210, height: 297, label: "A4", css: "A4" },
  letter: { width: 215.9, height: 279.4, label: "Letter", css: "letter" },
};

export const CALIBRATION_MARGIN_MM = 15;
export const CALIBRATION_FONT_PT = 9;
export const CALIBRATION_LINE_PT = 11;
/** Header and footer bands inside the margin box, each one line tall. */
const BAND_LINES = 2;
const PAGE_COUNT = 3;
const POINTS_PER_MM = 72 / 25.4;

export interface PageGeometry {
  widthMm: number;
  heightMm: number;
  /** Body lines that fit between the header and footer bands. */
  linesPerPage: number;
  /** Monospace columns that fit across the margin box. */
  columns: number;
}

/** Page size and how many calibration lines fit, from paper, orientation, and the fixed margin and font. */
export function pageGeometry(paper: PaperSize, orientation: Orientation): PageGeometry {
  const size = PAPER_MM[paper];
  const widthMm = orientation === "portrait" ? size.width : size.height;
  const heightMm = orientation === "portrait" ? size.height : size.width;
  const contentHeightPt = (heightMm - 2 * CALIBRATION_MARGIN_MM) * POINTS_PER_MM;
  const contentWidthPt = (widthMm - 2 * CALIBRATION_MARGIN_MM) * POINTS_PER_MM;
  // JetBrains Mono advances 600/1000 em per character.
  const columns = Math.floor(contentWidthPt / (CALIBRATION_FONT_PT * 0.6));
  const linesPerPage = Math.floor(contentHeightPt / CALIBRATION_LINE_PT) - BAND_LINES;
  return { widthMm, heightMm, linesPerPage, columns };
}

/** Sets the printed paper size with an `@page` rule; the dialog may still override it. */
function usePageRule(paper: PaperSize, orientation: Orientation) {
  useEffect(() => {
    const sheet = Array.from(document.styleSheets).find((candidate) => {
      try { return Array.from(candidate.cssRules).some((rule) => rule.cssText.includes("--print-sheet")); } catch { return false; }
    });
    if (!sheet) return undefined;
    let index = -1;
    try {
      index = sheet.insertRule(`@page { size: ${PAPER_MM[paper].css} ${orientation}; margin: 0; }`, sheet.cssRules.length);
    } catch {
      return undefined;
    }
    return () => { try { sheet.deleteRule(index); } catch { /* already gone */ } };
  }, [paper, orientation]);
}

function rulerTicks(lengthMm: number): number[] {
  const ticks: number[] = [];
  for (let mm = 10; mm < lengthMm; mm += 10) ticks.push(mm);
  return ticks;
}

function CalibrationPage({ index, geometry, paper, orientation }: {
  index: number; geometry: PageGeometry; paper: PaperSize; orientation: Orientation;
}) {
  // print.css sizes the page from --page-height (one pixel shorter when printing).
  const pageStyle = {
    width: `${geometry.widthMm}mm`, "--page-height": `${geometry.heightMm}mm`,
  } as CSSProperties;
  const boxStyle: CSSProperties = {
    left: `${CALIBRATION_MARGIN_MM}mm`, top: `${CALIBRATION_MARGIN_MM}mm`,
    right: `${CALIBRATION_MARGIN_MM}mm`, bottom: `${CALIBRATION_MARGIN_MM}mm`,
    fontSize: `${CALIBRATION_FONT_PT}pt`, lineHeight: `${CALIBRATION_LINE_PT}pt`,
  };
  const ruler = "0123456789".repeat(Math.ceil(geometry.columns / 10)).slice(0, geometry.columns);
  const lines = Array.from({ length: geometry.linesPerPage }, (_, line) => {
    const number = String(line + 1).padStart(3, " ");
    if (line === 0) return `${number} ${ruler}`.slice(0, geometry.columns);
    if (line === geometry.linesPerPage - 1) return `${number} LAST LINE OF PAGE ${index + 1}`;
    return `${number} The quick brown fox jumps over the lazy dog. ÅÉÎÕÜ ñ ß → ≠ ≤ ≥ λ Ж`.slice(0, geometry.columns);
  });
  return (
    <section className="print-page" style={pageStyle} aria-label={`Page ${index + 1} of ${PAGE_COUNT}`}>
      {rulerTicks(geometry.widthMm).map((mm) => (
        <span key={`x${mm}`} className="print-tick print-tick-x" style={{ left: `${mm}mm` }} />
      ))}
      {rulerTicks(geometry.heightMm).map((mm) => (
        <span key={`y${mm}`} className="print-tick print-tick-y" style={{ top: `${mm}mm` }} />
      ))}
      <div className="print-margin-box" style={boxStyle}>
        <div className="print-band">
          <span>Nulis print calibration · {PAPER_MM[paper].label} {orientation}</span>
          <span>{CALIBRATION_MARGIN_MM} mm margins · ticks every 10 mm</span>
        </div>
        <pre className="print-body">{lines.join("\n")}</pre>
        <div className="print-band print-band-footer"><span>Page {index + 1} of {PAGE_COUNT}</span></div>
      </div>
    </section>
  );
}

/** Root of the Print Preview window. */
export function PrintApp() {
  const style = useWindowTheme("print");
  const [paper, setPaper] = useState<PaperSize>(() => (navigator.language === "en-US" ? "letter" : "a4"));
  const [orientation, setOrientation] = useState<Orientation>("portrait");
  const [error, setError] = useState<string | null>(null);
  const geometry = useMemo(() => pageGeometry(paper, orientation), [paper, orientation]);
  usePageRule(paper, orientation);
  useEffect(() => {
    document.title = "Print Preview";
    // Lets print.css release the window-height layout from styles.css when printing.
    document.documentElement.classList.add("print-window");
    return () => document.documentElement.classList.remove("print-window");
  }, []);
  const print = () => {
    setError(null);
    const size = PAPER_MM[paper];
    void printThisWindow({ widthMm: size.width, heightMm: size.height, landscape: orientation === "landscape" }).catch((reason: unknown) => setError(reason instanceof Error ? reason.message : String(reason)));
  };
  return (
    <div className="print-root" style={style}>
      <header className="print-toolbar">
        <label>Paper
          <select value={paper} onChange={(event) => setPaper(event.target.value as PaperSize)}>
            {(Object.keys(PAPER_MM) as PaperSize[]).map((id) => <option key={id} value={id}>{PAPER_MM[id].label}</option>)}
          </select>
        </label>
        <label>Orientation
          <select value={orientation} onChange={(event) => setOrientation(event.target.value as Orientation)}>
            <option value="portrait">Portrait</option>
            <option value="landscape">Landscape</option>
          </select>
        </label>
        <span className="print-hint">Calibration pages. The print dialog opens on this paper; keep it.</span>
        <button type="button" className="primary" onClick={print}>Print…</button>
      </header>
      {error ? <p role="alert" className="print-error">{error}</p> : null}
      <main className="print-pages">
        {Array.from({ length: PAGE_COUNT }, (_, index) => (
          <CalibrationPage key={index} index={index} geometry={geometry} paper={paper} orientation={orientation} />
        ))}
      </main>
    </div>
  );
}
