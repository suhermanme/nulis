# Improvement plan 4: language capabilities (M41–M48)

**Status:** M41 (audit and design) complete, awaiting approval. No production code has changed. Progress lives in `IMPLEMENTATION_TRACKER.md`; handovers in `docs/handover/`.
**Date:** 2026-10-23, against commit `1494931` (Nulis 0.4.0), branch `feat/language-capabilities`.
**Milestone numbering:** continues `IMPROVEMENT_PLAN_3.md` (M36–M40). The request's M0–M7 are M41–M48 here.
**Companion documents:** [LANGUAGE_CAPABILITIES.md](LANGUAGE_CAPABILITIES.md) (contracts, registry, lifecycle, resolution, plugin API v2) and [LANGUAGE_MATRIX.md](LANGUAGE_MATRIX.md) (every file type: verified state and planned providers).

## 1. Request

Turn Nulis's language support into a capability-based system: a core that owns documents and the shared editing UI, bundled language providers behind stable interfaces, and a future path to external language plugins. Deliver auto-indentation and basic autocomplete without a language server, keep LSP optional, and keep every existing workflow working. Do not refactor production code before this plan is approved; then work one milestone at a time, stopping at each boundary with a handover.

## 2. Audit findings

All statements below were checked in the code at `1494931`. Line numbers refer to that commit.

### 2.1 Stack

- Rust workspace of 16 crates plus a Tauri 2 desktop adapter (`apps/desktop/src-tauri`) and a React 18 interface (`apps/desktop/src`). The core (`nulis-core`) owns documents, revisions, selections, undo and every command. The webview only ever holds bounded viewport snapshots (`ARCHITECTURE.md`, "Viewport boundary").
- The IPC protocol is versioned (`IPC_PROTOCOL_VERSION = 42`, `crates/nulis-core/src/lib.rs:82`, mirrored in `apps/desktop/src/contracts.ts:2`); the UI and the core ship in one binary, so a protocol bump is a coordinated change, not a compatibility break.
- Platforms: macOS (universal `.dmg`), Windows (NSIS), Linux (`.deb`, `.rpm`). CI runs fmt, clippy `-D warnings`, `cargo test --workspace`, ESLint, `tsc`, Vitest, and builds, installs, and starts every bundle (`.github/workflows/ci.yml`). Every crate has `unsafe_code = "forbid"`.

### 2.2 Language detection and registration

- `nulis-syntax` owns `LanguageRegistry` (`crates/nulis-syntax/src/lib.rs`): a `BTreeMap<LanguageId, LanguageModule>` built by `with_builtins()` (line 230) from three kinds of module: Tree-sitter grammars (JSON, Python), 37 table-driven lexer specs (`lexer.rs`, `SPECS`), and a Markdown line scanner. Plain text is a special case outside the map. Total: 41 entries, matching the requested list exactly.
- `detect_path` (line 309) lowercases the file name and its **last** extension, then returns the **first module in ID order** whose file names or extensions match. There is no explicit precedence between file name and extension, no compound extensions (`.d.ts` works only because `ts` is the last part), and no user associations. `register` exists (line 250) but there is no `unregister`, and no caller outside the crate.
- No two built-in languages claim the same extension or file name today (checked by extracting every table), so the implicit ordering has not mattered yet. It will as soon as plugins add languages.
- `detect_content` (line 303) runs only when the name gives Plain Text: `#!` interpreter, `<?php`, `<?xml` (XSL when `<xsl:` appears), HTML doctype. Called from `Editor::open` (`editor.rs:1052`).
- Gaps found by probing the registry: `Dockerfile.dev` and `x.jsonc` open as Plain Text; `.h` is always C; `.scpt` (a compiled, binary AppleScript file) is mapped to the AppleScript text language.
- A language chosen with **View As** (`language.set`, `editor.rs:2973`) is session state only: it is not in the recovery manifest (`nulis-filesystem/src/recovery.rs:17`) or the workbench session. `ARCHITECTURE.md` (M14 section) says the manifest records encoding and language; it records neither. To be corrected when this is fixed (M42).

### 2.3 Highlighting, folds, brackets, symbols

- Analysis runs in the core per viewport window (256 KiB, 25 ms, 20,000 spans) and travels with each viewport snapshot. Tokens are theme-independent `SyntaxToken`s.
- Lexer modes: Code (brackets and brace folds), Markup (element folds, no bracket pairs), CSS (brackets and folds), Delimited (column colors only). Tree-sitter JSON gives brackets, folds and `pair` symbols; Python gives function/class folds but **no bracket pairs**, so Go to Matching Bracket does nothing in Python. Markdown gives highlights only.
- `analyze` special-cases `"plaintext"` and `"markdown"` by ID (lines 354–358); `comment_syntax` (line 262) special-cases `"python"` and `"markdown"`.

### 2.4 Markdown preview

- Core: `markdown_preview_source` (`editor.rs:1850`, 1 MiB cap, reads unsaved text) and `markdown_preview_image` (4 MiB, confined to the document's folder). UI: `MarkdownPreview.tsx` (`react-markdown` + `remark-gfm`, `skipHtml`).
- The UI decides by language ID: `syntax.languageId === "markdown"` in `EditorViewport.tsx` (lines 619, 2020, 2077, 2245) for the Edit/View switch and map tones, and `App.tsx` (859–866) prints the preview through `PrintApp.tsx` when it is showing.

### 2.5 JSON, XML, YAML, plist tools

- One closed enum, `ToolKind` (`format_tools.rs:33`), maps `json`, `xml`/`xsl`, `yaml`, `plist` to a tool set. It is matched in `validate_document` (`editor.rs:3485`), `format_document` (3541) and `tool_source` (3736), and sent to the UI as `DocumentSummary.tool`.
- The same list is repeated three more times: the TypeScript `ToolKind` and `TOOL_LABELS` (`contracts.ts:301–304`) and the native menu's `TOOL_SETS` (`src-tauri/src/lib.rs:259–270`, which also hard-codes "Minify is JSON only").
- Behavior to preserve exactly: 8 MiB limit; UTF-8 or Windows-1252 decoding; first problem reported with line, column and byte offset, and the caret moved there; JSON keeps key order and number spelling; XML changes only inter-element whitespace; plist validates structure first; YAML Beautify is refused for tags and asks for confirmation when it would drop comments or expand anchors (`accept_loss`); Minify is JSON only; one revision-checked undo step; an unchanged result creates no revision.
- The algorithms are mature and well tested (`format_tools.rs`, `yaml_tools.rs`, `plist_tools.rs`). Only their dispatch is coupled to language IDs.

### 2.6 Editing infrastructure

- **Enter** inserts the caret line's line ending only (`EditorViewport.tsx:1933`, `lineBreakAt`). No indentation is carried.
- **Tab** inserts `\t`, or indents when the selection spans lines; **Shift+Tab** outdents (1823–1827). **Backspace** is a plain `editor.deleteBackward`.
- Indent/outdent are core line operations with `LineFormatOptions { tab_width: 4, indent_with_spaces: false }` (`line_ops/format.rs:13`). The desktop never sends options, so indentation is always one tab. The editor draws tabs at width 4 (`styles.css:410`). The default setting `editor.tabSize: 4` (`lib.rs:286`) is not read by any editing path.
- **Typed edits** are serialized in the UI, each naming the revision the previous one returned (`queueEdit`, `EditorViewport.tsx:811`). New editing commands must join that queue.
- **Toggle Comment** uses `comment_syntax`. **Go to Matching Bracket** uses the viewport's bracket pairs in the UI (`EditorViewport.tsx:1212`).
- **No completion, snippets, or LSP code exists** anywhere in the repository.
- **Commands** are registered once in the core with stable IDs and drive the palette, menus, toolbar, shortcuts and macros. Macros replay command IDs with recorded arguments.
- **Settings:** the persisted customization document (`nulis-settings`, schema 1, `deny_unknown_fields`) holds appearance, keybindings, File menu and toolbar. `SettingsStore` already has defaults, user, workspace and **per-language** layers (`nulis-settings/src/lib.rs:26`), and `docs/settings.schema.json` describes a `settings.json`, but nothing loads or saves user values: only built-in defaults are ever present. View options and editor preferences live in browser storage.

### 2.7 Extension mechanisms

- **Plugins (API v1, M12):** `nulis-plugin-host`. Manifest with `deny_unknown_fields` (so older Nulis rejects unknown fields: new contribution types need a new API version). Contributions are commands only; the only permission is `showMessage`. Code runs in a pinned Boa engine inside a **fresh worker process per activation or command**, with a 2-second deadline, size limits, and digest-bound user approval. Install, approve, revoke and uninstall exist. Process-per-call is fine for commands and whole-document tools, too slow for per-keystroke providers.
- **Scripts (Rhai) and macros:** deny-by-default, bounded; not a language-extension mechanism.
- **Themes:** declarative JSON, validated; a good model for declarative language contributions.
- `LanguageModule::tree_sitter` is public, so another grammar can be registered in Rust, but `LexerSpec` is `&'static` data, so lexer languages cannot be loaded at run time yet.

### 2.8 Tests and baseline

Run on this Mac at `1494931` on 2026-10-23:

- `cargo test --workspace`: all passed, 345 tests (nulis-core 165, nulis-syntax 15, nulis-plugin-host 9 + process-worker tests, nulis-settings 4, nulis-desktop 9).
- `npx vitest run` in `apps/desktop`: 22 files, 191 tests passed.
- clippy, fmt, lint and typecheck were not re-run for M41 because no code changed; CI ran them on `1494931`.

### 2.9 Defects found during the audit (not fixed in M41)

1. Toggle Comment in a `.css` file inserts `//`, which plain CSS does not accept (the CSS spec shares one table with SCSS and Less). Fix in M46.
2. Python has no bracket pairs, so bracket matching does nothing there. Fix in M46.
3. `.scpt` is binary but opens as AppleScript text. Fix in M46 (detect binary content, as already done for binary plists).
4. The language override is lost on restart and recovery; `ARCHITECTURE.md` describes it otherwise. Fix in M42.

## 3. Proposed architecture

Details, type sketches and the plugin API v2 manifest are in [LANGUAGE_CAPABILITIES.md](LANGUAGE_CAPABILITIES.md). In short:

### 3.1 Layers

| Layer | Owns | Where |
| --- | --- | --- |
| Core | documents, undo, revisions, commands, keyboard dispatch, settings, the language catalog, the capability registry, provider isolation, shared completion/diagnostic UI | `nulis-core`, `nulis-settings`, new `nulis-language` crate, `apps/desktop` |
| Bundled providers | highlighting (existing lexers and grammars), comments, indentation rules, keywords, snippets, the JSON/XML/YAML/plist tools, Markdown preview | `nulis-syntax`, existing tool modules in `nulis-core`, provider tables in `nulis-language` |
| External plugins (later) | declarative language data first; whole-document tools in the existing worker sandbox; LSP servers the user configures | `nulis-plugin-host` (API v2), new `nulis-lsp` crate |

### 3.2 Key decisions

1. **Capabilities are Rust traits in a new pure crate, `nulis-language`**, with a registry keyed by language and capability kind. It has no Tauri or I/O dependency, like `nulis-syntax` and `nulis-search`. Alternatives considered: (a) extend `nulis-syntax` (rejected: it would mix highlighting with formatting, completion and lifecycle); (b) put the registry inside `nulis-core` (workable, but `nulis-core` is already 22k lines and the contracts should be testable on their own).
2. **Mature code stays where it is.** The format algorithms remain in `nulis-core`'s tool modules; M43 wraps them as providers and replaces the four `ToolKind` lists with one capability descriptor. Highlighting stays in `nulis-syntax` behind a highlight provider.
3. **The UI asks for capabilities, never for language IDs.** `DocumentSummary` gains a `capabilities` object (validate, format, compact, preview kind, comment, completion, indentation). Toolbar, Tools menu, palette, native menu and the Markdown View switch read it. One completion popup, one diagnostics surface, one settings system.
4. **Indentation and completion are core commands over the piece table**, not UI logic: `editor.newline` (auto-indent on Enter, every caret, one undo step), outdent-on-type through `editor.insertText`, and `completion.request` / `completion.cancel` returning items with explicit byte replacement ranges and the revision they apply to. This keeps the "UI holds only a viewport" rule and makes multi-caret, undo and macros consistent.
5. **Detection gets explicit precedence:** user association, then exact file name, then the longest matching compound extension, then the last extension, then content sniffing, then Plain Text. Same-tier conflicts: built-in before plugin, plugins by user preference and otherwise by plugin ID, with the conflict shown in Settings. A golden test pins every current mapping before the change.
6. **External plugins are declarative first.** API v2 adds `contributes.languages` (associations, comments, brackets, lexer tables, indentation rules as linear-time regexes, keywords, snippets) as validated, bounded data with no code. Executable providers are limited to whole-document validate/format through the existing worker sandbox with new, user-approved permissions. Per-keystroke plugin code, plugin-rendered previews, and a marketplace are deferred.
7. **LSP is one client, opt-in, user-configured.** A `nulis-lsp` crate runs servers the user names (argument vectors, as the terminal does), after explicit approval, never from workspace files or plugin manifests. Servers are not bundled.
8. **Settings get a real `settings.json`.** Wire the existing `SettingsDocument` (schema 1) and `SettingsStore` layers, including per-language overrides, so indentation, suggestions, associations and capability switches have one home. The customization document is unchanged.

### 3.3 Compatibility

- Command IDs `tools.validate`, `tools.beautify`, `tools.minify` and their titles stay. Existing shortcuts are unchanged. New commands get new IDs.
- Macros recorded before M44 replay Enter as the literal `editor.insertText` they recorded; new recordings use `editor.newline`.
- Customization files keep schema 1. `settings.json` is new; a missing file means defaults that reproduce today's behavior, except where §7 decides otherwise.
- A document whose language or provider becomes unavailable keeps its language ID, falls back to Plain Text highlighting, keeps its content and dirty state, and recovers its features when the provider returns.
- Plugin API v1 manifests remain valid. API v2 is opted into with `"apiVersion": 2`.

### 3.4 Security

Unchanged boundaries: no plugin code in the webview, no new webview capabilities, every IPC payload bounded and revalidated in Rust. New inputs (declarative language data, settings, LSP traffic) are size-capped and validated; regexes use the `regex` crate (linear time) with a compiled-size limit; snippet bodies are text; LSP servers run only after the user approves the exact command. Provider calls are wrapped so that a panic or an over-budget result disables that provider and falls back, never crashing the editor. See LANGUAGE_CAPABILITIES.md §8.

## 4. Dependencies

| Need | Decision | Alternatives and trade-offs |
| --- | --- | --- |
| Capability registry, indentation, completion | No new dependency; `regex` (already in the workspace) for rule patterns | A Tree-sitter-based indenter: more accurate for some languages, but needs a grammar per language (ADR-0011) |
| Completion UI | Own React component | Embedding Monaco or CodeMirror for completion only would duplicate the editor model and break the viewport boundary |
| Snippets | Small built-in parser for a subset (`$1`, `${1:text}`, `$0`) | Full TextMate snippet grammar (variables, transforms) deferred |
| LSP | Evaluate `lsp-types` (MIT) for message types in M47; JSON-RPC framing written in-crate over `std::process` | `tower-lsp` is server-side; `async-lsp` would pull an async runtime into the core |
| Settings | Existing `nulis-settings` | — |

Every new dependency is pinned and recorded in `THIRD_PARTY_LICENSES.md` with its license.

## 5. Milestones

Each milestone is independently reviewable, bumps the IPC protocol when DTOs change, updates `IMPLEMENTATION_TRACKER.md`, the user guide where behavior changes, and writes `docs/handover/M4x_HANDOVER.md`. "All suites green" means `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace`, `npm run lint`, `npm run typecheck`, and Vitest, run and passing on macOS, with CI passing on all three platforms.

### M41 — Audit and design (request M0)

Deliverables: this plan, LANGUAGE_CAPABILITIES.md, LANGUAGE_MATRIX.md, tracker section, proposed ADRs 0014–0017, roadmap row. **Acceptance:** the owner approves the plan and answers §7.

### M42 — Language catalog and capability contracts (M1)

1. Commit a **golden detection test first**: every current extension, file name and content-sniff case with its present result.
2. `LanguageCatalog` in `nulis-syntax`: association records with a source (built-in, plugin, user), explicit precedence tiers, compound extensions, `register`/`unregister`, and a deterministic conflict report.
3. New crate `nulis-language`: `CapabilityKind`, `ProviderId`, provider traits, the registry, resolution, enable/disable, and the isolation wrapper (panic and budget containment).
4. Remove ID special cases from `analyze` and `comment_syntax` by giving modules a kind and comment data.
5. Persist `settings.json` (user layer and per-language overrides); support `files.associations`.
6. Persist the View As override in the recovery manifest and workbench session (optional field, old files still load).
7. `DocumentSummary.capabilities` (filled from built-ins) and `language.list` with associations; IPC 43.

**Acceptance:** golden test unchanged; a user association overrides a built-in one; plugin-origin conflicts resolve deterministically and are reported; unknown or unregistered languages fall back to Plain Text without losing the ID; registering and unregistering is tested; View As survives restart and recovery; no visible UI change. All suites green.

### M43 — First-party migration (M2)

1. JSON, XML/XSL, YAML and plist tools registered as `Validator` and `Formatter` providers (Beautify and Compact modes, loss note for YAML). `ToolKind` matching removed from `editor.rs`; `TOOL_LABELS`, the TS `ToolKind` and the native `TOOL_SETS` replaced by the capability descriptor.
2. Markdown preview registered as a `PreviewProvider` of kind `markdown`; the UI uses a renderer registry keyed by preview kind instead of `languageId === "markdown"` (Edit/View switch, print, map tones).
3. Settings ▸ Languages: per-language switches for each capability that has a provider.

**Acceptance:** every existing message, caret placement, loss confirmation, size limit and undo behavior is identical (existing tests unchanged and passing, plus new snapshot tests of messages); disabling `json.format` hides Beautify and Minify everywhere and leaves the document untouched, re-enabling restores them; disabling Markdown preview while in View mode returns to Edit without losing text; a provider that panics in a test is contained and reported. All suites green.

### M44 — Core auto-indentation (M3)

1. Settings: `editor.insertSpaces` (default false, as today), `editor.tabSize` (4), `editor.autoIndent` (`none` | `keep` | `full`), `editor.detectIndentation` (per document, from the first 64 KiB).
2. `editor.newline`: carries indentation, applies the language's `IndentProvider`, splits `{|}` and `<a>|</a>` onto three lines, works for every caret, one undo step.
3. Outdent-on-type for closing tokens on a whitespace-only line (`}`, `</`, `end`, `else`), merged into the typed character's undo step.
4. Tab inserts the indent unit; smart Backspace removes one indent unit inside leading spaces; Shift+Tab unchanged.
5. Rule providers: brace family, Python, YAML (spaces only), Makefile (tab after a rule line), markup, keyword-block family; carry-only for the rest (see LANGUAGE_MATRIX.md).
6. Paste: exact by default; optional `editor.pasteAdjustIndent` re-bases a multiline paste onto the caret's indentation.

**Acceptance:** table-driven Rust tests per rule family (newline, block indent, outdent, tabs vs spaces, CRLF, multi-caret, read-only refusal); Vitest tests that keys reach the right commands through the edit queue; undo/redo restores exact bytes in one step; `autoIndent: none` reproduces today's bytes exactly; macros recorded before M44 replay byte-identically. All suites green.

### M45 — Shared autocomplete (M4)

1. `completion.request { documentId, revision, position, trigger, requestId }` and `completion.cancel`. Items carry label, kind, detail, insert text or snippet, an explicit byte replacement range, and their source.
2. Providers: current-document words (bounded window around the caret), language keywords (from the lexer tables; Python and JSON lists added), built-in snippets as data.
3. Ranking (exact prefix, then word-start, then subsequence; provider rank; recency) and deduplication.
4. One `CompletionPopup`: ARIA listbox, Up/Down/Page keys, Enter or Tab accepts, Escape dismisses, mouse; hidden during IME composition; snippet placeholders navigated with Tab.
5. Stale responses dropped by request ID and revision; typing never waits for completion.
6. Settings: `editor.suggest.auto`, `delayMs`, `minChars`, `acceptOnEnter`; command `editor.triggerSuggest` with a configurable shortcut.
7. First languages: JavaScript, TypeScript, Python, Rust, SQL, HTML.

**Acceptance:** replacement ranges correct for ASCII, multi-byte UTF-8, Windows-1252 and multi-caret; a response for an old revision is never applied; typing latency unchanged in the existing typing tests; keyboard-only and screen-reader labels checked in tests. All suites green.

### M46 — Expand bundled coverage (M5)

Fill the planned columns of LANGUAGE_MATRIX.md for every language; fix the four defects in §2.9; Markdown heading folds and YAML indentation folds; proposed new associations (`.jsonc`, `Dockerfile.*`, `CMakeLists.txt` and others listed in the matrix) only as approved in §7. A smoke test runs Enter, Tab, Backspace, Toggle Comment and completion in every language and asserts no error and plain fallback where nothing is provided.

### M47 — Optional LSP (M6)

`nulis-lsp`: one JSON-RPC client over stdio; servers configured in user settings only and approved per exact command; start on first matching document, shut down when the last closes; initialize/request timeouts, `$/cancelRequest`, restart with back-off (three tries), full-text sync for documents up to 8 MiB, UTF-8/UTF-16 position conversion. Adapters into the completion, diagnostics and formatting contracts. Shared diagnostics UI (gutter marks, underline, a Problems list) also used by validators. Start with rust-analyzer; pick the second server in M47 after verifying license, platform builds and maintenance (candidates in the matrix). CI uses a fake server binary built from the repository.

**Acceptance:** fake-server tests for start, crash, timeout, cancel, stale results and shutdown on all three CI platforms; no server configured means no process and no change in behavior; manual check with rust-analyzer on macOS, Windows and Linux.

### M48 — External plugin readiness (M7)

Plugin API v2: `apiVersion`, `contributes.languages` and associations as validated data, run-time lexer tables (owned `LexerSpec`), indentation rules, keywords, snippets; per-plugin and per-capability enable, disable and remove; conflict UI; error isolation tests with malformed and hostile manifests; PLUGIN_API and PLUGIN_GUIDE chapters and a declarative example plugin. Executable validators/formatters through the worker sandbox only after a written security review. No marketplace.

## 6. Test strategy

- **Golden tests before migration** (detection table, tool messages and outputs) so refactors prove no change.
- **Pure-crate tests** in `nulis-language` for resolution, precedence, conflicts, enable/disable, isolation, indentation rules and ranking.
- **Core command tests** in `nulis-core` for revision checks, multi-caret, undo, read-only, encodings and limits.
- **Vitest** for keyboard routing, popup behavior, stale-response handling, capability-driven menus and the Settings window.
- **Fuzzing:** extend the existing fuzz setup with indentation-rule and snippet-parser targets (M44/M45).
- **Manual checks** per milestone on the three test VMs, recorded in `PLATFORM_MATRIX.md`.

## 7. Questions for the owner

1. **Auto-indent default.** Recommended: `full` (language rules) for new and existing users, with `none` restoring today's behavior. Alternative: `keep` (carry indentation only) by default.
2. **Automatic suggestions.** Recommended: the popup opens automatically after 2 identifier characters in languages with a keyword provider, never in Plain Text or Markdown; manual everywhere with the shortcut.
3. **Completion shortcut.** Ctrl+Space is the usual choice, but on macOS it switches input sources by default. Recommended: Ctrl+Space on Windows and Linux, Ctrl+Space and Option+Esc on macOS, both rebindable.
4. **New file associations** (`.jsonc` as JSON, `Dockerfile.*` as Dockerfile, `CMakeLists.txt` as plain text until a CMake language exists, and the others listed in the matrix). They change how some files open. Recommended: add them in M46 and list them in the release notes.
5. **Second LSP server** after rust-analyzer: clangd, gopls, or pyright (pyright needs Node.js). Recommended: decide in M47 after verification.

## 8. Risks

| Risk | Mitigation |
| --- | --- |
| Refactoring detection or tools changes behavior | Golden tests committed before each migration; migration only re-routes calls |
| Auto-indent surprises users or breaks macros | Setting with a `none` value; old macros replay byte-identically; manual checks per language family |
| Completion adds typing latency | Requests run off the typing queue; bounded windows; stale results dropped; latency covered by tests |
| Windowed analysis cannot see context above the viewport | Indentation reads a bounded number of previous lines from the piece table, not the viewport; documented limits |
| `LexerSpec` is static data | Owned specs in M48 only; built-ins stay static |
| LSP process lifecycle differs by platform | Fake-server CI on all three OSs; terminal crate's process patterns reused |
| Plugin API v2 widens the attack surface | Declarative data first, bounded and linear-time; executable providers gated on a security review |
| Scope creep toward an IDE | Matrix states which languages get what; equivalent intelligence across languages is explicitly not promised |

## 9. Deferred

Marketplace and remote installation; plugin-rendered previews; per-keystroke plugin code; semantic highlighting; incremental LSP sync; workspace-level LSP configuration; full TextMate snippet syntax; new Tree-sitter grammars (allowed later under ADR-0011); cross-file completion.
