# M38 handover: export jobs, selection, hex dump, font fallback

Plan: `IMPROVEMENT_PLAN_3.md` §3.3, §3.4, §3.5, §4 (M38). Status: code and automated tests done 2026-10-21 on branch `m36-print`. Not yet tried in the app by hand. Like the rest of printing, it is only in development builds until M39.

## What changed

| Area | Files |
| --- | --- |
| Export as a job: `file.exportPdfStart`, `file.exportPdfProgress`, `file.exportPdfCancel` (one at a time, like Count All); `file.exportPdfTo` stays for scripts | `crates/nulis-core/src/print_jobs.rs` |
| Progress (`ExportPhase`, `ExportProgress`), `ExportObserver`, cancel, and the question above 1,000 pages (`ExportNeedsConfirmation`, estimated from the line count) | `crates/nulis-core/src/editor/print_export.rs`, `editor.rs` |
| Status bar: progress bar and Cancel; the "Export N pages?" dialog | `apps/desktop/src/App.tsx`, `styles.css`, `contracts.ts` |
| Highlighting in 64 KiB windows with a 16 KiB lookback from a line start | `print_export.rs` (`range_window`) |
| Export Selection to PDF…: every selection with its own line numbers, a rule between ranges, a column selection as the whole lines it spans | `print_export.rs` (`print_spans`, `print_ranges`, `DocumentLines`), `nulis-print` (`separator_before`, `Gutter::Separator`), menus in `App.tsx`, `src-tauri/src/lib.rs`, core `WORKBENCH_ACTIONS` |
| Hex dump: in the hexadecimal editor, Export to PDF writes `offset  16 bytes  characters` rows without line numbers | `print_export.rs` (`HexLines`, `export_hex_dump`), `App.tsx` |
| Font fallback and wide characters: installed TrueType-outline fonts for characters JetBrains Mono lacks, embedded as subsets; Chinese, Japanese, and Korean take two cells | `crates/nulis-print/src/fonts.rs` (new), `pdf.rs`, `layout.rs` (`cell_width`) |
| The export example takes a scope (`document`, `hexDump`) | `crates/nulis-core/examples/export_pdf.rs` |

## Decisions

- **Asking before large exports uses an estimate:** lines (plus separators) over rows per page, a lower bound before wrapping. It costs one fast scan, which the export needs anyway for line numbers. On "Export" the job runs again with `confirmed`; the scan is repeated, which is cheap.
- **Highlight windows are 64 KiB** so dense code stays under `MAX_HIGHLIGHT_SPANS` (20,000 per analysis). Plain export still reads 256 KiB windows. A comment that began more than 16 KiB before a window edge can still lose its color there.
- **Fallback fonts:** `fontdb` 0.23 (MSRV 1.60) finds installed fonts once per run; `subsetter` 0.1.1 (no dependencies; keeps glyph numbers) trims them. Only TrueType outlines are used. CFF fonts (Noto Sans CJK, most `.otf`) would need a different PDF font type, and color fonts (emoji) draw from bitmaps. At most 16 fallback fonts per PDF. Fallback is on in the app and off in the deterministic tests.
- **No shaping:** one glyph per character, so Arabic (unjoined, left to right) and Indic or Javanese scripts do not print correctly. This is out of scope in plan §8.
- **Markdown preview printing moves to M39,** which builds Print. Export to PDF of a Markdown file exports its source, as planned.

## Verification

- `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -D warnings` clean.
- `cargo test --workspace`: all pass. New tests cover:
  - the large-export question (70,000 lines give at least 1,148 pages; nothing is written until confirmed),
  - cancel (no file and no temporary file left),
  - a block comment crossing a 64 KiB edge keeping its color (this test fails with the lookback set to 0),
  - selections (line numbers, rule, no extra empty line, CRLF, column selection, empty selection refused),
  - the hex dump rows,
  - separator rows,
  - wide characters wrapping and cutting whole,
  - fallback fonts (the test adapts to the fonts installed).
- Frontend: lint and typecheck clean; Vitest 20 files, 178 tests (new: the large-export dialog runs again with `confirmed: true`).
- By eye (PDFKit render, macOS 26): `Samples/sample-binary.plist` as a hex dump. A mixed-script file: Chinese, Japanese, Korean, Greek, Cyrillic and symbols print, with CJK columns lined up; Arabic and Devanagari print unshaped; Javanese and emoji are reported missing (7). Fallback fonts used: Arial Unicode MS, AppleGothic, Courier New, Menlo; the PDF is 362 KB.

## Open

- Try in a development build (back up the user data first; see M36): progress and Cancel on a large file, the large-export dialog, Export Selection, the hex dump, a file with CJK text.
  Test files are in `~/Desktop/nulis-print-tests` on the user's Mac: `big-server.log` (150,000 lines, at least 2,460 pages, so the large-export dialog appears) and `scripts.rs` (mixed scripts; 7 characters should be reported missing). The user postponed this check on 2026-10-21.
- Windows and Linux: fallback depends on the installed fonts; Linux distributions often have only CFF CJK fonts (Noto Sans CJK), which are not used yet.
- Rectangular selections print whole lines, not the column block.
