# M50 handover — repository hygiene

First milestone of `docs/IMPROVEMENT_PLAN_6.md`. Branch `chore/repo-health`, from `main` at `9a0d0db`. Metadata and documentation only; no source code changed.

## What changed

- **`Cargo.toml`**: `[workspace.package] repository` is `https://github.com/suhermanme/nulis` (was `https://example.invalid/nulis`).
- **Member crates**: all 19 (`apps/desktop/src-tauri`, `benchmarks`, every `crates/*`) gained `repository.workspace = true` after their `license.workspace = true` line. None inherited it before, so the fix to the root alone would have changed no package. `fuzz/Cargo.toml` is outside the workspace and `publish = false`, and stays as it was.
- **`tmp/`**: deleted at the owner's request (plan §5 Q1). It was untracked and held 30 accessibility probes (`ax*.swift`, compiled `ax*bin`) and screenshots; no tracked file referred to it. `tmp/` is now in `.gitignore`.
- **`README.md`** ("Get the dependencies"): `git clone` names `https://github.com/suhermanme/nulis.git` instead of `<repository-url>`, and a new paragraph says `target/` grows past 10 GB and `cargo clean` reclaims it.

## Checks run

- `cargo metadata --format-version 1 --no-deps`: every package reports `https://github.com/suhermanme/nulis`.
- `cargo fmt --all --check` clean; `cargo check --workspace --locked` clean, so `Cargo.lock` did not change.
- The full suite was not re-run: no Rust or TypeScript source changed. CI runs it on push.

## Next

M51: Node 24 for the build, Node 24 releases of the GitHub actions (removes the Node 20 deprecation warning), an MSRV job for `rust-version = "1.82"`, and Dependabot.
