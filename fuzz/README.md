# Document fuzzing

The `piece_table` target compares arbitrary byte replacements against a byte-vector model. Run it with the nightly Rust toolchain and `cargo-fuzz`:

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run piece_table -- -max_total_time=300
```

Crashing inputs are saved under `fuzz/artifacts/piece_table/`. Add a minimized regression to `nulis-document` before fixing a finding. The normal workspace test suite also runs deterministic generated edit sequences without a nightly toolchain.
