# Third-party licenses

Nulis itself is licensed under the GNU General Public License, version 3 or (at your option) any later version (`GPL-3.0-or-later`); see [LICENSE](LICENSE).

Dependencies are accepted under licenses that can be combined with GPL version 3: MIT, Apache-2.0, BSD-style, ISC, Zlib, Boost, Unicode, Unlicense, MPL-2.0, and similar. A dependency offered under several licenses is used under one that is compatible.

GPL version 3 was chosen over version 2 because the locked dependency graph contains crates that require Apache-2.0 (`tao`, `dpi`, `openssl`, `gethostname`, `sync_wrapper`), and the Free Software Foundation considers Apache-2.0 compatible with GPL version 3 but not with version 2. A new dependency under a license that cannot be combined with GPL version 3 (for example GPL-2.0-only) requires a recorded licensing decision before adoption.

Initial direct dependencies:

| Dependency | Purpose | License |
| --- | --- | --- |
| Rust, Cargo | toolchain | Apache-2.0 / MIT |
| Tauri 2 | desktop shell | Apache-2.0 / MIT |
| Tauri dialog plugin | native user-selected file dialogs | Apache-2.0 / MIT |
| Tauri clipboard manager plugin | write-only native text clipboard access | Apache-2.0 / MIT |
| Tauri single-instance plugin | a second launch focuses the running window | Apache-2.0 / MIT |
| serde / serde_json | IPC and settings serialization | Apache-2.0 / MIT |
| tempfile | secure temporary files and atomic save persistence | Apache-2.0 / MIT |
| thiserror | typed Rust errors | Apache-2.0 / MIT |
| Tree-sitter / tree-sitter-language | bounded incremental parsing and language ABI | MIT |
| tree-sitter-json | built-in JSON grammar | MIT |
| tree-sitter-python | built-in Python grammar | MIT |
| ssh2 | SFTP and SSH transport | Apache-2.0 / MIT |
| libssh2 (compiled in by ssh2) | SSH protocol implementation | BSD-3-Clause |
| OpenSSL 3 (compiled in on macOS and Linux) | cryptography for SSH | Apache-2.0 |
| keyring | platform credential storage | Apache-2.0 / MIT |
| portable-pty | cross-platform pseudoterminal sessions | MIT |
| Rhai | reduced embedded automation-script evaluator | Apache-2.0 / MIT |
| rustix | plugin worker address-space limit on Linux | Apache-2.0 / MIT |
| libfuzzer-sys | optional document fuzz target | Apache-2.0 / MIT |
| Boa | isolated JavaScript plugin evaluator | Unlicense / MIT |
| sha2 | plugin package digests | Apache-2.0 / MIT |
| base64 | bounded Markdown image encoding | Apache-2.0 / MIT |
| semver | plugin and engine-version validation | Apache-2.0 / MIT |
| React | view layer | MIT |
| react-markdown / remark-gfm | safe Markdown preview and GitHub-style syntax | MIT |
| xterm.js / xterm fit addon | integrated terminal rendering and sizing | MIT |
| Vite | frontend build | MIT |
| TypeScript | frontend type system | Apache-2.0 |
| Vitest | frontend tests | MIT |
| ESLint and TypeScript ESLint | frontend linting | MIT |
| NvChad Base46 themes | bundled color palettes | MIT; upstream license texts in `themes/nvchad-source/` |

Transitive notices will be generated and reviewed before the first distributed build. This file is not a substitute for the license texts included in release artifacts.
