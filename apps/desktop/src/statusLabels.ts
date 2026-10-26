/** Status bar wording: encodings, sizes and counts, code points. */
import type { DocumentSummary } from "./contracts";

export function encodingLabel(document: DocumentSummary | undefined): string {
  return document?.encoding === "windows1252" ? "Windows-1252" : "UTF-8";
}

/** Groups the digits of a decimal string of any length: 1234567 → 1,234,567. */
export function groupDigits(value: string): string {
  return /^\d+$/.test(value) ? value.replace(/\B(?=(\d{3})+(?!\d))/g, ",") : value;
}

/** "1,024 bytes · 1,000 characters · 180 words · 12 lines"; counts are left out when not counted. */
export function sizeLabel(byteLen: string, count?: { characters?: string; words?: string; lines?: string }): string {
  const part = (value: string | undefined, one: string, many: string) =>
    value === undefined ? [] : [`${groupDigits(value)} ${value === "1" ? one : many}`];
  return [
    `${groupDigits(byteLen)} ${byteLen === "1" ? "byte" : "bytes"}`,
    ...part(count?.characters, "character", "characters"),
    ...part(count?.words, "word", "words"),
    ...part(count?.lines, "line", "lines"),
  ].join(" · ");
}

/** "U+00E9" for the status bar. */
export function codePointLabel(codePoint: number): string {
  return `U+${codePoint.toString(16).toUpperCase().padStart(4, "0")}`;
}
