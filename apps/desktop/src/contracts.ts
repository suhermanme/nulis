/** Versioned JSON-compatible values mirrored from nulis-core's public IPC DTOs. */
export const IPC_PROTOCOL_VERSION = 47 as const;

export type ThemeKind = "dark" | "light";

/** Readability floors authored with a user theme, each 0..1000. */
export interface ThemeCustomFloors {
  maxSaturation: number;
  minLuma: number;
  maxLuma: number;
  minChroma: number;
}

/** A theme sent to `theme.save`; mirrors nulis-theme's `Theme`, which rejects unknown fields. */
export interface ThemeInput {
  id: string;
  name: string;
  version: number;
  kind: ThemeKind;
  colors: Record<string, string>;
  schema?: string;
}
export interface ThemeRecord {
  schema?: string;
  id: string;
  name: string;
  version: number;
  kind: ThemeKind;
  colors: Record<string, string>;
  builtIn: boolean;
  /** UI-side extras; `theme.list` never sends them, so they are read as absent. */
  uiTokens?: string[];
  plugins?: string[];
  /** Readability floors authored with a user theme; built-ins carry none. */
  custom?: ThemeCustomFloors | null;
}

export interface EditorPreferences {
  theme: string;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  fontLigatures: boolean;
  /** Paint the selected theme's colors in every Nulis window, not just the editor. */
  applyUiTheme?: boolean;
}

/** The window theme payload: colors after readability floors plus UI token presence. */
export interface ThemeView {
  theme: string;
  colors: Record<string, string>;
  kind: ThemeKind;
  baseTheme: string;
  /** Readability floors of a user theme; built-in themes carry none. */
  customTheme?: ThemeCustomFloors;
  /** Theme-packaged UI tokens (semantic UI color roles). */
  uiTokens?: string[];
}

export interface Keybinding {
  key: string;
  command: string;
}

export type MenuEntry = { kind: "command"; command: string } | { kind: "separator" };

export interface CustomizationDocument {
  schemaVersion: 1;
  appearance: EditorPreferences;
  keybindings: Keybinding[];
  menu: MenuEntry[];
  toolbar: string[];
}

export interface ApplicationInfo {
  productName: string;
  applicationId: string;
  version: string;
  ipcProtocolVersion: number;
}

export interface CommandDescriptor {
  id: string;
  title: string;
  category: string;
}

export interface CommandContext {
  windowId?: string;
  documentId?: string;
}

export interface ExecuteCommandRequest {
  id: string;
  context: CommandContext;
  arguments: unknown;
}

export interface CommandFailure {
  kind: "invalidId" | "duplicate" | "unknown" | "unavailable" | "failed";
  detail: string;
}

export interface MacroSummary {
  id: string;
  name: string;
  actionCount: number;
}

export interface RecordingStatus {
  recording: boolean;
  id: string | null;
  name: string | null;
  actionCount: number;
  failed: string | null;
}

export interface MacroDefinition {
  schemaVersion: 1;
  id: string;
  name: string;
  actions: Array<{ command: string; arguments: unknown }>;
}

export interface MacroRunOutcome {
  actionsCompleted: number;
  document: DocumentSummary | null;
}

export type SandboxCapability =
  | "executeCommand"
  | "readDocument"
  | "editDocument"
  | "readWorkspace"
  | "writeWorkspace"
  | "readSettings"
  | "showMessage";

export interface SandboxLimits {
  memoryBytes: number;
  executionMillis: number;
  operations: number;
  hostCalls: number;
  outputBytes: number;
}

export interface SandboxPolicy {
  capabilities: SandboxCapability[];
  allowedCommands: string[];
  limits: SandboxLimits;
}

export interface ScriptDefinition {
  apiVersion: 1;
  name: string;
  source: string;
  requestedCapabilities: SandboxCapability[];
  requestedCommands: string[];
}

export interface StoredScript {
  id: string;
  definition: ScriptDefinition;
  approvedPolicy: SandboxPolicy | null;
}

export interface ScriptSummary {
  id: string;
  name: string;
  requestedCapabilities: SandboxCapability[];
  requestedCommands: string[];
  approved: boolean;
}

export interface ScriptRunOutcome {
  report: { operations: number; hostCalls: number; output: string };
  document: DocumentSummary | null;
  messages: string[];
}

export type PluginPermission = "showMessage";

/** A contribution family a user can switch off without revoking a plugin. */
export type PluginCapability =
  | "commands" | "languages" | "highlight" | "indentation" | "completion" | "snippets";

export interface PluginCommandContribution {
  command: string;
  title: string;
  category: string;
}

export interface PluginGrant {
  permissions: PluginPermission[];
}

export interface PluginLanguageSummary {
  id: string;
  displayName: string;
  extensions: string[];
  filenames: string[];
  capabilities: PluginCapability[];
}

export interface PluginSummary {
  id: string;
  name: string;
  version: string;
  digest: string;
  apiVersion: number;
  requestedPermissions: PluginPermission[];
  approved: boolean;
  /** False keeps an approved plugin installed but inert. */
  enabled: boolean;
  disabledCapabilities: PluginCapability[];
  commands: PluginCommandContribution[];
  languages: PluginLanguageSummary[];
}

/** One language's claim on a file pattern, from `language.conflicts`. */
export interface AssociationClaim {
  language: string;
  displayName: string;
  /** `builtin` or the contributing plugin ID. */
  source: string;
}

export interface LanguageConflict {
  pattern: string;
  claims: AssociationClaim[];
}

export interface PluginRunOutcome {
  pluginId: string;
  command: string | null;
  result: unknown;
  messages: string[];
}

/** Characters in a document; absent for documents too large to count. */
export interface CharacterCount {
  revision: string;
  characters?: string;
  words?: string;
  lines?: string;
  /** Read from the first 1 MiB only when the document is too large to count. */
  lineEnding?: LineEndingStyle;
}

export type LineEnding = "lf" | "crlf" | "cr";
export type LineEndingStyle = LineEnding | "mixed" | "none";

export interface LineEndingConversion {
  document: DocumentSummary;
  /** Number of line endings changed. */
  converted: number;
  selections: Selection[];
}

/** Where the caret of the focused editor is, for the status bar. */
export interface CaretPosition {
  line: string;
  column: number;
  /** Number of carets or selections. */
  carets: number;
  /** Bytes covered by all selections, as a decimal string. */
  selectedBytes: string;
  /** The primary selection as a range, unless it is rectangular. */
  selection?: ByteRange;
  /** Unicode code point of the character after the caret, if any. */
  codePoint?: number;
}

/** Result of `file.exportPdfTo`; counts are decimal strings like other 64-bit values. */
export interface PdfExportResult {
  path: string;
  pages: string;
  /** Characters left blank because the print font lacks them. */
  missingGlyphs: string;
  bytes: string;
}

/** Progress of an export started with `file.exportPdfStart`. */
export interface PdfExportReport {
  progress: {
    phase: "countingLines" | "countingPages" | "writing";
    done: string;
    total: string;
    pagesWritten: string;
    estimatedPages: string;
  };
  done: boolean;
  cancelled: boolean;
  /** At least this many pages: the export stopped to ask first. */
  needsConfirmation?: string;
  error?: string;
  result?: PdfExportResult;
  /** Pages laid out by `file.printStart`, as a decimal string. */
  printedPages?: string;
}

/** Text at a cell of the page grid: column, cells, style index, text. */
export type PrintPiece = [number, number, number, string];

/** What every printed page shares (`nulis_print::PageFrame`). Lengths are points from the top-left corner. */
export interface PrintPageFrame {
  widthMm: number;
  heightMm: number;
  paperWidthMm: number;
  paperHeightMm: number;
  landscape: boolean;
  marginLeftPt: number;
  headerTopPt: number;
  bodyTopPt: number;
  footerTopPt: number;
  fontSizePt: number;
  lineHeightPt: number;
  cellPt: number;
  columns: number;
  rows: number;
  hasHeader: boolean;
  hasFooter: boolean;
  background: string | null;
  ruleColor: string;
  styles: Array<{ color: string; bold: boolean }>;
}

/** One laid-out page (`nulis_print::PrintedPage`). */
export interface PrintedPage {
  number: number;
  header: PrintPiece[];
  footer: PrintPiece[];
  rows: PrintPiece[][];
  /** Rows that are a rule between two printed ranges. */
  rules: number[];
}

/** A batch from `print.pages`; each page is JSON text. */
export interface PrintPagesBatch {
  printId: number;
  title: string;
  frame: PrintPageFrame;
  total: number;
  start: number;
  pages: string[];
}

export interface DocumentSummary {
  documentId: string;
  name: string;
  byteLen: string;
  revision: string;
  encoding: TextEncoding;
  dirty?: boolean;
  sourcePath?: string;
  /** Stable checkpoint identity; lets an untitled tab keep its place across restarts. */
  recoveryId?: string;
  /** The language the document is highlighted as. */
  languageId?: string;
  /** The user chose the language with View As instead of detection. */
  languageOverride?: boolean;
  /** Whether the document currently refuses every content change. */
  readOnly?: boolean;
  /** What the document's language offers right now; absent when nothing. */
  capabilities?: DocumentCapabilities;
  /** What Tab inserts (spaces or a tab) and how wide a tab is drawn. */
  indentation?: { spaces: boolean; width: number; tabSize: number };
  /** A binary format such as "binary property list": shown in the hexadecimal view. */
  binaryFormat?: string;
}

/**
 * Language features of a document, each named by its provider's label
 * ("JSON"). The interface asks for features, never for language IDs.
 */
export interface DocumentCapabilities {
  /** Validate, by label. */
  validate?: string;
  /** Beautify, by label. */
  format?: string;
  /** Minify, by label. */
  compact?: string;
  /** Beautify may drop something: run Validate first and ask before Beautify. */
  formatMayLoseData?: boolean;
  /** A rendered view, by kind ("markdown"). */
  preview?: string;
  /** Completion opens while typing ("auto") or only when asked ("manual"). */
  completion?: "auto" | "manual";
  /** The approved language server for the document's language, by ID. */
  languageServer?: string;
}

/** One configured language server, as `lsp.state` lists it. */
export interface LanguageServerStatus {
  id: string;
  command: string[];
  languages: string[];
  approved: boolean;
  /** Project folders it runs for now. */
  running: string[];
  /** Why it failed last, when it did. */
  error?: string;
  /** Set aside after repeated failures, until restarted. */
  stopped: boolean;
}

/** The answer to `lsp.state`. */
export interface LanguageServersState {
  servers: LanguageServerStatus[];
  /** Problems in settings.json's `lsp.servers`, if any. */
  configError: string | null;
}

/** A problem a language server reported, placed in the document. */
export interface DocumentProblem {
  /** Byte offsets as decimal strings, like every 64-bit coordinate. */
  startByte: string;
  endByte: string;
  /** One-based line of the start. */
  line: number;
  severity: "error" | "warning" | "information" | "hint";
  message: string;
  source?: string;
}

/** The answer to `lsp.diagnostics` for one document. */
export interface DocumentProblems {
  server?: string;
  revision: string;
  problems: DocumentProblem[];
}

export type CapabilityKind = "highlight" | "comment" | "indentation" | "completion" | "diagnostics"
  | "format" | "compact" | "preview" | "commands" | "symbols";

/** A language and the capabilities Settings can switch off for it. */
export interface LanguageCapabilities {
  id: string;
  displayName: string;
  capabilities: { kind: CapabilityKind; label: string; enabled: boolean }[];
}

/** The user's settings.json as the core reports it. */
export interface SettingsState {
  revision: number;
  document: { schemaVersion: 1; values: Record<string, unknown> };
  rejectedAssociations: string[];
  loadError: string | null;
}

/** One suggestion from `completion.request`. */
export interface CompletionItem {
  label: string;
  kind: "snippet" | "keyword" | "type" | "word";
  /** Replaces the typed word; for a snippet, the snippet body. */
  insert: string;
  snippet?: boolean;
  detail?: string;
}

/** The answer to `completion.request`, for one revision of the document. */
export interface CompletionList {
  requestId: number;
  revision: string;
  replaceStart: string;
  replaceEnd: string;
  prefix: string;
  items: CompletionItem[];
}

/** When the suggestion list opens and how it is accepted (settings.json `editor.suggest`). */
export interface SuggestSettings {
  /** Open while typing in languages with their own words. */
  auto: boolean;
  /** Enter accepts (Tab always does). */
  acceptOnEnter: boolean;
  /** Characters of a word typed before the list opens by itself. */
  minChars: number;
  /** Pause after typing before asking, in milliseconds. */
  delayMs: number;
}

export const DEFAULT_SUGGEST_SETTINGS: SuggestSettings = { auto: true, acceptOnEnter: true, minChars: 2, delayMs: 120 };

/** Reads `editor.suggest` from settings.json values; anything missing keeps its default. */
export function suggestSettingsOf(values: Record<string, unknown> | undefined): SuggestSettings {
  const editor = values?.editor;
  const suggest = typeof editor === "object" && editor !== null ? (editor as { suggest?: unknown }).suggest : undefined;
  const read = typeof suggest === "object" && suggest !== null ? suggest as Record<string, unknown> : {};
  const number = (value: unknown, fallback: number, minimum: number, maximum: number) =>
    typeof value === "number" && Number.isFinite(value) ? Math.min(maximum, Math.max(minimum, Math.round(value))) : fallback;
  return {
    auto: typeof read.auto === "boolean" ? read.auto : DEFAULT_SUGGEST_SETTINGS.auto,
    acceptOnEnter: typeof read.acceptOnEnter === "boolean" ? read.acceptOnEnter : DEFAULT_SUGGEST_SETTINGS.acceptOnEnter,
    minChars: number(read.minChars, DEFAULT_SUGGEST_SETTINGS.minChars, 1, 10),
    delayMs: number(read.delayMs, DEFAULT_SUGGEST_SETTINGS.delayMs, 0, 2000),
  };
}

export interface ValidationReport {
  valid: boolean;
  message: string;
  byteOffset?: string;
  line?: number;
  column?: number;
  /** What Beautify would lose (YAML comments, anchors), or why it cannot run. */
  beautifyNote?: string;
  /** Beautify would refuse this document (YAML with tags). */
  beautifyUnavailable?: boolean;
}

export interface RecoveryCandidate {
  recoveryId: string;
  name: string;
  sourcePath: string | null;
  sourceChanged?: boolean;
  byteLen: string;
}

export interface TerminalSummary {
  terminalId: string;
  title: string;
  size: { columns: number; rows: number };
  running: boolean;
  exitCode: number | null;
}

export interface TerminalOutput {
  terminalId: string;
  bytes: number[];
  startOffset: string;
  nextOffset: string;
  dropped: boolean;
  running: boolean;
  exitCode: number | null;
}

export type TextEncoding = "utf8" | "windows1252";

export type SearchMode = "literal" | "regex";

export interface SearchOptions {
  mode: SearchMode;
  caseSensitive: boolean;
  /** Only where the text is not part of a longer word. */
  wholeWord?: boolean;
  /** Searches each line only between these columns (from 1; both included). */
  columns?: ColumnRange;
}

/** Columns as the status bar counts them; a missing end means the line's start or end. */
export interface ColumnRange {
  from?: number;
  to?: number;
}

/** Matches that start in a visible range, for "highlight all". */
export interface RangeMatches {
  revision: string;
  matches: Array<{ startByte: string; endByte: string }>;
  truncated: boolean;
}

export interface ByteRange {
  startByte: string;
  endByte: string;
}

export interface FoundMatch {
  startByte: string;
  endByte: string;
  line: string;
}

export interface FoundLine {
  line: string;
  startByte: string;
  /** The start of the line (at most 512 bytes), without its ending. */
  text: string;
  truncated: boolean;
  matches: number;
}

/** Result of `search.findAll`. */
export interface FindAllResult {
  revision: string;
  matches: FoundMatch[];
  lines: FoundLine[];
  truncated: boolean;
}

/** An open document whose file another program changed or removed. */
export interface ExternalChange {
  documentId: string;
  path: string;
  kind: "modified" | "deleted";
  dirty: boolean;
}

/** A file's size and modification time when it was previewed. */
export interface FileStamp {
  size: string;
  modified: string | null;
}

/** One file found by `search.previewReplaceInFiles`. */
export interface ReplacePreviewFile {
  path: string;
  matches: number;
  stamp: FileStamp;
  /** Open in the editor: its in-memory text is changed and left unsaved. */
  open: boolean;
}

export interface ReplacePreview {
  files: ReplacePreviewFile[];
  filesScanned: number;
  truncated: boolean;
}

/** Result for one file of `search.replaceInFiles`. */
export interface ReplaceFileOutcome {
  path: string;
  replacements: number;
  error: string | null;
  document: DocumentSummary | null;
}

/** Which files `search.findInFiles` reads below its root. */
export interface FileSearchScope {
  include: string[];
  recursive: boolean;
}

export interface FindResult {
  revision: string;
  selection?: Selection;
  wrapped: boolean;
}

export interface ReplaceAllResult {
  document: DocumentSummary;
  replacements: number;
}

export interface WorkspaceSearchMatch {
  path: string;
  startByte: string;
  endByte: string;
  line: string;
  lineText: string;
}

export interface WorkspaceSearchResult {
  matches: WorkspaceSearchMatch[];
  filesScanned: number;
  skippedFiles: number;
  truncated: boolean;
}

export interface WorkspaceSummary {
  root: string;
  name: string;
}

export type WorkspaceEntryKind = "directory" | "file" | "symlink" | "other";

export interface WorkspaceEntry {
  name: string;
  relativePath: string;
  kind: WorkspaceEntryKind;
  byteLen?: string;
}

export interface WorkspaceDirectory {
  relativePath: string;
  entries: WorkspaceEntry[];
  truncated: boolean;
}

export type SplitDirection = "horizontal" | "vertical";

/** A pane. It shows one document; `tabs` holds that single reference. */
export interface PaneNode {
  kind: "pane";
  id: string;
  tabs: string[];
  activeTab: string | null;
}

/** A branch; `ratio` is the first child's share in thousandths. */
export interface SplitNode {
  kind: "split";
  direction: SplitDirection;
  ratio: number;
  first: LayoutNode;
  second: LayoutNode;
}

export type LayoutNode = PaneNode | SplitNode;

/** A top-level tab holding one tree of panes. */
export interface LayoutGroup {
  id: string;
  name: string;
  root: LayoutNode;
  activePane: string;
}

/** Schema v2. Tabs are stable `path:`/`recovery:` references, never document IDs. */
export interface WorkbenchSession {
  schemaVersion: 2;
  workspaceRoot: string | null;
  documents: string[];
  /** Top-level tabs in strip order; a tab with several panes is a named workspace. */
  layouts: LayoutGroup[];
  activeLayout: string | null;
  sidebarVisible: boolean;
  sidebarWidth: number;
  activePanel: "files" | "search";
  viewModes: Record<string, "text" | "hex">;
  /** Languages chosen with View As, by document reference; omitted when empty. */
  languages?: Record<string, string>;
}

export interface CustomizationState {
  revision: number;
  document: CustomizationDocument;
}

/** A find or replace action sent from the Search panel to the focused editor. */
export interface SearchRequest {
  /** Changes for every request, including a repeated one. */
  id: number;
  /** "highlight" only updates the query, options, and highlighting. */
  action: "next" | "previous" | "replace" | "replaceAll" | "highlight";
  query: string;
  replacement: string;
  regex: boolean;
  caseSensitive: boolean;
  wholeWord?: boolean;
  /** Searches each line only between these columns. */
  columns?: ColumnRange;
  /** Turns highlighting of every match on or off. */
  highlight?: boolean;
  /** Limits Replace All to this range (the selection). */
  within?: ByteRange;
}

/** A starting point of the Places explorer. */
export interface Place {
  name: string;
  path: string;
  kind: "computer" | "volume" | "home";
}

export interface PlaceEntry {
  name: string;
  /** Absolute path. */
  path: string;
  kind: "directory" | "file" | "symlink" | "other";
  byteLen?: string;
}

export interface PlaceDirectory {
  path: string;
  entries: PlaceEntry[];
  truncated: boolean;
}

export type CompareMode = "text" | "binary";
export type ChangeKind = "insert" | "delete" | "replace";

export interface DiffRange {
  startByte: string;
  endByte: string;
}

export interface DiffChange {
  kind: ChangeKind;
  left: DiffRange;
  right: DiffRange;
  leftStartLine: string;
  rightStartLine: string;
  leftLineCount: number;
  rightLineCount: number;
}

export interface DiffPage {
  mode: CompareMode;
  changes: DiffChange[];
  nextLeftByte: string | null;
  nextRightByte: string | null;
  nextLeftLine: string | null;
  nextRightLine: string | null;
  truncated: boolean;
}

export type FolderEntryKind = "directory" | "file";
export type FolderCompareStatus = "leftOnly" | "rightOnly" | "different";

export interface FolderCompareEntry {
  relativePath: string;
  status: FolderCompareStatus;
  leftKind: FolderEntryKind | null;
  rightKind: FolderEntryKind | null;
  leftByteLen: string | null;
  rightByteLen: string | null;
}

export interface FolderCompareResult {
  leftRoot: string;
  rightRoot: string;
  entries: FolderCompareEntry[];
  identicalFiles: number;
  comparedFiles: number;
  skippedSymlinks: number;
  truncated: boolean;
}

export interface Selection {
  anchorByte: string;
  activeByte: string;
}

/** Caret targets of `editor.moveHorizontal`. */
export type HorizontalDirection = "left" | "right" | "lineStart" | "lineEnd" | "documentStart" | "documentEnd";

/** Whole-line commands of `editor.lineOperation`. */
export type LineOperation =
  | "selectLines" | "deleteLines" | "duplicateLines" | "moveLinesUp" | "moveLinesDown"
  | "deleteToLineStart" | "deleteToLineEnd" | "insertLineBelow"
  | "indent" | "outdent" | "toggleComment" | "trimTrailingSpaces" | "tabsToSpaces" | "spacesToTabs"
  | "sortAscending" | "sortDescending" | "sortUnique" | "alignColumns" | "unalignColumns";

export type Justify = "left" | "center" | "right";
export type NumberPadding = "none" | "zeros" | "spaces";

export interface NumberSum {
  /** Exact decimal text. */
  sum: string;
  count: number;
}

export interface WorkspaceFileList {
  files: string[];
  truncated: boolean;
}

/** Selection transforms of `editor.transformSelections`. */
export type TextTransformName =
  | "uppercase" | "lowercase" | "trimWhitespace" | "invertCase" | "capitalize" | "base64Encode" | "base64Decode";

export interface LineEditResult {
  document: DocumentSummary;
  selections: Selection[];
}

export interface RectangularSelection {
  anchorByte: string;
  activeByte: string;
  anchorLine: string;
  activeLine: string;
  anchorColumn: string;
  activeColumn: string;
}

export type SyntaxToken =
  | "comment"
  | "keyword"
  | "string"
  | "number"
  | "function"
  | "type"
  | "property"
  | "escape"
  | "invalid";

export interface LanguageDescriptor {
  id: string;
  displayName: string;
}

export interface SyntaxHighlight {
  startByte: string;
  endByte: string;
  token: SyntaxToken;
}

export interface SyntaxFold {
  startByte: string;
  endByte: string;
  startLine: string;
  endLine: string;
}

export interface SyntaxSymbol {
  name: string;
  kind: "property";
  startByte: string;
  line: string;
}

export interface SyntaxBracketPair {
  openByte: string;
  closeByte: string;
}

export interface SyntaxSnapshot {
  languageId: string;
  parsedBytes: number;
  highlights: SyntaxHighlight[];
  folds: SyntaxFold[];
  symbols: SyntaxSymbol[];
  bracketPairs: SyntaxBracketPair[];
  truncated: boolean;
  timedOut: boolean;
}

export interface ClipboardPayload {
  text: string;
  revision: string;
  hadInvalidUtf8: boolean;
}

export interface ViewportCursor {
  byteOffset: string;
  line: string;
  lineContinuation: boolean;
}

export interface ViewportLine {
  line: string;
  startByte: string;
  endByte: string;
  text: string;
  sourceByteWidths: number[];
  continuation: boolean;
  truncated: boolean;
  change?: LineChange;
  /** Absent on the last line and on a truncated segment. */
  ending?: LineEnding;
}

/** Editing-session marker for one logical line; not a diff against a baseline. */
export type LineChange = "unsaved" | "saved" | "unsavedDeletion" | "savedDeletion";

export interface LineChangeStatus {
  available: boolean;
  approximate: boolean;
}

export interface ViewportRequest {
  documentId: string;
  startByte: string;
  startLine: string;
  lineContinuation?: boolean;
  maxBytes: number;
}

export interface ViewportAtRequest {
  documentId: string;
  byteOffset: string;
  maxBytes: number;
}

export interface ViewportLineRequest {
  documentId: string;
  line: string;
  maxBytes: number;
}

export interface ViewportSnapshot {
  documentId: string;
  revision: string;
  documentByteLen: string;
  encoding: TextEncoding;
  lines: ViewportLine[];
  selections: Selection[];
  rectangularSelection?: RectangularSelection;
  syntax?: SyntaxSnapshot;
  bookmarks: string[];
  lineChanges?: LineChangeStatus;
  next?: ViewportCursor;
}

export interface DocumentMapSnapshot {
  documentId: string;
  revision: string;
  documentByteLen: string;
  totalLines: number | null;
  samples: Array<{ byteOffset: string; line: string; text: string }>;
  changes?: Array<{ byteOffset: string; line: string; change: LineChange }>;
  lineChanges?: LineChangeStatus;
}

export interface MarkdownPreviewSource {
  documentId: string;
  revision: string;
  text: string;
}

export interface HexViewportRequest {
  documentId: string;
  startByte: string;
  maxBytes: number;
  bytesPerRow: number;
}

export interface HexViewportRow {
  startByte: string;
  bytes: number[];
}

export interface HexCursor {
  byteOffset: string;
}

export interface HexViewportSnapshot {
  documentId: string;
  revision: string;
  documentByteLen: string;
  bytesPerRow: number;
  rows: HexViewportRow[];
  selections: Selection[];
  next?: HexCursor;
}

export interface HexCaret {
  byteOffset: string;
  highNibble: boolean;
}

export interface HexEditResult {
  document: DocumentSummary;
  caret: HexCaret;
}

export interface HexClipboardPayload {
  hex: string;
  revision: string;
}
