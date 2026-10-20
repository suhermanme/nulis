# Release checklist

Nulis is currently a locally built application. A public release needs a hosted
CI run and platform-specific signing material. Do not publish CI's unsigned
bundle artifacts as a final release.

## Validate source and dependencies

```sh
npm ci
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm run lint
npm run typecheck
npm run test
npm audit --audit-level=high
cargo audit
```

The current audit has zero known vulnerabilities. RustSec also reports four
transitive warnings (`paste`, `proc-macro-error`, `smartstring`, and `glib`);
review their dependency paths and usage before public distribution. The
unpatched `fast-float` vulnerability was removed by upgrading Boa to 0.20.

The separate `fuzz/` workspace uses nightly Rust and `cargo-fuzz`. Run
`cargo +nightly fuzz run piece_table -- -max_total_time=300` before a release;
archive any crashing inputs as document regressions.

## Build and inspect bundles

The CI workflow builds Debian, macOS `.app`, and Windows NSIS artifacts on
their native hosted runners. Review the build logs, install each artifact on
its target OS, test open/edit/save/restart/recovery and keyboard/screen-reader
flows, and compare the artifact hash to the release upload. Run the
performance corpus in `benchmarks/README.md` on target hardware and record
peak resident memory and first-viewport time.

For a local macOS application bundle:

```sh
cd apps/desktop
npx tauri build --bundles app --ci
```

macOS bundles are ad-hoc signed (`bundle.macOS.signingIdentity` is `"-"` in `tauri.conf.json`), locally and in CI. That seals the bundle so Gatekeeper can offer **Open Anyway** under System Settings ▸ Privacy & Security for a downloaded copy, instead of reporting it as damaged; CI verifies the signature on the app and on the copy inside the `.dmg`. For a release without that step, replace `"-"` with an Apple Developer ID identity and provide notarization credentials (`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, and related variables). The installers do not show the license as a click-through agreement: the GPL does not require acceptance to run the program, and its full text is in the application under Help ▸ About Nulis ▸ View License.

## Platform test matrix

`docs/PLATFORM_MATRIX.md` records what CI and people have verified on each operating system. Update it after a check by hand.

## Manual interaction checks (M15–M18)

Automated tests cover the logic; these need a person on each target OS before a milestone in `IMPROVEMENT_SPEC.md` is marked complete:

- Pointer: double- and triple-click on identifiers, accented text, emoji, tabs, and wrapped lines; drag a selection forward, backward, and across a viewport edge; copy with Option (macOS) or Control; cancel with Escape; confirm a tab drag never starts a text drag.
- IME: compose text next to a selection and after a drag move.
- Line change indicators: unsaved, saved, and deletion markers in every built-in theme and in high-contrast mode; save failure; undo across a save; two panes on one document.
- Workspaces: drag a tab onto each side of the content, split one pane of a workspace, rename it, drag a pane back to the tab strip, resize dividers by pointer and keyboard, restart with one file missing, and use the minimum window size.
- Settings: open from the application menu, the shortcut, the palette, and the toolbar and confirm one window; close with unapplied changes; apply a toolbar change and check the native menu; move the window to a second display, disconnect it, and reopen.
- Assistive technology: tab, pane, divider, toolbar, and settings-section names with VoiceOver, NVDA, and Orca.

## Publish

Tag and upload only after the signed artifacts have been installed and checked
on supported platforms. Publish checksums, license notices, release notes, and
a private security-reporting channel alongside the downloads. Publishing and
updater metadata are separate approval steps.

Tauri references: [distribution](https://v2.tauri.app/distribute/),
[macOS signing](https://v2.tauri.app/distribute/sign/macos/), and
[Windows signing](https://v2.tauri.app/distribute/sign/windows/).

## Update the Homebrew cask

After the release is published, point `Casks/nulis.rb` at it: set `version` to
the new version and `sha256` to the `Nulis_<version>_universal.dmg` line of the
release's `SHA256SUMS.txt`. Check it from a local tap of your working copy
(Homebrew audits casks only by name), then commit it to `main`:

```sh
brew style Casks/nulis.rb
brew tap nulis/local "$PWD"   # after committing locally; taps read the last commit
brew audit --cask --online nulis/local/nulis
brew untap nulis/local
```

Users who tapped the repository get the new version with `brew upgrade --cask nulis`.
