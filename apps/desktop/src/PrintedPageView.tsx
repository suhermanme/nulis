import { memo, type CSSProperties } from "react";
import type { PrintPageFrame, PrintPiece, PrintedPage } from "./contracts";

/**
 * One page laid out by the core (`nulis_print::PrintedPage`), drawn as
 * fixed-size HTML for Print and the Page Setup preview. Every piece of text
 * is placed at its cell, so Print matches the exported PDF column for
 * column. Text is rendered as React text nodes, never parsed as HTML.
 */

/** CSS pixels per millimetre. */
export const PX_PER_MM = 96 / 25.4;

function Piece({ piece, frame }: { piece: PrintPiece; frame: PrintPageFrame }) {
  const [column, cells, styleIndex, text] = piece;
  const style = frame.styles[styleIndex] ?? frame.styles[0];
  // A single character from another font is centred in its cells, like the PDF.
  const single = Array.from(text).length === 1;
  return (
    <span className={single ? "print-piece print-cell" : "print-piece"} style={{
      left: `${column * frame.cellPt}pt`,
      width: `${cells * frame.cellPt}pt`,
      color: style?.color,
      fontWeight: style?.bold ? 700 : 400,
    }}>{text}</span>
  );
}

function Line({ pieces, topPt, frame }: { pieces: PrintPiece[]; topPt: number; frame: PrintPageFrame }) {
  return (
    <div className="print-line" style={{ top: `${topPt}pt`, left: `${frame.marginLeftPt}pt`, height: `${frame.lineHeightPt}pt` }}>
      {pieces.map((piece) => <Piece key={piece[0]} piece={piece} frame={frame} />)}
    </div>
  );
}

export const PrintedPageView = memo(function PrintedPageView({ page, frame, total }: {
  page: PrintedPage; frame: PrintPageFrame; total?: number;
}) {
  // print.css sizes the page from --page-height (one pixel shorter when printing).
  const style = {
    width: `${frame.widthMm}mm`,
    "--page-height": `${frame.heightMm}mm`,
    fontSize: `${frame.fontSizePt}pt`,
    lineHeight: `${frame.lineHeightPt}pt`,
    background: frame.background ?? "#ffffff",
  } as CSSProperties;
  const label = total ? `Page ${page.number} of ${total}` : `Page ${page.number}`;
  return (
    <section className="print-page" style={style} aria-label={label}>
      {frame.hasHeader ? <Line pieces={page.header} topPt={frame.headerTopPt} frame={frame} /> : null}
      {page.rows.map((pieces, row) => pieces.length > 0
        ? <Line key={row} pieces={pieces} topPt={frame.bodyTopPt + row * frame.lineHeightPt} frame={frame} />
        : null)}
      {page.rules.map((row) => (
        <span key={`rule-${row}`} className="print-rule" style={{
          top: `${frame.bodyTopPt + (row + 0.5) * frame.lineHeightPt}pt`,
          left: `${frame.marginLeftPt}pt`,
          width: `${frame.columns * frame.cellPt}pt`,
          borderTopColor: frame.ruleColor,
        }} />
      ))}
      {frame.hasFooter ? <Line pieces={page.footer} topPt={frame.footerTopPt} frame={frame} /> : null}
    </section>
  );
});

/** A page drawn at `widthPx` wide, for the Page Setup preview. */
export function ScaledPageView({ page, frame, widthPx }: { page: PrintedPage; frame: PrintPageFrame; widthPx: number }) {
  const scale = widthPx / (frame.widthMm * PX_PER_MM);
  return (
    <div className="print-preview-frame" style={{ width: `${widthPx}px`, height: `${frame.heightMm * PX_PER_MM * scale}px` }}>
      <div style={{ transform: `scale(${scale})`, transformOrigin: "top left" }}>
        <PrintedPageView page={page} frame={frame} />
      </div>
    </div>
  );
}
