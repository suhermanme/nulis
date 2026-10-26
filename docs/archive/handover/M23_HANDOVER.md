# M23 handover — External changes and Reload

**Status:** completed 2026-10-20 on macOS (automated checks), on `main`. Not tried by hand in the running app; Windows/Linux not run.

## Objectives

User request: when another program modifies an open file, ask whether to reload it; offer a manual Reload.

## Implementation

**Core (`crates/nulis-core/src/editor/external_changes.rs`, new)**

- `open_source(path)`: the disk-backed open steps (fingerprint before/after, `FileSource`, append store) moved here from `open_file`, which now calls it; `reload` uses it too.
- `external_changes()`: for each open local document, compares `fingerprint(path)` with the fingerprint recorded at open/save; reports `modified` or `deleted` and whether the document is dirty. Only metadata calls; files that can't be examined are skipped.
- `reload(id, discard)`: re-opens the file inside the same document ID. Refused while dirty unless `discard`. Keeps Read-Only mode; clamps carets and bookmarks to the new length; resets undo, line-change markers, and the navigation index; removes the old append store and any recovery checkpoint.
- `keep_external_version(id)`: adopts the file's current fingerprint (or none if deleted) so the change is no longer reported and the next Save replaces/recreates the file.
- Commands `editor.externalChanges`, `file.reload` (`{ discard }`), `editor.keepExternalVersion`; IPC 29 → 30.

**Adapter:** File menu always has **Reload from Disk** (`file.reload`), like Close File.

**Desktop**

- `App.tsx`: polls `editor.externalChanges` on window focus and every 3 s while focused (desktop shell only); one dialog per `documentId:kind` (Reload / Keep Mine; Keep Open / Close for deleted; stronger wording when dirty); `requestReload` for the menu, palette, and right-click (confirms when dirty); a per-document epoch in the view key makes every view re-read a reloaded document.
- `EditorViewport.tsx`: `onReload` prop → right-click **Reload from Disk**.
- `commands.ts`: `file.reload` in the palette and customizable commands. `contracts.ts`: `ExternalChange`.

**Docs:** `USER_GUIDE.md` (Files changed by other programs), `FEATURE_PARITY.md`.

## Tests

- Core: change reported until reload, reload keeps the ID and clears undo; reload with edits needs `discard`; Keep Mine stops reports and lets Save overwrite; deleted file reported.
- Desktop: focus-triggered prompts — Keep Mine (dirty wording), not asked twice, Reload, deleted → Close; File ▸ Reload from Disk with unsaved edits asks first; right-click item.
- Full suite: fmt/clippy clean; Rust all passed (nulis-core 108); lint/typecheck clean; Vitest 124 passed.

## Decisions

- Polling `stat` on focus and every 3 s instead of a file-system watcher: no new dependency, works the same on every platform, and costs one metadata call per open file.
- Reload keeps the document ID so tabs, panes, and workspace layout are untouched.

## Known issues

- No merge of both versions; the choice is one or the other.
- If another program rewrites the file **in place** (not by replacing it), the editor's disk-backed view of unchanged text can show the new bytes before you answer; programs that save by replacing the file (most editors) are not affected. Reload is the safe answer then.
- The caret keeps its byte offset, not its line, after a reload.
- Remote documents are not checked.

## Resume

Next: M24 New application icon (user request).
