# M37 handover: `nulis-print` layout and Export to PDF

Plan: `IMPROVEMENT_PLAN_3.md` §3.0, §3.2, §3.5, §3.7, §4 (M37). Status: code and automated tests done 2026-10-21 on branch `m36-print`. Not yet tried in the app by hand. Export to PDF, like Print, is only in development builds until M39.

## What changed

| Area | Files |
| --- | --- |
| New crate `nulis-print`: `PageSetup` (paper, orientation, margins, font size, line numbers, wrap or cut, highlighting, tab width, header and footer with `{file}` `{path}` `{date}` `{page}` `{pages}`), `Geometry`, `Paginator` (styled lines in, pages out, one page in memory), `Palette` (print theme, black and white, current theme colors), streaming `PdfWriter`, and `export_pdf` over a `LineSource` | `crates/nulis-print/` |
| Core: `DocumentManager::export_pdf`, `print_window` (256 KiB windows, cut after the last line break or at a whole character, the editor's own line splitting and syntax analysis), `print_line_count`; command `file.exportPdfTo`; `EditorError::Export`; IPC protocol 40 | `crates/nulis-core/src/editor/print_export.rs`, `editor.rs`, `lib.rs` |
| Example: export any file through the core from the command line | `crates/nulis-core/examples/export_pdf.rs` |
| Desktop: File ▸ Export to PDF… asks where to save (suggesting `<name>.pdf` next to the file), exports the current text with unsaved edits, and reports pages and any missing characters | `apps/desktop/src/App.tsx`, `contracts.ts`, `App.test.tsx` |

## How it works

- **Three passes, each bounded:** count lines (raw bytes, 4 MiB steps) to size the line-number gutter; count pages only when a header or footer shows `{pages}`; then lay out and write. Each step locks the document for one window and checks the revision, so editing is never blocked and an edit stops the export ("the document changed during the export; export it again").
- **Wrapping breaks between words:** at the last space in the second half of the row; a longer word is split at the edge (found when the user exported `IMPROVEMENT_PLAN_3.md`: "Missi / ng").
- **A long line split across windows** arrives as continuation segments that carry on the open row; tab stops continue across wrapped rows.
- **The PDF** is written one object per chunk straight to a temporary file in the destination folder. Only each object's offset is kept. The fonts, page tree, outline (one entry per 100 pages above 100 pages), information, and cross-reference table are written at the end; the file is synced and moved into place. The output is deterministic: there is no creation date.
- **Text is real text:** Type 0 / CID TrueType fonts with Identity-H and a ToUnicode map. Each character is placed in its own cell, so a missing glyph never shifts the columns after it. Missing glyphs are left blank and counted.
- **Refusals:** a relative path, a destination that is the document's own file, a stale revision, out-of-range setup values, and margins that leave fewer than 10 text columns or no rows.

## Decisions

- **The whole font is embedded instead of a subset.** `subsetter` 0.2.6 needs Rust 1.85 (workspace minimum 1.82) and brings `skrifa`, `read-fonts` and `write-fonts`. Compressed, the regular weight adds about 128 KB per PDF, and the bold weight about the same only when a page uses it (keywords in the print palette). Subsetting can come later.
- **Page Setup values are not saved yet.** The command takes a `setup` (defaults otherwise); storing `print.*` settings moves to M39 with the Page Setup dialog that edits them.
- **System font fallback** for characters JetBrains Mono lacks (CJK, emoji) moves to M38. M37 counts them, and the status line reports them.
- **Line numbers count the empty line after a final line break**, as the editor shows it.

## Verification

- `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -D warnings` clean.
- `cargo test --workspace`: all pass. nulis-print has 15 unit tests and 5 export tests (cross-reference offsets point at their objects, page counts, outline above 100 pages, missing glyphs, determinism, empty document). nulis-core has 154 tests, including `export_pdf_reads_every_window_and_refuses_unsafe_destinations`: 40,000 lines give 656 pages, a 700 KiB line read in three windows gives 131 pages, and the refusals hold.
- Frontend: lint and typecheck clean; Vitest 21 files, 179 tests (new: Export to PDF suggests `notes.pdf` and reports pages and missing characters).
- PDFKit (macOS) opens the exported files. Page sizes are right (A4, Letter landscape), text copies out exactly (including ↪ and …), the title and producer are set, and `Samples/sample.rs` renders with the editor's highlighting in the print palette.

## Open

- Try File ▸ Export to PDF… in a development build (back up the user data first; see M36).
- M38: progress and Cancel (exports run in one command call today), the 1,000-page question, syntax state across window edges, selection, hex dump, Markdown preview, system font fallback.
- Wide characters (CJK) take one cell, and combining marks take their own cell.
