# Nulis theme specification

Theme files are declarative JSON with the suffix `.nulis-theme.json`. They contain no scripts or remote resources.

## Shape

This abbreviated excerpt shows the envelope; an installable file must include every token in the following section.

```json
{
  "$schema": "https://schemas.example.invalid/nulis/theme/v1.json",
  "id": "nulis.example",
  "name": "Example",
  "version": 1,
  "kind": "dark",
  "colors": {
    "editor.background": "#101318",
    "editor.foreground": "#E7E9EE",
    "editor.selection": "#39566B",
    "ui.accent": "#66C2A5"
  }
}
```

Required identity fields are `id`, `name`, integer `version`, and `kind` (`dark` or `light`). Colors accept opaque `#RRGGBB` or alpha `#RRGGBBAA`. Unknown keys are rejected in strict validation and preserved only by explicit migration tooling.

## Required semantic tokens

- Editor: `editor.background`, `editor.foreground`, `editor.selection`, `editor.currentLine`, `editor.lineNumber`, `editor.caret`, `editor.gutter`, `editor.focus`.
- Syntax: `syntax.comment`, `syntax.keyword`, `syntax.string`, `syntax.number`, `syntax.function`, `syntax.type`, `syntax.property`, `syntax.escape`, `syntax.invalid`.
- Chrome: `ui.background`, `ui.panel`, `ui.surface`, `ui.raised`, `ui.border`, `ui.text`, `ui.mutedText`, `ui.accent`, `ui.accentForeground`, `ui.focus`, `ui.danger`, `ui.warning`, `ui.success`, `ui.menu`, `ui.toolbar`, `ui.tab`, `ui.statusBar`, `ui.statusBarText`, `ui.inputBackground`, `ui.inputBorder`, `ui.hover`, `ui.overlay`.
- Terminal: `terminal.background`, `terminal.foreground`, `terminal.cursor`.
- Diff: `diff.inserted`, `diff.deleted`, `diff.modified`.

## Optional tokens

- `ui.icon`: toolbar and side-bar icons. Without it, Nulis uses a shade between `ui.mutedText` and `ui.text`.
- `ui.iconDisabled`: icons of unavailable commands. Without it, Nulis blends `ui.icon` into `ui.toolbar`.

## Contrast floors

Every theme, bundled or installed, is checked when it is loaded. Colors below a floor are strengthened toward the theme's own accent or text color (and on toward white or black when that is not enough); colors that already meet the floors are left exactly as written. The theme file on disk is never changed.

| Color | Floor (WCAG contrast ratio) |
| --- | --- |
| `editor.selection` against `editor.background` | 1.6:1 |
| `editor.selection` against `editor.currentLine` | 1.25:1, as far as the floors above and below allow |
| `editor.foreground` on `editor.selection` | 4.5:1, or as much as the theme's text has (its contrast with the background ÷ 1.6) |
| `ui.mutedText` against `ui.toolbar`, `ui.panel`, `ui.background` | 4.5:1 |
| `ui.icon` against the same surfaces | 3:1 |
| `ui.iconDisabled` against the same surfaces | 1.8:1 |

A new selection is the theme's accent blended into the editor background, so it keeps the theme's character. The NvChad catalog is generated with these floors applied (`cargo run -p nulis-theme --example import_base46 -- themes/nvchad-source themes/nvchad-catalog.json`), and a test checks every bundled theme against them. Language modules map captures to stable syntax tokens and may provide fallbacks, not arbitrary CSS.

Built-in themes include Nulis's own **Nulis · Ember Night** (`themes/nulis-ember-dark.nulis-theme.json`) and **Nulis · Paper Current** (`themes/nulis-paper-light.nulis-theme.json`), listed first because themes sort by name, plus all 97 converted palettes from the pinned NvChad Base46 snapshot in `themes/nvchad-catalog.json`. The searchable customization panel switches themes live by updating CSS custom properties, editor render styles, diff surfaces, and xterm colors without changing document state.

User installation accepts an explicitly selected regular `.nulis-theme.json` file no larger than 256 KiB. The native catalog validates the complete document, refuses symlinks, duplicate IDs, and built-in ID shadowing, caps the catalog at 64 themes, and copies it atomically beneath application data. Discovery is deterministic and exposes only validated themes. Built-ins cannot be removed, and an active user theme must be switched before uninstall. Nulis JSON theme files cannot contain CSS, scripts, imports, URLs, or remote resources.

## Custom themes

A bundled theme is never edited. **Duplicate Selected** copies it under a new validated id (`<source>.copy<N>`), and the copy becomes an ordinary user theme. The customization panel edits the copy's supported editor, syntax, and chrome colors with a native color picker, a hexadecimal field, and the arrow keys; **Save Colors** sends the edited colors to `theme.save`, which atomically replaces the user file, refuses built-in ids, and re-applies the contrast floors on load, so the file keeps the author's colors and Nulis shows a readable copy. **Revert** discards unsaved edits, and **Remove Selected** uninstalls only user themes, never a built-in and never the active theme.

Editing colors previews live in the Appearance panel before any save; leaving the panel restores the committed appearance. The **Apply theme colors** preference (default: off) persists the selected theme's colors across restarts: when it is on, the window chrome is painted from the selected theme's readable colors in every Nulis window, and a user theme is marked with `data-custom="true"` on the window root. Built-in themes keep their own chrome appearance in both states.

## NvChad/Base46 import

The same installer accepts a regular `.lua` theme file containing `M.base_16` and `M.base_30` color tables and `M.type = "dark"` or `"light"`. The importer reads the text without executing Lua, maps the Base46 palette to Nulis semantic tokens (secondary text starts from `light_grey`, as `grey_fg` is meant for comments), applies the contrast floors, and installs a generated `nvchad.<filename>` JSON theme. All 16 `base_16` colors must resolve to `#RRGGBB` values. Entries can use literal colors, literal `local` color constants, or references to `M.base_30` entries. `base_30` entries may be omitted when a semantic token has a `base_16` fallback. Computed palette values and escaped strings are unsupported. Lua outside the extracted palette, local literal colors, and type assignment has no effect on the imported colors. The normal 256 KiB file limit, symlink refusal, duplicate-ID check, and 64-user-theme catalog limit also apply. Bundled themes are read-only and do not use the user-theme quota.
