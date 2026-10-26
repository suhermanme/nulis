# M28 handover — File conveniences

**Status:** completed 2026-10-20 on macOS (automated checks), on `main`. Not tried by hand; Windows/Linux not run.

## Implementation

- `crates/nulis-core/src/editor/rename.rs` (new): `rename_file(id, name)` renames the source file within its folder. It refuses remote and untitled documents, empty names, `.`/`..`, separators, NUL, Windows-reserved characters (on Windows), names over 255 bytes, an existing destination (a letter-case-only rename of the same file is allowed), and a source whose fingerprint changed or that vanished. The piece table keeps reading through its open handle, so text and unsaved edits are untouched; name, path, fingerprint, and language are updated. Error `EditorError::Rename`.
- `lib.rs`: real command `file.renameTo { name }`; presentation commands `file.saveAll`, `file.closeAll`, `file.openRecent`, `file.clearRecent`, `file.rename`, `file.revealInPlaces`; IPC 33 → 34.
- Adapter: `FILE_EXTRA_MENU_COMMANDS` (Open Recent, Save All, Rename File, Show in Places) and Close All Files are always added to the native File menu unless the customized menu already lists them.
- Desktop:
  - `preferences.ts`: recent files in `localStorage` (`nulis.recentFiles.v1`, 20 entries) and `withRecentFile`.
  - `App.tsx`: `addDocument` records local files (dialog, Places, folder tree, drag and drop); Save As and Rename update the list. Save All reuses `saveDocumentFor` in order and reports skipped files; Close All reuses `continueTabsClose`. Open Recent is a palette-style dialog; `openPath` now returns whether it opened. Rename is a small form dialog that preselects the name without its extension.
  - `FileExplorer.tsx`: `reveal` prop lists the ancestors of the file (reusing loaded folders), expands them, marks the row `revealed`, scrolls and focuses it, clears a filter that would hide it, and reports a hidden file. The first Places load now merges its expanded folders instead of replacing them.

## Tests

- Core: rename keeps text and edits, re-detects the language, then saves to the new path; refuses bad names, an existing file (left untouched), and untitled documents; refuses a changed or deleted source.
- Desktop: recent list recording, filtering, opening, and dropping a missing file; Save All, Rename, Show in Places, and Close All through the File menu; Places reveal with expansion, highlight, focus, and the hidden-file message.

## Fix after M29 (Windows CI)

Rename File treated an existing destination as "the same file under another letter case" when the fingerprints matched. On Windows a fingerprint is only size and modification time, so a different file of the same size written in the same instant matched and was overwritten. The case-only check now compares names: the new name must differ from the current one only in letter case, and no folder entry may be spelled exactly like it. Found by `refuses_bad_names_existing_files_and_untitled_documents` on the Windows runner; a test for a case-only rename was added.

## Known issues

- The existence check and the rename are separate steps, so a file created at the destination in between could be replaced on Unix.
- A rename does not move a recovery manifest written before it; the next checkpoint writes one with the new path.
- Revealing a file below a folder that Places does not list (for example a symlinked path) expands the folders but cannot highlight it.
- Save All skips remote documents (they are reported as unsaved); Save on a remote document asks for a local destination.

## Resume

Next: M29 Column and clipboard tools.
