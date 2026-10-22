# Plan 3 (print and PDF): where to resume

Written 2026-10-21 at the end of a session, for the next one. Read this first, then `IMPROVEMENT_PLAN_3.md` and the milestone handovers (`M36_HANDOVER.md`, `M37_HANDOVER.md`, `M38_HANDOVER.md`).

## State

- **Branch `m36-print`** was merged into `main` (fast-forward) on 2026-10-22 at the end of M40. Not pushed. The working tree was clean when this note was written.
- **M36** (Print Preview spike, commands, font) and **M37** (`nulis-print`, Export to PDF) are done and were checked by hand on macOS.
- **M38** (export job with progress and Cancel, large-export question, selection, hex dump, font fallback, highlight windows) is done and was checked in the app on macOS on 2026-10-22 (results in `M38_HANDOVER.md`). One bug found there (exported PDFs got mode `0600`) is fixed.
- **M39** (Print from the core's pages, Page Setup dialog, Markdown preview print, printing in every build) is done and checked on macOS (2026-10-22, `M39_HANDOVER.md`). **M40** (docs and release notes) is done (`M40_HANDOVER.md`). What remains: the Windows and Linux checks (step 6 below), then the 0.4.0 release.
- Printing is in **every build** since M39 (the development-only switches were removed).
- IPC protocol 41.
- Last full check (all passed):
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - in `apps/desktop`: `npm run typecheck`, `npm run lint`, `npx vitest run --environment jsdom` (20 files, 178 tests).

## Running a test build safely

Development builds use the real app identifier `com.nulis.editor`, so they read and write the user's own session. Always:

1. Ask the user to quit the installed Nulis. Only one copy may run, and a second launch just brings the first forward.
2. Back up:
   ```sh
   B=<scratchpad>/nulis-backup-<name>; mkdir -p "$B"
   cp -Rp ~/Library/Application\ Support/com.nulis.editor "$B/appsupport"
   cp -Rp ~/Library/WebKit/com.nulis.editor "$B/webkit"
   cp -p ~/Library/Preferences/com.nulis.editor.plist "$B/prefs.plist"
   ```
3. Start: `cd apps/desktop && npm run dev`, in the background. Vite reloads CSS and TS changes; a Rust change rebuilds and restarts the app.
4. Afterwards, stop it and restore:
   ```sh
   rsync -a --delete "$B/appsupport/" ~/Library/Application\ Support/com.nulis.editor/
   rsync -a --delete "$B/webkit/" ~/Library/WebKit/com.nulis.editor/
   defaults import com.nulis.editor "$B/prefs.plist"
   diff -rq "$B/appsupport" ~/Library/Application\ Support/com.nulis.editor
   ```

The user can drive the dialogs; UI scripting is not allowed on their Mac. They save PDFs to the Desktop, and we measure them.

**Checking PDFs** (macOS, no other tools installed):
- `docs/handover/tools/pdf_measure.swift` gives page count, size, and positions of the calibration text.
- `docs/handover/tools/pdf_inspect.swift` gives size, title, first and last lines, outline, and renders page 1 to PNG.

Compile with `swiftc -O -o /tmp/x <file>`. Exports are deterministic (no creation date), so `cmp` against a known-good export proves the same output.

**Exporting without the app:**

```sh
cargo run -p nulis-core --example export_pdf -- <input> <out.pdf> ['{"orientation":"landscape"}'] [document|hexDump]
```

## Done: the M38 check in the app (2026-10-22)

Test files on the user's Mac: `~/Desktop/nulis-print-tests/`. If they are gone, regenerate:
- `big-server.log`: 150,000 log lines, about 10 MB.
- `scripts.rs`: Chinese, Japanese, Korean, Greek, Cyrillic, Arabic, Devanagari, Javanese, symbols, and emoji, plus the alignment rows `|中|文|列|` and `|ab|cd|ef|`.

| Check | Expected |
| --- | --- |
| Export `big-server.log` | Dialog "Export 2,460 pages?"; then a progress bar and Cancel in the status bar |
| Cancel partway | "Export to PDF cancelled"; no PDF and no `.nulis-export-*` file left in the folder |
| Export it to the end | About 2,460 pages; the outline has one entry per 100 pages |
| Export Selection with two selections | Only those lines, with their own line numbers, and a thin rule between them |
| Hex editor, Export to PDF | Name ends in "(hex)"; offset, 16 bytes (gap after 8), characters |
| Export `scripts.rs` | Chinese, Japanese, and Korean print, with the two alignment rows lined up; the status bar says 7 characters are left blank (Javanese 5, emoji 2) |

All passed; recorded in `M38_HANDOVER.md` and `IMPLEMENTATION_TRACKER.md`. A second selection is added with Option-drag (Alt on Windows and Linux), not Cmd.

**Test build as an app** (instead of `npm run dev`), from `apps/desktop`; the frontend goes to the scratchpad so the project's `dist` is not overwritten:

```sh
npx tauri build --debug --bundles app --config '{"build":{"beforeBuildCommand":"tsc -b && vite build --mode development --outDir <scratchpad>/dist --emptyOutDir","frontendDist":"<scratchpad>/dist"}}'
```

The app is `target/debug/bundle/macos/Nulis.app`. It uses the same user data, so back up first.

## M39, Print and Page Setup (done; macOS checked)

What was built and what to check: `M39_HANDOVER.md`. The plan below is kept for reference; step 1 stores the values in local storage instead (see the handover's decisions), and step 6 (Windows and Linux) is still open.

Plan §3.2 and §3.6. Suggested order:

1. **Saved settings.** Add `print.*` to the settings (`crates/nulis-settings/src/lib.rs`, which has `SETTINGS_SCHEMA_VERSION` 1 and `AppearanceSettings`; ADR-0005 says layered JSON with schema versions) and to `docs/settings.schema.json`. The value is `nulis_print::PageSetup` plus the color choice. The export commands already accept `setup` and `colors`; the desktop sends nothing yet.
2. **Page Setup dialog.** `file.pageSetup` currently only says "not finished yet" (`App.tsx`). It edits paper, orientation, margins, font size, line numbers, wrap or cut, colors (print, black and white, current theme), and header and footer, with a live first-page preview. For current-theme colors, send the theme's `editor.*` and `syntax.*` keys as `{ "kind": "theme", "colors": { … } }`.
3. **Print from the core's pages.** Today `PrintApp.tsx` shows calibration pages. Replace them with the document's pages from `nulis-print`:
   - Add a command that lays out and returns pages in bounded batches (`Page` → rows → runs, with resolved colors). Add it to `PRINT_COMMANDS` in `src-tauri/src/lib.rs` so the Print Preview window may call it. This window currently reads only the theme; the plan (§3.6) allows "read the prepared page model".
   - Render each page as a fixed-size `<section>`, as the calibration pages do. Keep `html.print-window`, the 1 px shorter printed page, and `set_macos_print_paper` (M36 findings).
   - Print uses the same bundled font through `@font-face` (`print.css`).
   - Cap Print at 5,000 pages and suggest Export to PDF above that (plan §3.4).
   - Keep the Print window visible: on macOS the print dialog is a sheet on it.
4. **Markdown preview print** (moved here from M38): when the preview pane is active, print the rendered preview.
5. **Turn it on:** make both `PRINT_COMMANDS_ENABLED` flags always true and remove `PRINT_MENU_ENABLED`. Check the release build: `cargo clippy --release`.
6. **Windows and Linux.** CI builds the packages; the Windows VM is in the user's memory notes (`windows-test-vm`, install via nightly.link from CI artifacts). Check Print paper size there. M36 only set the paper on macOS: Chromium (WebView2) may honor `@page`, and WebKitGTK is unknown. Check font fallback too: Linux often has only CFF CJK fonts, which are not used.

## Last: M40, docs and release

`USER_GUIDE.md` (the Printing and PDF section is written; check it again before release), `FEATURE_PARITY.md`, `IMPROVEMENT_PLAN.md` §4.1 rows (S04), ADR-0013 (page layout in the core, streaming PDF, fonts), and the release notes. Then merge `m36-print` into `main`.

## Known limits (decided; say so in the user guide)

- No text shaping: Arabic, Indic, and Javanese scripts print incorrectly; emoji (color fonts) are left blank and counted.
- Fallback uses only TrueType-outline fonts (not CFF), at most 16 per PDF. The bundled JetBrains Mono is embedded whole (about 128 KB, plus bold when used), because `subsetter` 0.2 needs Rust 1.85 and the workspace minimum is 1.82.
- A column selection prints the whole lines it spans.
- A comment that started more than 16 KiB before a 64 KiB highlight window can lose its color there.
- Wide characters take two cells by a built-in range table (`nulis_print::layout::cell_width`), not full Unicode East Asian Width data.

## Corrections made during the session

- M37's frontend test count was first written as 21 files and 179 tests; it was 20 and 177 (fixed in `M37_HANDOVER.md`).
- One commit went in with a clippy error because the command chain did not stop on it. It was fixed by amending the commit (`5365f61`). Check clippy's exit status, not only its output.
