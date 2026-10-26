# M29 handover — Column and clipboard tools

**Status:** completed 2026-10-20 on macOS (automated checks), on `main`. Not tried by hand; Windows/Linux not run.

## Implementation

- `crates/nulis-core/src/editor/column_tools.rs` (new):
  - `insert_number_sequence(id, rev, NumberSequence { start, step, padding })`: a column block gets one number per row through `insert_rectangular_text` (short rows are padded); otherwise one number per selection or caret, or at each line start of a single selection that spans lines (the line holding a selection end at column 0 is excluded). Checked arithmetic (`NumberOutOfRange`); at most `MAX_RECTANGULAR_ROWS` targets and `MAX_EDIT_BYTES` inserted.
  - `sum_numbers(id)`: reads the block rows or selections (≤ 8 MiB) and adds standalone numbers as exact decimals (i128 mantissa, at most 18 decimals kept).
  - `justify_columns(id, rev, Justify)`: rewrites each row's cell inside a block of some width (`NeedsColumnSelection` otherwise); blank cells, short rows, and cells that would not survive decoding are skipped; trailing padding only when text follows the block. The block's anchor and active bytes are mapped through the edits so it stays selected.
- `line_ops/format.rs`: `LineOperation::AlignColumns` / `UnalignColumns` (`FormatKind`), `LineFormatOptions.delimiter: Option<char>` (detected from the first line with tab, comma, semicolon, or `|`; `NoColumnDelimiter`). Quoted delimiters are text. Aligning pads each field except the last before its delimiter, measured in display columns; lines without the delimiter are untouched. Unalign trims spaces before each delimiter.
- `nulis-filesystem/src/workspace.rs`: `WorkspaceService::list_files` — breadth-first, at most 20,000 files, 5,000 folders, depth 32; symlinks, dot folders, and `node_modules` skipped; unreadable folders skipped.
- `lib.rs`: commands `editor.insertNumberSequence`, `editor.justifyColumns` (macro-recordable), `editor.sumNumbers`, `workspace.listFiles`; presentation commands `format.{insertNumbers, sumNumbers, justifyLeft, justifyCenter, justifyRight, alignColumns, unalignColumns}`, `edit.{copyAppend, cutAppend, pasteCopy, pasteFromHistory}`, `file.quickOpen`; IPC 34 → 35. New errors: `NoColumnDelimiter`, `NeedsColumnSelection`, `NumberOutOfRange`, `SelectionTooLarge`.
- Adapter: a column section in the native Format menu, clipboard items in the Edit menu, Quick Open in the File extras.
- Desktop:
  - `Picker.tsx` (new): the shared searchable list; Open Recent now uses it.
  - `quickOpen.ts` (new): `matchScore` / `rankMatches` (exact name, name prefix, name substring, path substring, then letters in order with fewer gaps first).
  - `EditorViewport.tsx`: Insert Number bar; `transferSelection(cut, append)` and `appendedClipboard`; Paste and Copy; `onClipboardText` (every copy and cut) and `insertRequest` (paste from history).
  - `App.tsx`: clipboard history (20 entries, each ≤ 1 MiB, memory only); Quick Open (Ctrl/⌘ P) listing open documents, then folder files, then recent files, 50 results; folder files open through `workspace.openFile`.
  - `contracts.test.ts` (new): the desktop protocol constant must equal the core's.

## Tests

- Core: numbers down a block with zero padding and undo; per caret with right alignment and a negative step; per line start; out-of-range refusal; number parsing and exact sums for a selection and a block; right, center, left justify on one block in a row; justify needs a block; align, re-align (no change), and unalign back to the source with a quoted delimiter; a chosen delimiter; no delimiter refused.
- Filesystem: file list walks folders and skips dot folders and `node_modules`.
- Desktop: append helper; copy-append, paste-copy, number bar, sum, and history insert in the viewport; Quick Open ranking and opening a folder file with Ctrl P; Paste from History through the palette; ranking unit test; protocol version guard.

## Known issues

- Justify and number padding count characters, not display width, so wide (East Asian) characters and tabs inside a block can misalign.
- Convert to Fixed-Width pads with spaces before a tab delimiter, which a tab stop may not line up; the conversion is meant for comma, semicolon, and `|`.
- Quick Open lists folder files once per opening; files created afterwards appear the next time it opens.
- Clipboard history records only copies and cuts made in Nulis, not text copied in other programs.

## Resume

Next: M30 Integration and quality.
