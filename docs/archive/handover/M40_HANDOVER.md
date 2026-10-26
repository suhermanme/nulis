# M40 handover: docs and release

Plan: `IMPROVEMENT_PLAN_3.md` §4 (M40). Status: done 2026-10-22; `m36-print` merged into `main` (fast-forward, not pushed).

## What changed

| Document | Change |
| --- | --- |
| `USER_GUIDE.md` (and so Nulis Help, which is built from it) | Files ▸ *Printing and PDF*: Print, the exports, what each view prints, the page limits, Page Setup, fonts and scripts. Print shortcut and two limits in their tables. Written during M39. |
| `DECISIONS.md` | ADR-0013: pages laid out once in the core for Print and PDF. |
| `FEATURE_PARITY.md` | A Print, Page Setup, and Export to PDF line under Specialized tools. |
| `IMPROVEMENT_PLAN.md` §4.1 | S04 row marked done; printing removed from the deferred list. |
| `ROADMAP.md`, `PLATFORM_MATRIX.md` | M36–M40 complete; the macOS check by hand recorded. |
| `README.md` | Print and Export to PDF under Tools; `nulis-print` in the crate table. |
| `release-notes/v0.4.0.md` | Notes for the next release. |

## Decisions

- **`settings.schema.json` is unchanged.** Page Setup values are stored with the editor's view preferences (`nulis.printSettings.v1`), not in `settings.json`, which the app never writes (see `M39_HANDOVER.md`). Adding `print.*` keys to the schema would describe settings that nothing reads.
- **The version is not bumped.** The notes are written for 0.4.0, but `package.json`, `Cargo.toml`, and `tauri.conf.json` still say 0.3.0. Bumping and tagging is the release step, after the Windows and Linux checks; the release workflow refuses a tag without `release-notes/v<version>.md`.

## Open

- Done: Windows 11 and Ubuntu 26.04 checked by hand on test VMs (results in `M39_HANDOVER.md`), and Nulis 0.4.0 released on 2026-10-23.
- Linux font fallback when only CFF CJK fonts are installed (the Ubuntu VM has Droid Sans Fallback, a TrueType font, so it was not seen).
