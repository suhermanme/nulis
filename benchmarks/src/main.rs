use nulis_core::{DocumentManager, ViewportRequest};
use nulis_document::PieceTable;
use nulis_large_file::{FileSource, IndexCancellation, MemoryAppendStore, ReadAt, SparseLineIndex};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Seek, SeekFrom, Write},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

const VIEWPORT_BYTES: usize = 64 * 1024;
const RANDOM_READ_BYTES: usize = 4 * 1024;
const RANDOM_READS: u64 = 256;
const INDEX_SAMPLE_BYTES: u64 = 64 * 1024 * 1024;
const EDITS: u64 = 1_000;
const LINE_CHANGE_EDITS: u64 = 5;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [command, path, size] if command == "generate-sparse" => {
            generate_sparse(Path::new(path), parse_size(size)?)?;
        }
        [command, path, size, kind] if command == "generate-pattern" => {
            generate_pattern(Path::new(path), parse_size(size)?, kind)?;
        }
        [command, path] if command == "probe" => probe(Path::new(path))?,
        [command, path] if command == "probe-line-changes" => probe_line_changes(Path::new(path))?,
        _ => {
            return Err(
                "usage: nulis-benchmarks generate-sparse <path> <bytes|MiB|GiB> | generate-pattern <path> <bytes|MiB|GiB> <long-line|short-lines|multilingual|malformed|binary> | probe <path> | probe-line-changes <path>"
                    .into(),
            );
        }
    }
    Ok(())
}

fn generate_pattern(path: &Path, len: u64, kind: &str) -> io::Result<()> {
    let pattern: &[u8] = match kind {
        "long-line" => b"Nulis long logical line with no terminator ",
        "short-lines" => b"a\n",
        "multilingual" => "Nulis 日本語 العربية 😀\n".as_bytes(),
        "malformed" => &[0xff, 0xfe, 0, 0xc3, 0x28, b'\n'],
        "binary" => b"",
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown corpus kind",
            ))
        }
    };
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    let mut buffer = vec![0_u8; VIEWPORT_BYTES];
    let mut offset = 0_u64;
    let mut random = 0x004E_554C_4953_u64;
    let mut pattern_position = 0_usize;
    while offset < len {
        let count = usize::try_from((len - offset).min(VIEWPORT_BYTES as u64))
            .expect("bounded by buffer length");
        if kind == "binary" {
            for byte in &mut buffer[..count] {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                *byte = u8::try_from(random & 255).expect("byte");
            }
        } else {
            for byte in &mut buffer[..count] {
                *byte = pattern[pattern_position];
                pattern_position += 1;
                if pattern_position == pattern.len() {
                    pattern_position = 0;
                }
            }
            if kind == "multilingual"
                && offset + u64::try_from(count).expect("count") == len
                && pattern_position > 0
            {
                if let Err(error) = std::str::from_utf8(&pattern[..pattern_position]) {
                    let incomplete = pattern_position - error.valid_up_to();
                    buffer[count - incomplete..count].fill(b' ');
                }
            }
        }
        file.write_all(&buffer[..count])?;
        offset += u64::try_from(count).expect("count");
    }
    file.sync_all()?;
    println!("generated_path={}", path.display());
    println!("logical_bytes={len}");
    println!("fixture_kind={kind}");
    Ok(())
}

fn generate_sparse(path: &Path, len: u64) -> io::Result<()> {
    if len < 64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "fixture must be at least 64 bytes",
        ));
    }
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)?;
    file.set_len(len)?;
    write_marker(&mut file, 0, b"NULIS-BEGIN\n")?;
    write_marker(&mut file, len / 2, b"NULIS-MIDDLE\n")?;
    write_marker(&mut file, len - 10, b"NULIS-END\n")?;
    file.sync_all()?;
    println!("generated_path={}", path.display());
    println!("logical_bytes={len}");
    println!("fixture_kind=sparse");
    Ok(())
}

fn write_marker(file: &mut fs::File, offset: u64, marker: &[u8]) -> io::Result<()> {
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(marker)
}

fn probe(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let source = Arc::new(FileSource::open(path)?);
    let open_elapsed = started.elapsed();
    let source_len = source.len();

    let viewport_len = usize::try_from(source_len.min(VIEWPORT_BYTES as u64))?;
    let mut viewport = vec![0_u8; viewport_len];
    let started = Instant::now();
    source.read_exact_at(0, &mut viewport)?;
    let viewport_elapsed = started.elapsed();

    let mut random_buffer = vec![0_u8; RANDOM_READ_BYTES];
    let mut random_state = 0x004E_554C_4953_u64;
    let mut checksum = 0_u64;
    let started = Instant::now();
    for _ in 0..RANDOM_READS {
        random_state = random_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let available_start = source_len.saturating_sub(RANDOM_READ_BYTES as u64);
        let offset = if available_start == 0 {
            0
        } else {
            random_state % available_start
        };
        let read = source.read_at(offset, &mut random_buffer)?;
        checksum = checksum.wrapping_add(
            random_buffer[..read]
                .iter()
                .map(|byte| u64::from(*byte))
                .sum::<u64>(),
        );
    }
    let random_elapsed = started.elapsed();

    let sample = PrefixSource {
        inner: Arc::clone(&source),
        len: source_len.min(INDEX_SAMPLE_BYTES),
    };
    let started = Instant::now();
    let index = SparseLineIndex::build(
        &sample,
        1_024,
        VIEWPORT_BYTES,
        &IndexCancellation::default(),
    )?;
    let index_elapsed = started.elapsed();

    let mut table = PieceTable::new(source, Box::new(MemoryAppendStore::new()));
    let edit_offset = table.len() / 2;
    let started = Instant::now();
    for _ in 0..EDITS {
        table.insert(edit_offset, b"x")?;
    }
    let edit_elapsed = started.elapsed();

    println!("nulis_benchmark_version=1");
    println!("source_bytes={source_len}");
    print_duration("open_metadata", open_elapsed);
    print_duration("first_viewport", viewport_elapsed);
    print_duration("random_reads_256x4k", random_elapsed);
    println!("random_read_checksum={checksum}");
    println!("index_sample_bytes={}", sample.len());
    println!("index_sample_lines={}", index.line_count());
    print_duration("index_sample", index_elapsed);
    print_duration("piece_table_edits_1000", edit_elapsed);
    println!("piece_count_after_edits={}", table.piece_count());
    Ok(())
}

/// Opens the source through the editor core, makes a few spread edits, and
/// measures the first viewport and map with line change markers attached.
fn probe_line_changes(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let sessions = tempfile::tempdir()?;
    let manager = DocumentManager::new(sessions.path().to_path_buf());
    let started = Instant::now();
    let document = manager.open_file(path)?;
    let open_elapsed = started.elapsed();
    let id = document.document_id.clone();
    let len = document.byte_len;

    let mut revision = document.revision;
    let started = Instant::now();
    for step in 0..LINE_CHANGE_EDITS {
        // Descending offsets keep every later edit position valid.
        let offset = len / LINE_CHANGE_EDITS * (LINE_CHANGE_EDITS - 1 - step);
        revision = manager
            .replace(&id, revision, offset, offset, "edited\n")?
            .revision;
    }
    let edit_elapsed = started.elapsed();

    let viewport = |start_byte: u64| ViewportRequest {
        document_id: id.clone(),
        start_byte,
        start_line: 1,
        line_continuation: false,
        max_bytes: VIEWPORT_BYTES as u64,
    };
    let started = Instant::now();
    let first = manager.viewport(&viewport(0))?;
    let first_elapsed = started.elapsed();
    let started = Instant::now();
    let deep = manager.viewport(&viewport(len / LINE_CHANGE_EDITS * (LINE_CHANGE_EDITS - 1)))?;
    let deep_elapsed = started.elapsed();
    let started = Instant::now();
    let map = manager.document_map(&id)?;
    let map_elapsed = started.elapsed();
    let marked = |lines: &[nulis_core::ViewportLine]| {
        lines.iter().filter(|line| line.change.is_some()).count()
    };

    println!("nulis_benchmark_version=1");
    println!("source_bytes={len}");
    println!("line_change_edits={LINE_CHANGE_EDITS}");
    print_duration("core_open", open_elapsed);
    print_duration("line_change_edits", edit_elapsed);
    print_duration("first_viewport_with_markers", first_elapsed);
    print_duration("deep_viewport_with_markers", deep_elapsed);
    print_duration("document_map_with_markers", map_elapsed);
    println!("first_viewport_marked_lines={}", marked(&first.lines));
    println!("deep_viewport_marked_lines={}", marked(&deep.lines));
    println!("map_change_ticks={}", map.changes.len());
    println!("line_changes_available={}", first.line_changes.available);
    println!(
        "line_changes_approximate={}",
        first.line_changes.approximate
    );
    Ok(())
}

fn print_duration(name: &str, duration: Duration) {
    println!("{name}_microseconds={}", duration.as_micros());
}

#[derive(Debug)]
struct PrefixSource {
    inner: Arc<FileSource>,
    len: u64,
}

impl ReadAt for PrefixSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        if offset >= self.len {
            return Ok(0);
        }
        let bounded = usize::try_from(
            (self.len - offset).min(u64::try_from(buffer.len()).expect("usize fits in u64")),
        )
        .expect("bounded by buffer length");
        self.inner.read_at(offset, &mut buffer[..bounded])
    }
}

fn parse_size(value: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let (number, multiplier) = if let Some(number) = value.strip_suffix("GiB") {
        (number, 1024_u64.pow(3))
    } else if let Some(number) = value.strip_suffix("MiB") {
        (number, 1024_u64.pow(2))
    } else {
        (value, 1)
    };
    Ok(number
        .parse::<u64>()?
        .checked_mul(multiplier)
        .ok_or("fixture size overflow")?)
}

#[cfg(test)]
mod tests {
    use super::{generate_pattern, parse_size};
    use std::fs;

    #[test]
    fn parses_binary_fixture_sizes() {
        assert_eq!(parse_size("10GiB").expect("size"), 10 * 1024_u64.pow(3));
        assert_eq!(parse_size("100MiB").expect("size"), 100 * 1024_u64.pow(2));
        assert_eq!(parse_size("4096").expect("size"), 4096);
    }

    #[test]
    fn pattern_corpus_is_deterministic_and_exactly_sized() {
        let root = tempfile::tempdir().expect("temporary root");
        for kind in [
            "long-line",
            "short-lines",
            "multilingual",
            "malformed",
            "binary",
        ] {
            let path = root.path().join(kind);
            generate_pattern(&path, 257, kind).expect("generate corpus");
            let bytes = fs::read(&path).expect("read corpus");
            assert_eq!(bytes.len(), 257);
            if kind == "short-lines" {
                assert_eq!(&bytes[..4], b"a\na\n");
            }
            if kind == "malformed" {
                assert_eq!(bytes[0], 0xff);
            }
            if kind == "multilingual" {
                assert!(std::str::from_utf8(&bytes).is_ok());
            }
        }
    }
}
