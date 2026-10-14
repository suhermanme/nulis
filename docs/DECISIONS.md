# Architecture decision record

## ADR-0001 — Rust headless core with a thin Tauri adapter

**Status:** Accepted (M00)

The core is a Rust workspace independent of Tauri. Tauri commands translate versioned DTOs and delegate to `nulis-core`. This permits alternative UIs, core tests without a webview, and a narrow native security boundary.

## ADR-0002 — Byte-addressed storage with 64-bit offsets

**Status:** Accepted (M00)

Canonical storage is bytes and every persistent file position is `u64`. Text character/grapheme/line mapping is derived through encoding-aware indexes. This preserves malformed input, supports binary editing, and avoids 32-bit limits.

## ADR-0003 — Piece tree over immutable source and append-only add storage

**Status:** Implemented in M01

Use a deterministic implicit treap with aggregate byte lengths over immutable original and append-store pieces. File-backed sources expose positional bounded reads; opening reads metadata only. Piece-local newline aggregates may be added after the sparse index measurements show whether maintaining them on every edit is worthwhile.

## ADR-0009 — Safe replacement uses reviewed platform primitives

**Status:** Implemented in M01

Safe-save output uses an exclusively created sibling temporary, output-length validation, file sync, a second concurrent-change check, and `tempfile` persistence. Existing destinations use atomic replacement (`rename` on Unix and `MoveFileExW` with replacement on Windows); new destinations use a no-clobber path. Nulis never implements replacement as delete-then-rename. `tempfile` 3.x is MIT/Apache-2.0 and supports the workspace's Rust baseline.

## ADR-0010 — Recovery stores public piece snapshots after add-buffer sync

**Status:** Implemented in M01

Recovery manifests contain versioned source ranges, source identity, and append-store metadata rather than private treap nodes or copied source bytes. The append store is synced before the manifest is atomically replaced. Reopen validates source identity and all ranges; an append tail not referenced by the last committed manifest is allowed and can be reclaimed later.

## ADR-0011 — UI reads bounded revisioned viewport snapshots

**Status:** Implemented as the M02 foundation

The desktop UI receives opaque document IDs and byte-capped viewport snapshots, never the complete authoritative document. Mutation commands carry the observed revision and fail when stale. Viewport cursors include byte, line, and long-line continuation state. Selection state is a vector of independent anchor/active byte ranges even while the initial UI exposes one caret. JSON represents every 64-bit coordinate and revision as a decimal string; TypeScript uses `bigint` for arithmetic and never narrows it to `number`.

## ADR-0012 — Native file selection is narrow and storage remains core-owned

**Status:** Implemented in M02

The desktop uses the official Tauri dialog plugin only to select open and save paths and grants only its open/save-dialog permissions. Selected open paths pass to centralized `file.open`; selected save destinations pass to `file.saveAs`, while `file.save` reuses the core-owned source path. The Rust core opens immutable sources without materializing them, creates append stores in the platform application-data session directory, and performs fingerprint-checked atomic saves. The webview receives bounded decoded viewport data and path selections, not general filesystem authority or whole-file content. Version 2.2.0 of the dialog plugin is pinned because it supports the workspace Rust baseline; upgrades require an explicit toolchain compatibility review.

## ADR-0013 — Clipboard transfer is bounded and cut is copy-before-delete

**Status:** Implemented in M02 for the primary text selection

The core returns at most 8 MiB of selected text together with the revision it observed. The UI receives only native text-write permission; it cannot read or clear the system clipboard and has no image/HTML clipboard capabilities. Cut first completes the clipboard write and only then requests deletion against the returned revision. A failed clipboard write leaves the document unchanged, while an intervening edit causes stale-revision rejection instead of deleting unexpected content. Invalid UTF-8 is surfaced explicitly when represented with replacement characters.

## ADR-0014 — View navigation uses bounded observed checkpoints

**Status:** Implemented in M02

Document sessions retain at most 8,192 known line-start coordinates gathered from bounded snapshots. Arbitrary byte reveal and one-based line lookup scan from the nearest known coordinate with a fixed 64 KiB buffer; edits clear the coordinates. A complete per-line table is deliberately excluded because it would make memory proportional to line count. The tradeoff is that a first random jump into an unvisited region can take linear time while remaining bounded in memory; future background indexing may add cancellable acceleration without changing the viewport API.

## ADR-0015 — Streaming regex requires a finite match bound

**Status:** Implemented in M04

Literal and regex search use 64 KiB random-access windows with only pattern-sized overlap. A regex is accepted only when static analysis proves a finite maximum match width no larger than 1 MiB. This supports bounded multiline expressions while rejecting unbounded quantifiers that could otherwise require retaining an arbitrarily large candidate or produce incorrect results at a window boundary. Search result pages, recursive file counts, Replace All match counts, and replacement bytes each have explicit caps. Regex anchors receive surrounding source context so chunk edges do not become false document or line boundaries.

## ADR-0016 — Rectangles remain descriptors, not expanded selections

**Status:** Implemented in M05

Rectangular selection state stores two byte corners plus logical lines and scalar columns in one fixed-size descriptor. Line-local byte ranges are derived only for rendering or an editing operation and are capped at 100,000 rows; they never become authoritative entries in the 1,024-element normal selection vector. This keeps idle selection state constant-size for extremely tall blocks while allowing atomic piece-table batches, row-distributed paste, encoding-aware columns, and virtual-space padding. Display-cell semantics for tabs remain a future renderer enhancement; the authoritative column unit is a decoded scalar.

## ADR-0017 — Syntax trees are viewport-scoped and language modules are declarative

**Status:** Implemented in M06

Tree-sitter parses only the byte-capped viewport window and the resulting tree is discarded after bounded highlights, folds, symbols, and bracket pairs are extracted. Nulis does not retain a full-document parse tree because that would make syntax memory proportional to file size and complicate edits across multi-gigabyte documents. Language modules register detection metadata and declarative node-kind mappings behind `nulis-syntax`; storage and UI code know only stable language IDs and token DTOs. JSON is built in to exercise the contract, while plain text remains a zero-parser fallback. The known tradeoff is window-local structural context, accepted in favor of deterministic resource limits.

## ADR-0018 — Workspace traversal and session persistence remain core-owned

**Status:** Implemented in M08

The desktop may choose a workspace root through the native dialog, but enumeration and file resolution belong to `nulis-filesystem` behind centralized commands. Requests use relative paths, reject roots and parent components, check every component for symlinks, and return one bounded sorted directory at a time. This preserves the narrow webview boundary and prevents the explorer from acquiring ambient filesystem access.

Workbench composition is persisted separately from authoritative document/recovery state. A versioned, size-capped session records only the workspace root, source-backed open paths, active/split paths, sidebar layout, panel, and view modes. It is validated and atomically replaced below application data. Opaque document IDs are never persisted; startup reopens each path and constructs a fresh mapping. Untitled recovery remains the responsibility of the durable recovery system rather than this layout snapshot.

## ADR-0019 — Comparison is paged and merge reuses document transactions

**Status:** Implemented in M09

Text diff is exact within explicitly bounded line windows rather than retaining a full-document edit graph. Binary diff and file equality stream fixed-size chunks. This preserves source-size-independent memory and makes incomplete comparison explicit through continuation cursors; the accepted tradeoff is that alignment is recalculated at page boundaries. Compare coordinates stay byte-native and lossless across IPC.

A merge hunk is not a second mutation system. The core reads a capped source range, verifies the target revision, and applies one ordinary piece-table replacement so undo/redo, bookmark relocation, selection state, and navigation invalidation remain consistent. Folder traversal is separately capability-owned by `nulis-filesystem`, is deterministic and capped, and never follows symbolic links.

## ADR-0020 — Automation is bounded data before executable code

**Status:** Accepted for M11

Recorded macros are versioned command-and-JSON sequences, not source code. Validation caps action count, aggregate bytes, per-action arguments, and JSON depth before playback. Only an explicit recordable-command allowlist is captured after successful command execution and the same allowlist is enforced again during playback, preventing tampered storage from acquiring unrelated command capabilities. Observed editor revisions become a reserved placeholder that is resolved from authoritative document state immediately before replay, so macros do not weaken stale-write protection. The library is atomically persisted below application data with 128-macro and 1 MiB total caps. Playback invokes the existing command registry serially with one inherited context, checks cancellation between actions, stops at the first failure, and does not retain command results.

Scripts request capabilities but receive only a separate host-created grant. The initial grant model is deny-by-default, command execution requires an exact allowlist match, and resource budgets cover source, memory, wall time, host calls, and output. The contract intentionally defines no ambient filesystem, network, process, environment, credential, terminal, or remote access. A runtime may be selected only if it can enforce these limits and route every host operation through the grant.

## ADR-0021 — Sandboxed scripts use a reduced raw Rhai engine

**Status:** Accepted for M11

Nulis pins Rhai 1.26.1 and creates only `Engine::new_raw`, without a standard package or module resolver. Cargo features compile out modules, arrays, maps, closures, custom syntax, and script-defined functions. Scripts retain variables, conditions, loops, primitive operators, bounded strings, and eight explicitly registered `nulis_*` functions; structured host arguments and results cross as bounded JSON strings. This is intentionally smaller than general-purpose Rhai and keeps native authority visible at registration sites.

Each run validates that the host-created grant is a subset of the script request, applies deterministic operation fuel plus elapsed-time and cancellation checks, and caps variables, expression depth, interned strings, string size, host calls, JSON depth/bytes, and output. A host adapter receives the same execution guard so it can cooperatively stop bounded native work. The structural-memory budget determines the maximum string payload; removing collections and script functions prevents recursive data or call-stack amplification. Persistence keeps definitions and approvals separate, definition changes revoke approval, and core adapters expose only bounded document/workspace/settings/message operations plus the audited macro-safe command set.

## ADR-0022 — Plugins use disposable JavaScript worker processes

**Status:** Accepted for M12

Nulis executes API-v1 plugins in a fresh child process per activation rather than loading third-party native code or evaluating it in the long-lived core. The desktop executable doubles as the worker entry point, avoiding a separately packaged sidecar while keeping the GUI/webview and core orchestration out of the plugin process. A pinned Boa 0.19.1 engine is used without its runtime layer, so the JavaScript realm receives no Node, Web, module-loader, filesystem, network, process, environment, or native IPC facilities.

Packages are folders containing a strict manifest and one self-contained JavaScript entry. Installation copies only those validated files. User grants are persisted separately against a SHA-256 digest of both files; a byte change invalidates approval. Contributions are visible only while approved, runtime command registrations must exactly equal manifest declarations, and returned host-operation requests are authorized again at the core boundary. The first stable permission is deliberately narrow (`showMessage`). Process death, parent-enforced deadlines/cancellation, engine limits, and bounded JSON isolate failures; OS-level per-worker memory quotas and signed distribution remain M14 work.

## ADR-0023 — Customization is validated command data, not UI-specific behavior

**Status:** Accepted for M13

Appearance, keybindings, File-menu entries, and toolbar entries live in one schema-versioned native document. Shortcuts use a portable `Primary`/`Shift`/`Alt` grammar and every configurable action remains a command ID resolved against current core descriptors. The browser menu, macOS native menu, toolbar, and keyboard dispatcher project the same document, preventing separate shortcut or menu action paths from drifting. Storage is size/count bounded, strictly validated, and atomically replaced.

Themes likewise remain declarative immutable data. A fixed complete semantic vocabulary avoids partially themed surfaces and runtime fallback ambiguity. User installation accepts only explicitly selected, bounded regular JSON files, rejects built-in shadowing and duplicate IDs, and copies validated data under application control. Arbitrary CSS, scripts, imports, URLs, and plugin execution are not part of the theme contract.

## ADR-0004 — Commands are the application action boundary

**Status:** Accepted (M00)

Menus, shortcuts, palette entries, macros, and plugins invoke registered commands. The registry validates globally unique IDs and exposes metadata independently from handlers.

## ADR-0005 — Layered JSON settings with schema versions

**Status:** Accepted (M00)

Settings merge in order: defaults, user, workspace, language. Each persisted document carries a schema version and migrations run before values enter the store. JSON is chosen for frontend tooling and editable representation.

## ADR-0006 — Versioned JSON-compatible IPC contracts

**Status:** Accepted (M00)

M00 uses serde DTOs mirrored in a single TypeScript contract module, with an explicit protocol version and contract tests. A generated schema is planned before the contract grows substantially.

## ADR-0007 — Plugins begin out-of-process/capability-first

**Status:** Accepted as design direction

The API uses manifests, activation events, permissions, and serializable messages. Native in-process plugins are excluded from the default trust model. An embedded script runtime may be used only with explicit host capabilities and resource budgets.

## ADR-0008 — Centralized identity

**Status:** Accepted (M00)

Human name, package names, and `com.nulis.editor` live in root/package/Tauri configuration and the core application descriptor. Platform-specific code consumes these values rather than scattering identifiers.

## ADR-0009 — Line change markers reuse theme status colors

**Status:** Accepted (M16), to be revisited

The line change indicator needs distinct unsaved, saved, and deletion markers. Adding three required theme keys would invalidate every installed user theme and the 97 converted built-ins. Markers therefore map to each theme's existing `ui.danger` (unsaved), `ui.success` (saved), and `ui.danger` (deletion) colors through the `--lci-*` CSS variables. Unsaved and saved lines are both thin solid bars, red then green as in UltraEdit, by user request; deletions are a notch. Because the two bars differ only in color, the state is also given in each marker's tooltip and in the status bar, and forced-colors mode keeps a solid versus outlined bar. Dedicated optional theme keys can be added later without breaking stored themes.

## ADR-0010 — Settings is a second window with a per-window command allowlist

**Status:** Accepted (M18)

Tauri application commands are callable from every window. Rather than give the Settings webview the editor's boundary, the adapter checks the calling window's label on each command: Settings may run eight customization, theme, and listing commands and nothing that reads or changes documents, the workspace, the terminal, or remote profiles. Its frame is persisted and bounds-checked natively so the webview needs no window-management permissions.

## ADR-0011 — Most languages are lexer tables, not grammars

**Status:** Accepted

Broad language coverage through Tree-sitter would add a pinned grammar crate per language, several megabytes of generated parsers, and grammars that do not exist or are unmaintained for some targets (Batch, VBScript, AppleScript, Oracle SQL). A table-driven lexer gives comments, strings, numbers, keywords, types, calls, brackets, and brace folds for a language in a few dozen lines of data with no dependency. It cannot provide symbols or grammar-accurate constructs; a language that needs those can still register a Tree-sitter module under the same ID.

## ADR-0012 — Places may list any directory

**Status:** Accepted

The opened-folder explorer is confined to a canonical root and refuses symlinks. A Places view in the style of other editors has to reach the whole machine, so `places.list` accepts any absolute path. This does not widen what the editor window can read: `file.open` already opens any path. Listings stay bounded, expose names and sizes only, and remain unavailable to the Settings window.
