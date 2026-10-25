# Theme customization — implementation plan

**Scope:** user-owned custom themes built from bundled themes.

**Status (2026-10-25): shipped as M49 and merged to `main` at `bcf62f4`; the
`feat/theme-customization` branch is deleted.** M1–M4 completed as planned, then two
manual-acceptance follow-ups amended this plan — the panel's readability reporting, and the
removal of the "Apply theme colors" switch. The switch removal is one of this plan's own stop
conditions (`IPC_PROTOCOL_VERSION` touched), so it is recorded as an amendment below under
*Follow-ups*, not as a silent exception. Handover: `docs/handover/M49_HANDOVER.md`.

**Verified contract (do not widen):**

- `nulis_theme::Theme` keeps exactly `$schema`, `id`, `name`, `version`, `kind`, `colors` and `#[serde(deny_unknown_fields)]`. No new serialized fields anywhere.
- `ThemeRecord` stays `{ ...theme, builtIn }`.
- Contrast floors stay server-side: `Theme::ensure_readable` in `crates/nulis-theme/src/contrast.rs`, applied on every read/list path (`crates/nulis-theme/src/lib.rs:177`, `:316`, `:491`) and on the value `theme.save` returns. The authored file keeps the author's colors; what Nulis shows meets the floors.
- `IPC_PROTOCOL_VERSION` stays 47 and `nulis-core`'s public method surface is unchanged — true through
  M1–M4, and `theme.readable` was added additively inside 47. The switch-removal follow-up is the one
  planned exception: dropping `applyUiTheme` from the appearance DTO is a DTO change, so the version went
  to 48 in both `crates/nulis-core/src/lib.rs:90` and `apps/desktop/src/contracts.ts:2`.
- Generated theme data is fully tracked (`themes/*.json`, `themes/nvchad-source/*.lua`); no generation step, no `build.rs` in the theme crates, so a clean checkout builds with no manual regeneration.

---

## M1 — Theme catalog writes user themes (`crates/nulis-theme`) — **complete**

**Files:** `crates/nulis-theme/src/lib.rs` (+ tests in the same file).

`pub fn save(&self, theme: &Theme) -> Result<ThemeRecord, ThemeError>`:

1. `theme.validate()` — invalid id, metadata, missing token, non-hex color, unknown token → typed error, nothing written.
2. `ThemeError::BuiltIn(id)` for any id that belongs to a bundled theme.
3. `MAX_USER_THEMES` (64) enforced only for a new file; editing an owned theme succeeds at the limit.
4. Atomic write: `tempfile::NamedTempFile::new_in` → `write_all` → `sync_all` → `persist`; `create_dir_all` for the directory; symlinks and non-regular files rejected.
5. Returns `ThemeRecord { theme: floored clone, built_in: false }` — identical to what `list` reads back after a restart.

`install`, `uninstall`, `list` semantics unchanged.

**Acceptance:** `cargo test -p nulis-theme` green, covering create→edit→restart parity, built-in refusal with an untouched catalog, invalid data leaving no file, the limit, and authored-vs-readable colors.

**Tests run:** `cargo test -p nulis-theme`, `cargo clippy -p nulis-theme --all-targets`, `cargo fmt -- --check crates/nulis-theme`.

## M2 — `theme.save` through core and the settings window — **complete**

**Files:** `crates/nulis-core/src/lib.rs` (command + `ThemeInput`/`theme_save`), its `#[cfg(test)] mod tests` (starting 4724), `apps/desktop/src-tauri/src/lib.rs` (settings-window command allowlist).

- Command id `theme.save`; input mirrors `Theme` (no `builtIn`), `deny_unknown_fields` on the request.
- Response is the record as `theme.list` serializes it, so `theme.save` output equals the `theme.list` record for the same theme.
- Errors surface as `ThemeError` text (built-ins: "built-in theme cannot be changed").
- `theme.uninstall` stays as-is: built-in refusal, and refusal while selected.

**Acceptance:** `cargo test -p nulis-core` green, including `custom_theme_edits_persist_through_core_commands_and_protect_built_ins` (`crates/nulis-core/src/lib.rs:5656`); `cargo build -p nulis-desktop` compiles with `theme.save` authorized for the settings window only.

**Tests run:** `cargo test -p nulis-core`, `cargo build -p nulis-desktop`, `cargo clippy --workspace --all-targets`.

## M3 — Settings ▸ Appearance: duplicate, edit, preview, remove — **complete, then amended**

**Files:** `apps/desktop/src/SettingsWindow.tsx`, `contracts.ts` (`ThemeInput`), `styles.css` (preview rules), tests in `apps/desktop/src/App.test.tsx`.

- **Duplicate** → `<source>.copy` (`copy-2`, `copy-3`, … for repeats; `.copy.copy` for copies), `version: 1`, name `<name> copy`; the copy is a draft until **Save Colors** stores it.
- **Color grid** over `THEME_TOKENS` with a filter field; native `input[type=color]`, `input[type=text] pattern="#[0-9a-fA-F]{6}"` with `THEME_HEX`, and arrow-key nudges on the color field; unsaved edits live in a draft, **Revert** discards them, **Save Colors** sends the whole record without `builtIn`.
- **Live preview** paints the panel's preview element (`.settings-preview`) from inline CSS variables built from the draft, unsaved edits included; invalid hex never changes the preview, and nothing reaches the editor.
- **Remove Selected** asks for confirmation, is disabled for built-ins, and keeps the active-theme refusal.

**Acceptance:** duplicate → edit → save → restart shows the custom colors; built-ins unedited; invalid hex blocked client-side and typed server-side; removing a built-in and removing the active theme both refused; the main-window theme behavior in `App.test.tsx:974-1009` still passes.

**Tests run:** `npm --prefix apps/desktop run test`, `npm --prefix apps/desktop run typecheck`, `npm --prefix apps/desktop run lint`.

**Amended by the follow-ups below:** the store button labels itself for what it writes (**Save Copy**,
**Save Colors**, **Save Name**, **Save Name & Colors**), each row the contrast floors touch carries its own
readability notice, the preview box paints the floors-applied palette, and the **Apply theme colors**
switch is gone.

## M4 — Docs, tracker, regression sweep — **docs complete, full sweep deferred**

**Files:** `docs/THEME_SPEC.md` (Custom themes section), `docs/IMPLEMENTATION_TRACKER.md`, `docs/handover/M49_HANDOVER.md`, this plan.

Spec text must match shipped behavior exactly: command name, id pattern, limits, floor behavior, the preview scope, and the deletion rules.

**Acceptance:** `cargo fmt -- --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`, `npm --prefix apps/desktop run test`, `npm --prefix apps/desktop run typecheck`, `npm --prefix apps/desktop run lint`, `npm --prefix apps/desktop run frontend:build` all green; handover names the commands added, the files touched, and every test file that changed.

**As shipped:** the gate ran green on the M49 commit (2026-10-25), and the docs list above grew to
`README.md`, `USER_GUIDE.md` (the Help window renders it), `ARCHITECTURE.md`, `SECURITY.md`,
`DECISIONS.md`, `ROADMAP.md`, `FEATURE_PARITY.md`, `PLATFORM_MATRIX.md`, `WORK_STATE.md` and
`RELEASE.md`. The owner chose "commit now, checks later" for the two follow-ups, so the full sweep over
them is recorded as **pending** in `docs/IMPLEMENTATION_TRACKER.md`; only the tests each follow-up
touched were run.

---

---

## Follow-ups (2026-10-25, from manual acceptance)

**The panel reports the floors.** Manual acceptance found a save that read as a failure: a picked
`editor.selection` stored fine and painted something else, because the contrast floors strengthen four
tokens (`editor.selection`, `ui.mutedText`, `ui.icon`, `ui.iconDisabled`) on the read path, and
`editor.gutter` — which painted as typed — is not one of them. No rule changed; floors stay server-side
and the authored file keeps the author's hex, exactly as this plan required. `Theme::ensure_readable`
returns the tokens it strengthened, the read-only `theme.readable` answers a draft with
`{ colors, changes: [{ token, from, to }] }`, the panel marks each affected row with what Nulis will
paint, and the preview paints the floors so preview and editor agree. `theme.readable` is in the Settings
window allowlist and deliberately **not** in `CUSTOMIZATION_MUTATIONS` — it stores nothing, so it never
triggers the other window's reload. Being additive, it shipped inside IPC 47. Help text:
`docs/USER_GUIDE.md` §"Theme colors and readability". ADR-0024.

**The "Apply theme colors" switch is removed.** The owner read it as a third apply control — "what this
checkbox do? we have apply button already" — and asked for it out as confusing UI. It had gated *which
surfaces* follow the applied theme, not whether a theme applies: the theme select plus **Apply** always
repaints the editor, and the switch decided whether the theme's colors export onto the window root. The
panel's default was off, core's on, so it usually did nothing visible. Applying a theme now paints the
editor and every Nulis window, always: `exportWindowTheme` runs ungated in the per-window listener, the
workbench marks a user theme `data-custom="true"`, and built-ins export their own palette so a built-in's
chrome never shifts. `applyUiTheme` leaves the frontend appearance DTO (⇒ **IPC 47 → 48**) and stays in
Rust as `#[serde(default, skip_serializing_if = "std::ops::Not::not")]`: `deny_unknown_fields` would
refuse the documents switch-era builds wrote, and skipping the off value keeps the documents this build
writes loadable by a build that predates the switch, because builds share one `com.nulis.editor` store.
`apps/desktop/src/windowTheme.ts` left M3's file list here — the plan's own scope-creep rule, so this
file is where the change is recorded. ADR-0025.

## Out of scope

Theme files in the file explorer, theme import from the editor, per-window theme overrides, theme export,
syntax-token regeneration, `apps/desktop/src-tauri/gen/schemas/*` (gitignored build artifacts; regenerate
only if `tauri build` complains). `IPC_PROTOCOL_VERSION` changes were out of scope for M1–M4 and stayed
unused through them; the switch-removal follow-up above is the amendment that took 47 → 48.

## Risks and stop conditions

- **Wire drift.** Any new serialized field, extra payload key, or protocol bump stops M1/M2; the server rejects unknown keys via `deny_unknown_fields`, so the UI sends only `Theme`'s six keys.
- **Floor placement.** Applying floors in the UI (or persisting floored colors as authored) contradicts `THEME_TOKENS`/`contrast.rs` and stops M1.
- **Built-in mutation.** Any write path that can name a bundled theme id stops M1/M2.
- **Preview leakage.** Preview styles that escape the panel's roots, or that paint the editor from unsaved colors, stop M3.
- **Scope creep.** If a milestone needs a file outside its list, the plan is amended first (this file), not the code.
- **Stop conditions:** a red `cargo test --workspace` at M4, a Tauri capability/permission change, or any need to touch `IPC_PROTOCOL_VERSION`.
  None of these stopped M1–M4. The one that fired later was the protocol bump, and it fired on the
  follow-up, not on a milestone: the owner asked for the switch out, dropping it from the DTO is a DTO
  change, and the rule is the rule — both versions moved together and the tests compare them. No Tauri
  capability changed; the workspace test run over the follow-ups is pending, by the owner's choice.
