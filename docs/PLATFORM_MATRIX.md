# Platform test matrix

What has been verified on each operating system, and how. Update it when a check is run by hand; keep entries to what was actually observed. Last updated 2026-10-20, for Nulis 0.3.0.

## Automated (every push to `main` and every release tag)

| Check | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Formatting, Clippy (`-D warnings`), Rust tests | CI (`macos-latest`) | CI (`windows-latest`) | CI (`ubuntu-latest`) |
| Desktop lint, typecheck, Vitest | – | – | CI (`ubuntu-latest`) |
| Build the bundle | `.app` and `.dmg` | NSIS installer | `.deb` and `.rpm` |
| Install it as a user would | signature checked, `.dmg` mounted | silent install for the current user | `apt` (Ubuntu) and `dnf` (Fedora container) with library check |
| Start it and stay running for 15 s | yes | yes | yes, on a virtual display |
| Release packages | universal `.dmg` | `x64-setup.exe` | `.deb`, `.rpm` |

Also in CI: document fuzzing, a large-file smoke benchmark, and dependency advisories.

Tests that guard platform differences: `contracts.test.ts` (the desktop and core protocol versions match), `commands.test.ts` (the core, the desktop, and the native menus list the same commands), and the Windows Rust job, which caught a Rename File check that relied on inode numbers that Windows does not have.

## By hand

| Area | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Settings, Find and Replace, Help windows (no menu bar, no blank flash, instant reopen) | – | yes, Windows 11 (build `191feef`, 2026-10-20) | not yet |
| Document map on long files | – | yes, Windows 11 (build `191feef`, 2026-10-20) | not yet |
| M20–M29 features (keys, Find window, line endings, view options, file commands, column and clipboard tools, Quick Open) | not recorded | not yet | not yet |
| New icon in the Dock, Finder, taskbar, and Start menu; About background in dark and light themes | not recorded | not yet | not yet |
| 0.3.0: Find window fit, regular expressions, Search in column, groups in Replace | yes, debug build driven by hand-written automation (2026-10-20) | not yet | not yet |
| 0.3.0: readable selection and icons in NvChad themes, the two Nulis themes first | yes, by the user on a release build (2026-10-20) | not yet | not yet |
| 0.3.0: Count All on a 31.8 GB log (no limit, progress, Cancel) | yes, by the user on a release build (2026-10-20) | not yet | not yet |
| 0.3.0: YAML and plist tools, CSV/TSV and YAML highlighting, binary plist in hex | not recorded | not yet | not yet |
| Print, Page Setup, and Export to PDF (M36–M39): matching pages, progress and Cancel, 2,460-page log, selection, hex dump, fallback fonts, Markdown, theme colors | yes, by the user on a debug build (2026-10-22; `handover/M38_HANDOVER.md`, `M39_HANDOVER.md`) | not yet | not yet |
| Screen readers (VoiceOver, NVDA, Orca) | not yet | not yet | not yet |

The checks still to do by hand are listed in `RELEASE.md` under *Manual interaction checks*, and each milestone's handover in `docs/handover/` notes what it did not try by hand.
