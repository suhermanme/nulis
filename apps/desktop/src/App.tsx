import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type {
  ApplicationInfo,
  CaretPosition,
  CharacterCount,
  CommandDescriptor,
  CustomizationDocument,
  DocumentSummary,
  EditorPreferences,
  FolderCompareResult,
  LanguageDescriptor,
  PaneNode,
  PluginRunOutcome,
  RecoveryCandidate,
  SearchRequest,
  ExternalChange,
  ThemeRecord,
  ToolKind,
  ValidationReport,
  WorkbenchSession,
  WorkspaceDirectory,
  WorkspaceEntry,
  WorkspaceSummary,
} from "./contracts";
import { EditorViewport } from "./EditorViewport";
import { HexViewport } from "./HexViewport";
import { CompareView } from "./CompareView";
import { FolderCompareView } from "./FolderCompareView";
import { RemotePanel } from "./RemotePanel";
import { HelpWindow } from "./HelpWindow";
import licenseText from "../../../LICENSE?raw";
import { TerminalPanel } from "./TerminalPanel";
import { MacroPanel } from "./MacroPanel";
import { PluginPanel } from "./PluginPanel";
import { SettingsWindow, type SettingsMainAction } from "./SettingsWindow";
import { WorkTabs } from "./EditorLayout";
import { FileExplorer, loadExplorerTab, saveExplorerTab, type ExplorerTab } from "./FileExplorer";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { ContextMenu, MenuButton, OverflowToolbar, type MenuItem, type ToolbarItem } from "./Toolbar";
import {
  FIND_EVENTS, FindReplacePanel, ResultsWindow, type FindContext, type FindMessage, type FindTab, type MatchTarget, type ResultList,
} from "./FindReplacePanel";
import { Icon, IconButton, iconForCommand, type IconName } from "./icons";
import { PALETTE_COMMANDS, shortcutLabel } from "./commands";
import {
  closeWindowAfterSave, configureNativeMenu, executeCommand, getApplicationInfo, listCommands, openFindWindow, openHelpWindow, openResultsWindow, openSettingsWindow, takeLaunchPaths,
  setToolsMenu,
} from "./ipc";
import {
  LAYOUT_LIMITS,
  PENDING_PREFIX,
  type DropZone,
  type LayoutResult,
  type WorkTab,
  activeDocumentId as tabActiveDocument,
  closePane,
  documentReference,
  documentTab,
  extractPane,
  groupTabs,
  isWorkspace,
  listPanes,
  movePane,
  nextPaneId,
  nextTabId,
  nextWorkspaceName,
  paneDocument,
  removeDocument,
  replaceDocument,
  resizeActivePane,
  setRatio,
  splitPane,
  tabDocuments,
  tabsFromSession,
  tabsToSession,
  ungroupTab,
  viewCount,
} from "./layout";
import {
  BUILT_IN_THEMES,
  loadEditorPreferences,
  loadLineChangeDefault,
  loadWelcomeOnStartup,
  normalizeEditorPreferences,
  preferenceCssVariables,
  saveEditorPreferences,
  saveLineChangeDefault,
  saveWelcomeOnStartup,
} from "./preferences";

const NAV_ITEMS = ["Files", "Symbols", "Remote"] as const;
const EXPLORER_ENTRY_LIMIT = 500;
const MAX_LOADED_WORKSPACE_DIRECTORIES = 256;
const NATIVE_MENU_EVENT = "nulis://menu-command";
const EXTERNAL_CHECK_INTERVAL_MS = 3000;
/** Files sent by the system while Nulis runs: Finder Open With, the Dock, a second launch. */
const OPEN_PATHS_EVENT = "nulis://open-paths";
const CUSTOMIZATION_CHANGED_EVENT = "nulis://customization-changed";
const APPEARANCE_PREVIEW_EVENT = "nulis://appearance-preview";
const NAME_ERROR = `A name needs 1 to ${LAYOUT_LIMITS.nameChars} visible characters`;
/** Short text shown under a toolbar icon; other commands use their title. */
const TOOLBAR_CAPTIONS: Record<string, string> = {
  "file.new": "New", "file.open": "Open", "file.save": "Save", "file.saveAs": "Save As",
  "editor.undo": "Undo", "editor.redo": "Redo", "workspace.open": "Folder", "workspace.close": "Close Folder",
  "workbench.commandPalette": "Commands", "workbench.toggleSidebar": "Sidebar", "workbench.toggleSplit": "Split",
  "workbench.splitRight": "Split Right", "workbench.splitDown": "Split Down", "workbench.toggleEditorMode": "Hex",
  "workbench.closeTab": "Close Tab", "workbench.closeFile": "Close", "workbench.openSettings": "Settings", "app.about": "About", "workbench.openHelp": "Help", "editor.toggleReadOnly": "Read Only",
  "view.toggleLineChangeIndicators": "Changes", "tools.validate": "Validate", "tools.beautify": "Beautify",
  "tools.minify": "Minify", "workspace.rename": "Rename", "workspace.ungroup": "Ungroup",
};
const NAV_ICONS: Record<typeof NAV_ITEMS[number], IconName> = {
  Files: "files", Symbols: "symbols", Remote: "remote",
};

/** A dirty-document prompt may be one step of quitting or of closing a tab. */
type CloseFlow = { kind: "quit" } | { kind: "tabs"; tabIds: string[] } | null;

/** Closes every view of a document, dropping tabs that end up empty. */
function dropDocument(tabs: WorkTab[], documentId: string): WorkTab[] {
  return tabs.flatMap((tab) => removeDocument(tab, documentId) ?? []);
}

function defaultCustomization(): CustomizationDocument {
  return {
    schemaVersion: 1,
    appearance: loadEditorPreferences(),
    keybindings: [
      { key: "Primary+N", command: "file.new" }, { key: "Primary+O", command: "file.open" },
      { key: "Primary+S", command: "file.save" }, { key: "Primary+Shift+S", command: "file.saveAs" },
      { key: "Primary+W", command: "workbench.closeFile" },
      { key: "Primary+K", command: "workbench.commandPalette" },
      { key: "Primary+Shift+L", command: "editor.toggleReadOnly" },
      { key: "Primary+Z", command: "editor.undo" }, { key: "Primary+Shift+Z", command: "editor.redo" },
    ],
    menu: [
      { kind: "command", command: "file.new" }, { kind: "command", command: "file.open" },
      { kind: "separator" }, { kind: "command", command: "file.save" },
      { kind: "command", command: "file.saveAs" },
      { kind: "separator" }, { kind: "command", command: "workbench.closeFile" },
    ],
    toolbar: ["file.new", "file.open", "file.save", "workbench.closeFile", "editor.undo", "editor.redo"],
  };
}

function matchesChord(event: KeyboardEvent, chord: string): boolean {
  const parts = chord.split("+");
  const key = parts.at(-1)?.toLowerCase();
  return Boolean(key) && event.key.toLowerCase() === key
    && (event.metaKey || event.ctrlKey) === parts.includes("Primary")
    && event.shiftKey === parts.includes("Shift") && event.altKey === parts.includes("Alt");
}

function defaultSession(): WorkbenchSession {
  return {
    schemaVersion: 2,
    workspaceRoot: null,
    documents: [],
    layouts: [],
    activeLayout: null,
    sidebarVisible: true,
    sidebarWidth: 260,
    activePanel: "files",
    viewModes: {},
  };
}

/** The core migrates older schemas; anything else falls back to an empty session. */
function usableSession(value: WorkbenchSession | null | undefined): WorkbenchSession {
  return value && value.schemaVersion === 2 && Array.isArray(value.layouts)
    && Array.isArray(value.documents) ? value : defaultSession();
}

function encodingLabel(document: DocumentSummary | undefined): string {
  return document?.encoding === "windows1252" ? "Windows-1252" : "UTF-8";
}

/** Groups the digits of a decimal string of any length: 1234567 → 1,234,567. */
function groupDigits(value: string): string {
  return /^\d+$/.test(value) ? value.replace(/\B(?=(\d{3})+(?!\d))/g, ",") : value;
}

/** "1,024 bytes · 1,000 characters"; the characters are left out when not counted. */
function sizeLabel(byteLen: string, characters: string | undefined): string {
  const bytes = `${groupDigits(byteLen)} ${byteLen === "1" ? "byte" : "bytes"}`;
  return characters === undefined ? bytes
    : `${bytes} · ${groupDigits(characters)} ${characters === "1" ? "character" : "characters"}`;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

function comparablePath(path: string): string {
  return path.replaceAll("\\", "/").replace(/\/+$/, "");
}

function keepFocusInDialog(event: ReactKeyboardEvent<HTMLElement>) {
  if (event.key !== "Tab") return;
  const controls = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"));
  if (controls.length === 0) return;
  if (event.shiftKey && document.activeElement === controls[0]) {
    event.preventDefault();
    controls[controls.length - 1].focus();
  } else if (!event.shiftKey && document.activeElement === controls[controls.length - 1]) {
    event.preventDefault();
    controls[0].focus();
  }
}

export function App() {
  const desktopShell = isTauri();
  const nativeMenuActionRef = useRef<(command: string) => void>(() => undefined);
  const pluginsActivatedRef = useRef(false);
  const [info, setInfo] = useState<ApplicationInfo | null>(null);
  const [commands, setCommands] = useState<CommandDescriptor[]>([]);
  const [languages, setLanguages] = useState<LanguageDescriptor[]>([
    { id: "plaintext", displayName: "Plain Text" },
    { id: "json", displayName: "JSON" },
    { id: "markdown", displayName: "Markdown" },
    { id: "python", displayName: "Python" },
  ]);
  const [documents, setDocuments] = useState<DocumentSummary[]>([]);
  const [tabs, setTabs] = useState<WorkTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [renameRequest, setRenameRequest] = useState<{ tabId: string | null; signal: number }>({ tabId: null, signal: 0 });
  const [lineChangeDefault, setLineChangeDefault] = useState(loadLineChangeDefault);
  const [welcomeOnStartup, setWelcomeOnStartup] = useState(loadWelcomeOnStartup);
  // The welcome screen stays for this session after its tick box turns it off for the next start.
  const [welcomeVisible, setWelcomeVisible] = useState(loadWelcomeOnStartup);
  const [aboutOpen, setAboutOpen] = useState(false);
  // Status bar facts about the focused editor, each tagged with the document it describes.
  const [caret, setCaret] = useState<(CaretPosition & { documentId: string }) | null>(null);
  const [characterCount, setCharacterCount] = useState<(CharacterCount & { documentId: string }) | null>(null);
  const [licenseOpen, setLicenseOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const startupDocumentRef = useRef(false);
  const [lineChangeOverrides, setLineChangeOverrides] = useState<Record<string, boolean>>({});
  const [comparing, setComparing] = useState(false);
  const [folderComparison, setFolderComparison] = useState<FolderCompareResult | null>(null);
  const [secondaryDocumentId, setSecondaryDocumentId] = useState<string | null>(null);
  const [viewModes, setViewModes] = useState<Record<string, "text" | "hex">>({});
  const [activePanel, setActivePanel] = useState<"files" | "remote">("files");
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(260);
  const [resizingSidebar, setResizingSidebar] = useState(false);
  const [workspace, setWorkspace] = useState<WorkspaceSummary | null>(null);
  const [workspaceDirectories, setWorkspaceDirectories] = useState<Record<string, WorkspaceDirectory>>({});
  const [expandedDirectories, setExpandedDirectories] = useState<string[]>([]);
  const [workspaceDocumentIds, setWorkspaceDocumentIds] = useState<Record<string, string>>({});
  const [searchRequest, setSearchRequest] = useState<SearchRequest | undefined>();
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; label: string; items: MenuItem[] } | null>(null);
  const [findTab, setFindTab] = useState<FindTab | null>(null);
  const [findSeed, setFindSeed] = useState<{ id: number; text: string } | undefined>();
  const [searchResults, setSearchResults] = useState<ResultList | null>(null);
  const pendingFindOpen = useRef<{ tab: FindTab; seed?: string } | null>(null);
  const resultsForWindow = useRef<ResultList | null>(null);
  const findMessageRef = useRef<(message: FindMessage) => void>(() => undefined);
  const [revealTarget, setRevealTarget] = useState<{ documentId: string; byteOffset: string } | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [macroOpen, setMacroOpen] = useState(false);
  const [pluginOpen, setPluginOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [explorerTab, setExplorerTab] = useState<ExplorerTab>(loadExplorerTab);
  const [preferences, setPreferences] = useState(loadEditorPreferences);
  const [themes, setThemes] = useState<ThemeRecord[]>(BUILT_IN_THEMES);
  const [customization, setCustomization] = useState<CustomizationDocument>(defaultCustomization);
  const [macroRefreshVersion, setMacroRefreshVersion] = useState(0);
  // Bumped when a document is reloaded from disk, so its views read it afresh.
  const [documentEpochs, setDocumentEpochs] = useState<Record<string, number>>({});
  const [externalPrompt, setExternalPrompt] = useState<{
    kind: "modified" | "deleted" | "confirmReload"; document: DocumentSummary;
  } | null>(null);
  // Outside changes already asked about, as `documentId:kind`, so each is asked once.
  const promptedChanges = useRef(new Set<string>());
  const [paletteQuery, setPaletteQuery] = useState("");
  const [paletteIndex, setPaletteIndex] = useState(0);
  const [sessionReady, setSessionReady] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const [pendingCloseId, setPendingCloseId] = useState<string | null>(null);
  const [recoveryCandidates, setRecoveryCandidates] = useState<RecoveryCandidate[]>([]);
  const [busyDecision, setBusyDecision] = useState(false);
  const [closeFlow, setCloseFlow] = useState<CloseFlow>(null);
  const [status, setStatus] = useState("Connecting to the Nulis core…");
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const activeTabIdRef = useRef(activeTabId);
  activeTabIdRef.current = activeTabId;
  const tabOrderRef = useRef<string[]>([]);
  const customizationRef = useRef(customization);
  customizationRef.current = customization;

  useEffect(() => {
    let mounted = true;
    void (async () => {
      try {
        const [nextInfo, nextCommands, nextThemes, nextCustomization, nextLanguages] = await Promise.all([
          getApplicationInfo(), listCommands(),
          executeCommand<ThemeRecord[]>({ id: "theme.list", context: { windowId: "main" }, arguments: {} })
            .catch(() => [...BUILT_IN_THEMES]),
          executeCommand<CustomizationDocument>({ id: "customization.get", context: { windowId: "main" }, arguments: {} })
            .catch(defaultCustomization),
          executeCommand<LanguageDescriptor[]>({ id: "language.list", context: { windowId: "main" }, arguments: {} })
            .catch(() => []),
        ]);
        const restored = usableSession(await executeCommand<WorkbenchSession>({
          id: "session.load",
          context: { windowId: "main" },
          arguments: {},
        }).catch(() => null));
        if (!mounted) return;
        setInfo(nextInfo);
        setCommands(nextCommands);
        if (Array.isArray(nextLanguages) && nextLanguages.length > 0) setLanguages(nextLanguages);
        const loadedThemes = Array.isArray(nextThemes) && nextThemes.length > 0 ? nextThemes : [...BUILT_IN_THEMES];
        const loadedCustomization = nextCustomization?.appearance ? nextCustomization : defaultCustomization();
        setThemes(loadedThemes);
        setCustomization(loadedCustomization);
        setPreferences(loadedCustomization.appearance);
        setSidebarVisible(restored.sidebarVisible);
        setSidebarWidth(restored.sidebarWidth);
        // Older sessions may name the removed Search panel.
        setActivePanel(restored.activePanel === "remote" ? "remote" : "files");

        if (restored.workspaceRoot) {
          try {
            const openedWorkspace = await executeCommand<WorkspaceSummary>({
              id: "workspace.open", context: { windowId: "main" },
              arguments: { root: restored.workspaceRoot },
            });
            const listing = await executeCommand<WorkspaceDirectory>({
              id: "workspace.list", context: { windowId: "main" },
              arguments: { path: "", maxEntries: EXPLORER_ENTRY_LIMIT },
            });
            if (!mounted) return;
            setWorkspace(openedWorkspace);
            setWorkspaceDirectories({ "": listing });
          } catch {
            setStatus("The previous workspace is no longer available");
          }
        }

        // Each reference restores independently; only missing leaves are removed.
        const reopened: DocumentSummary[] = [];
        const resolved = new Map<string, string>();
        const references = [...new Set(restored.documents)];
        for (const reference of references) {
          if (reference.startsWith("recovery:")) {
            // An untitled tab keeps its place until its checkpoint is restored or discarded.
            resolved.set(reference, `${PENDING_PREFIX}${reference.slice("recovery:".length)}`);
            continue;
          }
          if (!reference.startsWith("path:")) continue;
          try {
            const opened = await executeCommand<DocumentSummary>({
              id: "file.open", context: { windowId: "main" }, arguments: { path: reference.slice("path:".length) },
            });
            if (!reopened.some((document) => document.documentId === opened.documentId)) reopened.push(opened);
            resolved.set(reference, opened.documentId);
          } catch {
            // One unavailable file does not prevent the remaining session from restoring.
          }
        }
        if (!mounted) return;
        let restoredTabs = tabsFromSession(restored, (reference) => resolved.get(reference) ?? null);
        for (const document of reopened) {
          if (viewCount(restoredTabs, document.documentId) > 0) continue;
          restoredTabs = [...restoredTabs,
            documentTab(nextTabId(restoredTabs), nextPaneId(restoredTabs), document.documentId)];
        }
        setDocuments(reopened);
        setTabs(restoredTabs);
        setActiveTabId(restoredTabs.find((tab) => tab.id === restored.activeLayout)?.id ?? restoredTabs[0]?.id ?? null);
        setViewModes(Object.fromEntries(reopened.map((document) => [
          document.documentId,
          restored.viewModes[documentReference(document) ?? ""] ?? "text",
        ])));
        const pathCount = references.filter((reference) => reference.startsWith("path:")).length;
        setSessionReady(true);
        if (pathCount > reopened.length) {
          setStatus(`Restored ${reopened.length} files; ${pathCount - reopened.length} unavailable`);
        } else {
          setStatus(reopened.length > 0 || restored.workspaceRoot ? "Session restored" : "Ready");
        }
      } catch (error: unknown) {
        if (!mounted) return;
        setSessionReady(true);
        setStatus(errorMessage(error, "Native core unavailable"));
      }
    })();
    return () => { mounted = false; };
  }, []);

  useEffect(() => {
    if (!sessionReady) return;
    const timeout = window.setTimeout(() => {
      const stored = tabsToSession(tabs, documents);
      const session: WorkbenchSession = {
        schemaVersion: 2,
        workspaceRoot: workspace?.root ?? null,
        documents: stored.documents,
        layouts: stored.layouts,
        activeLayout: stored.layouts.some((tab) => tab.id === activeTabId)
          ? activeTabId : stored.layouts[0]?.id ?? null,
        sidebarVisible,
        sidebarWidth,
        activePanel,
        viewModes: Object.fromEntries(documents.flatMap((document) => {
          const reference = documentReference(document);
          return reference && stored.documents.includes(reference)
            ? [[reference, viewModes[document.documentId] ?? "text"]] : [];
        })),
      };
      void executeCommand({
        id: "session.save", context: { windowId: "main" }, arguments: session,
      }).catch((error: unknown) => setStatus(errorMessage(error, "Could not save session")));
    }, 150);
    return () => window.clearTimeout(timeout);
  }, [activeTabId, activePanel, documents, sessionReady, sidebarVisible, sidebarWidth, tabs, viewModes, workspace?.root]);

  // When the active tab closes, its neighbour in the strip takes over.
  useEffect(() => {
    const previous = tabOrderRef.current;
    tabOrderRef.current = tabs.map((tab) => tab.id);
    if (tabs.some((tab) => tab.id === activeTabId)) return;
    const index = activeTabId ? previous.indexOf(activeTabId) : -1;
    setActiveTabId(tabs[Math.min(Math.max(index, 0), tabs.length - 1)]?.id ?? null);
  }, [activeTabId, tabs]);

  useEffect(() => {
    if (!sessionReady) return;
    const recoverable = documents.filter((document) => document.dirty && !document.remoteUri);
    if (recoverable.length === 0) return;
    const timeout = window.setTimeout(() => {
      for (const document of recoverable) {
        void executeCommand({
          id: "recovery.checkpoint",
          context: { windowId: "main", documentId: document.documentId },
          arguments: {},
        }).catch((error: unknown) => setStatus(errorMessage(error, "Could not checkpoint edits")));
      }
    }, 750);
    return () => window.clearTimeout(timeout);
  }, [documents, sessionReady]);

  useEffect(() => {
    if (!sessionReady) return;
    void executeCommand<RecoveryCandidate[]>({
      id: "recovery.list", context: { windowId: "main" }, arguments: {},
    }).then((candidates) => {
      const available = Array.isArray(candidates) ? candidates : [];
      setRecoveryCandidates(available);
      // Drop remembered untitled tabs whose checkpoint no longer exists.
      const known = new Set(available.map((candidate) => `${PENDING_PREFIX}${candidate.recoveryId}`));
      setTabs((current) => current.flatMap((tab) => tabDocuments(tab))
        .filter((id) => id.startsWith(PENDING_PREFIX) && !known.has(id))
        .reduce(dropDocument, current));
    }).catch((error: unknown) => {
      setStatus(errorMessage(error, "Could not inspect recovery checkpoints"));
    });
  }, [sessionReady]);

  useEffect(() => {
    saveEditorPreferences(preferences, themes);
  }, [preferences, themes]);

  useEffect(() => {
    if (!resizingSidebar) return;
    const onPointerMove = (event: PointerEvent) => setSidebarWidth(
      Math.min(480, Math.max(180, event.clientX - 68)),
    );
    const onPointerUp = () => setResizingSidebar(false);
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp, { once: true });
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
    };
  }, [resizingSidebar]);

  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];
  const activeWorkspace = activeTab ? isWorkspace(activeTab) : false;
  const shownDocumentId = activeTab ? tabActiveDocument(activeTab) : null;
  const activeDocument = useMemo(
    () => documents.find((document) => document.documentId === shownDocumentId),
    [documents, shownDocumentId],
  );
  const activeDocumentId = activeDocument?.documentId ?? null;
  // JSON and XML tools exist only while such a document is focused as text.
  const toolKind: ToolKind | null = !activeDocument || viewModes[activeDocument.documentId] === "hex" ? null
    : activeDocument.languageId === "json" ? "json"
      : activeDocument.languageId === "xml" || activeDocument.languageId === "xsl" ? "xml" : null;
  const toolLabel = toolKind === "json" ? "JSON" : "XML";
  const toolAvailable = (commandId: string) => toolKind !== null && (commandId !== "tools.minify" || toolKind === "json");
  useEffect(() => {
    if (desktopShell) void setToolsMenu(toolKind).catch(() => undefined);
  }, [desktopShell, toolKind]);
  const pendingCloseDocument = documents.find((document) => document.documentId === pendingCloseId);
  const secondaryDocument = useMemo(
    () => {
      const selected = documents.find((document) => document.documentId === secondaryDocumentId);
      return selected && selected.documentId !== activeDocumentId ? selected
        : documents.find((document) => document.documentId !== activeDocumentId);
    },
    [activeDocumentId, documents, secondaryDocumentId],
  );
  const paletteCommands = useMemo(() => {
    const query = paletteQuery.trim().toLocaleLowerCase();
    return commands.filter((command) => PALETTE_COMMANDS.has(command.id)
      || command.category.startsWith("Plugin ·"))
      .filter((command) => !command.id.startsWith("tools.") || toolAvailable(command.id))
      .filter((command) => !query
        || `${command.title} ${command.category} ${command.id}`.toLocaleLowerCase().includes(query))
      .slice(0, 100);
    // `toolAvailable` depends only on the focused document's tool set.
  }, [commands, paletteQuery, toolKind]);

  useEffect(() => {
    if (!sessionReady || pluginsActivatedRef.current) return;
    pluginsActivatedRef.current = true;
    void executeCommand<PluginRunOutcome[]>({
      id: "plugin.activateStartup", context: { windowId: "main" }, arguments: {},
    }).then((outcomes) => {
      const messages = outcomes.flatMap((outcome) => outcome.messages);
      if (messages.length > 0) setStatus(messages[messages.length - 1]);
    }).catch((error: unknown) => {
      setStatus(errorMessage(error, "Could not activate startup plugins"));
    });
  }, [sessionReady]);

  const upsertDocument = (updated: DocumentSummary) => {
    setDocuments((current) => current.map((document) => (
      document.documentId === updated.documentId ? updated : document
    )));
  };
  const updateActiveTab = (change: (tab: WorkTab) => WorkTab) => {
    setTabs((current) => current.map((tab) => tab.id === activeTabIdRef.current ? change(tab) : tab));
  };
  /** Focuses the tab and pane that show a document, opening a new tab when none does. */
  const revealIn = (current: WorkTab[], documentId: string): WorkTab[] => {
    const holds = (tab: WorkTab) => tabDocuments(tab).includes(documentId);
    const holder = current.find((tab) => tab.id === activeTabIdRef.current && holds(tab)) ?? current.find(holds);
    if (!holder) {
      const created = documentTab(nextTabId(current), nextPaneId(current), documentId);
      setActiveTabId(created.id);
      return [...current, created];
    }
    setActiveTabId(holder.id);
    const pane = listPanes(holder.root).find((item) => paneDocument(item) === documentId);
    return current.map((tab) => tab.id === holder.id && pane ? { ...tab, activePane: pane.id } : tab);
  };
  const showDocument = (documentId: string) => setTabs((current) => revealIn(current, documentId));
  const addDocument = (created: DocumentSummary) => {
    setDocuments((current) => current.some(
      (document) => document.documentId === created.documentId,
    ) ? current : [...current, created]);
    showDocument(created.documentId);
  };

  useEffect(() => {
    if (!desktopShell || !sessionReady) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "enter") {
        setDragOver(true);
      } else if (event.payload.type === "leave") {
        setDragOver(false);
      } else if (event.payload.type === "drop") {
        setDragOver(false);
        const paths = event.payload.paths;
        void (async () => {
          for (const path of paths) {
            try {
              const opened = await executeCommand<DocumentSummary>({
                id: "file.open", context: { windowId: "main" }, arguments: { path },
              });
              if (disposed) return;
              setDocuments((current) => current.some((item) => item.documentId === opened.documentId)
                ? current : [...current, opened]);
              setTabs((current) => revealIn(current, opened.documentId));
              setStatus(`Opened ${opened.name}`);
            } catch (error: unknown) {
              if (!disposed) setStatus(errorMessage(error, `Could not open ${path}`));
            }
          }
        })();
      }
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch((error: unknown) => {
      if (!disposed) setStatus(errorMessage(error, "Could not enable file drop"));
    });
    return () => { disposed = true; unlisten?.(); };
  }, [desktopShell, sessionReady]);

  useEffect(() => {
    if (!desktopShell || !sessionReady) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onCloseRequested((event) => {
      // The webview may not destroy its own window, so closing always goes
      // through the native command, which rechecks dirty state in the core.
      event.preventDefault();
      const dirty = documents.find((document) => document.dirty);
      if (!dirty) {
        void closeWindowAfterSave().catch((error: unknown) => setStatus(errorMessage(error, "Could not close window")));
        return;
      }
      for (const document of documents.filter((item) => item.dirty && !item.remoteUri)) {
        void executeCommand({
          id: "recovery.checkpoint",
          context: { windowId: "main", documentId: document.documentId }, arguments: {},
        }).catch((error: unknown) => setStatus(errorMessage(error, "Could not checkpoint edits")));
      }
      setCloseFlow({ kind: "quit" });
      setPendingCloseId(dirty.documentId);
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch((error: unknown) => setStatus(errorMessage(error, "Could not guard window close")));
    return () => { disposed = true; unlisten?.(); };
  }, [desktopShell, documents, sessionReady]);

  const openFile = async () => {
    if (!sessionReady) { setStatus("Restoring the previous session…"); return; }
    setStatus("Choose a file…");
    try {
      const selected = await open({ multiple: false, directory: false });
      if (typeof selected !== "string") { setStatus("Ready"); return; }
      const opened = await executeCommand<DocumentSummary>({
        id: "file.open", context: { windowId: "main" }, arguments: { path: selected },
      });
      addDocument(opened);
      setStatus("Ready");
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not open file"));
    }
  };

  // Settings changed the startup preference: follow it right away.
  const applyWelcomeSetting = () => {
    const next = loadWelcomeOnStartup();
    setWelcomeOnStartup(next);
    setWelcomeVisible(next);
  };

  const newFile = async () => {
    if (!sessionReady) { setStatus("Restoring the previous session…"); return; }
    setStatus("Creating document…");
    try {
      const created = await executeCommand<DocumentSummary>({
        id: "file.new", context: { windowId: "main" },
        arguments: { name: `Untitled ${documents.length + 1}`, initialText: "" },
      });
      addDocument(created);
      setStatus("Ready");
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not create document"));
    }
  };

  const saveDocumentFor = async (document: DocumentSummary, saveAs: boolean): Promise<boolean> => {
    if (!sessionReady) { setStatus("Restoring the previous session…"); return false; }
    try {
      if (!saveAs && document.sourcePath) {
        setStatus(`Saving ${document.name}…`);
        const saved = await executeCommand<DocumentSummary>({
          id: "file.save",
          context: { windowId: "main", documentId: document.documentId },
          arguments: { expectedRevision: document.revision },
        });
        upsertDocument(saved);
        setStatus(`Saved ${saved.name}`);
        return true;
      }
      const destination = await saveDialog({
        title: saveAs ? "Save As" : "Save File",
        defaultPath: document.sourcePath ?? document.name,
      });
      if (!destination) { setStatus("Save cancelled"); return false; }
      setStatus(`Saving ${document.name}…`);
      const saved = await executeCommand<DocumentSummary>({
        id: "file.saveAs",
        context: { windowId: "main", documentId: document.documentId },
        arguments: { expectedRevision: document.revision, path: destination },
      });
      upsertDocument(saved);
      setStatus(`Saved ${saved.name}`);
      return true;
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not save file"));
      return false;
    }
  };

  const saveDocument = async (saveAs: boolean) => {
    if (!activeDocument) { setStatus("No active document to save"); return; }
    await saveDocumentFor(activeDocument, saveAs);
  };

  useEffect(() => {
    if (!desktopShell) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<string>(NATIVE_MENU_EVENT, (event) => {
      nativeMenuActionRef.current(event.payload);
    }).then((stopListening) => {
      if (disposed) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [desktopShell]);

  useEffect(() => {
    if (!desktopShell || commands.length === 0) return;
    const entries = customization.menu.map((entry) => entry.kind === "separator" ? entry : {
      ...entry,
      title: commands.find((command) => command.id === entry.command)?.title ?? entry.command,
      accelerator: customization.keybindings.find((binding) => binding.command === entry.command)?.key,
    });
    void configureNativeMenu(entries).catch((error: unknown) => setStatus(errorMessage(error, "Could not configure native menu")));
  }, [commands, customization.keybindings, customization.menu, desktopShell]);

  // The separate Find window follows the focused document, its selection, and the open documents.
  const findActiveDocument = activeDocument && viewModes[activeDocument.documentId] !== "hex" ? activeDocument : null;
  const findContext: FindContext = {
    activeDocument: findActiveDocument,
    textDocuments: documents.filter((document) => viewModes[document.documentId] !== "hex"),
    selection: caret && caret.documentId === findActiveDocument?.documentId ? caret.selection : undefined,
    defaultFolder: workspace?.root,
  };
  const findContextKey = JSON.stringify(findContext);
  const findContextRef = useRef(findContext);
  findContextRef.current = findContext;
  const documentsRef = useRef(documents);
  documentsRef.current = documents;
  const externalPromptRef = useRef(externalPrompt);
  externalPromptRef.current = externalPrompt;
  const checkExternalRef = useRef<() => Promise<void>>(async () => undefined);
  // Other programs may change open files: check when the window comes back
  // to the front and every few seconds while it is in front.
  useEffect(() => {
    if (!desktopShell || !sessionReady) return;
    const check = () => { void checkExternalRef.current(); };
    const timer = window.setInterval(() => { if (document.hasFocus()) check(); }, EXTERNAL_CHECK_INTERVAL_MS);
    window.addEventListener("focus", check);
    check();
    return () => { window.clearInterval(timer); window.removeEventListener("focus", check); };
  }, [desktopShell, sessionReady]);
  useEffect(() => {
    if (desktopShell) void emitTo("find", FIND_EVENTS.context, findContextRef.current).catch(() => undefined);
  }, [desktopShell, findContextKey]);

  findMessageRef.current = (message) => {
    switch (message.kind) {
      case "ready":
        void emitTo("find", FIND_EVENTS.context, findContextRef.current).catch(() => undefined);
        if (pendingFindOpen.current) {
          void emitTo("find", FIND_EVENTS.open, pendingFindOpen.current).catch(() => undefined);
          pendingFindOpen.current = null;
        }
        break;
      case "search": searchInEditor(message.request); break;
      case "openMatch": void openMatch(message.target); break;
      case "documentChange": upsertDocument(message.document); break;
      case "status": setStatus(message.message); break;
      case "results": showSearchResults(message.list); break;
      case "resultsReady": sendResultsToWindow(); break;
    }
  };
  useEffect(() => {
    if (!desktopShell) return;
    let disposed = false;
    let stop: (() => void) | undefined;
    void listen<FindMessage>(FIND_EVENTS.message, (event) => findMessageRef.current(event.payload)).then((unlisten) => {
      if (disposed) unlisten(); else stop = unlisten;
    }).catch(() => undefined);
    return () => { disposed = true; stop?.(); };
  }, [desktopShell]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const binding = customization.keybindings.find((candidate) => matchesChord(event, candidate.key));
      if (binding) {
        if (desktopShell && customization.menu.some((entry) => entry.kind === "command" && entry.command === binding.command)) return;
        event.preventDefault();
        nativeMenuActionRef.current(binding.command);
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key === "," && !event.altKey && !event.shiftKey) {
        // The desktop shell owns this accelerator through its native Settings menu item.
        if (desktopShell) return;
        event.preventDefault();
        nativeMenuActionRef.current("workbench.openSettings");
        return;
      }
      if (event.key === "F1" && !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey) {
        // The desktop shell owns this accelerator through its native Help menu item.
        if (desktopShell) return;
        event.preventDefault();
        nativeMenuActionRef.current("workbench.openHelp");
        return;
      }
      if (event.key === "Escape") {
        setPaletteOpen(false);
        setPluginOpen(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [customization.keybindings, customization.menu, desktopShell]);

  useEffect(() => {
    if (!desktopShell) return;
    let disposed = false;
    const stops: Array<() => void> = [];
    const keep = (stop: () => void) => { if (disposed) stop(); else stops.push(stop); };
    // The Settings window stored a newer revision: refresh every surface, including native menus.
    void listen(CUSTOMIZATION_CHANGED_EVENT, () => {
      void Promise.all([
        executeCommand<CustomizationDocument>({ id: "customization.get", context: { windowId: "main" }, arguments: {} }),
        executeCommand<ThemeRecord[]>({ id: "theme.list", context: { windowId: "main" }, arguments: {} }),
      ]).then(([document, nextThemes]) => {
        if (disposed || !document?.appearance) return;
        if (Array.isArray(nextThemes) && nextThemes.length > 0) setThemes(nextThemes);
        setCustomization(document);
        setPreferences(document.appearance);
        setLineChangeDefault(loadLineChangeDefault());
        setLineChangeOverrides({});
        applyWelcomeSetting();
      }).catch((error: unknown) => setStatus(errorMessage(error, "Could not refresh settings")));
    }).then(keep);
    void listen<EditorPreferences | null>(APPEARANCE_PREVIEW_EVENT, (event) => {
      setPreferences(event.payload
        ? normalizeEditorPreferences(event.payload, BUILT_IN_THEMES.concat([]))
        : customizationRef.current.appearance);
    }).then(keep);
    return () => { disposed = true; stops.forEach((stop) => stop()); };
  }, [desktopShell]);

  const openWorkspace = async () => {
    if (!sessionReady) { setStatus("Restoring the previous session…"); return; }
    try {
      const selected = await open({ multiple: false, directory: true });
      if (typeof selected !== "string") return;
      setStatus("Opening workspace…");
      const openedWorkspace = await executeCommand<WorkspaceSummary>({
        id: "workspace.open", context: { windowId: "main" }, arguments: { root: selected },
      });
      const listing = await executeCommand<WorkspaceDirectory>({
        id: "workspace.list", context: { windowId: "main" },
        arguments: { path: "", maxEntries: EXPLORER_ENTRY_LIMIT },
      });
      setWorkspace(openedWorkspace);
      setWorkspaceDirectories({ "": listing });
      setExpandedDirectories([]);
      setWorkspaceDocumentIds({});
      setExplorerTab("folder");
      setActivePanel("files");
      setSidebarVisible(true);
      setStatus(`Opened ${openedWorkspace.name}`);
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not open workspace"));
    }
  };

  const closeWorkspace = async () => {
    try {
      await executeCommand({
        id: "workspace.close", context: { windowId: "main" }, arguments: {},
      });
      setWorkspace(null);
      setWorkspaceDirectories({});
      setExpandedDirectories([]);
      setWorkspaceDocumentIds({});
      setStatus("Workspace closed");
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not close workspace"));
    }
  };

  const toggleWorkspaceDirectory = async (path: string) => {
    if (expandedDirectories.includes(path)) {
      setExpandedDirectories((current) => current.filter((entry) => entry !== path));
      return;
    }
    if (!workspaceDirectories[path]
      && Object.keys(workspaceDirectories).length >= MAX_LOADED_WORKSPACE_DIRECTORIES) {
      setStatus(`Explorer directory limit (${MAX_LOADED_WORKSPACE_DIRECTORIES}) reached`);
      return;
    }
    setExpandedDirectories((current) => [...current, path]);
    if (workspaceDirectories[path]) return;
    try {
      const listing = await executeCommand<WorkspaceDirectory>({
        id: "workspace.list", context: { windowId: "main" },
        arguments: { path, maxEntries: EXPLORER_ENTRY_LIMIT },
      });
      setWorkspaceDirectories((current) => ({ ...current, [path]: listing }));
    } catch (error: unknown) {
      setExpandedDirectories((current) => current.filter((entry) => entry !== path));
      setStatus(errorMessage(error, "Could not list folder"));
    }
  };

  const openWorkspaceFile = async (entry: WorkspaceEntry) => {
    const existingId = workspaceDocumentIds[entry.relativePath];
    if (existingId && documents.some((document) => document.documentId === existingId)) {
      showDocument(existingId);
      return;
    }
    const expectedPath = workspace
      ? `${comparablePath(workspace.root)}/${comparablePath(entry.relativePath)}`
      : null;
    const restoredDocument = expectedPath ? documents.find(
      (document) => document.sourcePath && comparablePath(document.sourcePath) === expectedPath,
    ) : undefined;
    if (restoredDocument) {
      setWorkspaceDocumentIds((current) => ({
        ...current,
        [entry.relativePath]: restoredDocument.documentId,
      }));
      showDocument(restoredDocument.documentId);
      return;
    }
    try {
      const opened = await executeCommand<DocumentSummary>({
        id: "workspace.openFile", context: { windowId: "main" },
        arguments: { path: entry.relativePath },
      });
      setWorkspaceDocumentIds((current) => ({ ...current, [entry.relativePath]: opened.documentId }));
      addDocument(opened);
      setStatus(`Opened ${entry.name}`);
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not open workspace file"));
    }
  };

  /** Opens a file chosen in Places by absolute path, or focuses it when already open. */
  const openPath = async (path: string) => {
    const existing = documents.find((document) => document.sourcePath === path);
    if (existing) { showDocument(existing.documentId); return; }
    try {
      const opened = await executeCommand<DocumentSummary>({
        id: "file.open", context: { windowId: "main" }, arguments: { path },
      });
      addDocument(opened);
      setStatus(`Opened ${opened.name}`);
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not open file"));
    }
  };

  // Files from the system (Open With, a double-click, the Dock, the command
  // line) open once the previous session has been restored.
  const openPathRef = useRef(openPath);
  openPathRef.current = openPath;
  useEffect(() => {
    if (!desktopShell || !sessionReady) return undefined;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const openAll = async (paths: string[]) => {
      for (const path of paths) await openPathRef.current(path);
    };
    void takeLaunchPaths().then((paths) => { if (!disposed) void openAll(paths); })
      .catch((error: unknown) => setStatus(errorMessage(error, "Could not open the requested files")));
    void listen<string[]>(OPEN_PATHS_EVENT, (event) => { void openAll(event.payload); })
      .then((stop) => { if (disposed) stop(); else unlisten = stop; });
    return () => { disposed = true; unlisten?.(); };
  }, [desktopShell, sessionReady]);

  const finishCloseDocument = async (documentId: string, discard: boolean): Promise<boolean> => {
    try {
      await executeCommand({
        id: "file.close", context: { windowId: "main", documentId }, arguments: { discard },
      });
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not close document"));
      return false;
    }
    setDocuments((current) => {
      const remaining = current.filter((document) => document.documentId !== documentId);
      if (remaining.length < 2) setComparing(false);
      return remaining;
    });
    // The document session is gone, so every view of it closes in every tab.
    setTabs((current) => dropDocument(current, documentId));
    setLineChangeOverrides((current) => Object.fromEntries(
      Object.entries(current).filter(([key]) => key !== documentId),
    ));
    setWorkspaceDocumentIds((current) => Object.fromEntries(
      Object.entries(current).filter(([, value]) => value !== documentId),
    ));
    return true;
  };

  const closeDocument = (documentId: string) => {
    const document = documents.find((item) => item.documentId === documentId);
    if (document?.dirty) {
      setPendingCloseId(documentId);
      return;
    }
    void finishCloseDocument(documentId, false);
  };

  /** Closes one pane; the document itself closes only with its final view. */
  const closePaneView = (paneId: string) => {
    const tab = tabsRef.current.find((item) => item.id === activeTabIdRef.current);
    const pane = tab ? listPanes(tab.root).find((item) => item.id === paneId) : undefined;
    const documentId = pane ? paneDocument(pane) : null;
    if (!tab || !documentId) return;
    if (viewCount(tabsRef.current, documentId) > 1) {
      setTabs((current) => current.flatMap((item) => item.id === tab.id ? closePane(item, paneId) ?? [] : item));
      return;
    }
    closeDocument(documentId);
  };

  /**
   * Closes tabs one after another. Documents shown nowhere else are released,
   * prompting for each dirty one; cancelling keeps the remaining tabs.
   */
  const continueTabsClose = async (tabIds: string[], openDocuments: DocumentSummary[], current: WorkTab[]) => {
    let open = openDocuments;
    let remaining = current;
    for (const [index, tabId] of tabIds.entries()) {
      const tab = remaining.find((item) => item.id === tabId);
      if (!tab) continue;
      const others = remaining.filter((item) => item.id !== tabId);
      const shown = new Set(tabDocuments(tab));
      const orphans = open.filter((document) => shown.has(document.documentId)
        && viewCount(others, document.documentId) === 0);
      const dirty = orphans.find((document) => document.dirty);
      if (dirty) {
        setCloseFlow({ kind: "tabs", tabIds: tabIds.slice(index) });
        setPendingCloseId(dirty.documentId);
        return;
      }
      for (const orphan of orphans) {
        if (!await finishCloseDocument(orphan.documentId, false)) return;
      }
      open = open.filter((document) => !orphans.includes(document));
      remaining = others;
      setTabs((latest) => latest.filter((item) => item.id !== tabId));
    }
    setCloseFlow(null);
    setPendingCloseId(null);
  };

  const closeTab = (tabId: string) => { void continueTabsClose([tabId], documents, tabsRef.current); };
  const closeOtherTabs = (tabId: string) => {
    setActiveTabId(tabId);
    void continueTabsClose(tabsRef.current.filter((tab) => tab.id !== tabId).map((tab) => tab.id), documents, tabsRef.current);
  };

  const decideClose = async (decision: "save" | "discard" | "cancel") => {
    if (!pendingCloseId || busyDecision) return;
    if (decision === "cancel") { setPendingCloseId(null); setCloseFlow(null); return; }
    const document = documents.find((item) => item.documentId === pendingCloseId);
    if (!document) { setPendingCloseId(null); return; }
    setBusyDecision(true);
    try {
      if (decision === "save" && !await saveDocumentFor(document, false)) return;
      if (!await finishCloseDocument(document.documentId, decision === "discard")) return;
      const remaining = documents.filter((item) => item.documentId !== document.documentId);
      if (closeFlow?.kind === "tabs") {
        await continueTabsClose(closeFlow.tabIds, remaining, dropDocument(tabsRef.current, document.documentId));
        return;
      }
      const nextDirty = closeFlow?.kind === "quit" ? remaining.find((item) => item.dirty) : undefined;
      setPendingCloseId(nextDirty?.documentId ?? null);
      if (closeFlow?.kind === "quit" && !nextDirty) {
        setCloseFlow(null);
        try { await closeWindowAfterSave(); }
        catch (error) { setStatus(errorMessage(error, "Could not close window")); }
      }
    } finally {
      setBusyDecision(false);
    }
  };

  const restoreCandidate = async (candidate: RecoveryCandidate) => {
    if (busyDecision) return;
    setBusyDecision(true);
    try {
      const existing = candidate.sourcePath ? documents.find(
        (document) => document.sourcePath === candidate.sourcePath,
      ) : undefined;
      if (existing?.dirty) {
        setStatus(`Save or close ${existing.name} before restoring its checkpoint`);
        return;
      }
      const restored = await executeCommand<DocumentSummary>({
        id: "recovery.restore", context: { windowId: "main" },
        arguments: { recoveryId: candidate.recoveryId },
      });
      // The restored session takes over the remembered untitled tab or the stale file view.
      const placeholder = `${PENDING_PREFIX}${candidate.recoveryId}`;
      setDocuments((current) => current.some((item) => item.documentId === restored.documentId)
        ? current : [...current, restored]);
      setTabs((current) => revealIn(current.map((tab) => replaceDocument(
        existing ? replaceDocument(tab, existing.documentId, restored.documentId) : tab,
        placeholder, restored.documentId,
      )), restored.documentId));
      if (existing) await finishCloseDocument(existing.documentId, false);
      setRecoveryCandidates((current) => current.filter((item) => item.recoveryId !== candidate.recoveryId));
      setStatus(`Recovered ${restored.name}`);
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not restore document"));
    } finally {
      setBusyDecision(false);
    }
  };

  const discardCandidate = async (candidate: RecoveryCandidate) => {
    if (busyDecision) return;
    setBusyDecision(true);
    try {
      await executeCommand({
        id: "recovery.discard", context: { windowId: "main" },
        arguments: { recoveryId: candidate.recoveryId },
      });
      setRecoveryCandidates((current) => current.filter((item) => item.recoveryId !== candidate.recoveryId));
      setTabs((current) => dropDocument(current, `${PENDING_PREFIX}${candidate.recoveryId}`));
      setStatus(`Discarded recovery for ${candidate.name}`);
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not discard recovery"));
    } finally {
      setBusyDecision(false);
    }
  };

  const currentTab = () => tabsRef.current.find((tab) => tab.id === activeTabIdRef.current) ?? tabsRef.current[0];
  const applyTab = (result: LayoutResult) => {
    if (result.error !== undefined) { setStatus(result.error); return false; }
    const next = result.tab;
    setTabs((current) => current.map((tab) => tab.id === next.id ? next : tab));
    return true;
  };
  /** Drops a tab into the active tab's content: the two become one workspace. */
  const groupTab = (sourceTabId: string, targetPaneId: string, zone: DropZone) => {
    const current = tabsRef.current;
    const target = currentTab();
    const source = current.find((tab) => tab.id === sourceTabId);
    if (!target || !source || source.id === target.id) return;
    const fresh = !isWorkspace(target);
    const result = groupTabs(target, targetPaneId, zone, source, nextWorkspaceName(current));
    if (result.error !== undefined) { setStatus(result.error); return; }
    const grouped = result.tab;
    setTabs(current.filter((tab) => tab.id !== source.id).map((tab) => tab.id === grouped.id ? grouped : tab));
    setActiveTabId(grouped.id);
    // A new workspace starts with its name selected, ready to be typed over.
    if (fresh) setRenameRequest((request) => ({ tabId: grouped.id, signal: request.signal + 1 }));
    setStatus(`Grouped into ${grouped.name}`);
  };
  const groupWithNextTab = (zone: DropZone) => {
    const current = tabsRef.current;
    const target = currentTab();
    const next = target ? current[current.indexOf(target) + 1] : undefined;
    if (!target || !next) { setStatus("There is no next tab to group with"); return; }
    groupTab(next.id, target.activePane, zone);
  };
  const rearrangePane = (paneId: string, targetPaneId: string, zone: DropZone) => {
    const tab = currentTab();
    if (tab) applyTab(movePane(tab, paneId, targetPaneId, zone));
  };
  /** Lifts a pane out of its workspace into a tab of its own. */
  const popOutPane = (paneId: string, index?: number) => {
    const current = tabsRef.current;
    const tab = currentTab();
    const lifted = tab ? extractPane(tab, paneId, nextTabId(current)) : null;
    if (!tab || !lifted?.remaining) { setStatus("This tab has a single pane"); return; }
    const kept = current.map((item) => item.id === tab.id ? lifted.remaining as WorkTab : item);
    const at = index === undefined ? kept.findIndex((item) => item.id === tab.id) + 1 : Math.min(index, kept.length);
    setTabs([...kept.slice(0, at), lifted.extracted, ...kept.slice(at)]);
    setActiveTabId(lifted.extracted.id);
  };
  const ungroupWorkspace = () => {
    const current = tabsRef.current;
    const tab = currentTab();
    if (!tab || !isWorkspace(tab)) { setStatus("The active tab is not a workspace"); return; }
    const index = current.indexOf(tab);
    setTabs([...current.slice(0, index), ...ungroupTab(tab, current), ...current.slice(index + 1)]);
    setStatus(`Ungrouped ${tab.name}`);
  };
  /** Opens another view of the active document in a new pane. */
  const splitActive = (zone: "right" | "bottom") => {
    const tab = currentTab();
    const documentId = tab ? tabActiveDocument(tab) : null;
    if (!tab || !documentId) { setStatus("Open a document before splitting the editor"); return; }
    applyTab(splitPane(tab, tab.activePane, zone, documentId, nextPaneId(tabsRef.current), nextWorkspaceName(tabsRef.current)));
  };
  const toggleSplit = () => {
    const tab = currentTab();
    if (tab && isWorkspace(tab)) closePaneView(tab.activePane);
    else splitActive("right");
  };
  const cyclePane = (offset: number) => {
    updateActiveTab((tab) => {
      const panes = listPanes(tab.root);
      const index = panes.findIndex((pane) => pane.id === tab.activePane);
      return { ...tab, activePane: panes[(index + offset + panes.length) % panes.length].id };
    });
  };
  const reorderTab = (tabId: string, index: number) => {
    setTabs((current) => {
      const from = current.findIndex((tab) => tab.id === tabId);
      if (from < 0 || index < 0 || index >= current.length) return current;
      const next = [...current];
      const [moved] = next.splice(from, 1);
      next.splice(index, 0, moved);
      return next;
    });
  };
  const cycleTab = (offset: number) => {
    const current = tabsRef.current;
    if (current.length === 0) return;
    const index = current.findIndex((tab) => tab.id === activeTabIdRef.current);
    setActiveTabId(current[(index + offset + current.length) % current.length].id);
  };
  const shiftTab = (offset: number) => {
    const index = tabsRef.current.findIndex((tab) => tab.id === activeTabIdRef.current);
    if (index >= 0) reorderTab(tabsRef.current[index].id, index + offset);
  };
  const renameTab = (tabId: string, name: string) => {
    setTabs((current) => current.map((tab) => tab.id === tabId ? { ...tab, name } : tab));
    setStatus(`Workspace renamed to ${name}`);
  };
  const requestRename = () => {
    const tab = currentTab();
    if (!tab || !isWorkspace(tab)) { setStatus("Group tabs into a workspace before renaming it"); return; }
    setRenameRequest((request) => ({ tabId: tab.id, signal: request.signal + 1 }));
  };
  const toggleLineChangeDefault = () => {
    const next = !lineChangeDefault;
    saveLineChangeDefault(next);
    setLineChangeDefault(next);
    setLineChangeOverrides({});
    setStatus(next ? "Line change indicators shown" : "Line change indicators hidden");
  };
  const openSettings = () => {
    if (desktopShell) {
      void openSettingsWindow().catch((error: unknown) => setStatus(errorMessage(error, "Could not open Settings")));
    } else {
      setSettingsOpen(true);
    }
  };
  const openHelp = () => {
    if (desktopShell) {
      void openHelpWindow().catch((error: unknown) => setStatus(errorMessage(error, "Could not open Help")));
    } else {
      setHelpOpen(true);
    }
  };
  const toggleReadOnly = async (documentId: string) => {
    try {
      const updated = await executeCommand<DocumentSummary>({
        id: "editor.toggleReadOnly", context: { windowId: "main", documentId }, arguments: {},
      });
      upsertDocument(updated);
      setStatus(updated.readOnly ? `${updated.name} is read-only` : `${updated.name} can be edited`);
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not change the mode"));
    }
  };
  const toggleViewMode = (documentId: string) => {
    setViewModes((current) => ({
      ...current, [documentId]: current[documentId] === "hex" ? "text" : "hex",
    }));
  };

  const chooseFolder = async (): Promise<string | null> => {
    const selected = await open({ multiple: false, directory: true, title: "Choose a folder to search" });
    return typeof selected === "string" ? selected : null;
  };
  const compareFolders = async () => {
    try {
      const left = await open({ multiple: false, directory: true, title: "Choose the left folder" });
      if (typeof left !== "string") return;
      const right = await open({ multiple: false, directory: true, title: "Choose the right folder" });
      if (typeof right !== "string") return;
      setStatus("Comparing folders in bounded chunks…");
      const result = await executeCommand<FolderCompareResult>({
        id: "compare.folders", context: { windowId: "main" },
        arguments: { leftRoot: left, rightRoot: right, maxResults: 1000 },
      });
      setFolderComparison(result);
      setComparing(false);
      setStatus(`${result.entries.length} folder differences`);
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not compare folders"));
    }
  };
  /** Shows a search result: an open document, or a file that is opened first. */
  const openMatch = async (match: MatchTarget) => {
    try {
      let target = match.documentId
        ? documents.find((document) => document.documentId === match.documentId)
        : documents.find((document) => document.sourcePath === match.path);
      if (!target && match.path) {
        target = await executeCommand<DocumentSummary>({
          id: "file.open", context: { windowId: "main" }, arguments: { path: match.path },
        });
        addDocument(target);
      } else if (target) {
        showDocument(target.documentId);
      } else {
        setStatus("That document is no longer open");
        return;
      }
      await executeCommand({
        id: "editor.setSelections", context: { windowId: "main", documentId: target.documentId },
        arguments: { selections: [{ anchorByte: match.startByte, activeByte: match.endByte }] },
      });
      setRevealTarget({ documentId: target.documentId, byteOffset: match.startByte });
      setStatus(`Opened match in ${target.name}`);
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not open search result"));
    }
  };

  /** Runs a JSON or XML tool on the focused document. */
  const runTool = async (commandId: string) => {
    if (!activeDocument || !toolAvailable(commandId)) {
      setStatus(commandId === "tools.minify" ? "Minify is available for JSON documents"
        : "These tools are available for JSON and XML documents");
      return;
    }
    const context = { windowId: "main", documentId: activeDocument.documentId };
    const validate = async () => {
      const report = await executeCommand<ValidationReport>({ id: "tools.validate", context, arguments: {} });
      setStatus(report.message);
      if (report.valid || report.byteOffset === undefined) return;
      // Put the caret on the problem and bring it into view.
      await executeCommand({
        id: "editor.setSelections", context,
        arguments: { selections: [{ anchorByte: report.byteOffset, activeByte: report.byteOffset }] },
      });
      setRevealTarget({ documentId: activeDocument.documentId, byteOffset: report.byteOffset });
      setMacroRefreshVersion((version) => version + 1);
    };
    if (commandId === "tools.validate") { await validate(); return; }
    try {
      const updated = await executeCommand<DocumentSummary>({
        id: commandId, context, arguments: { expectedRevision: activeDocument.revision },
      });
      if (updated.revision === activeDocument.revision) {
        setStatus(`${toolLabel} is already ${commandId === "tools.minify" ? "minified" : "formatted"}`);
        return;
      }
      upsertDocument(updated);
      setMacroRefreshVersion((version) => version + 1);
      setStatus(`${commandId === "tools.minify" ? "Minified" : "Beautified"} ${toolLabel}`);
    } catch (error: unknown) {
      setStatus(errorMessage(error, `Could not format ${toolLabel}`));
      // An invalid document is left untouched; show where the problem is.
      if (errorMessage(error, "").startsWith("Invalid")) await validate();
    }
  };

  /** Local effects of the presentation-only commands; one dispatch path for every entry point. */
  const workbenchActions: Record<string, (() => void | Promise<void>) | undefined> = {
    "workbench.toggleSplit": toggleSplit,
    "workbench.splitRight": () => splitActive("right"),
    "workbench.splitDown": () => splitActive("bottom"),
    // Closes the file in the focused pane; a workspace keeps its other panes.
    "workbench.closeFile": () => {
      const tab = currentTab();
      if (tab) closePaneView(tab.activePane); else setStatus("No file is open");
    },
    "workbench.closeTab": () => { const tab = currentTab(); if (tab) closeTab(tab.id); },
    "workbench.openSettings": openSettings,
    "workbench.openHelp": openHelp,
    "workbench.nextTab": () => cycleTab(1),
    "workbench.previousTab": () => cycleTab(-1),
    "workbench.moveTabLeft": () => shiftTab(-1),
    "workbench.moveTabRight": () => shiftTab(1),
    "view.toggleLineChangeIndicators": toggleLineChangeDefault,
    "workspace.rename": requestRename,
    "workspace.groupRight": () => groupWithNextTab("right"),
    "workspace.groupDown": () => groupWithNextTab("bottom"),
    "workspace.ungroup": ungroupWorkspace,
    "workspace.popOutPane": () => { const tab = currentTab(); if (tab) popOutPane(tab.activePane); },
    "workspace.closePane": () => { const tab = currentTab(); if (tab) closePaneView(tab.activePane); },
    "workspace.focusNextPane": () => cyclePane(1),
    "workspace.focusPreviousPane": () => cyclePane(-1),
    "workspace.growPane": () => updateActiveTab((tab) => resizeActivePane(tab, 50)),
    "workspace.shrinkPane": () => updateActiveTab((tab) => resizeActivePane(tab, -50)),
  };

  const runPaletteCommand = async (command: CommandDescriptor) => {
    if (!sessionReady) { setStatus("Restoring the previous session…"); return; }
    setPaletteOpen(false);
    setPaletteQuery("");
    try {
      if (command.id === "app.about") { setLicenseOpen(false); setAboutOpen(true); return; }
      if (command.id === "file.new") return void await newFile();
      if (command.id === "file.open") return void await openFile();
      if (command.id === "file.save") return void await saveDocument(false);
      if (command.id === "file.saveAs") return void await saveDocument(true);
      if (command.id === "workspace.open") return void await openWorkspace();
      if (command.id === "workspace.close") return void await closeWorkspace();
      if (command.id === "workbench.toggleSidebar") {
        await executeCommand({ id: command.id, context: { windowId: "main" }, arguments: {} });
        setSidebarVisible((visible) => !visible);
        return;
      }
      if (command.id.startsWith("tools.")) return void await runTool(command.id);
      const workbenchAction = workbenchActions[command.id];
      if (workbenchAction) {
        await executeCommand({ id: command.id, context: { windowId: "main" }, arguments: {} });
        await workbenchAction();
        return;
      }
      if (command.id === "workbench.toggleEditorMode") {
        if (!activeDocument) { setStatus("Open a document before changing editor mode"); return; }
        await executeCommand({ id: command.id, context: { windowId: "main" }, arguments: {} });
        toggleViewMode(activeDocument.documentId);
        return;
      }
      if (command.id === "file.reload") return void requestReload(activeDocument);
      if (command.id === "editor.toggleReadOnly") {
        if (!activeDocument) { setStatus("No active document"); return; }
        return void await toggleReadOnly(activeDocument.documentId);
      }
      if (command.id === "editor.undo" || command.id === "editor.redo") {
        if (!activeDocument) { setStatus("No active document"); return; }
        upsertDocument(await executeCommand<DocumentSummary>({
          id: command.id,
          context: { windowId: "main", documentId: activeDocument.documentId }, arguments: {},
        }));
        setStatus(command.title);
        return;
      }
      const result = await executeCommand<{
        productName?: string; version?: string; pluginId?: string; messages?: string[];
      }>({
        id: command.id,
        context: { windowId: "main", documentId: activeDocumentId ?? undefined }, arguments: {},
      });
      if (result.pluginId) {
        setStatus(result.messages?.at(-1) ?? `Ran ${command.title}`);
      } else {
        setStatus(result.productName ? `${result.productName} ${result.version ?? ""}`.trim() : command.title);
      }
    } catch (error: unknown) {
      setStatus(errorMessage(error, `Could not run ${command.title}`));
    }
  };

  nativeMenuActionRef.current = (commandId) => {
    if (commandId === "workbench.commandPalette") {
      if (sessionReady) { setPaletteOpen(true); setPaletteQuery(""); setPaletteIndex(0); }
      return;
    }
    if (commandId === "workbench.openPlugins") { if (sessionReady) setPluginOpen(true); return; }
    if (commandId === "file.reload") { requestReload(activeDocument); return; }
    const command = commands.find(({ id }) => id === commandId)
      ?? (workbenchActions[commandId] || commandId.startsWith("tools.")
        ? { id: commandId, title: commandId, category: "View" } : undefined);
    if (command) void runPaletteCommand(command);
  };

  const handleSettingsAction = (action: SettingsMainAction) => {
    setSettingsOpen(false);
    nativeMenuActionRef.current(action);
  };

  const renderWorkspaceEntries = (path: string, depth = 0): ReactNode => {
    const directory = workspaceDirectories[path];
    if (!directory) {
      return <div className="tree-loading" style={{ paddingLeft: 12 + depth * 14 }}>Loading…</div>;
    }
    return <>
      {directory.entries.map((entry) => {
        const isDirectory = entry.kind === "directory";
        const expanded = isDirectory && expandedDirectories.includes(entry.relativePath);
        return <div key={entry.relativePath}>
          <button
            className={`tree-row ${entry.kind}`}
            type="button"
            style={{ paddingLeft: 8 + depth * 14 }}
            disabled={entry.kind === "symlink" || entry.kind === "other"}
            aria-expanded={isDirectory ? expanded : undefined}
            title={entry.kind === "symlink" ? "Symlinks are shown but not traversed" : entry.relativePath}
            onClick={() => {
              if (isDirectory) void toggleWorkspaceDirectory(entry.relativePath);
              if (entry.kind === "file") void openWorkspaceFile(entry);
            }}
          >
            <span className="tree-icon" aria-hidden="true">{isDirectory ? (expanded ? "▾" : "▸") : entry.kind === "file" ? "·" : "↗"}</span>
            <span className="tree-name">{entry.name}</span>
            {entry.byteLen ? <span className="tree-size">{entry.byteLen}</span> : null}
          </button>
          {expanded ? renderWorkspaceEntries(entry.relativePath, depth + 1) : null}
        </div>;
      })}
      {directory.truncated ? <p className="tree-limit">First {EXPLORER_ENTRY_LIMIT} entries shown</p> : null}
    </>;
  };

  const layoutStyle = {
    "--sidebar-width": `${sidebarWidth}px`,
    ...preferenceCssVariables(preferences, themes),
  } as CSSProperties;

  const forgetPrompts = (documentId: string) => {
    for (const key of [...promptedChanges.current]) if (key.startsWith(`${documentId}:`)) promptedChanges.current.delete(key);
  };

  /** Replaces a document's text with its file as it is now. */
  const reloadDocument = async (documentId: string, discard: boolean) => {
    try {
      const reloaded = await executeCommand<DocumentSummary>({
        id: "file.reload", context: { windowId: "main", documentId }, arguments: { discard },
      });
      if (reloaded) upsertDocument(reloaded);
      forgetPrompts(documentId);
      setDocumentEpochs((current) => ({ ...current, [documentId]: (current[documentId] ?? 0) + 1 }));
      setStatus(`Reloaded ${reloaded?.name ?? "the file"} from disk`);
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not reload the file"));
    }
  };

  /** File ▸ Reload from Disk: asks first when unsaved edits would be lost. */
  const requestReload = (document: DocumentSummary | null | undefined) => {
    if (!document?.sourcePath || document.remoteUri) { setStatus("Only a file on this computer can be reloaded"); return; }
    if (document.dirty) setExternalPrompt({ kind: "confirmReload", document });
    else void reloadDocument(document.documentId, false);
  };

  const keepEditorVersion = async (documentId: string) => {
    try {
      const kept = await executeCommand<DocumentSummary>({
        id: "editor.keepExternalVersion", context: { windowId: "main", documentId }, arguments: {},
      });
      if (kept) upsertDocument(kept);
      setStatus("Kept the version in the editor; saving will replace the file");
    } catch (error: unknown) {
      setStatus(errorMessage(error, "Could not keep the editor's version"));
    }
  };

  const decideExternal = async (choice: "reload" | "keep" | "close" | "cancel") => {
    const prompt = externalPrompt;
    setExternalPrompt(null);
    if (!prompt || choice === "cancel") return;
    const { documentId } = prompt.document;
    if (choice === "reload") await reloadDocument(documentId, true);
    else if (choice === "keep") await keepEditorVersion(documentId);
    else closeDocument(documentId);
  };

  /** Asks the core which open files were changed by other programs. */
  const checkExternalChanges = async () => {
    if (externalPromptRef.current || !sessionReady) return;
    try {
      const changes = await executeCommand<ExternalChange[]>({
        id: "editor.externalChanges", context: { windowId: "main" }, arguments: {},
      });
      if (!Array.isArray(changes)) return;
      const next = changes.find((change) => !promptedChanges.current.has(`${change.documentId}:${change.kind}`));
      const document = next && documentsRef.current.find((item) => item.documentId === next.documentId);
      if (!next || !document) return;
      promptedChanges.current.add(`${next.documentId}:${next.kind}`);
      setExternalPrompt({ kind: next.kind, document: { ...document, dirty: next.dirty } });
    } catch {
      // A failed check is retried on the next focus or tick.
    }
  };
  checkExternalRef.current = checkExternalChanges;

  /** Opens the Find and Replace panel on a tab, optionally with text to find. */
  const openFindPanel = (tab: FindTab, seed?: string) => {
    if (desktopShell) {
      // A new window asks for this once it is listening; an open one gets it now.
      pendingFindOpen.current = { tab, seed };
      void openFindWindow().catch((error: unknown) => setStatus(errorMessage(error, "Could not open Find and Replace")));
      void emitTo("find", FIND_EVENTS.open, { tab, seed }).catch(() => undefined);
      return;
    }
    setFindTab(tab);
    if (seed) setFindSeed((previous) => ({ id: (previous?.id ?? 0) + 1, text: seed }));
  };

  /** Shows a list of matching lines: its own window on the desktop, a panel otherwise. */
  const showSearchResults = (list: ResultList) => {
    if (!desktopShell) { setSearchResults(list); return; }
    resultsForWindow.current = list;
    void openResultsWindow().catch((error: unknown) => setStatus(errorMessage(error, "Could not show the results")));
    sendResultsToWindow();
  };
  const sendResultsToWindow = () => {
    const list = resultsForWindow.current;
    if (!list) return;
    void emitTo("find-results", FIND_EVENTS.results, {
      ...list, openDocumentIds: documentsRef.current.map((document) => document.documentId),
    }).catch(() => undefined);
  };

  /** Sends a find or replace action to the editor in the focused pane. */
  const searchInEditor = (request: Omit<SearchRequest, "id">) => {
    if (!activeDocument || viewModes[activeDocument.documentId] === "hex") {
      if (request.action !== "highlight") setStatus("Open a text document to search in");
      return;
    }
    setSearchRequest((previous) => ({ ...request, id: (previous?.id ?? 0) + 1 }));
  };

  const copyPath = (document: DocumentSummary) => {
    void writeText(document.sourcePath ?? document.name)
      .then(() => setStatus("Copied file path"))
      .catch((error: unknown) => setStatus(errorMessage(error, "Could not copy the path")));
  };
  /** The right-click menu of a tab: close, save, and workspace actions. */
  const openTabMenu = (tabId: string, x: number, y: number) => {
    const tab = tabs.find((item) => item.id === tabId);
    if (!tab) return;
    const document = documents.find((item) => item.documentId === tabActiveDocument(tab));
    const workspaceTab = isWorkspace(tab);
    const items: MenuItem[] = [
      { label: "Close", shortcut: shortcutFor("workbench.closeFile"), onSelect: () => closeTab(tabId) },
      { label: "Close All Files Except This", disabled: tabs.length < 2, onSelect: () => closeOtherTabs(tabId) },
      { kind: "separator" },
      { label: "Save", shortcut: shortcutFor("file.save"), disabled: !document?.dirty && Boolean(document?.sourcePath),
        onSelect: () => { if (document) void saveDocumentFor(document, false); } },
      { label: "Save As…", shortcut: shortcutFor("file.saveAs"), disabled: !document,
        onSelect: () => { if (document) void saveDocumentFor(document, true); } },
      { kind: "separator" },
      { label: "Copy File Path", disabled: !document, onSelect: () => { if (document) copyPath(document); } },
    ];
    if (workspaceTab) {
      items.push({ kind: "separator" },
        { label: "Rename Workspace", onSelect: () => setRenameRequest((request) => ({ tabId, signal: request.signal + 1 })) },
        { label: "Ungroup Workspace", onSelect: () => { setActiveTabId(tabId); activeTabIdRef.current = tabId; ungroupWorkspace(); } });
    }
    setContextMenu({ x, y, label: `${workspaceTab ? tab.name : document?.name ?? "Tab"} menu`, items });
  };
  const openPaneMenu = (paneId: string, x: number, y: number) => {
    const pane = activeTab ? listPanes(activeTab.root).find((item) => item.id === paneId) : undefined;
    const document = documents.find((item) => item.documentId === (pane ? paneDocument(pane) : null));
    if (!pane) return;
    setContextMenu({ x, y, label: `${document?.name ?? "Pane"} menu`, items: [
      { label: "Close", onSelect: () => closePaneView(paneId) },
      { label: "Move to New Tab", onSelect: () => popOutPane(paneId) },
      { kind: "separator" },
      { label: "Save", shortcut: shortcutFor("file.save"), disabled: !document?.dirty && Boolean(document?.sourcePath),
        onSelect: () => { if (document) void saveDocumentFor(document, false); } },
      { label: "Save As…", shortcut: shortcutFor("file.saveAs"), disabled: !document,
        onSelect: () => { if (document) void saveDocumentFor(document, true); } },
      { kind: "separator" },
      { label: "Copy File Path", disabled: !document, onSelect: () => { if (document) copyPath(document); } },
    ] });
  };
  const openDocumentMenu = (documentId: string, x: number, y: number) => {
    const holder = tabs.find((tab) => tabDocuments(tab).includes(documentId));
    if (holder) openTabMenu(holder.id, x, y);
  };

  const shortcutFor = (commandId: string) => shortcutLabel(
    customization.keybindings.find(({ command }) => command === commandId)?.key,
  );
  const commandTitle = (commandId: string) => commands.find(({ id }) => id === commandId)?.title ?? commandId;
  const runById = (commandId: string) => nativeMenuActionRef.current(commandId);
  const hasNextTab = activeTab ? tabs.indexOf(activeTab) < tabs.length - 1 : false;
  const toolItems = toolKind === null ? [] : [
    { command: "tools.validate", caption: "Validate", label: `Validate ${toolLabel}` },
    { command: "tools.beautify", caption: "Beautify", label: `Beautify ${toolLabel}` },
    ...(toolKind === "json" ? [{ command: "tools.minify", caption: "Minify", label: "Minify JSON" }] : []),
  ];
  const needsDocument = (commandId: string) => commandId.startsWith("file.save") || commandId === "workbench.closeFile";
  const toolbarItems: ToolbarItem[] = [
    ...customization.toolbar.flatMap((commandId): ToolbarItem[] => {
      const command = commands.find(({ id }) => id === commandId);
      // A tool placed on the toolbar still shows only for its kind of document.
      if (commandId.startsWith("tools.") && !toolAvailable(commandId)) return [];
      return command ? [{
        id: commandId, label: `Toolbar: ${command.title}`, title: command.title,
        caption: TOOLBAR_CAPTIONS[commandId] ?? command.title.replace(/…$/, ""),
        icon: iconForCommand(commandId), shortcut: shortcutFor(commandId) || undefined,
        essential: commandId.startsWith("file."),
        disabled: !sessionReady || (needsDocument(commandId) && !activeDocument),
        onSelect: () => runById(commandId),
      }] : [];
    }),
    ...toolItems.filter((item) => !customization.toolbar.includes(item.command)).map((item): ToolbarItem => ({
      id: `nulis.${item.command}`, caption: item.caption, label: item.label, title: item.label, icon: iconForCommand(item.command),
      shortcut: shortcutFor(item.command) || undefined, onSelect: () => runById(item.command),
    })),
    { id: "nulis.splitRight", caption: "Split Right", label: "Split editor right", title: "Split Horizontal", icon: "splitHorizontal",
      shortcut: shortcutFor("workbench.splitRight") || undefined, disabled: !activeDocument, onSelect: () => runById("workbench.splitRight") },
    { id: "nulis.splitDown", caption: "Split Down", label: "Split editor down", title: "Split Vertical", icon: "splitVertical",
      shortcut: shortcutFor("workbench.splitDown") || undefined, disabled: !activeDocument, onSelect: () => runById("workbench.splitDown") },
    { id: "nulis.files", caption: "Sidebar", label: "Toggle Files side bar", title: "Files", icon: "sidebar", pressed: sidebarVisible,
      shortcut: shortcutFor("workbench.toggleSidebar") || undefined, onSelect: () => setSidebarVisible((visible) => !visible) },
    { id: "nulis.automation", caption: "Automation", label: macroOpen ? "Close automation" : "Open automation", title: "Automation", icon: "automation",
      pressed: macroOpen, disabled: !sessionReady, onSelect: () => setMacroOpen((shown) => !shown) },
    { id: "nulis.plugins", caption: "Plugins", label: pluginOpen ? "Close plugins" : "Open plugins", title: "Plugins", icon: "plugins",
      pressed: pluginOpen, disabled: !sessionReady, onSelect: () => setPluginOpen((shown) => !shown) },
    { id: "nulis.settings", caption: "Settings", label: settingsOpen ? "Close settings" : "Open settings", title: "Settings", icon: "settings",
      essential: true, pressed: settingsOpen, shortcut: shortcutLabel("Primary+,"),
      onSelect: () => { if (settingsOpen) setSettingsOpen(false); else openSettings(); } },
  ];
  const fileMenu: MenuItem[] = customization.menu.map((entry) => entry.kind === "separator" ? entry : {
    label: commandTitle(entry.command), shortcut: shortcutFor(entry.command),
    disabled: needsDocument(entry.command) && !activeDocument,
    onSelect: () => runById(entry.command),
  });
  // Reload and Close File are always offered, even in a customized menu that omits them.
  if (!customization.menu.some((entry) => entry.kind === "command" && entry.command === "file.reload")) {
    fileMenu.push({ kind: "separator" }, {
      label: "Reload from Disk", disabled: !activeDocument?.sourcePath || Boolean(activeDocument.remoteUri),
      onSelect: () => runById("file.reload"),
    });
  }
  if (!customization.menu.some((entry) => entry.kind === "command" && entry.command === "workbench.closeFile")) {
    fileMenu.push({ kind: "separator" }, {
      label: "Close File", shortcut: shortcutFor("workbench.closeFile"), disabled: !activeDocument,
      onSelect: () => runById("workbench.closeFile"),
    });
  }
  const viewMenu: MenuItem[] = [
    { label: "Line Change Indicators", checked: lineChangeDefault, shortcut: shortcutFor("view.toggleLineChangeIndicators"),
      onSelect: () => runById("view.toggleLineChangeIndicators") },
    { label: "Primary Side Bar", checked: sidebarVisible, onSelect: () => setSidebarVisible((visible) => !visible) },
    { label: "Read-Only Mode", checked: activeDocument?.readOnly === true, disabled: !activeDocument,
      shortcut: shortcutFor("editor.toggleReadOnly"), onSelect: () => runById("editor.toggleReadOnly") },
    { kind: "separator" },
    { label: "Split Editor Right", disabled: !activeDocument, onSelect: () => runById("workbench.splitRight") },
    { label: "Split Editor Down", disabled: !activeDocument, onSelect: () => runById("workbench.splitDown") },
    { kind: "separator" },
    { label: "Settings…", shortcut: shortcutLabel("Primary+,"), onSelect: () => runById("workbench.openSettings") },
  ];
  const helpMenu: MenuItem[] = [
    { label: "Nulis Help", shortcut: "F1", onSelect: () => runById("workbench.openHelp") },
    { kind: "separator" },
    { label: "About Nulis", onSelect: () => runById("app.about") },
  ];
  const workspaceMenu: MenuItem[] = [
    { label: "Rename Workspace", disabled: !activeWorkspace, onSelect: () => runById("workspace.rename") },
    { label: "Group with Next Tab Side by Side", disabled: !hasNextTab, onSelect: () => runById("workspace.groupRight") },
    { label: "Group with Next Tab Stacked", disabled: !hasNextTab, onSelect: () => runById("workspace.groupDown") },
    { label: "Ungroup Workspace", disabled: !activeWorkspace, onSelect: () => runById("workspace.ungroup") },
    { kind: "separator" },
    { label: "Move Pane to New Tab", disabled: !activeWorkspace, onSelect: () => runById("workspace.popOutPane") },
    { label: "Close Pane", disabled: !activeWorkspace, onSelect: () => runById("workspace.closePane") },
    { label: "Focus Next Pane", disabled: !activeWorkspace, onSelect: () => runById("workspace.focusNextPane") },
    { kind: "separator" },
    { label: "Next Tab", disabled: tabs.length < 2, onSelect: () => runById("workbench.nextTab") },
    { label: "Move Tab Left", disabled: tabs.length < 2, onSelect: () => runById("workbench.moveTabLeft") },
    { label: "Move Tab Right", disabled: tabs.length < 2, onSelect: () => runById("workbench.moveTabRight") },
  ];

  // The character count follows edits, a little behind so typing never waits for it.
  const countedId = activeDocument?.documentId;
  const countedRevision = activeDocument?.revision;
  const countedEncoding = activeDocument?.encoding;
  useEffect(() => {
    if (!countedId) return undefined;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void executeCommand<CharacterCount>({
        id: "editor.characterCount", context: { windowId: "main", documentId: countedId }, arguments: {},
      }).then((count) => {
        if (!cancelled && count) setCharacterCount({ ...count, documentId: countedId });
      }).catch(() => undefined);
    }, 250);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [countedEncoding, countedId, countedRevision]);
  const hexMode = activeDocument ? viewModes[activeDocument.documentId] === "hex" : false;
  const activeCaret = caret && !hexMode && caret.documentId === activeDocument?.documentId ? caret : null;
  const activeCharacters = characterCount && characterCount.documentId === activeDocument?.documentId
    ? characterCount.characters : undefined;

  const renderView = (document: DocumentSummary, pane: PaneNode, active: boolean, ordinal: number, total: number) => {
    const viewId = total > 1 ? `pane ${ordinal}` : "primary";
    return viewModes[document.documentId] === "hex"
      ? <HexViewport key={`${pane.id}-${macroRefreshVersion}-${documentEpochs[document.documentId] ?? 0}`} document={document} onDocumentChange={upsertDocument}
          onStatus={setStatus} viewId={pane.id} />
      : <EditorViewport key={`${pane.id}-${macroRefreshVersion}-${documentEpochs[document.documentId] ?? 0}`} document={document} onDocumentChange={upsertDocument}
          onStatus={setStatus} viewId={viewId} autoFocus={active}
          lineHeight={preferences.fontSize * preferences.lineHeight} languages={languages}
          showLineChanges={lineChangeOverrides[document.documentId] ?? lineChangeDefault}
          onToggleLineChanges={() => setLineChangeOverrides((current) => ({
            ...current, [document.documentId]: !(current[document.documentId] ?? lineChangeDefault),
          }))}
          onCaret={active ? (position) => setCaret(position ? { ...position, documentId: document.documentId } : null) : undefined}
          onToggleReadOnly={() => void toggleReadOnly(document.documentId)}
          onToggleHexMode={() => toggleViewMode(document.documentId)}
          onCopyPath={() => copyPath(document)}
          onReload={document.sourcePath && !document.remoteUri ? () => requestReload(document) : undefined}
          onOpenFind={active ? openFindPanel : undefined}
          searchRequest={active ? searchRequest : undefined}
          revealOffset={active && revealTarget?.documentId === document.documentId ? revealTarget.byteOffset : undefined} />;
  };

  // Without the welcome screen, the application starts on a new empty document.
  useEffect(() => {
    if (!sessionReady || startupDocumentRef.current) return;
    startupDocumentRef.current = true;
    if (!welcomeOnStartup && tabs.length === 0 && recoveryCandidates.length === 0) void newFile();
    // Runs once, when the previous session has been restored.
  }, [sessionReady]);

  const productCopy = <>
    <p className="eyebrow">PROFESSIONAL EDITING, BUILT TO SCALE</p>
    <h1>Write, inspect,<br />and understand.</h1>
    <p className="lede">Nulis is an original cross-platform editor foundation for text, code, binary data, and files larger than memory.</p>
  </>;
  const productFacts = <dl className="foundation-stats"><div><dt>{commands.length}</dt><dd>core commands</dd></div>
    <div><dt>64-bit</dt><dd>file coordinates</dd></div>
    <div><dt>v{info?.ipcProtocolVersion ?? "—"}</dt><dd>IPC contract</dd></div>
  </dl>;
  const welcome = !welcomeVisible ? <div className="no-documents">No file is open</div> : <div className="welcome">
    {productCopy}
    <div className="welcome-actions">
      <button className="primary" type="button" disabled={!sessionReady} onClick={() => void newFile()}>New text file</button>
      <button type="button" disabled={!sessionReady} onClick={() => void openFile()}>Open a file</button>
      <button type="button" disabled={!sessionReady} onClick={() => void openWorkspace()}>Open a folder</button>
    </div>
    <label className="welcome-startup"><input type="checkbox" checked={!welcomeOnStartup}
      onChange={(event) => {
        const show = !event.currentTarget.checked;
        saveWelcomeOnStartup(show);
        setWelcomeOnStartup(show);
      }} /><span>Do not show this on startup</span></label>
    {productFacts}
  </div>;

  return (
    <main className={`workbench${sidebarVisible ? "" : " sidebar-hidden"}${resizingSidebar ? " resizing" : ""}`} style={layoutStyle}>
      {dragOver ? <div className="file-drop-target" aria-hidden="true">Drop files to open</div> : null}
      <header className="titlebar">
        <button className="brand" type="button" title="About Nulis" aria-label="About Nulis" onClick={() => { setLicenseOpen(false); setAboutOpen(true); }}>
          <span className="brand-mark" aria-hidden="true">N</span>
          <span className="title-copy"><strong>Nulis</strong></span>
        </button>
        {!desktopShell ? <nav className="menubar" aria-label="Application menu">
          <MenuButton name="File" items={fileMenu} />
          <MenuButton name="View" items={viewMenu} />
          {toolItems.length > 0 ? <MenuButton name="Tools" items={toolItems.map((item) => ({
            label: item.label, shortcut: shortcutFor(item.command), onSelect: () => runById(item.command),
          }))} /> : null}
          <MenuButton name="Workspace" items={workspaceMenu} />
          <MenuButton name="Help" items={helpMenu} />
        </nav> : null}
        <OverflowToolbar className="custom-toolbar" label="Custom toolbar" items={toolbarItems} />
        <IconButton icon="command" label="Open command palette" tooltip="Search commands" caption="Commands"
          className="command-button" shortcut={shortcutFor("workbench.commandPalette") || undefined}
          disabled={!sessionReady} onClick={() => setPaletteOpen(true)} />
      </header>

      <aside className="activitybar" aria-label="Primary navigation">
        {NAV_ITEMS.map((item) => {
          const panel = item.toLowerCase() as "files" | "remote";
          const available = item === "Files" || item === "Remote";
          const selected = available && activePanel === panel && sidebarVisible;
          return <button
            className={selected ? "active" : ""} type="button" key={item}
            disabled={!available || !sessionReady} aria-pressed={selected}
            onClick={() => {
              if (!available) return;
              if (activePanel === panel && sidebarVisible) setSidebarVisible(false);
              else { setActivePanel(panel); setSidebarVisible(true); }
            }}
          ><Icon name={NAV_ICONS[item]} /><small>{item}</small></button>;
        })}
      </aside>

      <section className="sidebar" aria-label={activePanel === "remote" ? "Remote" : "Explorer"}>
        {activePanel === "remote" ? <RemotePanel onOpen={addDocument} onStatus={setStatus} />
        : <FileExplorer tab={explorerTab} onTab={(next) => { setExplorerTab(next); saveExplorerTab(next); }}
          documents={documents} activeDocumentId={activeDocumentId} onShowDocument={showDocument}
          onOpenPath={(path) => void openPath(path)} onStatus={setStatus} onDocumentMenu={openDocumentMenu}
          folder={<>
          <div className="panel-heading"><span>EXPLORER</span><div>
            <IconButton icon="open" label="Open workspace" onClick={() => void openWorkspace()} />
            {workspace ? <IconButton icon="close" label="Close workspace" onClick={() => void closeWorkspace()} /> : null}
          </div></div>
          {workspace ? <>
            <div className="workspace-heading" title={workspace.root}>{workspace.name}</div>
            <div className="workspace-tree" role="tree" aria-label={`${workspace.name} files`}>
              {renderWorkspaceEntries("")}
            </div>
          </> : <div className="empty-explorer"><p>No folder is open.</p>
            <button type="button" onClick={() => void openWorkspace()}>Open Folder</button>
            <button type="button" onClick={() => void openFile()}>Open File</button>
          </div>}
        </>} />}
        <div className="sidebar-resizer" role="separator" aria-label="Resize sidebar"
          aria-orientation="vertical" onPointerDown={() => setResizingSidebar(true)} />
      </section>

      <section className="editor" aria-label="Editor">
        <WorkTabs tabs={tabs} activeTabId={activeTab?.id ?? null} documents={documents} renderView={renderView}
          emptyState={welcome} rename={renameRequest}
          content={folderComparison ? <FolderCompareView result={folderComparison} onClose={() => setFolderComparison(null)} />
            : activeDocument && comparing && secondaryDocument
              ? <CompareView left={activeDocument} right={secondaryDocument} onDocumentChange={upsertDocument} onStatus={setStatus} />
              : undefined}
          onSelectTab={setActiveTabId} onCloseTab={closeTab} onNewTab={() => void newFile()}
          onReorderTab={reorderTab} onRenameTab={renameTab} onInvalidName={() => setStatus(NAME_ERROR)}
          onGroupTab={groupTab} onMovePane={rearrangePane} onPopOutPane={popOutPane} onClosePane={closePaneView}
          onTabMenu={openTabMenu} onPaneMenu={openPaneMenu}
          onFocusPane={(paneId) => updateActiveTab((tab) => ({ ...tab, activePane: paneId }))}
          onRatio={(path, ratio) => updateActiveTab((tab) => setRatio(tab, path, ratio))}>
          <IconButton icon={activeWorkspace ? "unsplit" : "splitHorizontal"}
            label={activeWorkspace ? "Close split view" : "Split editor"}
            disabled={!activeDocument} onClick={toggleSplit} />
          <IconButton icon="compare" label={comparing ? "Close document comparison" : "Compare open documents"}
            pressed={comparing} disabled={documents.length < 2}
            onClick={() => {
              if (!comparing && !secondaryDocumentId) {
                setSecondaryDocumentId(documents.find((document) => document.documentId !== activeDocumentId)?.documentId ?? null);
              }
              setComparing((enabled) => !enabled);
            }} />
          <IconButton icon="folders" label="Compare folders" disabled={!sessionReady} onClick={() => void compareFolders()} />
          <IconButton icon="terminal" label={terminalOpen ? "Close terminal" : "Open terminal"}
            pressed={terminalOpen} disabled={!sessionReady} onClick={() => setTerminalOpen((shown) => !shown)} />
          {activeDocument ? <IconButton icon={viewModes[activeDocument.documentId] === "hex" ? "editor" : "hex"}
            label={viewModes[activeDocument.documentId] === "hex" ? "Show text editor" : "Show hexadecimal editor"}
            onClick={() => toggleViewMode(activeDocument.documentId)} /> : null}
        </WorkTabs>
        {terminalOpen ? <TerminalPanel theme={(themes.find(({ id }) => id === preferences.theme) ?? BUILT_IN_THEMES[0]).colors}
          onClose={() => setTerminalOpen(false)} onStatus={setStatus} /> : null}
      </section>

      <footer className="statusbar"><span className="status-indicator" aria-hidden="true" />
        <span role="status">{status}</span><span className="status-spacer" />
        {activeDocument ? <button type="button" className={`status-mode${activeDocument.readOnly ? " read-only" : ""}`}
          aria-pressed={activeDocument.readOnly === true}
          aria-label={activeDocument.readOnly ? "Read-only mode: switch to Edit mode" : "Edit mode: switch to Read-Only mode"}
          title={`${activeDocument.readOnly ? "Read-Only mode — click to allow editing" : "Edit mode — click to make this document read-only"}${
            shortcutFor("editor.toggleReadOnly") ? ` (${shortcutFor("editor.toggleReadOnly")})` : ""}`}
          onClick={() => void toggleReadOnly(activeDocument.documentId)}>
          <Icon name={activeDocument.readOnly ? "lock" : "edit"} size={12} />{activeDocument.readOnly ? "READ ONLY" : "EDIT"}</button> : null}
        {activeCaret ? <span title="Caret position: line and column">Ln {groupDigits(activeCaret.line)}, Col {groupDigits(String(activeCaret.column))}{
          activeCaret.carets > 1 ? ` · ${activeCaret.carets} carets` : ""}{
          activeCaret.selectedBytes !== "0" ? ` · ${groupDigits(activeCaret.selectedBytes)} bytes selected` : ""}</span> : null}
        {activeDocument ? <span title={activeCharacters === undefined
          ? "Size in bytes. Characters are counted for documents up to 64 MiB."
          : "Size in bytes, and the number of characters in the document's encoding"}>
          {sizeLabel(activeDocument.byteLen, activeCharacters)}</span> : null}
        <span>{info ? `${info.productName} ${info.version}` : "Core pending"}</span>
        <span>{encodingLabel(activeDocument)}</span><span>LF</span>
      </footer>

      {paletteOpen ? <div className="palette-backdrop" role="presentation"
        onMouseDown={(event) => { if (event.target === event.currentTarget) setPaletteOpen(false); }}>
        <section className="command-palette" role="dialog" aria-modal="true" aria-label="Command palette">
          <input autoFocus aria-label="Search commands" placeholder="Type a command" value={paletteQuery}
            onChange={(event) => { setPaletteQuery(event.currentTarget.value); setPaletteIndex(0); }}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown") { event.preventDefault(); setPaletteIndex((index) => Math.min(index + 1, paletteCommands.length - 1)); }
              if (event.key === "ArrowUp") { event.preventDefault(); setPaletteIndex((index) => Math.max(index - 1, 0)); }
              if (event.key === "Enter" && paletteCommands[paletteIndex]) { event.preventDefault(); void runPaletteCommand(paletteCommands[paletteIndex]); }
              if (event.key === "Escape") setPaletteOpen(false);
            }} />
          <div className="palette-results" role="listbox" aria-label="Commands">
            {paletteCommands.map((command, index) => <button type="button" role="option"
              aria-selected={index === paletteIndex} className={index === paletteIndex ? "selected" : ""}
              key={command.id} onMouseEnter={() => setPaletteIndex(index)}
              onClick={() => void runPaletteCommand(command)}>
              <span>{command.title}</span><small>{command.category}</small>
            </button>)}
            {paletteCommands.length === 0 ? <p>No matching commands</p> : null}
          </div>
        </section>
      </div> : null}
      {macroOpen ? <MacroPanel activeDocument={activeDocument} onClose={() => setMacroOpen(false)}
        onDocumentChange={(document) => {
          upsertDocument(document);
          setMacroRefreshVersion((version) => version + 1);
        }} onStatus={setStatus} /> : null}
      {pluginOpen ? <PluginPanel onClose={() => setPluginOpen(false)}
        onCommandsChange={setCommands} onStatus={setStatus} /> : null}
      {settingsOpen ? <div className="settings-backdrop" role="presentation">
        <SettingsWindow mode="inline"
          onPreview={(appearance) => setPreferences(appearance ?? customization.appearance)}
          onSaved={(state, nextThemes) => {
            setThemes(nextThemes);
            setCustomization(state.document);
            setPreferences(state.document.appearance);
            setLineChangeDefault(loadLineChangeDefault());
            setLineChangeOverrides({});
            applyWelcomeSetting();
            setStatus("Customization saved");
          }}
          onMainAction={handleSettingsAction} onClose={() => setSettingsOpen(false)} />
      </div> : null}
      {findTab ? <FindReplacePanel tab={findTab} onTab={setFindTab} onClose={() => setFindTab(null)} seed={findSeed}
        activeDocument={activeDocument && viewModes[activeDocument.documentId] !== "hex" ? activeDocument : null}
        textDocuments={documents.filter((document) => viewModes[document.documentId] !== "hex")}
        selection={caret && caret.documentId === activeDocument?.documentId ? caret.selection : undefined}
        onEditorSearch={searchInEditor} onOpenMatch={(target) => void openMatch(target)}
        onDocumentChange={upsertDocument} onResults={showSearchResults} onChooseFolder={chooseFolder} defaultFolder={workspace?.root}
        onStatus={setStatus} /> : null}
      {searchResults ? <ResultsWindow list={searchResults} onClose={() => setSearchResults(null)}
        onOpenMatch={(target) => void openMatch(target)} onStatus={setStatus}
        openDocumentIds={new Set(documents.map((document) => document.documentId))} /> : null}
      {contextMenu ? <ContextMenu {...contextMenu} onClose={() => setContextMenu(null)} /> : null}
      {aboutOpen ? <div className="decision-backdrop" onPointerDown={(event) => {
        if (event.target === event.currentTarget) setAboutOpen(false);
      }}>
        <section className="decision-dialog about-dialog" role="dialog" aria-modal="true" aria-labelledby="about-title"
          onKeyDown={(event) => {
            keepFocusInDialog(event);
            if (event.key === "Escape") setAboutOpen(false);
          }}>
          <header className="about-identity">
            <span className="brand-mark" aria-hidden="true">N</span>
            <div><h2 id="about-title">Nulis</h2><small>Version {info?.version ?? "—"}</small></div>
          </header>
          {licenseOpen ? <pre className="license-text" tabIndex={0} aria-label="GNU General Public License, version 3">{licenseText}</pre> : <>
            {productCopy}
            <p className="about-note"><strong>About the name.</strong> “Nulis” is a Javanese word that means “writing” or “to write”.</p>
            {productFacts}
            <p className="about-note">Copyright © 2026 Muhammad Edy Suherman.</p>
            <p className="about-note">Nulis is free software: you can redistribute it and/or modify it under the terms of the
              GNU General Public License as published by the Free Software Foundation, either version 3 of the License,
              or (at your option) any later version. It is distributed in the
              hope that it will be useful, but <strong>without any warranty</strong>; without even the implied warranty of
              merchantability or fitness for a particular purpose.</p>
          </>}
          <div className="decision-actions">
            <button type="button" aria-pressed={licenseOpen} onClick={() => setLicenseOpen((open) => !open)}>
              {licenseOpen ? "Back" : "View License"}</button>
            <button autoFocus type="button" onClick={() => setAboutOpen(false)}>Close</button>
          </div>
        </section>
      </div> : null}
      {helpOpen ? <div className="settings-backdrop" role="presentation">
        <HelpWindow mode="inline" onClose={() => setHelpOpen(false)} />
      </div> : null}
      {pendingCloseDocument ? <div className="decision-backdrop">
        <section className="decision-dialog" role="dialog" aria-modal="true" aria-labelledby="close-document-title"
          onKeyDown={(event) => {
            keepFocusInDialog(event);
            if (event.key === "Escape") void decideClose("cancel");
          }}>
          <h2 id="close-document-title">Save changes to {pendingCloseDocument.name}?</h2>
          <p>Your edits will be lost if you discard them.</p>
          <div className="decision-actions">
            <button autoFocus type="button" disabled={busyDecision} onClick={() => void decideClose("cancel")}>Cancel</button>
            <button type="button" disabled={busyDecision} onClick={() => void decideClose("discard")}>Discard</button>
            <button className="primary" type="button" disabled={busyDecision} onClick={() => void decideClose("save")}>Save</button>
          </div>
        </section>
      </div> : null}
      {externalPrompt && !pendingCloseDocument ? <div className="decision-backdrop">
        <section className="decision-dialog" role="dialog" aria-modal="true" aria-labelledby="external-title"
          aria-describedby="external-detail"
          onKeyDown={(event) => {
            keepFocusInDialog(event);
            if (event.key === "Escape") void decideExternal(externalPrompt.kind === "confirmReload" ? "cancel" : "keep");
          }}>
          {externalPrompt.kind === "deleted" ? <>
            <h2 id="external-title">{externalPrompt.document.name} was deleted or moved</h2>
            <p id="external-detail">Another program removed the file. Keep it open to save it again, or close it.</p>
            <div className="decision-actions">
              <button type="button" onClick={() => void decideExternal("close")}>Close</button>
              <button autoFocus className="primary" type="button" onClick={() => void decideExternal("keep")}>Keep Open</button>
            </div>
          </> : externalPrompt.kind === "modified" ? <>
            <h2 id="external-title">{externalPrompt.document.name} was changed by another program</h2>
            <p id="external-detail">{externalPrompt.document.dirty
              ? "Reloading shows the new version and discards your unsaved edits. Keeping yours means saving will replace the other program's changes."
              : "Reload to show the new version, or keep the version in the editor."}</p>
            <div className="decision-actions">
              <button type="button" onClick={() => void decideExternal("keep")}>Keep Mine</button>
              <button autoFocus className="primary" type="button" onClick={() => void decideExternal("reload")}>
                {externalPrompt.document.dirty ? "Discard Edits and Reload" : "Reload"}</button>
            </div>
          </> : <>
            <h2 id="external-title">Reload {externalPrompt.document.name} from disk?</h2>
            <p id="external-detail">Your unsaved edits will be lost.</p>
            <div className="decision-actions">
              <button autoFocus type="button" onClick={() => void decideExternal("cancel")}>Cancel</button>
              <button className="primary" type="button" onClick={() => void decideExternal("reload")}>Discard Edits and Reload</button>
            </div>
          </>}
        </section>
      </div> : null}
      {recoveryCandidates.length > 0 && !pendingCloseDocument ? <div className="decision-backdrop">
        <section className="decision-dialog" role="dialog" aria-modal="true" aria-labelledby="recovery-title"
          onKeyDown={keepFocusInDialog}>
          <h2 id="recovery-title">Recover unsaved work?</h2>
          <p>These documents have edits saved by Nulis before the previous session ended.</p>
          <div className="recovery-list">
            {recoveryCandidates.map((candidate) => <div className="recovery-row" key={candidate.recoveryId}>
              <span><strong>{candidate.name}</strong><small>{candidate.sourcePath ?? "Untitled document"}</small>
                {candidate.sourceChanged ? <small>The original file changed; this checkpoint cannot be restored.</small> : null}
              </span>
              <button type="button" disabled={busyDecision} onClick={() => void discardCandidate(candidate)}>Discard</button>
              <button className="primary" type="button" disabled={busyDecision || candidate.sourceChanged}
                onClick={() => void restoreCandidate(candidate)}>Restore</button>
            </div>)}
          </div>
          <div className="decision-actions"><button autoFocus type="button" onClick={() => setRecoveryCandidates([])}>Later</button></div>
        </section>
      </div> : null}
    </main>
  );
}
