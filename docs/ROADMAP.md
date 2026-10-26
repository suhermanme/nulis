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
| M10 | remote providers, credentials, terminal | Complete locally; remote providers and credentials removed on 2026-10-22 (the terminal stays) |
| M11 | macros and sandboxed scripting | Complete locally; hosted platform validation continues under M14 |
| M12 | plugin host, permissions, SDK stabilization | Complete locally; hosted platform validation continues under M14 |
| M13 | customization and complete theme system | Complete locally; hosted platform/native-menu validation continues under M14 |
| M14 | accessibility, recovery, fuzzing, packaging, hardening | In progress; hosted CI builds and installs on all three platforms; screen-reader checks, notarization, and Windows code signing pending |
| M15 | precise pointer editing: double-click word selection and drag-to-move text | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |
| M16 | saved/unsaved line change indicator and map marks | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |
| M17 | workspace tabs: group tabs into a named tab of nested panes | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |
| M18 | coherent vector toolbar icons and separate sectioned Settings window | Implemented and tested locally on macOS; hosted OS matrix and manual pointer/IME/accessibility checks pending |
| M19–M30 | menu-reference feature audit: editing keys, Find and Replace window, Replace in Files, Reload, new icon, format and transforms, search and navigation, view and file features, column and clipboard tools, integration ([IMPROVEMENT_PLAN.md](archive/plans/IMPROVEMENT_PLAN.md)) | Complete; released in Nulis 0.2.0 and 0.2.1 |
| M31–M35 | Find window fit, regular expressions and column search, theme contrast, YAML and plist tools, CSV/TSV and YAML highlighting, unlimited Count All ([IMPROVEMENT_PLAN_2.md](archive/plans/IMPROVEMENT_PLAN_2.md)) | Complete; released in Nulis 0.3.0 |
| M36–M40 | Print, Page Setup, and Export to PDF: page layout in the core, streaming PDF export, native print dialog, print theme, large-file progress ([IMPROVEMENT_PLAN_3.md](archive/plans/IMPROVEMENT_PLAN_3.md)) | Complete; released in Nulis 0.4.0 |
| M41–M48 | Language capabilities: language catalog and capability registry, first-party providers, auto-indentation, shared autocomplete, broader language coverage, optional LSP, declarative language plugins ([IMPROVEMENT_PLAN_4.md](archive/plans/IMPROVEMENT_PLAN_4.md)) | Complete; released in Nulis 0.5.0 |
| M49 | Custom themes: catalog write path in `nulis-theme`, `theme.save`, the Appearance panel's **Duplicate Selected** / color editing / rename / **Remove Selected**, live preview, and contrast-floor reporting in the panel ([THEME_IMPLEMENTATION_PLAN.md](archive/plans/THEME_IMPLEMENTATION_PLAN.md)) | Complete; released in Nulis 0.5.1 (2026-10-26, IPC protocol 48). The owner's macOS manual acceptance passed for the readability notice (2026-10-25); the rest of the manual list and the Windows and Linux checks stay open ([archive/handover/M49_HANDOVER.md](archive/handover/M49_HANDOVER.md)) |
| — | Unplanned window and terminal work: a word under every toolbar control with a measured **More** overflow menu, the icon rail and symbols picker removed, the status bar's mode chip as an indicator, and the integrated terminal's divider, monospace grid, cursor, and hide-keeps-shell ([WORK_STATE_HISTORY.md](archive/WORK_STATE_HISTORY.md)) | Released in Nulis 0.5.2 (2026-10-26, IPC protocol 48, unchanged). The macOS walkthrough passed for the toolbar, and the owner accepted the mode chip; the packaged terminal's native check and the Windows and Linux checks stay open |

Milestones advance only when the prior architecture and tests are sound. A scaffold or documented contract is not counted as a working feature.
