# M20 handover — Core editing keys, line operations, editor context menu

**Status:** completed 2026-10-20 on macOS (automated checks). Manual use in the running app and Windows/Linux runs are pending.

## Objectives

Fix the most basic editing gaps found in the M19 audit: Home/End, document start/end, forward Delete, Select All, whole-line commands, and a right-click menu in the text editor.

## Implementation

**Core (`crates/nulis-core`)**

- `src/editor/line_ops.rs` (new child module of `editor.rs`, so it uses its private helpers):
  - `DocumentManager::select_all`, `delete_forward`, `line_operation`.
  - `LineOperation` enum: `selectLines`, `deleteLines`, `duplicateLines`, `moveLinesUp`, `moveLinesDown`, `deleteToLineStart`, `deleteToLineEnd`, `insertLineBelow`; result `LineEditResult { document, selections }`.
  - Lines touched by all selections are merged into blocks (adjacent lines join). A selection ending exactly at a line start excludes that line. Each operation is one `replace_many` (one undo step), relocates bookmarks, and records line changes. Reads are limited to the touched lines and `MAX_EDIT_BYTES`.
  - Moving does nothing if any block is at the top/bottom. Duplicate moves the selections to the copy. The last line without an ending borrows the nearest ending (before it, else the first in the file, else LF).
  - `smart_line_start`: Home goes to the first non-blank (bounded 4 KiB scan), then to column 0.
- `src/editor.rs`: `HorizontalDirection` gained `lineStart`, `lineEnd`, `documentStart`, `documentEnd`, handled in `move_horizontal`.
- `src/lib.rs`: commands `editor.deleteForward`, `editor.selectAll`, `editor.lineOperation` (`register_line_commands`); all three are macro-recordable; IPC protocol 26 → 27.

**Desktop (`apps/desktop`)**

- `src/contracts.ts`: protocol 27, `HorizontalDirection`, `LineOperation`, `LineEditResult`.
- `src/EditorViewport.tsx`: `handleEditingKey` (Home/End, Ctrl/⌘+Home/End, macOS ⌘+arrows, Delete, Ctrl/⌘ A, Ctrl/⌘ L, Ctrl/⌘ Shift K, Ctrl/⌘ Shift D, Alt+↑/↓, Ctrl/⌘ Enter, macOS ⌘ Delete / ⌘ Forward Delete); `runLineOperation` joins the typed-edit queue so it never uses a stale revision; right-click menu using the existing `ContextMenu`; new optional props `onToggleHexMode`, `onCopyPath`.
- `src/App.tsx`: passes those two props.
- `src-tauri/capabilities/default.json`: `clipboard-manager:allow-read-text` for the menu's Paste (main window only).

**Docs:** `USER_GUIDE.md` (caret keys, right-click menu, Line commands section, shortcut table), `FEATURE_PARITY.md`.

## Tests

- New core tests (10) in `line_ops.rs`: smart Home/End on CRLF and last line, forward delete of UTF-8 scalar and CRLF, stale revision and read-only refusal, select all/lines, delete lines incl. last line and undo, duplicate with CRLF, move up/down and edges, multi-caret move as one undo step, delete to line edges, insert line below with indentation.
- New desktop tests (3) in `EditorViewport.test.tsx`: key dispatch, context menu actions (incl. Paste via clipboard, Hex Mode, Copy File Path), read-only behavior.
- Results: `cargo test --workspace` all passed (nulis-core 96); `cargo clippy --workspace --all-targets -D warnings` clean; `cargo fmt --check` clean; `npm run lint`, `npm run typecheck` clean; Vitest 11 files / 118 tests passed.

## Decisions

- One `editor.lineOperation` command with an operation enum instead of eight commands.
- Shortcuts are Nulis's own choices. Ctrl+Backspace/Delete on Windows/Linux are left alone (they conventionally mean delete-word, which Nulis doesn't have yet); delete-to-line-edge keys are macOS only.
- Right-click does not move the caret; the menu works on the current selection.

## Known issues / remaining

- Not in the command palette or native menus yet (the palette only runs zero-argument commands); planned for the integration milestone.
- Bookmarks on lines that are moved collapse to the edit start instead of following the line.
- Not tried by hand in the running app; Windows/Linux not run.

## Resume

Next milestone is M21 (Find and Replace window), see `IMPLEMENTATION_TRACKER.md`.
