# Roadmap

| Milestone | Scope | State |
| --- | --- | --- |
| M00 | workspace, docs, CI, headless core, Tauri/React shell, commands, settings, typed IPC | Complete; CI runs on macOS, Windows, and Linux |
| M01 | disk-backed source, piece tree, indexing, undo, safe streaming save | Complete locally; hosted Windows/Linux validation pending |
| M02 | virtual viewport, caret, selection, input, scrolling, IME, clipboard | Complete locally; hosted platform and manual assistive-technology validation continue under M14 hardening |
| M03 | tabs, splits, multi-caret, navigation, transforms, encodings | Complete locally; hosted platform validation continues under M14 |
| M04 | streaming find/replace and find in files | Complete locally; hosted large-corpus validation continues under M14 |
| M05 | first-class rectangular editing | Complete locally; hosted platform pointer/input validation continues under M14 |
| M06 | modular languages and Tree-sitter | Complete locally; additional grammars can register through the module contract |
| M07 | scalable hex editing | Complete locally; hosted platform input validation continues under M14 |
| M08 | workspace, explorer, palette, layouts, sessions | Complete locally; hosted platform validation continues under M14 |
| M09 | diff, merge, binary/folder compare | Complete locally; hosted platform and large-corpus validation continue under M14 |
| M10 | remote providers, credentials, terminal | Complete locally; hosted keychain, SFTP interoperability, and PTY validation continue under M14 |
| M11 | macros and sandboxed scripting | Complete locally; hosted platform validation continues under M14 |
| M12 | plugin host, permissions, SDK stabilization | Complete locally; hosted platform validation continues under M14 |
| M13 | customization and complete theme system | Complete locally; hosted platform/native-menu validation continues under M14 |
| M14 | accessibility, recovery, fuzzing, packaging, hardening | In progress; hosted CI builds and installs on all three platforms; screen-reader checks, notarization, and Windows code signing pending |
| M15 | precise pointer editing: double-click word selection and drag-to-move text | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |
| M16 | saved/unsaved line change indicator and map marks | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |
| M17 | workspace tabs: group tabs into a named tab of nested panes | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |
| M18 | coherent vector toolbar icons and separate sectioned Settings window | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |
| M19–M30 | menu-reference feature audit: editing keys, Find and Replace window, Replace in Files, Reload, new icon, format and transforms, search and navigation, view and file features, column and clipboard tools, integration ([IMPROVEMENT_PLAN.md](IMPROVEMENT_PLAN.md)) | Complete; released in Nulis 0.2.0 and 0.2.1 |
| M31–M35 | Find window fit, regular expressions and column search, theme contrast, YAML and plist tools, CSV/TSV and YAML highlighting, unlimited Count All ([IMPROVEMENT_PLAN_2.md](IMPROVEMENT_PLAN_2.md)) | Complete; released in Nulis 0.3.0 |
| M36–M40 | Print, Page Setup, and Export to PDF: page layout in the core, streaming PDF export, native print dialog, print theme, large-file progress ([IMPROVEMENT_PLAN_3.md](IMPROVEMENT_PLAN_3.md)) | Planned; approved 2026-10-21 |

Milestones advance only when the prior architecture and tests are sound. A scaffold or documented contract is not counted as a working feature.
