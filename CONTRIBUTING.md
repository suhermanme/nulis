# Contributing to Nulis

Thank you for helping. This page covers setting up, what a change must pass, the rules for new code, and how to sign off your commits.

## Setting up

You need Rust 1.90 or later, Node.js 24 (`.nvmrc` names it) with npm 11, a C compiler, and what Tauri 2 needs on your system; [Building from source](README.md#building-from-source) in the README lists them. Then:

```sh
git clone https://github.com/suhermanme/nulis.git nulis
cd nulis
npm ci
npm run dev        # the app with live reload
```

## Rules for a change

- Run the checks under [Testing and quality checks](#testing-and-quality-checks) before sending a change.
- New behavior needs a test. A bug fix needs a test that fails without the fix.
- Keep operations bounded: a new feature must not read, hold, or send a whole document unless its purpose requires it, and then it needs a size limit.
- Record a new dependency and its license in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md). It must be compatible with GPL version 3.
- Nulis is an original implementation. It takes ideas about what an editor should do from existing editors but contains no code or assets from them. [docs/CLEAN_ROOM.md](docs/CLEAN_ROOM.md) has the rules.
- Some interface tests read Rust source files (for example, `commands.test.ts` checks the core's command table). After moving or renaming Rust code, run `npm run test` as well as the Rust checks.
- A change that alters a message between the interface and the core raises `IPC_PROTOCOL_VERSION` in both `crates/nulis-core/src/lib.rs` and `apps/desktop/src/contracts.ts`.
- Changed behavior is described in [docs/USER_GUIDE.md](docs/USER_GUIDE.md), which is also the in-app Help.

## Testing and quality checks

These are the checks CI runs on every push to `main` and every pull request ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)). All must pass before a change is merged. CI also builds an installable bundle on macOS, Windows, and Linux, runs the document fuzzer for a minute, runs a large-file smoke benchmark, and checks dependencies for known advisories.

```sh
# Rust
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# Interface
npm run lint
npm run typecheck
npm run test
```

- Rust unit tests live beside the code they test. They include files larger than 4 GiB, made as sparse files so they take no real disk space.
- Interface tests use Vitest and Testing Library against a mocked core.
- The document model has a fuzz target; see [fuzz/README.md](fuzz/README.md).
- Performance methodology and the baseline measurements are in [docs/PERFORMANCE.md](docs/PERFORMANCE.md) and [benchmarks/README.md](benchmarks/README.md).

Clippy runs with the pedantic lints as errors. CI also builds the workspace with the minimum Rust version that `rust-version` in `Cargo.toml` names.

### Build parts separately

```sh
npm run frontend:build --workspace @nulis/desktop   # interface only, into apps/desktop/dist
cargo build --workspace --locked                    # all Rust crates
cargo build --release -p nulis-benchmarks           # the performance probe
```

## Commits and pull requests

- Work on a branch, not on `main`, and open a pull request against `main`.
- Commit messages follow the style already in the history: a type and an area, then what changed, such as `fix(markdown): say when an allowed remote image fails to load` or `refactor(core): move hex editing to editor/hex.rs`. Types in use: `feat`, `fix`, `refactor`, `test`, `docs`, `ci`, `chore`. The body says why.
- One logical change per commit, so each can be reviewed and reverted on its own.

## Signing off: the Developer Certificate of Origin

Every commit must be signed off. By signing off you state that you wrote the change, or otherwise have the right to submit it under the project's license, as set out in the [Developer Certificate of Origin](https://developercertificate.org/). Signing off adds a line with your name and email address to the commit message:

```text
Signed-off-by: Your Name <you@example.com>
```

Git adds it for you with `-s`:

```sh
git commit -s -m "fix(search): …"
```

The name and email address come from `git config user.name` and `git config user.email`. A CI check refuses a pull request with a commit that lacks the line. To sign off commits you have already made on your branch:

```sh
git commit --amend -s --no-edit          # the last commit
git rebase --signoff main                # every commit since main
git push --force-with-lease
```

Commits by Dependabot are exempt; its pull requests are reviewed like any other.

## Reporting bugs and security problems

Use the issue forms on GitHub for bugs and feature requests. Do not report a security problem in a public issue; report it privately as [docs/SECURITY.md](docs/SECURITY.md#reporting-a-vulnerability) describes.

## License

By contributing, you agree that your contribution is licensed under the GNU General Public License, version 3 or later, like the rest of Nulis.
