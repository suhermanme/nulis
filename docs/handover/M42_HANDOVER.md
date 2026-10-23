# M42 handover — language catalog and capability contracts

**Date:** 2026-10-23. **Branch:** `feat/language-capabilities` (not pushed; the owner will push with the rest of plan 4). **Plan:** `IMPROVEMENT_PLAN_4.md` §5 M42. **Tracker:** `IMPLEMENTATION_TRACKER.md`, M42.

## What changed

- **Catalog** (`crates/nulis-syntax/src/lib.rs`): `AssociationSource`, `FileMatcher`, `Association`, `AssociationConflict`; `register(module, source)`, `unregister`, `set_user_associations`, `associations`, `conflicts`, `contains`. `detect_path` resolves in tiers (user, file name, longest extension); within a tier built-in beats plugin, plugins by ID. Plain Text is a module; modules carry a `Scanner` and comment data.
- **Golden test** `crates/nulis-syntax/tests/detection_golden.rs`: every 0.4.0 extension, file name, content sniff, picker entry, and comment marker. Change it only when a milestone decides to change a mapping, and list that in the release notes.
- **`nulis-language`** (new crate): `CapabilityKind`, `ProviderId`, `ProviderOrigin`, `ProviderInfo`, `Validator`, `Formatter` (+ `FormatMode`, `FormatAssessment`, `FormatOutcome`), `PreviewProvider`, `Provider`, `CapabilityRegistry` (`register`, `unregister_origin`, switches, `set_preference`, `resolve`, `resolve_all`, `capabilities`, `providers`, `suspended`, `call`).
- **Settings** (`nulis-settings`): `SettingsFileStore` for `settings.json` (256 KiB cap, atomic write), validation of `files.associations` and `[language]` sections. Core: `SettingsState`, commands `settings.state` and `settings.saveChecked` (not in the palette).
- **View As** is remembered: `DocumentSummary.languageOverride`, workbench session `languages` (omitted when empty), desktop re-applies on start. Unknown languages show as Plain Text in the viewport.
- IPC protocol **43**.

## Not done here, on purpose

- Capability descriptors in `DocumentSummary` and the Settings ▸ Languages page: M43.
- Indentation and completion traits: M44, M45.

## Checks

All suites pass on macOS (see the tracker for counts). Not tried in the running app. To try: back up and quit the installed Nulis first (`handover/PLAN3_RESUME.md`), then `npm run tauri dev` in `apps/desktop`; open a `.txt`, choose View As ▸ Markdown, quit, start again: it should still be Markdown. Put `{"schemaVersion":1,"values":{"files":{"associations":{"*.h":"cpp"}}}}` in `settings.json` in the data folder and restart: `.h` files open as C++.

## Next: M43

Start with golden tests of every tool message, caret target, and output (`format_tools.rs`, `yaml_tools.rs`, `plist_tools.rs`, `validate_document`, `format_document`), commit them, then register the tools as providers in a `CapabilityRegistry` owned by `DocumentManager` and replace the four `ToolKind` lists.
