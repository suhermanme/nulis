# Work state

What is true of the repository now. The full history, from M00 to 0.5.2, is in [archive/WORK_STATE_HISTORY.md](archive/WORK_STATE_HISTORY.md); progress by milestone is in [IMPLEMENTATION_TRACKER.md](IMPLEMENTATION_TRACKER.md).

## Released: Nulis 0.5.2 (2026-10-26)

Tag `v0.5.2`: the integrated terminal (resizable panel, the editor's font, a cursor that shows focus, a shell that survives hiding), the status bar's mode chip as an indicator, and toolbar labels with the **More** overflow menu. IPC protocol 48. Release notes: [release-notes/v0.5.2.md](release-notes/v0.5.2.md). `Casks/nulis.rb` pins the 0.5.2 `.dmg`.

## On `main` since 0.5.2 (not released; planned as 0.5.3)

[Plan 6](IMPROVEMENT_PLAN_6.md), repository health:

- **M50:** the real repository URL in every crate, and the scratch `tmp/` folder removed.
- **M51:** Node 24 (from `.nvmrc`) and Node 24 releases of every GitHub action, so CI no longer warns about Node 20. The real Rust minimum is 1.90 (ADR-0026), checked by a CI job. Dependabot is on.
- **M52:** the Markdown preview asks before fetching images from the web (`markdown.preview.remoteImages`, default `ask`), and never fetches over plain `http:`. IPC protocol **49**.
- **M53, M54:** the largest core and UI files are split by topic, with code moved only and the public API and test names unchanged.
- **M55:** finished plans and handovers archived, a shorter README with `docs/INSTALL.md`, `CONTRIBUTING.md` with DCO sign-off, issue forms, private vulnerability reporting, and CI checks for sign-offs and documentation links.

## Open

- **Release 0.5.3** after M55: release notes, version bump, tag, cask.
- **Owner's checks:** the Windows and Linux checks in [PLATFORM_MATRIX.md](PLATFORM_MATRIX.md), and `brew audit --cask --online nulis`, which needs the tap trusted on the owner's machine.

## Verifying a change

All of these pass on `main`:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm run lint
npm run typecheck
npm run test        # also after moving Rust code: some tests read Rust source files
npm run frontend:build --workspace @nulis/desktop
```

CI runs them on Linux, macOS and Windows, plus a build on the minimum Rust, fuzzing, a large-file benchmark, a dependency audit, and building, installing and starting every bundle.
