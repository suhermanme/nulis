# M27 handover — View features (paused before implementation)

**Status:** paused 2026-10-20 at the start of the milestone because the session's usage limit was near. No M27 code was written; `main` is clean at the commit that adds this note.

## Re-plan

M27 "View and file" was too large for one milestone and is split:

- **M27 View features:** line-ending detection, status label, and conversion; Enter inserts the document's line ending; font zoom (Ctrl/⌘ = / - / 0); word wrap remembered and in the View menu (Alt Z); show whitespace and line endings; line-number toggle; a column marker (e.g. at 80); Always on Top.
- **M28 File conveniences:** Save All, Close All, Open Recent (in-app picker), Rename File, show the file in Places.
- The former M28 (column and clipboard tools) becomes M29 and integration becomes M30.

## Findings so far (for the next session)

- The status bar's line-ending label is a fixed `"LF"` in `apps/desktop/src/App.tsx` (search `<span>LF</span>`).
- Enter inserts `"\n"` regardless of the file (`EditorViewport.tsx`, `onKeyDown`, `insertText("\n")`).
- `ViewportLine` (`crates/nulis-core/src/editor.rs`, ~line 267; built at ~5043 and ~5080) has no line-ending field; adding `ending: "lf" | "crlf" | "cr" | null` there is the cheapest way to render line-ending marks and choose Enter's ending.
- Suggested core API: `editor.lineEndings` (bounded scan: counts of LF/CRLF/CR in the first 1 MiB → `lf | crlf | cr | mixed | none`) and `editor.convertLineEndings { expectedRevision, target }` (whole document, one undo step, bounded like Replace All: refuse above 64 MiB or past the replacement-count limit).
- Font size lives in `preferences.fontSize` (customization appearance); zoom can be a separate per-user multiplier in `localStorage` so it does not rewrite Settings.
- Word wrap is local `useState(false)` in `EditorViewport.tsx`; persist it the way `lineChangeDefault` is persisted (`preferences.ts`).
- Always on Top needs `core:window:allow-set-always-on-top` in `apps/desktop/src-tauri/capabilities/default.json`.

## Resume

1. Read `IMPROVEMENT_PLAN.md` §9, this note, and `IMPLEMENTATION_TRACKER.md`.
2. `git status` should be clean on `main`.
3. Implement M27 in this order: line endings (core + status bar + Enter), word wrap persistence, zoom, whitespace/line-ending marks, line numbers and column marker, Always on Top. Test each; run the full suite (`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm run lint`, `npm run typecheck`, `npm test`).
