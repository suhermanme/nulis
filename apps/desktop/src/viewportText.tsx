/**
 * Text geometry for the editor viewport: converting between byte offsets, UTF-16
 * indexes, scalar columns and pointer positions, and drawing one line with its
 * highlights, diagnostics and whitespace marks.
 */
import { Fragment, type ReactNode } from "react";
import type { DocumentProblem, SyntaxToken, ViewportSnapshot } from "./contracts";
import { worstSeverity } from "./ProblemsPanel";

const textEncoder = new TextEncoder();

export const MAX_HIGHLIGHTS_PER_LINE = 200;

export function utf16IndexForByte(text: string, sourceByteWidths: number[], byteOffset: number): number {
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

/** Source bytes covered by the first `utf16Index` UTF-16 units of a line. */
export function byteOffsetForUtf16(text: string, sourceByteWidths: number[], utf16Index: number): number {
  let bytes = 0;
  let utf16 = 0;
  let scalarIndex = 0;
  for (const scalar of text) {
    if (utf16 >= utf16Index) break;
    bytes += sourceByteWidths[scalarIndex] ?? textEncoder.encode(scalar).length;
    utf16 += scalar.length;
    scalarIndex += 1;
  }
  return bytes;
}

export function utf16IndexForScalarColumn(text: string, column: bigint): number {
  let utf16 = 0;
  let scalarColumn = 0n;
  for (const scalar of text) {
    if (scalarColumn >= column) break;
    utf16 += scalar.length;
    scalarColumn += 1n;
  }
  return utf16;
}

export function scalarColumnForByte(
  text: string,
  sourceByteWidths: number[],
  byteOffset: number,
): string {
  const utf16 = utf16IndexForByte(text, sourceByteWidths, byteOffset);
  return String(Array.from(text.slice(0, utf16)).length);
}

export function byteOffsetForClientX(
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

/**
 * UTF-16 index in a rendered line's text under a window point, using the
 * browser's own layout so wrapped rows hit the right character. The point is
 * clamped into the line's box. Returns null when the browser cannot tell.
 */
export function utf16IndexAtPoint(
  textLength: number,
  clientX: number,
  clientY: number,
  element: HTMLElement,
): number | null {
  const bounds = element.getBoundingClientRect();
  if (bounds.width <= 0 || bounds.height <= 0) return null;
  const x = Math.min(Math.max(clientX, bounds.left + 1), bounds.right - 1);
  const y = Math.min(Math.max(clientY, bounds.top + 1), bounds.bottom - 1);
  const owner = element.ownerDocument as Document & {
    caretPositionFromPoint?(x: number, y: number): { offsetNode: Node; offset: number } | null;
    caretRangeFromPoint?(x: number, y: number): Range | null;
  };
  let node: Node | null = null;
  let offset = 0;
  if (typeof owner.caretPositionFromPoint === "function") {
    const position = owner.caretPositionFromPoint(x, y);
    if (position) ({ offsetNode: node, offset } = position);
  } else if (typeof owner.caretRangeFromPoint === "function") {
    const range = owner.caretRangeFromPoint(x, y);
    if (range) ({ startContainer: node, startOffset: offset } = range);
  }
  if (!node || !element.contains(node)) return null;
  const before = element.ownerDocument.createRange();
  before.setStart(element, 0);
  before.setEnd(node, offset);
  return Math.min(textLength, before.toString().length);
}

export function scalarColumnForClientX(text: string, clientX: number, element: HTMLElement): string {
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

export function viewportContainsByte(snapshot: ViewportSnapshot, byteOffset: bigint): boolean {
  return snapshot.lines.some((line) => {
    const start = BigInt(line.startByte);
    const textEnd = start + BigInt(
      line.sourceByteWidths.reduce((total, width) => total + width, 0),
    );
    return byteOffset >= start && byteOffset <= textEnd;
  });
}

/**
 * Wraps each space and tab so CSS can draw a mark over it; the characters
 * themselves stay, so widths and caret positions do not change.
 */
export function markWhitespace(segment: string, offset: number): ReactNode {
  if (!/[ \t]/.test(segment)) return segment;
  return segment.split(/([ \t])/).filter(Boolean).map((part, index) => part === " " || part === "\t"
    ? <span className={part === " " ? "ws-space" : "ws-tab"} key={`ws-${offset}-${index}`}>{part}</span>
    : part);
}

export function renderLineText(
  text: string,
  selectionRanges: Array<{ start: number; end: number }>,
  caretIndices: number[],
  syntaxRanges: Array<{ start: number; end: number; token: SyntaxToken }>,
  bracketRanges: Array<{ start: number; end: number }>,
  dropIndex?: number,
  highlightRanges: Array<{ start: number; end: number }> = [],
  diagnosticRanges: Array<{ start: number; end: number; severity: DocumentProblem["severity"] }> = [],
  showWhitespace = false,
): ReactNode[] {
  const boundaries = [...new Set([
    0,
    text.length,
    ...selectionRanges.flatMap((range) => [range.start, range.end]),
    ...highlightRanges.flatMap((range) => [range.start, range.end]),
    ...diagnosticRanges.flatMap((range) => [range.start, range.end]),
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
    const diagnostic = worstSeverity(diagnosticRanges
      .filter((range) => start >= range.start && end <= range.end));
    const className = [
      selected ? "selection" : syntax ? `syntax syntax-${syntax.token}` : "",
      bracket ? "bracket-match" : "",
      highlighted && !selected ? "find-highlight" : "",
      diagnostic ? `diagnostic diagnostic-${diagnostic}` : "",
    ].filter(Boolean).join(" ");
    const content = showWhitespace ? markWhitespace(segment, start) : segment;
    output.push(className
      ? <span className={className} key={`text-${start}`}>{content}</span>
      : showWhitespace ? <Fragment key={`text-${start}`}>{content}</Fragment> : segment);
  }
  if (output.length === 0) output.push(" ");
  return output;
}

/** UTF-16 ranges, in one rendered line, of the matches the core found for the visible lines. */
export function lineHighlights(
  text: string, sourceByteWidths: number[], lineStart: bigint, lineEnd: bigint,
  matches: ReadonlyArray<{ start: bigint; end: bigint }>,
): Array<{ start: number; end: number }> {
  const ranges: Array<{ start: number; end: number }> = [];
  for (const match of matches) {
    if (match.end <= lineStart || match.start >= lineEnd) continue;
    const from = match.start > lineStart ? Number(match.start - lineStart) : 0;
    const to = Number((match.end < lineEnd ? match.end : lineEnd) - lineStart);
    const start = utf16IndexForByte(text, sourceByteWidths, from);
    const end = utf16IndexForByte(text, sourceByteWidths, to);
    if (end > start) ranges.push({ start, end });
    if (ranges.length >= MAX_HIGHLIGHTS_PER_LINE) break;
  }
  return ranges;
}
