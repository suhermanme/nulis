## What this changes and why

<!-- What a user or a reviewer will notice, and the reason. Link an issue if there is one. -->

## How it was checked

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`
- [ ] `npm run lint`, `npm run typecheck`, `npm run test`
- [ ] New behavior has a test; a bug fix has a test that fails without it
- [ ] `docs/USER_GUIDE.md` describes any changed behavior
- [ ] Every commit is signed off (`git commit -s`; see CONTRIBUTING.md)
