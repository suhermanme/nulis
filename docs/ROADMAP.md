# Roadmap

| Milestone | Scope | State |
| --- | --- | --- |
| M00 | workspace, docs, CI, headless core, Tauri/React shell, commands, settings, typed IPC | Complete locally; hosted OS matrix pending first CI run |
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
| M14 | accessibility, recovery, fuzzing, packaging, hardening | In progress locally; hosted platform runs and signing credentials pending |
| M15 | precise pointer editing: double-click word selection and drag-to-move text | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |
| M16 | saved/unsaved line change indicator and map marks | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |
| M17 | workspace tabs: group tabs into a named tab of nested panes | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |
| M18 | coherent vector toolbar icons and separate sectioned Settings window | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |

Milestones advance only when the prior architecture and tests are sound. A scaffold or documented contract is not counted as a working feature.
