# M21 handover — Find and Replace window

**Status:** completed 2026-10-20 on macOS (automated checks). Not yet tried by hand in the running app; Windows/Linux not run.

## Objectives

Replace the side-bar Search panel and the inline find bar with one floating Find and Replace window (user request with 9 screenshots), including a "lines containing" list that goes to the clicked line.

## Implementation

**Search engine (`crates/nulis-search`)**

- `SearchOptions.whole_word` (serde default `false`): a whole-word search becomes `\b(?:…)\b` around the escaped literal or the user's regex. Literal whole-word searches therefore need UTF-8 query bytes.

**Core (`crates/nulis-core`)**

- `src/editor/find_all.rs` (new): `DocumentManager::find_all` returns matches with line numbers plus one entry per matching line (first 512 bytes, match count), optionally inside a byte range, capped at 10,000 matches / 5,000 lines with a `truncated` flag. Lines are counted in one forward pass (`LineTracker`, works on any `ReadAt`), so cost is proportional to the searched range. `add_bookmarks` (Bookmark All).
- `editor.rs`: `replace_all` takes `within: Option<(u64, u64)>` (selection scope); `find_in_files` takes a `FileSearchScope { include, recursive }` (name wildcards `*`/`?`, case-insensitive) and returns `line` and `line_text` per match.
- `lib.rs`: commands `search.findAll`, `editor.addBookmarks`; `search.replaceAll` gains `within`; `search.findInFiles` gains `scope`. IPC 27 → 28.

**Desktop (`apps/desktop/src`)**

- `FindReplacePanel.tsx` (new): `FloatingWindow` (draggable, non-modal, Escape closes), `FindReplacePanel` (Find / Find in Files / Replace tabs, scopes, options, history in `localStorage`, Count All, Previous/Next, Replace, Replace All), `ResultsWindow` (matching lines, Go To, Bookmark All, Copy, Show line numbers, keyboard navigation).
- Current-document Next/Previous/Replace/Replace All and highlighting go through the existing `SearchRequest` path into the focused `EditorViewport`; selection and all-open-files scopes use `search.findAll` / `search.replaceAll` directly.
- `EditorViewport.tsx`: inline find bar removed; Ctrl/⌘ F/H, the toolbar magnifier and the context menu call the new `onOpenFind(tab, seed)` prop (seeded with a selection ≤ 1,000 chars); whole-word highlighting; `SearchRequest` gains `wholeWord`, `highlight`, `within` and the `highlight` action; `CaretPosition.selection` reports the primary selection.
- `App.tsx`: Search removed from the activity bar and side bar (older sessions that saved the Search panel fall back to Files); panel and results window rendered; `openMatch` opens a document or file at a match.
- `contracts.ts`, `icons.tsx` (history icon), `styles.css` (theme-token styles, forced-colors support).

**Docs:** `USER_GUIDE.md` (Search chapter rewritten, activity bar, shortcuts), `FEATURE_PARITY.md`, `IMPROVEMENT_PLAN.md` (§4.8 and roadmap), tracker.

## Tests

- Core: `find_all_reports_lines_previews_and_counts` (CR/CRLF/LF lines, whole word literal and regex, cap, range), `replace_all_within_a_range_touches_only_that_range` (one undo step), `bookmark_all_adds_line_starts_and_checks_bounds`, `find_in_files_filters_names_skips_subfolders_and_reports_lines`, `wildcards_match_whole_names`; search crate `whole_word_search_skips_parts_of_longer_words`.
- Desktop: viewport tests rewritten for the panel hand-off and request-driven highlighting; App tests for Find in Files (filter, folder, subfolders, open at line), current-document find/replace, Count All + lines list + Go To + Bookmark All + line-number toggle, and all-open-files Replace All / empty-selection guard.
- See the tracker for the full-suite results.

## Decisions

- The window lives inside the main window (a positioned, draggable panel), not a separate OS window: it needs live editor state, and separately created windows are what failed on Windows (M19).
- Replace in Files is its own milestone (M22) because it writes files on disk.
- Not implemented from the screenshots: search in column range, preserve case, regex dialect choice, ignore options, favorites.

## Known issues

- **Replace** (single) works only in the current document; for other scopes use Replace All.
- "Selected text" uses the selection at the moment the button is pressed; stepping matches in a selection or open files uses a list computed once and refreshed when a document's revision changes.
- Line previews show the start of a line (512 bytes), so a match far into a very long line is not visible in the preview.
- The window's position is not remembered between openings.

## Resume

Next: M22 Replace in Files (preview + confirmation, open documents edited in memory, closed files saved atomically). Then the user-requested M23 (external file change prompt and Reload) and M24 (new app icon).
