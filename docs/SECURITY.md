# Security and threat model

## Assets and trust boundaries

Protect user files, credentials, editor state, host commands, and workspace privacy. Treat opened files, paths, workspaces, preview markup, terminal text, plugins, themes, and scripts as untrusted. The webview/native IPC boundary is privileged.

## Baseline controls

- Tauri exposes an explicit command list and minimal capabilities; remote web origins receive no IPC authority.
- IPC DTOs are typed, size-limited where payloads can grow, and validated again in Rust.
- Paths are normalized and checked by the owning provider; archives/plugins may not escape their install root.
- File saves use exclusive same-directory temporary creation, content sync, concurrent-change detection, and platform atomic replacement.
- Recovery manifests are versioned and parsed without automatically opening their paths; callers explicitly validate the original fingerprint and trusted session root before restoration.
- Shell commands use argument arrays, never concatenated shell interpolation.
- Preview content uses a restricted origin/sandbox, sanitization, no native bridge, and a restrictive content security policy.
- Plugin permissions are denied by default, user-visible, and enforced at every host operation.
- Scripts have capability, time, memory, and cancellation budgets.
- Themes remain declarative and cannot execute code.

## Current M14 posture

The shell enables core event/window/webview capability sets plus the native dialog plugin's open/save permissions and the clipboard plugin's text-write permission for local bundled content. Clipboard read, clear, image, and HTML permissions remain denied. It does not grant the webview a general filesystem plugin capability: selected paths enter the validated Rust command boundary and file bytes stay behind bounded viewport queries. Save destinations return to the core, which performs fingerprint-checked atomic replacement without exposing filesystem write APIs to the webview. Clipboard selection reads are capped at 8 MiB, and cut cannot delete until the native clipboard write succeeds. Session append stores and the workbench session live below the platform application-data directory.

The workspace service owns one canonical root. Every explorer request accepts only normalized relative components, rejects absolute paths and parent traversal, checks each component for symlinks, and exposes at most 1,000 sorted entries from one directory. Symlinks are visible but cannot be traversed or opened. Persisted workbench JSON is schema-checked, capped at 1 MiB and 128 source-backed documents, written through a synced temporary file, and validated again before restoration. A missing source or workspace is skipped without preventing the remaining session from loading.

Document comparison accepts only opaque document IDs and bounded coordinates. Hunk application copies at most 8 MiB and rejects a stale target revision before mutation. Folder comparison receives two explicit native-dialog selections, canonicalizes them in Rust, walks at most 10,000 entries per root, skips symbolic links instead of following them, and returns only bounded metadata/difference results. The webview still has no general filesystem read capability.


Terminal processes are created only by `nulis-terminal`. Programs and arguments remain separate arrays; no command string is constructed or evaluated. At most 16 sessions exist, input and read pages are capped at 64 KiB, retained output is capped at 1 MiB per session, dimensions and environment data are validated, and closing the UI terminates its child. The Tauri capability allowlist still grants the webview no shell/process or network plugin permission: terminal and SFTP access are narrow core commands subject to the common 1 MiB IPC request cap.

Recorded macro data is schema-versioned and bounded by action count, aggregate bytes, argument bytes, and JSON nesting depth before playback. Recording captures only an explicit safe command set after successful execution; session, file-open, workspace, credential, terminal, and automation commands are excluded. Playback rechecks that allowlist, so tampering with application-data JSON cannot add those capabilities. Saved observed revisions become a placeholder resolved immediately before replay, preserving optimistic concurrency checks. The macro library is validated on every read/write, capped at 128 entries and 1 MiB, and durably atomically replaced below application data. Playback checks cancellation between actions, stops at the first failure, and retains no result stream.

The script evaluator is deny-by-default: requested authority is distinct from and limits the persisted user-approved grant, while command execution also needs an exact allowlist entry. Changing script source, requested capabilities, commands, or metadata revokes approval. The 64-entry, 4 MiB script library is validated on every read/write and atomically replaced below application data. It uses a raw Rhai engine with modules, collections, closures, custom syntax, and script functions compiled out. There is no file module resolver or standard package. Only named `nulis_*` functions can reach the host, every attempt consumes the host-call budget and rechecks authority, and arguments/results remain depth- and byte-bounded JSON strings. Operation fuel, elapsed time, cooperative cancellation, output, variables, expression depth, string interning, and memory-derived string-size ceilings apply per run.

IPC protocol v13 exposes script lifecycle operations through the central command boundary. Host adapters are limited to the current document, safe workspace paths, effective settings, collected messages, and the same audited editor-command set used by macros. Workspace reads return bounded directory metadata; writes can replace only an existing regular non-symlink file and use fingerprint-checked atomic replacement. Terminal, credential, file-open, session, automation, process, network, environment, and arbitrary filesystem commands are not script-callable. The webview receives no additional native capability.

Plugin installation begins with an explicit native folder selection, but file reads and copies happen in `nulis-plugin-host`. Package roots, manifest/entry components, and file types are checked without following symlinks; only the strict manifest and one UTF-8 JavaScript entry are copied beneath application data. IDs, semantic versions, engine compatibility, activation events, command namespaces, duplicate commands, sizes, and counts are validated. Existing plugin IDs are never overwritten implicitly.

Installed bytes are SHA-256 identified. Approval records live separately, may include only manifest-requested permissions, and match one exact package digest. Unapproved or changed plugins contribute no palette commands and cannot execute. Revocation removes contributions immediately; uninstall revokes before deleting the exact validated package directory. API v1 grants only `showMessage`, and the core checks every worker-returned operation against that grant before surfacing it.

Each activation or command invocation uses a fresh Nulis worker process with a bounded JSON pipe and a pinned Boa engine. The realm has no Node/Web runtime, module loader, native IPC, filesystem, network, process, environment, credential, terminal, clipboard, document, workspace, or arbitrary core-command API. Source, argument, response, command, operation, and message payloads are capped; JavaScript loop, recursion, and stack limits apply. The parent kills the worker after two seconds or on cancellation, and rejects crashes, nonzero exits, malformed output, excess output, or manifest/runtime registration mismatch. Per-process OS memory quotas and package signing remain explicit M14 hardening work.

Customization JSON is schema-versioned, capped at 256 KiB and 64 entries per configurable list, validated before use, and atomically replaced below application data. Shortcut chords use a small canonical grammar, duplicate chords are rejected, menu separators cannot form unsafe edge/consecutive layouts, toolbar commands are unique, and every loaded or newly saved action must belong to an explicit safe zero-argument command set or an approved plugin contribution. Terminal, credential, automation-lifecycle, session, and customization-lifecycle commands cannot be promoted into menus or shortcuts. The native menu adapter independently validates its bounded DTO and translates only supported portable accelerators.

Theme installation starts from an explicit native file selection. Rust accepts only a regular non-symlink JSON file no larger than 256 KiB, requires the complete semantic token set and strict hex colors, rejects duplicate IDs and built-in shadowing, caps the user catalog at 64 files, and installs through exclusive atomic persistence. Theme data contains no source, CSS, URL, or executable field; uninstall targets only the validated ID-derived file beneath the user-theme directory.

Markdown View mode renders bounded current source with raw HTML skipped. Local images are restricted to the saved document directory and 4 MiB per image. Updater and arbitrary URL navigation remain disabled. Plugin execution is available only through the isolated, approved API-v1 boundary above. The narrowly scoped save dialog is enabled only to choose a destination; all writes remain in the validated core command boundary. Development configuration must not be shipped as production policy.

Dirty local and untitled documents checkpoint only source ranges and append-store pointers under application data. The append store is synced before an atomic manifest write. A restore requires the original fingerprint to match and the append store (and untitled original) to resolve within the session root; a changed source is shown as a conflict with only a discard choice. Linux workers set a 512 MiB virtual address-space limit before executing plugin code. macOS and Windows OS memory quotas remain release-hardening work.

The webview has no general window-destroy permission. Its guarded native close command verifies that no dirty documents remain before destroying the window. Native Quit requests travel through the same close path.

## Supply chain and release

CI runs locked dependency installs once lockfiles exist, vulnerability/license auditing is required before releases, and production artifacts will be signed where platform credentials exist. Update metadata must be signed and delivered over authenticated transport. Signing/notarization remains deferred until credentials are available.

Security reports should contain reproduction steps without sensitive user data. A private reporting channel must be published before public release.
