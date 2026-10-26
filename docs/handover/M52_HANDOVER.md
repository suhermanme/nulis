# M52 handover — Markdown preview privacy

Third milestone of `docs/IMPROVEMENT_PLAN_6.md`, on branch `chore/repo-health`. IPC protocol 48 → 49.

## Why

Before this, `MarkdownPreview.tsx` passed any `http(s):` image straight to an `<img>`, and the CSP allowed `img-src https: http:`. Previewing a Markdown file therefore contacted every server it linked, revealing the reader's address and when the file was opened, and over `http:` in clear text. The README itself does this with its CI badge. `SECURITY.md` treats preview markup as untrusted and promises workspace privacy.

## What changed

- **Setting.** `markdown.preview.remoteImages`: `"block"` | `"ask"` | `"allow"`, default `"ask"` (owner's choice, plan §5 Q4).
  - Frontend: `RemoteImagePolicy`, `DEFAULT_REMOTE_IMAGE_POLICY` and `remoteImagePolicyOf(values)` in `contracts.ts`, which falls back to `"ask"` for an absent or unknown value.
  - Core: `SettingsDocument::validate` (`crates/nulis-settings/src/lib.rs`) refuses any other value with a reason naming the key.
- **Preview** (`apps/desktop/src/MarkdownPreview.tsx`).
  - An `https:` image becomes an `<img>` only under `"allow"`, or under `"ask"` after **Load remote images**. Otherwise it renders a `role="img"` placeholder with the alt text, the host, and the full URL as a tooltip; `"ask"` adds the button.
  - Approval is held per document ID in a small module store read with `useSyncExternalStore`, so split panes of one document update together. Document IDs are counter-assigned (`document-{n}`) and never reused, so an approval ends with its document; a reopened file asks again.
  - `http:` images are blocked under every policy ("Insecure remote image from …, blocked").
  - Local relative images still load through `editor_markdown_preview_image`, unchanged.
- **Editor wiring.** `App.tsx` reads the policy with `editor.suggest` (`loadSuggestSettings`, which re-runs on settings changes) and passes it to `EditorViewport` (`remoteImages` prop), then on to the preview.
- **Print.** The print window can read no settings, by design. `PrintContent::Markdown` gained `remote_images: bool` (`#[serde(default)]`, so absent means no). `App.tsx` sets it with `remoteImagesAllowed(policy, documentId)`, and `PrintWindowContent::Markdown` returns it (`remoteImages`). `PrintApp` renders `"allow"` or `"block"`, never offering the button. `PageSetupDialog` takes the same boolean.
- **CSP** (`tauri.conf.json`): `img-src 'self' asset: http://asset.localhost data: https:`, with `http:` removed. `http://asset.localhost` is the asset protocol on Windows and stays. The new test `the_content_security_policy_never_loads_images_over_plain_http` in the desktop crate parses the config and fails on any other `http:` source.
- **Settings window.** Editor ▸ **Markdown preview** ▸ **Remote images** (Ask for each document / Never load / Always load), part of the Editor section's draft. `withIndentation` writes `markdown.preview.remoteImages` only when it differs from the stored value, so saving indentation adds no key; other `markdown.preview` values are kept.
- **Styles.** `.markdown-remote-image*` in `styles.css` use theme tokens only.
- **Docs.** `USER_GUIDE.md` ("Markdown preview"; the Help window renders this file), `SECURITY.md` (baseline controls), and `settings.schema.json`.

## Found, not changed

`react-markdown`'s default URL transform drops `data:` image sources, so inline `data:` images never displayed, before or after this change. The old `data:` branch in `PreviewImage` was dead code, and I kept it. Allowing `data:` would mean a custom `urlTransform`; that is out of scope here, so the test that would have covered it was dropped rather than faked.

## Checks run (macOS, 2026-10-26)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` clean.
- `cargo test --workspace --locked`: 471 passed, 0 failed. New: `every_remote_image_policy_is_accepted`, `the_content_security_policy_never_loads_images_over_plain_http`. Extended: `malformed_values_are_refused_with_a_reason` (two bad policies) and the print-content test (`remoteImages` default false, true round-trips, camelCase on the way out).
- `npm run lint`, `npm run typecheck` clean; Vitest desktop 26 files / 261 tests (+8), SDK 1 / 2; `frontend:build` green.
  - New in `MarkdownPreview.test.tsx`: ask (no fetch, then the button loads all of the document's remote images), approval shared across panes of a document but not across documents, block, allow, `http:` under all three, and the setting parser.
  - New in `PrintApp.test.tsx`: the print window follows the editor's decision.
  - New in `App.test.tsx`: Settings saves the choice beside existing `markdown.preview` values.
  - Existing tests are unchanged, except that two `PrintApp` fixtures gained `remoteImages: false`.

## Owner's check (not done here)

The webview has no accessibility tree and `screencapture` is black in this shell, so the native check is yours:

1. `python3 -m http.server 8765` in an empty folder (it logs every request).
2. Save `probe.md` with `![a](http://127.0.0.1:8765/a.png)` and `![b](https://example.com/b.png)` and a local image beside it.
3. Open it in Nulis and switch to **View**. Expect: the local image shows; both remote ones are placeholders; the server logs nothing.
4. Press **Load remote images**: the `https:` one loads, the `http:` one stays blocked, the server still logs nothing.
5. **Print…** shows the same; set the policy to **Never load** in Settings and the button disappears.

## Next

M53: split `nulis-core`'s `editor.rs` and `lib.rs` (code moves only).
