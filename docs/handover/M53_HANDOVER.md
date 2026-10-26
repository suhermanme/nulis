# M53 handover — split the core's largest files

Fourth milestone of `docs/IMPROVEMENT_PLAN_6.md`. Branch `refactor/split-core`, from `main` at `58a6db9`. Code moves only: no behavior, public item or test assertion changed.

## Result

| File | Before | After |
| --- | --- | --- |
| `crates/nulis-core/src/editor.rs` | 8,504 | 2,406 |
| `crates/nulis-core/src/lib.rs` | 6,298 | 1,560 |

The largest file in `nulis-core/src` is now `editor/tests.rs` (2,508 lines). The plan's ceiling was roughly 2,500.

## Where things went (one commit each)

`editor.rs` → `editor/`:

| New file | Lines | What |
| --- | --- | --- |
| `tests.rs` | 2,508 | the editor's test module |
| `types.rs` | 623 | the 45 public request/response types, re-exported with `pub use types::{…}` so every path is unchanged |
| `viewport.rs` | 709 | viewports, the document map, the Markdown preview source and images |
| `selections.rs` | 505 | selections, rectangular selection, word at a point, caret movement |
| `search.rs` | 414 | find, replace a match, Replace All, Find in Files |
| `recovery.rs` | 356 | crash-recovery checkpoints, candidates, restore, discard |
| `tools.rs` | 302 | Validate and Beautify |
| `hex.rs` | 292 | the hex view and hex edits |
| `languages.rs` | 280 | the language catalog, associations, capabilities, View As |
| `compare.rs` | 159 | comparing two documents and applying a change |
| `bookmarks.rs` | 106 | toggle, move to, clear bookmarks |

`lib.rs` → `src/`:

| New file | Lines | What |
| --- | --- | --- |
| `builtin_commands.rs` | 2,018 | the 31 `register_*` functions and the workbench action table |
| `tests.rs`, `settings_tests.rs` | 1,740, 232 | the two test modules |
| `arguments.rs` | 647 | the 65 command argument structs (all `deny_unknown_fields`) and their defaults |
| `script_host.rs` | 191 | `CoreScriptHost`, what a running script may do |

`editor.rs` keeps the `DocumentManager` core (create, open, save, close, editing, undo and redo, transforms), the session state, the range helpers that six editing modules share, and the error type. `lib.rs` keeps the `NulisCore` façade, its state, and the error helpers.

## What changed besides position

- **Visibility.** Moved items that `editor.rs`, a sibling or the crate root still use got `pub(super)`. The two compare requests and their methods were `pub(super)` in `editor` (read by the crate root), so they are `pub(crate)` one level down and re-exported as `pub(crate)`: the same reach. Fields of the argument structs and of `CoreScriptHost` are `pub(super)`, because the façade reads or builds them.
- **Imports.** Each new file imports explicitly (pedantic clippy forbids `use super::*` outside tests). Library names are imported from their own crates, not through the parent.
- **Tests.** Only import paths changed: `editor/tests.rs` imports `CompareMode` from `nulis_diff`, and five calls name `super::types::wildcard_match`.
- **One doc link.** `[MAX_LINE_PREVIEW_BYTES]` in `types.rs` names its path (`super::MAX_LINE_PREVIEW_BYTES`); it had resolved through `editor`'s imports.

## How each move was checked

The moves were scripted, not hand-copied:
- **Extraction.** Each item was taken with its doc comments and attributes, using a brace matcher that skips strings, chars and comments.
- **Imports and visibility.** These were fixed from compiler errors, in a loop.
- **Per-commit checks.** Every commit then passed:
  1. **Token comparison** against the previous commit. Ignoring `use` lines, `mod` lines, `pub(super)` and whitespace, the code before and after must be the same multiset of tokens. Every commit's only additions are its new file's `//!` doc comment and the closing brace of the new `impl` block, plus `pub(crate)` ×4 in the compare commit.
  2. **String literals.** Test modules were dedented except on lines inside multi-line string literals (38 such lines in `lib.rs`'s tests, none in the editor's). All 1,207 string literals of `lib.rs`'s tests are byte-identical.
  3. **Tests.** `cargo fmt --check`, `cargo clippy -p nulis-core --all-targets -D warnings`, and the core tests: 205 before and after every commit.

## Checks run (macOS, 2026-10-26)

- Frontend: Vitest 262 tests pass, the same as before the moves (after the fix below), with lint and typecheck clean.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` clean; `cargo test --workspace --locked` 471 passed, 0 failed (the same as on `main`); `cargo +1.90 check --workspace --all-targets --locked` clean.
- **Public API.** `cargo doc -p nulis-core` was built from `main` and from this branch:
  - `all.html` lists the same 180 public items;
  - `NulisCore` has the same 60 public methods and `DocumentManager` the same 143.
- **Doc warnings.** `cargo doc` gives 3 warnings, the same 3 as on `main` (public docs linking to private constants in `count_jobs.rs`, `editor/find_all.rs` and `print_jobs.rs`). They predate this plan and are left for M55.

## Found after the moves: a frontend test reads the Rust source

`apps/desktop/src/commands.test.ts` imports `crates/nulis-core/src/lib.rs?raw` and looks there for the `WORKBENCH_ACTIONS` table and for every command ID the native menus send. After `builtin_commands.rs` took both, two tests failed and a third ("every presentation command has a handler") passed while checking nothing, because it found no table. The Rust checks could not see this. It surfaced in the full Vitest run and in the Frontend job of CI run 40345434639. Fix: the test reads `lib.rs` and `builtin_commands.rs` together, and fails with a message naming both files if the table is missing, so a later move cannot silently empty it. `contracts.test.ts` also reads `lib.rs`, only for `IPC_PROTOCOL_VERSION`, which stays there. `ARCHITECTURE.md` cited `lib.rs:90` for that constant and now names it instead of a line. Verified: Vitest 262/262, lint, typecheck.

## Not done

- Tests were not split per topic. `editor/tests.rs` (2,508 lines) still holds every editor test, and `tests.rs` the core's. Splitting them would mean deciding where each test belongs, which this mechanical milestone did not do.
- The frontend is untouched (M54).

## Next

M54: split `App.tsx` and `EditorViewport.tsx`.
