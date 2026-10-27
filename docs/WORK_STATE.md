# Work state

What is true of the repository now. The full history, from M00 to 0.5.2, is in [archive/WORK_STATE_HISTORY.md](archive/WORK_STATE_HISTORY.md); progress by milestone is in [IMPLEMENTATION_TRACKER.md](IMPLEMENTATION_TRACKER.md).

## Released: Nulis 0.6.0 (2026-10-27)

Tag `v0.6.0`: the terminal's right-click menu (Copy, Paste, Select All, Clear) and a grab pointer while selected text is dragged (branch `feat/drag-pointer-terminal-menu`, merged fast-forward to `main` through pull request #12 and deleted), with fixes since 0.5.4: light themes reach the toolbar controls, panels, title bar and terminal selection; caret placement and Up/Down on wrapped rows; a full-height caret on empty lines; bookmarks on one line only, drawn as a ribbon clear of the fold arrow; no view jump when the editor takes focus back; minimum window sizes on GNOME Wayland; tree-sitter 0.27 parse deadline and the Dependabot updates. IPC protocol 49, unchanged. Release notes: [release-notes/v0.6.0.md](release-notes/v0.6.0.md). Manual checks of these changes on each system are not yet recorded in [PLATFORM_MATRIX.md](PLATFORM_MATRIX.md).

## Nulis 0.5.4 (2026-10-27)

Tag `v0.5.4`: a fix only. Settings, Find and Replace, Help, Lines Containing and Print Preview opened about 1.5 s late on every system, because their page reported it was ready from inside an animation frame, which a hidden web view never runs; they now report as soon as the themed page is laid out (`667dea1`, CI run 40419841547 green). The owner checked the fix on macOS, Windows and Linux. IPC protocol 49, unchanged. Release notes: [release-notes/v0.5.4.md](release-notes/v0.5.4.md). Tag `v0.5.4` on `822a1dc`: CI run 40421043868 passed every job; release run 40421050772 built, installed, started and published the universal `.dmg`, the Windows installer, the `.deb`, the `.rpm` and `SHA256SUMS.txt`. `Casks/nulis.rb` pins the published 0.5.4 `.dmg` (`a6624baa…`); `brew style` and the online audit from a local tap found no offenses.

## Nulis 0.5.3 (2026-10-26)

Tag `v0.5.3` on `aebc3ba`: plan 6, M50–M55, listed below. IPC protocol 49. Release notes: [release-notes/v0.5.3.md](release-notes/v0.5.3.md). CI run 40360634929 passed every job; release run 40365326411 built, installed, started and published the universal `.dmg`, the Windows installer, the `.deb`, the `.rpm` and `SHA256SUMS.txt`. `Casks/nulis.rb` pins the published 0.5.3 `.dmg` (`a2526cce…`).

## Nulis 0.5.2 (2026-10-26)

Tag `v0.5.2`: the integrated terminal (resizable panel, the editor's font, a cursor that shows focus, a shell that survives hiding), the status bar's mode chip as an indicator, and toolbar labels with the **More** overflow menu. IPC protocol 48. Release notes: [release-notes/v0.5.2.md](release-notes/v0.5.2.md). `Casks/nulis.rb` pins the 0.5.2 `.dmg`.

## In 0.5.3, since 0.5.2

[Plan 6](IMPROVEMENT_PLAN_6.md), repository health:

- **M50:** the real repository URL in every crate, and the scratch `tmp/` folder removed.
- **M51:** Node 24 (from `.nvmrc`) and Node 24 releases of every GitHub action, so CI no longer warns about Node 20. The real Rust minimum is 1.90 (ADR-0026), checked by a CI job. Dependabot is on.
- **M52:** the Markdown preview asks before fetching images from the web (`markdown.preview.remoteImages`, default `ask`), and never fetches over plain `http:`. IPC protocol **49**.
- **M53, M54:** the largest core and UI files are split by topic, with code moved only and the public API and test names unchanged.
- **M55:** finished plans and handovers archived, a shorter README with `docs/INSTALL.md`, `CONTRIBUTING.md` with DCO sign-off, issue forms, private vulnerability reporting, and CI checks for sign-offs and documentation links.

## Open

- **Owner's checks:** the Windows and Linux checks in [PLATFORM_MATRIX.md](PLATFORM_MATRIX.md). (The online cask audit for 0.5.3 passed on 2026-10-27.)

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
