# Performance methodology

## Invariants

- Opening a source does not read or decode the complete file.
- Resident memory must be bounded by configured caches, indexes, edits, viewport data, and undo—not source length.
- First viewport work has priority over optional full-file indexing.

## Corpus generation

Benchmarks will deterministically generate 1 MiB, 100 MiB, 1 GiB, and sparse/generated 10 GiB+ sources; one extremely long line; millions of short lines; multilingual UTF-8; malformed bytes; and high-entropy binary. Large fixtures are generated locally and excluded from version control.

## Measurements

Record OS, filesystem, CPU, memory, build profile, commit, and cold/warm cache state. Measure peak RSS after open, time to first viewport, indexing rate, sequential and random navigation, edit p50/p95/p99 latency, search and save throughput, and undo/redo latency.

For each source-size series, compare RSS growth. A failure occurs if source materialization or an index makes memory approach source size. The M01 gate requires source-size-independent opening plus bounded viewport/index buffers; product budgets will be tightened as M02 rendering and later services are added.

The release-mode `nulis-benchmarks` executable generates sparse fixtures and emits stable key/value measurements. See `benchmarks/README.md`. Peak RSS must be collected around the process with `/usr/bin/time -l` on macOS or `/usr/bin/time -v` on Linux; Windows CI should record peak working set through PowerShell process counters. Sparse-fixture timings must be labeled and must not be compared with physically allocated corpora.

## Safety

Sparse files must be detected before results are compared across filesystems. Save benchmarks use disposable fixtures and verify output byte-for-byte. Interrupted-save tests confirm the original remains intact.

## M01 baseline

Automated tests open a generated sparse file larger than 5 GiB, verify its 64-bit length, and read only a five-byte marker beyond the 5 GiB boundary. A virtual-source test edits across 4 GiB without allocating the source, and a 5,000-edit test checks that the implicit treap remains bounded in height.

The first local release baseline was captured on 2026-10-11 on macOS 26.7 x86-64 against a warm-cache, sparse 10 GiB source. Opening metadata took 34 µs, the first 64 KiB viewport read took 24 µs, 256 random 4 KiB reads took 389 µs, a 64 MiB line-index sample took 91,559 µs, and 1,000 inserts took 154 µs. Peak resident set size reported by `/usr/bin/time -l` was 1,908,736 bytes (about 1.82 MiB). These numbers demonstrate the bounded-memory architecture on this fixture; they are not cross-machine performance targets. Hosted Linux/Windows baselines and cold-cache, physically allocated corpora remain pending.

## M02 viewport gate

The desktop test suite supplies a bounded 20,000-line snapshot for a logical 10 GiB document and asserts that virtualization creates no more than 40 `.editor-line` elements at the default 500 px viewport height. Separate tests verify off-screen caret reveal, long-line continuation near a deep byte offset, sparse-checkpoint line lookup after edits, and drag selection across viewport pages. These are deterministic architecture guards rather than frame-time benchmarks; interactive scroll latency and platform webview memory remain part of the broader production benchmark matrix.

## M07 hexadecimal gate

The hexadecimal core reads one aligned window of at most 64 KiB and the desktop requests 4 KiB pages regardless of source length. A sparse-file regression writes and reads a marker beyond 5 GiB through the real file-backed piece table. A separate 10 GiB logical-document UI test supplies thousands of rows and asserts that virtualization retains no more than 40 `.hex-row` elements at the default viewport height. Hexadecimal clipboard and edit payloads are independently capped at 256 KiB.

## M08 workspace gate

The explorer requests exactly one directory at a time and retains no more than 1,000 entries per loaded directory; the desktop default is 500 and it holds at most 256 loaded directories. Entries are sorted directory-first without following symlinks, and unopened directory contents never cross IPC. Session JSON is capped at 1 MiB and 128 source-backed documents, so restart state cannot grow with workspace size. Large-directory latency and paged enumeration remain part of the M14 platform benchmark matrix.

## M09 comparison gate

Text comparison retains at most 1 MiB and 1,024 records from each source plus a bounded LCS matrix; the desktop uses 256 KiB pages and 500-change results. Binary and folder content checks use 64 KiB buffers and never materialize source files. Folder traversal retains at most 10,000 entries per root and returns at most 5,000 differences (1,000 in the desktop). Regressions exercise comparison beyond 5 GiB, result pagination, symlink exclusion, and one-action merge undo. Physically allocated multi-gigabyte compare throughput remains part of M14 benchmarking.

## M14 recovery and fuzzing gate

Recovery checkpoints serialize source ranges and sync inserted bytes without reading the full logical document. Generated byte-vector model tests cover arbitrary replacement sequences, malformed bytes, and snapshot restoration across 32 seeds. The separate libFuzzer target accepts arbitrary operation streams; local non-instrumented smoke runs completed 10,000 iterations, and hosted nightly sanitizer runs are configured in CI. The benchmark generator now streams exact-size long-line, short-line, multilingual, malformed-byte, and deterministic binary fixtures for the physically allocated corpus.

## M16 line change indicator gate

`nulis-benchmarks probe-line-changes` opens a fixture through the editor core, applies five edits spread across the file, and reads the first viewport, a deep viewport, and the document map with markers attached. On 2026-10-13, on macOS 26.7 x86-64 against a warm-cache sparse 10 GiB source, opening took 287 µs, the five edits 283 µs, the first viewport 274 µs, a viewport at 8 GiB 212 µs, and the map 5,322 µs; the map carried exactly five ticks and peak resident set size was 10,489,856 bytes (about 10 MiB). Marker state is a range list capped at 4,096 entries, so it grows with the number of separate edits, not with file length or line count. The figure is a sparse-fixture architecture check, not a cross-machine target; hosted Linux/Windows runs remain pending. Addressing a deep viewport by line number still counts line endings from the nearest sparse checkpoint and is outside this gate.
