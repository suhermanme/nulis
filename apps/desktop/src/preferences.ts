import emberTheme from "../../../themes/nulis-ember-dark.nulis-theme.json";
import paperTheme from "../../../themes/nulis-paper-light.nulis-theme.json";
import nvchadCatalog from "../../../themes/nvchad-catalog.json";
import type { EditorPreferences, ThemeRecord } from "./contracts";

export type { EditorPreferences } from "./contracts";

export const DEFAULT_EDITOR_PREFERENCES: EditorPreferences = {
  theme: "nulis.ember-dark",
  fontFamily: 'ui-monospace, "SFMono-Regular", Consolas, "Liberation Mono", monospace',
  fontSize: 13,
  lineHeight: 1.7,
  fontLigatures: true,
};

export const FONT_FAMILIES = [
  { label: "System Monospace", value: DEFAULT_EDITOR_PREFERENCES.fontFamily },
  { label: "SF Mono", value: '"SF Mono", "SFMono-Regular", monospace' },
  { label: "Menlo", value: 'Menlo, Monaco, monospace' },
  { label: "Consolas", value: 'Consolas, "Liberation Mono", monospace' },
  { label: "Cascadia Code", value: '"Cascadia Code", Consolas, monospace' },
  { label: "JetBrains Mono", value: '"JetBrains Mono", monospace' },
  { label: "Fira Code", value: '"Fira Code", monospace' },
  { label: "Courier New", value: '"Courier New", monospace' },
] as const;

export const BUILT_IN_THEMES: ThemeRecord[] = [
  { id: emberTheme.id, name: emberTheme.name, version: emberTheme.version, kind: "dark", colors: emberTheme.colors, builtIn: true },
  { id: paperTheme.id, name: paperTheme.name, version: paperTheme.version, kind: "light", colors: paperTheme.colors, builtIn: true },
  ...nvchadCatalog.map((theme) => ({
    id: theme.id,
    name: theme.name,
    version: theme.version,
    kind: theme.kind as ThemeRecord["kind"],
    colors: theme.colors,
    builtIn: true,
  })),
];

const STORAGE_KEY = "nulis.editorPreferences.v1";

function boundedNumber(value: unknown, fallback: number, minimum: number, maximum: number): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(maximum, Math.max(minimum, value))
    : fallback;
}

export function normalizeEditorPreferences(value: unknown, themes: readonly ThemeRecord[] = BUILT_IN_THEMES): EditorPreferences {
  if (!value || typeof value !== "object") return DEFAULT_EDITOR_PREFERENCES;
  const candidate = value as Partial<EditorPreferences>;
  const theme = themes.some((item) => item.id === candidate.theme)
    ? candidate.theme as string
    : DEFAULT_EDITOR_PREFERENCES.theme;
  const knownFont = FONT_FAMILIES.find((item) => item.value === candidate.fontFamily)?.value;
  return {
    theme,
    fontFamily: knownFont ?? DEFAULT_EDITOR_PREFERENCES.fontFamily,
    fontSize: boundedNumber(candidate.fontSize, DEFAULT_EDITOR_PREFERENCES.fontSize, 8, 40),
    lineHeight: boundedNumber(candidate.lineHeight, DEFAULT_EDITOR_PREFERENCES.lineHeight, 1, 2.5),
    fontLigatures: typeof candidate.fontLigatures === "boolean"
      ? candidate.fontLigatures
      : DEFAULT_EDITOR_PREFERENCES.fontLigatures,
  };
}

export function loadEditorPreferences(): EditorPreferences {
  try {
    return normalizeEditorPreferences(JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "null"));
  } catch {
    return DEFAULT_EDITOR_PREFERENCES;
  }
}

export function saveEditorPreferences(preferences: EditorPreferences, themes: readonly ThemeRecord[] = BUILT_IN_THEMES): void {
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(normalizeEditorPreferences(preferences, themes)));
}

const LINE_CHANGE_KEY = "nulis.lineChangeIndicators.v1";

/** Persistent global default for the line change indicator lane. */
export function loadLineChangeDefault(): boolean {
  try {
    return window.localStorage.getItem(LINE_CHANGE_KEY) !== "false";
  } catch {
    return true;
  }
}

export function saveLineChangeDefault(enabled: boolean): void {
  try {
    window.localStorage.setItem(LINE_CHANGE_KEY, String(enabled));
  } catch {
    // The indicator still follows the in-memory choice for this session.
  }
}

const VIEW_OPTIONS_KEY = "nulis.viewOptions.v1";

/** Editor display choices from the View menu, remembered on this computer. */
export interface ViewOptions {
  wordWrap: boolean;
  /** Font zoom in percent, one of {@link ZOOM_LEVELS}. */
  zoom: number;
  showWhitespace: boolean;
  showLineEndings: boolean;
  lineNumbers: boolean;
  columnMarker: boolean;
  /** Column the marker is drawn after. */
  markerColumn: number;
}

export const ZOOM_LEVELS = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300] as const;

export const DEFAULT_VIEW_OPTIONS: ViewOptions = {
  wordWrap: false, zoom: 100, showWhitespace: false, showLineEndings: false,
  lineNumbers: true, columnMarker: false, markerColumn: 80,
};

export function normalizeViewOptions(value: unknown): ViewOptions {
  const candidate = typeof value === "object" && value !== null ? value as Partial<Record<keyof ViewOptions, unknown>> : {};
  const flag = (key: keyof ViewOptions) => typeof candidate[key] === "boolean"
    ? candidate[key] as boolean : DEFAULT_VIEW_OPTIONS[key] as boolean;
  const zoom = ZOOM_LEVELS.find((level) => level === candidate.zoom) ?? DEFAULT_VIEW_OPTIONS.zoom;
  const column = Number.isInteger(candidate.markerColumn) ? Number(candidate.markerColumn) : DEFAULT_VIEW_OPTIONS.markerColumn;
  return {
    wordWrap: flag("wordWrap"), zoom, showWhitespace: flag("showWhitespace"), showLineEndings: flag("showLineEndings"),
    lineNumbers: flag("lineNumbers"), columnMarker: flag("columnMarker"),
    markerColumn: Math.max(1, Math.min(500, column)),
  };
}

export function loadViewOptions(): ViewOptions {
  try {
    const stored = window.localStorage.getItem(VIEW_OPTIONS_KEY);
    return stored ? normalizeViewOptions(JSON.parse(stored)) : DEFAULT_VIEW_OPTIONS;
  } catch {
    return DEFAULT_VIEW_OPTIONS;
  }
}

export function saveViewOptions(options: ViewOptions): void {
  try {
    window.localStorage.setItem(VIEW_OPTIONS_KEY, JSON.stringify(normalizeViewOptions(options)));
  } catch {
    // The choice still applies for this session.
  }
}

/** The next zoom level in a direction, or 100% for `0`. */
export function stepZoom(zoom: number, direction: -1 | 0 | 1): number {
  if (direction === 0) return 100;
  const index = ZOOM_LEVELS.findIndex((level) => level === zoom);
  const current = index < 0 ? ZOOM_LEVELS.indexOf(100) : index;
  return ZOOM_LEVELS[Math.max(0, Math.min(ZOOM_LEVELS.length - 1, current + direction))];
}

const WELCOME_KEY = "nulis.welcomeOnStartup.v1";

/** Whether the welcome screen is shown when the application starts without open documents. */
export function loadWelcomeOnStartup(): boolean {
  try {
    return window.localStorage.getItem(WELCOME_KEY) !== "false";
  } catch {
    return true;
  }
}

export function saveWelcomeOnStartup(enabled: boolean): void {
  try {
    window.localStorage.setItem(WELCOME_KEY, String(enabled));
  } catch {
    // The choice still applies to this session.
  }
}

export function preferenceCssVariables(preferences: EditorPreferences, themes: readonly ThemeRecord[] = BUILT_IN_THEMES): Record<string, string> {
  const theme = themes.find((item) => item.id === preferences.theme)
    ?? BUILT_IN_THEMES[0];
  const colors = theme.colors;
  return {
    "--bg": colors["ui.background"],
    "--surface": colors["ui.surface"],
    "--panel": colors["ui.panel"],
    "--raised": colors["ui.raised"],
    "--menu": colors["ui.menu"],
    "--border": colors["ui.border"],
    "--muted": colors["ui.mutedText"],
    "--accent": colors["ui.accent"],
    "--accent-strong": colors["editor.caret"],
    "--accent-fg": colors["ui.accentForeground"],
    "--focus": colors["ui.focus"],
    "--danger": colors["ui.danger"],
    "--warning": colors["ui.warning"],
    "--success": colors["ui.success"],
    "--ui-text": colors["ui.text"],
    "--toolbar": colors["ui.toolbar"],
    "--tab": colors["ui.tab"],
    "--statusbar": colors["ui.statusBar"],
    "--statusbar-text": colors["ui.statusBarText"],
    "--input-bg": colors["ui.inputBackground"],
    "--input-border": colors["ui.inputBorder"],
    "--hover": colors["ui.hover"],
    "--overlay": colors["ui.overlay"],
    "--editor-bg": colors["editor.background"],
    "--editor-fg": colors["editor.foreground"],
    "--editor-selection": colors["editor.selection"],
    "--editor-current-line": colors["editor.currentLine"],
    "--editor-line-number": colors["editor.lineNumber"],
    "--editor-caret": colors["editor.caret"],
    "--editor-gutter": colors["editor.gutter"],
    "--editor-focus": colors["editor.focus"],
    "--syntax-comment": colors["syntax.comment"],
    "--syntax-keyword": colors["syntax.keyword"],
    "--syntax-string": colors["syntax.string"],
    "--syntax-number": colors["syntax.number"],
    "--syntax-function": colors["syntax.function"],
    "--syntax-type": colors["syntax.type"],
    "--syntax-property": colors["syntax.property"],
    "--syntax-escape": colors["syntax.escape"],
    "--syntax-invalid": colors["syntax.invalid"],
    "--terminal-bg": colors["terminal.background"],
    "--terminal-fg": colors["terminal.foreground"],
    "--terminal-cursor": colors["terminal.cursor"],
    "--diff-inserted": colors["diff.inserted"],
    "--diff-deleted": colors["diff.deleted"],
    "--diff-modified": colors["diff.modified"],
    "--lci-unsaved": colors["ui.danger"],
    "--lci-saved": colors["ui.success"],
    "--lci-deleted": colors["ui.danger"],
    "--editor-font-family": preferences.fontFamily,
    "--editor-font-size": `${preferences.fontSize}px`,
    "--editor-line-height": String(preferences.lineHeight),
    "--editor-font-ligatures": preferences.fontLigatures ? "normal" : "none",
    colorScheme: theme.kind,
  };
}
