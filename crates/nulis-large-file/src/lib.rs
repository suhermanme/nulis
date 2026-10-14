//! Bounded-memory storage primitives for large sources.

mod add_buffer;
mod line_index;
mod source;

pub use add_buffer::{AppendStore, DiskAppendStore, MemoryAppendStore};
pub use line_index::{IndexCancellation, LineCheckpoint, SparseLineIndex};
pub use source::FileSource;

use std::io;

/// Random-access immutable bytes without requiring source materialization.
pub trait ReadAt: Send + Sync {
    /// Total source length in bytes.
    fn len(&self) -> u64;

    /// Whether the source has no bytes.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Reads at most `buffer.len()` bytes beginning at `offset`.
    ///
    /// A short read is valid at end of source. Implementations must return
    /// `InvalidInput` when the platform cannot represent a requested offset.
    ///
    /// # Errors
    /// Returns an I/O error when the source cannot serve the requested range.
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize>;

    /// Fills a buffer or reports an unexpected end of source.
    ///
    /// # Errors
    /// Returns an I/O error from the source or [`io::ErrorKind::UnexpectedEof`].
    fn read_exact_at(&self, mut offset: u64, mut buffer: &mut [u8]) -> io::Result<()> {
        while !buffer.is_empty() {
            let read = self.read_at(offset, buffer)?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "source ended before the requested range",
                ));
            }
            offset = offset
                .checked_add(u64::try_from(read).expect("usize always fits into u64"))
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "offset overflow"))?;
            buffer = &mut buffer[read..];
        }
        Ok(())
    }
}

/// A simple in-memory source used for tests and small, newly created documents.
#[derive(Clone, Debug, Default)]
pub struct MemorySource {
    bytes: Vec<u8>,
}

impl MemorySource {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

impl ReadAt for MemorySource {
    fn len(&self) -> u64 {
        u64::try_from(self.bytes.len()).expect("usize always fits into u64")
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        let start = usize::try_from(offset)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "offset exceeds platform"))?;
        if start >= self.bytes.len() {
            return Ok(0);
        }
        let available = self.bytes.len() - start;
        let count = available.min(buffer.len());
        buffer[..count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::{MemorySource, ReadAt};

    #[test]
    fn bounded_reads_do_not_materialize_the_source() {
        let source = MemorySource::new(b"0123456789".to_vec());
        let mut buffer = [0; 3];
        assert_eq!(source.read_at(7, &mut buffer).expect("read"), 3);
        assert_eq!(&buffer, b"789");
    }
}
