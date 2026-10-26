# M25 handover — Format and transforms

**Status:** completed 2026-10-20 on macOS (automated checks), on `main`. Not tried by hand; Windows/Linux not run.

## Implementation

- `crates/nulis-syntax/src/lib.rs`: `CommentSyntax` and `LanguageRegistry::comment_syntax` (first line marker and block markers from each lexer spec; markup languages default to `<!-- -->`; Python `#`; Markdown `<!-- -->`; none for plain text and JSON).
- `crates/nulis-core/src/editor.rs`: `TextTransform` gains `InvertCase`, `Capitalize`, `Base64Encode` (raw bytes), `Base64Decode` (whitespace ignored; errors `InvalidBase64`, `Base64NotText`); error `NoCommentSyntax`.
- `crates/nulis-core/src/editor/line_ops/format.rs` (new): `LineFormatOptions { tabWidth = 4, indentWithSpaces = false, ignoreCase = false }`; `Plan::format_lines` splits each block into lines (keeping CR/LF/CRLF), reformats, and maps carets per line (shifts at the insertion column; sort selects the block). Unchanged blocks become no-op edits so their carets survive; all-unchanged returns no edit.
- `line_ops.rs`: new `LineOperation` variants and `line_operation_with(…, options)`; trim, tab conversion, and sort cover the whole document when every selection is empty.
- `lib.rs`: `editor.lineOperation` takes `options`; presentation commands `edit.*` (6) and `format.*` (17) added to `WORKBENCH_ACTIONS`; IPC 30 → 31.
- Adapter: native **Format** menu (5 groups) and line commands appended to **Edit**.
- Desktop: `commands.ts` `EDITOR_ACTIONS` maps command IDs to action names; `App.tsx` `sendEditorAction` routes them to the focused editor through a new `editorAction` prop; `EditorViewport.tsx` `runEditorAction`, `formatDateTime`, Tab/Shift+Tab/Ctrl+⌘ /, and a grouped **Format** list replacing the three-item Transform list.

## Tests

- Core: indent/outdent with tabs and spaces and caret mapping; toggle comment in Rust (aligned, uncomment), HTML block comments, plain text refused; trim/space→tab/tab→space over the whole document and one undo; sort ascending/descending/unique with endings preserved and selection-only sort; capitalize/invert/Base64 round trip and invalid input.
- Syntax: comment markers per language.
- Desktop: Tab indents a multi-line selection, Shift+Tab, Ctrl+/; Tab inserts a tab otherwise; Format list → transform and line operation; `editorAction` → Base64 and date insertion; palette → focused editor.

## Known issues / remaining

- Tab width (4) and tab-vs-spaces indentation are fixed; there is no setting yet (the options exist in the command for a future Settings control).
- Sorting compares Unicode code points (or lower-cased text), not locale collation.
- Toggle comment uses only the first line marker of a language; block-comment languages wrap each line.

## Resume

Next: M26 Search and navigation.
