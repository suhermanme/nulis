# M48 handover — external plugin readiness

**Date:** 2026-10-24. **Branch:** `feat/language-capabilities`. **Plan:** `IMPROVEMENT_PLAN_4.md` §5 M48. **Tracker:** `IMPLEMENTATION_TRACKER.md`, M48.

## What changed

- **Plugin API v2 (`crates/nulis-plugin-host`).** `PLUGIN_API_VERSION = 2` and the accepted range is `1..=2`. `api_version` defaults to `1`, so every v1 manifest keeps its meaning. `main` is now `Option<String>`: required by v1, required by v2 whenever commands or activation events are present, and optional for a package that contributes only languages. `contributes.languages` is validated for identifiers, display names, associations, `firstLine`, comments, brackets, highlight tables, indentation, completion switches, and snippets. Caps: 32 languages, 64 associations, 1,024 keywords plus types, 32 string rules, 16 bracket pairs, 256 snippets, 4 KiB per snippet, 4,096-byte regex with a 1 MiB compiled-size limit, 256 KiB manifest. Unknown fields, invalid regexes, duplicate language IDs, empty delimiters, bad brackets, control characters, and escaping paths are rejected at install time.
- **Owned syntax data (`crates/nulis-syntax`).** Built-in lexer tables are compile-time `LexerSeed` values; `LexerSpec`, `StringRule`, `Vocabulary`, `CommentSyntax`, and the scanner are owned, so plugins never hand static references to the core. `LanguageModule::declarative(descriptor, lexer, comments, vocabulary, first_line)` builds a language from validated data, compiles `firstLine` with a bounded `RegexBuilder`, and returns `SyntaxError::InvalidDeclarative` on bad data. `firstLine` patterns are tried only after built-in content detection and after file-name matching fails; built-in patterns win, then plugins ordered by ID. Plugin languages can add associations to a built-in ID but cannot redefine it.
- **Capability state and provider rebuild (`crates/nulis-core`).** `PluginActivation { grant, enabled, disabled_capabilities }` is stored in the existing digest-bound `plugin-grants.json` approval document. Old documents (no `enabled` or `disabledCapabilities`) mean “approved and enabled”. `PluginCapability` covers `commands`, `languages`, `highlight`, `indentation`, `completion`, and `snippets`. `NulisCore::refresh_plugin_languages` clears every plugin language, association, and provider, then rebuilds from approved and enabled plugins after install, approval, revoke, enable/disable, capability change, uninstall, and startup. Each contribution is applied independently: a bad one adds an error to `plugin.languageErrors` and the rest keep working.
- **New commands and IPC 47.** `plugin.setEnabled`, `plugin.setCapabilityEnabled`, `plugin.languageErrors`, and `language.conflicts`. `plugin.*` command descriptors and `plugin.activateStartup`/`run_plugin` now require approval, `enabled`, and the `commands` capability. `language.conflicts` is available to the Settings window allowlist. `IPC_PROTOCOL_VERSION` and `contracts.ts` both say **47**.
- **Providers for plugin languages.** `RegexIndenter` handles `increaseAfter` and `outdentOnType`; keyword/word completion and `OwnedSnippetCompletion` provide completion. Provider IDs are `plugin:{id}/{language}.{kind}`, lowercased and hashed when longer than 128 bytes. All plugin providers are removed before each rebuild.
- **Desktop.** `PluginPanel` lists contributed languages, shows an Enable/Disable switch, and exposes a checkbox per capability family the plugin declares. Settings ▸ Languages has a **File association conflicts** fieldset that shows each pattern with its claims and sources; choosing one drafts `files.associations` and saves it on Apply, keeping existing associations. `contracts.ts` gained `PluginCapability`, `PluginLanguageSummary`, `AssociationClaim`, `LanguageConflict`, and the expanded `PluginSummary`.
- **Docs and example.** `PLUGIN_API.md` documents both versions, the complete declarative schema, limits, capability switches, and approval storage. `PLUGIN_GUIDE.md` has a declarative-language chapter, limits, and troubleshooting. `LANGUAGE_CAPABILITIES.md` §9 records the implementation. `examples/plugins/nulis-nim` is a complete v2 package with no JavaScript and a README; a host test loads it.
- **Tests.** New plugin-host tests cover v2 acceptance, v1 compatibility, v2 entry-point restrictions, hostile manifests, approval switches with pre-v2 documents, summaries, and the example package. Syntax tests cover declarative highlighting/completion/detection, invalid declarative data, and clearing plugin languages/associations. Language tests cover `RegexIndenter`. Core tests cover the builder, provider filtering, bad-contribution isolation, provider-ID bounds, sync/removal, conflicts, plain-text fallback, and command-level enable/disable/capability behavior. Desktop adds a capability-switch test and a conflict-choice test.

## Security review for executable language providers

M48 deliberately **does not add executable language providers**. The security review outcome is a scope decision:

- a declarative language is data only; it has no code path, no path/URL fields, and regexes are compiled at install time with bounded automata;
- the executable plugin runtime keeps its existing process isolation, 2-second deadline, JSON-only protocol, and permission boundary;
- adding `document.readText` / `document.replaceText` providers would need a separate written review of data-flow, approval wording, per-keystroke cost, and output bounds before implementation. None of that is present in this milestone.

## Try it in the app

Back up Nulis data and quit the running app first (see `docs/handover/PLAN3_RESUME.md`, “Running a test build safely”).

1. Build and start the desktop app.
2. Open **Plugins ▸ Install from folder…** and choose `examples/plugins/nulis-nim`.
3. Approve it. It requests no permissions and starts no process.
4. Open a `.nim` file (or create one). It opens as Nim with highlighting, comments, indentation, completion, and the three snippets. An extensionless file whose first line is `#!/usr/bin/env nim` is also detected.
5. Open a `.json` file. Because the Nim example does not claim `.json`, there is no conflict. To see the conflict UI, temporarily change the example's `extensions` to include `json`, reinstall, then open **Settings ▸ Languages**: “File association conflicts” lists `*.json` with `JSON (builtin)` and `Nim (nulis.nim-language)`. Choose Nim and Apply; `files.associations` now maps `*.json` to `nim`, and a newly opened `.json` file uses Nim.
6. In **Plugins**, clear **highlight**: colors disappear while completion and indentation stay. Clear **languages**: the language and its file associations disappear. Use **Disable** to make the whole plugin inert without revoking it, then **Enable** and **Revoke**.
7. Uninstall the plugin. Open `.nim` files stay readable and fall back to Plain Text.

## Checks run

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test --workspace` | 455 passed, 0 failed |
| `npm run lint` (in `apps/desktop`) | clean |
| `npm run typecheck` | clean |
| `npx vitest run` | 210 passed (23 files) |

## Not done by hand

- The in-app walkthrough above on macOS, Windows, and Linux.
- The M46 and M47 manual checks remain pending from their handovers.
- CI has not run for M48 yet; the branch must be pushed and a pull request opened for the workflow to run.

## Known limits

- Declarative `brackets` are validated but auto-pairing does not consume them yet.
- `firstLine` is tried only when the file name matches nothing. User `files.associations` are file-name rules and do not participate in content detection.
- A plugin language that declares no highlight/indentation/completion/snippet/comments is treated as association-only; it can extend a built-in language but contributes no runtime module of its own.
- Disabling `languages` removes associations and providers for that plugin immediately, but an already-open document keeps its language ID and renders as Plain Text until it is reopened or the language returns.
- There is no marketplace, no plugin update flow, and no executable language provider.
- Plugin grants are per package digest: changing the manifest or source resets enable/capability switches as well as approval.

## Next

1. Run the manual walkthrough on the three platforms and record results.
2. Open the pull request from `feat/language-capabilities` so CI runs.
3. Start any post-plan work from a new plan; do not add executable language providers without the written security review described above.
