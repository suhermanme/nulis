# M54 handover — split the largest UI files

Fifth milestone of `docs/IMPROVEMENT_PLAN_6.md`. Branch `refactor/split-ui`, from `main` at `e2b21d3` (after M53). No behavior change intended; the owner's smoke run of the built app is the remaining check.

## Result

| File | Before | After |
| --- | --- | --- |
| `apps/desktop/src/App.tsx` | 3,025 | 2,589 |
| `apps/desktop/src/EditorViewport.tsx` | 2,978 | 2,790 |

## What moved (one commit each)

| New file | Lines | From | What |
| --- | --- | --- | --- |
| `statusLabels.ts` | 28 | App | status bar wording: encoding, sizes and counts, code points |
| `workbenchDefaults.ts` | 65 | App | the empty customization and session, `dropDocument`, path helpers |
| `RenameDialog.tsx` | 34 | App | the rename-file dialog |
| `TerminalDock.tsx` | 140 | App | `useTerminalDock` (open, alive, height, drag, focus, and the three effects that persist the height, follow a drag, and fit the column) and `TerminalDock` (the divider and the panel) |
| `useDocumentProblems.ts` | 84 | App | language-server problems: their state, the ask-after-change and forget-closed effects, the active document's counts |
| `errors.ts` | 4 | App | `errorMessage`, shared by App and the hooks |
| `usePdfExport.tsx` | 193 | App | Export to PDF and Print: job and large-export state, the six handlers (copied verbatim), and `LargeExportDialog` |
| `viewportText.tsx` | 198 | EditorViewport | eleven pure helpers: byte, UTF-16 and column conversions, pointer positions, drawing a line with its highlights and whitespace |

## Effect order

React runs effects in declaration order. The changes:
- `useDocumentProblems` is called exactly where App derived the active problems, so its two effects sit where they were.
- `useTerminalDock` is called where the terminal state was declared, which moves its three effects ahead of some of App's. They write only terminal state (and local storage for the height), so nothing else can observe the new order.
- `usePdfExport` is called after `activeTheme`, because its handlers read it. Its handlers were already closures over that later constant, and none runs during render.

## How each step was checked

- **The full test list.** Every desktop test was recorded as file, name and result before M54 (262 tests, all passing), and the list was compared after each step: identical every time. That comparison caught the one slip, a missing import of `utf16IndexForScalarColumn`. Vitest does not type-check, so the slip showed up as four failing column-selection tests, which went back to passing once the import was added.
- **Tokens.** Each step was compared token by token with the previous commit, ignoring imports, `export` and whitespace. The pure moves add only header comments. The hook steps lose no original token; they add only the hooks' signatures, returns and calls, plus the dialog's early `return null` in place of its ternary.
- **The usual checks.** `tsc -b` and ESLint (`--max-warnings 0`) are clean after every step.

## Checks run (macOS, 2026-10-26)

`npm run lint`, `npm run typecheck`, `npm run test` (desktop 262/262, SDK 2/2), `npm run frontend:build` and `cargo test -p nulis-desktop` (10/10) all pass.

## Stopped on purpose

- **EditorViewport's remaining blocks.** Key handling, the edit queue, navigation, find and replace, and the document map all share the viewport snapshot, the edit queue and the selection state. Moving them into hooks would need the hooks to pass that shared state back and forth, which is a redesign, not a move. The document map was looked at closely: its drawing reads the scroll position, row height and snapshot, and its click handler drives the viewport's navigation. Pulling it out would buy about 100 lines and is not worth the risk in a move-only milestone.
- **Test files.** `App.test.tsx` and `EditorViewport.test.tsx` were not split; every test keeps its exact name and file.
- **Duplicated `message()` helpers.** `MacroPanel`, `PluginPanel` and `SettingsWindow` each define the same function as `errors.ts`. They were left alone, as outside these two files.

## Owner's smoke run

On the Desktop build of this branch: type and undo in a document, split a pane, open the terminal and drag its divider, hide and show it (the shell survives), preview a Markdown file, Export to PDF, Print, and rename a file.

## Next

M55: archive the finished plans and handovers, trim the README, add `CONTRIBUTING.md` (with DCO sign-off) and issue templates, and fix the three `cargo doc` link warnings M53 noted.
