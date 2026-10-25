import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  ApplicationInfo,
  CommandDescriptor,
  CustomizationDocument,
  CustomizationState,
  EditorPreferences,
  CapabilityKind,
  LanguageCapabilities,
  LanguageConflict,
  LanguageDescriptor,
  LanguageServersState,
  SettingsState,
  PluginSummary,
  ThemeInput,
  ThemeRecord,
} from "./contracts";
import { CUSTOMIZABLE_COMMANDS } from "./commands";
import { Icon, IconButton, type IconName } from "./icons";
import { suggestSettingsOf } from "./contracts";
import { executeCommand, getApplicationInfo, listCommands } from "./ipc";
import { LAYOUT_LIMITS } from "./layout";
import { exportWindowTheme, useShowWhenReady } from "./windowTheme";
import {
  BUILT_IN_THEMES,
  FONT_FAMILIES,
  loadLineChangeDefault,
  loadWelcomeOnStartup,
  normalizeEditorPreferences,
  preferenceCssVariables,
  previewCssVariables,
  type WindowThemePayload,
  saveLineChangeDefault,
  saveWelcomeOnStartup,
} from "./preferences";

export const SETTINGS_SECTIONS = [
  { id: "general", title: "General", icon: "general" },
  { id: "editor", title: "Editor", icon: "editor" },
  { id: "appearance", title: "Appearance & Themes", icon: "appearance" },
  { id: "languages", title: "Languages", icon: "language" },
  { id: "keybindings", title: "Keybindings", icon: "keyboard" },
  { id: "menus", title: "Menus & Toolbar", icon: "menus" },
  { id: "workspace", title: "Workspace", icon: "files" },
  { id: "extensions", title: "Extensions & Security", icon: "security" },
] as const satisfies ReadonlyArray<{ id: string; title: string; icon: IconName }>;

export type SettingsSectionId = typeof SETTINGS_SECTIONS[number]["id"];
export type SettingsMainAction = "workspace.rename" | "workbench.openPlugins";

const SECTION_KEY = "nulis.settings.section.v1";
const CONTEXT = { windowId: "main" } as const;

interface SettingsWindowProps {
  /** `window` is the standalone desktop window; `inline` is the browser fallback. */
  mode: "window" | "inline";
  /** Live appearance preview; null restores the saved appearance. */
  onPreview(appearance: EditorPreferences | null): void;
  onSaved(state: CustomizationState, themes: ThemeRecord[]): void;
  onMainAction(action: SettingsMainAction): void;
  onClose(): void;
  /** Increment when the host asks to close (window close button). */
  closeSignal?: number;
  /** Increment when another window stored a newer customization. */
  externalChangeSignal?: number;
  /** Called after a language server is approved, revoked, or restarted. */
  onLanguageServersChanged?(): void;
}

function loadSection(): SettingsSectionId {
  try {
    const stored = window.localStorage.getItem(SECTION_KEY);
    return SETTINGS_SECTIONS.find((section) => section.id === stored)?.id ?? "general";
  } catch {
    return "general";
  }
}

/** How each capability is named next to its switch. */
const FEATURE_NAMES: Partial<Record<CapabilityKind, string>> = {
  diagnostics: "Validate", format: "Beautify", compact: "Minify", preview: "Preview", indentation: "Auto-indent rules",
  completion: "Suggestions",
};

/** The `editor` indentation values of settings.json, with Nulis's defaults. */
interface IndentationDraft {
  autoIndent: "full" | "keep" | "none";
  insertSpaces: boolean;
  tabSize: number;
  detectIndentation: boolean;
  /** `editor.suggest.auto` and `editor.suggest.acceptOnEnter`. */
  suggestAuto: boolean;
  suggestAcceptOnEnter: boolean;
}

function indentationOf(state: SettingsState | null): IndentationDraft {
  const editor = state?.document.values.editor;
  const values = typeof editor === "object" && editor !== null ? editor as Record<string, unknown> : {};
  const autoIndent = values.autoIndent === "keep" || values.autoIndent === "none" ? values.autoIndent : "full";
  const tabSize = typeof values.tabSize === "number" ? Math.min(16, Math.max(1, Math.round(values.tabSize))) : 4;
  const suggest = suggestSettingsOf(state?.document.values);
  return {
    autoIndent,
    insertSpaces: values.insertSpaces === true,
    tabSize,
    detectIndentation: values.detectIndentation !== false,
    suggestAuto: suggest.auto,
    suggestAcceptOnEnter: suggest.acceptOnEnter,
  };
}

/** Writes the indentation draft into settings.json values, keeping other editor values. */
function withIndentation(values: Record<string, unknown>, draft: IndentationDraft): Record<string, unknown> {
  const editor = typeof values.editor === "object" && values.editor !== null ? values.editor as Record<string, unknown> : {};
  const suggest = typeof editor.suggest === "object" && editor.suggest !== null ? editor.suggest as Record<string, unknown> : {};
  const { suggestAuto, suggestAcceptOnEnter, ...indentation } = draft;
  return {
    ...values,
    editor: { ...editor, ...indentation, suggest: { ...suggest, auto: suggestAuto, acceptOnEnter: suggestAcceptOnEnter } },
  };
}

type FeatureSwitches = Record<string, Partial<Record<CapabilityKind, boolean>>>;

function switchesOf(rows: readonly LanguageCapabilities[]): FeatureSwitches {
  return Object.fromEntries(rows.map((row) => [row.id,
    Object.fromEntries(row.capabilities.map((capability) => [capability.kind, capability.enabled]))]));
}

/**
 * Writes the switches into settings.json values as each language's
 * `"[id]": { "disabledCapabilities": [...] }`, keeping everything else.
 */
function withSwitches(values: Record<string, unknown>, switches: FeatureSwitches,
  rows: readonly LanguageCapabilities[]): Record<string, unknown> {
  const next: Record<string, unknown> = { ...values };
  for (const row of rows) {
    const key = `[${row.id}]`;
    const section: Record<string, unknown> = typeof next[key] === "object" && next[key] !== null
      ? { ...(next[key] as Record<string, unknown>) } : {};
    const listed = row.capabilities.map((capability) => capability.kind as string);
    // Kinds this list does not show (another provider's, or unknown) are kept as they were.
    const kept = Array.isArray(section.disabledCapabilities)
      ? section.disabledCapabilities.filter((kind): kind is string => typeof kind === "string" && !listed.includes(kind)) : [];
    const disabled = [...kept, ...row.capabilities
      .filter((capability) => switches[row.id]?.[capability.kind] === false).map((capability) => capability.kind)];
    if (disabled.length > 0) section.disabledCapabilities = disabled;
    else delete section.disabledCapabilities;
    if (Object.keys(section).length > 0) next[key] = section;
    else delete next[key];
  }
  return next;
}

function fileAssociations(state: SettingsState | null): [string, string][] {
  const files = state?.document.values.files;
  const associations = typeof files === "object" && files !== null
    ? (files as { associations?: unknown }).associations : undefined;
  return typeof associations === "object" && associations !== null
    ? Object.entries(associations).filter((entry): entry is [string, string] => typeof entry[1] === "string") : [];
}

function same(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

/** Writes the user's conflict choices into `files.associations`, keeping others. */
function withAssociations(
  values: Record<string, unknown>, choices: Record<string, string>,
): Record<string, unknown> {
  if (Object.keys(choices).length === 0) return values;
  const files = typeof values.files === "object" && values.files !== null
    ? values.files as Record<string, unknown> : {};
  const associations = typeof files.associations === "object" && files.associations !== null
    ? files.associations as Record<string, unknown> : {};
  return { ...values, files: { ...files, associations: { ...associations, ...choices } } };
}

function message(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : typeof error === "string" ? error : fallback;
}

function move<T>(values: T[], index: number, offset: -1 | 1): T[] {
  const target = index + offset;
  if (target < 0 || target >= values.length) return values;
  const next = [...values];
  [next[index], next[target]] = [next[target], next[index]];
  return next;
}

const THEME_HEX = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;

/** The six-digit form a theme color needs for the native color picker. */
export function expandedThemeColor(color: string): string | null {
  if (!THEME_HEX.test(color)) return null;
  return color.length === 4
    ? `#${color.slice(1).split("").map((channel) => channel + channel).join("")}`
    : color;
}

/** The core's theme name limit in bytes; a longer name is rejected on save. */
export const MAX_THEME_NAME = 128;

/** Whether `name` is a theme name the core accepts: non-empty once trimmed, within the byte limit. */
export function themeNameIsUsable(name: string): boolean {
  const trimmed = name.trim();
  return trimmed.length > 0 && new TextEncoder().encode(trimmed).length <= MAX_THEME_NAME;
}

/** A theme id for a copy of `base` that no theme in `taken` already uses. */
export function copiedThemeId(base: string, taken: readonly string[]): string {
  // The id must pass the core's rules: lowercase letters, digits, dashes, and dots between groups.
  const slug =
    base
      .trim()
      .toLowerCase()
      .replace(/\.+/g, ".")
      .replace(/[^a-z0-9.-]+/g, "-")
      .slice(0, 100)
      .replace(/^[-.]+|[-.]+$/g, "") || "theme";
  const stem = slug.includes(".") ? slug : `${slug}.1`;
  let candidate = `${stem}.copy`;
  for (let suffix = 2; taken.includes(candidate); suffix += 1) candidate = `${stem}.copy-${suffix}`;
  return candidate;
}

/** A theme name for a copy of `base` that no theme in `taken` already displays. */
export function copiedThemeName(base: string, taken: readonly string[]): string {
  // The name is what the picker shows, so the suffix counts display names, not ids.
  let candidate = `${base.trim()} copy`;
  for (let suffix = 2; taken.includes(candidate); suffix += 1) candidate = `${base.trim()} copy ${suffix}`;
  return candidate;
}

/** The colors the Appearance preview paints: the draft theme's colors, ignoring hex the user is still typing. */
export function previewColors(theme: ThemeRecord | null, colorDraft: Record<string, string>): Record<string, string> {
  if (!theme) return {};
  const colors = { ...theme.colors };
  for (const [token, color] of Object.entries(colorDraft)) if (THEME_HEX.test(color)) colors[token] = color;
  return colors;
}

/** What the core would paint for a drafted palette: `theme.readable`'s reply, floors applied. */
export type ReadableThemeReply = {
  colors: Record<string, string>;
  changes: { token: string; from: string; to: string }[];
};

/** A readable caption for a theme token, shown as the tooltip of the Appearance fields. */
export function themeTokenLabel(token: string): string {
  return token.split(".").map((word) => `${word.charAt(0).toUpperCase()}${word.slice(1)}`).join(" ");
}

/** The color one arrow-key nudge produces in a hex field: plain arrows ±1 on blue, Shift ±1 on red, Alt ±1 on green; null when the text is not a color. */
export function nudgedThemeColor(color: string, key: string, shift: boolean, alt: boolean): string | null {
  const expanded = expandedThemeColor(color);
  if (!expanded) return null;
  const channel = shift ? 0 : alt ? 1 : 2;
  const step = key === "ArrowUp" || key === "ArrowRight" ? 1 : -1;
  const channels = [1, 3, 5].map((start) => Number.parseInt(expanded.slice(start, start + 2), 16));
  channels[channel] = Math.min(255, Math.max(0, channels[channel] + step));
  return `#${channels.map((value) => value.toString(16).padStart(2, "0")).join("")}`;
}

/** Separate, sectioned settings surface with a draft that is applied or discarded as a whole. */
export function SettingsWindow({
  mode, onPreview, onSaved, onMainAction, onClose, closeSignal = 0, externalChangeSignal = 0,
  onLanguageServersChanged,
}: SettingsWindowProps) {
  const [section, setSection] = useState<SettingsSectionId>(loadSection);
  const [saved, setSaved] = useState<CustomizationState | null>(null);
  const [draft, setDraft] = useState<CustomizationDocument | null>(null);
  const [themes, setThemes] = useState<ThemeRecord[]>(BUILT_IN_THEMES);
  const [commands, setCommands] = useState<CommandDescriptor[]>([]);
  const [languages, setLanguages] = useState<LanguageDescriptor[]>([]);
  const [settingsState, setSettingsState] = useState<SettingsState | null>(null);
  const [featureRows, setFeatureRows] = useState<LanguageCapabilities[]>([]);
  const [featureDraft, setFeatureDraft] = useState<FeatureSwitches>({});
  const [indentDraft, setIndentDraft] = useState<IndentationDraft>(() => indentationOf(null));
  const [colorDraft, setColorDraft] = useState<Record<string, string>>({});
  const [shown, setShown] = useState<Record<string, string> | null>(null);
  const [themeDraft, setThemeDraft] = useState<ThemeRecord | null>(null);
  const [themeNameDraft, setThemeNameDraft] = useState<string | null>(null);
  const [plugins, setPlugins] = useState<PluginSummary[]>([]);
  const [conflicts, setConflicts] = useState<LanguageConflict[]>([]);
  const [associationDraft, setAssociationDraft] = useState<Record<string, string>>({});
  const [servers, setServers] = useState<LanguageServersState | null>(null);
  const [serverBusy, setServerBusy] = useState<string | null>(null);
  const [info, setInfo] = useState<ApplicationInfo | null>(null);
  const [savedLineChanges, setSavedLineChanges] = useState(loadLineChangeDefault);
  const [welcome, setWelcome] = useState(loadWelcomeOnStartup);
  const [savedWelcome, setSavedWelcome] = useState(loadWelcomeOnStartup);
  const [lineChanges, setLineChanges] = useState(loadLineChangeDefault);
  const [filter, setFilter] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const [confirm, setConfirm] = useState<"close" | "reset" | "remove" | null>(null);
  const signals = useRef({ close: closeSignal, external: externalChangeSignal });

  const loadLanguageSettings = useCallback(async () => {
    const [state, rows, conflictRows] = await Promise.all([
      executeCommand<SettingsState>({ id: "settings.state", context: CONTEXT, arguments: {} }),
      executeCommand<LanguageCapabilities[]>({ id: "language.capabilities", context: CONTEXT, arguments: {} }),
      executeCommand<LanguageConflict[]>({ id: "language.conflicts", context: CONTEXT, arguments: {} }).catch(() => []),
    ]);
    const nextRows = Array.isArray(rows) ? rows : [];
    setSettingsState(state ?? null);
    setFeatureRows(nextRows);
    setFeatureDraft(switchesOf(nextRows));
    setIndentDraft(indentationOf(state ?? null));
    setConflicts(Array.isArray(conflictRows) ? conflictRows : []);
  }, []);

  const loadLanguageServers = useCallback(async () => {
    const state = await executeCommand<LanguageServersState | null>({
      id: "lsp.state", context: CONTEXT, arguments: {},
    });
    setServers(state && Array.isArray(state.servers) ? state : null);
  }, []);

  const loadState = useCallback(async () => {
    const [state, nextThemes] = await Promise.all([
      executeCommand<CustomizationState>({ id: "customization.state", context: CONTEXT, arguments: {} }),
      executeCommand<ThemeRecord[]>({ id: "theme.list", context: CONTEXT, arguments: {} }).catch(() => [...BUILT_IN_THEMES]),
    ]);
    const loadedThemes = Array.isArray(nextThemes) && nextThemes.length > 0 ? nextThemes : [...BUILT_IN_THEMES];
    setThemes(loadedThemes);
    setSaved(state);
    setDraft(state.document);
    setStale(false);
    return { state, themes: loadedThemes };
  }, []);

  useEffect(() => {
    let mounted = true;
    void loadState().catch((error: unknown) => { if (mounted) setNotice(message(error, "Settings are unavailable")); });
    void loadLanguageSettings().catch(() => undefined);
    void loadLanguageServers().catch(() => undefined);
    void listCommands().then((next) => { if (mounted && Array.isArray(next)) setCommands(next); }).catch(() => undefined);
    void executeCommand<LanguageDescriptor[]>({ id: "language.list", context: CONTEXT, arguments: {} })
      .then((next) => { if (mounted && Array.isArray(next)) setLanguages(next); }).catch(() => undefined);
    void executeCommand<PluginSummary[]>({ id: "plugin.list", context: CONTEXT, arguments: {} })
      .then((next) => { if (mounted && Array.isArray(next)) setPlugins(next); }).catch(() => undefined);
    void getApplicationInfo().then((next) => { if (mounted) setInfo(next); }).catch(() => undefined);
    return () => { mounted = false; };
  }, [loadLanguageServers, loadLanguageSettings, loadState]);

  // The welcome screen's own tick box changes this preference from the main window.
  useEffect(() => {
    const onStorage = () => {
      const next = loadWelcomeOnStartup();
      setSavedWelcome(next);
      setWelcome(next);
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  useEffect(() => {
    try { window.localStorage.setItem(SECTION_KEY, section); } catch { /* Navigation still works without storage. */ }
    setFilter("");
  }, [section]);

  const pendingSections = useMemo(() => {
    const pending = new Set<SettingsSectionId>();
    if (!saved || !draft) return pending;
    const before = saved.document;
    if (welcome !== savedWelcome) pending.add("general");
    if (before.appearance.theme !== draft.appearance.theme) pending.add("appearance");
    if (!same({ ...before.appearance, theme: "" }, { ...draft.appearance, theme: "" }) || lineChanges !== savedLineChanges
      || !same(indentDraft, indentationOf(settingsState))) pending.add("editor");
    if (!same(before.keybindings, draft.keybindings)) pending.add("keybindings");
    if (!same(before.menu, draft.menu) || !same(before.toolbar, draft.toolbar)) pending.add("menus");
    if (!same(featureDraft, switchesOf(featureRows)) || Object.keys(associationDraft).length > 0) pending.add("languages");
    return pending;
  }, [associationDraft, draft, featureDraft, featureRows, indentDraft, lineChanges, saved, savedLineChanges, savedWelcome, settingsState, welcome]);
  const pending = pendingSections.size > 0;

  const discard = useCallback(() => {
    if (saved) setDraft(saved.document);
    setFeatureDraft(switchesOf(featureRows));
    setIndentDraft(indentationOf(settingsState));
    setLineChanges(savedLineChanges);
    setWelcome(savedWelcome);
    setAssociationDraft({});
    setThemeDraft(null);
    setThemeNameDraft(null);
    setColorDraft({});
    onPreview(null);
  }, [featureRows, onPreview, saved, savedLineChanges, savedWelcome, settingsState]);

  const apply = useCallback(async (): Promise<boolean> => {
    if (!saved || !draft || busy) return false;
    setBusy(true);
    try {
      const next = same(saved.document, draft) ? saved : await executeCommand<CustomizationState>({
        id: "customization.saveChecked", context: CONTEXT,
        arguments: { expectedRevision: saved.revision, document: draft },
      });
      const featuresChanged = !same(featureDraft, switchesOf(featureRows));
      const indentationChanged = !same(indentDraft, indentationOf(settingsState));
      const associationsChanged = Object.keys(associationDraft).length > 0;
      if (settingsState && (featuresChanged || indentationChanged || associationsChanged)) {
        let values = settingsState.document.values;
        if (featuresChanged) values = withSwitches(values, featureDraft, featureRows);
        if (indentationChanged) values = withIndentation(values, indentDraft);
        if (associationsChanged) values = withAssociations(values, associationDraft);
        await executeCommand<SettingsState>({
          id: "settings.saveChecked", context: CONTEXT,
          arguments: { expectedRevision: settingsState.revision, document: { ...settingsState.document, values } },
        });
        await loadLanguageSettings();
        setAssociationDraft({});
      }
      saveLineChangeDefault(lineChanges);
      setSavedLineChanges(lineChanges);
      saveWelcomeOnStartup(welcome);
      setSavedWelcome(welcome);
      setSaved(next);
      setDraft(next.document);
      setNotice("Settings saved");
      onSaved(next, themes);
      return true;
    } catch (error: unknown) {
      const text = message(error, "Could not save settings");
      if (text.includes("changed elsewhere")) setStale(true);
      setNotice(text);
      return false;
    } finally {
      setBusy(false);
    }
  }, [associationDraft, busy, draft, featureDraft, featureRows, indentDraft, lineChanges, loadLanguageSettings, onSaved, saved, settingsState, themes, welcome]);

  const requestClose = useCallback(() => {
    if (pending) setConfirm("close");
    else { onPreview(null); onClose(); }
  }, [onClose, onPreview, pending]);

  useEffect(() => {
    if (closeSignal !== signals.current.close) {
      signals.current.close = closeSignal;
      requestClose();
    }
  }, [closeSignal, requestClose]);

  useEffect(() => {
    if (externalChangeSignal === signals.current.external) return;
    signals.current.external = externalChangeSignal;
    // A newer revision never replaces unapplied edits silently.
    if (pending) setStale(true);
    else {
      void loadState().catch(() => undefined);
      void loadLanguageSettings().catch(() => undefined);
      void loadLanguageServers().catch(() => undefined);
    }
  }, [externalChangeSignal, loadLanguageServers, loadLanguageSettings, loadState, pending]);

  const reload = () => {
    setBusy(true);
    void loadState().then(({ state, themes: nextThemes }) => {
      onPreview(null);
      onSaved(state, nextThemes);
      setNotice("Loaded the latest settings");
    }).catch((error: unknown) => setNotice(message(error, "Could not reload settings")))
      .finally(() => setBusy(false));
  };

  /** Approves, revokes, or restarts one configured language server. */
  const changeServer = async (id: string, command: "lsp.approve" | "lsp.revoke" | "lsp.restart") => {
    if (serverBusy) return;
    setServerBusy(id);
    try {
      const state = await executeCommand<LanguageServersState>({
        id: command, context: CONTEXT, arguments: { id },
      });
      if (state && Array.isArray(state.servers)) setServers(state);
      setNotice(command === "lsp.approve" ? `Approved ${id}`
        : command === "lsp.revoke" ? `Revoked ${id}` : `Restarted ${id}`);
      onLanguageServersChanged?.();
    } catch (error: unknown) {
      setNotice(message(error, "Could not change the language server"));
    } finally {
      setServerBusy(null);
    }
  };

  const restoreDefaults = async () => {
    setConfirm(null);
    setBusy(true);
    try {
      await executeCommand({ id: "customization.reset", context: CONTEXT, arguments: {} });
      saveLineChangeDefault(true);
      setSavedLineChanges(true);
      setLineChanges(true);
      saveWelcomeOnStartup(true);
      setSavedWelcome(true);
      setWelcome(true);
      const { state, themes: nextThemes } = await loadState();
      onPreview(null);
      onSaved(state, nextThemes);
      setNotice("Defaults restored");
    } catch (error: unknown) {
      setNotice(message(error, "Could not restore defaults"));
    } finally {
      setBusy(false);
    }
  };

  const installTheme = async () => {
    setBusy(true);
    try {
      const selected = await open({ multiple: false, directory: false, filters: [{ name: "Nulis or NvChad theme", extensions: ["json", "lua"] }] });
      if (typeof selected !== "string") return;
      const installed = await executeCommand<ThemeRecord>({ id: "theme.install", context: CONTEXT, arguments: { path: selected } });
      setThemes(await executeCommand<ThemeRecord[]>({ id: "theme.list", context: CONTEXT, arguments: {} }));
      setNotice(`Installed ${installed.name}`);
    } catch (error: unknown) {
      setNotice(message(error, "Could not install theme"));
    } finally {
      setBusy(false);
    }
  };

  const removeTheme = async (id: string) => {
    if (busy || !saved) return;
    setConfirm(null);
    const target = themes.find((theme) => theme.id === id);
    if (saved.document.appearance.theme === id) {
      setNotice(`${target?.name ?? id} is the theme in use. Apply another theme before removing it.`);
      return;
    }
    if (target?.builtIn) {
      setNotice(`${target.name} is built into Nulis; only your themes can be removed`);
      return;
    }
    setBusy(true);
    try {
      await executeCommand({ id: "theme.uninstall", context: CONTEXT, arguments: { id } });
      const nextThemes = await executeCommand<ThemeRecord[]>({ id: "theme.list", context: CONTEXT, arguments: {} });
      setThemes(nextThemes);
      onSaved(saved, nextThemes);
      setNotice("Theme removed");
    } catch (error: unknown) {
      setNotice(message(error, "Could not remove theme"));
    } finally {
      setBusy(false);
    }
  };

  const onNavigationKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = SETTINGS_SECTIONS.findIndex((item) => item.id === section);
    const target = event.key === "ArrowDown" ? index + 1 : event.key === "ArrowUp" ? index - 1
      : event.key === "Home" ? 0 : event.key === "End" ? SETTINGS_SECTIONS.length - 1 : -1;
    const next = SETTINGS_SECTIONS[target];
    if (!next) return;
    event.preventDefault();
    setSection(next.id);
    event.currentTarget.querySelector<HTMLElement>(`[data-section="${next.id}"]`)?.focus();
  };

  // The separate window is shown once its settings (or why they failed) are on screen.
  useShowWhenReady(mode === "window" && (Boolean(saved && draft) || notice !== null));

  const selectedTheme = saved && draft
    ? themes.find((theme) => theme.id === draft.appearance.theme)
    : undefined;
  const editableTheme = themeDraft ?? (selectedTheme && !selectedTheme.builtIn ? selectedTheme : null);
  const editedColors: Record<string, string> = editableTheme
    ? { ...editableTheme.colors, ...colorDraft }
    : {};
  const colorChanges = Object.keys(colorDraft).length;
  // The name is edited with the colors: only the trimmed name is stored, and an empty
  // name or one past the core's byte limit is what `theme.save` itself refuses.
  const themeName = (themeNameDraft ?? editableTheme?.name ?? "").trim();
  const nameIsUsable = themeNameIsUsable(themeName);
  const nameChanged = themeNameDraft !== null && themeName !== (editableTheme?.name ?? "");
  /** A duplicate, its color edits, and a new name are a draft; the store button writes them, Revert discards them. */
  const draftIsPending = themeDraft !== null || colorChanges > 0 || nameChanged;
  // The store button sits above the color list, where the name is edited, and names what it
  // writes — so the one action the section has is not labelled after only one of its fields.
  const storeAction = themeDraft ? "Save Copy"
    : nameChanged && colorChanges > 0 ? "Save Name & Colors"
    : nameChanged ? "Save Name"
    : "Save Colors";

  // Live preview paints what Nulis will paint, floors included, so the preview box and the
  // editor never disagree; discarding restores the saved theme.
  const draftColors = useMemo(() => previewColors(editableTheme, colorDraft), [editableTheme, colorDraft]);
  const previewing = themeDraft !== null || colorChanges > 0;
  const paintedColors = shown ?? { ...editableTheme?.colors, ...draftColors };
  const previewStyle = previewing ? (previewCssVariables(paintedColors) as CSSProperties) : undefined;

  // Picked colors, an unsaved duplicate, and a typed name belong to the theme that was open when they were made.
  useEffect(() => {
    setColorDraft({});
    setThemeDraft(null);
    setThemeNameDraft(null);
  }, [draft?.appearance.theme]);

  // Drafted colors export onto the window root for the live preview; closing the
  // panel or discarding the draft restores the committed theme appearance. The
  // preview path stores nothing: only Save Colors writes a theme.
  useEffect(() => {
    if (!previewing || !editableTheme) return undefined;
    const payload: WindowThemePayload = {
      theme: editableTheme.id,
      baseTheme: editableTheme.id,
      kind: editableTheme.kind,
      colors: paintedColors,
      customTheme: editableTheme.builtIn ? undefined : (editableTheme.custom ?? undefined),
      uiTokens: editableTheme.uiTokens,
    };
    return exportWindowTheme(window.document.documentElement, payload, editableTheme.colors);
  }, [previewing, editableTheme, paintedColors]);

  // The contrast floors live in the core, and they decide what the editor paints. The panel asks
  // the core what this drafted palette would show, so a color Nulis will strengthen is named
  // while it is picked — and the preview paints the same colors the editor will.
  useEffect(() => {
    if (!editableTheme) {
      setShown(null);
      return undefined;
    }
    const theme: ThemeInput = {
      id: editableTheme.id,
      name: themeName || editableTheme.name,
      version: themeDraft ? 1 : editableTheme.version,
      kind: editableTheme.kind,
      colors: draftColors,
    };
    let stale = false;
    const asked = window.setTimeout(() => {
      void executeCommand<ReadableThemeReply>({ id: "theme.readable", context: CONTEXT, arguments: { theme } })
        .then((reply) => { if (!stale) setShown(reply.colors); })
        .catch(() => { if (!stale) setShown(null); });
    }, 150);
    return () => {
      stale = true;
      window.clearTimeout(asked);
    };
  }, [draftColors, editableTheme, themeDraft, themeName]);

  if (!saved || !draft) {
    return <div className={`settings-window ${mode}`} role={mode === "inline" ? "dialog" : undefined} aria-label="Settings">
      <p className="settings-loading" role="status">{notice ?? "Loading settings…"}</p>
      {notice ? <button type="button" onClick={onClose}>Close</button> : null}
    </div>;
  }

  const updateAppearance = (change: Partial<EditorPreferences>, themeList: readonly ThemeRecord[] = themes) => {
    const appearance = normalizeEditorPreferences({ ...draft.appearance, ...change }, themeList);
    setDraft({ ...draft, appearance });
    onPreview(appearance);
  };


  /** Clones the selected theme into an editable draft; nothing is stored until Save Colors. */
  const duplicateTheme = () => {
    if (!selectedTheme) return;
    const id = copiedThemeId(selectedTheme.id, themes.map((theme) => theme.id));
    const name = copiedThemeName(selectedTheme.name, themes.map((theme) => theme.name));
    setThemeDraft({ ...selectedTheme, id, name, version: 1, builtIn: false });
    setColorDraft({});
    setNotice(`Editing a copy of "${selectedTheme.name}" as ${id}; Save Colors stores it`);
  };

  /** Stores the drafted theme: a duplicate, or the edited colors of the selected user theme. */
  const saveThemeColors = async () => {
    if (!editableTheme || !saved) return;
    if (Object.values(colorDraft).some((color) => !THEME_HEX.test(color))) {
      setNotice("Colors need #rgb or #rrggbb");
      return;
    }
    if (!nameIsUsable) {
      setNotice("Give the theme a name under 128 characters");
      return;
    }
    setBusy(true);
    try {
      // Only the authored fields travel: `theme.save` takes a theme file, and the built-in
      // flag and the readability floors belong to the server.
      const source = themeDraft ?? editableTheme;
      const theme: ThemeInput = {
        id: source.id,
        name: themeName,
        version: themeDraft ? 1 : source.version,
        kind: source.kind,
        colors: { ...source.colors, ...editedColors },
      };
      await executeCommand<ThemeRecord>({ id: "theme.save", context: CONTEXT, arguments: { theme } });
      const nextThemes = await executeCommand<ThemeRecord[]>({ id: "theme.list", context: CONTEXT, arguments: {} });
      const savedDraft = themeDraft;
      setThemes(nextThemes);
      setThemeDraft(null);
      setThemeNameDraft(null);
      setColorDraft({});
      if (savedDraft) updateAppearance({ theme: theme.id }, nextThemes);
      onSaved(saved, nextThemes);
      // The name is what the picker shows, so it is what the save confirms — for a rename
      // of a stored theme as much as for a fresh duplicate.
      setNotice(`Saved ${theme.name}`);
    } catch (error: unknown) {
      setNotice(message(error, "Could not save theme colors"));
    } finally {
      setBusy(false);
    }
  };
  const configurable = commands.filter((command) => CUSTOMIZABLE_COMMANDS.has(command.id)
    || command.category.startsWith("Plugin ·"));
  const title = (id: string) => commands.find((command) => command.id === id)?.title ?? id;
  const options = configurable.map((command) => <option key={command.id} value={command.id}>{command.title}</option>);
  const needle = filter.trim().toLocaleLowerCase();
  const matches = (text: string) => !needle || text.toLocaleLowerCase().includes(needle);
  const current = SETTINGS_SECTIONS.find((item) => item.id === section) ?? SETTINGS_SECTIONS[0];
  const filterField = (label: string) => <input className="settings-filter" type="search" aria-label={label}
    placeholder={label} value={filter} onChange={(event) => setFilter(event.currentTarget.value)} />;

  const panels: Record<SettingsSectionId, ReactNode> = {
    general: <>
      <fieldset className="settings-group"><legend>Application</legend>
        <dl className="settings-facts">
          <div><dt>Product</dt><dd>{info ? `${info.productName} ${info.version}` : "Nulis"}</dd></div>
          <div><dt>IPC contract</dt><dd>{info ? `v${info.ipcProtocolVersion}` : "—"}</dd></div>
          <div><dt>Customization revision</dt><dd>{saved.revision}</dd></div>
        </dl>
      </fieldset>
      <fieldset className="settings-group"><legend>Startup</legend>
        <label className="settings-check"><input aria-label="Show the welcome screen on startup" type="checkbox" checked={welcome}
          onChange={(event) => setWelcome(event.currentTarget.checked)} /><span>Show the welcome screen on startup</span></label>
        <p className="settings-help">When this is off, Nulis starts with a new empty document if no files are restored from the previous session.</p>
      </fieldset>
      <fieldset className="settings-group"><legend>Defaults</legend>
        <p className="settings-help">Restores appearance, keybindings, the File menu, the toolbar, and the welcome screen. Open documents and installed themes are not affected.</p>
        <button type="button" disabled={busy} onClick={() => setConfirm("reset")}>Restore Defaults…</button>
      </fieldset>
    </>,
    editor: <>
      <fieldset className="settings-group"><legend>Font</legend>
        <label><span>Editor font</span><select aria-label="Editor font" value={draft.appearance.fontFamily}
          onChange={(event) => updateAppearance({ fontFamily: event.currentTarget.value })}>
          {FONT_FAMILIES.map((font) => <option key={font.label} value={font.value}>{font.label}</option>)}</select></label>
        <div className="settings-number-row">
          <label><span>Font size</span><input aria-label="Editor font size" type="number" min="8" max="40" value={draft.appearance.fontSize}
            onChange={(event) => updateAppearance({ fontSize: event.currentTarget.valueAsNumber })} /></label>
          <label><span>Line height</span><input aria-label="Editor line height" type="number" min="1" max="2.5" step="0.1" value={draft.appearance.lineHeight}
            onChange={(event) => updateAppearance({ lineHeight: event.currentTarget.valueAsNumber })} /></label>
        </div>
        <label className="settings-check"><input aria-label="Font ligatures" type="checkbox" checked={draft.appearance.fontLigatures}
          onChange={(event) => updateAppearance({ fontLigatures: event.currentTarget.checked })} /><span>Enable font ligatures</span></label>
      </fieldset>
      <fieldset className="settings-group"><legend>Indentation</legend>
        <label><span>Auto-indent</span><select aria-label="Auto-indent" value={indentDraft.autoIndent}
          onChange={(event) => {
            const autoIndent = event.currentTarget.value as IndentationDraft["autoIndent"];
            setIndentDraft((current) => ({ ...current, autoIndent }));
          }}>
          <option value="full">Follow the language</option>
          <option value="keep">Keep the line's indentation</option>
          <option value="none">Off</option>
        </select></label>
        <div className="settings-number-row">
          <label><span>Tab size</span><input aria-label="Tab size" type="number" min="1" max="16" value={indentDraft.tabSize}
            onChange={(event) => {
              const tabSize = Math.min(16, Math.max(1, Math.round(event.currentTarget.valueAsNumber || 4)));
              setIndentDraft((current) => ({ ...current, tabSize }));
            }} /></label>
        </div>
        <label className="settings-check"><input aria-label="Insert spaces" type="checkbox" checked={indentDraft.insertSpaces}
          onChange={(event) => {
            const insertSpaces = event.currentTarget.checked;
            setIndentDraft((current) => ({ ...current, insertSpaces }));
          }} /><span>Indent with spaces instead of tabs</span></label>
        <label className="settings-check"><input aria-label="Detect indentation" type="checkbox" checked={indentDraft.detectIndentation}
          onChange={(event) => {
            const detectIndentation = event.currentTarget.checked;
            setIndentDraft((current) => ({ ...current, detectIndentation }));
          }} /><span>Follow how each file is already indented</span></label>
        <p className="settings-help">Enter indents the new line, a closing bracket or tag typed on its own line moves left, and Backspace removes one level of spaces. YAML always uses spaces and Makefile recipes always use tabs.</p>
      </fieldset>
      <fieldset className="settings-group"><legend>Suggestions</legend>
        <label className="settings-check"><input aria-label="Show suggestions while typing" type="checkbox" checked={indentDraft.suggestAuto}
          onChange={(event) => {
            const suggestAuto = event.currentTarget.checked;
            setIndentDraft((current) => ({ ...current, suggestAuto }));
          }} /><span>Show suggestions while typing</span></label>
        <label className="settings-check"><input aria-label="Enter accepts a suggestion" type="checkbox" checked={indentDraft.suggestAcceptOnEnter}
          onChange={(event) => {
            const suggestAcceptOnEnter = event.currentTarget.checked;
            setIndentDraft((current) => ({ ...current, suggestAcceptOnEnter }));
          }} /><span>Enter accepts a suggestion (Tab always does)</span></label>
        <p className="settings-help">Suggestions come from words in the document, the language's keywords, and snippets. They open by themselves after two letters in languages with keywords, never in Plain Text or Markdown. Show them anywhere with Ctrl+Space (on macOS also Option+Esc).</p>
      </fieldset>
      <fieldset className="settings-group"><legend>Gutter</legend>
        <label className="settings-check"><input aria-label="Show line change indicators by default" type="checkbox" checked={lineChanges}
          onChange={(event) => setLineChanges(event.currentTarget.checked)} /><span>Show line change indicators by default</span></label>
        <p className="settings-help">Marks lines edited since the last save and lines saved during this editing session. A document can override this from its toolbar until it is closed.</p>
      </fieldset>
      <div className="settings-preview" style={previewStyle} data-previewing={previewStyle ? "true" : undefined}><small>Preview</small><code>const nulis = <span className="settings-preview-selection">"write clearly"</span>;</code></div>
    </>,
    appearance: <>
      <fieldset className="settings-group"><legend>Color theme</legend>
        {filterField("Filter themes")}
        <label><span>Theme</span><select aria-label="Color theme" value={draft.appearance.theme}
          onChange={(event) => updateAppearance({ theme: event.currentTarget.value })}>
          {themes.filter((theme) => theme.id === draft.appearance.theme || matches(theme.name)).map((theme) => (
            <option key={theme.id} value={theme.id}>{theme.name}{theme.builtIn ? "" : " · User"}</option>))}
        </select></label>
        <p className="settings-help">{themes.length} themes available · {selectedTheme?.kind === "light" ? "Light" : "Dark"} theme selected</p>
        <p className="settings-help">The applied theme paints the editor and every Nulis window. The preview shows it while you edit; <strong>Apply</strong> saves it.</p>
      </fieldset>
      <fieldset className="settings-group">
        <legend>User themes</legend>
        <p className="settings-help">Installed themes are read-only. Duplicate one to change its colors; copies live beside your settings.</p>
        <div className="settings-inline">
          <button type="button" disabled={busy} onClick={() => void installTheme()}>Install Theme...</button>
          <button type="button" disabled={busy || !selectedTheme} onClick={() => void duplicateTheme()}>Duplicate Selected</button>
          <button type="button" disabled={busy || !selectedTheme || selectedTheme.builtIn} onClick={() => setConfirm("remove")}>Remove Selected</button>
        </div>
        {editableTheme ? (
          <div className="settings-colors">
            {themeDraft ? <p className="settings-help" role="status">"{themeDraft.name}" ({themeDraft.id}) is a draft. Save Copy stores it; Revert discards it.</p> : null}
            <p className="settings-help">Selection, secondary text, and icons are kept readable: a color below Nulis's contrast floors is painted strengthened in this theme's own hues, and saved exactly as you typed it. Pick a highlight that the text over it clears, or darken that text, and your color paints as typed.</p>
            <label className="settings-theme-name"><span>Theme name</span>
              <input type="text" aria-label="Theme name" maxLength={MAX_THEME_NAME} disabled={busy}
                value={themeNameDraft ?? editableTheme.name}
                onChange={(event) => setThemeNameDraft(event.currentTarget.value)} />
            </label>
            {nameChanged && nameIsUsable ? <p className="settings-help" role="status">"{themeName}" becomes the theme's name in the picker.</p> : null}
            <div className="settings-color-actions">
              <button type="button" disabled={busy || !draftIsPending || !nameIsUsable} onClick={() => void saveThemeColors()}>{storeAction}</button>
              <button type="button" disabled={busy || !draftIsPending}
                onClick={() => { setColorDraft({}); setThemeDraft(null); setThemeNameDraft(null); }}>Revert</button>
            </div>
            {Object.keys(editableTheme.colors).map((token) => {
              const color = editedColors[token] ?? "";
              const picker = expandedThemeColor(color);
              const paints = shown?.[token];
              // A color the floors strengthen is worth saying out loud: the row would otherwise
              // read as a saved color that the editor refuses to paint.
              const strengthened = Boolean(paints && picker && paints.toLowerCase() !== picker.toLowerCase());
              return (
                <label key={token} className="settings-color" data-invalid={picker ? undefined : "true"}>
                  <span title={themeTokenLabel(token)}>{token}</span>
                  <input type="color" value={picker ?? "#000000"} disabled={busy} aria-label={`${token} color`}
                    onChange={(event) => {
                      const color = event.currentTarget.value;
                      setColorDraft((current) => ({ ...current, [token]: color }));
                    }} />
                  <input type="text" aria-label={`${token} hex`} value={color} onKeyDown={(event) => {
                    if (event.key !== "ArrowUp" && event.key !== "ArrowDown" && event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
                    const nudged = nudgedThemeColor(color, event.key, event.shiftKey, event.altKey);
                    if (nudged === null) return;
                    event.preventDefault();
                    setColorDraft((current) => ({ ...current, [token]: nudged }));
                  }} onChange={(event) => {
                    const color = event.currentTarget.value;
                    setColorDraft((current) => ({ ...current, [token]: color }));
                  }} />
                  {strengthened
                    ? <small className="settings-color-shown">Nulis paints {paints} — {themeTokenLabel(token)} stays readable. Your color is saved as typed.</small>
                    : null}
                </label>
              );
            })}
          </div>
        ) : selectedTheme ? <p className="settings-help">"{selectedTheme.name}" is installed. Duplicate it to change its colors and name.</p> : null}
      </fieldset>
      <div className="settings-preview" style={previewStyle} data-previewing={previewStyle ? "true" : undefined}><small>Preview</small><code>const nulis = <span className="settings-preview-selection">"write clearly"</span>;</code></div>
    </>,
    languages: <>
      <fieldset className="settings-group"><legend>Language features</legend>
        <p className="settings-help">Switch a tool or the preview off for one language. Open documents keep their text; the tool disappears from the toolbar and menus.</p>
        {settingsState?.loadError ? <p className="settings-help" role="alert">settings.json could not be read ({settingsState.loadError}). Nulis is using defaults; Apply writes a new file.</p> : null}
        <ul className="settings-list" aria-label="Language features">
          {featureRows.filter((row) => matches(`${row.displayName} ${row.id}`)).map((row) => (
            <li key={row.id}><strong>{row.displayName}</strong>
              {row.capabilities.map((capability) => {
                const name = FEATURE_NAMES[capability.kind] ?? capability.kind;
                return <label className="settings-check" key={capability.kind}>
                  <input type="checkbox" aria-label={`${name} for ${row.displayName}`}
                    checked={featureDraft[row.id]?.[capability.kind] ?? capability.enabled}
                    onChange={(event) => {
                      const checked = event.currentTarget.checked;
                      setFeatureDraft((current) => ({ ...current, [row.id]: { ...current[row.id], [capability.kind]: checked } }));
                    }} />{name}
                </label>;
              })}
            </li>))}
        </ul>
      </fieldset>
      <fieldset className="settings-group"><legend>Language modules</legend>
        {filterField("Filter languages")}
        <ul className="settings-list" aria-label="Languages">
          {languages.filter((language) => matches(`${language.displayName} ${language.id}`)).map((language) => (
            <li key={language.id}><strong>{language.displayName}</strong><code>{language.id}</code></li>))}
        </ul>
        <p className="settings-help">A language is detected from the file name. Use View As in the editor toolbar to override it for one document, or add a pattern to files.associations in settings.json to override it for a kind of file.</p>
        {fileAssociations(settingsState).length > 0 ? <ul className="settings-list" aria-label="Your file associations">
          {fileAssociations(settingsState).map(([pattern, language]) => (
            <li key={pattern}><strong>{pattern}</strong><code>{language}</code>
              {settingsState?.rejectedAssociations.includes(pattern) ? <span>ignored</span> : null}</li>))}
        </ul> : null}
      </fieldset>
      <fieldset className="settings-group"><legend>File association conflicts</legend>
        <p className="settings-help">More than one language claims these patterns, so the first claim is used. Choosing one here writes <code>files.associations</code> in settings.json and applies to files opened afterwards.</p>
        {conflicts.length === 0
          ? <p className="settings-help">No conflicts.</p>
          : <ul className="settings-list" aria-label="File association conflicts">
            {conflicts.filter((conflict) => matches(conflict.pattern)).map((conflict) => {
              const saved = fileAssociations(settingsState).find(([pattern]) => pattern === conflict.pattern)?.[1];
              const chosen = associationDraft[conflict.pattern] ?? saved;
              return <li key={conflict.pattern}><strong>{conflict.pattern}</strong>
                {conflict.claims.map((claim) => <label className="settings-check" key={claim.language}>
                  <input type="radio" name={`conflict-${conflict.pattern}`}
                    aria-label={`Use ${claim.displayName} for ${conflict.pattern}`}
                    checked={chosen === claim.language}
                    onChange={() => setAssociationDraft((current) => ({ ...current, [conflict.pattern]: claim.language }))} />
                  {claim.displayName} <code>{claim.source}</code>
                </label>)}
              </li>;
            })}
          </ul>}
      </fieldset>
      <fieldset className="settings-group"><legend>Language servers</legend>
        <p className="settings-help">Nulis starts no language server unless it is listed in <code>lsp.servers</code> in settings.json and you approve its exact command here. A server starts when a document of one of its languages opens, and stops when the last one closes.</p>
        {servers?.configError ? <p className="settings-help server-error" role="alert">{servers.configError}</p> : null}
        {servers === null
          ? <p className="settings-help">Language servers are unavailable in this window.</p>
          : servers.servers.length === 0
            ? <p className="settings-help">No language server is configured. Add one to <code>lsp.servers</code> in settings.json, for example <code>{'"rust": { "command": ["rust-analyzer"], "languages": ["rust"] }'}</code>, then reopen Settings.</p>
            : <ul className="settings-list server-list" aria-label="Language servers">
              {servers.servers.filter((server) => matches(`${server.id} ${server.languages.join(" ")}`)).map((server) => <li key={server.id}>
                <div className="server-heading"><strong>{server.id}</strong>
                  <span className={server.approved ? "approved" : "approval-required"}>{server.approved ? "Approved" : "Not approved"}</span>
                  {server.stopped ? <span className="server-stopped">Stopped after failures</span> : null}
                </div>
                <code className="server-command">{server.command.join(" ")}</code>
                <span>{server.languages.join(", ")}</span>
                {server.running.length > 0
                  ? <span title={server.running.join("\n")}>Running for {server.running.length} {server.running.length === 1 ? "folder" : "folders"}</span>
                  : null}
                {server.error ? <span className="server-error" role="alert">{server.error}</span> : null}
                <div className="settings-inline">
                  <button type="button" disabled={serverBusy !== null}
                    onClick={() => void changeServer(server.id, server.approved ? "lsp.revoke" : "lsp.approve")}>
                    {server.approved ? "Revoke" : "Approve"}
                  </button>
                  {server.approved ? <button type="button" disabled={serverBusy !== null}
                    onClick={() => void changeServer(server.id, "lsp.restart")}>Restart</button> : null}
                </div>
              </li>)}
            </ul>}
        <p className="settings-help">The command is a program and its arguments, never a shell line. Editing the command or its language list revokes the approval. Reported problems appear in the status bar and in the Problems panel.</p>
      </fieldset>
    </>,
    keybindings: <>
      <fieldset className="settings-group"><legend>Keyboard shortcuts</legend>
        {filterField("Filter shortcuts")}
        {draft.keybindings.map((binding, index) => matches(`${binding.key} ${title(binding.command)}`) ? (
          <div className="customization-row" key={`${binding.command}-${index}`}>
            <input aria-label={`Shortcut ${index + 1}`} value={binding.key} onChange={(event) => {
              const keybindings = [...draft.keybindings];
              keybindings[index] = { ...binding, key: event.currentTarget.value };
              setDraft({ ...draft, keybindings });
            }} />
            <select aria-label={`Shortcut command ${index + 1}`} value={binding.command} onChange={(event) => {
              const keybindings = [...draft.keybindings];
              keybindings[index] = { ...binding, command: event.currentTarget.value };
              setDraft({ ...draft, keybindings });
            }}>{options}</select>
            <IconButton icon="close" label={`Remove shortcut ${index + 1}`}
              onClick={() => setDraft({ ...draft, keybindings: draft.keybindings.filter((_, item) => item !== index) })} />
          </div>) : null)}
        <button type="button" onClick={() => setDraft({ ...draft, keybindings: [...draft.keybindings, { key: "Primary+Alt+1", command: configurable[0]?.id ?? "app.about" }] })}>Add shortcut</button>
        <p className="settings-help">Use a canonical chord such as Primary+Shift+S. Primary is Command on macOS and Control elsewhere.</p>
      </fieldset>
    </>,
    menus: <>
      <fieldset className="settings-group"><legend>File menu</legend>
        {draft.menu.map((entry, index) => <div className="customization-row" key={`${entry.kind}-${index}`}>
          {entry.kind === "separator" ? <span className="customization-separator">Separator</span>
            : <select aria-label={`Menu command ${index + 1}`} value={entry.command} onChange={(event) => {
              const menu = [...draft.menu];
              menu[index] = { kind: "command", command: event.currentTarget.value };
              setDraft({ ...draft, menu });
            }}>{options}</select>}
          <IconButton icon="previous" label={`Move menu item ${index + 1} up`} onClick={() => setDraft({ ...draft, menu: move(draft.menu, index, -1) })} />
          <IconButton icon="next" label={`Move menu item ${index + 1} down`} onClick={() => setDraft({ ...draft, menu: move(draft.menu, index, 1) })} />
          <IconButton icon="close" label={`Remove menu item ${index + 1}`} onClick={() => setDraft({ ...draft, menu: draft.menu.filter((_, item) => item !== index) })} />
        </div>)}
        <div className="settings-inline">
          <button type="button" onClick={() => setDraft({ ...draft, menu: [...draft.menu, { kind: "command", command: configurable[0]?.id ?? "app.about" }] })}>Add command</button>
          <button type="button" onClick={() => setDraft({ ...draft, menu: [...draft.menu, { kind: "separator" }] })}>Add separator</button>
        </div>
      </fieldset>
      <fieldset className="settings-group"><legend>Toolbar</legend>
        {draft.toolbar.map((command, index) => <div className="customization-row" key={`${command}-${index}`}>
          <span className="customization-command">{title(command)}</span>
          <IconButton icon="previous" label={`Move ${title(command)} up`} onClick={() => setDraft({ ...draft, toolbar: move(draft.toolbar, index, -1) })} />
          <IconButton icon="next" label={`Move ${title(command)} down`} onClick={() => setDraft({ ...draft, toolbar: move(draft.toolbar, index, 1) })} />
          <IconButton icon="close" label={`Remove ${title(command)} from toolbar`} onClick={() => setDraft({ ...draft, toolbar: draft.toolbar.filter((_, item) => item !== index) })} />
        </div>)}
        <select aria-label="Add toolbar command" value="" onChange={(event) => {
          const command = event.currentTarget.value;
          if (command && !draft.toolbar.includes(command)) setDraft({ ...draft, toolbar: [...draft.toolbar, command] });
        }}><option value="">Add command…</option>{options}</select>
      </fieldset>
    </>,
    workspace: <>
      <fieldset className="settings-group"><legend>Workspaces</legend>
        <p className="settings-help">Drag a tab into the editor area to group it with the active tab. The result is one workspace tab with split panes; drag a pane header back to the tab strip to ungroup it.</p>
        <p className="settings-help">A workspace name is display metadata stored with the session. Renaming it never renames a file or changes a document path.</p>
        <button type="button" onClick={() => onMainAction("workspace.rename")}>Rename Active Workspace…</button>
      </fieldset>
      <fieldset className="settings-group"><legend>Limits</legend>
        <dl className="settings-facts">
          <div><dt>Open tabs</dt><dd>{LAYOUT_LIMITS.tabs}</dd></div>
          <div><dt>Panes per workspace</dt><dd>{LAYOUT_LIMITS.panes}</dd></div>
          <div><dt>Nesting depth</dt><dd>{LAYOUT_LIMITS.depth}</dd></div>
        </dl>
      </fieldset>
    </>,
    extensions: <>
      <fieldset className="settings-group"><legend>Installed plugins</legend>
        {plugins.length === 0 ? <p className="settings-help">No plugins installed.</p> : <ul className="settings-list" aria-label="Plugins">
          {plugins.map((plugin) => <li key={plugin.id}><strong>{plugin.name}</strong><code>{plugin.version}</code>
            <span>{plugin.approved ? "Approved" : "Approval required"}</span></li>)}
        </ul>}
        <button type="button" onClick={() => onMainAction("workbench.openPlugins")}>Manage Plugins…</button>
      </fieldset>
      <fieldset className="settings-group"><legend>Security</legend>
        <p className="settings-help">This window can read and store customization, list themes and plugins, and import a theme chosen in the system file picker. It has no access to open documents, the file system, or the terminal.</p>
      </fieldset>
    </>,
  };

  const chromeColors = previewing && editableTheme && editableTheme.id === draft.appearance.theme
    ? { [editableTheme.id]: draftColors }
    : undefined;
  const style = mode === "window"
    ? preferenceCssVariables(draft.appearance, themes, chromeColors) as CSSProperties
    : undefined;
  return <div className={`settings-window ${mode}`} style={style}
    role={mode === "inline" ? "dialog" : undefined}
    aria-modal={mode === "inline" ? true : undefined} aria-label="Settings"
    onKeyDown={(event) => { if (event.key === "Escape" && !confirm) { event.stopPropagation(); requestClose(); } }}>
    <nav className="settings-sidebar" aria-label="Settings sections">
      <div className="settings-sidebar-title">Settings</div>
      <div role="tablist" aria-orientation="vertical" aria-label="Settings sections" onKeyDown={onNavigationKey}>
        {SETTINGS_SECTIONS.map((item) => <button type="button" role="tab" key={item.id} data-section={item.id}
          id={`settings-tab-${item.id}`} aria-controls="settings-detail" aria-selected={item.id === section}
          tabIndex={item.id === section ? 0 : -1} className={item.id === section ? "active" : ""}
          onClick={() => setSection(item.id)}>
          <Icon name={item.icon} size={18} /><span>{item.title}</span>
          {pendingSections.has(item.id) ? <i className="settings-pending" role="img" aria-label="unapplied changes" title="Unapplied changes" /> : null}
        </button>)}
      </div>
    </nav>
    <section className="settings-detail" id="settings-detail" role="tabpanel" aria-labelledby={`settings-tab-${current.id}`}>
      <header><h2>{current.title}</h2>
        {mode === "inline" ? <IconButton icon="close" label="Close settings" onClick={requestClose} /> : null}</header>
      {stale ? <div className="settings-banner" role="alert">
        <span>Settings were changed in another window. Reload to edit the latest version; your unapplied changes here will be discarded.</span>
        <button type="button" disabled={busy} onClick={reload}>Reload</button>
      </div> : null}
      <div className="settings-scroll">{panels[section]}</div>
      <footer>
        {/* An error outranks the pending markers: hiding it left Apply looking inert. A theme
            draft is the section's own scope — the store button writes it, Apply never does. */}
        <span className="settings-notice" role="status">
          {notice ?? [draftIsPending ? "Unsaved theme changes" : null, pending ? "Unapplied changes" : null]
            .filter(Boolean)
            .join(" · ")}
        </span>
        <button type="button" disabled={busy} onClick={() => { discard(); onClose(); }}>Cancel</button>
        <button className="primary" type="button" disabled={busy || !pending || stale} onClick={() => void apply()}>Apply</button>
      </footer>
    </section>
    {confirm ? <div className="decision-backdrop">
      <section className="decision-dialog" role="alertdialog" aria-modal="true" aria-labelledby="settings-confirm-title">
        {confirm === "close" ? <>
          <h2 id="settings-confirm-title">Apply your changes to Settings?</h2>
          <p>Some settings have been edited but not applied.</p>
          <div className="decision-actions">
            <button autoFocus type="button" onClick={() => setConfirm(null)}>Continue Editing</button>
            <button type="button" onClick={() => { setConfirm(null); discard(); onClose(); }}>Discard</button>
            <button className="primary" type="button" disabled={busy || stale} onClick={() => {
              void apply().then((stored) => { setConfirm(null); if (stored) onClose(); });
            }}>Apply</button>
          </div>
          </> : confirm === "remove" ? <>
            <h2 id="settings-confirm-title">Remove this theme?</h2>
            <p>"{selectedTheme?.name ?? draft.appearance.theme}" and its stored colors are deleted. A theme in use stays until you apply another one.</p>
            <div className="decision-actions">
              <button autoFocus type="button" onClick={() => setConfirm(null)}>Cancel</button>
              <button className="primary danger" type="button" onClick={() => { if (selectedTheme) void removeTheme(selectedTheme.id); }}>Remove Theme</button>
            </div>
          </> : <>
          <h2 id="settings-confirm-title">Restore default settings?</h2>
          <p>Appearance, keybindings, the File menu, and the toolbar return to their defaults.</p>
          <div className="decision-actions">
            <button autoFocus type="button" onClick={() => setConfirm(null)}>Keep Settings</button>
            <button className="primary" type="button" onClick={() => void restoreDefaults()}>Restore Defaults</button>
          </div>
        </>}
      </section>
    </div> : null}
  </div>;
}
