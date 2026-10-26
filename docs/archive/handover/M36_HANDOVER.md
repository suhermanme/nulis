# M36 handover: print spike and commands

Plan: `IMPROVEMENT_PLAN_3.md` §3.0, §3.1, §4 (M36). Status: code and automated tests done 2026-10-21 on branch `m36-print`. **macOS print check passed** (below); Windows and Linux are still to do.

## What changed

| Area | Files |
| --- | --- |
| Commands `file.print`, `file.pageSetup`, `file.exportPdf` as workbench actions, registered only while `PRINT_COMMANDS_ENABLED` (development builds) | `crates/nulis-core/src/lib.rs` |
| Print Preview window (`print`): `open_print_window`, `print_window_print` (calls `WebviewWindow::print()`), theme-only allowlist `PRINT_COMMANDS`, closes with the editor | `apps/desktop/src-tauri/src/lib.rs`, `capabilities/print.json` |
| Native File menu: Export to PDF…, Page Setup…, Print… (Primary+Alt+P), development builds only | `apps/desktop/src-tauri/src/lib.rs` (`append_print_menu_items`) |
| In-window File menu and handlers; Page Setup and Export to PDF say "not finished yet" | `apps/desktop/src/App.tsx`, `commands.ts`, `ipc.ts`, `main.tsx` |
| macOS: the shared `NSPrintInfo` gets the preview's paper and orientation before printing (`set_macos_print_paper`); `print_window_print` takes a validated `PrintPaper` | `apps/desktop/src-tauri/src/lib.rs`, `Cargo.toml` (`objc2-app-kit` `NSPrintInfo`, already in the lockfile through wry) |
| Calibration pages: A4/Letter, portrait/landscape, 15 mm margin box, 10 mm ticks, numbered lines ending in "LAST LINE OF PAGE n", `@page` size rule | `apps/desktop/src/PrintApp.tsx`, `print.css`, `PrintApp.test.tsx` |
| JetBrains Mono 2.304 Regular and Bold (OFL-1.1) | `apps/desktop/src/assets/fonts/jetbrains-mono/`, `THIRD_PARTY_LICENSES.md`, `.gitattributes` |

## Findings and decisions

- **Tauri 2.12.3 / wry 0.57 print paths differ per platform:**
  - macOS: `NSPrintOperation` from the `WKWebView`, run as a sheet on the webview's window, with zero margins from `PrintOptions::default()`. The window must be visible, so Print Preview is a visible window, not the hidden one the plan first described (plan §3.0 and §3.6 updated).
  - Windows: wry runs `window.print()`, so WebView2 shows Chromium's print preview. Its own margin and header/footer options apply on top of ours; `@page { margin: 0 }` should switch Chromium's headers and footers off.
  - Linux: `webkit2gtk::PrintOperation::run_dialog` with no parent window.
- **macOS uses the paper of the last print job and ignores `@page { size }`**, shrinking the pages to fit (first run: A4 pages printed on A5 at 70.5%). Nulis now sets the paper and orientation itself before opening the dialog.
- **`styles.css` pins `html`, `body`, and `#root` to the window with `overflow: hidden`.** It loads after `print.css`, so the print rules are scoped to `html.print-window` to win; without that only the visible part of the window printed ("Page 1 of 1").
- **A page exactly as tall as the paper can spill a blank page** (A4 portrait: 297 mm = 1122.5 px). Printed pages are 1 px shorter.
- **PDF library: `pdf-writer` 0.15** (MIT OR Apache-2.0), with `subsetter` 0.2 and `ttf-parser` 0.25 for embedding a font subset. `krilla` 0.8 was considered: it is easier, but it keeps the whole document in memory until it finishes, which conflicts with streaming very large files to disk (plan §3.4). Not added to `Cargo.toml` yet; M37 adds them.
- **Font:** one copy of the TrueType files, used by the Print Preview through `@font-face` and, from M37, embedded in PDFs, so both outputs use the same glyphs.
- **Feature flag:** `nulis_core::PRINT_COMMANDS_ENABLED = cfg!(debug_assertions)` and the matching `PRINT_COMMANDS_ENABLED = import.meta.env.DEV` in `commands.ts`. M39 sets both to always on.

## Verification

- `cargo fmt --check` clean.
- `cargo clippy -p nulis-core -p nulis-desktop --all-targets -D warnings` clean in both debug and release.
- `cargo test`: nulis-core passes; nulis-desktop 9 tests pass, including the new Print Preview allowlist checks.
- Frontend: lint and typecheck clean; Vitest 20 files, 176 tests (2 new in `PrintApp.test.tsx`).

## macOS result (2026-10-21, macOS 26, Save as PDF from the system dialog)

| Paper | Pages | Page size | Margins measured | Each page ends with its last line and footer |
| --- | --- | --- | --- | --- |
| A4 portrait | 3 | 210.0 × 297.0 mm | 15.0 mm left and top, 15.1 mm bottom | yes |
| A4 landscape | 3 | 297.0 × 210.0 mm | 15.0 mm left, 14.7 mm top, 15.2 mm bottom | yes |

The font rendered as JetBrains Mono, with accented letters, arrows, λ and Ж. Measured with PDFKit (text positions of the header, the last line, and the footer). Letter was not printed.

## To check by hand on Windows and Linux

Build a development build (it shares the real user data directory `com.nulis.editor`, so back it up first). Then File ▸ Print… ▸ Print… on each platform, print to PDF (macOS Save as PDF, Microsoft Print to PDF, CUPS-PDF), with A4 and Letter in both orientations:

1. Exactly 3 pages come out, with no blank pages in between.
2. The dashed margin box is 15 mm from each edge, and the red ticks fall every 10 mm. Measure in a PDF viewer.
3. Each page ends with "LAST LINE OF PAGE n" and its footer, and nothing spills onto the next page.
4. The text is JetBrains Mono, and accented letters, arrows, λ and Ж print.
5. Windows: the browser header and footer (date, URL) do not appear.

If a platform fails 1–3, plan §6 has the fallback: print the generated PDF on that platform instead.

## Open

- Manual checks above: Windows test VM and Linux. Letter paper on macOS.
- Windows/Linux have no paper-setting code yet. If Chromium or WebKitGTK also ignore `@page { size }`, add the equivalent there (WebView2 `PrintSettings`, GTK `PageSetup`).
- Primary+Alt+P works through the native menu accelerator only. On macOS Option+P types "π" when the menu does not catch it; check on macOS.
