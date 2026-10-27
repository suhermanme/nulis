# Platform test matrix

What has been verified on each operating system, and how. Update it when a check is run by hand; keep entries to what was actually observed. Last updated 2026-10-27 for Nulis 0.5.4 (secondary windows open without the 1.5 s wait); the 0.5.3 entries are for the Markdown preview remote images and plan 6; the 0.5.2 entries are for the toolbar labels, window cleanup and integrated terminal; the M49 entries are from Nulis 0.5.1; the 0.5.0 entries below are dated 2026-10-24.

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

Nulis 0.5.0 passed every job above on all three systems (CI run 40096525220 on `7bf01ba`, 2026-10-24), and the release run for tag `v0.5.0` (40096867716) installed and started each package before publishing it. The Homebrew cask (`Casks/nulis.rb`) uses the `.dmg`'s `SHA256SUMS.txt` entry and passes `brew style` and `brew audit --cask --online`.

Nulis 0.5.1 passed every job on all three systems (CI run 40216554644 on `1b929ef`, 2026-10-26, after run
40215756053 caught a rustfmt offense and an unused import on the release commit), and the release run for tag
`v0.5.1` (40217132454) installed and started each package before publishing it. Its cask pins the published
0.5.1 `.dmg` hash and passes `brew style`; the online cask audit has not been run yet, because Homebrew
refuses to audit a cask from an untrusted tap — trusting the tap is the owner's decision.

Nulis 0.6.0 passed every job on all three systems (CI run 40509704621 on `729563d`, 2026-10-27), and the release
run for tag `v0.6.0` (40509715573) installed and started each package before publishing it. The cask pins the published
0.6.0 `.dmg` hash (`d38ae8cb…`, checked against a fresh download and `SHA256SUMS.txt`); `brew style` and the online audit
from a local tap found no offenses.

Nulis 0.5.4 passed every job on all three systems (CI run 40421043868 on `822a1dc`, 2026-10-27), and the release
run for tag `v0.5.4` (40421050772) installed and started each package before publishing it. The cask pins the published
0.5.4 `.dmg` hash (`a6624baa…`, checked against a fresh download and `SHA256SUMS.txt`); `brew style` and the online audit
from a local tap found no offenses.

Nulis 0.5.3 passed every job on all three systems (CI run 40360634929 on `aebc3ba`, 2026-10-26), and the release
run for tag `v0.5.3` (40365326411, 2026-10-27) installed and started each package before publishing it; one tag push
started one run this time. The cask pins the published 0.5.3 `.dmg` hash (`a2526cce…`, checked against a fresh download
and `SHA256SUMS.txt`) and passes `brew style`. The owner ran `brew audit --cask --online suhermanme/nulis/nulis` on 2026-10-27: no offenses, with the downloaded `.dmg` matching the pinned hash. A stale local tap (`nulis/local`, left from an earlier release check) had made the short name ambiguous, and was untapped first.

Nulis 0.5.2 passed every job on all three systems (CI run 40315778439 on `d39894c`, 2026-10-26), and the release
run for tag `v0.5.2` (40316439728) installed and started each package before publishing it. One tag push started
two release runs; the duplicate (40316443328) was cancelled while it waited, before it built anything. The cask pins the
published 0.5.2 `.dmg` hash (`96c3bfd0…`, checked against a fresh download) and passes `brew style`; the online
audit is still the owner's, for the same reason.

Tests that guard platform differences: `contracts.test.ts` (the desktop and core protocol versions match), `commands.test.ts` (the core, the desktop, and the native menus list the same commands), and the Windows Rust job, which caught a Rename File check that relied on inode numbers that Windows does not have and a language server that the reader had seen stop before Windows reported the exit.

## By hand

| Area | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Settings, Find and Replace, Help windows (no menu bar, no blank flash, instant reopen) | – | yes, Windows 11 (build `191feef`, 2026-10-20) | not yet |
| Document map on long files | – | yes, Windows 11 (build `191feef`, 2026-10-20) | not yet |
| M20–M29 features (keys, Find window, line endings, view options, file commands, column and clipboard tools, Quick Open) | not recorded | not yet | not yet |
| New icon in the Dock, Finder, taskbar, and Start menu; About background in dark and light themes | not recorded | not yet | not yet |
| M47 language servers: configure and approve `rust-analyzer`, gutter dot and underline, Problems panel jump, revoke and restart | not yet | not yet | not yet |
| Custom themes (M49): Duplicate, color editor with live preview, Save Copy/Save Colors/Save Name, rename, Revert, Remove Selected; the applied theme paints every window; built-ins refused; copy survives a restart | partly, by the user on a local build (2026-10-25): applying a user theme, editing colors, and the panel's readability notice. Relaunch and restore, Remove Selected, and the removed **Apply theme colors** switch are not yet checked | not yet | not yet |
| 0.5.0: automatic indentation, suggestions, Problems panel and declarative language plugin (`examples/plugins/nulis-nim`) | not yet | not yet | not yet |
| 0.3.0: Find window fit, regular expressions, Search in column, groups in Replace | yes, debug build driven by hand-written automation (2026-10-20) | not yet | not yet |
| 0.3.0: readable selection and icons in NvChad themes, the two Nulis themes first | yes, by the user on a release build (2026-10-20) | not yet | not yet |
| 0.3.0: Count All on a 31.8 GB log (no limit, progress, Cancel) | yes, by the user on a release build (2026-10-20) | not yet | not yet |
| 0.3.0: YAML and plist tools, CSV/TSV and YAML highlighting, binary plist in hex | not recorded | not yet | not yet |
| Print, Page Setup, and Export to PDF (M36–M39): matching pages, progress and Cancel, 2,460-page log, selection, hex dump, fallback fonts, Markdown, theme colors | yes, by the user on a debug build (2026-10-22; `archive/handover/M38_HANDOVER.md`, `M39_HANDOVER.md`) | yes, Windows 11 test VM, CI build (2026-10-22): print, Markdown print without the browser's header and footer, export, page counts | yes, Ubuntu 26.04 test VM, CI build (2026-10-23): print in portrait and landscape, export, page counts |
| 0.5.4: Settings, Find and Replace and Help appear at once, not after 1.5 s | yes, by the user (2026-10-27; local release build of `667dea1`) | yes, by the user on Windows 11 (2026-10-27; CI installer, run 40419841547) | yes, by the user on Ubuntu 26.04 (2026-10-27; CI `.deb`, run 40419841547) and Fedora 44 (2026-10-27; published 0.5.4 `.rpm`, checksum matched, no missing libraries) |
| 0.5.3: Markdown preview remote images: placeholder and **Load remote images**, an `https:` image loading, an unreachable one saying so, `http:` never fetched (checked against a local server's request log), Print following the preview; M54 smoke run (typing, split pane, terminal divider and hide/show, Print, Export to PDF, Rename File) | yes, by the user on a release build (2026-10-26; `handover/M52_HANDOVER.md`, `handover/M54_HANDOVER.md`) | not yet | not yet |
| Screen readers (VoiceOver, NVDA, Orca) | not yet | not yet | not yet |

The checks still to do by hand are listed in `RELEASE.md` under *Manual interaction checks*, and each milestone's handover in `docs/handover/` notes what it did not try by hand.
