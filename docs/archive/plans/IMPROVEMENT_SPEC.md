# Editor interaction and workspace improvement specification

**Status:** M15–M18 are implemented and covered by automated tests on macOS (see `WORK_STATE.md`). No milestone is closed yet: the hosted Windows/Linux runs and the manual pointer, IME, keyboard, and assistive-technology checks in *Delivery gates* are still outstanding.  
**Applies to:** Nulis desktop on macOS, Windows, and Linux.  
**References reviewed:** a private screen recording of a reference editor (text selection, tab splits, toolbar, map) and `workspace-grouping.mp4` (tabs grouped into a named workspace tab, side-by-side and stacked panes, rename flow). These are behavior references; Nulis keeps its own visual design and command architecture.

## Scope and order

| Milestone | Outcome | Depends on |
| --- | --- | --- |
| M15 — Precise pointer editing | Double-click word selection and drag-to-move selected text | Existing revisioned selection/edit commands |
| M16 — Line change indicator | A toggleable gutter marks unsaved and saved edits per logical line | M15 pointer tests; existing save/recovery lifecycle |
| M17 — Workspace tabs | Group tabs into one named workspace tab of nested horizontal/vertical panes | Existing tabs, session persistence, and guarded close |
| M18 — Desktop controls and settings | Cohesive vector icons and a separate, sectioned Settings window | M17 window/layout state and existing customization commands |

Each milestone is complete only after its acceptance scenarios pass on macOS, Windows, and Linux and its session/keyboard/accessibility behavior is checked. Existing M14 release gates remain separate.

## Shared constraints

- The Rust core remains authoritative for document bytes, revisions, selections, saves, and recovery. React may preview pointer drags and arrange panes, but commits document edits through central commands.
- Viewport, LCI, and layout metadata must stay bounded for files larger than memory. JSON byte coordinates remain decimal-string `u64` values.
- No feature silently discards dirty documents, changes a file path when renaming a workspace, or grants the settings webview general filesystem access.
- Pointer features have keyboard and screen-reader equivalents. Hit targets, focus, pressed/disabled state, light/dark themes, reduced motion, and high contrast are included in acceptance checks.

## M15 — Precise pointer editing

### Word selection

Double-clicking text selects the complete word under the pointer in one action. The native input's default selection is not authoritative; the editor translates the hit position to a byte range and commits it to the core. A click on whitespace selects that whitespace run; a click on punctuation selects the contiguous punctuation run. A click at a line ending selects the adjacent logical word when one exists. Selection must not split a Unicode grapheme, a CRLF pair, or malformed source bytes. Language modules may later supply word-boundary rules, but the first release uses one documented Unicode-aware rule across languages. Triple-click selects a logical line, including its line ending when present.

Acceptance: double-click Python identifiers, Markdown words, accented text, emoji, punctuation, tabs, wrapped lines, and text next to malformed UTF-8; selection and reported byte offsets agree. Double-clicking inside a current selection replaces it with the newly selected word. Keyboard word selection follows the same boundary rule.

### Drag selected text

Pressing and holding inside an existing nonempty ordinary selection begins a text drag after a small movement threshold. Before the threshold, a click places the caret. Dragging outside the selection shows an insertion caret and scrolls near viewport edges. Releasing inside the same document moves the exact selected bytes as one revision-checked undo action; the destination is interpreted after removal, so forward and backward moves land correctly. Releasing inside the original range is a no-op. Holding the platform copy modifier during the drop copies instead of moves. Escape or dropping outside a valid text surface cancels without an edit.

The move command accepts source range, destination byte offset, observed revision, and move/copy intent. The core validates boundaries and size, performs the move atomically, and returns the final caret/selection. Moving must preserve original bytes and line endings, including malformed text. A stale revision rejects the drop and refreshes the viewport; it never deletes the source first. Dragging a rectangle or multiple disjoint selections uses the existing selection gesture until a separate design for those cases is approved. Moving text between different documents is staged with M17 and must use an explicit safe transfer contract before it is enabled.

Acceptance: drag one word, a multi-line range, and a wrapped selection in both directions; verify exact bytes, one undo/redo step, auto-scroll, copy modifier, cancellation, no-op self-drop, stale revision, and no accidental file/tab drag. Test with a large file while retaining bounded viewport reads.

## M16 — Line change indicator (LCI)

Add a slim indicator lane between line numbers and editor text. An **unsaved** marker identifies lines touched since the last successful save; a **saved** marker identifies lines edited and saved during the current open editing session. A new untitled document begins without markers and receives unsaved markers on edit. Saving changes its unsaved markers to saved markers only after the atomic save succeeds; failed or canceled saves leave them unsaved. A subsequent edit on a saved-marked line makes it unsaved again. Closing and reopening a clean file starts a new editing session with no saved markers. Recovered unsaved edits regain their unsaved markers.

Insertion, replacement, deletion, paste, transform, drag move, multi-caret, rectangle edit, and undo/redo update the logical-line markers. For a deleted line with no remaining row, show the deletion at the nearest surviving line boundary; show a distinct top/end boundary marker when needed. Soft wraps are one logical line and display one marker. Marker state follows lines through insertions/deletions and does not drift when line numbers move. Undo/redo restores the marker state associated with the resulting document revision; returning to the saved content clears unsaved markers. This is an **editing-session indicator**, not a Git diff or a promise to retain a full-file historical baseline.

The editor toolbar and View menu expose **Show/Hide Line Change Indicators**; the setting has a persistent global default and an optional per-document override for the current session. Themes supply separate semantic colors and shapes for saved, unsaved, and deletion markers. Color alone must not carry meaning: a hover/focus description and accessible status expose the state. The document map may include aligned change ticks when visible, using the same bounded status query.

The core exposes revisioned status for requested visible logical lines, plus bounded positional samples for the map. Tracking uses sparse ranges or a disk-backed edit journal; opening a large file cannot scan it solely to initialize LCI, and memory cannot scale with total file length or line count. If a valid marker cannot be computed, the UI reports that limitation instead of showing a misleading clean gutter.

Acceptance: saved versus unsaved colors, toggles, save failure, Save As, undo/redo, moved/deleted lines, CRLF/LF/CR, malformed bytes, multi-caret and rectangle edits, session recovery, two panes showing the same document, and theme/high-contrast rendering. Benchmark a sparse 10 GiB file with a few edits and confirm bounded memory and first-viewport time.

Reference editors show a gutter indicator with distinct saved/unsaved colors. Nulis follows that interaction concept while using its own persistence and large-file rules.

## M17 — Workspace tabs

The reference is `workspace-grouping.mp4`. There is one tab strip. A tab is either a single document or a **workspace**: a tab that groups several documents as split panes. There is no separate layout bar and no tab strip inside a pane.

### Grouping

Dragging a tab from the strip into the content of the active tab groups the two. A drop overlay previews the half the dragged tab will occupy (left, right, top, or bottom of the pane under the pointer) before release. On release both tabs leave the strip and one workspace tab takes their place, with each document in its own pane. Dropping onto a pane of an existing workspace splits only that pane, so mixed arrangements such as one pane on the left and two stacked on the right are possible. A whole workspace can be dropped into another tab. Dragging the active tab onto its own content does nothing.

Each pane of a workspace has a small header with the document name, a control that moves the pane to its own tab, and a close control. Dragging a pane header onto another pane rearranges the workspace; dragging it onto the tab strip takes it out again as an ordinary tab. A workspace left with one pane becomes an ordinary document tab. Dividers resize by pointer and keyboard.

### Workspace name

A new workspace is named **Workspace** (then *Workspace 2*, …) and its name is immediately selected for typing. Double-click, a context action, F2, or **Workspace → Rename Workspace** edits it later. Enter commits; Escape cancels. A name is display/session metadata, not a filesystem rename: validate a trimmed, nonempty name with a reasonable length cap, persist it with the workbench session, and never change a document path or recovery ID. The title bar keeps showing the opened folder.

### Documents and closing

The same document may appear in several panes or tabs, but all views refer to one authoritative document session. Closing a pane or a tab closes only those views; the existing Save/Discard/Cancel flow applies if it would release the final view of a dirty document. Grouping, ungrouping, and rearranging never save, close, or duplicate a document.

Keep tab and pane drag distinct from text drag by source element. Provide commands so everything is usable without drag and drop: **Group with Next Tab Side by Side/Stacked**, **Ungroup Workspace**, **Move Pane to New Tab**, **Close Pane**, **Focus Next/Previous Pane**, **Grow/Shrink Pane**, **Next/Previous Tab**, **Move Tab Left/Right**, and **Rename Workspace**. Split Editor remains a shortcut that opens a second view of the active document in a new pane.

### Session

Session schema v2 stores the tabs in strip order, each with its name, pane tree, focused pane, and ratios. Migrate schema-v1 active/secondary split state without losing open paths or view modes. Persist by stable source/recovery identity rather than transient core document IDs. Restore available documents independently, remove only missing panes, and keep every remaining tab usable. Validate size, depth, pane count, references, and ratios before accepting a session; limits are 128 tabs, 16 panes per workspace, and depth 6. User-facing feedback explains a rejected oversized workspace.

Acceptance: group two tabs on each side; build and resize a mixed nested workspace; rename it; take a pane back out; reorder tabs; restart with all available and one missing file; recover an untitled pane; close a workspace holding the last view of a dirty document; and use every operation from the keyboard. Test minimum window size and screen-reader tab/pane names.

## M18 — Desktop controls and sectioned Settings window

### Toolbar icon system

Replace text glyphs and mixed button styles with one coherent, original-feeling vector icon set across the title toolbar, tab actions, editor tools, map/LCI toggles, and menus where icons are useful. Cover at least **New, Open, Close, Save, Save As, Undo, Redo, Wrap, Column Editing, Hex, Find, Previous, Next, Replace, Highlight, Go To, LCI, Map, Browser/Preview, Split Horizontal, Split Vertical, Files, and Settings**. Icons have consistent 20–24 px geometry, line weight, optical alignment, and active/disabled/hover states. The command ID remains the action source; an icon mapping is presentation data, so custom toolbar order and shortcuts continue to work.

Use bundled SVG or an appropriately licensed vector package, with no runtime icon download. Every icon-only control has an accessible name and a tooltip containing its action and shortcut. Active toggles show a visible pressed state. At narrow widths, low-priority controls move into an overflow menu; essential file actions remain reachable and icons never deform when the window resizes. Theme colors and high-contrast mode apply to every state. Do not copy another editor's artwork or Apple-only SF Symbol assets into cross-platform bundles.

Acceptance: inspect all built-in themes and three window widths; verify icon geometry, labels, hover/pressed/disabled states, overflow, keyboard focus, screen-reader names, and command parity for custom toolbar entries.

### Settings window

Open **Settings** as a separate singleton desktop window, reachable through the application menu, keyboard shortcut, command palette, and toolbar. Reinvoking brings the existing window forward. On macOS, use the standard Settings menu placement and a familiar separate-window layout; use an equivalent separate window on Windows and Linux. A left sidebar lists **General, Editor, Appearance & Themes, Languages, Keybindings, Menus & Toolbar, Workspace, and Extensions & Security**. The right side shows only the selected section, with a clear heading, search/filter where useful, and short grouped controls. The sidebar and detail area resize sensibly, and the last selected section and window frame restore within visible display bounds.

Reuse the existing validated customization/settings commands and draft model. Changes with immediate preview are visually marked as pending; **Apply/Save**, **Cancel**, and **Restore Defaults** have consistent meaning. Closing with unapplied changes asks whether to apply, discard, or continue editing. Successful saves update all editor windows and native menus through a native event; simultaneous settings/editor views cannot overwrite a newer customization revision silently. Theme import still uses a narrow native file picker, and the settings webview gets no broad filesystem, shell, or document access. Settings content is scrollable within its detail pane when needed, but the whole window is not one long settings form.

Acceptance: open/focus one settings window from all entry points; navigate every section by keyboard and screen reader; edit and cancel/apply a theme or toolbar choice; test stale revisions and close with pending changes; verify native menu refresh, multi-monitor window restoration, small screens, all supported OSes, and main-editor state remaining intact while Settings is open. Apple describes a separate macOS Settings window in its [settings interface guidance](https://developer.apple.com/documentation/foundation/adding-a-settings-interface-to-your-app) and a leading navigation sidebar in its [sidebar guidance](https://developer.apple.com/design/human-interface-guidelines/sidebars).

## Delivery gates

For each milestone, update `FEATURE_PARITY.md`, `WORK_STATE.md`, IPC contracts and protocol version when DTOs change, product screenshots, and the release checklist. Add focused core tests for byte/revision behavior and frontend interaction tests for pointer/layout/settings behavior. Run the existing full test, lint, typecheck, audit, and bundle checks, then perform manual pointer, IME, keyboard, and accessibility checks on the hosted OS matrix. Do not mark a milestone complete from mock UI alone.
