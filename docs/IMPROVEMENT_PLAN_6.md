# Improvement plan 6: repository health (M50–M55)

**Status:** approved 2026-10-26 with every §5 question answered (answers recorded in §5).
**Date:** 2026-10-26, against commit `9a0d0db` (Nulis 0.5.2), branch `main`.
**Milestone numbering:** continues `IMPROVEMENT_PLAN_4.md` (M41–M48) and `THEME_IMPLEMENTATION_PLAN.md` (M49, "plan 5" in the tracker).
**Progress:** M50–M55 complete (§4); M54 stopped short of the remaining EditorViewport blocks on purpose (see its handover). M51 found the true Rust minimum is 1.90 (ADR-0026). Progress lives in `IMPLEMENTATION_TRACKER.md` ("Plan 6"); handovers in `docs/handover/`.

## 1. Request

Plan the ten improvements found by the 2026-10-26 repository review so they can be built one milestone at a time. Each milestone stops at its boundary with a handover in `docs/handover/` and an entry in `IMPLEMENTATION_TRACKER.md`.

## 2. Baseline at `9a0d0db`

- `cargo clippy --workspace --all-targets --locked -D warnings`, `npm run lint`, `npm run typecheck` and `npm audit --omit=dev` (0 vulnerabilities) all pass on macOS.
- The security boundary is sound and stays as it is. Per-window command allowlists are enforced in `authorize_window_command` (`apps/desktop/src-tauri/src/lib.rs:425`), document bytes reach only the main window (`require_editor_window`), and local preview images are confined to the document's folder (`crates/nulis-core/src/editor.rs:2328`).
- IPC protocol 48 (`crates/nulis-core/src/lib.rs:90`), 468 Rust tests.

## 3. The ten findings and where they land

| # | Finding | Milestone |
|---|---------|-----------|
| 1 | Markdown preview loads `http(s):` images, so opening a file can contact any server; the CSP allows `img-src https: http:` | M52 |
| 2 | `Cargo.toml` `repository = "https://example.invalid/nulis"` | M50 |
| 3 | CI frontend job uses Node 20 (end of life April 2026); bundle and release jobs use 22 | M51 |
| 4 | `rust-version = "1.82"` is declared but never built | M51 |
| 5 | No Dependabot; many dependencies are pinned with `=` | M51 |
| 6 | `tmp/` (30 scratch probes and screenshots) is neither committed nor ignored | M50 |
| 7 | Very large files: `editor.rs` 8.5k lines, `nulis-core/src/lib.rs` 6.3k, `App.tsx` 3k, `EditorViewport.tsx` 3k | M53, M54 |
| 8 | `docs/` holds four finished improvement plans and 27 handovers at top level; the README is 34 KB | M55 |
| 9 | No `CONTRIBUTING.md` or issue templates | M55 |
| 10 | `target/` uses 13 GB locally | M50 (documentation only) |

## 4. Milestones

Every milestone ends with all suites green: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -D warnings`, `cargo test --workspace --locked`, `npm run lint`, `npm run typecheck` and `npm run test`, run on macOS, plus CI passing on all three platforms. Each milestone is one branch and one reviewable merge.

### M50 — Repository hygiene (findings 2, 6, 10)

Small and risk-free; it goes first to clear the noise.

1. Set `repository = "https://github.com/suhermanme/nulis"` in `[workspace.package]`. Check that every crate inherits it (`repository.workspace = true`) and add the line where one does not.
2. Delete `tmp/` (owner's answer, §5 Q1) and add `tmp/` to `.gitignore` so future scratch files stay out of `git status`.
3. In the README's "Building from source", add one line saying `target/` grows to over 10 GB and `cargo clean` reclaims it. Nothing else changes for this finding.

**Acceptance:** `cargo metadata --format-version 1 --no-deps` shows the GitHub URL for every package; `git status` is clean with `tmp/` present; no code changes.

### M51 — Toolchain and dependency maintenance (findings 3, 4, 5)

1. **Node for the build** (§5 Q2: Node 24). Set `node-version: 24` in every `setup-node` step of `ci.yml` and `release.yml`, raise `engines.node` to `>=24.0.0` (and `engines.npm` to the npm that ships with it), and add `.nvmrc` containing `24` so local and CI match. Check that `npm ci`, the Vite build and Vitest pass on 24.
2. **Node for the actions.** The recurring CI warning about Node 20 comes from the actions' own runtime, not from the build: `actions/checkout@v4`, `actions/setup-node@v4`, `actions/upload-artifact@v4` and `actions/download-artifact@v4` run on Node 20, which GitHub is retiring. Move each to its current major that runs on Node 24, and check `Swatinem/rust-cache` and `dtolnay/rust-toolchain` the same way. **Acceptance for this step:** a CI and a release dry run show no Node 20 deprecation annotation.
3. **MSRV** (§5 Q3: keep it). Add a CI job `Rust MSRV` on `ubuntu-latest` that runs `cargo check --workspace --all-targets --locked` with the toolchain named by `rust-version`. If 1.82 fails, raise `rust-version` to the lowest version that builds, found by trying toolchains locally, and record the version and the dependency that forces it in `DECISIONS.md`.
4. **Dependabot.** Add `.github/dependabot.yml` with weekly updates for `cargo` (root), `npm` (root, covering the workspaces) and `github-actions`, grouped by ecosystem and limited to 5 open PRs each. Exact pins (`=x.y.z`) stay pinned and Dependabot proposes bumps as PRs, which keeps the pinning policy while showing security fixes.
5. Update `docs/RELEASE.md` and the README's build prerequisites where they name the Node or Rust versions.

**Acceptance:** CI is green with the new MSRV job; every workflow builds on Node 24 and no run shows the Node 20 deprecation warning; Dependabot's first scan opens update PRs or reports nothing to update, and neither breaks CI.

### M52 — Markdown preview privacy (finding 1)

`SECURITY.md:5` treats preview markup as untrusted and promises workspace privacy. A remote image in a `.md` file breaks that promise: it reveals the reader's IP address, when they opened the file, and with `http:` it does so in clear text.

1. **Core.** Add a setting `markdown.preview.remoteImages` with the values `block` | `ask` | `allow`. The default is `ask` (§5 Q4), so no remote request is made until the reader asks for one. Store it in the persisted settings document (`nulis-settings`) alongside the existing editor keys and describe it in `docs/settings.schema.json`. Carry it to the UI in the existing settings snapshot; bump IPC to 49 if a DTO changes.
2. **UI** (`MarkdownPreview.tsx:14`). For `http(s):` sources:
   - `block`: render the alt text in a styled placeholder showing the host name (`Remote image from example.com, blocked`) and never create an `<img>`;
   - `ask`: the same placeholder plus a **Load remote images** button that applies to that document until it is closed;
   - `allow`: today's behavior.
   `data:` URIs and local relative images are unchanged. Also apply the setting to the print preview (`PrintApp.tsx`) and the Page Setup preview (`PageSetupDialog.tsx`), which reuse the component.
3. **CSP** (`tauri.conf.json`). Remove `http:` from `img-src`. `https:` stays, because `ask` (after the button) and `allow` load secure sources.
4. **Settings ▸ Appearance or Languages ▸ Markdown:** a three-way control for the setting, with the Help topic updated.
5. **Docs:** `SECURITY.md` (preview threat model), `USER_GUIDE.md` (the placeholder and the setting), and release notes.

**Tests.** Vitest: each of the three modes, a mixed document (one remote, one local, one `data:` image) where only the remote one changes, the button in `ask` mode, and `http:` blocked under every mode once the CSP no longer allows it. Rust: the setting's default, round-trip and unknown-value rejection (`deny_unknown_fields`). A CSP test or CI grep asserting `img-src` contains no `http:`.

**Acceptance:** with the default setting (`ask`), and before **Load remote images** is pressed, opening a Markdown file whose images point at a local listener (for example `python3 -m http.server`) produces no request in that listener's log; local images still render; print output matches the on-screen preview.

### M53 — Split the core's largest files (finding 7, Rust)

A pure move with no behavior change. Do it after M52 so the privacy change does not have to land on moving files.

1. `crates/nulis-core/src/editor.rs` (8.5k lines): move cohesive sections into `editor/` submodules on the pattern of the existing `editor/line_ops/` and `editor/indentation.rs`. Likely seams, to be confirmed by reading: sessions and open/save, viewport and snapshots, Markdown preview, validate/format dispatch, find/replace glue, and recovery. Keep the `Editor` public API and re-export paths unchanged.
2. `crates/nulis-core/src/lib.rs` (6.3k lines): separate the command registry and descriptors, customization (toolbar, menus, keybindings), and the `NulisCore` façade.
3. Move one seam per commit, so each commit compiles, passes tests and reviews as a diff that mostly moves code.
4. Move each test module next to the code it tests.

**Acceptance:** no public item renamed (`cargo doc` and the desktop crate build unchanged); the test count stays at 468 or above with no test edited except for `use` paths; no file in `nulis-core/src` over roughly 2,500 lines; `git diff --stat` shows mostly moves.

### M54 — Split the largest UI files (finding 7, frontend)

1. `App.tsx` (3k lines): extract hooks for the workbench state (layout and dividers, terminal panel, windows/IPC events, session restore) and keep `App` as the composition.
2. `EditorViewport.tsx` (3k lines): extract the key handling (Enter, Tab, Backspace, the typed-edit queue `queueEdit`), the Markdown Edit/View switch, and the minimap into hooks or components.
3. `App.test.tsx` and `EditorViewport.test.tsx` (2–3k lines each) follow the code they test. Each test keeps its exact name and assertions.

**Acceptance:** Vitest shows the same test names and count; no change in the rendered DOM (spot-checked with the existing snapshot-style tests); a manual smoke run of the dev build covering typing, a split pane, the terminal, and the Markdown preview.

### M55 — Documentation and contributor on-ramp (findings 8, 9)

Goes last so the docs describe the code after M50–M54.

1. **Archive.** Move `IMPROVEMENT_PLAN.md` through `IMPROVEMENT_PLAN_4.md`, `IMPROVEMENT_SPEC.md`, `THEME_IMPLEMENTATION_PLAN.md` and every finished `docs/handover/*` into `docs/archive/`, with a short `docs/archive/README.md` index. Fix every link (`grep -rn "IMPROVEMENT_PLAN\|handover/" docs README.md`). This plan and active handovers stay in place.
2. **`WORK_STATE.md`.** Trim it to the current release and open items; older sections go into the archive.
3. **README.** Keep install basics, a feature summary, build and test commands, and links. Move the Homebrew troubleshooting, macOS Gatekeeper detail and long architecture text to `USER_GUIDE.md` and `ARCHITECTURE.md`. Target under 12 KB.
4. **`CONTRIBUTING.md`:** prerequisites, the all-suites-green command list, branch and commit style (as used in `git log`), the clean-room rule (`CLEAN_ROOM.md`), and DCO sign-off (§5 Q5): every commit carries `Signed-off-by:` (`git commit -s`). Enforce it in CI with a small job that checks each PR commit for the trailer, and mention it in the PR template.
5. **`.github/ISSUE_TEMPLATE/`:** `bug_report.yml` (version, OS, steps, expected vs actual, logs), `feature_request.yml`, and `config.yml` pointing security reports to `SECURITY.md`. Add a short `pull_request_template.md` with the checks list.

**Acceptance:** no broken relative links (checked with a link checker such as `lychee --offline docs README.md`, added to CI if the owner agrees); the README renders correctly on GitHub; new issue forms appear in the repository's "New issue" page after merge.

## 5. Questions for the owner

1. **`tmp/`**: delete it, or keep it locally and only ignore it? **Answer (2026-10-26):** delete it.
2. **Node version** for CI and `engines`: 22 or 24? **Answer:** 24. The owner also reported a constant CI warning about Node 20; that comes from the actions' runtime and is fixed by M51 step 2.
3. **MSRV**: keep and enforce one (raised if needed), or drop `rust-version`? **Answer:** keep it.
4. **Remote images default**: `block`, `ask`, or keep `allow`? And should `allow` exist at all? **Answer (2026-10-26):** default `ask`; all three modes offered. The owner first asked which images this means. Answer given: images that a Markdown document links from a web server (`![x](https://…)`), fetched when the preview renders that document. The app's own interface images and images stored beside the document are not affected.
5. **Contributions**: DCO sign-off, a CLA, or neither? **Answer:** DCO.

## 6. Risks

- **M53/M54 conflicts with feature work.** Large moves conflict with any open branch. Schedule them when no feature branch is open, and land each seam quickly.
- **MSRV surprise.** Pinned dependencies (`boa_engine`, `rhai`, `tauri`) may already need a Rust newer than 1.82. M51 then raises the declared version, which is a documentation change and not a regression.
- **Dependabot volume.** Grouping and the PR limit keep it manageable; exact pins mean nothing changes without a reviewed PR.
- **CSP regressions.** Tightening `img-src` could hide an image the UI relies on. M52 checks the Help, Settings, Print and About windows by hand before merging.

## 7. Release

M50–M52 can ship as **0.5.3** (M52 is user-visible). M53–M55 change no behavior and ship with the next feature release.
