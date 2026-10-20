# M26 handover — Search and navigation

**Status:** completed 2026-10-20 on macOS (automated checks), on `main`. Not tried by hand; Windows/Linux not run.

## Implementation

- `crates/nulis-core/src/editor/navigation.rs` (new): `change_near(from, direction)` (line start of the next/previous line-change range outside the caret's line), `bookmark_lines` (one forward pass with `LineTracker`, previews via `line_preview`), `delete_bookmarked_lines` (bookmarks as carets → `DeleteLines`, then clears bookmarks; restores the selection on failure).
- `editor.rs`: `character_count` now also returns `words` (runs of non-space bytes) and `lines` (LF, CRLF, and lone CR endings + 1) in the same pass and under the same 64 MiB bound.
- `lib.rs`: commands `editor.changeNear`, `editor.bookmarkLines`, `editor.deleteBookmarkedLines` (macro-recordable); 18 `search.*` presentation commands; the M20–M26 registrations grouped in `register_editing_extensions`; IPC 31 → 32.
- Adapter: native **Search** menu (find, files, navigation, bookmarks).
- Desktop:
  - `commands.ts`: `search.*` editor actions and `FIND_WINDOW_COMMANDS` (open the Find window on a tab).
  - `EditorViewport.tsx`: `navigate()` for every search action; per-view back/forward stacks (100 entries) recorded before Go to Line, find, bookmark moves, bracket jumps, symbol jumps, and change navigation; `matchBrace`; Go to `line:column` (column in characters, clamped to the line); F3, Ctrl/⌘ Shift \, Ctrl/⌘ [ ], Alt F5; `onShowResults` for the bookmark list; `codePoint` in the caret report.
  - `App.tsx`: routes `search.open*` to the Find window and other `search.*` to the focused editor; status bar shows words, lines, and `U+XXXX`.

## Tests

- Core: change navigation across two edited lines and the ends; bookmark list numbering and Delete Bookmarked Lines with undo; character/word/line counts with UTF-8 and mixed endings.
- Desktop: bracket jump → next change → back, back, forward; F3 opens Find with no query and repeats a search with Shift F3; List Bookmarks → results; Go to `40:3` with a multi-byte character; status bar counts and code point.

## Known issues

- Go Back/Forward history is per editor view and lost when the view closes; it does not cross documents.
- Matching bracket uses the bracket pairs of the loaded viewport window, so a partner far outside it is not found.
- Ctrl/⌘ Shift \ is layout-dependent; it is matched as `\` or `|`.
- The status bar's "LF" label is still fixed (M27 adds real line-ending detection).

## Resume

Next: M27 View and file.
