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
- Final gate on the M49 commit (2026-10-25): fmt and clippy `-D warnings` clean; `cargo test --workspace` 46 result lines green, 0 failures; desktop Vitest 23 files / 220 tests; typecheck, lint and `frontend:build` green.

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

## Manual acceptance follow-up (2026-10-25) — readability feedback

**What the owner saw.** Changing `editor.selection` saved and did not paint; changing `editor.gutter` painted as typed. Both saves worked. Four tokens — `editor.selection`, `ui.mutedText`, `ui.icon`, `ui.iconDisabled` — are measured against the contrast floors when a theme is loaded, and `editor.gutter` is not one of them. The picked `#ff9300` puts ember's `editor.foreground #c9d1d9` at 1.44:1, far under the 4.5:1 text-on-selection floor, so `readable_selection` took the first background→accent blend that fits: `#43586B`. The file kept `#ff9300` exactly.

**The fix is reporting, not a new rule.** A color that is quietly strengthened reads as a save that failed, so the panel now says what Nulis will paint:

- `Theme::ensure_readable` returns the tokens it changed as `(token, new color)` (`crates/nulis-theme/src/contrast.rs:165`).
- **`theme.readable`** (additive in IPC 47, like M49's other commands) — `crates/nulis-core/src/lib.rs`: `{ theme }` → `{ colors, changes: [{ token, from, to }] }`. It stores nothing and reads nothing, so it is in the Settings window allowlist and deliberately **not** in `CUSTOMIZATION_MUTATIONS`: it never triggers the other window's reload.
- `apps/desktop/src/SettingsWindow.tsx` asks it of the unsaved draft (debounced 150 ms) and marks each affected row: **"Nulis paints #43586B — Editor Selection stays readable. Your color is saved as typed."** A pick that already reads well gets no marker.
- The preview box now paints the floors-applied palette, so preview and editor agree: `previewCssVariables` (`apps/desktop/src/preferences.ts:278`) gained `--editor-selection`, and the preview's sample code shows a selected span (`.settings-preview-selection`, `apps/desktop/src/styles.css`).
- Help: `docs/USER_GUIDE.md` §Settings and Themes → **"Theme colors and readability"** (the Help window renders this file). `docs/THEME_SPEC.md` §Contrast floors/§Custom themes and `docs/ARCHITECTURE.md` (the Settings allowlist) record the reporting path.
- The owner's workaround, documented in the guide: give the text on the selection room — darken `editor.foreground`, or pick a darker highlight — so the two clear 4.5:1, and the highlight paints as typed.

**Checks run for the follow-up:** `cargo test -p nulis-theme` (`ensure_readable_reports_the_colors_it_strengthens`), `cargo test -p nulis-core` (`readable_theme_colors_report_the_floors_and_store_nothing`), `npx vitest run src/App.test.tsx -t "theme"` (10 passed, incl. the new panel test "a color the contrast floors strengthen says so in its row, and the preview paints what the editor will"), `commands.test.ts` / `contracts.test.ts` / `preferences.test.ts` green, `npx tsc -b` clean.

## Manual acceptance follow-up (2026-10-25) — the "Apply theme colors" switch is removed

**What the owner asked.** "What this checkbox do? we have apply button already", then: "if it is duplicated or no use, better to remove it, it is confusing UI". It was not a duplicate of **Apply** — it gated *which surfaces* follow the applied theme: the theme select plus **Apply** always repaints the editor, and the switch decided whether the theme's colors also export onto the window root (every Nulis window's chrome, and the theme across restarts). The panel's default was off, the core's default on, so the control read as a third apply control and usually did nothing visible.

**Behavior now.** Applying a theme paints the editor and every Nulis window, always. `exportWindowTheme` runs unconditionally in the per-window listener (`apps/desktop/src/windowTheme.ts`), the workbench marks a user theme with `data-custom="true"`, and built-ins export their own palette so a built-in's chrome never shifts. The panel shows one line where the switch was: *"The applied theme paints the editor and every Nulis window. The preview shows it while you edit; **Apply** saves it."* (`apps/desktop/src/SettingsWindow.tsx`, `.settings-apply` rules dropped from `styles.css`.)

- **DTO change ⇒ IPC 47 → 48**, bumped together in `crates/nulis-core/src/lib.rs:90` and `apps/desktop/src/contracts.ts:2` (`contracts.test.ts` compares them). `applyUiTheme` is out of `EditorPreferences`, `DEFAULT_EDITOR_PREFERENCES`, `normalizeEditorPreferences`, `WindowThemePayload` and `preferenceWindowTheme` (`apps/desktop/src/preferences.ts`), and out of the App gate that skipped the export (`apps/desktop/src/App.tsx`).
- **Rust keeps the field, and writes it only when it is on.** `AppearanceSettings.apply_ui_theme` is `#[serde(default, skip_serializing_if = "std::ops::Not::not")]`: `deny_unknown_fields` would refuse documents the switch-era builds wrote, and skipping the off value keeps documents this build writes loadable by a build that predates the switch — the session root and settings file are shared between builds (`docs/handover/PLAN3_RESUME.md`, "Running a test build safely"). Core's document default is off.
- **Tests rewritten for the new behavior:** `preferences.test.ts` "stored preferences written before the theme-colors switch normalize without it"; `App.test.tsx` "applying a user theme marks the window, repaints every window, and survives a restart" and "a built-in theme repaints its own palette and never marks the window" (no checkbox interaction, no `applyUiTheme` in the assertions); `nulis-settings` `appearance_loads_documents_written_while_the_switch_existed`; `nulis-core` `applying_a_custom_theme_accepts_the_appearance_the_editor_posts` (the appearance the editor posts has no flag, and an older document's flag survives the save and the restart).
- `docs/THEME_SPEC.md` §Custom themes describes the new behavior and the compatibility field.

**Checks run for the removal:** `npx tsc -b` clean; `npx vitest run src/preferences.test.ts` 5 passed; `npx vitest run src/App.test.tsx` 88 passed; `cargo test -p nulis-settings` 8 passed; `cargo test -p nulis-core` 203 passed. The full boundary run below stays pending, as the owner chose.

## Next

1. Manual acceptance on the owner's display (Settings ▸ Appearance → Duplicate → picker/hex → Save Colors → relaunch → list and restore → Remove Selected): the flow is proven at the state-file level, the live UI run needs a woken display (`M49-window-reopen-offscreen` covers the off-screen window launch).
2. Manual Windows and Linux checks (`docs/PLATFORM_MATRIX.md`), and the color picker on each platform.
3. Manual check of the readability feedback — **the owner reports it working on their display
   (2026-10-25)**, from the `~/Desktop/Nulis.app` build of 22:56: the panel shows the notice while
   picking. The rest of M49's manual list (relaunch and restore, Remove Selected, Windows and
   Linux) is still open.
4. Full boundary run over the follow-up — `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, desktop Vitest, lint, typecheck, `frontend:build` — recorded in `docs/IMPLEMENTATION_TRACKER.md`.
5. Manual check of the removed switch on the owner's display: Appearance shows no **Apply theme colors** checkbox, and applying a user theme repaints the window chrome (title bar area, side panels, status line) in every Nulis window, with the built-in themes painting their own. From the `~/Desktop/Nulis.app` build of the removal commit; the Desktop copy and any installed Nulis share the same `com.nulis.editor` store, so run one at a time.
