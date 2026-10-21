# Improvement plan 3: Print, Page Setup, and Export to PDF (M36–M40)

**Status:** approved 2026-10-21. Open questions answered (§7): Quick Open keeps Primary+P and Print uses Primary+Alt+P; the bundled font is JetBrains Mono; rendered Markdown to PDF and Print All are deferred. Progress will live in `IMPLEMENTATION_TRACKER.md`; handovers in `docs/handover/`.
**Date:** 2026-10-21, against commit `c58f9d2` (Nulis 0.3.0).
**Milestone numbering:** continues `IMPROVEMENT_PLAN_2.md` (M31–M35 are complete), so this plan starts at M36.

## 1. User request

Add printing and export to PDF.

`IMPROVEMENT_PLAN.md` §4.1 listed "Print… / Print All / Page & Print Setup (S04)" as Missing and deferred it. This plan takes it up, and adds Export to PDF.

## 2. Findings

### 2.1 What exists today

- **No print or PDF code anywhere.** No command, menu item, setting, or dependency mentions printing.
- **File menu** is built in `apps/desktop/src-tauri/src/lib.rs` (`file_builder`, around line 600) from command IDs such as `file.save` and `file.saveAs`. Commands are declared in `crates/nulis-core/src/lib.rs` (`CommandDescriptor::new`, around line 3731), so new `file.*` commands appear in the menu, palette, and shortcuts the usual way.
- **The interface never holds the whole document** (README principle 1). It gets bounded viewport snapshots (`Editor::viewport`, `crates/nulis-core/src/editor.rs:1676`). Printing a whole file therefore has to be driven by the core, not by the webview's DOM.
- **Syntax highlighting is bounded per call:** `LanguageRegistry::analyze` parses at most 256 KiB (`MAX_SYNTAX_BYTES`) and 20,000 spans per call (`crates/nulis-syntax/src/lib.rs:11–17`). A full-document print must analyze in windows and join them.
- **Tokens are theme-independent** (`SyntaxToken`: Comment, Keyword, String, …), and colors come from `nulis-theme`. So the printed output can use any theme, including a light print theme, without touching the lexers.
- **Fonts:** the editor uses system monospace fonts through CSS (`--editor-font-family`, `styles.css:58`). Nulis bundles no font files.
- **Markdown preview** renders through `react-markdown` + `remark-gfm` with `skipHtml` (`MarkdownPreview.tsx:34`).
- **Hex view** shows bounded 64 KiB windows (`MAX_HEX_VIEWPORT_BYTES`).
- **Tauri 2.1 with wry.** Tauri exposes `Webview::print()`, which opens the native print dialog on macOS, Windows, and Linux. Neither Tauri nor wry has a cross-platform "save page as PDF"; doing it through each platform's webview API would need platform bindings, and every crate forbids `unsafe`.
- **Extra windows** follow ADR-0010: own window, own capability file, per-window command allowlist (`open_find_window`, `open_settings_window` in `lib.rs`).
- **Primary+P is taken** by Quick Open (`App.tsx:934`), so Print uses Primary+Alt+P (§7, question 1).

### 2.2 What printing has to handle

- Files of any size, including larger than memory (bounded work, progress, cancel).
- Unsaved edits: print what the buffer holds now, not what is on disk.
- Long lines: wrap or cut off.
- Tabs, control characters, invalid bytes (shown as U+FFFD, as in the editor), and the document's encoding.
- Characters the chosen font does not have (CJK, emoji, rare scripts).
- Dark themes: printing white-on-black wastes ink and often prints badly.

## 3. Design

### 3.0 Approach: one page layout in the core, two outputs

A new crate, **`nulis-print`**, turns a document into a list of pages. Both features use it, so Print and Export to PDF produce the same pages.

```text
document (piece table) ──► nulis-print layout ──► page model ──┬─► PDF writer ──► file        (Export to PDF)
  + syntax windows          (paper, margins, font,             │
  + theme colors             wrap, header/footer)              └─► paged HTML ──► print window ──► native print dialog   (Print)
```

- **Layout** reads the document in line ranges, runs syntax analysis per window, expands tabs, wraps or cuts lines, and emits pages of styled runs plus header and footer text. It never holds more than a few pages at once.
- **PDF writer** uses a pure-Rust PDF library (`pdf-writer`, MIT/Apache-2.0, no `unsafe`) and embeds the font file, compressed (M37: subsetting needs Rust 1.85, above the workspace's 1.82). Pages are written to a temporary file as they are produced, then the file is moved into place atomically, like Save.
- **Print** renders the same page model as fixed-size HTML pages (one `<section>` per page, `break-after: page`, `@page { margin: 0 }`) in the Print Preview window and calls `Webview::print()`. The window is visible, not hidden: on macOS wry shows the print dialog as a sheet on the webview's own window (found in M36). The OS dialog then handles printer choice, copies, and page ranges. On macOS that dialog also has "Save as PDF", but Export to PDF stays a separate command so it works the same on all three platforms.

Why not simply call `window.print()` on the editor: the editor only has the lines on screen, uses the dark theme, and is virtualized. Why not generate PDF through the webview: no cross-platform API, and it would need `unsafe` platform code.

### 3.1 Commands and menu (M36)

| Command | Title | Where |
| --- | --- | --- |
| `file.print` | Print… | File menu, palette, toolbar overflow |
| `file.exportPdf` | Export to PDF… | File menu, palette |
| `file.pageSetup` | Page Setup… | File menu, palette |

- File menu order: … Save As… / **Export to PDF…** / separator / **Page Setup…** / **Print…** / separator / Close File …
- The commands act on the active editor. They are disabled when no document is open, and in Compare views (out of scope, §8).
- Shortcut for Print: Primary+Alt+P. Quick Open keeps Primary+P (§7, question 1).
- Macros may run `file.print` (opens the dialog). `file.exportPdf` is not on the macro allowlist in this plan because it writes a file; plugins cannot call either.

### 3.2 Page Setup (M37)

A small dialog, also shown as a "Page Setup…" button inside the Print and Export flow. Values are saved as `print.*` settings (`docs/settings.schema.json`, layered like other settings).

| Setting | Values | Default |
| --- | --- | --- |
| `print.paperSize` | A4, Letter, Legal, A3, A5 | Letter in en-US locale, A4 elsewhere |
| `print.orientation` | portrait, landscape | portrait |
| `print.margins` | narrow, normal, wide, custom (mm) | normal (15 mm) |
| `print.fontSize` | 6–24 pt | 9 pt |
| `print.lineNumbers` | on / off | on |
| `print.longLines` | wrap, cut off | wrap (continuation lines marked with ↪ in the gutter) |
| `print.colors` | print theme (light), current theme, black and white | print theme |
| `print.header` | text with fields `{file}`, `{path}`, `{date}`, `{page}`, `{pages}` | `{file}` left, `{date}` right |
| `print.footer` | same fields | `Page {page} of {pages}` centered |
| `print.syntaxHighlighting` | on / off | on |

- **Print theme:** a built-in light theme tuned for paper (white background, dark text, token colors that stay distinct in grayscale). "Current theme" uses the editor theme, including its background. "Black and white" keeps bold/italic per token and drops color.
- A live preview of the first page sits in the dialog (rendered from the same page model, at small size).

### 3.3 What gets printed (M38)

- **Whole document** by default. **Selection only** when there is a non-empty selection (option in the dialog; rectangular and multiple selections print each range in order, separated by a thin rule).
- **Unsaved buffer** content, with the title "Untitled n" for new documents.
- **Hex view:** when the active view is hex, print a hex dump (offset, 16 bytes in hex, ASCII column), using the same layout engine with plain text runs.
- **Markdown:** when the Markdown preview is the active pane, Print prints the rendered preview (sanitized HTML, `skipHtml` kept, local images only). Export to PDF of rendered Markdown needs its own layout and is deferred (§7, question 3); Export to PDF of a Markdown file exports its source.
- **Characters and bytes:** tabs expand to the editor's tab width; control characters print as their visible symbols (␀, ␛ …); invalid bytes print as U+FFFD, matching the editor. Characters the font lacks are drawn as a box, and the export reports how many were missing.

### 3.4 Large files and progress (M38)

- Layout runs as a background job with a progress bar ("Preparing page 412 of about 3,100") and Cancel, like Count All.
- Page count is estimated from the line index before starting. Above **1,000 pages** Nulis asks before continuing.
- **Print** is capped at **5,000 pages**, because the print window must hold the HTML for every page. Beyond that the dialog suggests Export to PDF, or a page range.
- **Export to PDF** has no fixed cap; it streams pages to disk with bounded memory. It still asks above 1,000 pages.
- Syntax analysis runs per 256 KiB window cut at line boundaries. Each window starts a few lines early and drops the overlap, so a multi-line comment or string that crosses a window edge is usually colored correctly (see §6).
- Files over the syntax size limit, or when `print.syntaxHighlighting` is off, print without color, which is much faster.

### 3.5 Fonts (M37)

The PDF must embed a font, and system fonts differ per machine. Plan:

1. Bundle **JetBrains Mono** (SIL Open Font License, GPL-compatible; §7, question 2), regular and bold, about 300 KB per weight.
2. For characters the bundled font lacks, look up a fallback in installed system fonts (font discovery through a pure-Rust crate such as `fontdb`), and embed a subset of only the glyphs used.
3. Shaping is simple left-to-right, one glyph per character, which is right for code. Right-to-left and complex scripts print in logical order and may look wrong (§8).

Print through the webview uses the same bundled font through `@font-face`, so Print and PDF match. The bundled font is added to `THIRD_PARTY_LICENSES.md`.

### 3.6 Print window and security (M39)

- New window `print` ("Print Preview", visible; see §3.0), with its own capability file `capabilities/print.json` allowing only: read the prepared page model, report progress, close. It cannot run editor or file commands (ADR-0010).
- The page HTML is built from text runs and escaped; document text is never parsed as HTML. CSP stays `script-src 'self'`; no inline scripts.
- Export to PDF writes only to the path chosen in the native Save dialog (`tauri-plugin-dialog`, already used for Save As), through the same atomic-replace code as Save.
- The PDF contains document text, file name, and date; no other metadata. Producer is "Nulis <version>".

### 3.7 PDF details (M37)

- Text is real text (selectable and searchable), with a ToUnicode map so copy-paste from the PDF gives the original characters.
- One PDF outline (bookmark) entry per 100 pages for long documents, and the file name as the document title.
- Page size and margins follow Page Setup exactly; no scaling.
- Output is deterministic for the same input and settings (fixed creation date in tests), so tests can compare files.

## 4. Milestones

| Milestone | Scope | Depends on | Cx |
| --- | --- | --- | --- |
| **M36** Spike and commands | §3.0, §3.1: prove `Webview::print()` with fixed-size HTML pages on all three OSes (margins, page breaks, page count match); pick the PDF library (chosen: `pdf-writer`, see `handover/M36_HANDOVER.md`); add JetBrains Mono; add the three commands and menu items (disabled behind a flag until M39) | – | M |
| **M37** `nulis-print` layout and PDF export | §3.2 settings, §3.5 fonts, §3.7 PDF: page model, wrap/cut, line numbers, header/footer, print theme, streaming PDF writer, Export to PDF end to end for text documents | M36 | L |
| **M38** Scope, large files, other views | §3.3, §3.4, §3.5 fallback: selection printing, unsaved buffers, hex dump, background job with progress, cancel, page-count prompts and caps, windowed syntax, installed fonts for missing characters and two cells for wide ones (Markdown preview print moved to M39) | M37 | L |
| **M39** Print window and Page Setup dialog | §3.2 dialog with live preview and saved `print.*` settings, §3.6 print window and capability, Print end to end (including the Markdown preview), flag removed | M37 (M38 for large files) | M |
| **M40** Docs and release | `USER_GUIDE.md` section, Help pages, `settings.schema.json`, `FEATURE_PARITY.md` and `IMPROVEMENT_PLAN.md` §4.1 rows updated, ADR-0013 (print layout in the core), release notes | M38, M39 | S |

## 5. Testing and acceptance

- **Layout unit tests** (`nulis-print`): page breaks, wrap and cut-off, tabs, wide characters, line numbers across wrapped lines, header/footer fields, every paper size and orientation, empty document, a document of one very long line.
- **PDF tests:** generated files parse with a PDF reader crate in tests; text extracted from them equals the source; deterministic output compared against small golden files; `qpdf --check` in CI where available.
- **Large-file tests:** a 2 GB generated file exports with memory below a fixed budget (benchmark probe in `benchmarks/`); cancel stops within one page; page-count prompt appears above 1,000 pages.
- **Syntax seam test:** a block comment crossing a 256 KiB window edge is colored as a comment on both sides.
- **Interface tests** (Vitest): menu items enabled/disabled per view, Page Setup saves settings, progress and cancel, the 5,000-page Print cap message.
- **Security tests:** document text containing `<script>`, `</section>`, and HTML entities prints as literal text; the print window cannot call `file.save` or `file.open` (capability test like the Settings window's).
- **Manual checks per OS** (macOS, Windows test VM, Linux): print to a real or virtual printer (Microsoft Print to PDF, CUPS-PDF, macOS Save as PDF), compare against the exported PDF page by page; dark theme document prints with the print theme.

Acceptance: a user can print or export any open text document, selection, or hex view, on all three platforms, with matching pages, without the app freezing, and with unsaved edits included.

## 6. Risks

| Risk | Effect | Mitigation |
| --- | --- | --- |
| Webview print engines handle `@page` and margins differently (WebKit, WebView2, WebKitGTK) | Printed pages split differently from the PDF | M36 spike first; fixed-size page sections with zero `@page` margin; if a platform cannot be made to match, print that platform from the generated PDF instead |
| `Webview::print()` behaves differently or is missing on one platform in Tauri 2.1 | Print missing there | Spike in M36; upgrade Tauri if a fix exists; fall back to opening the exported PDF in the system viewer |
| Very large HTML in the print window | Slow or crashing print | 5,000-page cap; suggest Export to PDF |
| Syntax state across 256 KiB windows | Wrong colors near window edges | Overlapping windows; worst case is wrong color, never wrong text |
| Font coverage | Boxes instead of characters | System font fallback; report missing count |
| Bundle size | Installers grow by about 1 MB | Two weights only; subset at export time |
| New dependencies | Supply-chain and license review | Pure-Rust, MIT/Apache-2.0 crates, pinned versions, added to `THIRD_PARTY_LICENSES.md` |

## 7. Open questions (answered 2026-10-21)

1. **Shortcut for Print.** Primary+P is Quick Open. Options were (a) keep Quick Open on Primary+P and give Print Primary+Alt+P, or (b) move Quick Open and use Primary+P for Print. **Answer: (a).** Quick Open keeps Primary+P, so habits from 0.2–0.3 still work; Print uses Primary+Alt+P.
2. **Which font to bundle:** JetBrains Mono, Iosevka, or DejaVu Sans Mono. **Answer: JetBrains Mono** (OFL, good coverage of Latin, Greek and Cyrillic, clear at small sizes).
3. **Rendered Markdown to PDF.** Needs a Markdown layout engine in the core (headings, lists, tables, images). **Answer: deferred.** Printing the preview covers most needs, and the macOS print dialog also has Save as PDF.
4. **Print All** (every open document in one job). **Answer: deferred** to a later plan; easy to add once M38 exists.

## 8. Out of scope

Printing Compare and Folder Compare views, printing the terminal, Print All, rendered Markdown to PDF, right-to-left and complex-script shaping, PDF/A, encrypted or signed PDFs, printing from plugins, and exporting to formats other than PDF (HTML, RTF).
