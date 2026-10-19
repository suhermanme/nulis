# Implementation tracker (M19–M29)

Authoritative progress and handover record for `IMPROVEMENT_PLAN.md`. Keep it in step with the repository; never mark work complete without the verification listed.

## Current state

- **Current milestone:** M25 — pending (next to start)
- **Overall:** 6 of 11 milestones completed (M19–M24)
- **Baseline before M19 (commit `66853e1`):** `cargo test -p nulis-core` 86 passed; desktop Vitest 11 files, 115 tests passed.

| Milestone | Status | Handover |
| --- | --- | --- |
| M19 Audit and foundation (+ Windows Settings window bug) | Completed (Windows/Linux runtime check pending) | `handover/M19_HANDOVER.md` |
| M20 Core editing keys, line operations, editor context menu | Completed (manual and Windows/Linux checks pending) | `handover/M20_HANDOVER.md` |
| M21 Find and Replace window | Completed (manual and Windows/Linux checks pending) | `handover/M21_HANDOVER.md` |
| M22 Replace in Files | Completed (manual and Windows/Linux checks pending) | `handover/M22_HANDOVER.md` |
| M23 External changes and Reload (user request) | Completed (manual and Windows/Linux checks pending) | `handover/M23_HANDOVER.md` |
| M24 New application icon (user request) | Completed | `handover/M24_HANDOVER.md` |
| M25 Format and transforms | Pending | – |
| M26 Search and navigation | Pending | – |
| M27 View and file | Pending | – |
| M28 Column and clipboard tools | Pending | – |
| M29 Integration and quality | Pending | – |

## M19 — Audit and foundation

**Deliverables:** `IMPROVEMENT_PLAN.md`, this tracker, `handover/M19_HANDOVER.md`; baseline test run.

**User-reported bug fixed in M19:** on Windows the Settings window opened blank, and on Windows and Linux Settings showed the application menu bar.

- Cause 1 (blank window): Tauri documents that building a window inside a synchronous command or an event handler deadlocks on Windows (WebView2). `open_settings_window`/`open_help_window` were synchronous commands and the native Settings/Help menu items built the window inside `on_menu_event`.
- Fix: both commands are now `async`, and the menu handlers spawn the window creation on the async runtime.
- Cause 2 (menu bar): `app.set_menu` attaches the application menu to every window on Windows/Linux.
- Fix: `detach_application_menu` calls `remove_menu()` on the Settings and Help windows (no-op on macOS, which has one global menu bar).
- File: `apps/desktop/src-tauri/src/lib.rs`.

**Verification:** `cargo clippy -p nulis-desktop --all-targets -D warnings` clean; `cargo test -p nulis-desktop` passed (6). The Windows/Linux code path is not compiled on this Mac (only `x86_64-apple-darwin` installed). **Pending:** CI build on Windows/Linux and a manual check that Settings and Help render and show no menu bar.

## M20 — Core editing keys, line operations, editor context menu

**Scope**

1. Caret: Home (smart: first non-blank, then column 0), End, Primary+Home/End (document start/end); all with Shift to extend. On macOS also Primary+Left/Right as line start/end and Primary+Up/Down as document start/end.
2. Forward Delete (selection, or next character; malformed bytes one at a time; CRLF as one unit).
3. Select All (Primary+A).
4. Line operations as one undo step each, over every line touched by any selection: Select Line(s), Delete Line(s), Duplicate Line(s), Move Line(s) Up/Down, Delete to Line Start, Delete to Line End, Insert Line Below.
5. Editor right-click menu: Undo, Redo, Cut, Copy, Paste, Find, Replace, Select All, Select Line, Select Word, Toggle Hex Mode, Copy File Path.
6. Palette entries and default shortcuts for the line operations.

**Exclusions:** rectangle-specific line semantics beyond "rows of the rectangle", customizing the context menu, native Edit menu additions (M27), word-wise movement.

**Expected files:** `crates/nulis-core/src/editor.rs`, `crates/nulis-core/src/lib.rs`, `apps/desktop/src/EditorViewport.tsx`, `apps/desktop/src/contracts.ts`, tests in `editor.rs` and `EditorViewport.test.tsx`, `docs/USER_GUIDE.md`, `docs/FEATURE_PARITY.md`.

**Acceptance criteria**

- Home/End/Primary+Home/End move and extend correctly on LF, CRLF, CR, and the last line without a newline.
- Delete removes the next scalar, a whole CRLF pair, or the selection; refused in read-only mode.
- Select All selects bytes `0..len`.
- Each line operation is one undo step, keeps bytes and line endings exact, works with several carets, and fails cleanly above the edit-size limit.
- Right-click on the text shows the menu; each item runs the same action as its shortcut; Escape closes it.
- All suites green (`cargo test --workspace`, clippy, fmt, lint, typecheck, Vitest).

**Risks / rollback:** IPC version bump to 27; revert the M20 commit to roll back (no persisted data changes).

**Result (completed 2026-10-20):** all acceptance criteria met by automated tests on macOS. Palette entries (scope item 6) moved to M29 because the palette only runs zero-argument commands. Verification: `cargo test --workspace` passed (nulis-core 96, was 86); clippy and fmt clean; lint and typecheck clean; Vitest 118 passed (was 115). Not yet tried by hand in the running app. Details: `handover/M20_HANDOVER.md`.

## M21 — Find and Replace window

**Origin:** user request after M20 with 9 screenshots of the reference editor's Find and Replace dialog (S19–S27 in the plan).

**Scope**

1. One floating, draggable Find and Replace panel inside the main window (not a separate OS window), opened by Ctrl/⌘ F (Find tab), Ctrl/⌘ H (Replace tab), the editor toolbar, and the right-click menu; seeded with the selected text.
2. Tabs: Find, Find in Files, Replace. (Replace in Files is M22.)
3. Find: multi-line Find what with recent-search history; Find where = current document, selected text, all open files; Options = match case, whole word, regular expression, highlight all items found, list lines containing the text, close after find; Count All, Previous, Next.
4. Replace: Replace with (with history); Replace where = current document, selected text, all open files; Options = match case, whole word, regex, close after Replace All; Next, Replace, Replace All.
5. Find in Files: Find where = folder or all open files; file-name filter (`*.rs; *.md`), directory with Browse, include subdirectories; options match case, whole word, regex; results list "file:line: text" that opens the file at the match.
6. "Lines containing" results window: one row per matching line (optional line numbers), click or Go To jumps to that line, Bookmark All, Copy, Close.
7. Remove the Search panel from the side bar and the inline find bar from the editor; the Go To Line bar stays.

**Core changes:** `SearchOptions.whole_word`; `search.findAll` (matches + matching lines with line numbers and bounded line text, optional selection range, result cap); `search.replaceAll` gains an optional selection range; `search.findInFiles` gains name filters, a recursive flag, and line number + line text per match; `editor.addBookmarks` for Bookmark All. IPC 27 → 28.

**Exclusions:** Replace in Files (M22); search in column range, preserve case, regex dialects, ignore options, favorites (deferred).

**Acceptance criteria**

- Each option changes results as described; whole word works for literal and regex searches.
- Count All and the lines list agree with the core; clicking a row puts the caret on that line in the right document.
- Replace All in a selection changes only matches inside it and is one undo step.
- Find in Files respects the filter and the subdirectory switch and shows line numbers.
- The panel can be dragged, keeps its tab and options while open, closes with Escape or its close button, and all controls are keyboard reachable with labels.
- All suites green.

**Result (completed 2026-10-20):** scope items 1–7 implemented. Verification on macOS: `cargo fmt --check` and `cargo clippy --workspace --all-targets -D warnings` clean; `cargo test --workspace` all passed (nulis-core 101, nulis-search 8); `npm run lint`, `npm run typecheck` clean; Vitest 11 files / 120 tests passed. Tests that used the removed find bar and Search side panel were rewritten for the new window. Not yet tried by hand in the running app. Details and known limits: `handover/M21_HANDOVER.md`.

**Risks / rollback:** existing tests that use the inline find bar or Search side panel must be rewritten; large documents: lists are capped (10,000 matches / 5,000 lines) and say when truncated. Revert the M21 commit to roll back.

## M22 — Replace in Files

**Scope:** a Replace in Files tab with the Find in Files fields plus Replace with; a preview list of files and match counts before anything changes; explicit confirmation; open documents changed in memory (one undo step each, left dirty); closed files saved with the existing atomic save after a fingerprint check; a per-file success/failure report; size and file-count limits.
**Acceptance:** nothing is written before confirmation; a file changed on disk after the preview is skipped and reported; read-only documents are skipped; results agree with Find in Files.

**Result (completed 2026-10-20, on `main`):** all acceptance criteria covered by tests. Verification on macOS: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` clean; `cargo test --workspace` all passed (nulis-core 104, nulis-desktop 7); lint and typecheck clean; Vitest 11 files / 122 tests passed. Details: `handover/M22_HANDOVER.md`.

## M23 — External changes and Reload

**Scope**

1. Core `editor.externalChanges`: for every open local document, compare the file's current fingerprint with the one recorded at open/save; report `modified` or `deleted`, with the document's dirty state. Only `stat` calls; no file content is read.
2. Core `file.reload` (`discard` flag): re-open the file in the same document (same tab and ID): new source, empty undo history, cleared line-change markers, caret kept on the same line where possible, bookmarks past the end dropped; refused while dirty unless `discard` is set.
3. Core `editor.keepExternalVersion`: the user keeps the editor's version; the new fingerprint becomes the expected one so the next Save overwrites the outside change instead of failing.
4. Desktop: check when the window regains focus and every 3 seconds while focused; one prompt per change: "*name* was changed by another program" with **Reload** / **Keep Mine** (a warning when there are unsaved edits); for a deleted file **Keep Open** / **Close**. 
5. **File ▸ Reload from Disk** (palette, File menu, editor right-click menu): asks first when there are unsaved edits.

**Exclusions:** remote (SFTP) documents; three-way merge of both versions; a file-system watcher (polling `stat` on focus/interval needs no new dependency).

**Acceptance:** an outside edit is reported once; Reload shows the new text in every view of the document and keeps the tab; Keep Mine stops the prompt and a later Save succeeds; a deleted file is reported; Reload with unsaved edits needs confirmation; nothing is read from disk by the check itself.

**Result (completed 2026-10-20, on `main`):** all criteria covered by tests. Verification on macOS: fmt and clippy clean; `cargo test --workspace` passed (nulis-core 108, nulis-desktop 7); lint and typecheck clean; Vitest 124 passed. Details: `handover/M23_HANDOVER.md`.

## User requests queued during M21

- **M23 External changes and Reload:** prompt when another program changes or deletes an open file (Reload / Keep mine), plus a manual Reload command (also Revert to Saved). Core already fingerprints sources and flags changed sources during recovery (`FEATURE_PARITY.md`).
- **M24 New application icon:** done — new master SVG and icon sets; the in-window mark follows the theme; About and README updated.
- **Done in M21:** the reference product's name was removed from code and documents (README, syntax comment, CLEAN_ROOM, DECISIONS, IMPROVEMENT_SPEC, this plan). It remains in earlier git commit contents and messages; rewriting history was not done because it is destructive and needs the owner's decision.

## Design decisions and deviations

- M21 (revised after user testing): Find and Replace and the matching-lines list are separate native windows like Settings, not panels inside the main window. Verification after the change: Vitest 121 passed; `cargo test -p nulis-desktop` 7 passed; clippy, lint, typecheck clean.

- Milestones are numbered M19+ to continue `ROADMAP.md` rather than restart at M01.
- Reference shortcuts are not copied; Nulis keeps its own bindings.

## Known issues and open questions

- Windows/Linux verification of the M19 window fix is pending.
- Clipboards/Templates/Spacing/Hide-Show Lines submenus were not expanded in the screenshots; their behavior is unclear and not planned.

## Next steps

0. M25 Format and transforms is next.
1. Run the app (`npm run dev`) and try M20 keys/context menu and the M21 window by hand on macOS.
2. Push to CI to build Windows/Linux, and check the M19 Settings/Help window fix there.
3. Start M22 per the scope above, then M23 and M24 (user requests).
