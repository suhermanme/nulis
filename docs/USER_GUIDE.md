# Nulis User Guide

## Getting Started

Nulis is a desktop editor for text, source code, binary data, and files larger than memory. This guide explains how to use it. It is also available inside the application under **Help ▸ Nulis Help** (F1), where each chapter is a section in the side bar.

*Nulis* is a Javanese word that means "writing" or "to write".

In this guide, **Ctrl/⌘** means the Control key on Windows and Linux and the Command key on macOS.

### The window at a glance

From top to bottom, the window has these parts.

| Part | What it is for |
| --- | --- |
| Title bar | The **Nulis** brand (click it to open the About page), the main toolbar, and the **Commands** button at the far right. |
| Main toolbar | Icon buttons with a caption under each: New, Open, Save, Close, Undo, Redo, Split Right, Split Down, Files, Automation, Plugins, and Settings. Validate, Beautify, and Minify appear here while a document that has them is focused (JSON, XML, YAML, property lists). What does not fit moves into the **More** control: its menu names each action that left, with its shortcut, and ends with **Customize Toolbar…**, which opens Settings on the toolbar editor. The last controls to leave are the file buttons and **Settings**. |
| Side bar | File View. Show or hide it with the **Files** button in the toolbar, or View ▸ Files Panel. Drag its edge to resize it. |
| Tab strip | One tab per open file or workspace, the **+** button for a new tab, and on the right the buttons for split view, document comparison, folder comparison, the terminal, and the hexadecimal editor. |
| Editor toolbar | Tools for the focused document: find, Go To, bookmarks, Transform, encoding, View As, word wrap, column editing, folding, line change indicators, the document map, the **EDIT** / **READ ONLY** mode button, and the insert/overwrite indicator. |
| Editor | The text (or hexadecimal) view, with line numbers and the change-indicator lane in the gutter. |
| Status bar | Messages about the last action on the left. On the right: the **EDIT** / **READ ONLY** mode indicator, the caret's line and column and the code point after it, the document's size and counts, the zoom when it is not 100%, the version, the encoding, and the line endings. See *The status bar* below. |

Every icon button has a tooltip that names its action and its shortcut. Where the word under an icon is shorter than the action itself, the tooltip gives both, such as **Compare Documents — Compare**.

#### Menus

On the desktop, Nulis uses the operating system's menu bar: **File**, **Edit**, **View**, **Search**, **Format**, **Tools**, **Workspace**, **Window**, and **Help**. On Windows and Linux the menu bar belongs to the editor window only; Settings, Find and Replace, and Help have none. On macOS, **Settings…** and **About Nulis** are also in the **Nulis** application menu.

#### Command palette

Press **Ctrl/⌘ K** or click **Commands** to open the command palette. Type part of a command name and press Enter. Every command in the menus and toolbars can be run from here, so you never have to remember where a command lives.

#### The status bar

The bar along the bottom of the window describes the focused document.

| Part | Shows |
| --- | --- |
| Message (left) | The result of the last action, or why something did not happen. |
| **EDIT** / **READ ONLY** | The document's mode, shown as an indicator. Green **EDIT** means it can be changed; amber **READ ONLY** with a lock means it cannot. The chip is not a control: switch the mode with the editor toolbar's mode button, **View ▸ Read-Only Mode**, the command palette, or **Ctrl/⌘ Shift L**. |
| **Ln**, **Col** | The line and column of the caret, updated as it moves. With several carets it adds how many; with a selection it adds how many bytes are selected. It is not shown in the hexadecimal view. |
| Size | The document's size in bytes and its number of characters, for example `1,234 bytes · 1,200 characters`. The two differ when the text has characters that take more than one byte. |
| Zoom | The editor zoom, such as `125%`, shown only when it is not 100%. Click it to reset. |
| Version, encoding | The Nulis version and the document's text encoding. |
| Line endings | **LF**, **CRLF**, **CR**, or **Mixed**. Click it to convert the document's line endings (see [Line endings](#line-endings)). |

The character count follows your edits a moment behind. It counts characters in the document's encoding: in UTF-8 an accented letter or an emoji is one character even though it takes several bytes, and a line break is one character (two for a Windows line ending). Characters are counted for documents up to 64 MiB; a larger document shows its size in bytes only.

#### Welcome screen

When no file is open at startup, Nulis shows a welcome screen with buttons for a new text file, opening a file, and opening a folder. Tick **Do not show this on startup** to turn it off. The same option is under **Settings ▸ General ▸ Startup**. When the welcome screen is off, Nulis starts with a new empty document unless files are restored from the previous session.

## Files

### Opening, creating, saving, and closing files

| Action | How |
| --- | --- |
| New file | **Ctrl/⌘ N**, the **New** toolbar button, the **+** button in the tab strip, or **File ▸ New Text File**. |
| Open a file | **Ctrl/⌘ O**, the **Open** toolbar button, or **File ▸ Open File…**. You can also drag files from the file manager onto the window, or open them from File View. |
| Open a file from Finder (macOS) | Right-click the file and choose **Open With ▸ Nulis**, or drop it on the Nulis icon in the Dock. If Nulis is already open, the file opens in the existing window. |
| Find a file by name | **Ctrl/⌘ P** or **File ▸ Quick Open…** lists the open files, the files of the opened folder, and recent files. Type part of a name; file-name matches come first, then matches anywhere in the path, then the letters in order. Enter opens the file. Folders starting with a dot and `node_modules` are not listed, and at most 20,000 files of a folder are. |
| Open a recent file | **File ▸ Open Recent…** lists the last 20 files you opened on this computer. Type to narrow the list; Enter or a click opens the file. A file that no longer exists is removed from the list. **Clear Recent Files** empties it. |
| Open a folder | **Open a folder** on the welcome screen, or the *Open Folder* command in the palette. The folder appears in the **Folder** tab of File View. |
| Save | **Ctrl/⌘ S**, the **Save** toolbar button, or **File ▸ Save**. |
| Save under a new name | **Ctrl/⌘ Shift S** or **File ▸ Save As…**. |
| Save every changed file | **File ▸ Save All**. A file that has never been saved asks where to save it; cancelling skips that file. |
| Rename the file | **File ▸ Rename File…** renames the file in its folder. Unsaved changes stay unsaved, and the text is not reloaded. A name that is already taken is refused. Untitled files cannot be renamed this way. |
| Find the file on disk | **File ▸ Show in Places** opens File View on the **Places** tab with the folders above the file expanded and the file highlighted. |
| Close the file | **Ctrl/⌘ W**, the **Close** toolbar button, the **×** on the tab, or **File ▸ Close File**. |
| Close every file | **File ▸ Close All Files**. Nulis asks about each file with unsaved changes; **Cancel** stops and keeps the remaining tabs. |
| Close the window | **Ctrl/⌘ Shift W**, or the window's close button. |

Right-click a tab, or a file in the **Open** list of File View, for a menu with **Close**, **Close All Files Except This**, **Save**, and **Save As…**.

#### Unsaved changes

A tab shows a dot before its name while the file has unsaved changes. The dot disappears when you save, and also when you undo (or redo) back to exactly the saved content.

If you close a file, a tab, or the window while something is unsaved, Nulis asks whether to **Save**, **Discard**, or **Cancel**. Nothing is discarded without that question.

#### How saving works

Nulis never overwrites a file in place. It writes the new content to a temporary file in the same folder, flushes it to disk, checks its length, checks that the original was not changed by another program in the meantime, and only then replaces the original in one step. If anything fails, the original file is untouched.

### Files changed by other programs

Nulis notices when another program changes or removes a file you have open. It checks when its window comes back to the front and every few seconds while it is in front, without reading the file's content.

- **Changed:** choose **Reload** to show the new version, or **Keep Mine** to keep the text in the editor. With unsaved edits, the button reads **Discard Edits and Reload**; keeping yours means the next Save replaces the other program's changes.
- **Deleted or moved:** **Keep Open** keeps the text so you can save it again; **Close** closes it.

Each change is asked about once. To reload at any time, choose **File ▸ Reload from Disk**, right-click the text and choose **Reload from Disk**, or use the command palette. If the document has unsaved edits, Nulis asks before discarding them. Reloading keeps the tab and caret position; undo history and line change markers start over.

### Sessions and recovery

Nulis runs as a single copy. Starting it again while it is open brings the existing window to the front instead of opening a second one, so two copies never compete over the same session and recovery data.

When you quit, Nulis remembers the open files, the tabs and workspaces with their pane layout, the opened folder, and the side bar. They are restored at the next start. A file that no longer exists is skipped, and the status bar says how many could not be restored.

While you work, Nulis records unsaved edits of local and untitled documents shortly after you make them. If the application or the computer stops unexpectedly, the next start offers to **recover** each document that had unsaved work, including untitled ones. You can restore or discard each one. A recovered document counts as unsaved until you save it.

### Very large files

Nulis is built to open files larger than memory, tens of gigabytes included.

- Opening is immediate, because the file is not read into memory. Only the part on screen is read.
- Scrolling, Go To, and the hexadecimal view work anywhere in the file.
- Edits are stored separately from the original file and merged when you save, so typing in the middle of a huge file is as fast as in a small one.
- The editor shows a large file one stretch at a time. The **↑ Previous viewport** and **Next viewport ↓** buttons at the bottom right of the editor, filled in the theme's accent color, move to the stretch before or after.
- Find and Replace All read the file in small pieces. **Count All** counts every match in the whole file, with progress and a **Cancel** button.

A few features that need the whole document at once — the JSON, XML, YAML, and plist tools, for example — have size limits and tell you when a file is too large for them.

### Printing and PDF

The **File** menu has **Export to PDF…**, **Export Selection to PDF…**, **Page Setup…**, and **Print…**. All four lay out the pages the same way, so a printed page and a page of the PDF match line for line.

| Action | How |
| --- | --- |
| Print | **Ctrl/⌘ Alt P** or **File ▸ Print…**. Nulis lays out the pages, with a progress bar and **Cancel** in the status bar, then opens **Print Preview** with every page. Click **Print…** there for the system print dialog, where you can also save the pages as a PDF. |
| Save as a PDF file | **File ▸ Export to PDF…** asks where to save, then writes the PDF in the background with a progress bar and **Cancel**. Above 100 pages, the PDF gets an outline with one entry per 100 pages. |
| Print only the selection | **File ▸ Export Selection to PDF…** exports each selection with its own line numbers and a thin rule between selections. A column selection exports the whole lines it spans. |
| Choose how pages look | **File ▸ Page Setup…** (see below). Print and both exports use what you save there. |

What gets printed depends on the view:

- **Code and text:** the document's text in a fixed-width font, with the editor's syntax highlighting.
- **Markdown in View mode:** **Print…** prints the formatted preview on white paper, within the Page Setup margins and without a header or footer. **Export to PDF…** always exports the Markdown source.
- **Hexadecimal editor:** Print and Export to PDF produce a hex dump: the offset, 16 bytes, and their characters on each row, without line numbers.

Above 1,000 pages, Nulis asks before going on. **Print** stops at 5,000 pages; for longer documents use **Export to PDF…**, which has no page limit. The number of pages depends on Page Setup: a larger font, wider margins, or landscape paper fit fewer lines on a page, so the same file takes more pages.

**The paper in the print dialog.** On macOS and Linux, the system print dialog opens on the paper and orientation chosen in Page Setup. On Linux a landscape page shows there as a paper such as *A4 landscape* with *Portrait* selected; leave it that way. On Windows, choose the same paper in the print dialog.

#### Page Setup

**File ▸ Page Setup…** shows a preview of the first page that updates as you change:

- **Paper:** A4, Letter, Legal, A3, or A5 (Letter by default in the United States, A4 elsewhere), and **Portrait** or **Landscape**.
- **Margins:** Narrow (10 mm), Normal (15 mm), Wide (25 mm), or Custom, with each side from 0 to 80 mm.
- **Font size:** 6 to 24 points.
- **Long lines:** wrap onto the next row (marked ↪ in the line numbers) or cut off at the edge.
- **Colors:** print colors on white paper, black and white, or the current theme's colors (a dark theme gives dark pages).
- **Line numbers** and **Syntax highlighting**, each on or off.
- **Header** and **Footer:** text on the left, center, and right. These fields are filled in for each page: `{file}` (the file name), `{path}` (the full path), `{date}`, `{page}`, and `{pages}` (the page count). Leave all three parts empty for no header or footer.

**Restore Defaults** puts every value back. **Save** keeps the values on this computer for every document. When a Markdown file is in View mode, the preview shows the formatted page instead, and only paper, orientation, and margins apply to it.

#### Characters and fonts

Pages use the JetBrains Mono font bundled with Nulis. In a PDF, characters that font lacks, such as Chinese, Japanese, or Korean, are drawn from fonts installed on your computer. Chinese, Japanese, and Korean characters take two columns, so columns stay lined up. Emoji and characters with no suitable installed font are left blank, and the status bar says how many there were. Print Preview can draw some of these, such as emoji, because it also uses the system's fonts. Scripts whose letters join or reorder, such as Arabic and Devanagari, are printed one letter at a time and do not look right.

## Tabs and Workspaces

Each open file has a tab. Click a tab to show it, drag it to reorder it, and use the arrow keys to move between tabs when the tab strip has focus. When there are more tabs than fit, left and right arrow buttons appear at the ends of the strip; the active tab is always scrolled into view.

### Workspaces: several files in one tab

A **workspace** is a tab that shows several files at once in split panes.

- **Create one by dragging.** Drag a tab down into the editor area. A highlight shows where it will land: left, right, top, or bottom of the pane under the pointer. Release to group the two files. Press Escape while dragging to cancel.
- **Create one from the toolbar.** **Split Right** and **Split Down** show the focused file a second time, beside or below itself.
- **Create one from the menu.** **Workspace ▸ Group with Next Tab Side by Side** or **Stacked**.

The new tab is named "Workspace". Type a name right away, or rename it later by double-clicking the tab, pressing **F2** on it, or choosing **Workspace ▸ Rename Workspace**.

Inside a workspace:

| Action | How |
| --- | --- |
| Resize panes | Drag the divider between two panes. With the divider focused, the arrow keys move it; Home and End move it to its limits. |
| Rearrange panes | Drag a pane's header onto another pane. |
| Move a pane back to its own tab | Drag its header to the tab strip, click the pop-out button in its header, or choose **Workspace ▸ Move Pane to New Tab**. |
| Close one pane | The **×** in its header, or **Workspace ▸ Close Pane**. |
| Move focus between panes | **Workspace ▸ Focus Next Pane**. |
| Dissolve the workspace | **Workspace ▸ Ungroup Workspace**. Every pane becomes a tab again. |

The same file can be shown in more than one pane or tab. All views stay in step: an edit in one appears in the others.

## Editing

### Editing text

Typing, deleting, and the arrow, Page Up, and Page Down keys work as in any editor. Hold **Shift** with a movement key to extend the selection.

**Home** moves to the first character that is not a space or tab; press it again to reach the very start of the line. **End** moves to the end of the line. **Ctrl/⌘ Home** and **Ctrl/⌘ End** go to the start and end of the document. On macOS, **⌘ ←** / **⌘ →** also go to the line start and end, and **⌘ ↑** / **⌘ ↓** to the document start and end. **Delete** removes the character after the caret (a Windows line ending counts as one character).

Right-click the text for a menu with Undo, Redo, Cut, Copy, Paste, Find, Replace, Select All, Select Line, Select Word, the line commands below, Hex Mode, and Copy File Path. The menu works on the current caret and selection; right-clicking does not move the caret.

| Action | How |
| --- | --- |
| Select a word | Double-click it, or press **Ctrl/⌘ D** with the caret in it. |
| Select a line | Triple-click it, or press **Ctrl/⌘ L**. |
| Select everything | **Ctrl/⌘ A**. |
| Extend a selection | Shift-click, or Shift with the arrow keys. |
| Move selected text | Drag the selection and drop it at the new place. |
| Copy selected text by dragging | Hold **Option** (macOS) or **Ctrl** (Windows and Linux) while dropping. |
| Cancel a drag | Press Escape. |
| Undo / redo | **Ctrl/⌘ Z** / **Ctrl/⌘ Shift Z** (also **Ctrl Y** on Windows and Linux), or the toolbar buttons. |
| Cut, copy, paste | The usual system shortcuts and the **Edit** menu. |
| Add to the clipboard | **Edit ▸ Copy and Append** / **Cut and Append** add the selection to the end of what is already on the clipboard, on a new line. |
| Swap with the clipboard | **Edit ▸ Paste and Copy** puts the selection on the clipboard and the clipboard's text in its place. |
| Paste an earlier copy | **Edit ▸ Paste from History…** lists the last 20 texts you copied or cut in Nulis during this session (each up to 1 MiB). Type to narrow the list and choose one to paste. The history is kept in memory only and is gone when Nulis closes; **Clear History** empties it sooner. |
| Insert / overwrite | Press **Insert**, or click the **INS** / **OVR** indicator in the editor toolbar. In overwrite mode, typing replaces the character under the caret. |
| Change case, format, encode | Choose from the **Format** menu, the **Format** list in the editor toolbar, or the command palette; see *Formatting* below. |
| Word wrap | **Alt Z**, **View ▸ Word Wrap**, or the **Wrap** button in the editor toolbar. Nulis remembers the choice. |

### Formatting

The **Format** menu, the **Format** list in the editor toolbar, and the command palette hold these commands. Each is one undo step.

| Command | What it does |
| --- | --- |
| To Upper Case, To Lower Case, Invert Case | Changes the letters in the selection. |
| Capitalize Words | Starts each word in the selection with a capital and lower-cases the rest. |
| Trim Selection | Removes spaces at both ends of the selection. |
| Indent Lines / Outdent Lines | Adds or removes one level of indentation at the start of each selected line, as a tab or as spaces depending on the document (see Indentation). **Tab** indents when the selection covers more than one line; **Shift Tab** always outdents. |
| Toggle Line Comment | **Ctrl/⌘ /**. Comments out the selected lines with the language's comment marker (`//`, `#`, `--`, or `<!-- -->` for markup), lined up at the shallowest indentation; if every line is already commented, removes the markers. Plain text and JSON have no comments. |
| Trim Trailing Spaces | Removes spaces and tabs at the end of lines. |
| Tabs to Spaces / Leading Spaces to Tabs | Converts using tab stops every four columns. |
| Sort Lines Ascending / Descending, Sort and Remove Duplicates | Sorts the selected lines. Line endings stay in place; a file without a final line ending keeps it that way. |
| Encode Base64 / Decode Base64 | Encodes the selected bytes, or decodes Base64 text. Decoding refuses text that is not Base64, and binary results (use the hex editor for those). |
| Insert Date and Time | Inserts the local date and time as `2026-10-20 14:05:09`. |

Trimming, tab conversion, and sorting apply to the whole document when nothing is selected (up to 1 MiB of text); the other commands work on the lines that hold a caret or selection. The tab width is four columns and indentation uses a tab.

### Line commands

Line commands act on every line that holds a caret or part of a selection. A selection that ends at the very start of a line does not include that line. Each command is one undo step and keeps the file's line endings.

| Action | How |
| --- | --- |
| Duplicate line | **Ctrl/⌘ Shift D**. The caret moves to the copy, so pressing it again keeps copying downward. |
| Delete line | **Ctrl/⌘ Shift K**. |
| Move line up / down | **Alt ↑** / **Alt ↓** (**⌥ ↑** / **⌥ ↓** on macOS). Nothing moves when a line is already at the top or bottom. |
| Open a new line below | **Ctrl/⌘ Enter**. The new line gets the same indentation; the current line is not split. |
| Delete to line start / end | **⌘ Delete** / **⌘ Forward Delete** on macOS. |

A drag-and-drop move or copy is a single undo step. Undo history is kept per document, within a memory budget; very old steps of a long session can be dropped to stay inside it.

### Edit mode and Read-Only mode

Every document is in one of two modes, in the spirit of an editor with separate modes for changing and for reading text.

| Mode | What it means |
| --- | --- |
| **EDIT** | The normal mode. Typing and every other change works. |
| **READ ONLY** | The document cannot be changed. You can still move the caret, select, copy, search, go to a line, fold, set bookmarks, switch language and encoding, compare, and save. |

Switch modes with any of these:

- the **EDIT** / **READ ONLY** button in the editor toolbar, beside **INS**;
- **Ctrl/⌘ Shift L**;
- **View ▸ Read-Only Mode**, or *Toggle Read-Only Mode* in the command palette.

Read-Only mode is hard to miss: the status bar badge and the toolbar button turn amber and read **READ ONLY**, the tab and the pane header show a lock, the editor toolbar gains an amber line, and the caret becomes a steady block instead of a blinking bar.

In Read-Only mode, typing, Backspace, paste, cut, drag-and-drop, Transform, Replace, the JSON, XML, YAML, and plist tools, hexadecimal edits, macros, and Undo and Redo all leave the document untouched, and the status bar says that the document is read-only. The protection is enforced by the editor core, not only by the window, so no command, macro, or script can change a read-only document.

The mode belongs to the document, not to one view: every tab and pane showing it switches together. It lasts until you switch it back or close the document; a document always opens in Edit mode. Read-Only mode does not change the file's permissions on disk.

### Multiple carets and column editing

#### Multiple carets

Hold **Alt** (Option on macOS) and click to add a caret. Everything you type, delete, or paste then happens at every caret at once, and counts as one undo step. Click without Alt to return to a single caret.

#### Column (block) editing

Column editing selects a rectangle of text across several lines.

- Hold **Alt + Shift** and drag, or
- turn on the **Column editing** button in the editor toolbar, then drag normally.

With a rectangle selected:

- **Typing** inserts the same text on every line of the rectangle, and the rectangle stays active so you can keep typing.
- **Backspace** and **Delete** remove one column from every line.
- **Paste** distributes multi-line clipboard text across the rows, one line per row.
- The rectangle may extend past the end of shorter lines ("virtual space"); typing pads those lines with spaces.

#### Column tools

These are in the **Format** menu and the command palette. Each change is one undo step.

| Tool | What it does |
| --- | --- |
| Insert Number Sequence… | Opens a bar above the text: choose the first number, the step (negative counts down), and padding (none, leading zeros, or aligned right), then press Enter. One number goes on each row of a column block, at each caret, or at the start of each line of a selection that spans lines. Selected text is replaced, as typing would. |
| Sum Selected Numbers | Adds the numbers in the column block or selection and shows the total in the status bar, for example `Sum: 3.50 (2 numbers)`. The arithmetic is exact. A number glued to letters, such as `v2`, is skipped; commas are not read as thousands separators. |
| Left Justify / Center / Right Justify Columns | Moves the text in each row of a column block to its left edge, center, or right edge. Blank cells are left alone, and a row that ends inside the block gets no trailing spaces. The block stays selected. |
| Convert to Fixed-Width Columns | Pads delimited text (CSV and similar) so the columns line up. The delimiter is found from the first line that has one: tab, comma, semicolon, or `|`. A delimiter inside double quotes is part of the text. With nothing selected it covers the whole document. |
| Convert to Delimited Columns | Removes that padding (spaces before each delimiter) again. |

### Bookmarks and folding

| Action | How |
| --- | --- |
| Toggle a bookmark on the current line | **Ctrl/⌘ F2**, or the bookmark button. |
| Next / previous bookmark | **F2** / **Shift F2**. |
| Remove all bookmarks | The clear-bookmarks button. |
| Fold or unfold a region | Click the small triangle beside the line number, or use the **Fold** button for the region at the caret. |

Matching brackets are highlighted when the caret is next to one.

The **document map** at the right edge shows a miniature of the document. A long document is scaled to fit the map's height, so the whole file is always visible; click a place in it to jump to the matching line. Documents larger than 8 MiB are shown from sampled lines. Its left edge shows ticks where lines have changed.

### Line change indicators

A thin bar in the gutter, between the line number and the text, shows what has changed since the file was opened.

| Marker | Meaning |
| --- | --- |
| Red bar | The line has been changed and is not saved yet. |
| Green bar | The line was changed and has been saved during this session. |
| Small red notch between two lines | Lines were deleted at this place. |
| No marker | The line is as it was when the file was opened. |

The markers follow undo and redo: undoing a change removes its marker. Hover a marker for its meaning in words.

Turn the indicators on or off for one document with the button in the editor toolbar, or for all documents with **View ▸ Line Change Indicators** or **Settings ▸ Editor ▸ Gutter**.

### View options

The **View** menu changes how every editor shows text. Nulis remembers these choices on this computer.

| Option | What it does |
| --- | --- |
| Word Wrap (**Alt Z**) | Wraps long lines at the window edge instead of scrolling sideways. |
| Show Whitespace | Draws `·` over spaces and `→` over tabs. The text itself does not change. |
| Show Line Endings | Labels the end of each line with **LF**, **CRLF**, or **CR**. |
| Line Numbers | Shows or hides line numbers. Bookmarks, folding arrows, and change markers stay in a narrow gutter. |
| Column Marker at 80 | Draws a thin vertical line after column 80. |
| Zoom In / Zoom Out / Reset Zoom (**Ctrl/⌘ =**, **Ctrl/⌘ -**, **Ctrl/⌘ 0**) | Makes the editor text larger or smaller, from 50% to 300%. The font size in Settings stays the same. |
| Always on Top | Keeps the Nulis window above other windows until you turn it off. It is not remembered after Nulis closes. |

## Search

### The Find and Replace window

Press **Ctrl/⌘ F** (Find) or **Ctrl/⌘ H** (Replace), click the magnifier in the editor toolbar, or right-click the text and choose **Find…** or **Replace…**. It opens as its own window, like Settings, so you can place it anywhere, including on another display. It is not modal, so you can keep editing while it is open. Selected text (up to 1,000 characters) is put into **Find what**. Escape or the close button closes it.

The window has four tabs: **Find**, **Find in Files**, **Replace**, and **Replace in Files**.

- **Find what** and **Replace with** accept several lines. Enter runs the main action (Shift Enter finds the previous match); Ctrl/⌘ Enter adds a line break. The clock button next to each field lists your last 20 entries.
- **Find where** / **Replace where**: **Current document**, **Selected text** (the selection at the moment you press the button), or **All open files**.
- **Options**: **Match case**, **Match whole word** (the text is not part of a longer word), **Use regular expressions** (see [Regular expressions](#regular-expressions); **Regex help** next to it opens that section), and on the Find and Replace tabs **Search in column** (see [Search in column](#search-in-column)). On the Find tab also **Highlight all items found**, **List lines containing find string**, and **Close after find**; on the Replace tabs **Close after replace all**. Nulis remembers the options the next time the window opens.
- A mistake in a regular expression or in the column numbers is shown in red under **Find what**, and the buttons wait until it is fixed.
- **Find** tab buttons: **Count All** reports the number of matches and of lines that hold them, with no limit, even in a file of many gigabytes. While it counts, the status line shows how far it has got (for example *Counting… 41% (13.0 GB of 31.8 GB) · 1,234,567 matches so far*) and the button becomes **Cancel**; you can keep scrolling and editing other files meanwhile. If the document is changed during the count, the count stops and says so. **Previous** and **Next** move between matches. In the current document they start at the caret and wrap around; in a selection or across open files they step through the matches found.
- **Replace** tab buttons: **Next**, **Replace** (the current match, in the current document), and **Replace All**. Replace All is one undo step per document. Across open files, read-only documents are skipped and named in the result.

### Regular expressions

With **Use regular expressions** checked, **Find what** is a pattern. Nulis uses one regular-expression style, the one most editors and programming languages share, and an engine that always runs in time proportional to the text: no pattern can make Nulis hang, even on a file of several gigabytes.

| Pattern | Matches |
| --- | --- |
| `.` | any character except a line break (`(?s)` at the start makes it match line breaks too) |
| `\d` `\w` `\s` | a digit, a word character (letter, digit, `_`), white space (including line breaks); `\D` `\W` `\S` are the opposites |
| `[abc]` `[^abc]` `[a-z]` | one of, none of, a range; `[[:alpha:]]`, `[[:digit:]]` and the other POSIX classes work inside brackets |
| `\p{L}` `\p{Greek}` | a Unicode letter, a Greek character, and other Unicode classes |
| `*` `+` `?` `{2,5}` | repeat 0 or more, 1 or more, 0 or 1, 2 to 5 times; add `?` (`*?`, `+?`) to match as little as possible |
| `a\|b` | `a` or `b` |
| `(…)` `(?<name>…)` `(?:…)` | a group you can refer to in the replacement, a named group, a group that is not numbered |
| `^` `$` | the start and end of a line |
| `\b` | the edge of a word |
| `(?i)` | ignore case from here on |
| `\.` `\(` `\\` | a character that otherwise has a meaning: a dot, a parenthesis, a backslash |

Examples: `\d{4}-\d{2}-\d{2}` finds dates such as 2026-10-20; `^[ \t]+$` finds lines with only spaces and tabs; `TODO|FIXME` finds either word; `"[^"]*"` finds text in double quotes.

**Replacing.** In **Replace with**, `$1` (or `\1`) puts back what the first group matched, `${name}` a named group, and `$0` or `$&` the whole match; `$$` is a dollar sign. `\n`, `\t`, and `\\` are a line break, a tab, and a backslash. For example, find `(\w+)=(\d+)` and replace with `$2 = $1` to swap the two sides. A reference to a group the pattern does not have is reported before anything changes. Without regular expressions the replacement is inserted exactly as typed.

**Not supported.** Lookahead and lookbehind (`(?=…)`, `(?!…)`, `(?<=…)`, `(?<!…)`) and backreferences inside the pattern (`(\w)\1`) are left out on purpose: they are what can make other engines run for minutes. Nulis names them in the message if you use one. A single match may be at most 1 MiB; a pattern such as `(?s).*` that would match more says so instead of stopping somewhere in the middle.

### Search in column

On the Find and Replace tabs, check **Search in column** and fill in **From** and **To** to search each line only between those columns. Columns are counted the way the status bar shows **Col**: characters from 1, a tab being one character. Both columns are included; leave **From** empty to start at the beginning of each line, or **To** empty to go to its end. A match must lie entirely inside the columns, and with regular expressions `^` and `$` match at the edges of the columns, so `^\d+$` finds columns that hold only digits. Count All, Next, Previous, Replace, Replace All, and highlighting all respect the columns.

### Lines containing the text

With **List lines containing find string** checked, **Count All** or **Next** also opens a separate window listing every matching line. Click a line, or select it and press **Go To** or Enter, to put the caret on that match. **Bookmark All** bookmarks every listed line in open documents, **Copy** copies the list, and **Show line numbers** turns the number column on or off. The list shows the first 512 bytes of each line and stops at 5,000 lines or 10,000 matches, and says so when it does.

### In files

On the **Find in Files** tab, choose **Files in a folder** or **All open files**. For a folder, enter name patterns in **In files/types** (for example `*.rs; *.md`; empty means every file; letters match either case), choose the folder with **Browse…** (the open workspace folder is the default), and decide whether to **Include subdirectories**. Press **Find**. Results open in the list window with the file name, line number, and line; click one to open the file at that match. The search skips symbolic links and stops at fixed limits on the number of files and results, and says so when it does.

### Replace in files

On the **Replace in Files** tab, fill in **Find what** and **Replace with**, then choose **Files in a folder** (with the same file-type, folder, and subfolder fields as Find in Files) or **All open files**.

For a folder, nothing changes until you confirm:

1. Press **Preview**. The window lists every file that would change and how many matches it has. Files that are open in Nulis are marked; their text is changed in the editor and left unsaved, so you can review it and undo it.
2. Press **Replace All**, then **Replace** to confirm. Closed files are saved right away, with the same safe save as **Save**.
3. The result says how many matches were replaced and lists any file that was not changed and why: for example, a file that another program changed after the preview, or a document in Read-Only mode.

Changing the text, the replacement, the options, or the folder after a preview makes it out of date; preview again before replacing. One Replace in Files changes at most 1,000 files.

### Moving around

The **Search** menu collects Find and Replace with the commands below; each is also in the command palette.

| Action | How |
| --- | --- |
| Find next / previous without opening the window | **F3** / **Shift F3** repeat the last search; with no earlier search, F3 opens Find. |
| Go to the matching bracket | **Ctrl/⌘ Shift \**. Next to a bracket, the caret goes to its partner; elsewhere, to the closing bracket around it. |
| Go back / forward | **Ctrl/⌘ [** / **Ctrl/⌘ ]** return to where the caret was before a jump (Go to Line, find, bookmarks, brackets, changes). |
| Next / previous change | **Alt F5** / **Alt Shift F5** move between lines with line change indicators. |
| List bookmarks | **Search ▸ List Bookmarks…** opens the bookmarked lines in the results window; click one to go there. |
| Delete bookmarked lines | **Search ▸ Delete Bookmarked Lines** removes every bookmarked line in one undo step and clears the bookmarks. |

The status bar also shows the number of words and lines (for documents up to 64 MiB) and the Unicode code point of the character after the caret, such as `U+00E9`.

### Go to a line

Press **Ctrl/⌘ G**, type a line number, or a line and column such as `120:8`, and press Enter. This also works in very large files.

## Languages and Encodings

### Languages and encodings

#### Syntax highlighting

Nulis picks a language from the file name and extension. For names that say nothing, it looks at the first bytes of the file: a `#!` line, an XML declaration, `<?php`, or an HTML doctype.

To override the choice, use the **View As** list in the editor toolbar. This also switches which tools are offered: choosing JSON, XML, YAML, or Property List turns on that language's tools for a file with any extension. Nulis remembers the choice for files it reopens at the next start.

To always open a kind of file as a certain language, add it to `files.associations` in `settings.json`, in the folder where Nulis keeps its data (see "Where Nulis stores data" in the README). A pattern is `*.ext`, an exact file name, or the start of a name followed by `*` (`Jenkinsfile.*`); letters match either case. Your associations come before Nulis's own. For example:

```json
{
  "schemaVersion": 1,
  "values": {
    "files": { "associations": { "*.h": "cpp", "Jenkinsfile": "ruby" } }
  }
}
```

Nulis reads the file when it starts, so restart it after editing; files already open keep their language until reopened. A pattern that is not valid, or a language that does not exist, is ignored. Language names to use: `plaintext`, `applescript`, `bash`, `batch`, `c`, `cpp`, `csharp`, `css`, `csv`, `dockerfile`, `go`, `html`, `ignore`, `ini`, `java`, `javascript`, `json`, `jsonc`, `kotlin`, `lua`, `makefile`, `markdown`, `mysql`, `objective-c`, `perl`, `php`, `plist`, `plsql`, `powershell`, `python`, `ruby`, `rust`, `sql`, `swift`, `toml`, `tsv`, `tsx`, `typescript`, `vb`, `vbscript`, `xml`, `xsl`, `yaml`, `zig`.

When Nulis's own languages and an installed plugin claim the same pattern, **Settings ▸ Languages** lists the claims under **File association conflicts**. Choosing one records your choice in `files.associations` when you press **Apply**; your association then wins. A plugin can also add associations to a built-in language instead of defining a new one.

Supported languages: AppleScript, Bash and other shells, Batch, C, C++, C#, CSS, CSV, Dockerfile, Go, HTML, Ignore File (`.gitignore` and similar), INI, Java, JavaScript, JSON, JSON with Comments (`.jsonc`), Kotlin, Lua, Makefile, Markdown, MySQL / MariaDB, Objective-C / Objective-C++, Oracle PL/SQL, Perl, PHP, PowerShell, Property List (`.plist`), Python, Ruby, Rust, SQL, Swift, TOML, TSV, TSX (`.tsx`), TypeScript, VBScript / ASP, Visual Basic, XML, XSL, YAML, and Zig. Anything else is shown as plain text.

- **JavaScript, TypeScript, and TSX** also highlight regular expressions (`/^a+$/g`), the code inside `${…}` in template strings, and, in JavaScript and TSX, JSX elements with their attributes. Plain TypeScript files do not get JSX, because there `<Type>value` is a type cast.
- **Markdown** sections fold under their headings, and **YAML** blocks fold by indentation.

- **CSV and TSV** files color each column differently, so a column keeps its color all the way down. Quoted fields may hold the separator, doubled quotes (`""`), and line breaks; text after a closing quote is marked as an error. For `.csv` files Nulis takes the separator from the first line: comma, semicolon, tab, or `|`. TSV (`.tsv`, `.tab`) always uses tabs.
- **YAML** also marks document separators (`---`, `...`), anchors and aliases (`&name`, `*name`), tags (`!Ref`, `!!str`), block text after `|` or `>`, and quoted keys.
- A **binary property list** (a `.plist` file in Apple's binary format) or a **compiled AppleScript** (`.scpt`) is not text: it opens read-only in the hexadecimal editor, and the status bar says why. XML property lists and AppleScript source (`.applescript`) open as text as usual.

#### Text encoding

The encoding list in the editor toolbar switches between **UTF-8** and **Windows-1252**. Changing it re-reads the same bytes in the other encoding; it does not rewrite the file. Bytes that are not valid in the chosen encoding are preserved exactly when you save, as long as you do not edit them.

### Line endings

The status bar shows which line endings the document uses: **LF** (macOS, Linux), **CRLF** (Windows), **CR** (classic Mac), or **Mixed** when it has more than one kind. A document without any line break shows **LF**. For a document larger than 64 MiB, the label comes from its first 1 MiB.

**Enter** inserts the same line ending as the line the caret is on, so a Windows file stays CRLF.

To convert, click the label in the status bar, or use **Format ▸ Convert Line Endings to LF / CRLF / CR**. Every line ending in the document changes in one undo step; bookmarks stay on their lines. Conversion works on documents up to 64 MiB with at most 1,000,000 line endings to change, and not in Read-Only mode. Save the file to keep the change.

### Indentation

**Enter** starts the new line with the right indentation for the language: one level deeper after `{`, `(`, `[`, a Python `:`, an opening HTML or XML tag, a YAML key without a value, a Makefile rule, or a block keyword such as Ruby `def`, Lua `then`, or Bash `do`. Pressing Enter between a pair, as in `{|}` or `<p>|</p>`, puts the closing part on its own line. A closing keyword typed on its own line (`end`, `fi`, `End Sub`, `else`) moves left when you press Enter. Other languages keep the indentation of the line you were on.

Typing `}`, `)`, or `]` on a line that holds only indentation moves the line to match its opening bracket; so do `</` in HTML and XML and `else:`, `elif …:`, `except …:`, and `finally:` in Python. One **Undo** takes back the move and the character together.

**Tab** inserts a tab, or spaces up to the next tab stop when the document is indented with spaces. **Backspace** in indentation made of spaces removes one level at a time.

Nulis looks at how each file is already indented (tabs, or 2, 4, or 8 spaces) and follows it. Otherwise it uses **Settings ▸ Editor ▸ Indentation**: Auto-indent (follow the language, keep the line's indentation, or off), tab size, and spaces instead of tabs. YAML always uses spaces and Makefile recipes always use tabs, as those formats require. To switch the language rules off for one language only, untick **Auto-indent rules** for it in **Settings ▸ Languages**. Settings for one language can also go in `settings.json`, for example `"[python]": { "editor": { "tabSize": 4, "insertSpaces": true } }`.

With Auto-indent off, Enter inserts only the line break, as in earlier versions. Pasted text is inserted exactly as copied.

### Suggestions

While you type a word, Nulis can suggest how to finish it: words already in the document (nearest first), the language's keywords and types, and short snippets such as `fn`, `def`, `for`, or `html5`. In languages with their own keywords the list opens by itself after two letters; in Plain Text and Markdown it opens only when you ask. Press **Ctrl+Space** to ask anywhere (on macOS, **Option+Esc** works too, since Ctrl+Space often switches the keyboard input source). The **Show Suggestions** command in the palette does the same and can be given another shortcut in Settings.

In the list, **Up**/**Down** and **Page Up**/**Page Down** move, **Enter** or **Tab** inserts, **Escape** closes, and a click inserts. Keywords follow the case you started with in languages where case does not matter (`SEL` becomes `SELECT`). If you have several carets, every caret that typed the same word gets the suggestion, in one undo step.

A snippet selects its first placeholder; type to replace it and press **Tab** for the next one. A placeholder that appears twice, such as the loop variable in a JavaScript `for`, gets the same text when inserted, but editing one copy does not change the other. Moving the caret out of the placeholder, or Undo, ends the snippet.

To change when the list opens, or to stop Enter from inserting, use **Settings ▸ Editor ▸ Suggestions**. To switch suggestions off for one language, untick **Suggestions** for it in **Settings ▸ Languages**.

### Language servers (optional)

Nulis can show the problems a language server reports, such as `rust-analyzer` for Rust or `clangd` for C and C++. Nulis does not include or download language servers; install one yourself and tell Nulis where it is.

Add it to `lsp.servers` in `settings.json`, giving the program and its arguments as a list (never a shell line) and the languages it serves:

```json
{
  "schemaVersion": 1,
  "values": {
    "lsp": {
      "servers": {
        "rust": { "command": ["rust-analyzer"], "languages": ["rust"] },
        "clangd": { "command": ["clangd", "--background-index"], "languages": ["c", "cpp"] }
      }
    }
  }
}
```

Restart Nulis, then open **Settings ▸ Languages ▸ Language servers** and click **Approve**. Nothing runs before that, and changing the command or its language list withdraws the approval. **Revoke** stops the server; **Restart** clears one that Nulis set aside after repeated failures. Invalid entries are listed with the reason instead of being run.

A server starts when you open a document in one of its languages, runs once per project folder (the nearest folder with `.git`, `Cargo.toml`, `package.json`, `go.mod`, `pyproject.toml`, `compile_commands.json`, or `.clangd`), and stops when the last document it serves closes. The first answer can take several seconds while it indexes.

Problems appear as a coloured dot in the gutter and a wavy underline under the text. The status bar counts them; click the count to open the **Problems** panel, where choosing a problem selects it and scrolls it into view. The panel also says why the last request failed.

### Markdown preview

A Markdown document has an **Edit / View** switch at the left of the editor toolbar. **View** shows the formatted document, including tables, task lists, and other GitHub-style syntax, and images stored beside the file. Raw HTML inside Markdown is not rendered.

## Tools

### JSON, XML, YAML, and plist tools

When the focused document is JSON, XML (XSL counts as XML), YAML, or a property list (`.plist`), its tools appear in the main toolbar, in the **Tools** menu, and in the command palette. For any other document the buttons are hidden and the menu items are greyed out.

| Tool | What it does |
| --- | --- |
| **Validate** | Checks the document. If there is a problem, the status bar gives the line and column, and the caret moves to it. |
| **Beautify** | Re-indents the document with two spaces. It is one undo step. An invalid document is not changed; you are shown where the problem is instead. |
| **Minify** (JSON only) | Removes all whitespace that has no meaning. |

The buttons and menu items name the kind of document, for example **Validate YAML** or **Beautify plist**. Choosing another language with **View As** updates them at once.

You can switch any of these tools, and the Markdown preview, off for one language in **Settings ▸ Languages ▸ Language features**: untick it and press **Apply**. The tool disappears from the toolbar, the Tools menu, and the palette, also for documents that are already open; their text is not changed. The same page lists your `files.associations` and marks any that Nulis ignored.

Details worth knowing:

- JSON keeps the order of keys and the exact spelling of numbers. Nothing is reordered or rewritten.
- XML beautify only changes whitespace between elements. An element that contains text, or is marked `xml:space="preserve"`, is left exactly as written.
- XML validation checks that the document is **well-formed**: one root element, matching tags, quoted and unique attributes, and complete `&…;` references. It does not validate against a DTD or schema, and it does not check namespaces.
- **YAML** validation checks every document in the file (separated by `---`) and also reports a key that appears twice in the same mapping, which YAML does not allow but many programs accept silently.
- **YAML Beautify** re-writes the file from its values with two-space indentation; values and key order are kept. Because it starts from the values, it cannot keep comments, and it writes anchors and aliases (`&name`, `*name`) out in full; quoting and the spelling of numbers (such as `0x1F`) may also change. When the file has comments or anchors, Nulis says so and asks first; one Undo restores the file. A file with tags such as `!Ref` (common in CloudFormation templates) is not beautified at all, because the tags would be lost.
- **plist** validation checks that the file is well-formed XML and a correct property list: one value inside `<plist>`, a `<key>` before every value in a `<dict>` and no key twice, only property-list elements, and values that read as their type: whole numbers in `<integer>` (up to 64 bits, `0x` hex allowed), numbers in `<real>`, dates such as `2026-10-20T09:14:02Z` in `<date>`, Base64 in `<data>`, and empty `<true/>` and `<false/>`. plist Beautify is the XML one. Binary property lists (`bplist00`) are not text and are not checked.
- The tools work on documents up to 8 MiB.

### Hexadecimal editor

Click the hexadecimal button at the right of the tab strip (or run *Toggle Text / Hex Editor*) to see the focused file as bytes: offsets on the left, hexadecimal values in the middle, and text on the right. Selecting in one column selects the same bytes in the other. You can type over bytes, and undo and redo work as in the text view.

The hexadecimal view reads only the bytes on screen, so it works on files of any size. Click the button again to return to text.

### Comparing documents and folders

- **Compare open documents.** With two files open, click the compare button at the right of the tab strip. The files are shown side by side with differences marked. Each difference can be copied in either direction. Text files are compared line by line, binary files byte by byte.
- **Compare folders.** Click the folder-compare button and choose two folders. Nulis lists files that exist on one side only and files whose content differs.

### Terminal

Click the terminal button at the right of the tab strip to open a terminal below the editor. It runs your default shell. Click the button again to close it.

## File View

### File View

Click **Files** in the toolbar. File View has three tabs.

| Tab | Shows |
| --- | --- |
| **Open** | The files that are open now. Click one to switch to it; right-click for close and save commands. |
| **Places** | Your computer: the home folder, common folders, and mounted drives and volumes, as a tree you can expand. |
| **Folder** | The folder you opened with *Open Folder*. |

Folders are read only when you expand them, so browsing a large disk stays fast. Type a pattern such as `*.md; *.txt` in the filter field to show only matching files, and use the button beside it to show hidden files and folders. Click a file to open it.

**File ▸ Show in Places** goes the other way: it opens the **Places** tab with every folder above the focused file expanded, and highlights the file.

## Automation

Click **Automation** in the main toolbar.

### Macros

A macro records editing actions and replays them.

1. Give the macro a name and an ID, and press **Start Recording**.
2. Do the edits in the editor.
3. Press **Stop & Save**, or **Cancel** to throw the recording away.

Saved macros are listed in the panel. Press the run button beside one to replay it in the focused document.

### Scripts

A script is a small program that drives the editor. Type or paste the source, list the editor commands it needs, and press **Save Script**. A script cannot run until you press **Approve**; approval covers exactly the commands you listed, and editing the script requires approving it again. **Revoke** withdraws the approval.

## Plugins

Click **Plugins** in the main toolbar.

- **Install from folder…** installs a plugin package from a folder on disk.
- A newly installed plugin does nothing until you press **Approve requested access**. The panel shows what the plugin asks for. Approval is tied to the exact content of the package: if the plugin's files change, it must be approved again.
- **Revoke** withdraws approval; **Uninstall** removes the plugin.
- **Disable** makes an approved plugin inert without revoking it. Each feature the plugin contributes — **Commands**, **Languages**, **Highlight**, **Indentation**, **Completion**, **Snippets** — has its own checkbox, so you can keep a language and turn off, say, only its colors.
- A plugin that adds a language supplies it as data: file associations, comments, highlighting, indentation, completion, and snippets, with **no JavaScript and no process**. Turning **Languages** off removes the language, its associations, and its providers; an open document then shows as Plain Text until it is reopened.

Plugins run in a separate, restricted process, not inside the editor. Commands that a plugin adds appear in the command palette.

To write your own plugin, see [Building Plugins](PLUGIN_GUIDE.md).

## Settings and Themes

Open Settings with **Ctrl/⌘ ,**, the **Settings** toolbar button, or the menu. Settings opens in its own window.

| Section | What you can change |
| --- | --- |
| **General** | Whether the welcome screen is shown at startup; restore all defaults. |
| **Editor** | Font, font size, line height, ligatures; whether line change indicators are shown by default. |
| **Appearance & Themes** | The color theme, and your own themes (see [Your own theme colors](#your-own-theme-colors)). There are 99 built-in themes, with Nulis's own **Nulis · Ember Night** (dark) and **Nulis · Paper Current** (light) at the top. The theme you apply paints the editor and every Nulis window. Install a theme file (Nulis theme JSON, or a NvChad Base46 Lua theme), **Duplicate Selected** to make an editable copy of any theme, and **Remove Selected** to uninstall one you installed. Every theme keeps the text selection, secondary text, and toolbar icons easy to see: where a theme's own colors are too faint, Nulis strengthens them in that theme's hues, and the panel names any color it will paint (see [Theme colors and readability](#theme-colors-and-readability)). The Nulis mark in the title bar and the About window's background follow the theme. |
| **Languages** | The languages Nulis recognizes, which language features are on for each, file association conflicts, and the language servers you configured. |
| **Keybindings** | The shortcut for any command. |
| **Menus & Toolbar** | Which commands are in the File menu and on the main toolbar, and in what order: add one from the list, move a row up or down, remove it. The toolbar's **More ▸ Customize Toolbar…** opens Settings on this page (a Settings window that is already open keeps the page it was showing). |
| **Workspace** | Workspace and layout options. |
| **Extensions & Security** | Installed plugins and their approval state. |

Changes are a draft until you press **Apply**. A dot marks each section with unapplied changes, and **Cancel** throws the draft away. If you close the window with unapplied changes, Nulis asks whether to apply or discard them. Appearance changes are previewed live in the main window while you edit them, and the theme you apply paints the editor and every Nulis window — there is no separate switch for that. Each section has a filter field for finding a setting or command by name.

### Your own theme colors

Built-in themes, and themes you installed from a file, are read-only. To change a theme's colors, press **Duplicate Selected**: the panel makes an editable copy named *"<name> copy"* and marks it **· User** in the theme list. A copy is a draft, like everything else in Settings — **Revert** discards it, and the store button says exactly what it will write: **Save Copy** for a new copy, then **Save Colors**, **Save Name**, or **Save Name & Colors** after edits. The preview box paints the copy's colors as you pick them, so you see them before you save, and it paints them the way the editor will.

- **Colors** — each editable color has a native color picker, a hexadecimal field, and the arrow keys. A hex value that is not a color is refused, and the reason is shown. [Theme colors and readability](#theme-colors-and-readability) covers the four colors Nulis keeps readable.
- **Name** — type over the name field and it becomes the theme's name in the picker. The copy keeps its own id, so your settings, keybindings and shortcuts that point at a theme keep pointing at it.
- **Remove Selected** uninstalls a theme you installed. Built-ins cannot be removed; the theme Nulis is using cannot be removed until you switch to another; Nulis asks before removing.
- Copies are numbered `.copy`, then `.copy2`, `.copy3`, and so on. Installed themes (files you install plus your copies) are capped at 64, and a theme file is capped at 256 KiB.

Your themes live beside your settings, under Nulis's application data, and survive a restart. A theme file is declarative data — colors, a name, the theme kind — and cannot contain CSS, scripts, URLs, imports, or remote resources.

### Theme colors and readability

Four of a theme's colors are measured against contrast floors, because you have to be able to see them: the **text selection**, **secondary text**, **toolbar icons**, and **disabled icons**. Where a color you pick falls below a floor, the theme file keeps your color and Nulis paints a stronger one — the theme's own accent blended into its background, the weakest blend that reads. Every other color paints exactly as you typed it.

The panel says when this happens. Under the color, it names the color Nulis will paint (**"Nulis paints #43586B — Editor Selection stays readable. Your color is saved as typed."**), and the preview box paints that color too, with a sample selection, so the panel and the editor never disagree.

**Text selection** is the color to know. The highlight has to stand out from the background and from the current line, and the text drawn on it has to stay readable — a bright highlight under light text cannot do all three, so Nulis takes the highlight back toward the theme's own hues. To get a highlight that paints as typed, give the text on it room: darken **`editor.foreground`**, or pick a darker highlight, so the two clear 4.5:1.

Saving always stores what you typed. Nothing here changes a theme you did not pick a color in.

## Keyboard Shortcuts

These are the defaults. Shortcuts for commands can be changed under **Settings ▸ Keybindings**. The native menus do not show the keys that the editor handles itself, such as **Alt Z**, **Ctrl/⌘ P**, zoom, and **F3**; they still work.

| Shortcut | Action |
| --- | --- |
| Ctrl/⌘ N | New file |
| Ctrl/⌘ O | Open file |
| Ctrl/⌘ S | Save |
| Ctrl/⌘ Shift S | Save As |
| Ctrl/⌘ W | Close file |
| Ctrl/⌘ Alt P | Print |
| Ctrl/⌘ Shift W | Close window |
| Ctrl/⌘ K | Command palette |
| Ctrl/⌘ Shift L | Switch between Edit and Read-Only mode |
| Ctrl/⌘ , | Settings |
| F1 | Nulis Help |
| Ctrl/⌘ Z | Undo |
| Ctrl/⌘ Shift Z, Ctrl Y | Redo (Ctrl Y on Windows and Linux) |
| Ctrl/⌘ F | Find and Replace window, Find tab |
| Ctrl/⌘ H | Find and Replace window, Replace tab |
| Ctrl/⌘ G | Go to line |
| Ctrl/⌘ D | Select the word at the caret |
| Ctrl/⌘ A | Select all |
| Ctrl/⌘ L | Select line |
| Ctrl/⌘ Shift D | Duplicate line |
| Ctrl/⌘ Shift K | Delete line |
| Alt ↑ / Alt ↓ | Move line up / down |
| Ctrl/⌘ Enter | Open a new line below |
| Ctrl/⌘ / | Toggle line comment |
| Alt Z | Word wrap on or off |
| Ctrl/⌘ P | Quick Open |
| Ctrl/⌘ = / Ctrl/⌘ - / Ctrl/⌘ 0 | Zoom in / out / reset |
| F3 / Shift F3 | Find next / previous |
| Ctrl/⌘ Shift \ | Go to matching bracket |
| Ctrl/⌘ [ / Ctrl/⌘ ] | Go back / forward |
| Alt F5 / Alt Shift F5 | Next / previous change |
| Tab / Shift Tab | Indent a multi-line selection / outdent |
| Home / End | Line start (first non-blank, then column 1) / line end |
| Ctrl/⌘ Home / End | Document start / end |
| Delete | Delete the next character |
| Ctrl/⌘ F2 | Toggle bookmark |
| F2 / Shift F2 | Next / previous bookmark |
| F2 (on a workspace tab) | Rename the workspace |
| Insert | Switch between insert and overwrite |
| Alt + click | Add a caret |
| Alt + Shift + drag | Select a rectangle |
| Double-click / triple-click | Select word / line |
| Escape | Close the Find and Replace window, a list, or a dialog; cancel a drag |

## Limits

Nulis keeps every operation bounded so that it stays responsive. The limits you may meet:

| Item | Limit |
| --- | --- |
| Open tabs | 128 |
| Panes in one workspace | 16, nested at most 6 levels deep |
| Workspace name | 80 characters |
| JSON, XML, YAML, and plist tools | Documents up to 8 MiB |
| One regular-expression match | 1 MiB |
| Carets or selections | 1,024 |
| Bookmarks per document | 1,024 |
| Rows of a column block (and Insert Number Sequence) | 100,000 |
| Copy, cut, and Sum Selected Numbers | 8 MiB of selected text |
| Lines containing the text | 5,000 lines (10,000 matches); Count All itself has no limit |
| Find in Files | 2,000 results from up to 10,000 files |
| Replace in Files | 1,000 files at a time |
| Character, word, and line counts | Documents up to 64 MiB |
| Converting line endings | Documents up to 64 MiB, at most 1,000,000 line endings changed |
| Document map with exact lines | Documents up to 8 MiB; larger ones are sampled |
| Markdown preview | Documents up to 1 MiB |
| Quick Open | 20,000 files of the opened folder (folders starting with a dot and `node_modules` are skipped) |
| Open Recent | The last 20 files |
| Paste from History | The last 20 copies of this session, each up to 1 MiB |
| File name in Rename File | 255 bytes |
| Print | 5,000 pages (Nulis asks above 1,000; Export to PDF has no page limit) |
| Header or footer text in Page Setup | 256 bytes for each part |

When a limit is reached, the status bar explains which one.

## About and License

**Help ▸ About Nulis** (or a click on the Nulis brand in the title bar) shows the version, the origin of the name, and the license, over a batik background (parang and kawung) that is dark or light to match the theme. **View License** shows the full license text.

Nulis is free software. You can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

Nulis is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

Copyright © 2026 Muhammad Edy Suherman.
