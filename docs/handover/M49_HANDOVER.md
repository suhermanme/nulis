# M49 handover — custom themes

M49 lets the user duplicate a shipped theme, edit its colors, and keep the result. Built-in themes are never written. Plan: `docs/THEME_IMPLEMENTATION_PLAN.md`. Spec: `docs/THEME_SPEC.md` ("Custom themes").

## What changed

- **`theme.save` (IPC 47)** — `crates/nulis-core/src/lib.rs` declares the command (`("theme.save", "Save Theme")` in the command list), routes it, and deserializes `{ "theme": Theme }` with `deny_unknown_fields`. The frontend sends the copy with `builtIn` stripped.
- **`ThemeCatalog::save`** — `crates/nulis-theme/src/lib.rs:222`. It validates the theme, refuses a built-in id (`ThemeError::BuiltIn`), then writes through the shared `write_theme(theme, replaces_existing = true)` helper.
- **`write_theme`** — `crates/nulis-theme/src/lib.rs:234`. Shared by `install` and `save`. `install` passes `replaces_existing = false`, so installing a theme whose id already exists returns `ThemeError::Duplicate` instead of overwriting the file; `save` passes `true`, so editing a user theme replaces its own file. A save that starts a new file still enforces `MAX_USER_THEMES` (64). The write is atomic in both paths: `tempfile::NamedTempFile::new_in` + `write_all` + `sync_all` + `persist` (replace) or `persist_noclobber` (new file).
- **Floors are display-only.** The file keeps the author's colors; `write_theme` returns the record after `Theme.ensure_readable`, so `theme.list` and every preview show the floored colors. The same rule as M33: `preview_theme` (`crates/nulis-core/src/lib.rs:4582`) floors user themes and passes built-ins through untouched.
- **Settings window (Appearance)** — `apps/desktop/src/SettingsWindow.tsx`: Duplicate creates a draft copy (`<id>.copy`, `copy-2`, ... ; name `<name> copy`; `version: 1`), each token has a color input and a hex input, **Save Colors** sends `theme.save`, **Revert** drops the draft, **Remove Selected** is disabled for built-ins, refuses the applied theme, and asks for confirmation before `theme.uninstall`.
- **Live preview stays in the panel** — `apps/desktop/src/styles.css` adds `.settings-preview { background-color: var(--editor-background, #fbf7ef); }` and the matching text/overlay rules. The panel's inline style carries the draft's values, so the editor and window chrome never see an unapplied theme.
- **No change to `apps/desktop/src/windowTheme.ts`** — the main window keeps broadcasting raw theme colors over `nulis://appearance-preview`; the server applies the floors, so the frontend stays as it was.

## Frontend wire shape

`ThemeInput` and `ThemeRecord` in `apps/desktop/src/contracts.ts` mirror `nulis_theme::Theme` (`crates/nulis-theme/src/lib.rs:79-89`) exactly: `$schema`, `id`, `name`, `version`, `kind`, `colors`, plus `builtIn` on the record. The earlier `uiTokens`/`plugins` members were removed: core serializes no such keys and rejects unknown ones, so a spread record carrying them fails `theme.save` at runtime.

## Checks run

- `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets` clean.
- `cargo test --workspace` green: nulis-core 200 passed, nulis-theme 16 passed (added: `saves_creates_then_edits_a_user_theme_across_a_restart`), nulis-desktop-lib 9 passed, nulis-automation 15 passed.
- New core test: `custom_theme_edits_persist_through_core_commands_and_protect_built_ins` (`crates/nulis-core/src/lib.rs:5656`) — duplicate, edit, save, reopen, list parity, refusal to overwrite a built-in, refusal to remove the applied theme, uninstall.
- Frontend: vitest, typecheck, lint, `frontend:build` — recorded in `docs/IMPLEMENTATION_TRACKER.md` M49 row.
- Final gate on `799fbda` (2026-10-25): fmt and clippy `-D warnings` clean; `cargo test --workspace` 46 result lines green, 0 failures; desktop Vitest 23 files / 220 tests; typecheck, lint and `frontend:build` green.

## Try it in the app

1. Settings (Ctrl/⌘ ,) → Appearance. Pick a built-in, press **Duplicate**; the status line names the copy and the token fields load its colors.
2. Change a color by picker or hex. The panel's theme preview changes; the editor does not. Invalid hex is refused.
3. **Save Colors** → the copy appears in the theme list marked "· User" and in the Appearance select.
4. **Revert** discards an unsaved draft. **Remove Selected** asks first, refuses built-ins, and refuses the theme the editor is using.
5. Restart Nulis: the copy is still listed with the saved colors.

## Known limits

- The copy's id must satisfy `validate_id` (dots, lowercase, safe path characters); the frontend's `<source>.copy` scheme satisfies it by construction.
- `theme.save` replaces the whole file; two settings windows editing the same copy have last-write-wins semantics, like the rest of settings.
- Deleting the applied theme is refused in the panel only; core keeps accepting `theme.uninstall`, and a customization pointing at a missing theme falls back to the default appearance as before (`crates/nulis-core/src/lib.rs:1386`).
- `crates/nulis-syntax/tests/self_hosting.rs` scans every file under the repository root, so a scratch tree inside the repo (a linked worktree, a fuzz corpus) turns it red. Keep scratch outside the repo; the current one is `../nulis-scratch-vbase`.

## Next

1. Manual acceptance on the owner's display (Settings ▸ Appearance → Duplicate → picker/hex → Save Colors → relaunch → list and restore → Remove Selected): the flow is proven at the state-file level, the live UI run needs a woken display (`M49-window-reopen-offscreen` covers the off-screen window launch).
2. Manual Windows and Linux checks (`docs/PLATFORM_MATRIX.md`), and the color picker on each platform.
