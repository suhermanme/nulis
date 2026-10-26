use crate::ReadAt;
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Default)]
pub struct IndexCancellation(AtomicBool);

impl IndexCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineCheckpoint {
    pub line: u64,
    pub byte_offset: u64,
}

/// Sparse line starts produced by an optional cancellable streaming pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparseLineIndex {
    stride: u64,
    line_count: u64,
    indexed_bytes: u64,
    complete: bool,
    checkpoints: Vec<LineCheckpoint>,
}

impl SparseLineIndex {
    /// Streams the source once using bounded memory. CRLF is one ending; lone
    /// CR and LF are also recognized.
    ///
    /// # Errors
    /// Returns an I/O error from the source or for a zero checkpoint stride.
    pub fn build(
        source: &dyn ReadAt,
        stride: u64,
        chunk_size: usize,
        cancellation: &IndexCancellation,
    ) -> io::Result<Self> {
        if stride == 0 || chunk_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "line index stride and chunk size must be non-zero",
            ));
        }
        let mut checkpoints = vec![LineCheckpoint {
            line: 0,
            byte_offset: 0,
        }];
        let scan = scan_from(
            source,
            stride,
            chunk_size,
            cancellation,
            0,
            0,
            &mut checkpoints,
        )?;
        Ok(Self {
            stride,
            line_count: scan.line.checked_add(1).ok_or_else(line_index_overflow)?,
            indexed_bytes: scan.offset,
            complete: scan.offset == source.len() && !cancellation.is_cancelled(),
            checkpoints,
        })
    }

    /// Invalidates the edited tail and rebuilds it from the nearest preceding
    /// checkpoint. The prefix before that checkpoint is retained.
    ///
    /// # Errors
    /// Returns an I/O error from the source or for a zero chunk size.
    pub fn refresh_from(
        &mut self,
        source: &dyn ReadAt,
        edit_offset: u64,
        chunk_size: usize,
        cancellation: &IndexCancellation,
    ) -> io::Result<()> {
        if chunk_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "line index chunk size must be non-zero",
            ));
        }
        let base = self.checkpoint_for_offset(edit_offset.min(self.indexed_bytes));
        self.checkpoints
            .retain(|checkpoint| checkpoint.line <= base.line);
        let scan = scan_from(
            source,
            self.stride,
            chunk_size,
            cancellation,
            base.byte_offset.min(source.len()),
            base.line,
            &mut self.checkpoints,
        )?;
        self.line_count = scan.line.checked_add(1).ok_or_else(line_index_overflow)?;
        self.indexed_bytes = scan.offset;
        self.complete = scan.offset == source.len() && !cancellation.is_cancelled();
        Ok(())
    }

    #[must_use]
    pub fn checkpoints(&self) -> &[LineCheckpoint] {
        &self.checkpoints
    }
    #[must_use]
    pub fn line_count(&self) -> u64 {
        self.line_count
    }
    #[must_use]
    pub fn indexed_bytes(&self) -> u64 {
        self.indexed_bytes
    }
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.complete
    }
    #[must_use]
    pub fn stride(&self) -> u64 {
        self.stride
    }

    /// Returns the nearest checkpoint at or before a line number.
    #[must_use]
    pub fn checkpoint_for_line(&self, line: u64) -> LineCheckpoint {
        let index = self
            .checkpoints
            .partition_point(|checkpoint| checkpoint.line <= line)
            .saturating_sub(1);
        self.checkpoints[index]
    }

    /// Returns the nearest checkpoint at or before a byte offset.
    #[must_use]
    pub fn checkpoint_for_offset(&self, byte_offset: u64) -> LineCheckpoint {
        let index = self
            .checkpoints
            .partition_point(|checkpoint| checkpoint.byte_offset <= byte_offset)
            .saturating_sub(1);
        self.checkpoints[index]
    }
}

#[derive(Clone, Copy, Debug)]
struct ScanResult {
    line: u64,
    offset: u64,
}

#[allow(clippy::too_many_arguments)]
fn scan_from(
    source: &dyn ReadAt,
    stride: u64,
    chunk_size: usize,
    cancellation: &IndexCancellation,
    start_offset: u64,
    start_line: u64,
    checkpoints: &mut Vec<LineCheckpoint>,
) -> io::Result<ScanResult> {
    let source_len = source.len();
    let mut buffer = vec![0_u8; chunk_size];
    let mut offset = start_offset;
    let mut line = start_line;
    let mut pending_cr = false;

    while offset < source_len && !cancellation.is_cancelled() {
        let read = source.read_at(offset, &mut buffer)?;
        if read == 0 {
            break;
        }
        for (index, byte) in buffer[..read].iter().copied().enumerate() {
            let relative = u64::try_from(index).map_err(|_| line_index_overflow())?;
            let absolute = offset
                .checked_add(relative)
                .ok_or_else(line_index_overflow)?;
            if pending_cr {
                pending_cr = false;
                if byte == b'\n' {
                    record_line(checkpoints, stride, &mut line, absolute + 1)?;
                    continue;
                }
                record_line(checkpoints, stride, &mut line, absolute)?;
            }
            match byte {
                b'\r' => pending_cr = true,
                b'\n' => record_line(checkpoints, stride, &mut line, absolute + 1)?,
                _ => {}
            }
        }
        offset = offset
            .checked_add(u64::try_from(read).map_err(|_| line_index_overflow())?)
            .ok_or_else(line_index_overflow)?;
    }
    if offset == source_len && pending_cr {
        record_line(checkpoints, stride, &mut line, offset)?;
    }
    Ok(ScanResult { line, offset })
}

fn record_line(
    checkpoints: &mut Vec<LineCheckpoint>,
    stride: u64,
    line: &mut u64,
    offset: u64,
) -> io::Result<()> {
    *line = line.checked_add(1).ok_or_else(line_index_overflow)?;
    if line.is_multiple_of(stride) {
        checkpoints.push(LineCheckpoint {
            line: *line,
            byte_offset: offset,
        });
    }
    Ok(())
}

fn line_index_overflow() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "line index offset overflow")
}

#[cfg(test)]
mod tests {
    use super::{IndexCancellation, SparseLineIndex};
    use crate::MemorySource;

    #[test]
    fn recognizes_line_endings_across_chunk_boundaries() {
        let source = MemorySource::new(b"a\r\nb\nc\rd".to_vec());
        let index =
            SparseLineIndex::build(&source, 1, 2, &IndexCancellation::default()).expect("index");
        assert_eq!(index.line_count(), 4);
        assert_eq!(
            index
                .checkpoints()
                .iter()
                .map(|item| item.byte_offset)
                .collect::<Vec<_>>(),
            vec![0, 3, 5, 7]
        );
        assert_eq!(index.checkpoint_for_line(2).byte_offset, 5);
        assert_eq!(index.checkpoint_for_offset(6).line, 2);
        assert!(index.is_complete());
    }

    #[test]
    fn cancellation_returns_an_explicit_partial_index() {
        let cancellation = IndexCancellation::default();
        cancellation.cancel();
        let index =
            SparseLineIndex::build(&MemorySource::new(vec![b'x'; 100]), 10, 8, &cancellation)
                .expect("index");
        assert!(!index.is_complete());
        assert_eq!(index.indexed_bytes(), 0);
    }

    #[test]
    fn refresh_reuses_the_stable_prefix_and_matches_a_full_rebuild() {
        let original = MemorySource::new(b"a\nb\nc\nd\ne\n".to_vec());
        let mut refreshed =
            SparseLineIndex::build(&original, 2, 3, &IndexCancellation::default()).expect("index");
        let edited = MemorySource::new(b"a\nb\nchanged\nd\ne\n".to_vec());

        refreshed
            .refresh_from(&edited, 5, 2, &IndexCancellation::default())
            .expect("refresh");
        let rebuilt = SparseLineIndex::build(&edited, 2, 2, &IndexCancellation::default())
            .expect("full rebuild");

        assert_eq!(refreshed, rebuilt);
        assert_eq!(refreshed.checkpoints()[1].byte_offset, 4);
    }
}
