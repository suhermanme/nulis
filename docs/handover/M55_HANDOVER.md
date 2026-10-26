# M55 handover — documentation and contributor on-ramp

Last milestone of `docs/IMPROVEMENT_PLAN_6.md`. Branch `docs/repo-docs`, from `main` at `f9a6c3b` (after M54).

## What changed (one commit each)

1. **Archive.** `docs/archive/` holds the finished plans (`plans/`: the spec and plans for M15–M49) and the handovers of M19–M49 with the print milestones' PDF tools (`handover/`), with an index (`archive/README.md`). Plan 6's handovers stay in `docs/handover/`. The moves are `git mv`, so history follows. A script resolved every relative Markdown link before the move and rewrote the 12 that changed. 60 more lines named an archived path in plain text (`handover/M26_HANDOVER.md` and the like, mostly in the tracker) and now name the new one. Code comments that cite a plan by name only (`IMPROVEMENT_PLAN_3.md` §3.2) were left alone: the name still finds it.
2. **`WORK_STATE.md`.** The 75 KB log, M00 to 0.5.2, moved unchanged to `archive/WORK_STATE_HISTORY.md`. The new `WORK_STATE.md` (2 KB) says what is released, what is on `main` since, what is open, and how to verify a change. Its old "Verification" (M13) and "Next" (M14) sections were stale. The tracker and the roadmap now point at the history for the sections they cite.
3. **README.** 34,788 → 24,352 bytes. Moved verbatim, with a short version and a link left behind:

   | Content | New home |
   | --- | --- |
   | Installing | new `docs/INSTALL.md` |
   | Principles, large-file engine, crates, desktop app | top of `ARCHITECTURE.md` |
   | What a shipped build depends on, signing | `RELEASE.md` |
   | Where Nulis stores data | `USER_GUIDE.md` under Files (the guide used to point at the README for it) |
   | Testing, building parts, contributing | `CONTRIBUTING.md` |

   A line-by-line comparison found every line that left the README in its new home, except three whose links had to change. The section finder ignores fenced code. A first attempt that did not was caught, because `# x86_64 arm64` inside a shell block read as a heading, and was reverted before anything was committed.
4. **Contributor on-ramp.**
   - `CONTRIBUTING.md`: setting up, rules for a change, the CI checks, commit style, DCO sign-off and how to add it after the fact.
   - Issue forms: `bug_report.yml` (version, system, install method, steps, expected, actual, logs) and `feature_request.yml`. `config.yml` turns off blank issues and links to private security reporting and the user guide.
   - `pull_request_template.md`.
   - `SECURITY.md` gains "Reporting a vulnerability". It used to say a private channel "must be published before public release", and none was.
   - A new CI job, **Signed-off commits**, runs `.github/scripts/check-sign-off.sh` on pull requests. Dependabot's commits are exempt, since it cannot sign off. Tested on this branch (2 commits, all signed) and on a throwaway repository with a signed, an unsigned and a Dependabot commit: it named the unsigned one, skipped Dependabot, and exited 1. Testing caught one bug: in a `case` pattern `[bot]` is a character class, so the literal had to be quoted.
5. **`cargo doc`.** The three warnings M53 noted (public docs linking to constants in private modules) are gone: the constants are named as code instead of linked.

## Checks run (macOS, 2026-10-26)

- Link check (a script that resolves every relative link and anchor in the tracked Markdown): 0 broken in 82 files. It was first checked on a sample with one broken file and one broken anchor, and found both.
- Vitest: the same 262 tests with the same names and results (the Help window reads `USER_GUIDE.md`). `cargo test --workspace`: 471 passed. `cargo doc -p nulis-core`: 0 warnings. `ci.yml` and the issue forms parse.

## Open, for the owner

- **Private vulnerability reporting is off on GitHub.** `SECURITY.md`, `CONTRIBUTING.md` and the issue chooser now send reporters to the repository's **Report a vulnerability** form, which works only once it is on (Settings ▸ Code security ▸ Private vulnerability reporting).
- **README size.** It is 24 KB, not the plan's 12 KB. Reaching 12 KB would mean condensing "What Nulis does" (about 6 KB), which rewrites the project's description rather than moving it.
- **The sign-off job** runs only on pull requests, so it has not run in CI yet. Merging M55 through a pull request would exercise it.
- **A link check in CI** (the plan's optional item). The script used here could run as a CI step.

## Next

Release 0.5.3 (M50–M55): release notes, the version bump, the tag, and the cask.
