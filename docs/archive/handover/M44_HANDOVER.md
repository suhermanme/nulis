# M44 handover — core auto-indentation

**Date:** 2026-10-23. **Branch:** `feat/language-capabilities` (not pushed). **Plan:** `IMPROVEMENT_PLAN_4.md` §5 M44. **Tracker:** `IMPLEMENTATION_TRACKER.md`, M44.

## What changed

- **Rules** `crates/nulis-language/src/indent.rs`: `IndentProvider`, `RuleIndenter` with families Carry, Brace, Python, Yaml, Makefile, Markup, Keywords (`RUBY`, `LUA`, `BASH`, `VISUAL_BASIC`, `PLSQL`, `APPLESCRIPT`), `detect_unit`, whitespace helpers. Tests in `indent/tests.rs`; fuzz target `fuzz/fuzz_targets/indent_rules.rs`.
- **Providers** `builtin_providers.rs::indentation_providers` maps languages to families and comment markers.
- **Core** `crates/nulis-core/src/editor/indentation.rs`: `AutoIndent`, `IndentSettings` (from `settings.json` `editor.*`), `IndentSettingsTable`, `IndentationSummary`; `DocumentManager::{newline, insert_tab, insert_typed_text, line_format_defaults, set_indent_settings}`; Backspace hook `indentation_backspace`. Sessions cache `detected_indent` and `indentation`; `refresh_language_state` refreshes capabilities and indentation together.
- **Commands:** `editor.newline`, `editor.tab` (recordable); `editor.insertText` takes `typed`; `editor.lineOperation` without `options` uses the document's unit.
- **Interface:** Enter/Tab/typing call the core; `.editor-surface` gets `tabSize`; Settings ▸ Editor ▸ Indentation; Languages page names the switch "Auto-indent rules".
- IPC protocol **45**.

## Try it in the app

Back up and quit Nulis first. In a `.rs` file type `fn a() {` and Enter: indented line; Enter between `{}` splits. Type `}` on an indented empty line: it moves left; one Undo restores. In a `.py`: `def f():` Enter indents; `return 1` Enter outdents; `else:` jumps left. In `.yaml` Enter after `key:` uses spaces even with tabs set; in `Makefile` a rule gets a tab. Open a 2-space JSON file: Tab inserts two spaces. Settings ▸ Editor ▸ Indentation ▸ Auto-indent: Off makes Enter a plain line break again.

## Next: M45

Shared autocomplete: `completion.request` / `completion.cancel` (revision + request ID), word, keyword, and snippet providers, one `CompletionPopup`. Owner choices: popup opens automatically after 2 identifier characters in languages with keyword providers, never in Plain Text or Markdown; shortcut Ctrl+Space everywhere plus Option+Esc on macOS, both rebindable.
