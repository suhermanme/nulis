# M43 handover — first-party migration

**Date:** 2026-10-23. **Branch:** `feat/language-capabilities` (not pushed). **Plan:** `IMPROVEMENT_PLAN_4.md` §5 M43. **Tracker:** `IMPLEMENTATION_TRACKER.md`, M43.

## What changed

- **Golden record** `crates/nulis-core/tests/tool_golden.rs` + `tests/golden/tools.json`: 26 documents × Validate, Beautify, Beautify with consent, Minify, from 0.4.0. Re-record only on purpose (`NULIS_RECORD_GOLDEN=1`), with a release note.
- **Providers** `crates/nulis-core/src/builtin_providers.rs`: `builtin_registry()` registers `builtin.{json,xml,yaml,plist}.{validate,format}`, `builtin.json.compact`, `builtin.markdown.preview`. Labels ("JSON", "XML", "YAML", "plist", "Markdown") are what menus and messages show.
- **Editor** (`editor.rs`): `DocumentCapabilities` on every session (cached) and summary; `validate_document` and `format_document` resolve providers and call them through `CapabilityRegistry::call`; `set_disabled_capabilities`, `language_capabilities`, `summaries`. `ToolKind` is gone.
- **Commands:** `language.set` returns the summary; new `language.capabilities`, `editor.summaries`. Settings window allowlist adds `settings.state`, `settings.saveChecked`, `language.capabilities`; `settings.saveChecked` notifies other windows.
- **Interface:** toolbar, palette, native Tools menu (`set_tools_menu` takes labels), YAML consent (`formatMayLoseData`), Markdown Edit/View switch (`capabilities.preview`) read capabilities. Settings ▸ Languages ▸ Language features has the switches (Apply/Discard) and lists `files.associations`.
- IPC protocol **44**.

## Try it in the app

Back up and quit Nulis first (`handover/PLAN3_RESUME.md`). Open a `.json`, a `.yaml` with a comment, a `.plist`, and a `.md`: tools and the Markdown switch as before. View As ▸ JSON on a `.txt`: Validate/Beautify/Minify appear at once. Settings ▸ Languages: untick Beautify and Minify for JSON, Apply: they vanish for the open JSON document; tick again, Apply: they return. Untick Preview for Markdown: the Edit/View switch goes away.

## Next: M44

Core auto-indentation (`editor.newline`, rule families, settings `editor.insertSpaces`/`tabSize`/`autoIndent`/`detectIndentation`). The owner chose `full` as the default. Commit a byte-exact test of today's Enter/Tab/Backspace behavior first, so `autoIndent: none` can be shown to reproduce it.
