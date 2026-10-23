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
  LanguageDescriptor,
  SettingsState,
  PluginSummary,
  ThemeRecord,
} from "./contracts";
import { CUSTOMIZABLE_COMMANDS } from "./commands";
import { Icon, IconButton, type IconName } from "./icons";
import { executeCommand, getApplicationInfo, listCommands } from "./ipc";
import { LAYOUT_LIMITS } from "./layout";
import { useShowWhenReady } from "./windowTheme";
import {
  BUILT_IN_THEMES,
  FONT_FAMILIES,
  loadLineChangeDefault,
  loadWelcomeOnStartup,
  normalizeEditorPreferences,
  preferenceCssVariables,
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
  { id: "workspace", title: "Workspace", icon: "workspace" },
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
  diagnostics: "Validate", format: "Beautify", compact: "Minify", preview: "Preview",
};

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

/** Separate, sectioned settings surface with a draft that is applied or discarded as a whole. */
export function SettingsWindow({
  mode, onPreview, onSaved, onMainAction, onClose, closeSignal = 0, externalChangeSignal = 0,
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
  const [plugins, setPlugins] = useState<PluginSummary[]>([]);
  const [info, setInfo] = useState<ApplicationInfo | null>(null);
  const [savedLineChanges, setSavedLineChanges] = useState(loadLineChangeDefault);
  const [welcome, setWelcome] = useState(loadWelcomeOnStartup);
  const [savedWelcome, setSavedWelcome] = useState(loadWelcomeOnStartup);
  const [lineChanges, setLineChanges] = useState(loadLineChangeDefault);
  const [filter, setFilter] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const [confirm, setConfirm] = useState<"close" | "reset" | null>(null);
  const signals = useRef({ close: closeSignal, external: externalChangeSignal });

  const loadLanguageSettings = useCallback(async () => {
    const [state, rows] = await Promise.all([
      executeCommand<SettingsState>({ id: "settings.state", context: CONTEXT, arguments: {} }),
      executeCommand<LanguageCapabilities[]>({ id: "language.capabilities", context: CONTEXT, arguments: {} }),
    ]);
    const nextRows = Array.isArray(rows) ? rows : [];
    setSettingsState(state ?? null);
    setFeatureRows(nextRows);
    setFeatureDraft(switchesOf(nextRows));
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
    void listCommands().then((next) => { if (mounted && Array.isArray(next)) setCommands(next); }).catch(() => undefined);
    void executeCommand<LanguageDescriptor[]>({ id: "language.list", context: CONTEXT, arguments: {} })
      .then((next) => { if (mounted && Array.isArray(next)) setLanguages(next); }).catch(() => undefined);
    void executeCommand<PluginSummary[]>({ id: "plugin.list", context: CONTEXT, arguments: {} })
      .then((next) => { if (mounted && Array.isArray(next)) setPlugins(next); }).catch(() => undefined);
    void getApplicationInfo().then((next) => { if (mounted) setInfo(next); }).catch(() => undefined);
    return () => { mounted = false; };
  }, [loadLanguageSettings, loadState]);

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
    if (!same({ ...before.appearance, theme: "" }, { ...draft.appearance, theme: "" }) || lineChanges !== savedLineChanges) pending.add("editor");
    if (!same(before.keybindings, draft.keybindings)) pending.add("keybindings");
    if (!same(before.menu, draft.menu) || !same(before.toolbar, draft.toolbar)) pending.add("menus");
    if (!same(featureDraft, switchesOf(featureRows))) pending.add("languages");
    return pending;
  }, [draft, featureDraft, featureRows, lineChanges, saved, savedLineChanges, savedWelcome, welcome]);
  const pending = pendingSections.size > 0;

  const discard = useCallback(() => {
    if (saved) setDraft(saved.document);
    setFeatureDraft(switchesOf(featureRows));
    setLineChanges(savedLineChanges);
    setWelcome(savedWelcome);
    onPreview(null);
  }, [featureRows, onPreview, saved, savedLineChanges, savedWelcome]);

  const apply = useCallback(async (): Promise<boolean> => {
    if (!saved || !draft || busy) return false;
    setBusy(true);
    try {
      const next = same(saved.document, draft) ? saved : await executeCommand<CustomizationState>({
        id: "customization.saveChecked", context: CONTEXT,
        arguments: { expectedRevision: saved.revision, document: draft },
      });
      if (settingsState && !same(featureDraft, switchesOf(featureRows))) {
        await executeCommand<SettingsState>({
          id: "settings.saveChecked", context: CONTEXT,
          arguments: {
            expectedRevision: settingsState.revision,
            document: { ...settingsState.document, values: withSwitches(settingsState.document.values, featureDraft, featureRows) },
          },
        });
        await loadLanguageSettings();
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
  }, [busy, draft, featureDraft, featureRows, lineChanges, loadLanguageSettings, onSaved, saved, settingsState, themes, welcome]);

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
    }
  }, [externalChangeSignal, loadLanguageSettings, loadState, pending]);

  const reload = () => {
    setBusy(true);
    void loadState().then(({ state, themes: nextThemes }) => {
      onPreview(null);
      onSaved(state, nextThemes);
      setNotice("Loaded the latest settings");
    }).catch((error: unknown) => setNotice(message(error, "Could not reload settings")))
      .finally(() => setBusy(false));
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
    if (!saved) return;
    if (saved.document.appearance.theme === id) {
      setNotice("Apply another theme before removing this one");
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

  if (!saved || !draft) {
    return <div className={`settings-window ${mode}`} role={mode === "inline" ? "dialog" : undefined} aria-label="Settings">
      <p className="settings-loading" role="status">{notice ?? "Loading settings…"}</p>
      {notice ? <button type="button" onClick={onClose}>Close</button> : null}
    </div>;
  }

  const updateAppearance = (change: Partial<EditorPreferences>) => {
    const appearance = normalizeEditorPreferences({ ...draft.appearance, ...change }, themes);
    setDraft({ ...draft, appearance });
    onPreview(appearance);
  };
  const configurable = commands.filter((command) => CUSTOMIZABLE_COMMANDS.has(command.id)
    || command.category.startsWith("Plugin ·"));
  const title = (id: string) => commands.find((command) => command.id === id)?.title ?? id;
  const options = configurable.map((command) => <option key={command.id} value={command.id}>{command.title}</option>);
  const needle = filter.trim().toLocaleLowerCase();
  const matches = (text: string) => !needle || text.toLocaleLowerCase().includes(needle);
  const current = SETTINGS_SECTIONS.find((item) => item.id === section) ?? SETTINGS_SECTIONS[0];
  const selectedTheme = themes.find((theme) => theme.id === draft.appearance.theme);
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
      <fieldset className="settings-group"><legend>Gutter</legend>
        <label className="settings-check"><input aria-label="Show line change indicators by default" type="checkbox" checked={lineChanges}
          onChange={(event) => setLineChanges(event.currentTarget.checked)} /><span>Show line change indicators by default</span></label>
        <p className="settings-help">Marks lines edited since the last save and lines saved during this editing session. A document can override this from its toolbar until it is closed.</p>
      </fieldset>
      <div className="settings-preview"><small>Preview</small><code>const nulis = "write clearly";</code></div>
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
      </fieldset>
      <fieldset className="settings-group"><legend>User themes</legend>
        <p className="settings-help">Themes are imported through the system file picker and validated before installation.</p>
        <div className="settings-inline">
          <button type="button" disabled={busy} onClick={() => void installTheme()}>Install Theme…</button>
          {selectedTheme && !selectedTheme.builtIn
            ? <button type="button" disabled={busy} onClick={() => void removeTheme(selectedTheme.id)}>Remove {selectedTheme.name}</button> : null}
        </div>
      </fieldset>
      <div className="settings-preview"><small>Preview</small><code>const nulis = "write clearly";</code></div>
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

  const style = mode === "window" ? preferenceCssVariables(draft.appearance, themes) as CSSProperties : undefined;
  return <div className={`settings-window ${mode}`} style={style} role={mode === "inline" ? "dialog" : undefined}
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
        <span className="settings-notice" role="status">{pending ? "Unapplied changes" : notice ?? ""}</span>
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
