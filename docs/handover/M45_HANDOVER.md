# M45 handover — shared autocomplete

**Date:** 2026-10-23. **Branch:** `feat/language-capabilities` (not pushed). **Plan:** `IMPROVEMENT_PLAN_4.md` §5 M45. **Tracker:** `IMPLEMENTATION_TRACKER.md`, M45.

## What changed

- **Vocabulary** (`nulis-syntax`): `Vocabulary`, `LanguageRegistry::{vocabulary, word_bytes}`; `LexerSpec::completion_words` (HTML element names); Python and JSON word lists.
- **Engine** (`nulis-language/src/completion.rs`): `CompletionProvider` (`priority`, `opens_automatically`), `WordCompletion`, `KeywordCompletion`, `SnippetCompletion` and snippet tables, `word_prefix`, `rank`, `expand_snippet`.
- **Core** (`nulis-core/src/editor/completion.rs`): `DocumentManager::{completions, accept_completion}`, `SnippetSession` (moved by `record_bookmark_edit`, cleared by Undo/Redo), Tab visits placeholders (`insert_tab`). Providers registered in `builtin_providers.rs::completion_providers`. `DocumentCapabilities.completion` is `auto` or `manual`.
- **Commands:** `completion.request { expectedRevision, requestId, explicit, minPrefix }`, `completion.accept { expectedRevision, replaceStart, replaceEnd, text, snippet }` (recordable), `edit.triggerSuggest` (workbench action "Show Suggestions").
- **Interface:** `CompletionPopup.tsx`; `EditorViewport` request/accept/keys/ARIA; `App` reads `editor.suggest` (`suggestSettingsOf`); Settings ▸ Editor ▸ Suggestions; Languages page names the switch "Suggestions".

## Try it in the app

Back up and quit Nulis first. In a `.js` file type `co`: the list opens; arrows move, Enter inserts. Type `fo` and pick the `for` snippet: `i` is selected, Tab goes to `length`, then into the body. In a `.sql` file type `SEL`: `SELECT`. In a `.txt` file nothing opens by itself; Ctrl+Space (or Option+Esc on macOS) shows the document's words. Settings ▸ Editor ▸ Suggestions turns automatic opening off.

## Next: M46

Expand bundled coverage and the owner's self-hosting goal: the repository self-test, an ignore-file language, JSX/TSX tags, JS/TS regex literals and `${…}`, the §2.9 defects (CSS comment marker, Python brackets, `.scpt` binary), Markdown heading folds, YAML indentation folds, the approved associations (`.jsonc`, `Dockerfile.*`, `Containerfile.*`), and the remaining matrix cells (snippets for Swift, Zig, …).
