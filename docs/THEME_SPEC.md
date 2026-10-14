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

Contrast validation warns when primary text/background, selection, caret, or focus indication falls below configured accessibility thresholds. Language modules map captures to stable syntax tokens and may provide fallbacks, not arbitrary CSS.

Built-in themes include the original `themes/nulis-ember-dark.nulis-theme.json` and `themes/nulis-paper-light.nulis-theme.json` plus all 97 converted palettes from the pinned NvChad Base46 snapshot in `themes/nvchad-catalog.json`. The searchable customization panel switches themes live by updating CSS custom properties, editor render styles, diff surfaces, and xterm colors without changing document state.

User installation accepts an explicitly selected regular `.nulis-theme.json` file no larger than 256 KiB. The native catalog validates the complete document, refuses symlinks, duplicate IDs, and built-in ID shadowing, caps the catalog at 64 themes, and copies it atomically beneath application data. Discovery is deterministic and exposes only validated themes. Built-ins cannot be removed, and an active user theme must be switched before uninstall. Nulis JSON theme files cannot contain CSS, scripts, imports, URLs, or remote resources.

## NvChad/Base46 import

The same installer accepts a regular `.lua` theme file containing `M.base_16` and `M.base_30` color tables and `M.type = "dark"` or `"light"`. The importer reads the text without executing Lua, maps the Base46 palette to Nulis semantic tokens, and installs a generated `nvchad.<filename>` JSON theme. All 16 `base_16` colors must resolve to `#RRGGBB` values. Entries can use literal colors, literal `local` color constants, or references to `M.base_30` entries. `base_30` entries may be omitted when a semantic token has a `base_16` fallback. Computed palette values and escaped strings are unsupported. Lua outside the extracted palette, local literal colors, and type assignment has no effect on the imported colors. The normal 256 KiB file limit, symlink refusal, duplicate-ID check, and 64-user-theme catalog limit also apply. Bundled themes are read-only and do not use the user-theme quota.
