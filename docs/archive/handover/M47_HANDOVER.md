# M47 handover — optional language servers

**Date:** 2026-10-24. **Branch:** `feat/language-capabilities` (not pushed). **Plan:** `IMPROVEMENT_PLAN_4.md` §5 M47. **Tracker:** `IMPLEMENTATION_TRACKER.md`, M47.

## What changed

- **`nulis-lsp` (new crate).** A small LSP client over a child process's standard input/output: `Content-Length` framing, JSON-RPC requests with deadlines, notifications, `$/cancelRequest` on timeout, shutdown, and position conversion between UTF-8 byte offsets and UTF-8/UTF-16 (`crates/nulis-lsp/src/lib.rs`). No network access and no editor state. A fake server (`src/bin/nulis-fake-lsp.rs`) supports normal, crash, timeout, and UTF-16 modes; `tests/client.rs` has 6 integration tests.
- **Core (`crates/nulis-core/src/language_servers.rs`).** Reads `lsp.servers` from effective settings into validated `ServerConfig`s (ID, command vector, languages, optional initialization options); invalid entries are dropped and reported. Approval records the exact ID + command + languages in `lsp-approvals.json` under the session root, so editing either revokes it. One process per project folder (nearest `.git`, `Cargo.toml`, `package.json`, `go.mod`, `pyproject.toml`, `compile_commands.json`, or `.clangd`), started on the first matching document and stopped when the last one closes. Full-text `didOpen`/`didChange`/`didClose`, `publishDiagnostics` clipped to 2,000 problems per document and placed against the revision that was sent, completion items merged into `completion.request`, and `textDocument/formatting` as the fallback for Beautify. Three consecutive failures set a server aside until restart.
- **Commands and IPC 46.** `lsp.state`, `lsp.approve`, `lsp.revoke`, `lsp.restart`, `lsp.diagnostics`. `DocumentCapabilities.languageServer` names the approved server for a document. New DTOs: `LanguageServerStatus`, `LanguageServersState`, `DocumentProblem`, `DocumentProblems`. `IPC_PROTOCOL_VERSION` and `contracts.ts` both say 46.
- **Desktop.** `App.tsx` polls `lsp.diagnostics` for the active document 500 ms and 2.5 s after its revision changes, ignores answers for another revision, and drops problems for closed documents. The status bar counts problems and opens the new `ProblemsPanel`; choosing one selects its bytes and reveals them. `EditorViewport` draws a severity dot in the gutter and a wavy underline over the range, splitting the line at range edges. Settings ▸ Languages ▸ Language servers lists configured servers with Approve/Revoke/Restart, running folders, errors, and configuration problems.
- **Window boundary.** `lsp.state/approve/revoke/restart` join the Settings window's allowlist; approve/revoke/restart join `CUSTOMIZATION_MUTATIONS`, so the main window refreshes open summaries and the other windows reload.
- **Tests.** New `ProblemsPanel.test.tsx` (4), EditorViewport diagnostics (2), and App polling/panel/jump/stale/Settings approval (4). Full Vitest: **208 passed** (23 files). Rust: **216 passed** across the workspace, including the 6 fake-server tests.

## Second server: clangd (verified)

- **License:** LLVM is Apache-2.0 WITH LLVM-exception, OSI-approved (`https://llvm.org/`, `https://releases.llvm.org/`). Nulis does not bundle it, so no third-party code enters the repository.
- **Platforms:** official LLVM release binaries cover macOS (x86-64 and arm64), Windows (x86-64), and Linux (x86-64, arm64); clangd is part of the LLVM release and is also packaged by Homebrew, apt, and dnf (`https://clangd.llvm.org/installation`).
- **Maintenance:** two major releases a year with regular point releases; LLVM 23.1.x was current in 2026 (`https://llvm.org/` release schedule, `https://github.com/llvm/llvm-project/releases`). Active upstream project with public issue tracking.
- **Rust:** `rust-analyzer` is MIT/Apache-2.0 and publishes binaries for the same three platforms; it stays the documented Rust example.

## Try it in the app

Back up and quit Nulis first. Add to `settings.json` (in the folder named in the README):

```json
{
  "schemaVersion": 1,
  "values": {
    "lsp": {
      "servers": {
        "rust": { "command": ["rust-analyzer"], "languages": ["rust"] }
      }
    }
  }
}
```

Restart Nulis, open a Rust file in a Cargo project, then approve `rust` in **Settings ▸ Languages ▸ Language servers**. Introduce a type error: a dot appears in the gutter and a wavy underline under the text; the status bar counts it; clicking the count opens the Problems panel and choosing the problem selects it. Edit the file and watch the count update. Then Revoke: the server stops and the marks clear. Also try `clangd` with `["c", "cpp"]` in a folder holding `compile_commands.json` or `.clangd`.

## Checks run

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test --workspace` | 216 passed |
| `npm run lint` (in `apps/desktop`) | clean |
| `npm run typecheck` | clean |
| `npx vitest run` | 208 passed (23 files) |

## Not done by hand

- rust-analyzer on macOS, Windows, and Linux (recorded as pending in `PLATFORM_MATRIX.md`). The automated tests use the fake server only.
- The M46 in-app check is still pending from the previous handover.
- CI has not run on M46/M47 because the branch has never been pushed. The workspace job now also runs `cargo test -p nulis-lsp` explicitly on all three platforms.

## Known limits

- One server process per project folder per server; no workspace-folder switching after start.
- Diagnostics are fetched only for the active document, on a fixed 500 ms/2.5 s schedule; there is no push-to-UI stream and no diagnostic-related code actions.
- Server completion is requested only when the built-in list would open (explicit or enough prefix); it is not cancelled per keystroke.
- A server's formatting result is applied as one undo step, but there is no range formatting or format-on-save.
- `lsp.approve/revoke/restart` need arguments, so they are not palette-safe; only `lsp.state` appears there.

## Next: M48

External plugin readiness: API v2 manifests with declarative languages and associations, owned run-time `LexerSpec`, indentation/keyword/snippet contributions, per-plugin and per-capability enable/disable/remove, conflict UI, hostile-manifest and isolation tests, documentation, and a security review before executable providers.
