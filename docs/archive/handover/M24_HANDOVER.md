# M24 handover — New application icon

**Status:** completed 2026-10-20 on `main` (visual check by rendering; the running app was rebuilt for the user to see).

## Objectives

User request: a modern, beautiful logo; the in-window logo follows the selected theme, the application icon does not need to; update About; add the logo to the README. The user supplied two concept images (a paper-and-pen "N" with Javanese batik) as inspiration.

## Implementation

- `apps/desktop/src-tauri/icons/nulis-source.svg`: new original vector master. An N formed by a cream paper sheet (left stem, text lines, folded corner, a page curl at the top), a batik ribbon (diagonal: indigo ground, rust flowers and leaves, gold dots, gold rim), and a pen (right stem: batik barrel, gold band and nib) on a deep navy tile.
- All platform icons regenerated from it with `npx tauri icon src-tauri/icons/nulis-source.svg` (PNG sizes, `icon.icns`, `icon.ico`, Windows Store logos, Android/iOS sets).
- `apps/desktop/src/BrandMark.tsx`: the same artwork as an inline SVG whose colors come from theme tokens through CSS classes (`styles.css`, `.brand-mark .mark-*`): paper = `--accent`, nib and rim = `--warning`, ribbon = `--editor-bg` with an `--accent` tint, flowers = `--accent`, tile = `--raised`, edge = `--border`; forced-colors mode uses system colors. Opacity layering instead of `color-mix` for older webviews. Per-instance IDs via `useId`.
- `App.tsx`: the title-bar brand button and the About page use `BrandMark` (28 px and 64 px).
- `README.md`: logo at the top (`docs/images/nulis-logo.png`, 512 px, from the icon set) and a sentence on what it shows; `docs/images/nulis-logo.svg` is a copy of the master.

## Tests and checks

- Rendered the master at 512 and 32 px, and the theme-colored mark with the Ember Dark and Paper Light colors (scratch renders, not committed); all legible.
- fmt, clippy, lint, typecheck clean; Vitest 124 passed (no test asserts on the logo).

## Known issues

- The README screenshots in `docs/images/screenshots/` still show the old title-bar "N" mark; retake them when convenient.
- At 16–32 px the batik detail reduces to color; a simplified small-size variant could be drawn if it looks busy in a taskbar.

## Resume

Next: M25 Format and transforms.
