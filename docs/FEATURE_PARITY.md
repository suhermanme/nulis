# Capability status

Legend: `[ ]` not started, `[~]` partial, `[x]` complete. “Complete” requires working implementation and appropriate tests.

## Foundation

- [~] Cross-platform desktop shell (CI builds, installs, and starts it on macOS, Windows, and Linux on every push; Windows checked by hand on a Windows 11 machine; Linux not yet tried by hand)
- [x] Headless Rust command registry
- [x] Layered settings merge foundation
- [~] Strongly typed IPC (versioned mirrored DTOs; generation pending)
- [x] CI (formatting, Clippy, and tests on macOS, Windows, and Linux; bundles installed and started on each; document fuzzing, a large-file benchmark, and dependency advisories; tagged releases publish every package with checksums)

## Documents and editing

- [x] Disk-backed document model and balanced piece structure (native open, visible atomic Save/Save As, disk-backed sessions, and guarded unsaved-close decisions)
- [x] Streaming safe save (same-directory synced output, concurrent-change detection, cross-platform replacement path)
- [x] Bounded undo/redo
- [x] Virtualized editor viewport (bounded snapshots, virtual rows, line numbers, sparse-checkpoint random navigation, off-screen caret reveal, and cross-page drag paging are tested)
- [x] Clipboard editing (bounded multi-selection copy/cut uses newline-joined core ranges and native text-write permission)
- [~] Edit and Read-Only modes (per-document toggle enforced in `nulis-document` for every edit, undo, and redo; status bar badge, toolbar button, tab and pane lock, block caret, command, View menu item, and Primary+Shift+L; the mode is not persisted across sessions)
- [x] Status bar (caret line, column, and code point; size in bytes with character, word, and line counts up to 64 MiB; zoom; encoding; line endings with conversion)
- [~] Insert/overwrite modes (Unicode-aware overwrite and UI toggle work; broader platform keyboard validation pending)
- [~] Multiple documents and workspace tabs (one tab strip; dragging a tab into the content groups it into a named workspace of nested panes, with drop preview, pane pop-out, keyboard commands, bounded limits, and schema-v2 sessions; hosted platform and screen-reader validation pending)
- [~] Precise pointer editing (core Unicode word selection for double-click and Primary+D, logical-line triple-click, and revision-checked drag move/copy as one undo step; manual pointer/IME checks on Windows and Linux pending)
- [~] Line change indicators (sparse unsaved/saved/deletion markers through save, undo/redo, and recovery, with map ticks and a persistent toggle; theme-specific marker colors and hosted validation pending)
- [x] Column and clipboard tools (M29: Insert Number Sequence, exact Sum, left/center/right justify in a column block, delimited ↔ fixed-width columns; Copy/Cut and Append, Paste and Copy, session clipboard history; Quick Open over open, folder, and recent files)
- [x] File conveniences (M28: Save All, Close All Files, Open Recent with a searchable picker and Clear Recent Files, Rename File in place with unsaved edits kept, Show in Places)
- [x] View features (M27: line-ending detection in the status bar, conversion to LF/CRLF/CR as one undo step, Enter keeps the line's ending; remembered word wrap with Alt Z; editor zoom with Ctrl/⌘ = - 0; show whitespace and line endings; line-number toggle; column marker at 80; Always on Top; View menu and palette)
- [x] Search and navigation (M26: F3 find next/previous, matching bracket, Go to line:column, back/forward history, next/previous change, bookmark list and Delete Bookmarked Lines, word and line counts, code point at the caret; native Search menu)
- [x] Formatting (M25: invert case, capitalize, trim, indent/outdent with Tab/Shift+Tab, language-aware toggle comment, trim trailing spaces, tabs↔spaces, sort ascending/descending/unique, Base64 encode/decode, insert date/time; Format menu, toolbar list, and palette; tab width fixed at 4)
- [x] Caret keys, forward delete, Select All, and line commands (smart Home/End, document start/end, Delete, select/delete/duplicate/move line, delete to line start/end, open line below; one undo step each, multi-caret aware) with an editor right-click menu (M20), in the palette and the native Edit menu
- [x] Multi-caret and multi-selection (bounded core fan-out, per-caret navigation, Alt-click creation, rendering, clipboard, and one-action undo/redo)
- [x] Column/block editing (constant-size core rectangle, bounded line-local operations, virtual-space insert, row-distributed paste, and Alt-Shift drag)
- [~] Encoding detection and conversion (lossless UTF-8 and Windows-1252 override/editing controls with source-byte mappings; automatic detection and broader conversion remain)
- [~] File change detection and recovery (fingerprints, dirty-close decisions, durable local/untitled checkpoints, and startup recovery choices work; M23: open files changed or removed by other programs are detected on focus and every 3 s with Reload / Keep Mine prompts, and File ▸ Reload from Disk; versions cannot be merged, and remote documents are not checked)

## Search and languages

- [x] Streaming find and replace (64 KiB reads, bounded results, wrap navigation, and atomic Replace All)
- [x] Regular expressions and multiline search (M32: one linear-time style; unbounded patterns such as `\w+` with a reported 1 MiB match cap; capture groups in Replace; lookaround and backreferences refused with a named message; Search in column; highlights from the core)
- [x] Find in files (regular-file scan with symlink exclusion, name filters, optional subfolders, line numbers and line text per match, and hard file/result caps)
- [~] Find and Replace window (M21: floating Find / Find in Files / Replace tabs; current document, selection, or all open files; whole word; Count All without a match limit, as a background job with progress and Cancel; matching-lines list with Go To, Bookmark All, Copy; history; M31–M32: the window fits its content, options are remembered, and a help button opens the regex section of Nulis Help) in its own windows
- [x] Replace in Files (M22: preview with match counts, explicit confirmation, open documents changed in memory, closed files saved atomically only if unchanged since the preview, per-file report)
- [x] Modular syntax highlighting and folding (bounded JSON/Python Tree-sitter modules, a Markdown scanner, and table-driven lexers for 40 more languages including JSON with Comments, TSX, Ignore File, C/C++, C#, Java, JavaScript/TypeScript, Rust, Swift, Zig, Go, PHP, Perl, Ruby, shell, Batch, PowerShell, SQL dialects, Visual Basic, HTML/XML/XSL/plist, CSS, CSV/TSV (a color per column), and YAML (anchors, tags, block text); binary property lists open read-only in the hex editor; stable token classes, bracket pairs, and brace/element folding)
- [x] Symbols, language detection, and bracket matching (file-name and extension detection, `#!`/XML/PHP/HTML content detection for names that say nothing, override command/control, bounded symbols, and paired-bracket rendering)
- [x] Language catalog and capability registry (owned language modules with source-labelled associations and first-line patterns; user `files.associations`; conflict reporting; per-language capability switches and providers resolved by kind and origin)
- [x] Automatic indentation (M44: Enter follows brace, Python, YAML, Makefile, markup, and keyword blocks; paired delimiters split; closers and `end`/`fi` outdent; Tab and Backspace follow detected indentation; per-language switches)
- [x] Suggestions while typing (M45: document words, declared keywords and types, and snippets with placeholders, ranked and bounded; Ctrl+Space and per-language switches)
- [x] Optional language servers (M47: configured and approved commands only, one process per project folder, start/request timeouts, cancel, restart, UTF-8/UTF-16 positions, Problems panel and gutter marks; manual `rust-analyzer` and `clangd` checks on the three platforms pending)
- [x] Declarative language plugins (API v2: highlighted, comments, indentation, completion, snippets, associations, and first-line detection supplied as manifest data with no code or process; per-plugin and per-family switches; Settings conflict choices; `examples/plugins/nulis-nim`)

## Specialized tools

- [x] JSON, XML, YAML, and plist tools (validate with caret placement on the first problem, beautify, and JSON minify, shown only for those documents; XML checking is well-formedness, not DTD or schema validation; YAML catches repeated keys, and Beautify asks before losing comments or anchors and refuses tags; plist checks structure and value types)
- [x] Scalable hexadecimal editor (bounded raw-byte viewport, 64-bit paging, synchronized hex/ASCII selection, byte-native editing, and deep sparse-file regressions)
- [x] Text/binary compare and merge (bounded line-window and raw-byte pages, bidirectional revision-checked hunk application, and content-aware folder comparison)
- [x] Workspace/file explorer (File View with Open, Places, and Folder tabs; Places browses the computer and mounted volumes with lazy bounded listings, a file-name filter, and optional hidden files; the opened folder keeps its canonical root and symlink rule; files open through the central command)
- [x] Integrated terminal (native PTY sessions, bounded output rings, direct argument arrays, resize/input/lifecycle commands, and xterm.js rendering)
- [~] Print, Page Setup, and Export to PDF (M36–M39: pages laid out once in the core for both; Print Preview window and the system print dialog; streaming PDF export with progress, Cancel, and an outline; selection and hex dump export; formatted Markdown printing; Page Setup with paper, margins, font size, wrap or cut, colors, line numbers, header and footer, and a live preview; installed fonts for missing characters; checked by hand on macOS, Windows and Linux pending; no text shaping)
- [~] Restricted Markdown/HTML preview (Markdown Edit/View mode with safe rendering, GitHub-style syntax, and bounded local images; separate HTML preview remains)

## Extensibility and customization

- [x] Central commands (every command is in the searchable palette, and the native File, Edit, View, Search, and Format menus carry the M20–M29 commands; `commands.test.ts` fails the build when the core, the desktop, and the menus disagree)
- [x] Plugin architecture (validated packages, isolated JavaScript workers, digest-bound approval, activation events, command contributions, manager UI, and versioned SDK; API v2 adds declarative languages with no code path, plus per-plugin and per-capability switches bound to the same approval digest)
- [x] Themes (complete semantic tokens, strict built-ins, bounded user-theme install/discovery/uninstall, persisted live switching, and contrast floors for selection, secondary text, and icons applied to every theme)
- [x] Macro recording and sandboxed scripting (bounded macros plus explicitly approved, capability-gated scripts)
- [~] Searchable settings and migrations (sectioned settings with per-section filters over schema-v1 customization storage and reset; workbench sessions migrate from schema v1 to v2; customization migration tooling for a future schema revision remains)
- [x] Configurable shortcuts, menus, and toolbars (portable chords drive web/native menus and command dispatch)
- [~] Icon toolbars and separate Settings window (one bundled vector icon set with accessible names, tooltips, pressed states, and an overflow menu; a singleton sectioned Settings window with revision-checked apply, a narrow command allowlist, and frame restoration; on Windows the secondary windows have no menu bar, open only once drawn, and Find, results, and Help reopen at once; checked by hand on Windows 11, Linux not yet)

## Documentation and licensing

- [~] User guide and Help window (`docs/USER_GUIDE.md` and `docs/PLUGIN_GUIDE.md`, bundled and shown under Help ▸ Nulis Help and F1 in a separate sectioned window with search; native-window behavior not yet exercised by hand)
- [x] Plugin developer guide and examples (`docs/PLUGIN_GUIDE.md`; `hello-nulis`, the TypeScript `nulis-tips`, and the declarative `nulis-nim` example are loaded and checked under test)
- [x] About page (icon, version, origin of the name, copyright, GPL version 3 notice, and the full license text, over a dark or light batik background that follows the theme)
- [x] Licensing (GPL-3.0-or-later declared in every manifest with the license text in `LICENSE`; every license in the locked dependency graph is compatible, as recorded in `THIRD_PARTY_LICENSES.md`)

## Production readiness

- [~] Accessibility validation and platform conventions (keyboard-only editor input, screen-reader status/instructions, reduced motion, and forced colors implemented; dialogs keep Tab inside and pickers and dialogs return focus when closed; Ctrl Y redo on Windows and Linux; manual screen-reader validation with VoiceOver, NVDA, and Orca pending)
- [~] Performance corpus and bounded-memory measurements (repeatable sparse generator and 10 GiB M01 baseline; full corpus pending)
- [~] Fuzzing and property-based document tests (generated model tests and a libFuzzer target work; hosted sanitizer run pending)
- [~] Installers, signing, and notarization (CI builds, installs, and starts the macOS `.dmg`, Windows NSIS installer, and Linux `.deb`/`.rpm` on hosted runners, and publishes them for tagged releases; the macOS app is ad-hoc signed; Developer ID signing, notarization, and Windows code signing pending)
