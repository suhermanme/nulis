# M31–M32 handover: Find window fit, regular expressions, column search

Plan: `IMPROVEMENT_PLAN_2.md` §2.3, §2.5, §3.0, §3.2. Status: completed 2026-10-20 on `main`, not yet committed or released.

## What changed

| Area | Files |
| --- | --- |
| Search engine: unbounded regex with 1 MiB cap, resume after the last match, `search_until`, column search (`ColumnRange`, line scanning over a `Slice` view), replacement templates (`ReplacementTemplate`, `expand_replacements`, `plan_replacements_each`), readable regex errors | `crates/nulis-search/src/lib.rs` |
| Core: Replace All with groups, `replace_match`, `matches_in_range`; commands `search.replaceMatch`, `search.matchesInRange`, `search.validatePattern`; IPC protocol 36 | `crates/nulis-core/src/editor.rs`, `editor/find_all.rs`, `lib.rs` |
| Desktop shell: `fit_find_window` (Find window only), `open_help_window` with a topic, `search.validatePattern` allowed for the Find window | `apps/desktop/src-tauri/src/lib.rs` |
| Find window: options layout, Search in column, remembered options, status-line errors, live highlights, Regex help button, fit to content | `FindReplacePanel.tsx`, `FindApp.tsx`, `fitWindow.ts`, `styles.css` |
| Editor: highlights from core matches, single Replace through `search.replaceMatch`, column options | `EditorViewport.tsx`, `contracts.ts` |
| Help: opening at a topic | `HelpWindow.tsx`, `HelpApp.tsx`, `ipc.ts` |
| Command errors shown with the core's message app-wide | `ipc.ts` (`commandError`) |
| User guide: Regular expressions, Search in column | `docs/USER_GUIDE.md` |

## Decisions

- One regex style on the linear-time `regex` crate (user decision). Lookaround and backreferences in patterns are refused with a message naming them; `fancy-regex`, PCRE2 and Oniguruma were considered and left out.
- Unbounded patterns scan in chunks of four times the overlap (about 4 MiB) so re-reading the overlap costs about 25%.
- Column numbers are the status bar's: Unicode scalars from 1, a tab is one. Matches stay inside one line in column mode.
- The window fit uses the difference between needed and shown height because Tauri's inner size on macOS includes the title bar for this window.
- Errors show in the status line above the buttons so controls do not move while typing.

## Verification

fmt and clippy clean; `cargo test --workspace` (nulis-core 140, nulis-search 22, nulis-desktop 9); lint and typecheck clean; Vitest 19 files, 170 tests. Manual on macOS (debug build): Find and Replace tabs fit; `id=\d+` finds and highlights; columns 1–4 restrict matches; `$2:$1` Replace All and a single undo; lookbehind error text.

## Open

- Windows and Linux: window fit and the Help topic event are untested there.
- The backreference error and single Replace with groups were checked by tests only, not by hand.
- Once, after the app was killed mid-session with the user's old session data, the main window opened blank; it did not happen again with fresh data. Watch for it.
- Debug builds share the user's data directory (`com.nulis.editor`); back it up before manual runs.
