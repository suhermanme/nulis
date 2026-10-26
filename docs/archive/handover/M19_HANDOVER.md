# M19 handover — Audit and foundation

**Status:** completed 2026-10-20 (Windows/Linux runtime check of the window fix pending).

## Objectives

Audit 18 reference-editor menu screenshots against Nulis, produce the improvement plan and tracker, and record a baseline. A user-reported Windows bug (blank Settings window; menu bar in Settings on Windows/Linux) was fixed during this milestone.

## What was done

- `docs/IMPROVEMENT_PLAN.md`: screenshot inventory (S01–S18), feature-by-feature comparison with evidence, priorities, milestone roadmap M20–M25, deferred list.
- `docs/IMPLEMENTATION_TRACKER.md`: status table and the M20 definition.
- `apps/desktop/src-tauri/src/lib.rs`: Settings/Help window commands made `async`; native menu handlers spawn window creation; new `detach_application_menu` removes the menu bar from Settings and Help on Windows/Linux.

## Tests

- Baseline before changes: `cargo test -p nulis-core` 86 passed; Vitest 115 passed.
- After the fix: `cargo clippy -p nulis-desktop --all-targets -- -D warnings` clean; `cargo test -p nulis-desktop` 6 passed.

## Decisions

- Milestones continue the existing numbering (M19+).
- Comparison is kept to product-neutral behavior (see `CLEAN_ROOM.md`); screenshots stay outside the repository.

## Known issues

- The non-macOS branch of `detach_application_menu` was not compiled locally (no Windows/Linux target installed). CI must build it, and someone must open Settings and Help on Windows and Linux.

## Resume

Read `IMPROVEMENT_PLAN.md` §9 and the M20 section of the tracker, then implement M20.
