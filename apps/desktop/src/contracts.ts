/** Versioned JSON-compatible values mirrored from nulis-core's public IPC DTOs. */
export const IPC_PROTOCOL_VERSION = 26 as const;

export interface ThemeRecord {
  schema?: string;
  id: string;
  name: string;
  version: number;
  kind: "dark" | "light";
  colors: Record<string, string>;
  builtIn: boolean;
}

export interface EditorPreferences {
  theme: string;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  fontLigatures: boolean;
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

export interface PluginCommandContribution {
  command: string;
  title: string;
  category: string;
}

export interface PluginGrant {
  permissions: PluginPermission[];
}

export interface PluginSummary {
  id: string;
  name: string;
  version: string;
  digest: string;
  requestedPermissions: PluginPermission[];
  approved: boolean;
  commands: PluginCommandContribution[];
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
}

/** Where the caret of the focused editor is, for the status bar. */
export interface CaretPosition {
  line: string;
  column: number;
  /** Number of carets or selections. */
  carets: number;
  /** Bytes covered by all selections, as a decimal string. */
  selectedBytes: string;
}

export interface DocumentSummary {
  documentId: string;
  name: string;
  byteLen: string;
  revision: string;
  encoding: TextEncoding;
  dirty?: boolean;
  sourcePath?: string;
  remoteUri?: string;
  /** Stable checkpoint identity; lets an untitled tab keep its place across restarts. */
  recoveryId?: string;
  /** The language the document is highlighted as. */
  languageId?: string;
  /** Whether the document currently refuses every content change. */
  readOnly?: boolean;
}

/** The built-in tool set a document's language has, if any. */
export type ToolKind = "json" | "xml";

export interface ValidationReport {
  tool: ToolKind;
  valid: boolean;
  message: string;
  byteOffset?: string;
  line?: number;
  column?: number;
}

export interface RecoveryCandidate {
  recoveryId: string;
  name: string;
  sourcePath: string | null;
  sourceChanged?: boolean;
  byteLen: string;
}

export interface RemoteProfile {
  id: string;
  label: string;
  host: string;
  port: number;
  username: string;
  rootPath: string;
  credentialId: string;
  hostKeySha256: string;
}

export type RemoteEntryKind = "directory" | "file" | "symlink" | "other";

export interface RemoteEntry {
  name: string;
  path: string;
  kind: RemoteEntryKind;
  byteLen?: string;
}

export interface RemoteDirectory {
  path: string;
  entries: RemoteEntry[];
  truncated: boolean;
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
  activePanel: "files" | "search" | "remote";
  viewModes: Record<string, "text" | "hex">;
}

export interface CustomizationState {
  revision: number;
  document: CustomizationDocument;
}

/** A find or replace action sent from the Search panel to the focused editor. */
export interface SearchRequest {
  /** Changes for every request, including a repeated one. */
  id: number;
  action: "next" | "previous" | "replace" | "replaceAll";
  query: string;
  replacement: string;
  regex: boolean;
  caseSensitive: boolean;
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
