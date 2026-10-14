# Benchmarks

The M01 probe exercises bounded opening, first-viewport reads, deterministic
random reads, a 64 MiB line-index sample, and 1,000 piece-table edits.

Build the release probe:

```sh
cargo build --release -p nulis-benchmarks
```

Create a disposable sparse 10 GiB fixture without writing 10 GiB of physical
data, then probe it:

```sh
target/release/nulis-benchmarks generate-sparse /tmp/nulis-10g.dat 10GiB
target/release/nulis-benchmarks probe /tmp/nulis-10g.dat
```

Measure peak resident memory around the `probe` command using the operating
system tool described in `docs/PERFORMANCE.md`. Results are key/value lines so
they can be captured and compared in CI without another runtime dependency.

For the M14 physically allocated corpus, use `generate-pattern` with an exact
size and one of `long-line`, `short-lines`, `multilingual`, `malformed`, or
`binary`. Generation streams through a 64 KiB buffer and uses a deterministic
binary seed. For example:

```sh
target/release/nulis-benchmarks generate-pattern /tmp/nulis-multilingual.dat 100MiB multilingual
target/release/nulis-benchmarks probe /tmp/nulis-multilingual.dat
```

Generate 1 MiB, 100 MiB, and 1 GiB physically allocated samples only on a
disposable filesystem with sufficient free space. The 10 GiB sparse fixture
tests logical size and bounded reads; label it sparse in any comparison.

## Line change indicator probe

`probe-line-changes` opens the fixture through the editor core, applies five
edits spread across the file, and measures the first viewport, a deep viewport,
and the document map with line change markers attached:

```sh
/usr/bin/time -l target/release/nulis-benchmarks probe-line-changes /tmp/nulis-10g.dat
```

Marker state is a sparse range list, so the marked-line and tick counts stay
proportional to the number of edits and peak memory must not grow with the
file. The probe never saves, so it does not write the fixture's logical size.
