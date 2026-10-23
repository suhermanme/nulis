# Implementation tracker (M19–M48)

Authoritative progress and handover record for `IMPROVEMENT_PLAN.md` (M19–M30), `IMPROVEMENT_PLAN_2.md` (M31–M35), `IMPROVEMENT_PLAN_3.md` (M36–M40), and `IMPROVEMENT_PLAN_4.md` (M41–M48). Keep it in step with the repository; never mark work complete without the verification listed.

## Current state

- **Current milestone:** M45 complete (2026-10-23); next is M46 (expand bundled coverage, self-hosting). M41 approved 2026-10-23 with every plan §7 recommendation accepted. Branch `feat/language-capabilities`. **To resume plan 4, read the "Plan 4" section below.** Plan 3 is complete and released in Nulis 0.4.0 (2026-10-23).
- **Overall:** M19–M30 complete (M19–M29 in Nulis 0.2.0, M30 after it); plan 2: 5 of 5 milestones completed (M31–M35); plan 3: 5 of 5 completed (M36–M40, in Nulis 0.4.0); plan 4: 5 of 8 (M41–M45)
- **Baseline before M19 (commit `66853e1`):** `cargo test -p nulis-core` 86 passed; desktop Vitest 11 files, 115 tests passed.

| Milestone | Status | Handover |
| --- | --- | --- |
| M19 Audit and foundation (+ Windows Settings window bug) | Completed (Windows/Linux runtime check pending) | `handover/M19_HANDOVER.md` |
| M20 Core editing keys, line operations, editor context menu | Completed (manual and Windows/Linux checks pending) | `handover/M20_HANDOVER.md` |
| M21 Find and Replace window | Completed (manual and Windows/Linux checks pending) | `handover/M21_HANDOVER.md` |
| M22 Replace in Files | Completed (manual and Windows/Linux checks pending) | `handover/M22_HANDOVER.md` |
| M23 External changes and Reload (user request) | Completed (manual and Windows/Linux checks pending) | `handover/M23_HANDOVER.md` |
| M24 New application icon (user request) | Completed | `handover/M24_HANDOVER.md` |
| M25 Format and transforms | Completed (manual and Windows/Linux checks pending) | `handover/M25_HANDOVER.md` |
| M26 Search and navigation | Completed (manual and Windows/Linux checks pending) | `handover/M26_HANDOVER.md` |
| M27 View features (split from "View and file") | Completed (manual and Windows/Linux checks pending) | `handover/M27_HANDOVER.md` |
| M28 File conveniences (split from "View and file") | Completed (manual and Windows/Linux checks pending) | `handover/M28_HANDOVER.md` |
| M29 Column and clipboard tools | Completed (manual and Windows/Linux checks pending) | `handover/M29_HANDOVER.md` |
| M30 Integration and quality | Completed (manual screen-reader and Linux checks pending) | `handover/M30_HANDOVER.md` |
| M31 Find window fits its content | Completed (checked on macOS; Windows/Linux pending) | `handover/M31_M32_HANDOVER.md` |
| M32 Regular expressions and column search | Completed (checked on macOS; Windows/Linux pending) | `handover/M31_M32_HANDOVER.md` |
| M33 Theme contrast | Completed (verified by the user on macOS; Windows/Linux pending) | this tracker, M33 section |
| M34 YAML and plist Validate | Completed (awaiting the user's check on macOS; Windows/Linux pending) | this tracker, M34 section |
| M35 CSV/TSV, YAML and plist highlighting | Completed (awaiting the user's check on macOS; Windows/Linux pending) | this tracker, M35 section |
| M36 Print spike and commands | Completed (print check passed on macOS; Windows/Linux pending) | `handover/M36_HANDOVER.md` |
| M37 `nulis-print` layout and PDF export | Completed (checked in the app on macOS; Windows/Linux pending) | `handover/M37_HANDOVER.md` |
| M38 Scope, large files, other views | Done; checked in the app on macOS 2026-10-22 | `handover/M38_HANDOVER.md` |
| M39 Print window and Page Setup dialog | Complete (macOS checked 2026-10-22; Windows/Linux pending) | `handover/M39_HANDOVER.md` |
| M40 Docs and release | Complete 2026-10-22 | `handover/M40_HANDOVER.md` |
| M41 Language capabilities: audit and design | Completed 2026-10-23 (approved) | this tracker, "Plan 4" section |
| M42 Language catalog and capability contracts | Completed 2026-10-23 (checked in the app on macOS) | `handover/M42_HANDOVER.md` |
| M43 First-party migration (tools, Markdown preview) | Completed 2026-10-23 (app check pending) | `handover/M43_HANDOVER.md` |
| M44 Core auto-indentation | Completed 2026-10-23 (app check pending) | `handover/M44_HANDOVER.md` |
| M45 Shared autocomplete | Completed 2026-10-23 (app check pending) | `handover/M45_HANDOVER.md` |
| M46 Expand bundled coverage | Not Started | — |
| M47 Optional LSP | Not Started | — |
| M48 External plugin readiness | Not Started | — |

## Plan 4 (M41–M48) — language capabilities

Plan: `IMPROVEMENT_PLAN_4.md`. Design: `LANGUAGE_CAPABILITIES.md`. Per-language state: `LANGUAGE_MATRIX.md`. The request's milestones M0–M7 are M41–M48. Statuses: Not Started, In Progress, Blocked, Completed.

### Handover for the next agent

1. Read `IMPROVEMENT_PLAN_4.md` (§2 findings and §5 milestones), then `LANGUAGE_CAPABILITIES.md`. Check §7 of the plan for the owner's answers; do not start M44 or M45 defaults without them.
2. Work on branch `feat/language-capabilities` (or a branch per milestone from it). One milestone at a time; stop at each boundary with a handover in `docs/handover/M4x_HANDOVER.md` and an update here.
3. Commit golden tests **before** any migration (detection table in M42, tool messages in M43), so the refactor shows no change.
4. Run the full check before marking anything Completed: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and in `apps/desktop` `npm run lint`, `npm run typecheck`, `npx vitest run`. Record counts here.
5. Running a desktop build uses the user's real data (`docs/handover/PLAN3_RESUME.md`, "Running a test build safely"): ask first and back up.
6. Bump `IPC_PROTOCOL_VERSION` in `crates/nulis-core/src/lib.rs` and `apps/desktop/src/contracts.ts` together whenever a DTO changes.

### M41 — Audit and design

**Status:** Completed and approved 2026-10-23. No production code changed.

| Task | Status |
| --- | --- |
| Audit: stack, detection, highlighting, preview, tools, editing, settings, plugins, tests | Completed |
| Probe the registry (comments, brackets, folds, detection edge cases) from a scratch crate outside the repo | Completed |
| Baseline test run | Completed |
| `IMPROVEMENT_PLAN_4.md`, `LANGUAGE_CAPABILITIES.md`, `LANGUAGE_MATRIX.md` | Completed |
| ADR-0014 to ADR-0017 in `DECISIONS.md`; roadmap row | Completed |
| Owner approval and answers to plan §7 | Completed (all recommendations accepted) |

**Baseline (commit `1494931`, macOS, 2026-10-23):** `cargo test --workspace` passed, 345 tests (nulis-core 165, nulis-syntax 15, nulis-plugin-host 9 unit, nulis-settings 4, nulis-desktop 9); Vitest 22 files, 191 tests passed. fmt, clippy, lint and typecheck not re-run (no code changed; CI passed on `1494931`).

**Files changed:** `docs/IMPROVEMENT_PLAN_4.md`, `docs/LANGUAGE_CAPABILITIES.md`, `docs/LANGUAGE_MATRIX.md` (new); `docs/IMPLEMENTATION_TRACKER.md`, `docs/DECISIONS.md`, `docs/ROADMAP.md` (updated).

**Decisions:** documents follow the repository's names (`IMPROVEMENT_PLAN_4.md` plays the role of the requested `IMPLEMENTATION_PLAN.md`; this tracker is the requested `IMPLEMENTATION_TRACKER.md`); milestone numbers continue at M41.

**Known issues found (not fixed):** see plan §2.9 — `.css` Toggle Comment uses `//`; Python has no bracket matching; `.scpt` opens as text; the View As language is not persisted although `ARCHITECTURE.md` says the recovery manifest records it.

### M42 — Language catalog and capability contracts

**Status:** Completed 2026-10-23; checked in the app on macOS by the owner the same day. Handover: `handover/M42_HANDOVER.md`.

| Task | Status |
| --- | --- |
| Golden detection test of every current mapping (commit first) | Completed (`de1bfec`) |
| `LanguageCatalog`: association sources, precedence tiers, compound extensions, `unregister`, conflict report | Completed |
| New crate `nulis-language`: contracts, registry, resolution, enable/disable, isolation wrapper | Completed (indentation and completion traits move to M44/M45, see decisions) |
| Remove ID special cases in `analyze` and `comment_syntax` | Completed |
| Persist `settings.json` (user and per-language layers); `files.associations` | Completed (no Settings UI yet; edited by hand, read at start) |
| Persist the View As override (workbench session) | Completed (recovery already kept it; plan §2.2 corrected) |
| Unknown language falls back to Plain Text in the viewport | Completed (found during M42) |
| `languageOverride` in document summaries; IPC 43 | Completed |
| `DocumentSummary.capabilities`, `language.list` associations | Moved to M43, where the interface first uses them |

**Acceptance:** golden test unchanged (passes against the new precedence); a user association overrides a built-in one; plugin conflicts resolve deterministically and are reported; unregistered languages fall back to Plain Text keeping the ID; register/unregister tested; View As survives a restart (desktop test); no visible UI change other than View As being remembered.

**Verification (macOS, 2026-10-23):** `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` 373 passed (was 345: nulis-syntax 22 unit + 4 golden, nulis-language 8, nulis-settings 7, nulis-core 170, nulis-filesystem +1); `npm run lint` and `npm run typecheck` clean; Vitest 22 files, 192 tests passed (was 191). **App check (macOS dev build, 2026-10-23, by the owner):** `*.h` mapped to C++ in `settings.json` opened `widget.h` as C++; View As ▸ Markdown on a `.txt` file survived quitting and restarting (the session held `languages`); everyday files opened with the same languages as before; the Symbols button is gone. User data was backed up before and restored after. CI not run (branch not pushed, by the owner's choice).

**Files changed:** `crates/nulis-syntax/src/lib.rs`, `crates/nulis-syntax/tests/detection_golden.rs` (new), `crates/nulis-language/` (new crate), `crates/nulis-settings/src/lib.rs`, `crates/nulis-core/src/lib.rs`, `crates/nulis-core/src/editor.rs`, `crates/nulis-core/src/editor/{line_ops,rename,print_export}.rs`, `crates/nulis-filesystem/src/session.rs`, `apps/desktop/src/{contracts.ts,App.tsx,EditorViewport.tsx,App.test.tsx}`, `Cargo.toml`, `Cargo.lock`, docs (`ARCHITECTURE.md`, `USER_GUIDE.md`, `README.md`, `settings.schema.json`, plan, matrix, this tracker).

**Decisions:**
- Indentation and completion traits are added with their first implementations (M44, M45) rather than as unused interfaces now.
- Capability descriptors in document summaries move to M43, together with the interface that reads them.
- `register` now takes an `AssociationSource`; built-in languages cannot be unregistered.
- A `[language]` section in `settings.json` is the per-language override format (`"[python]": { … }`).

**Known issues:** View As updates the interface's copy of the language but not its tool set until the next document summary arrives (existing behavior; fixed by capability descriptors in M43). `settings.json` has no Settings page yet (M43) and is read only at start.

### M43 — First-party migration

**Status:** Completed 2026-10-23 (automated checks on macOS; not yet tried in the running app). Handover: `handover/M43_HANDOVER.md`.

| Task | Status |
| --- | --- |
| Golden tests for every tool message, caret target and output (commit first) | Completed (`7665d03`, 26 documents) |
| JSON, XML/XSL, YAML, plist as `Validator`/`Formatter` providers; remove `ToolKind` matching | Completed |
| Replace TS `ToolKind`, `TOOL_LABELS`, native `TOOL_SETS` with the capability descriptor | Completed |
| Markdown as `PreviewProvider`; Edit/View switch reads the capability | Completed (map tones stay keyed to the Markdown language, see decisions) |
| Settings ▸ Languages capability switches | Completed |
| Disable/re-enable and provider-failure tests | Completed (core test reaches open documents; panic containment tested in `nulis-language`) |
| View As refreshes tools at once (known issue from M42) | Completed (`language.set` returns the summary) |

**Acceptance:** every tool message, location, note, and output is identical (golden test unchanged apart from the replaced `tool` fields); switching Beautify and Minify off for JSON hides them everywhere and leaves the document untouched, switching back restores them, also for open documents; a Markdown document without its preview has no Edit/View switch.

**Verification (macOS, 2026-10-23):** fmt and clippy (`-D warnings`) clean; `cargo test --workspace` 374 passed; lint and typecheck clean; Vitest 22 files, 195 tests passed (was 192). Not tried in the running app; CI not run (branch not pushed).

**Files changed:** `crates/nulis-language/src/{lib,tests}.rs` (`may_lose_data`, `replace_disabled`, `Deserialize` kinds), `crates/nulis-core/src/builtin_providers.rs` (new), `crates/nulis-core/src/{editor,lib,format_tools}.rs`, `crates/nulis-core/tests/{tool_golden.rs,golden/tools.json}` (new), `crates/nulis-core/Cargo.toml`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src/{contracts.ts,ipc.ts,App.tsx,EditorViewport.tsx,SettingsWindow.tsx}` and tests, docs.

**Decisions and visible changes:**
- Native Tools menu: an unavailable item now reads just "Validate", "Beautify", or "Minify" (0.4.0 showed a disabled "Minify JSON" for every non-JSON document).
- When a tool is not available, the status bar says "Minify is not available for this document" (and likewise for Validate and Beautify) instead of listing the four formats.
- Validation reports no longer carry `tool`; the interface never read it.
- New messages for new situations only: "Validate is not available for JSON documents" (and Beautify) when the user switched it off.
- The document map's Markdown colors still follow the Markdown language, not the preview switch: they are highlighting, not preview.
- Kept on purpose (golden): YAML Beautify with tags says "Invalid YAML: Beautify cannot keep YAML tags…" although the YAML is valid; worth rewording in a later milestone with a release note.

**Known issues:** associations are still edited in `settings.json` by hand (Settings shows them read-only).

### M44 — Core auto-indentation

**Status:** Completed 2026-10-23 (automated checks on macOS; not yet tried in the running app). Handover: `handover/M44_HANDOVER.md`.

| Task | Status |
| --- | --- |
| Indentation settings and per-document detection | Completed (`editor.autoIndent` default `full`, `insertSpaces` false, `tabSize` 4, `detectIndentation` true; `[language]` overrides; Settings ▸ Editor ▸ Indentation) |
| `editor.newline` (all carets, one undo step); UI Enter routed through the edit queue | Completed |
| Outdent-on-type merged into the typed character's undo step | Completed (`}` `)` `]`, `</`, Python `else:`/`elif:`/`except:`/`finally:`; keyword closers move on Enter instead, so `endpoint` never jumps) |
| Tab inserts indent unit; smart Backspace | Completed (`editor.tab`) |
| Rule families: brace, python, yaml, makefile, markup, keyword-block, carry | Completed |
| Paste: exact by default, optional re-basing | Exact: Completed. Optional re-basing: deferred (not needed for acceptance; no setting added) |
| Macro replay compatibility test; fuzz target for rules | Completed (`indent_rules` fuzz target, run in CI's nightly job; awkward-text smoke test) |

**Acceptance:** rule-family tests (newline, block indent, outdent, tabs vs spaces, CRLF and CR, multi-caret, read-only) pass; keys reach `editor.newline`/`editor.tab`/typed `insertText` through the edit queue; Undo restores exact bytes in one step; `autoIndent: none` inserts exactly the line ending 0.4.0 inserted (LF, CRLF, CR, last line); a line break inserted as text (old macros) is never indented.

**Verification (macOS, 2026-10-23):** fmt and clippy clean; `cargo test --workspace` 394 passed (nulis-language 20, nulis-core 178 + golden); lint and typecheck clean; Vitest 22 files, 195 tests passed. Fuzz target compiles (`cargo check` in `fuzz/`); not run locally (needs nightly). Not tried in the running app; CI not run (branch not pushed).

**Files changed:** `crates/nulis-language/src/indent.rs` and `indent/tests.rs` (new), `crates/nulis-language/src/lib.rs`, `crates/nulis-core/src/editor/indentation.rs` (new), `crates/nulis-core/src/{editor,lib,builtin_providers}.rs`, `crates/nulis-core/src/editor/rename.rs`, `apps/desktop/src/{contracts.ts,EditorViewport.tsx,SettingsWindow.tsx}` and tests, `fuzz/`, `.github/workflows/ci.yml`, docs.

**Decisions and visible changes:**
- Enter indents by default (owner's choice). A line holding only indentation loses it when Enter leaves it empty (no trailing spaces).
- Tab inserts spaces when the file is already indented with spaces (detection), even with "Indent with spaces" off; Indent/Outdent do the same.
- Two carets on one line: existing indentation is left alone (each still gets its new line's indentation).
- Renaming a file now also refreshes its tools (bug from M43).
- The interface no longer decides line endings; `lineBreakAt` was removed.

**Known issues / later:** Dockerfile `\` continuation and Markdown list continuation are carry only; paste re-basing not implemented; brace rules ignore block comments (`/* { */`); indentation context is limited to 64 lines / 16 KiB above the caret.

### M45 — Shared autocomplete

**Status:** Completed 2026-10-23 (automated checks on macOS; not yet tried in the running app). Handover: `handover/M45_HANDOVER.md`.

| Task | Status |
| --- | --- |
| `completion.request` / `completion.cancel` with revision and request ID | Request: Completed. Cancel: not built (see decisions) |
| Word, keyword and snippet providers; ranking and deduplication | Completed |
| Shared `CompletionPopup` (keyboard, mouse, ARIA, IME) and snippet placeholders | Completed (no list while composing; `aria-activedescendant`; Tab visits placeholders) |
| Settings and `edit.triggerSuggest` shortcut | Completed (`editor.suggest.auto`, `acceptOnEnter`, `minChars`, `delayMs`; Settings ▸ Editor ▸ Suggestions; Ctrl+Space, Option+Esc on macOS, rebindable Show Suggestions) |
| First languages: JavaScript, TypeScript, Python, Rust, SQL, HTML | Completed for every language (words everywhere, keywords and snippets as in the matrix) |

**Acceptance:** replacement ranges are exact with multi-byte UTF-8 and several carets (core tests); an answer for an older revision is never shown (Vitest) and accepting checks the revision and range again (core); typing never waits for completion (requests are chained after pending edits but edits do not wait for them); keyboard-only use and screen-reader attributes covered by Vitest.

**Verification (macOS, 2026-10-23):** fmt and clippy clean; `cargo test --workspace` 408 passed (nulis-language 26, nulis-syntax 23 + 4 golden, nulis-core 185 + golden); lint and typecheck clean; Vitest 22 files, 198 tests passed. Windows-1252 ranges are covered by the shared encoding helpers but have no dedicated test. Not tried in the running app; CI not run (branch not pushed).

**Files changed:** `crates/nulis-syntax/src/{lib,lexer}.rs` (`Vocabulary`, HTML and Python words), `crates/nulis-language/src/completion.rs` and `completion/tests.rs` (new), `crates/nulis-language/src/lib.rs`, `crates/nulis-core/src/editor/completion.rs` and `completion/tests.rs` (new), `crates/nulis-core/src/{editor,lib,builtin_providers}.rs`, `crates/nulis-core/src/editor/indentation.rs`, `apps/desktop/src/CompletionPopup.tsx` (new), `apps/desktop/src/{contracts.ts,commands.ts,EditorViewport.tsx,App.tsx,SettingsWindow.tsx,styles.css}` and tests, docs.

**Decisions:**
- No `completion.cancel`: the built-in providers are bounded (128 KiB each side, 100 items) and answer in milliseconds; staleness is handled by request ID and revision. Cancellation comes with language servers (M47).
- The replacement range is one per list (the typed word), not per item.
- A repeated snippet placeholder gets its text once; editing one copy does not change the others.
- Ctrl+Space and Option+Esc are fixed in the editor; the new `edit.triggerSuggest` command can take any other shortcut.
- Snippets are offered only with one caret; with several carets the expanded text is inserted at each and the caret goes to the end.

**Known issues / later:** no member completion after `.`; no CSS property or YAML key lists; the list does not show documentation; snippets for Swift, Zig, Kotlin-specific forms and others are not written yet (M46).

### M46 — Expand bundled coverage

**Status:** Not Started. **Depends on:** M43–M45 (done), owner answer §7.4 (given: add the associations, list them in the release notes).

| Task | Status |
| --- | --- |
| Self-hosting test over this repository; ignore-file language; JSX/TSX tags; JS/TS regex literals and template `${…}` | Not Started |
| Fill Table 2 of `LANGUAGE_MATRIX.md` for every language | Not Started |
| Fix plan §2.9 defects (CSS comment, Python brackets, `.scpt`) | Not Started |
| Markdown heading folds, YAML indentation folds | Not Started |
| Approved association changes | Not Started |
| Every-language smoke test (Enter, Tab, Backspace, Toggle Comment, completion) | Not Started |

### M47 — Optional LSP

**Status:** Not Started. **Depends on:** M43 (diagnostics contract), M45.

| Task | Status |
| --- | --- |
| Verify candidate servers (license, platforms, maintenance); choose the second server | Not Started |
| `nulis-lsp`: JSON-RPC client, lifecycle, timeouts, cancel, restart, position encoding | Not Started |
| Settings, approval bound to the exact command | Not Started |
| Adapters: completion, diagnostics, formatting; shared diagnostics UI | Not Started |
| Fake-server CI tests on three platforms; manual check with rust-analyzer | Not Started |

### M48 — External plugin readiness

**Status:** Not Started. **Depends on:** M42–M46.

| Task | Status |
| --- | --- |
| Plugin API v2 manifest (`apiVersion`, declarative languages); v1 still valid | Not Started |
| Owned run-time `LexerSpec`; regex and size validation | Not Started |
| Enable, disable, remove per plugin and per capability; conflict UI | Not Started |
| Hostile-manifest and isolation tests | Not Started |
| `PLUGIN_API.md` v2, `PLUGIN_GUIDE.md` chapter, declarative example plugin | Not Started |
| Security review before any executable language provider | Not Started |

## M19 — Audit and foundation

**Deliverables:** `IMPROVEMENT_PLAN.md`, this tracker, `handover/M19_HANDOVER.md`; baseline test run.

**User-reported bug fixed in M19:** on Windows the Settings window opened blank, and on Windows and Linux Settings showed the application menu bar.

- Cause 1 (blank window): Tauri documents that building a window inside a synchronous command or an event handler deadlocks on Windows (WebView2). `open_settings_window`/`open_help_window` were synchronous commands and the native Settings/Help menu items built the window inside `on_menu_event`.
- Fix: both commands are now `async`, and the menu handlers spawn the window creation on the async runtime.
- Cause 2 (menu bar): `app.set_menu` attaches the application menu to every window on Windows/Linux.
- Fix: `detach_application_menu` calls `remove_menu()` on the Settings and Help windows (no-op on macOS, which has one global menu bar).
- File: `apps/desktop/src-tauri/src/lib.rs`.

**Verification:** `cargo clippy -p nulis-desktop --all-targets -D warnings` clean; `cargo test -p nulis-desktop` passed (6). The Windows/Linux code path is not compiled on this Mac (only `x86_64-apple-darwin` installed). **Pending:** CI build on Windows/Linux and a manual check that Settings and Help render and show no menu bar.

## M20 — Core editing keys, line operations, editor context menu

**Scope**

1. Caret: Home (smart: first non-blank, then column 0), End, Primary+Home/End (document start/end); all with Shift to extend. On macOS also Primary+Left/Right as line start/end and Primary+Up/Down as document start/end.
2. Forward Delete (selection, or next character; malformed bytes one at a time; CRLF as one unit).
3. Select All (Primary+A).
4. Line operations as one undo step each, over every line touched by any selection: Select Line(s), Delete Line(s), Duplicate Line(s), Move Line(s) Up/Down, Delete to Line Start, Delete to Line End, Insert Line Below.
5. Editor right-click menu: Undo, Redo, Cut, Copy, Paste, Find, Replace, Select All, Select Line, Select Word, Toggle Hex Mode, Copy File Path.
6. Palette entries and default shortcuts for the line operations.

**Exclusions:** rectangle-specific line semantics beyond "rows of the rectangle", customizing the context menu, native Edit menu additions (M27), word-wise movement.

**Expected files:** `crates/nulis-core/src/editor.rs`, `crates/nulis-core/src/lib.rs`, `apps/desktop/src/EditorViewport.tsx`, `apps/desktop/src/contracts.ts`, tests in `editor.rs` and `EditorViewport.test.tsx`, `docs/USER_GUIDE.md`, `docs/FEATURE_PARITY.md`.

**Acceptance criteria**

- Home/End/Primary+Home/End move and extend correctly on LF, CRLF, CR, and the last line without a newline.
- Delete removes the next scalar, a whole CRLF pair, or the selection; refused in read-only mode.
- Select All selects bytes `0..len`.
- Each line operation is one undo step, keeps bytes and line endings exact, works with several carets, and fails cleanly above the edit-size limit.
- Right-click on the text shows the menu; each item runs the same action as its shortcut; Escape closes it.
- All suites green (`cargo test --workspace`, clippy, fmt, lint, typecheck, Vitest).

**Risks / rollback:** IPC version bump to 27; revert the M20 commit to roll back (no persisted data changes).

**Result (completed 2026-10-20):** all acceptance criteria met by automated tests on macOS. Palette entries (scope item 6) moved to M29 because the palette only runs zero-argument commands. Verification: `cargo test --workspace` passed (nulis-core 96, was 86); clippy and fmt clean; lint and typecheck clean; Vitest 118 passed (was 115). Not yet tried by hand in the running app. Details: `handover/M20_HANDOVER.md`.

## M21 — Find and Replace window

**Origin:** user request after M20 with 9 screenshots of the reference editor's Find and Replace dialog (S19–S27 in the plan).

**Scope**

1. One floating, draggable Find and Replace panel inside the main window (not a separate OS window), opened by Ctrl/⌘ F (Find tab), Ctrl/⌘ H (Replace tab), the editor toolbar, and the right-click menu; seeded with the selected text.
2. Tabs: Find, Find in Files, Replace. (Replace in Files is M22.)
3. Find: multi-line Find what with recent-search history; Find where = current document, selected text, all open files; Options = match case, whole word, regular expression, highlight all items found, list lines containing the text, close after find; Count All, Previous, Next.
4. Replace: Replace with (with history); Replace where = current document, selected text, all open files; Options = match case, whole word, regex, close after Replace All; Next, Replace, Replace All.
5. Find in Files: Find where = folder or all open files; file-name filter (`*.rs; *.md`), directory with Browse, include subdirectories; options match case, whole word, regex; results list "file:line: text" that opens the file at the match.
6. "Lines containing" results window: one row per matching line (optional line numbers), click or Go To jumps to that line, Bookmark All, Copy, Close.
7. Remove the Search panel from the side bar and the inline find bar from the editor; the Go To Line bar stays.

**Core changes:** `SearchOptions.whole_word`; `search.findAll` (matches + matching lines with line numbers and bounded line text, optional selection range, result cap); `search.replaceAll` gains an optional selection range; `search.findInFiles` gains name filters, a recursive flag, and line number + line text per match; `editor.addBookmarks` for Bookmark All. IPC 27 → 28.

**Exclusions:** Replace in Files (M22); search in column range, preserve case, regex dialects, ignore options, favorites (deferred).

**Acceptance criteria**

- Each option changes results as described; whole word works for literal and regex searches.
- Count All and the lines list agree with the core; clicking a row puts the caret on that line in the right document.
- Replace All in a selection changes only matches inside it and is one undo step.
- Find in Files respects the filter and the subdirectory switch and shows line numbers.
- The panel can be dragged, keeps its tab and options while open, closes with Escape or its close button, and all controls are keyboard reachable with labels.
- All suites green.

**Result (completed 2026-10-20):** scope items 1–7 implemented. Verification on macOS: `cargo fmt --check` and `cargo clippy --workspace --all-targets -D warnings` clean; `cargo test --workspace` all passed (nulis-core 101, nulis-search 8); `npm run lint`, `npm run typecheck` clean; Vitest 11 files / 120 tests passed. Tests that used the removed find bar and Search side panel were rewritten for the new window. Not yet tried by hand in the running app. Details and known limits: `handover/M21_HANDOVER.md`.

**Risks / rollback:** existing tests that use the inline find bar or Search side panel must be rewritten; large documents: lists are capped (10,000 matches / 5,000 lines) and say when truncated. Revert the M21 commit to roll back.

## M22 — Replace in Files

**Scope:** a Replace in Files tab with the Find in Files fields plus Replace with; a preview list of files and match counts before anything changes; explicit confirmation; open documents changed in memory (one undo step each, left dirty); closed files saved with the existing atomic save after a fingerprint check; a per-file success/failure report; size and file-count limits.
**Acceptance:** nothing is written before confirmation; a file changed on disk after the preview is skipped and reported; read-only documents are skipped; results agree with Find in Files.

**Result (completed 2026-10-20, on `main`):** all acceptance criteria covered by tests. Verification on macOS: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` clean; `cargo test --workspace` all passed (nulis-core 104, nulis-desktop 7); lint and typecheck clean; Vitest 11 files / 122 tests passed. Details: `handover/M22_HANDOVER.md`.

## M23 — External changes and Reload

**Scope**

1. Core `editor.externalChanges`: for every open local document, compare the file's current fingerprint with the one recorded at open/save; report `modified` or `deleted`, with the document's dirty state. Only `stat` calls; no file content is read.
2. Core `file.reload` (`discard` flag): re-open the file in the same document (same tab and ID): new source, empty undo history, cleared line-change markers, caret kept on the same line where possible, bookmarks past the end dropped; refused while dirty unless `discard` is set.
3. Core `editor.keepExternalVersion`: the user keeps the editor's version; the new fingerprint becomes the expected one so the next Save overwrites the outside change instead of failing.
4. Desktop: check when the window regains focus and every 3 seconds while focused; one prompt per change: "*name* was changed by another program" with **Reload** / **Keep Mine** (a warning when there are unsaved edits); for a deleted file **Keep Open** / **Close**. 
5. **File ▸ Reload from Disk** (palette, File menu, editor right-click menu): asks first when there are unsaved edits.

**Exclusions:** remote (SFTP) documents; three-way merge of both versions; a file-system watcher (polling `stat` on focus/interval needs no new dependency).

**Acceptance:** an outside edit is reported once; Reload shows the new text in every view of the document and keeps the tab; Keep Mine stops the prompt and a later Save succeeds; a deleted file is reported; Reload with unsaved edits needs confirmation; nothing is read from disk by the check itself.

**Result (completed 2026-10-20, on `main`):** all criteria covered by tests. Verification on macOS: fmt and clippy clean; `cargo test --workspace` passed (nulis-core 108, nulis-desktop 7); lint and typecheck clean; Vitest 124 passed. Details: `handover/M23_HANDOVER.md`.

## M30 — Integration and quality

**Delivered:** `commands.test.ts` checks that the core, the desktop, and the native menus list the same commands and that every presentation command has a handler (it found none missing from M20–M29); fixed a double undo and redo (the window's shortcuts re-ran keys the editor had handled); Ctrl Y redo on Windows and Linux; dialogs keep Tab inside (fields included) and pickers and the Rename dialog return focus when closed; the user guide, `FEATURE_PARITY.md`, and the new `PLATFORM_MATRIX.md` brought up to date. Also during M30 (user requests): the new icon, the batik About window, release 0.2.0, and the Homebrew cask.

**Result (completed 2026-10-20, on `main`):** fmt and clippy clean; `cargo test --workspace` passed (nulis-core 137); lint and typecheck clean; Vitest 15 files, 155 passed. Details: `handover/M30_HANDOVER.md`.

## M31 — Find window fits its content

**Delivered:** the Find and Replace window sizes its height to its content when it opens and whenever the content changes (tab, Options, a preview list, a status message); a manual resize is kept until then. `fit_find_window` (Find window only) grows the window by the difference between what the page needs and what it shows, so the title bar is accounted for on every platform; height stays between the minimum and the screen's usable height.

**Result (completed 2026-10-20):** see M32.

## M32 — Regular expressions and column search

**Delivered:** one regex style on the linear-time `regex` engine; unbounded patterns (`\w+`, `a.*b`) allowed with a 1 MiB match cap that is reported, never cut; matches across chunk edges found once; capture groups in Replace (`$1`, `${name}`, `$0`/`$&`, `\1`, `\n`, `\t`), checked before anything changes, for Replace, Replace All and Replace in Files (`search.replaceMatch` checks the range is still a match); Search in column (From/To, status-bar columns, `^`/`$` at the column edges) for every single-document action and highlighting; "Highlight all" painted from core matches of the visible lines (`search.matchesInRange`) and kept in step with the query and options; one-sentence pattern errors with a position (`search.validatePattern`), shown in the status line above the buttons; options remembered between sessions; a "Regex help" button that opens Nulis Help at the new Regular expressions section (`open_help_window` takes a topic). Also fixed: every command failure reached the UI as a generic message because the core's `{ kind, detail }` was never turned into an `Error`; `executeCommand` now does that. IPC protocol 36.

**Result (completed 2026-10-20, on `main`, not committed):** fmt and clippy clean; `cargo test --workspace` passed (nulis-core 140, nulis-search 22, nulis-desktop 9); lint and typecheck clean; Vitest 19 files, 170 passed. Checked in a debug build on macOS: window fit per tab, `id=\d+` with highlights, column 1–4 search, `$2:$1` Replace All and one-step undo, lookbehind message. Details: `handover/M31_M32_HANDOVER.md`.

## M33 — Theme contrast

**Delivered:** contrast floors applied to every theme when it loads (`crates/nulis-theme/src/contrast.rs`, `Theme::ensure_readable`): selection 1.6:1 against the editor background (an accent blend that keeps the theme's hue), text on it 4.5:1 or all the theme's text allows, apart from the current line where possible; secondary text 4.5:1; icons 3:1; disabled icons 1.8:1. New optional tokens `ui.icon` and `ui.iconDisabled`; toolbar and activity-bar icons use them, and disabled icons use a color instead of 40% opacity. The NvChad importer starts secondary text from `light_grey`; the catalog was regenerated. Theme files on disk are never changed. User request during M33: Nulis's own themes renamed **Nulis · Ember Night** and **Nulis · Paper Current** so they sort first by name. `THEME_SPEC.md` documents the tokens and floors.

**Result (completed 2026-10-20):** fmt and clippy clean; `cargo test --workspace` passed (nulis-theme 10, including every bundled theme against the floors and a faint installed theme corrected at load); lint and typecheck clean; Vitest 170 passed. Release build verified by the user on macOS: selection and icons readable in NvChad themes. Obsidian Ember and Onedark meet the selection floor but cannot fully meet the text-on-selection and current-line targets with their own colors.

## M34 — YAML and plist Validate and Beautify

**Delivered:** `yaml_tools.rs`: YAML Validate over every document in the stream with line and column, plus repeated keys in one mapping; YAML Beautify (user decision: Validate plus Beautify) re-writes from values with two-space indentation, keeps key order and line endings, asks first when the file has comments or anchors (`acceptLoss` on `tools.beautify`; the core refuses without it), and refuses files with tags. `plist_tools.rs`: plist Validate on the XML tokens — root `<plist>` with one value, key/value pairs in `<dict>` with no repeated key, only plist elements, integer (64-bit, hex), real, ISO date, Base64 data, empty true/false; plist Beautify is the XML formatter. plist is its own language (`.plist`, XML highlighting). Document summaries carry `tool`, so the desktop no longer repeats the language-to-tool mapping; the native Tools menu names YAML and plist. Dependency: `yaml-rust2` 0.13 (MIT/Apache-2.0). IPC protocol 37.

**Result (completed 2026-10-20):** fmt and clippy clean; `cargo test --workspace` passed (nulis-core 150); lint and typecheck clean; Vitest 171 passed.

## M35 — CSV/TSV, YAML, and binary plist

**Delivered:** a delimited lexer mode: CSV (`.csv`, separator guessed from the first line among comma, semicolon, tab, bar) and TSV (`.tsv`, `.tab`), each column in the next of six syntax colors, quoted fields with separators, doubled quotes, and line breaks, and text after a closing quote marked invalid. YAML additions to the code scanner: `---`/`...` markers, `&anchor`/`*alias`, tags, block scalars with their indented body as text, quoted keys. Binary property lists (`bplist` header) open read-only, report `binaryFormat` in the document summary, have no text tools, and are shown in the hex editor with a status message (user decision). Samples for CSV, TSV, and a binary plist. IPC protocol 38.

**Result (completed 2026-10-20):** syntax tests 15 passed (YAML forms, CSV columns, quoting, separator guess, TSV); core and Vitest suites pass (see the commit).

## Count All on huge files (user request after M35, 2026-10-20)

**Reported:** Count All on a 31.8 GB log stopped at "10000 matches on 4732 lines (limit reached)": it was built on Find All, which keeps every match with a line preview.

**Delivered:** `count_slice` in `nulis-search` counts matches and matching lines with no limit and bounded memory, in the same pass over the bytes (the scan loop now visits matches through a callback that knows whether a line break came before each one). `count_jobs.rs` in the core runs Count All as a background job over 64 MiB slices per document lock (`search.countStart`, `search.countProgress`, `search.countCancel`), stops if a document changes, and covers selections and all open files. The Find window shows progress and turns Count All into Cancel. Plain-text search now uses the regex engine's literal finder (byte-exact, ASCII-only case folding): 285 → 2,400 MB/s case-sensitive, 380 → 4,800 MB/s ignoring case. Also: the Previous/Next viewport buttons are filled with the theme accent (user request). IPC protocol 39.

**Result:** 4 GB test file counted in about 1 s, longest lock 18 ms; nulis-core 152 tests, Vitest 174 passed; verified by the user on the 31.8 GB file ("working and performance is fast").

## Icon redesign (user request, 2026-10-20)

The user asked for a modern, clean icon suited to macOS without the batik ribbon, and for batik to move to the About window instead. Three directions and five variations of the chosen one were presented on a design canvas; the user chose **A, Ink & Caret**: a bold cream N whose last stroke is an ember text caret, on a dark ink tile in the macOS shape (824 px body in a 1024 canvas, continuous corners, soft shadow).

- `apps/desktop/src-tauri/icons/nulis-source.svg` is the new master; every platform icon was regenerated with `npx tauri icon` (`icon.icns`, `icon.ico`, PNG sizes, Windows Store, iOS, Android). `docs/images/nulis-logo.svg` and `.png` and the README text follow it.
- `BrandMark.tsx` draws the same mark in theme colors: tile = raised surface, N = text color, caret = accent (light themes get a light tile with a dark N).
- About window: the user's batik artwork (parang and kawung, `apps/desktop/src/assets/about-batik-dark.jpg` and `-light.jpg`, cut from the supplied image) as its background, chosen by the theme's kind, anchored top right and faded from the dialog color; off in forced-colors mode.
- Checks: renders at 512, 128, 64, 32, and 16 px on light and dark; App tests for the dark and light About backgrounds; full suite passes (Vitest 149).

## Fix after M29 — secondary windows on Windows (user report, video)

**Problem:** on Windows, Settings and Find and Replace first appeared as an empty white or black window for one to two seconds, and showed the application menu bar, which also came back after saving settings. Causes: the menu was installed with `app.set_menu`, which on Windows and Linux draws it inside every window and reattaches it on every rebuild (M19 removed it only once, at creation); and windows were shown before their web view had drawn. Find, results, and Help were also rebuilt from scratch on every open.

**Fix (`apps/desktop/src-tauri/src/lib.rs`, `windowTheme.ts`, `SettingsWindow.tsx`):** on Windows and Linux the menu is attached to the editor window only (`attach_menu`); macOS keeps its global menu bar. Secondary windows are built hidden and shown when their page reports its first themed paint (`window_ready`, `useShowWhenReady`), or after 1.5 s at the latest. Find, results, and Help hide instead of closing, so reopening is instant and Find keeps its text; Settings still closes to discard unapplied changes.

**Verification:** clippy, workspace tests, lint, typecheck, Vitest 148 passed on macOS; CI built, installed, and started it on Windows; confirmed by hand on the Windows 11 test VM by the user (2026-10-20, build `191feef`).

## Fix after M29 — document map on long files (user report, Windows 0.1.1 video)

**Problem:** above 64 KiB the core sampled 1,024 slices of 128 bytes at even byte offsets. Rows started mid-line, glued neighboring lines together, and were placed by byte position, so the map did not look like the file and clicks landed unevenly (13% of the map opened line 670 of about 5,000, 38% only line 1,552). Not Windows-specific: the same code runs on every platform.

**Fix:** the core reads the whole text, with exact line numbers, up to 8 MiB (`DOCUMENT_MAP_FULL_BYTES`); above that each sample starts at a line start and stops at its line end (LF, CRLF, or CR). The desktop scales a document with more lines than fit to the map's height (no map scrolling), sizes the marker by visible lines, and a click opens the proportional line through `getViewportLine`.

**Tests:** core long-file map with CRLF lines and a 9 MiB sampled map (whole lines, increasing offsets, long-line fallback); desktop fit test (row count, no scrolling, click halfway opens line 2,501 and moves the marker there). Confirmed by hand on the Windows 11 test VM by the user (2026-10-20, build `191feef`).

## M29 — Column and clipboard tools

**Delivered:** Insert Number Sequence (column block, carets, or line starts; start, step, padding), Sum Selected Numbers (exact decimals), Left/Center/Right Justify in a column block, Convert to Fixed-Width / Delimited Columns (new line operations, delimiter detected or chosen), Copy and Append, Cut and Append, Paste and Copy, Paste from History (in-memory, 20 entries), Quick Open (Ctrl/⌘ P; open, folder, and recent files; core `workspace.listFiles`). A shared `Picker` now backs Open Recent, Quick Open, and Paste from History.

**Fix found during M29:** M28 raised the core IPC protocol to 34 but left the desktop constant at 33, so the M28 build would have refused to start. Both are 35 now, and `contracts.test.ts` compares them.

**Result (completed 2026-10-20, on `main`):** fmt and clippy clean; `cargo test --workspace` passed (nulis-core 135, nulis-filesystem 21); lint and typecheck clean; Vitest 14 files, 147 passed. IPC 35. Details: `handover/M29_HANDOVER.md`.

## M28 — File conveniences

**Delivered:** Save All (untitled files ask for a destination), Close All Files (the existing per-file save prompts), Open Recent (last 20 local files, searchable picker, missing files dropped, Clear Recent Files), Rename File (core `file.renameTo`, same folder, unsaved edits kept), Show in Places (expands the folders above the file and highlights it). File menu entries are always offered after the customizable part.

**Result (completed 2026-10-20, on `main`):** fmt and clippy clean; `cargo test --workspace` passed (nulis-core 127); lint and typecheck clean; Vitest 12 files, 141 passed. IPC 34. Details: `handover/M28_HANDOVER.md`.

## M27 — View features

**Delivered:** line-ending detection (status bar **LF / CRLF / CR / Mixed**, from the character-count pass, or the first 1 MiB above 64 MiB); conversion to LF, CRLF, or CR as one undo step from the status bar or the Format menu; Enter inserts the caret line's ending; word wrap remembered and in the View menu (Alt Z); editor zoom 50–300% (Ctrl/⌘ = - 0) with a status badge; show whitespace; show line endings; line-number toggle; column marker at 80; Always on Top. View options are saved per computer (`nulis.viewOptions.v1`). Also fixed an unhandled error in an M26 desktop test (a mock returned a document summary as selections).

**Result (completed 2026-10-20, on `main`):** fmt and clippy clean; `cargo test --workspace` passed (nulis-core 124); lint and typecheck clean; Vitest 12 files, 138 passed, no unhandled errors. IPC 33. Details: `handover/M27_HANDOVER.md`.

## M26 — Search and navigation

**Delivered:** F3/Shift+F3 find next/previous (opens Find when there is no search), Go to Matching Bracket (Ctrl/⌘ Shift \), Go to line:column, Go Back/Forward (Ctrl/⌘ [ / ]) over jumps, Next/Previous Change (Alt F5 / Alt Shift F5), List Bookmarks (results window), Delete Bookmarked Lines, word and line counts, code point at the caret, native Search menu, and palette entries. Not done from the plan: incremental search, character names.

**Result (completed 2026-10-20, on `main`):** fmt and clippy clean; `cargo test --workspace` passed (nulis-core 116); lint and typecheck clean; Vitest 130 passed. IPC 32. Details: `handover/M26_HANDOVER.md`.

## M25 — Format and transforms

**Scope (as delivered):** invert case, capitalize words, Base64 encode/decode (new `TextTransform`s); indent, outdent, toggle comment, trim trailing spaces, tabs→spaces, leading spaces→tabs, sort ascending/descending/unique (new `LineOperation`s with `LineFormatOptions`); insert date/time; Format menu (native), Edit-menu line commands, toolbar Format list, palette entries for all of these and for the M20 line commands; Tab/Shift+Tab and Ctrl/⌘ /. Also: theme-colored status badges (user request).

**Result (completed 2026-10-20, on `main`):** fmt and clippy clean; `cargo test --workspace` passed (nulis-core 113, nulis-syntax 13); lint and typecheck clean; Vitest 128 passed. IPC 31. Details: `handover/M25_HANDOVER.md`.

## User requests queued during M21

- **M23 External changes and Reload:** prompt when another program changes or deletes an open file (Reload / Keep mine), plus a manual Reload command (also Revert to Saved). Core already fingerprints sources and flags changed sources during recovery (`FEATURE_PARITY.md`).
- **M24 New application icon:** done — new master SVG and icon sets; the in-window mark follows the theme; About and README updated.
- **Done in M21:** the reference product's name was removed from code and documents (README, syntax comment, CLEAN_ROOM, DECISIONS, IMPROVEMENT_SPEC, this plan). It remains in earlier git commit contents and messages; rewriting history was not done because it is destructive and needs the owner's decision.

## Design decisions and deviations

- M21 (revised after user testing): Find and Replace and the matching-lines list are separate native windows like Settings, not panels inside the main window. Verification after the change: Vitest 121 passed; `cargo test -p nulis-desktop` 7 passed; clippy, lint, typecheck clean.

- Milestones are numbered M19+ to continue `ROADMAP.md` rather than restart at M01.
- Reference shortcuts are not copied; Nulis keeps its own bindings.

## Known issues and open questions

- Windows/Linux verification of the M19 window fix is pending.
- Clipboards/Templates/Spacing/Hide-Show Lines submenus were not expanded in the screenshots; their behavior is unclear and not planned.

## Next steps

0. The plan is complete. What remains is checking by hand, listed in `PLATFORM_MATRIX.md`: the M20–M29 features on each OS, the new icon and About background, and screen readers.
1. Not planned yet: native menus showing the shortcuts the page handles (Alt Z, zoom, Ctrl/⌘ P, F3), a Settings field for the column marker, and "Open with" registration on Windows and Linux.
