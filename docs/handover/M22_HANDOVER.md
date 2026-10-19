# M22 handover — Replace in Files

**Status:** completed 2026-10-20 on macOS (automated checks), on `main`. Not tried by hand in the running app; Windows/Linux not run.

## Objectives

Add the Replace in Files tab to the Find and Replace window, safely: nothing is written before the user sees what will change and confirms.

## Implementation

**Core (`crates/nulis-core/src/editor/replace_in_files.rs`, new)**

- `preview_replace_in_files(root, query, options, scope)`: runs Find in Files and groups matches per file; each file gets a `FileStamp` (size, modification time in ns) and an `open` flag. Capped at 1,000 files; `truncated` when a search limit was hit.
- `replace_in_files(targets, query, replacement, options)`: per file, independently:
  - open in the editor → `replace_all` on that document (one undo step, left unsaved); refused if Read-Only;
  - closed → skipped if its stamp no longer matches; otherwise opened as a temporary document, `replace_all`, `save` (existing atomic save with its own fingerprint check), then closed. On any failure the temporary document is discarded and the file is unchanged.
  - Returns a `ReplaceFileOutcome` per file (`replacements`, `error`, and the new `document` summary for open documents).
- Commands `search.previewReplaceInFiles`, `search.replaceInFiles` (not macro-recordable); IPC 28 → 29.

**Adapter:** both commands added to the Find window's allowlist (`FIND_COMMANDS`) and its test.

**Desktop:** `FindReplacePanel.tsx` gains the Replace in Files tab: Find what / Replace with, Files in a folder or All open files, the folder fields shared with Find in Files, options, **Preview** → file list (open files marked), **Replace All** → inline confirmation → per-file report with failures. A preview is tied to the exact query, replacement, options, folder, filter, and subfolder switch; any change disables Replace All until a new preview. Open documents' new summaries are sent to the editor window (`documentChange`). `contracts.ts` gains the preview/outcome types; `styles.css` the lists.

**Docs:** `USER_GUIDE.md` (Replace in files section, four tabs), `FEATURE_PARITY.md`.

## Tests

- Core: preview writes nothing and marks open files; replace changes a closed CRLF file on disk and an open document only in memory (dirty); a file changed after the preview is skipped while others continue; a Read-Only open document is reported and not changed.
- Adapter: Find window may run both commands.
- Desktop: App test for Browse → Preview → stale preview disables Replace All → confirmation text → command arguments → report with failures → open document marked unsaved.
- Full suite: fmt and clippy clean; `cargo test --workspace` passed (nulis-core 104, nulis-desktop 7); lint/typecheck clean; Vitest 122 passed.

## Decisions

- Two-step command pair instead of one call, so the confirmation shows exactly what will change and the stamp check catches changes made in between.
- Closed files reuse the editor's open → replace → atomic save path rather than a second writer, so encoding handling, temporary-file sync, and replacement semantics are the same as Save.

## Known issues

- Matches in open documents are counted from the file on disk in the preview; the editor's text may differ, and the report gives the real count.
- No per-file checkboxes to leave some previewed files out.
- The stamp is size + modification time; a change that keeps both identical would not be noticed (the atomic save's own fingerprint check still applies).

## Resume

Next: M23 External changes and Reload (user request), then M24 New application icon.
