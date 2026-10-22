import type { ThemeRecord } from "./contracts";

/**
 * Page Setup values (`IMPROVEMENT_PLAN_3.md` §3.2), remembered on this
 * computer like the View options. `setup` is sent to the core as
 * `nulis_print::PageSetup`, which checks every value again.
 */

export type PaperSize = "a3" | "a4" | "a5" | "letter" | "legal";
export type Orientation = "portrait" | "landscape";
export type LongLines = "wrap" | "cut";
export type ColorMode = "print" | "blackAndWhite" | "theme";
export type MarginPreset = "narrow" | "normal" | "wide" | "custom";

export interface Margins { top: number; right: number; bottom: number; left: number }
export interface Band { left: string; center: string; right: string }

export interface PageSetup {
  paper: PaperSize;
  orientation: Orientation;
  margins: Margins;
  fontSize: number;
  lineNumbers: boolean;
  longLines: LongLines;
  syntaxHighlighting: boolean;
  tabWidth: number;
  header: Band;
  footer: Band;
}

export interface PrintSettings {
  setup: PageSetup;
  colors: ColorMode;
}

export const PAPER_SIZES: ReadonlyArray<{ id: PaperSize; label: string; widthMm: number; heightMm: number }> = [
  { id: "a4", label: "A4", widthMm: 210, heightMm: 297 },
  { id: "letter", label: "Letter", widthMm: 215.9, heightMm: 279.4 },
  { id: "legal", label: "Legal", widthMm: 215.9, heightMm: 355.6 },
  { id: "a3", label: "A3", widthMm: 297, heightMm: 420 },
  { id: "a5", label: "A5", widthMm: 148, heightMm: 210 },
];

/** Margin presets in millimetres, the same on every side. */
export const MARGIN_PRESETS: Record<Exclude<MarginPreset, "custom">, number> = { narrow: 10, normal: 15, wide: 25 };

export const FONT_SIZE_RANGE = { min: 6, max: 24 } as const;
export const MARGIN_RANGE = { min: 0, max: 80 } as const;
/** Longest header or footer part, in bytes (`MAX_TEMPLATE_BYTES` in nulis-print). */
export const MAX_BAND_BYTES = 256;
/** Fields a header or footer may use. */
export const BAND_FIELDS = ["{file}", "{path}", "{date}", "{page}", "{pages}"] as const;

/** Letter paper in the United States, A4 elsewhere (plan §3.2). */
export function defaultPaper(language: string = typeof navigator === "undefined" ? "" : navigator.language): PaperSize {
  return language === "en-US" ? "letter" : "a4";
}

export function defaultPrintSettings(language?: string): PrintSettings {
  return {
    setup: {
      paper: defaultPaper(language),
      orientation: "portrait",
      margins: { top: 15, right: 15, bottom: 15, left: 15 },
      fontSize: 9,
      lineNumbers: true,
      longLines: "wrap",
      syntaxHighlighting: true,
      tabWidth: 4,
      header: { left: "{file}", center: "", right: "{date}" },
      footer: { left: "", center: "Page {page} of {pages}", right: "" },
    },
    colors: "print",
  };
}

/** Which preset the margins match, or `custom`. */
export function marginPreset(margins: Margins): MarginPreset {
  const sides = [margins.top, margins.right, margins.bottom, margins.left];
  for (const [preset, size] of Object.entries(MARGIN_PRESETS) as Array<[Exclude<MarginPreset, "custom">, number]>) {
    if (sides.every((side) => side === size)) return preset;
  }
  return "custom";
}

function clamp(value: unknown, fallback: number, min: number, max: number, step = 0.5): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, Math.round(value / step) * step));
}

/** One line of at most {@link MAX_BAND_BYTES} bytes, without control characters. */
export function cleanBandText(value: unknown): string {
  if (typeof value !== "string") return "";
  // eslint-disable-next-line no-control-regex -- control characters are what this removes.
  let text = value.replace(/[\u0000-\u001f\u007f-\u009f]/g, "");
  const encoder = new TextEncoder();
  while (encoder.encode(text).length > MAX_BAND_BYTES) text = Array.from(text).slice(0, -1).join("");
  return text;
}

function band(value: unknown, fallback: Band): Band {
  if (!value || typeof value !== "object") return fallback;
  const candidate = value as Partial<Record<keyof Band, unknown>>;
  return {
    left: cleanBandText(candidate.left ?? fallback.left),
    center: cleanBandText(candidate.center ?? fallback.center),
    right: cleanBandText(candidate.right ?? fallback.right),
  };
}

/** Settings from storage or an older version, with every value in range. */
export function normalizePrintSettings(value: unknown, language?: string): PrintSettings {
  const defaults = defaultPrintSettings(language);
  if (!value || typeof value !== "object") return defaults;
  const candidate = value as { setup?: Partial<Record<keyof PageSetup, unknown>>; colors?: unknown };
  const setup = candidate.setup && typeof candidate.setup === "object" ? candidate.setup : {};
  const fallback = defaults.setup;
  const margins = setup.margins && typeof setup.margins === "object" ? setup.margins as Partial<Record<keyof Margins, unknown>> : {};
  const margin = (side: keyof Margins) => clamp(margins[side], fallback.margins[side], MARGIN_RANGE.min, MARGIN_RANGE.max);
  return {
    setup: {
      paper: PAPER_SIZES.some(({ id }) => id === setup.paper) ? setup.paper as PaperSize : fallback.paper,
      orientation: setup.orientation === "landscape" ? "landscape" : "portrait",
      margins: { top: margin("top"), right: margin("right"), bottom: margin("bottom"), left: margin("left") },
      fontSize: clamp(setup.fontSize, fallback.fontSize, FONT_SIZE_RANGE.min, FONT_SIZE_RANGE.max),
      lineNumbers: typeof setup.lineNumbers === "boolean" ? setup.lineNumbers : fallback.lineNumbers,
      longLines: setup.longLines === "cut" ? "cut" : "wrap",
      syntaxHighlighting: typeof setup.syntaxHighlighting === "boolean" ? setup.syntaxHighlighting : fallback.syntaxHighlighting,
      tabWidth: Math.round(clamp(setup.tabWidth, fallback.tabWidth, 1, 16, 1)),
      header: band(setup.header, fallback.header),
      footer: band(setup.footer, fallback.footer),
    },
    colors: candidate.colors === "blackAndWhite" || candidate.colors === "theme" ? candidate.colors : "print",
  };
}

const STORAGE_KEY = "nulis.printSettings.v1";

export function loadPrintSettings(): PrintSettings {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    return stored ? normalizePrintSettings(JSON.parse(stored)) : defaultPrintSettings();
  } catch {
    return defaultPrintSettings();
  }
}

export function savePrintSettings(settings: PrintSettings): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(normalizePrintSettings(settings)));
  } catch {
    // The settings still apply for this session.
  }
}

/** Theme keys the print palette reads (`Palette::from_theme` in nulis-print). */
const THEME_PRINT_KEY = /^(editor|syntax)\./;

/**
 * The `colors` argument for the core's print and export commands. The
 * current theme sends its editor and syntax colors.
 */
export function colorChoice(mode: ColorMode, theme: ThemeRecord | undefined): Record<string, unknown> {
  if (mode !== "theme" || !theme) return { kind: mode === "blackAndWhite" ? "blackAndWhite" : "print" };
  const colors = Object.fromEntries(Object.entries(theme.colors).filter(([key]) => THEME_PRINT_KEY.test(key)).slice(0, 128));
  return { kind: "theme", colors };
}
