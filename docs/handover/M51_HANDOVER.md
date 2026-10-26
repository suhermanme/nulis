# M51 handover — toolchain and dependency maintenance

Second milestone of `docs/IMPROVEMENT_PLAN_6.md`, on branch `chore/repo-health` after M50.

## What changed

- **Node 24 for the build.** New `.nvmrc` (`24`). Every `actions/setup-node` step in `ci.yml` and `release.yml` reads it with `node-version-file: .nvmrc` (before: 20 in the frontend job, 22 elsewhere). `package.json` `engines` is `node >=24.0.0`, `npm >=11.0.0`; `package-lock.json` changed only in that root `engines` entry.
- **Node 24 for the actions.** The warning the owner saw on every run was GitHub's: *Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to run on Node.js 24: actions/checkout@v4, actions/setup-node@v4* (annotation on CI run 40315778439). Each action's `runs.using` was read from its `action.yml` per major:

  | Action | Was | Now | Notes |
  | --- | --- | --- | --- |
  | `actions/checkout` | v4 (node20) | v7 (node24) | no input changes |
  | `actions/setup-node` | v4 (node20) | v7 (node24) | v5/v6 changed automatic caching; we set `cache: npm` explicitly |
  | `actions/upload-artifact` | v4 (node20) | v7 (node24) | v5 still ran on node20 |
  | `actions/download-artifact` | v4 (node20) | v8 (node24) | v5 changed by-ID paths (we use `pattern:`); v8 errors on digest mismatch |
  | `softprops/action-gh-release` | v2 | v3 (node24) | runtime move only |
  | `Swatinem/rust-cache` | v2 | v2 | already node24 |

- **Rust minimum 1.90** (ADR-0026). `cargo +1.82 check` fails before compiling: 1.82 cannot parse `cssparser-macros` 0.7.1's edition-2024 manifest. The highest `rust_version` in `cargo metadata` is 1.90 (Tauri 2.12 and its plugins, `tree-sitter-language` 0.1.9), and `cargo +1.90 check --workspace --all-targets --locked` passes. New CI job `msrv` (**Rust minimum version**) reads `rust-version` from `Cargo.toml` with `sed` and runs `cargo check` on that exact toolchain via `dtolnay/rust-toolchain@master`.
- **Clippy at the new floor.** Clippy's MSRV-aware `manual_is_multiple_of` now applies: `crates/nulis-large-file/src/line_index.rs:218` and `crates/nulis-core/src/editor/print_export.rs:836` use `.is_multiple_of()`. Behavior is identical for a non-zero divisor, and both divisors are non-zero (`stride == 0` is rejected in `LineIndex` construction; the other is the literal 3).
- **Dependabot.** `.github/dependabot.yml`: weekly, cargo / npm / github-actions at `/`, at most 5 open PRs each, minor and patch updates grouped per ecosystem, all action updates grouped.
- **Docs.** README prerequisites (Rust 1.90, Node 24 with `.nvmrc`, npm 11); ADR-0026 in `DECISIONS.md`. `docs/RELEASE.md` names no versions, so it is unchanged.
- **Test.** `crates/nulis-syntax/tests/self_hosting.rs` counts `.nvmrc` as plain text by nature, beside `LICENSE` and `*.txt`; without that the new file fails the "every repo file gets a language" check.

## Checks run (macOS, 2026-10-26)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` clean.
- `cargo test --workspace --locked`: 469 passed, 0 failed.
- `cargo +1.90 check --workspace --all-targets --locked` clean (separate target dir, since removed).
- `npm run lint`, `npm run typecheck` clean; Vitest: desktop 26 files / 253 tests, plugin SDK 1 file / 2 tests.
- Workflow YAML parses. The MSRV `sed` extracts `1.90` from `Cargo.toml`.

## Not verified here

- **Node 24 itself.** The local machine has Node 26; fetching a Node 24 binary with `npx -p node@24` was refused by the sandbox, so CI is the first run on 24.
- **The workflows.** The new action majors and the `msrv` job run for the first time on push. Check that the run shows no Node 20 annotation.
- **Dependabot** starts after the merge to `main` (it reads the default branch).

## Follow-up opportunity

M37 embedded fonts whole because `subsetter` 0.2 needed Rust 1.85, above the old 1.82 floor. That constraint is gone; font subsetting in PDF export is now possible if wanted (not in this plan).

## Next

M52: Markdown preview remote images, setting `markdown.preview.remoteImages` = `block` | `ask` | `allow`, default `ask`, and `http:` removed from the CSP.
