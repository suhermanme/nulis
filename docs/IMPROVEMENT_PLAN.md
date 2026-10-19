# Improvement plan: menu-reference feature audit (M19–M29)

**Status:** authoritative plan for M19–M29. Progress lives in `IMPLEMENTATION_TRACKER.md`; per-milestone handovers live in `docs/handover/`.
**Date of audit:** 2026-10-20, against commit `66853e1` (Nulis 0.1.1).
**Milestone numbering:** continues `ROADMAP.md` (M00–M18 already exist), so this plan starts at M19.

## 1. Goals and scope

Use a set of menu screenshots from a commercial reference text editor for macOS to discover editing, search, view, format, and column features that users of this product category expect, check each against the current Nulis implementation, and deliver the valuable missing ones in small, tested milestones.

The screenshots are a **feature-discovery aid only**. Per `CLEAN_ROOM.md`, Nulis does not copy the reference product's wording, layout, icons, shortcuts, or artwork. Features are restated below as product-neutral behavior. Menu paths are recorded only as evidence of where a capability was observed. Shortcut choices follow Nulis's existing bindings and platform conventions, not the reference product's.

Out of scope: rewriting existing subsystems, cloning the reference product's menu structure, and any feature that conflicts with Nulis's priorities (data integrity, bounded memory, security; see `PRODUCT_SPEC.md`).

## 2. Screenshot inventory and method

The user supplied 17 screenshots from `~/Desktop` (taken 2026-10-20 05:02–05:06) and one more pasted directly into the session. The screenshots are not stored in the repository.

| ID | File (Desktop) | Shows |
| --- | --- | --- |
| S01 | `Screenshot 2026-10-20 at 05.02.02.png` | File menu |
| S02 | `… 05.02.11.png` | File ▸ FTP submenu |
| S03 | `… 05.02.20.png` | File ▸ Sort submenu |
| S04 | `… 05.02.31.png` | File ▸ Print Setup submenu |
| S05 | `… 05.03.15.png` | Edit menu |
| S06 | `… 05.03.34.png` | Edit ▸ Select submenu |
| S07 | `… 05.03.43.png` | Edit ▸ Insert submenu |
| S08 | `… 05.03.54.png` | Edit ▸ Delete submenu |
| S09 | `… 05.04.00.png` | Edit ▸ HEX Functions submenu |
| S10 | `… 05.04.30.png` | Search menu |
| S11 | `… 05.04.45.png` | Project menu |
| S12 | `… 05.05.01.png` | View menu |
| S13 | `… 05.05.37.png` | View menu (duplicate of S12) |
| S14 | `… 05.05.49.png` | Format menu |
| S15 | `… 05.06.00.png` | Format ▸ Paragraph Formatting submenu |
| S16 | `… 05.06.14.png` | Column menu |
| S17 | `… 05.06.36.png` | Tools menu and Tools ▸ Macro submenu |
| S18 | (pasted, no file) | Right-click menu inside an open document |
| S19–S27 | (pasted, no file; added after M20) | The reference editor's Find and Replace dialog: Find, Find In Files, Replace, Replace In Files tabs; "Find where" menus; expanded Options; and the "Lines containing find string" list over an open document |

Not covered by any screenshot: the Window and Help menus, and submenus that were not expanded (Clipboards, Insert Template, Themes, Toolbars/Windows, Column Markers, Spacing, Hide/Show Lines, Live preview, Scripting, Shell, Encryption, Convert To, Recent lists, Add To Project, AutoFill). Their contents are **unclear** and are not guessed.

Method: every visible item was listed with its menu path and enabled/checked state. Each item was then traced in Nulis through the core command registry (`crates/nulis-core/src/lib.rs`), the document engine (`crates/nulis-core/src/editor.rs`), the native menus (`apps/desktop/src-tauri/src/lib.rs`), the web menus, toolbars, and keyboard handlers (`apps/desktop/src/App.tsx`, `EditorViewport.tsx`), and existing docs (`FEATURE_PARITY.md`, `WORK_STATE.md`). A feature counts as implemented only when it is reachable by a user, not merely present in the core.

Status values: **Done** (implemented and reachable), **Partial** (some behavior or access path missing), **Missing**, **Unclear** (reference behavior can't be established from the label alone), **N/A** (provided by the operating system or out of product scope).

## 3. Key findings

1. **Basic caret and editing keys are missing in the text editor.** `EditorViewport.tsx` `onKeyDown` handles arrows, Page Up/Down, Backspace, Enter, Tab, and Insert, but **not Home, End, Primary+Home/End, or forward Delete**. Primary+A falls through to the hidden textarea, so **Select All does not select the document**. These are the most important gaps: they affect every user every day.
2. **The editor has no right-click menu.** `main.tsx` suppresses the webview menu globally and only tabs, panes, and the explorer provide their own (S18 shows the expected category of actions: clipboard, find/replace, selection, hex mode, file path).
3. **No line operations:** select line (only triple-click), delete line, duplicate line, move line up/down, delete to line start/end.
4. **Text transforms are limited** to uppercase, lowercase, and trimming the ends of a selection. Missing: invert case, capitalize words, indent/outdent a selection, toggle comment, trim trailing spaces per line, tabs↔spaces, sort lines, Base64, insert date/time.
5. **Several core features have no menu path.** Find/replace, bookmarks, transforms, word wrap, encoding, and language are only in the editor toolbar; there is no native Edit-level, Search, or Format menu content beyond the platform defaults.
6. **Line endings are invisible.** There is no detection display, conversion, or show-whitespace/show-line-endings view.
7. **Navigation gaps:** no find next/previous when the find bar is closed, no jump to matching bracket (pairs are already computed), no back/forward caret history, no next/previous change (LCI data already exists), no Replace in Files, word count lacks words/lines.
8. Many larger reference features **already exist** in Nulis: compare, hex editor, column mode, macros, scripting, terminal, remote (SFTP read-only), themes, bookmarks, JSON/XML tools, document map, line change indicator, workspace splits.

## 4. Feature comparison

Complexity: S (≤1 day), M (2–4 days), L (≥1 week). Value: H/M/L. Rec. = recommendation and milestone.

### 4.1 File and project (S01–S04, S11)

| Feature (path) | Expected behavior | Nulis today (evidence) | Status | Cx | Value | Rec. |
| --- | --- | --- | --- | --- | --- | --- |
| New File (File) | New untitled document | `file.new`, Primary+N | Done | – | – | – |
| New Custom… | New document from a chosen template/type | none | Missing | M | L | Defer |
| New Window | Second top-level editor window | single main window, single-instance plugin | Missing | L | L | Defer (architecture) |
| Open… | Native open | `file.open`, Primary+O | Done | – | – | – |
| Quick Open… | Type a path/name to open a file | palette lists commands only; explorer filter | Partial | M | M | M28 |
| Close / Close All Files / Close All Except This | Close one/all/others with save prompts | `workbench.closeFile`; tab menu "Close All Files Except This"; no Close All | Partial | S | M | M27 |
| FTP ▸ Browser / Open from / Account Manager (S02) | Browse and open remote files, manage accounts | `remote.*` SFTP read-only, saved profiles, keychain | Partial | – | – | – |
| FTP ▸ Save to FTP | Write back to remote | read-only by design (`FEATURE_PARITY.md`) | Missing | L | M | Defer (security review) |
| Revert to Saved | Discard edits, reload from disk | none | Missing | S | M | M23 (Reload) |
| Save / Save As | Atomic save | `file.save`, `file.saveAs` | Done | – | – | – |
| Save All | Save every dirty document | none | Missing | S | M | M27 |
| Rename File | Rename the open file on disk, keep it open | none | Missing | M | M | M27 |
| Encryption ▸ | Encrypt/decrypt file | none | Missing | L | L | Defer (crypto scope) |
| Delete Active File (disabled in S01) | Delete file on disk | none | Missing | S | L | Defer (destructive) |
| Compare Files… | Side-by-side diff | `compare.documents`, `compare.folders` | Done | – | – | – |
| Sort ▸ File / All Files / Merge / Advanced (S03) | Sort lines of a document | none | Missing | M | M | M25 (sort lines/selection; multi-file merge deferred) |
| Print… / Print All / Page & Print Setup (S04) | Print with page setup, printer font | none | Missing | L | L | Defer |
| Favorite Files… | Pinned file list | none | Missing | S | L | Defer |
| Convert To ▸ (not expanded) | Likely encoding/line-ending conversion | encoding override UTF-8/Windows-1252 only | Unclear | – | – | Covered by M27 EOL work |
| Recent Files / Recent Remote Files / Recent Projects | Most-recently-used lists | none | Missing | S | M | M27 (local recent files) |
| Project ▸ New/Open/Close Project/Workspace (S11) | Named project with file set | `workspace.open` opens a folder; sessions persist | Partial | – | – | Keep folder model |
| Project Settings, Add To Project, Backup/Restore | Project file management | none | Missing | L | L | Defer |
| Ctag Options / Create CTAG file | Symbol index across project | per-file symbols (JSON/Python) only | Missing | L | L | Defer |

### 4.2 Editing, clipboard, selection (S05–S09, S18)

| Feature (path) | Expected behavior | Nulis today (evidence) | Status | Cx | Value | Rec. |
| --- | --- | --- | --- | --- | --- | --- |
| Undo / Redo | History | `editor.undo/redo`, Primary+Z / Shift+Primary+Z | Done | – | – | – |
| Cut / Copy / Paste | Clipboard | viewport `onCopy/onCut/onPaste`, multi-selection aware | Done | – | – | – |
| Paste & Copy, Cut & Append, Copy & Append | Swap clipboard with selection; append to clipboard | none | Missing | S | L | M28 |
| Copy File Path/Name | Copy path of active file | tab and pane context menus | Partial (not in editor menu) | S | M | M20 (editor context menu) |
| Clipboards ▸ (not expanded) | Multiple clipboards | none | Unclear | M | L | M28 (clipboard history) |
| Select ▸ Select All | Select whole document | **Primary+A does nothing useful** | Missing | S | H | **M20** |
| Select ▸ Select Line | Select current line(s) | triple-click only | Partial | S | H | **M20** |
| Select ▸ Select Word | Word under caret | `editor.selectWordAt`, Primary+D | Done | – | – | menu path in M20 |
| Select ▸ Select Range… | Select between two positions | none | Missing | S | L | M26 (with Go To line:column) |
| Insert ▸ Date/Time | Insert timestamp | none | Missing | S | M | M25 |
| Insert ▸ Insert color | Color picker → hex text | none | Missing | S | L | Defer |
| Insert ▸ Line | New line below without splitting | none | Missing | S | M | M20 |
| Insert Template ▸ | User text templates | none | Unclear | M | L | Defer |
| Delete ▸ Delete (forward) | Delete next character | **forward Delete not handled** | Missing | S | H | **M20** |
| Delete ▸ Delete Line | Remove current line(s) | none | Missing | S | H | **M20** |
| Delete ▸ Delete to Start/End of Line | Delete from caret to line start/end | none | Missing | S | M | **M20** |
| Move Line Up / Down | Swap line(s) with neighbor | none | Missing | M | H | **M20** |
| Duplicate Line | Copy line(s) below | none | Missing | S | H | **M20** |
| HEX ▸ Hex Edit (S09) | Toggle hex view | `workbench.toggleEditorMode` | Done | – | – | context menu in M20 |
| HEX ▸ Hex Insert/Delete…, Hex Copy Selected View | Insert/delete N bytes; copy hex dump text | nibble typing and backward delete only | Partial | M | L | Defer |
| Spell Check | Check spelling | none | Missing | L | L | Defer |
| Read Only | Toggle read-only | `editor.toggleReadOnly`, Primary+Shift+L | Done | – | – | – |
| Encode/Decode Base64 | Transform selection | none | Missing | S | M | M25 |
| Comment Add/Remove, Comment/Uncomment Selection | Line comments using language syntax | lexer knows comment delimiters, no command | Missing | M | H | M25 |
| AutoFill, Start Dictation, Emoji & Symbols | macOS-provided Edit menu items | supplied by the OS default Edit menu | N/A | – | – | – |
| Editor right-click menu (S18) | Clipboard, find/replace, selection, hex, path actions | **none in the text area** | Missing | M | H | **M20** |
| S18 "Change open file path / Places to this file path" | Point Open dialog / file browser at the file's folder | Places tab exists, no "reveal" | Missing | S | M | M27 |
| S18 "File Tabs ▸", "Customize Menu…" | Tab actions; edit the context menu | tab menu exists; menus customizable for File only | Partial | M | L | Defer customizing |

### 4.3 Search and navigation (S10)

| Feature | Expected behavior | Nulis today | Status | Cx | Value | Rec. |
| --- | --- | --- | --- | --- | --- | --- |
| Find… / Quick Find | Find bar | find bar, Primary+F | Done (moves to the M21 panel) | – | – | M21 |
| Find Next / Previous | Repeat last search without the bar open | Enter/Shift+Enter only inside the bar | Partial | S | H | M21 (panel buttons); M26 (with panel closed) |
| Replace… | Replace bar | same bar, Primary+H | Done | – | – | – |
| Incremental Search | Search as you type | search runs on Enter | Partial | S | M | M21 |
| Find In Files… | Search a folder | `search.findInFiles` panel | Done | – | – | – |
| Replace In Files… | Replace across files | none | Missing | L | M | M22 (with preview and confirmation) |
| Match Brace | Jump to matching bracket | pairs rendered (`nulis-syntax`), no jump | Partial | S | H | M26 |
| Goto… | Line (and column) | Go to line bar, Primary+G | Partial (no column) | S | M | M26 |
| Toggle / Previous / Next / Clear Bookmarks | Bookmarks | `editor.toggleBookmark`, `moveToBookmark`, `clearBookmarks`, F2 keys | Done | – | – | – |
| Edit Bookmarks… | List and manage bookmarks | none | Missing | S | M | M26 |
| Delete All Bookmarked Lines | Remove marked lines | none | Missing | S | M | M26 |
| Last change / Next change (disabled in S10) | Jump between edited lines | LCI ranges exist in `line_changes.rs` | Missing | S | M | M26 |
| Word Count | Words, lines, characters | status bar bytes + characters only | Partial | S | M | M26 |
| Find Symbol | Jump to a symbol | symbols dropdown (JSON/Python only) | Partial | M | M | M26 (palette-style filter) |
| Character Properties | Show code point, name, bytes of caret char | none | Missing | S | L | M26 (status line, small) |
| Back / Forward (last position) | Caret navigation history | none | Missing | M | H | M26 |

### 4.4 View, layout, display (S12–S13)

| Feature | Expected behavior | Nulis today | Status | Cx | Value | Rec. |
| --- | --- | --- | --- | --- | --- | --- |
| Themes ▸ | Pick theme | 99 built-in themes, user themes | Done | – | – | – |
| Toolbars / Windows ▸ | Show/hide panels | side bar toggle, toolbar customization | Partial | – | – | – |
| View As (Highlighting File Type) | Language override | "View As" picker, `language.set` | Done | – | – | – |
| View As (File Encoding) | Decode as encoding | UTF-8 / Windows-1252 only | Partial | L | M | Defer more encodings |
| Set Font | Editor font | Settings ▸ Appearance | Done | – | – | – |
| Increase / Decrease Font Size | Zoom editor text | settings only, no shortcut | Partial | S | H | M27 |
| Toggle Word Wrap | Soft wrap | toolbar toggle, not persisted, no menu/shortcut | Partial | S | M | M27 |
| Split Horizontally / Vertically | Two views | `workbench.splitRight/Down` | Done | – | – | – |
| Duplicate Window | Same document in a new window | split view of same document | Partial | – | – | Keep splits |
| Highlight All Selected | Highlight occurrences of selection | find bar "Highlight matches" | Partial | S | M | M21 (highlight all found) |
| Display Ruler | Column ruler | none | Missing | M | L | M27 |
| Display Line Numbers | Toggle gutter numbers | always shown | Partial | S | L | M27 |
| Document Map | Minimap | document map | Done | – | – | – |
| Line Change Indicator | Edited-line gutter | LCI (M16) | Done | – | – | – |
| Show Spaces/Tabs, Show Line Endings | Visible whitespace | none | Missing | M | M | M27 |
| Show Page Breaks as Lines | Render form-feed as a rule | none | Missing | S | L | Defer |
| Column Markers ▸ | Vertical guides at columns | none | Missing | S | M | M27 |
| Spacing ▸ (not expanded) | Probably line spacing | line height in Settings | Unclear | – | – | – |
| Hide/Show Lines ▸ (not expanded) | Probably hide line ranges | folding exists | Unclear | – | – | – |
| Toggle Browser View / Live preview | Rendered preview | Markdown View mode; HTML preview pending | Partial | M | M | Defer (tracked in parity) |
| Always On Top | Keep window above others | none | Missing | S | L | M27 |

### 4.5 Format (S14–S15)

| Feature | Expected behavior | Nulis today | Status | Cx | Value | Rec. |
| --- | --- | --- | --- | --- | --- | --- |
| Reformat Paragraph, Paragraph ▸ Left/Right/Center Align, Fill | Reflow and align text | none | Missing | M | L | Defer |
| Convert Line Terminators to Wrap / Wrap to Terminators | Join hard-wrapped lines / hard-wrap at width | none | Missing | M | L | Defer |
| Tabs to Spaces, Spaces to Tabs (All / Leading) | Whitespace conversion | none | Missing | S | M | M25 |
| Trim Trailing Spaces | Per-line trailing whitespace | "Trim whitespace" trims selection ends only | Partial | S | H | M25 |
| To Upper / Lower Case | Case | `transformSelections` | Done | – | – | menu path M25 |
| Capitalize, Invert Case | Case | none | Missing | S | M | M25 |
| Add / Remove Indents | Indent selected lines | Tab inserts a tab character | Missing | S | H | M25 |
| HTML Tidy, XMLlint Tool | External tools | none | Missing | L | L | Defer |
| XML Convert to Line Terminators | Pretty-print XML | `tools.beautify` for XML | Done | – | – | – |
| Reformat / Compress JSON | Pretty-print / minify | `tools.beautify`, `tools.minify` | Done | – | – | – |
| ReIndent Selection | Language-aware re-indent | none | Missing | L | L | Defer |

### 4.6 Column (S16)

| Feature | Expected behavior | Nulis today | Status | Cx | Value | Rec. |
| --- | --- | --- | --- | --- | --- | --- |
| Column Mode | Rectangular selection mode | Alt+Shift drag, toolbar Column Editing | Done | – | – | – |
| Insert/Fill Columns, Delete Columns, Cut Columns | Edit a rectangle | typing, delete, cut work on rectangles | Done | – | – | – |
| Insert Number… | Number sequence down a column | none | Missing | S | M | M28 |
| Sum Columns/Selection | Add numbers in a rectangle | none | Missing | S | L | M28 |
| Convert to Fixed Column / Character Delimited | CSV ↔ fixed width | none | Missing | M | M | M28 |
| Left / Center / Right Justify | Align text in a rectangle | none | Missing | S | L | M28 |

### 4.7 Tools (S17)

| Feature | Expected behavior | Nulis today | Status | Cx | Value | Rec. |
| --- | --- | --- | --- | --- | --- | --- |
| Macro ▸ Record / Stop / Play / Play multiple / Load / Save / Edit | Macro lifecycle | `MacroPanel.tsx`, `nulis-automation` (bounded, allowlisted) | Done (repeat-count unverified) | – | – | Verify in M29 |
| Macro ▸ Set Auto Load, Macro for File Load/Save | Run macros on events | none | Missing | M | L | Defer |
| Scripting ▸, Shell ▸ (not expanded) | Scripts; shell | sandboxed scripts; integrated terminal | Done (contents unclear) | – | – | – |
| Tool Configuration…, Project Tool Configuration… | User-defined external commands | none | Missing | L | M | Defer (needs security design) |
| Display/Modify Templates… | Text templates | none | Missing | M | L | Defer |

### 4.8 Find and Replace dialog (S19–S27, user request)

| Feature | Expected behavior | Nulis today | Status | Rec. |
| --- | --- | --- | --- | --- |
| Floating Find and Replace window with four tabs | One movable panel over the editor | inline find bar in each editor + Search side-bar panel | Partial | M21 (Replace in Files tab M22) |
| Find where: current document / selected text / all open files | Scope of Find and Replace | current document only | Partial | M21 |
| Multi-line "Find what" / "Replace with" fields, history | Recall earlier searches | single-line inputs, no history | Partial | M21 (history); favorites deferred |
| Match case, regular expressions | Search options | yes | Done | – |
| Match whole word | Word-boundary search | none | Missing | M21 |
| Highlight all items found | Mark every match | "Highlight matches" toggle | Done | moves to M21 panel |
| List lines containing find string | List of matching lines; click goes to the line; Bookmark All, Copy, Show Line Numbers | none | Missing | M21 |
| Count All | Number of matches | none | Missing | M21 |
| Close after find / after replace all | Close the panel after the action | none | Missing | M21 |
| Find in Files: files/types filter, directory, include subdirectories, all open files | Folder search with filters | folder only, always recursive, results show byte offsets | Partial | M21 |
| Search in column From/To, Preserve case, Replace All from top, regex dialect choice, Ignore options | Advanced options | none | Missing | Deferred (low value or unclear) |

## 5. Prioritized recommendations

1. **Essential (M20):** standard caret keys, forward delete, Select All/Line, line operations, and an editor right-click menu. Smallest effort, highest daily impact, no new dependencies.
2. **High (M21–M22):** the Find and Replace window and Replace in Files (user request).
3. **High (M25):** transforms and formatting commands, exposed through a Format/Edit menu and the palette.
4. **High (M26):** search and navigation (find next/previous anywhere, match brace, back/forward, change navigation, word count).
5. **Medium (M27):** view and file conveniences (line endings, zoom, visible whitespace, ruler/markers, Revert, Save All, Close All, Recent Files, Rename).
6. **Medium/low (M28):** column data tools, clipboard extras, Quick Open.
7. **M29:** integration, menus/shortcut audit, accessibility, docs, release check.

## 6. Architecture and design

- **Core stays authoritative.** Every document change is a revision-checked core command in `nulis-core` (`editor.rs` + registration in `lib.rs`), so macros, scripts, plugins, menus, and keys share it. New edits go through `replace_many`, relocate bookmarks, and record line changes exactly like existing transforms.
- **Bounded work.** Line operations read only the touched lines with existing helpers (`find_line_start`, `find_line_boundary`, `previous_line_range`) and reject edits above `MAX_EDIT_BYTES`. Smart Home reads at most a fixed prefix of the line.
- **One command, one place.** New caret targets extend `editor.moveHorizontal`'s `HorizontalDirection` (`lineStart`, `lineEnd`, `documentStart`, `documentEnd`) instead of new commands. New edits are `editor.deleteForward`, `editor.selectAll`, and `editor.lineOperation` with an operation enum.
- **IPC.** DTO changes bump `IPC_PROTOCOL_VERSION` in `crates/nulis-core/src/lib.rs` and `apps/desktop/src/contracts.ts` together.
- **Macros.** Recordable editor edits are added to `is_recordable_command` so macros can replay them.
- **Context menu.** The editor reuses the existing `ContextMenu` component from `Toolbar.tsx`; the global `contextmenu` suppression in `main.tsx` stays.
- **Menus.** Later milestones add native Edit/Search/Format items through the same `append_commands` mechanism used for View/Tools/Workspace in the adapter.

## 7. Risks and compatibility

| Risk | Mitigation |
| --- | --- |
| Shortcut collisions (e.g. Primary+D is Select Word, Primary+G is Go To) | Keep existing bindings; pick new ones from free platform-conventional chords; document every new shortcut |
| Line operations on huge lines | Size check against `MAX_EDIT_BYTES` before reading; return a clear error |
| Multi-caret/rectangle semantics | Line operations act on the union of lines touched by all selections; rectangles are converted to their rows |
| Read-only documents | All edits go through `replace_many`, which already refuses read-only documents |
| IPC version drift | Version bumped in both mirrors, checked at startup |
| Windows/Linux behavior can't be tested on this Mac | Record as pending in the tracker; hosted CI and manual checks before closing milestones |

## 8. Testing and acceptance

Each milestone adds core unit tests in `crates/nulis-core/src/editor.rs` tests (bytes, revisions, undo as one step, CRLF/LF/CR, last line without newline, multi-caret, read-only refusal), registry tests where commands are added, and Vitest tests in `apps/desktop/src/*.test.tsx` for keys and menus. Every milestone runs: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `npm run lint`, `npm run typecheck`, `npm test`.

## 9. Milestone roadmap

| Milestone | Scope | Depends on | Cx |
| --- | --- | --- | --- |
| **M19** Audit and foundation | This plan, tracker, handover; baseline tests; plus the user-reported Windows Settings/Help window bug | – | S |
| **M20** Core editing keys, line operations, editor context menu | Home/End (smart Home), Primary+Home/End, Shift variants; forward Delete; Select All; Select Line; Delete Line; Duplicate Line; Move Line Up/Down; Delete to line start/end; Insert line below; right-click menu (Undo, Redo, Cut, Copy, Paste, Find, Replace, Select All/Line/Word, Hex mode, Copy File Path) | M19 | M |
| **M21** Find and Replace window | User request (2026-10-20, 9 screenshots): one floating, movable Find and Replace panel with Find / Find in Files / Replace / Replace in Files tabs, replacing the side-bar Search panel and the in-editor find bar. Find where: current document, selection, all open files. Options: match case, whole word, regex, highlight all, list matching lines, close after find. Count All, Previous, Next, Replace, Replace All. Find in Files: file-type filter, directory, subdirectories toggle, results with line numbers that open at the line. A "Lines containing" list that goes to the clicked line, with Bookmark All and Copy. Search history | M20 | L |
| **M22** Replace in Files | Replace across a folder with a preview list and confirmation; open documents edited in memory, closed files saved atomically; per-file failure report | M21 | M |
| **M23** External changes and Reload | User request: when another program changes or deletes an open file, notice it (file watcher plus a check when the window regains focus), and ask: Reload (discard edits only after confirmation if dirty) or Keep mine. A File ▸ Reload command and toolbar/context action reload on demand (this is also Revert to Saved). Reload keeps the caret line where possible and is a recoverable step | M20 | M |
| **M24** New application icon | User request: a modern original logo that fits the theme colors; SVG master, macOS/Windows/Linux icon sets regenerated through `tauri icon`, in-app About/title use, light and dark checks | – | S |
| **M25** Format and transforms | Invert case, capitalize, indent/outdent (Tab/Shift+Tab on selections), toggle line comment, trim trailing spaces, tabs↔spaces, sort lines (asc/desc, unique, case options), Base64 encode/decode, insert date/time; Format menu | M20 | M |
| **M26** Search and navigation | Find next/previous with the panel closed, match brace, Go to line:column and select range, back/forward history, next/previous change, bookmark list and delete-bookmarked-lines, word/line count, character info | M21 | M |
| **M27** View and file | Line-ending detection, status display, conversion; font zoom; persisted word wrap; show whitespace/line endings; column markers/ruler; line-number toggle; Revert, Save All, Close All, Recent Files, Rename File, reveal in Places; Always on Top | M20 | L |
| **M28** Column and clipboard tools | Insert number sequence, sum, justify in rectangle; CSV↔fixed width; copy/cut append, paste-and-copy, clipboard history; Quick Open | M25 | M |
| **M29** Integration and quality | Palette and native-menu entries for M20–M28 commands, shortcut audit, accessibility, user guide, parity doc, hosted OS matrix | M20–M28 | M |

**Re-plan 2 (2026-10-20):** two more user requests arrived during M21: external file change detection with Reload (M23, also covers Revert to Saved) and a new app icon (M24). Later milestones were renumbered again. The user also asked that the reference product's name be removed from code and documents; it now appears nowhere in the working tree.

**Re-plan (2026-10-20):** after M20 the user asked for a floating Find and Replace window (screenshots of the reference editor's dialog and its "lines containing" list). It became M21 and Replace in Files was split out as M22 because it writes files on disk and needs its own safety design. Later milestones were renumbered.

Each milestone must define scope/exclusions, tasks, files, tests, acceptance criteria, and rollback before it starts; M20's definition is in `IMPLEMENTATION_TRACKER.md`.

## 10. Deferred and out of scope

Writing to remote servers, file encryption, printing and page setup, spell check, project files with backup/restore, ctags, HTML Tidy/XMLlint external tools, user-defined external tools, templates, paragraph reflow/alignment, hard-wrap conversion, page-break rendering, macro auto-load and file-event macros, additional encodings, multiple top-level editor windows, deleting files from the editor, favorites, color insertion, hex insert-N-bytes dialog and hex-dump copy, context menu customization. OS-provided items (AutoFill, Dictation, Emoji & Symbols) are out of scope.

These can be revisited after M27; each needs its own design note, and security-sensitive ones (remote writes, external tools, encryption) need a review against `SECURITY.md`.
