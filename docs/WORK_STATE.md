# Work state

## Nulis 0.5.0 (plan 4, M41–M48)

Release 0.5.0 (2026-10-24). Automatic indentation, suggestions while typing, a language catalog with per-language capabilities, first-party providers behind a capability registry, broader language coverage, optional language servers with a Problems panel, and declarative language plugins. Scope, decisions, and verification: `IMPROVEMENT_PLAN_4.md`, `IMPLEMENTATION_TRACKER.md` (M41–M48), and `handover/M42_HANDOVER.md` to `M48_HANDOVER.md`; the user-facing summary is `release-notes/v0.5.0.md`. Plan 4 was checked by automated tests only: the in-app walkthrough on macOS, Windows, and Linux, the `rust-analyzer`/`clangd` checks, and CI for earlier milestones in the branch remain to be recorded in `PLATFORM_MATRIX.md`. IPC protocol 47.

## Nulis 0.4.0 (plan 3, M36–M40)

Released 2026-10-23. Print (pages laid out by the core, shown in a Print Preview window, then the system dialog), Page Setup with a live first-page preview, and Export to PDF (whole document, selection, hex dump) with progress, Cancel, and fallback fonts; formatted Markdown printing. Remote (SFTP) was removed. Checked by hand on macOS 15, Windows 11, and Ubuntu 26.04 test VMs; fixes from that check: Linux landscape (passed to WebKitGTK as upright custom paper), Windows' own print header and footer (margins as repeated padding), and Print's error wording. IPC protocol 42. Scope, decisions, and verification: `IMPROVEMENT_PLAN_3.md`, `handover/M36_HANDOVER.md` to `M40_HANDOVER.md`, and `handover/PLAN3_RESUME.md`; the user-facing summary is `release-notes/v0.4.0.md`.

## Nulis 0.3.0 (plan 2, M31–M35)

Released 2026-10-20. Scope, decisions, and verification are in `IMPROVEMENT_PLAN_2.md`, `IMPLEMENTATION_TRACKER.md` (M31–M35 and "Count All on huge files"), and `handover/M31_M32_HANDOVER.md`; the user-facing summary is `release-notes/v0.3.0.md`. In short: the Find window fits its content; one linear-time regex style with unbounded patterns, capture groups in Replace, and Search in column; contrast floors for selection, secondary text, and icons in every theme; YAML and plist Validate and Beautify; CSV/TSV and richer YAML highlighting; binary property lists in the hex editor; Count All without a limit as a background job; plain-text search on the regex engine's literal finder (about 8x faster). IPC protocol 39. Windows and Linux were checked only by CI for this release.

## Windows verification and fixes (implemented locally)

Every package is now built, installed, and started by CI (`.github/workflows/ci.yml`): `Nulis.app` on macOS, the `.deb` and `.rpm` on Linux (the `.rpm` on Fedora in a container), and the NSIS installer on Windows. The same packages were also built in containers and checked by hand on Ubuntu 22.04 and 24.04, Debian 12, Fedora 44, and a Windows 11 virtual machine.

Problems found on Windows and fixed: the plugin host did not compile (a Unix-only import); saving failed with "Access is denied" because `MoveFileExW` cannot replace a file the editor holds open (the save now uses `std::fs::rename`, which asks for POSIX rename semantics); plugin installation flushed files through read-only handles; and the bundled-OpenSSL feature was also building OpenSSL on Windows.

Problems found while using the application there, fixed on every platform: typed characters were dropped when keys arrived faster than the core answered (typed edits are now queued in order, each naming the revision the previous one returned); two running copies shared and overwrote one session and one set of recovery checkpoints (`tauri-plugin-single-instance` now makes a second launch focus the running window); and a window with nothing unsaved could only close through the page (the adapter now closes it natively).

## Self-contained builds (implemented locally)

Remote (SFTP) was removed on 2026-10-22, and with it `ssh2`, libssh2, OpenSSL, and `keyring`, so nothing is compiled from OpenSSL any more. Every build is still self-contained apart from the system web engine.

## Status bar and Read-Only mode (implemented locally, IPC v26)

**Read-Only mode.** `nulis_document::Document` carries a `read_only` flag; `replace`, `replace_many`, `undo`, and `redo` return `DocumentError::ReadOnly` while it is set, so every edit path in the core (typing, replace, transforms, tools, hex edits, compare merges, macros, scripts) is refused at one point. `editor.toggleReadOnly` switches it and returns the summary, which now has `readOnly`. The desktop shows the mode as an EDIT / READ ONLY badge in the status bar and the editor toolbar, a lock on the tab and pane header, an amber toolbar line, and a steady block caret. The viewport refuses typing, Backspace, paste, cut, and undo/redo up front with one clear status message. Primary+Shift+L is the default shortcut. The mode is session state only and a document opens in Edit mode.

**Status bar.** The focused editor reports its caret line and column through `onCaret`; the status bar shows them with caret and selection counts. `editor.characterCount` streams the document in 256 KiB chunks and counts characters in its encoding (UTF-8 sequence starts, or bytes for Windows-1252) for documents up to 64 MiB; the desktop asks for it 250 ms after the revision changes and shows "N bytes · M characters".

## Help window and plugin developer guide (implemented locally)

Help is a singleton window (`help`, `index.html?window=help`) laid out like Settings: a section list with a search field on the left and one chapter on the right. `HelpWindow.tsx` builds the sections from the second-level headings of `docs/USER_GUIDE.md` and inserts `docs/PLUGIN_GUIDE.md` after "Plugins" as "Building Plugins"; both files are imported as raw text, so the repository and the application show the same words. The window has its own capability file (events only) and may call only `customization.state` and `theme.list`, to follow the chosen theme. Outside the desktop shell the same component is an inline dialog. Help ▸ Nulis Help and F1 open the window natively from any window.

`docs/PLUGIN_GUIDE.md` documents the package, every manifest field and rule, the runtime API, permissions, the SDK, bundling with esbuild, the development loop, testing, limits, and troubleshooting with the host's exact error messages. `examples/plugins/nulis-tips` is a TypeScript example with two commands, startup activation, an argument, and a result; `crates/nulis-plugin-host/tests/process_worker.rs` runs it in the real worker.

## Documentation, Help, and license (implemented locally)

The project is licensed `GPL-3.0-or-later`: `LICENSE` holds the unmodified GPL version 3 text, and the Cargo workspace, the npm packages, and the Tauri bundle declare it. Version 3 was chosen because five dependencies require Apache-2.0, which is compatible with GPL version 3 but not version 2; `THIRD_PARTY_LICENSES.md` records this.

`README.md` describes the project, its architecture, how to build and test it, and where it stores data. `docs/USER_GUIDE.md` is the user guide; it is imported as raw text and rendered by `HelpGuide.tsx`, so the built-in help and the repository copy cannot drift apart. `workbench.openHelp` opens it from Help ▸ Nulis Help, F1, and the command palette. The Help menu also holds About Nulis on every platform, replacing the default system About panel. The About page adds the origin of the name, the copyright line, the GPL notice, and a View License button that shows `LICENSE`.

## JSON and XML tools (implemented locally, IPC v25)

`format_tools.rs` adds Validate and Beautify for JSON and XML, and Minify for JSON, through `tools.validate`, `tools.beautify`, and `tools.minify`. They apply to the focused document when its language is JSON, XML, or XSL (so "View As" enables them for a file with another name) and read at most 8 MiB. Validation reports the first problem with a line, character column, and byte offset, and the desktop moves the caret there. Formatting refuses an invalid document, is revision-checked, and is one undo step; an already formatted document produces no revision. JSON keeps key order, number spelling, and string contents exactly. XML changes only whitespace between elements: an element that holds text, or is marked `xml:space="preserve"`, is kept verbatim, and indentation is capped at 256 levels so output stays proportional to input. The XML check is well-formedness only (one root, matched and nested tags, quoted unique attributes, complete references); it does not validate against a DTD or schema and does not check namespaces. Document summaries now carry the language so the workbench can show the Validate/Beautify/Minify toolbar buttons, the Tools menu, and the palette entries only for these documents; the native Tools menu items are renamed and enabled for the focused document.

## Languages, File View, and Close File (implemented locally, IPC v24)

**Languages.** `nulis-syntax/src/lexer.rs` adds a table-driven lexical highlighter. A language is data: comment and string delimiters, keyword and type lists, and a few flags (case-insensitivity, variable sigils, preprocessor lines, `key:`/`key =` keys). Three scanners share it: code, markup (tags, attributes, entities, element folds, skipped `<script>`/`<style>` bodies), and CSS. It adds AppleScript, Bash, Batch, C, C++, C#, CSS/SCSS/Less, Dockerfile, Go, HTML, INI, Java, JavaScript, Kotlin, Lua, Makefile, MySQL/MariaDB, Objective-C/C++, Oracle SQL, Perl, PHP, PowerShell, Ruby, Rust, SQL, Swift, TOML, TypeScript, Visual Basic, VBScript/ASP, XML, XSL, YAML, and Zig, alongside the existing JSON, Python, and Markdown modules. Each produces highlights, bracket pairs, and brace or element folds under the existing span, fold, and byte limits. Detection uses extensions and well-known file names, then the first 256 bytes (`#!` interpreter, XML/PHP prolog, HTML doctype) when the name says nothing. The language picker is sorted by name. These lexers do not parse: there are no document symbols for them, regular-expression literals and heredocs are not recognized, PHP files are lexed as PHP throughout, and a construct that began before the viewport window is not known.

**File View.** The Files side bar has Open, Places, and Folder tabs. Places lists the computer root (named after the boot volume on macOS), other mounted volumes, and opens with the path to the home folder expanded. `places.roots` and `places.list` list one absolute directory at a time with a 1,000-entry cap, directories first, dotfiles hidden unless requested, and symlinks shown as their targets. A filter such as `*.md; *.rs` narrows files, never folders. Folder is the previous opened-folder tree, unchanged.

**Close File.** `workbench.closeFile` closes the file in the focused pane with the usual Save/Discard/Cancel prompt. It is in the default File menu and toolbar and bound to Primary+W; Close Window moved to Primary+Shift+W. The File menu always offers it, even when a customized menu omits it. A customization saved before this change keeps its own toolbar until it is edited or defaults are restored.

## M15–M18 — Editor interaction and workspace improvements (implemented locally)

Specified in `IMPROVEMENT_SPEC.md`. All four milestones are implemented and pass the automated suites on macOS. None is closed: the spec requires each acceptance scenario on macOS, Windows, and Linux plus manual pointer, IME, keyboard, and assistive-technology checks, and those have not been run. IPC protocol moved from v19 to v23.

**M15 — Precise pointer editing (IPC v20).** `editor.selectWordAt` selects a word, whitespace run, or punctuation run from one Unicode rule (alphanumeric or `_` is a word) with a 1 MiB bounded scan; it never splits a grapheme or a CRLF pair, keeps malformed bytes addressable, and resolves a line ending to the adjacent word. Double-click and Primary+D use it; triple-click selects the logical line with its ending. `editor.moveSelectedText` moves or copies the authoritative ordinary selection in one revision-checked batch and one undo step; a stale revision is rejected before anything is removed. The desktop starts a move only from inside a single nonempty selection after a 4 px threshold, previews an insertion caret, auto-scrolls, copies with Option (macOS) or Control, and cancels on Escape or a drop outside the text surface. Moving text between documents is not enabled.

**M16 — Line change indicator (IPC v21).** `line_changes.rs` tracks touched byte ranges (unsaved or saved) in at most 4,096 entries; nothing is scanned at open and memory does not depend on file size. Every edit path already recorded its replacements for bookmark relocation, and the tracker shares that record, so insert, replace, delete, paste, transform, drag move, multi-caret, rectangle, hex, merge, and Replace All all update markers. Save and Save As convert unsaved markers to saved only after the atomic write succeeds. Undo and redo restore the exact prior markers within one save epoch; undoing across a save marks the affected lines unsaved because they then differ from the saved file. Recovery checkpoints carry the unsaved ranges. Viewport lines and the document map report `unsaved`, `saved`, `unsavedDeletion`, or `savedDeletion`, plus an availability/approximation status that the UI shows instead of a clean gutter. The toolbar toggle is a per-document session override; the View menu command and Settings hold the persistent default.

**M17 — Workspace tabs (IPC v22).** Follows `workspace-grouping.mp4`: one tab strip, where a tab is a document or a workspace of split panes. Dragging a tab into the active tab's content previews the half it will take and, on release, merges both into one workspace tab whose name ("Workspace") is selected for typing. Dropping onto a pane of a workspace splits that pane; a pane header can be dragged onto another pane to rearrange or onto the strip to leave the workspace, and each pane has pop-out and close controls. Sessions are schema v2: tabs in strip order, each with a name and a tree of panes and horizontal/vertical splits with a bounded ratio. Limits are 128 tabs, 16 panes per workspace, and depth 6; references are `path:` or `recovery:` identities, never core document IDs. Schema-v1 sessions migrate on load, and a pane that stored several tabs restores as one tab per document. The model in `layout.ts` is pure and tested; `EditorLayout.tsx` renders the strip and panes. Every operation is also a command (Group with Next Tab Side by Side/Stacked, Ungroup Workspace, Move Pane to New Tab, Close Pane, focus/resize pane, next/previous tab, Move Tab Left/Right, Rename Workspace). All views of a document share one core session; closing a pane or a tab releases a document only with its last view and then uses the existing Save/Discard/Cancel flow. A remembered untitled pane returns to its place when its checkpoint is restored. An earlier draft of this milestone used a separate layout-group bar with a tab strip per pane; that was replaced.

**M18 — Icons and Settings window (IPC v23).** `icons.tsx` holds one original 24 px, 1.6 px-stroke vector set bundled with the app; command IDs stay the action source and `COMMAND_ICONS` is presentation data. Icon-only controls carry an accessible name, an action/shortcut tooltip, and a visible pressed state; the title toolbar moves low-priority controls into an overflow menu and keeps file actions and Settings visible. The editor toolbar gained Find, Go To Line, Highlight Matches, and Column Editing. Settings is a singleton second Tauri window (inline in the browser shell) with General, Editor, Appearance & Themes, Languages, Keybindings, Menus & Toolbar, Workspace, and Extensions & Security sections. It edits a draft, marks pending sections, and applies through `customization.saveChecked` at the revision it loaded, so a newer revision is reported rather than overwritten. The adapter restricts that window to eight customization/theme/list commands, denies it document viewports, relays previews and two presentation actions to the editor window, refreshes other windows after a change, and restores the window frame only when it is still on a connected display.

Known gaps against the spec:

- Hosted Windows/Linux runs and all manual checks (pointer, IME, screen reader, high contrast, multi-monitor restoration, native menus) are outstanding.
- Themes do not yet define their own marker colors; markers use each theme's warning, success, and danger colors with distinct shapes.
- Views of one document share the core selection, so the caret is common to them; scroll position is per view.
- Line numbers in the document map ignore lone CR endings, so map ticks can sit on a different row than the gutter in classic-Mac-ending files.
- Rectangular and multi-selection drags keep the existing selection gesture, and text cannot be dragged between documents.
- No product screenshots exist in the repository yet, so none were refreshed.

## Current milestone: M14 — Accessibility, recovery, fuzzing, packaging, and hardening (in progress)

M14 work now includes dirty tab indicators, Save/Discard/Cancel on tab and window close, and startup recovery choices for local and untitled sources. IPC protocol v19 adds a native close command that checks core-owned dirty state before destroying the window. Edits checkpoint to a bounded, versioned piece-table manifest after a short idle period; the append store is synced first. Save rebases the open document onto the newly written file while retaining undo history, so later edits can be recovered without copying a large source. Recovery shows changed-original conflicts without restoring them and rejects invalid app-data paths. Remote documents do not yet have persistent recovery sources.

The document crate has deterministic generated replacement and snapshot tests, plus a separate libFuzzer target and seed corpus. The optional target compiled and completed 10,000 local runs without sanitizer coverage; CI is configured for a 60-second nightly coverage run. Linux plugin workers now set a 512 MiB address-space limit before parsing input; macOS and Windows retain the existing engine and parent-process budgets pending platform-specific OS quotas. CI now uses lockfile installs and has Linux Debian, macOS application, and Windows NSIS bundle jobs. These hosted jobs have not run yet.

Release audits found no npm vulnerabilities. RustSec initially identified an unpatched `fast-float` vulnerability through Boa 0.19; the plugin engine was upgraded to Boa 0.20, which uses `fast-float2`, and plugin worker tests passed. The subsequent RustSec scan reports zero vulnerabilities and four transitive maintenance/soundness warnings for review. Boa 0.20 raises the minimum Rust toolchain to 1.82.

Recent desktop usability fixes: native file drops open through the central `file.open` command; `.py` and Markdown paths select bounded syntax highlighting automatically, with a View As selector for manual overrides; editor text uses the active semantic theme colors; and the optional right-side document map provides bounded whole-file previews and click navigation through IPC protocol v16.

Markdown documents now offer Edit and View modes. View renders the current in-memory source, including unsaved edits, with GitHub-style tables and task lists. Local images resolve relative to a saved document through a bounded native query; remote images use their URL. Preview source is capped at 1 MiB and individual local images at 4 MiB (IPC protocol v17).

## M13 — Customization and complete theme system complete locally

Implemented and locally verified:

- A pinned NvChad Base46 `v3.0` snapshot with all 97 upstream theme sources, both upstream license notices, a reproducible static conversion tool, and a checked-in converted catalog. All 97 themes ship as read-only built-ins alongside the two original Nulis themes; the desktop settings picker exposes them without installation or use of the 64-user-theme quota.

- A complete stable semantic theme vocabulary covering editor, syntax, workbench chrome, inputs, status, terminal, and diff surfaces; both original built-ins now provide every required opaque or alpha hex color.
- A native user-theme catalog with deterministic discovery, explicit install/uninstall, strict JSON and metadata validation, built-in shadow protection, regular-file/symlink checks, a 256 KiB file cap, a 64-theme cap, and exclusive atomic installation below application data.
- A schema-v1 customization document for appearance, portable keybindings, File-menu entries, and toolbar command IDs, with numeric/identifier/chord/uniqueness/layout validation, 64-entry list caps, a 256 KiB document cap, atomic persistence, and reset defaults.
- Core validation that selected themes exist and every configurable action belongs to the explicit safe zero-argument command set or an approved plugin contribution, plus IPC protocol v15 lifecycle commands for customization and themes.
- A searchable settings surface with live appearance preview, user-theme install/removal, editable canonical shortcut chords, ordered File-menu composition, ordered toolbar composition, cancel, validated save, and reset.
- One command dispatch path shared by the palette, portable shortcuts, configurable browser/native File menus, and the configurable title toolbar. The Tauri native-menu adapter independently validates entries and maps `Primary` accelerators to platform conventions.
- Theme propagation through CSS semantic variables and xterm terminal colors, including syntax property/escape/invalid and diff status tokens that previously used hard-coded colors.
- Regressions for customization round trips/reset/duplicate shortcuts, theme validation/install/list/uninstall, core persistence, portable native accelerators, invalid menu layouts, and existing desktop workflows.

Arbitrary CSS themes, remote theme resources, plugin-supplied theme execution, and unsigned theme distribution remain outside the contract. Hosted macOS/Windows/Linux native-menu and accessibility validation continues under M14.

## M12 — Plugin host, permissions, and SDK stabilization complete locally

Implemented and locally verified:

- A typed API-v1 plugin manifest with semantic plugin versions and Nulis engine ranges, namespaced command contributions, `onCommand`/`onStartup` activation events, reserved host namespaces, bounded fields, and strict unknown-field rejection.
- Symlink-safe folder installation that copies only `plugin.json` and one UTF-8 JavaScript entry point into application data, refuses implicit overwrite, and supports exact validated uninstall.
- SHA-256 package identities with a separate atomically persisted approval store. Grants can never exceed manifest requests and apply only to the exact manifest/source digest, so package changes automatically remove effective approval.
- A fresh child process for every activation or command invocation, using the Nulis executable's dedicated worker mode and a bounded single-request JSON protocol. Cancellation, timeout, crash, oversized output, malformed protocol data, and unsuccessful exit all terminate or reject the worker.
- A pinned Boa 0.19.1 JavaScript evaluator with no Node, Web, native IPC, module-loading, filesystem, network, process, environment, credential, terminal, remote, or clipboard API. Loop, recursion, stack, source, argument, operation, message, command, response, and wall-time limits are enforced.
- An API-v1 runtime for asynchronous activation, asynchronous command handlers, disposable command registration, and requested `showMessage` operations. Runtime registrations must exactly match the manifest and every host operation is independently authorized by the persisted grant.
- Central plugin commands for list/install/uninstall/approve/revoke/startup activation/cancellation, plus approved contributed-command routing through the same headless command boundary. IPC protocol v14 mirrors bounded plugin summaries and outcomes.
- A desktop Plugins manager showing package revision, contributions, requested permissions, and approval state, with explicit install/approve/revoke/uninstall controls. Approved contributions refresh into the searchable command palette; approved startup plugins activate once after session restoration.
- A stabilized TypeScript SDK with literal-preserving manifest validation helpers and the exact executable API-v1 surface, plus an installable Hello Nulis package with typed source and a self-contained JavaScript artifact.
- Regressions for package traversal, namespaces, digest invalidation, excessive grants, stable catalog state, install/no-overwrite/uninstall, async command execution, operation collection, runaway loops, missing commands, process cancellation, JSON worker transport, core contribution visibility, desktop approval controls, startup activation, and palette dispatch.

OS-enforced worker memory quotas, signed packages, and a marketplace remain explicitly deferred to M14 hardening; M12 keeps plugin code out of the native core address space and grants no ambient authority.

## M11 — Macro recording/playback and sandboxed scripting complete locally

Implemented and locally verified:

- A cross-milestone editor usability pass with visible File-menu Save/Save As commands, standard keyboard shortcuts, native destination selection, streamed fingerprint-checked atomic core writes, and IPC protocol v12.
- A persisted Appearance settings panel with live built-in dark/light themes, editor font family and size, line height, ligatures, and matching virtual-row geometry.
- A schema-v1 recorded macro model capped at 1,024 actions and 1 MiB aggregate data, with namespaced command IDs, 256 KiB per-action argument budgets, and a maximum JSON nesting depth of 32.
- Serial macro playback through the existing headless command registry, preserving command context, stopping on the first indexed failure, retaining no command-result payloads, and checking cancellation before every action.
- A single active bounded recorder that captures only an explicit allowlist of editor, hexadecimal, language, and replacement commands after successful execution. The same allowlist is rechecked at playback so tampered persisted data cannot invoke terminal, remote, credential, file-open, workspace, or automation authority. Overflow preserves prior actions, marks the recording failed, and requires cancellation rather than silently saving a partial macro.
- Revision-safe recording replaces observed `expectedRevision` values with a reserved current-revision placeholder. Playback resolves it immediately before each action through the authoritative document manager, preserving stale-write protection without baking old revisions into saved macros.
- An atomic application-data macro library capped at 128 macros and 1 MiB total, with validation on every load/save, stable ID ordering, exact lookup/deletion, synced temporary replacement, and parent-directory sync on Unix.
- IPC protocol v11 automation commands plus a non-modal desktop Macros panel for start, stop/save, cancel, list, run, and delete. Playback returns updated document metadata so bounded editor views refresh against the new revision.
- A deny-by-default script capability contract for exact command allowlists, document/workspace/settings/message host operations, and explicit memory, execution-time, host-call, output, source, and allowlist ceilings.
- A versioned script-definition request that cannot grant itself authority, plus a pinned Rhai 1.26.1 evaluator created from a raw engine without a standard package or module resolver. Modules, arrays, maps, closures, custom syntax, and script-defined functions are compiled out.
- Eight explicit `nulis_*` entry points for bounded JSON command/document/workspace/settings/message calls and output. Every host attempt consumes a call budget, rechecks its capability, and requires an exact command grant where applicable. The evaluator itself has no ambient filesystem, network, process, environment, credential, terminal, or remote capability.
- Deterministic operation fuel, elapsed-time interruption, thread-safe cancellation, a host execution guard, memory-derived string limits, bounded variables/expression depth/string interning, and exact output/JSON budgets.
- A separately persisted script library capped at 64 entries and 4 MiB. Definitions and grants remain separate, every grant is validated as a subset of the request, and any definition change automatically revokes approval.
- Capability-specific core adapters for bounded current-document reads/edits, symlink-safe workspace listing, atomic replacement of existing workspace files, effective settings, collected messages, and exact calls into the macro-safe editor-command set. Terminal, remote, credential, file-open, session, and automation commands remain excluded.
- IPC protocol v13 commands for list/get/save/delete, approve/revoke, run, and cooperative cancellation, plus a desktop Automation panel for script editing, visible permission requests, explicit approval, output/messages, and lifecycle controls.
- Regressions for unknown macro fields, argument byte/depth bounds, recorder overflow, atomic macro/script persistence, approval revocation after edits, live-revision replay, cancellation, stop-on-failure ordering, desktop recording/playback/script approval, deny-by-default authorization, excessive grants, exact command grants, unsafe command exclusion, bounded JSON, host-call/output/operation exhaustion, module rejection, and cross-thread script cancellation during both evaluation and host calls.

## M10 — Remote providers, credentials, and terminal complete locally

*Remote (SFTP) and its credential storage were removed on 2026-10-22; the terminal remains. The notes below are the record of M10 as built.*

Implemented and locally verified:

- A capability-scoped read-only SFTP provider with validated metadata-only profiles, normalized relative paths, 1,000-entry directory caps, explicit symlink exclusion, stable directory-first results, connection timeouts, and mandatory pinned SHA-256 host-key verification before authentication.
- Platform credential storage through `keyring`, serialized native-store access, 64 KiB secret limits, opaque credential identifiers in profile JSON, and no password persistence in settings, sessions, or profile metadata.
- Remote files exposed as synchronized random-access sources behind the existing piece table, with source bytes staying native and only bounded text/hex viewports crossing IPC.
- Native pseudoterminal sessions using direct program/argument arrays, platform-default shells, bounded argument/environment/input/read limits, a 16-session cap, 1 MiB cursor-addressed output rings, resize support, exit polling, and child termination on close/drop.
- A Remote sidebar for profile creation, keychain password updates, SFTP navigation, and opening remote documents, plus a lazily loaded xterm.js terminal panel with fit, keyboard input, bounded polling, resize, and lifecycle cleanup.
- IPC protocol v10 remote/profile/credential and terminal commands, remote document URIs, persisted Remote sidebar selection, and a Tauri allowlist that still exposes no shell/process, filesystem, or network plugin authority to the webview.
- Regression coverage for profile validation and metadata persistence, credential capability separation, path traversal rejection, terminal output eviction/cursors, direct PTY execution, and both command boundaries.

## M09 — Diff, merge, binary, and folder compare complete locally

Implemented and locally verified:

- A storage-independent `nulis-diff` model with lossless 64-bit byte ranges, bounded exact line-window LCS, fixed-chunk binary comparison, coalesced insert/delete/replace runs, explicit continuation cursors, and hard byte/line/change caps.
- Text and binary compare pages over the existing authoritative piece tables, including raw-byte differences beyond 5 GiB without source materialization.
- Bidirectional per-hunk merging capped at 8 MiB, guarded by the target revision, integrated with selection/bookmark relocation, and recorded as one undo/redo action.
- A desktop compare surface for any two open documents with Text/Binary modes, bounded previous/next window history, lossless offset feedback, and merge arrows in either direction.
- Deterministic content-aware folder comparison with 64 KiB reads, 10,000-entry traversal caps, a 5,000-result native cap, a 1,000-result desktop request, and explicit identical/compared/skipped/truncated counts.
- Folder comparison selected through native dialogs without webview filesystem authority; recursive traversal skips symlinks and never opens special files.
- IPC protocol v9 compare, merge, and folder DTOs plus regressions for line alignment, byte-run coalescing, deep 64-bit offsets, undoable merging, result bounds, folder content equality, and symlink exclusion.

## M08 — Workspace, explorer, palette, layouts, and sessions complete locally

Implemented and locally verified:

- A core-owned canonical workspace root with safe relative-path validation, component-by-component symlink rejection, centralized open/current/list/open-file/close commands, and no general filesystem authority in the webview.
- Lazy one-directory explorer loading with a 1,000-entry native hard cap, a 500-entry desktop request, a 256-loaded-directory UI cap, deterministic directory-first sorting, truncation feedback, visible non-traversable symlinks, and source files routed through `workspace.openFile`.
- A searchable keyboard-accessible command palette populated from centralized command descriptors, with file, workspace, undo/redo, sidebar, split, Text/Hex, and About actions.
- Resizable and hideable sidebar layout, activity-panel switching, split composition, and per-document Text/Hex mode integrated with existing bounded editor views.
- Schema-v1 workbench sessions capped at 1 MiB and 128 source paths, validated before save/load, durably atomically replaced below application data, and represented through IPC protocol v8.
- Startup restoration for workspace, source-backed tabs, active and secondary documents, split state, sidebar visibility/width, active panel, and view modes; missing paths fail independently rather than invalidating the whole session.
- Regressions for traversal and intermediate symlinks, bounded/sorted listing, session validation/round trips, headless command dispatch, lazy explorer expansion, workspace file opening, palette execution, and layout/session restoration.

## M07 — Scalable hexadecimal editing complete locally

Implemented and locally verified:

- A storage-independent `nulis-hex` layout and edit layer with validated 8–64-byte rows, 64-bit alignment, nibble replacement, and bounded hexadecimal clipboard parsing.
- Raw-byte viewport queries that bypass text decoding, align arbitrary offsets to rows, read at most 64 KiB, and expose lossless decimal-string coordinates through IPC protocol v7.
- File-backed hexadecimal reads beyond 5 GiB using the existing positional piece-table path without source materialization.
- Revision-checked high/low-nibble typing, byte insertion/overwrite, selection replacement, exact one-byte backward deletion, bounded hexadecimal copy/cut/paste, undo/redo, and bookmark relocation.
- A per-document Text/Hex mode with synchronized hexadecimal and ASCII columns, byte selection, keyboard navigation, offset jump, bounded page history, and 4 KiB virtualized pages.
- Deterministic regressions for row alignment, malformed clipboard input, nibble preservation, insert/overwrite behavior, selection replacement, stale revisions, deep sparse-file reads, desktop dispatch, and a 10 GiB logical-document DOM bound.

## M06 — Modular languages and Tree-sitter complete locally

Implemented and locally verified:

- A storage-independent `nulis-syntax` language registry with validated stable IDs, deterministic filename/extension detection, duplicate rejection, plain-text fallback, and declarative Tree-sitter module metadata.
- A pinned Tree-sitter 0.24 integration and built-in JSON grammar with stable comment, keyword, string, number, property, escape, and invalid token classes.
- Explicit 256 KiB input, 25 ms parse, 100,000 node, 20,000 highlight, 2,000 fold, 2,000 symbol, and 20,000 bracket-pair limits; timeout and truncation are represented in results.
- Viewport-scoped parsing that never retains a full-document syntax tree or makes syntax memory proportional to document size.
- Automatic language detection for new and opened documents plus centralized language list/override commands.
- IPC protocol v6 syntax DTOs with lossless decimal-string coordinates for highlights, folds, symbols, and bracket pairs.
- Desktop token rendering, matching-bracket feedback, clickable and keyboard-accessible viewport folds, bounded symbol navigation, and a document-language override.
- Regression coverage for module detection, malformed-tree recovery, hard budgets, core viewport integration, command dispatch, token rendering, bracket matching, folding, symbols, and language override.

## M05 — Rectangular editing complete locally

Implemented and locally verified:

- A constant-size core rectangular-selection descriptor with byte corners, logical lines, and encoding-aware scalar columns; rectangles never expand the authoritative 1,024-entry normal selection vector.
- On-demand line-local range derivation through bounded piece-table reads with a 100,000-row hard cap.
- Atomic rectangular insert, overwrite, delete, backward delete, cut, and Unicode transforms as one revision and one undo/redo action.
- Bounded rectangular clipboard extraction and row-distributed paste, including CRLF/LF/CR normalization and explicit row-count validation.
- Virtual-space insertion that pads only short target lines and remains subject to the existing 1 MiB aggregate edit cap.
- Alt-Shift pointer drag, local block preview, virtual-column placement, cross-page continuation, and viewport-only rectangle rendering.
- IPC protocol v5 rectangle DTOs with lossless decimal-string coordinates and core/UI regressions for constant-size state, atomic edits, virtual-space padding, row-distributed paste, rendering, and command dispatch.

## M04 — Streaming find and replace complete locally

Implemented and locally verified:

- A `nulis-search` engine over the public random-access byte interface using 64 KiB primary windows and bounded overlap.
- Cross-window literal search, ASCII-insensitive literal matching, finite-width multiline regular expressions, cancellation, bounded result pages, and anchor-safe surrounding context.
- Explicit 64 KiB pattern, 1 MiB regex-match, 10,000 result, and 8 MiB replacement-plan limits; unbounded regexes fail instead of degrading memory guarantees.
- Forward and backward wraparound find commands that retain lossless byte coordinates and update the authoritative core selection.
- Revision-checked Replace All as one descending piece-table batch, one revision, and one undo/redo action, including bookmark relocation.
- Recursive find in files with no symlink traversal, deterministic traversal, 10,000-file and 2,000-result hard caps, skipped-file accounting, and decimal-string offsets.
- A Ctrl/Cmd+F and Ctrl/Cmd+H desktop find/replace surface with next, previous, case, regex, Replace, and Replace All controls.
- A folder-picker Search sidebar whose results open through the centralized file command and reveal the match through a bounded viewport.
- IPC protocol v4 search DTOs and regression coverage for chunk boundaries, pagination, multiline regex, anchor context, wrapping, atomic replacement, recursive file search, and desktop dispatch.

## M03 — Normal editing complete locally

Implemented and locally verified:

- A hard-capped 1,024-selection model with stable deduplication and per-caret preferred columns.
- Atomic, non-overlapping batch replacement in the piece table. A multicaret fan-out advances one revision and is one undo/redo action.
- Multicaret insert, overwrite, backward delete, selection delete, horizontal movement, vertical movement, and newline-joined clipboard reads.
- Unicode uppercase, lowercase, and whitespace-trim transforms over multiple selections with bounded input/output and explicit malformed-UTF-8 rejection.
- Up to 1,024 byte-coordinate bookmarks per document, lossless decimal-string IPC transport, edit relocation, undo/redo relocation, clear, and wrapping previous/next navigation.
- Regression coverage for batch coordinate semantics, overlap rejection, undo/redo after length-changing edits, multicaret placement, transforms, and bookmark history.
- Closable multi-document tabs and side-by-side split views with a document selector per secondary pane.
- Alt-click multicaret creation, multiple selection/caret rendering, Escape collapse, per-caret movement, and bounded DOM behavior.
- Desktop controls and shortcuts for transforms, bookmarks, insert/overwrite mode, and soft wrapping.
- IPC protocol v3 source-byte-width mappings so rendered scalars retain exact document coordinates across malformed UTF-8 and Windows-1252.
- Lossless UTF-8 and Windows-1252 interpretation controls with encoding-aware input, navigation, clipboard extraction, and transforms. Existing source bytes are never rewritten merely by changing the interpretation.

## M02 — Editor viewport complete locally

Implemented and locally verified:

- Opaque core-owned document sessions with 64-bit byte coordinates and a multi-range selection model.
- Central `file.new`, replace, delete-backward, selection, undo, and redo commands.
- Optimistic revision validation that rejects stale UI mutations.
- IPC protocol v2 with a 256 KiB maximum viewport query and explicit long-line continuation cursors.
- Lossless decimal-string transport for all 64-bit IPC coordinates and revisions, with bounded `bigint` UI arithmetic.
- A React editor surface that holds only one bounded snapshot, virtualizes visible rows, renders line numbers/caret state, and dispatches ordinary keyboard, paste, undo/redo, and IME composition input to core commands.
- UTF-8-aware backward deletion and CRLF-safe viewport page boundaries.
- Native file selection routed through the centralized `file.open` command into a metadata-only `FileSource` and persistent disk add buffer under the platform application-data session directory.
- Bounded forward/backward viewport page history without retaining prior viewport contents.
- UTF-8 scalar-aware horizontal caret movement, shift-extension, and pointer placement based on measured rendered text.
- Core-owned vertical caret movement over CRLF, LF, and legacy CR endings, using fixed 64 KiB reads and a retained UTF-8 scalar column across shorter lines. Invalid bytes remain individually navigable.
- Bounded text copy/cut through an 8 MiB core selection query and a write-only native clipboard capability. Cut writes the clipboard before a revision-checked document deletion.
- Insert/overwrite mode with a visible mode control and `Insert`-key toggle; overwrite replaces Unicode scalars without consuming line endings.
- Byte-accurate pointer drag selection across visible lines, including line-ending highlights and shift-extension. Drag previews stay viewport-local and issue one authoritative selection command on release.
- A bounded sparse navigation index of at most 8,192 observed line starts, invalidated after edits.
- Arbitrary byte reveal and one-based line lookup using fixed-size scans from the nearest checkpoint.
- Automatic viewport replacement when keyboard movement takes the caret off-screen, including continuation windows for extremely long lines.
- Pointer drag auto-scroll within a page and drag continuation across bounded viewport pages without retaining page text.
- Page Up/Page Down keyboard paging.
- IME composition guards for synchronous composing state, key code 229, and duplicate post-composition input, plus a fallback input path for non-keyboard text input.
- Keyboard and screen-reader semantics with editor instructions, shortcut metadata, live line/column/selection status, reduced-motion caret behavior, and forced-color selection/caret styles.
- A deterministic 10 GiB logical-document UI test proving that a 20,000-line bounded snapshot renders no more than 40 editor rows.

M02 is complete locally. Manual VoiceOver, NVDA, Narrator, Linux screen-reader, and native IME matrices remain M14 production-hardening work, as do hosted Windows/Linux validation and interactive frame-time measurements.

## M01 — Document engine complete locally

Implemented and locally verified:

- Positional `FileSource` reads that open from metadata without source materialization.
- Memory and persistent disk append stores.
- Deterministic implicit-treap piece table with 64-bit aggregate lengths, bounded reads, inserts, deletes, replacements, and streaming output.
- Byte-budgeted undo/redo with deterministic eviction.
- Cancellable sparse CRLF/LF/CR line indexing with checkpoint-based refresh after edits.
- Bounded UTF-8 viewport decoding with malformed-byte replacement and source-byte/scalar mapping.
- Cross-platform safe-save pipeline using same-directory secure temporary files, length validation, sync, concurrent-change detection, replace/no-clobber persistence, and Unix directory sync.
- Versioned recovery manifests with add-buffer-before-manifest durability ordering, source fingerprint validation, and exact piece-table reconstruction.
- Repeatable sparse corpus generation and release benchmark probe. The first 10 GiB local run used about 1.82 MiB peak RSS.
- Tests against a byte-vector mutation oracle, a virtual source beyond 4 GiB, a sparse file beyond 5 GiB, sequential tree balancing, disk append reopen, interrupted save, concurrent modification, recovery reconstruction, and direct piece-table streaming save.

Optional disk spill for evicted undo entries is intentionally deferred; the required bounded byte-budget history is implemented. Hosted Windows/Linux CI remains necessary to validate platform behavior outside the local macOS environment.

## M00 — Foundation complete

Implemented and locally verified:

- Rust workspace and explicit subsystem crates.
- Headless command registry and layered settings model with unit tests.
- Versioned Rust/TypeScript IPC DTO boundary.
- Tauri 2 desktop adapter with IPC payload validation and a minimal capability allowlist.
- React/Vite original Nulis shell; UI dispatches centralized commands.
- Data-driven original dark and light themes.
- Plugin manifest/SDK contract and example plugin.
- Clean-room, product, architecture, security, performance, plugin, theme, parity, and decision documentation.
- Original Nulis vector mark and generated desktop/mobile icon set.
- Cross-platform CI definition for Linux, macOS, and Windows.

## Verification

M13 changes verified successfully on macOS x86-64 on 2026-10-13:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` (159 unit/integration tests passed; all doc tests passed)
- `cargo build --workspace`
- `npm run lint`
- `npm run typecheck`
- `npm run test` (49 frontend/SDK tests passed)
- `npm run frontend:build --workspace @nulis/desktop`

The preceding full release sweep on 2026-10-12 also completed:

- release `nulis-benchmarks` probe against a generated sparse 10 GiB source (1,908,736-byte peak RSS)
- `npm audit` (0 known vulnerabilities)
- `tauri build --bundles app` (release binary and 15.16 MiB `Nulis.app` produced)
- JSON parsing for all checked-in JSON documents

The default all-target Tauri bundle command built the release binary and `.app`, then failed in the managed environment's DMG helper. The explicit application bundle succeeded. A hosted CI run is still required to validate Linux and Windows. Git was initialized successfully after managed-environment approval.

## Next

Continue M14 with hosted platform and assistive-technology validation, macOS/Windows worker memory limits, release signing/notarization when credentials are available, and review of generated installers.
