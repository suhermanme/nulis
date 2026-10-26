# M27 handover — View features

**Status:** completed 2026-10-20 on macOS (automated checks), on `main`. Not tried by hand; Windows/Linux not run.

M27 was split from "View and file" before it started (2026-10-20, usage limit); file conveniences moved to M28, column and clipboard tools to M29, integration to M30.

## Implementation

- `crates/nulis-core/src/editor/line_endings.rs` (new):
  - `LineEnding` (`lf | crlf | cr`), `LineEndingStyle` (adds `mixed | none`), and a chunk-safe `LineEndingScanner` used by `character_count`.
  - `convert_line_endings(id, expected_revision, target)`: refuses read-only, stale, documents above 64 MiB, and more than 1,000,000 endings to change. The document gets one small edit per changed ending (cheap undo). Bookmarks and line change markers get at most about 2,048 merged ranges instead, because `ChangeTracker::apply` is quadratic in the number of edits; merged ranges never contain a bookmark, so bookmarks stay exact through convert, undo, and redo. Lines inside a merged range that kept their ending can show as changed.
  - Selections are mapped through the edits in one pass.
- `editor.rs`: `ViewportLine.ending` (absent on the last line and truncated segments); `CharacterCount.line_ending` (above 64 MiB it samples the first 1 MiB); errors `LineEndingConversionTooLarge`, `TooManyLineEndingChanges`.
- `lib.rs`: command `editor.convertLineEndings` (macro-recordable); presentation commands `format.lineEndings{Lf,Crlf,Cr}` and `view.{toggleWordWrap, toggleWhitespace, toggleLineEndings, toggleLineNumbers, toggleColumnMarker, zoomIn, zoomOut, zoomReset, toggleAlwaysOnTop}`; IPC 32 → 33.
- Adapter: Format menu section for line endings; the View menu lists the new view commands; capability `core:window:allow-set-always-on-top`.
- Desktop:
  - `preferences.ts`: `ViewOptions` saved in `localStorage` (`nulis.viewOptions.v1`), `ZOOM_LEVELS`, `stepZoom`.
  - `App.tsx`: view options state (a ref keeps quick repeats correct); Alt Z and Ctrl/⌘ = - 0 handled in the window key handler by `event.code`; zoom scales `--editor-font-size` and the row height only; status bar line-ending button with a convert menu and a zoom badge; Always on Top through `getCurrentWindow().setAlwaysOnTop` (not remembered).
  - `EditorViewport.tsx`: `lineBreakAt` (Enter uses the caret line's ending, else the nearest loaded line's, else LF); props `wordWrap`/`onToggleWordWrap`, `showWhitespace`, `showLineEndings`, `showLineNumbers`, `columnMarker`; spaces and tabs wrapped in spans whose marks are CSS pseudo-elements, so text and click mapping are unchanged.
  - `styles.css`: `--gutter-width`, whitespace and line-ending marks, column marker as a background gradient on each line (the current-line rule now sets only `background-color`).

## Tests

- Core: style detection; CRLF split across the 1 MiB sample and scan chunks; mixed → CRLF → LF → CR with undo/redo; caret and bookmarks after conversion and undo; 10,000 lines with bookmarks (merged ranges) and markers available; stale and read-only refusal; viewport line endings. A throwaway release run (not committed) converted 1,000,000 CRLF lines (22 MB) in 1.4 s, undo 0.7 s, redo 0.8 s, with a mid-line bookmark exact throughout.
- Desktop: `lineBreakAt`; Enter in a CRLF document inserts CRLF; conversion status text; status bar label and convert menu; View options through Alt Z, the View menu, and localStorage; zoom keys, badge, and reset; whitespace and line-ending marks, hidden line numbers, column marker; `preferences.test.ts` for normalizing and zoom steps.

## Known issues

- The column is fixed at 80 (stored as `markerColumn`; no Settings field yet).
- Zoom and the other view options apply to all editors, not per document.
- Native menu items show no shortcut text for Alt Z and Ctrl/⌘ = - 0 (the keys are handled by the web view).
- With word wrap on, the column marker is a fixed x-position; it is only meaningful for rows that are not wrapped.

## Resume

Next: M28 File conveniences.
