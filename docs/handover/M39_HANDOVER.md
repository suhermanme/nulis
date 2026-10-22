# M39 handover: Print, Page Setup, and printing on in every build

Plan: `IMPROVEMENT_PLAN_3.md` §3.2, §3.6, §4 (M39). Status: done 2026-10-22 on branch `m36-print`; checked in the app on macOS the same day (see below). Windows and Linux not tried.

## What changed

| Area | Files |
| --- | --- |
| Page model for Print: each `Page` becomes text pieces placed on the cell grid (`PrintedPage`), plus what every page shares (`PageFrame`: size, margins, rows, cell width, colors). Wide characters and characters JetBrains Mono lacks are pieces of their own, as the PDF places them | `crates/nulis-print/src/model.rs` (new), `lib.rs` (`print_pages`, `page_frame`, one shared `paginate` for PDF and Print), `setup.rs` (`band_layout`, shared with `pdf.rs`), `fonts.rs` (`bundled_has_glyph`) |
| Core: `DocumentManager::print_pages` (same lines and options as Export to PDF, through `print_source`), `print_preview` (first page only, `{pages}` estimated), the 5,000-page Print limit | `crates/nulis-core/src/editor/print_export.rs` |
| Commands: `file.printStart` (a job, sharing `file.exportPdfProgress` and `file.exportPdfCancel`; asks above 1,000 pages, refuses above 5,000), `file.printPreview`, `print.pages` (read the prepared pages, 100 at most per call), `print.release` | `crates/nulis-core/src/print_jobs.rs` |
| Print Preview window reads `print.pages` (added to its allowlist) or, for Markdown, `print_window_content` / `print_window_markdown_image`, which only reach the document the editor window chose. The prepared pages are released when the window closes | `apps/desktop/src-tauri/src/lib.rs` (`PrintContent`, `PrintContentState`, `open_print_window(content)`) |
| Print Preview draws the core's pages as fixed-size sections (`PrintedPageView`), loads them in batches of 50, and prints on their paper; a Markdown document prints its formatted preview on the Page Setup paper and margins | `apps/desktop/src/PrintApp.tsx`, `PrintedPageView.tsx` (new), `print.css`, `MarkdownPreview.tsx` (image loader can be swapped) |
| Page Setup dialog: paper, orientation, margins (narrow, normal, wide, custom), font size, long lines, colors (print, black and white, current theme), line numbers, syntax highlighting, header and footer; live first-page preview from the core; Restore Defaults | `apps/desktop/src/PageSetupDialog.tsx` (new), `styles.css` |
| Saved Page Setup values, used by Print, Export to PDF, and the preview | `apps/desktop/src/printSettings.ts` (new) |
| File ▸ Print… runs the layout job with the status-bar progress bar and Cancel, then opens Print Preview; Markdown in View mode prints the preview | `apps/desktop/src/App.tsx`, `EditorViewport.tsx` (`onMarkdownView`) |
| Printing in every build: `PRINT_COMMANDS_ENABLED` and `PRINT_MENU_ENABLED` removed | `crates/nulis-core/src/lib.rs`, `apps/desktop/src/commands.ts`, `App.tsx`, `src-tauri/src/lib.rs` |
| IPC protocol 41 (`open_print_window` takes the content; new commands) | `crates/nulis-core/src/lib.rs`, `apps/desktop/src/contracts.ts` |

## Decisions

- **Page Setup values are stored like the View options**, in the editor's local storage (`nulis.printSettings.v1`), not in `nulis-settings`. The resume note suggested `print.*` in the settings crate, but nothing the desktop changes at run time is stored there today (`SettingsStore` has only built-in defaults and is never written), while every other remembered choice uses `preferences.ts`. The core still checks every value (`PageSetup::validate`). `settings.schema.json` is left for M40 with the rest of the docs.
- **Print pages are kept as JSON text in the core** until the window reads them (about 15 KB a page, so at most about 75 MB at the 5,000-page limit), and dropped when the window closes or the next Print starts.
- **Print and Export share one job slot.** Starting either while the other runs is refused in the editor window with a message.
- **The Page Setup preview** lays out only the first page. In a document over 8 MiB it does not count lines first, so its line-number gutter is as wide as for ten million lines; the page count in the footer is the same estimate the large-print question uses.
- **No ligatures in print:** JetBrains Mono draws `->` and similar pairs as one glyph by default; Print turns that off so pages match the PDF.
- **Characters the bundled font lacks** are drawn by the webview's own fallback fonts in Print, centred in their cells. Print can therefore show emoji and scripts the PDF leaves blank.
- **Markdown** prints the formatted preview with light colors, flowing within the Page Setup margins; there is no header or footer on those pages.
- **Print Selection** is not offered: Print prints the whole document (or the hex dump in the hexadecimal editor). Export Selection to PDF covers selections.

## Verification

- `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -D warnings` clean; `cargo clippy -p nulis-desktop -p nulis-core --release --all-targets -D warnings` clean.
- `cargo test --workspace`: all pass. New tests:
  - `nulis-print`: pieces split around wide and missing characters; a page's header, footer, gutter, runs, continuation arrow, and rules land where the PDF draws them; the frame's sizes.
  - `nulis-core`: Print lays out the same number of pages as Export to PDF and hands them out in batches (line 62 starts page 2, the footer reads "Page 2 of 4"); `print.release` drops them; above 1,000 pages Print asks, above 5,000 it refuses ("at least 6,558"); wrapped lines that pass the limit during layout stop it; the first-page preview (landscape, estimated page count, a refused setup).
  - `nulis-desktop`: Print Preview may call `print.pages` but not `file.printStart`, `file.exportPdfStart`, `print.release`, `file.open`, or `file.save`.
- Frontend: lint and typecheck clean; Vitest 21 files, 190 tests. New: Print Preview reads pages in batches, keeps HTML-like document text as text, waits for every page, prints on the frame's paper; Markdown on the Page Setup paper; the empty window; the `@page` rule; Print from the File menu; the 5,000-page message; Page Setup preview, save, and Export using the saved values; a refused setup cannot be saved; the settings rules.

## To check in the app (macOS)

Back up the user data first (see `PLAN3_RESUME.md`). Then:

1. File ▸ Page Setup… on a code file: the preview follows each change; Save; reopen and the values are kept.
2. File ▸ Print… on the same file: the progress bar, then Print Preview with every page; Print… opens the system dialog on the chosen paper. Save as PDF and compare with File ▸ Export to PDF… of the same file (same pages, same line breaks).
3. Print `big-server.log`: "Print 2,460 pages?", then about 2,460 pages in the window. Printing more than 5,000 pages is refused with a message.
4. A Markdown file in View mode: Print… shows the formatted preview on white paper.
5. The hexadecimal editor: Print… prints the hex dump.
6. Current theme colors with a dark theme: the pages are dark.
7. A release build (`npm run build`) shows Print, Page Setup, and Export to PDF.

## Result of the check in the app (macOS, 2026-10-22)

1. Page Setup: the preview follows each change; saved values are kept. **Passed.**
2. Print of `sample.rs`, saved as PDF from the system dialog, against Export to PDF of the same file: both 2 A4 pages, same lines, page breaks, header, footer, and colors; page 1 renders the same. The Export file is larger (271 KB against 27 KB) only because Nulis embeds its font differently from macOS. **Passed.**
3. `big-server.log`: asks before printing, shows progress, Cancel works. **Passed.**
4. Markdown in View mode: Print Preview showed the formatted page, but **Page Setup previewed the source text** with line numbers. Fixed: `PageSetupDialog` takes `loadMarkdown` and, when Print would print the formatted preview, shows that page on the chosen paper and margins, with a note that only paper, orientation, and margins apply (`PageSetupDialog.test.tsx`). Rechecked. **Passed.**
5. Hexadecimal editor: Print prints the hex dump. **Passed.**
6. Current theme colors with a dark theme, and with a light theme. **Passed.**
7. Release build menu: not built; the switches that hid the items are gone, and the tests cover the menu.

## Windows and Linux check (2026-10-22, test VMs)

- Page counts match on every system for the same Page Setup: `big-server.log` at A4 landscape gives 3,751 pages at 9 pt and 5,173 at 12 pt on macOS, Windows, Ubuntu, and the reference export.
- **Linux printed landscape pages shrunk onto portrait paper:** WebKitGTK opened its dialog on the default paper. Fixed: Print opens the GTK dialog itself with the paper set (`print_on_linux` in `src-tauri/src/lib.rs`, with `gtk` and `webkit2gtk` at wry's versions). A landscape orientation in the GTK page setup makes WebKitGTK 2.50 print blank pages (reproduced with a Python GI script on the Ubuntu VM), so a landscape page is passed as an upright custom paper of the turned size, with zero page margins.
- **Windows added its own header and footer** (date, title, `tauri.localhost` address, page number) to Markdown prints, in the `@page` margins. Fixed: where the engine repeats padding on each page (`box-decoration-break: clone`, Chromium), the page margin is 0 and the margins are that padding (checked with Chrome 155: no header or footer, 15 mm kept at the top of every page). Code pages already print with no page margin.
- The Print error over 5,000 pages said "Could not export to PDF"; it now says "Could not print".
- Rechecked after the fixes: Windows prints Markdown without the browser's header and footer, margins kept (2026-10-22); Ubuntu prints landscape at full size on A4 landscape paper (2026-10-23).

## Open

- Windows: the paper is not set before the print dialog (macOS and Linux set it).
- User guide and in-app Help: done 2026-10-22 (`USER_GUIDE.md` ▸ Files ▸ Printing and PDF, the Print shortcut, and two limits; Help is built from the guide).
- `settings.schema.json` and the release notes: M40.
