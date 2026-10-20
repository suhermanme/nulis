# M30 handover — Integration and quality

**Status:** completed 2026-10-20 on macOS (automated checks), on `main`. The plan M19–M30 is complete.

## Implementation

- **Commands everywhere** — `apps/desktop/src/commands.test.ts` (new) reads the core's and the shell's Rust sources and checks: the desktop's `WORKBENCH_ACTIONS` equals the core's table; every presentation command is routed in `App.tsx` (or by `EDITOR_ACTIONS` / `FIND_WINDOW_COMMANDS`); every native menu item names a core command or a shell-only one (`app.quit`, `window.close`); the menus carry a sample of the M20–M29 commands. The audit found nothing missing; palette-only commands are older pane and split commands, and Clear Recent Files lives in the Open Recent picker.
- **Shortcuts** — `App.tsx`: the window's key handler returns when `event.defaultPrevented`, so a key a view handled (the editor's own undo and redo) is not run again through the customizable shortcuts. That was a real double undo and double redo on Primary Z and Primary Shift Z. `EditorViewport.tsx`: Ctrl Y is Redo on Windows and Linux. No clashing chords were found between the defaults, the window keys (Alt Z, Primary = - 0, Primary P, Primary ,, F1), and the editor keys.
- **Accessibility** — `Picker.tsx`: `useRestoreFocus` (reads the focused element during the first render, before autofocus) returns focus when Open Recent, Quick Open, or Paste from History closes; the Rename dialog is now a `RenameDialog` component using it; `keepFocusInDialog` cycles through inputs, selects, and text areas as well as buttons.
- **Docs** — user guide: menu list (Search, Format; no menu bar on secondary windows on Windows and Linux), side bar (search moved to its window), status bar summary, About background, Ctrl Y. `FEATURE_PARITY.md`: CI, shell, status bar, line commands, central commands, Settings window, About, accessibility. `PLATFORM_MATRIX.md` (new), linked from `RELEASE.md`.

## Tests

- `commands.test.ts` (3); App: a key the editor handled is not run again, while the shortcut still works outside the editor; focus returns after Quick Open and Tab stays inside the Rename dialog; viewport: Ctrl Y and Ctrl Shift Z redo.

## Known issues

- Native menus do not show the shortcuts the page handles (Alt Z, zoom, Primary P, F3); moving them into native accelerators risks running a command twice and needs its own design.
- Screen-reader checks (VoiceOver, NVDA, Orca) and Linux by hand are not done; see `PLATFORM_MATRIX.md`.
