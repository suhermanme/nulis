# M46 handover — expand bundled coverage

**Date:** 2026-10-23. **Branch:** `feat/language-capabilities` (not pushed). **Plan:** `IMPROVEMENT_PLAN_4.md` §5 M46. **Tracker:** `IMPLEMENTATION_TRACKER.md`, M46.

## What changed

- **Defects:** CSS Toggle Comment `/* */` (`LexerSpec::toggles_line_comments`); Python bracket pairs (bracket node kinds + both-ends check in `collect_node`); compiled AppleScript read-only hex (`binary_kind` in `editor.rs`).
- **Associations:** `FileMatcher::Prefix` (`name.*`, ranked between exact names and extensions); Dockerfile `dockerfile.*`/`containerfile.*`; JSON `.tsbuildinfo`; new `jsonc`, `tsx`, `ignore` languages. Golden detection test rows annotated "M46".
- **Highlighting:** lexer `ecmascript` (regex literals, template `${…}`) and `jsx` modes with a `Frame` stack; JavaScript and TSX have JSX, TypeScript does not.
- **Folds:** `push_heading_folds` (Markdown), `Scan::indentation_folds` (YAML).
- **Snippets:** Swift, Zig, Kotlin, PHP, PowerShell, Perl, Visual Basic/VBScript, AppleScript.
- **Tests:** `nulis-core/tests/every_language.rs`, `nulis-syntax/tests/self_hosting.rs`.
- **Release notes draft:** `docs/release-notes/v0.5.0.md` (marked draft).

## Try it in the app

Back up and quit Nulis first. Open `apps/desktop/src/App.tsx`: JSX tags and attributes are colored, View As shows TSX. Open a `.ts` with a template string: `${…}` content is code-colored; `/^x+$/g` is a string color while `a / b` is not. Fold a Markdown heading and a YAML key. Toggle Comment in a `.css` file. Open `.gitignore` and a `Dockerfile.dev`.

## Next: M47

Optional LSP: verify candidate servers (license, platforms, maintenance) and choose the second after rust-analyzer; `nulis-lsp` client (JSON-RPC over stdio, lifecycle, timeouts, cancel, restart, position encoding); servers only from user settings with approval bound to the exact command; adapters for completion, diagnostics, formatting; shared diagnostics UI; fake-server CI tests.
