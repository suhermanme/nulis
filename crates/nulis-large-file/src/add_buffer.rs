use crate::ReadAt;
use std::{
    fs::{File, OpenOptions},
    io::{self, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

/// Append-only edit storage addressed by stable byte offsets.
pub trait AppendStore: ReadAt {
    /// Appends bytes and returns their starting store offset.
    ///
    /// # Errors
    /// Returns an I/O error when the bytes cannot be appended.
    fn append(&mut self, bytes: &[u8]) -> io::Result<u64>;

    /// Makes buffered edits durable where supported.
    ///
    /// # Errors
    /// Returns an I/O error when durability cannot be established.
    fn sync(&mut self) -> io::Result<()>;
}

#[derive(Debug, Default)]
pub struct MemoryAppendStore {
    bytes: Vec<u8>,
}

impl MemoryAppendStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ReadAt for MemoryAppendStore {
    fn len(&self) -> u64 {
        u64::try_from(self.bytes.len()).expect("usize always fits into u64")
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        let start = usize::try_from(offset)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "offset exceeds platform"))?;
        if start >= self.bytes.len() {
            return Ok(0);
        }
        let count = (self.bytes.len() - start).min(buffer.len());
        buffer[..count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(count)
    }
}

impl AppendStore for MemoryAppendStore {
    fn append(&mut self, bytes: &[u8]) -> io::Result<u64> {
        let start = self.len();
        self.bytes.extend_from_slice(bytes);
        Ok(start)
    }

    fn sync(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A persistent append-only edit buffer. Its path is owned by the caller so a
/// session recovery service can retain it across crashes.
#[derive(Debug)]
pub struct DiskAppendStore {
    path: PathBuf,
    file: File,
    len: u64,
}

impl DiskAppendStore {
    /// Creates a new append store without overwriting an existing path.
    ///
    /// # Errors
    /// Returns an I/O error when secure creation fails.
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            len: 0,
        })
    }

    /// Reopens an existing append store for crash/session recovery.
    ///
    /// # Errors
    /// Returns an I/O error when the file cannot be opened or inspected.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let len = file.metadata()?.len();
        Ok(Self {
            path: path.to_path_buf(),
            file,
            len,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ReadAt for DiskAppendStore {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        if offset >= self.len || buffer.is_empty() {
            return Ok(0);
        }
        platform_read_at(&self.file, offset, buffer)
    }
}

impl AppendStore for DiskAppendStore {
    fn append(&mut self, bytes: &[u8]) -> io::Result<u64> {
        let start = self.len;
        self.file.seek(SeekFrom::Start(self.len))?;
        self.file.write_all(bytes)?;
        self.len = self
            .len
            .checked_add(u64::try_from(bytes.len()).expect("usize always fits into u64"))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "append store overflow"))?;
        Ok(start)
    }

    fn sync(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.file.sync_data()
    }
}

#[cfg(unix)]
fn platform_read_at(file: &File, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buffer, offset)
}

#[cfg(windows)]
fn platform_read_at(file: &File, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buffer, offset)
}

#[cfg(test)]
mod tests {
    use super::{AppendStore, DiskAppendStore};
    use crate::ReadAt;
    use std::fs;

    #[test]
    fn disk_store_appends_without_rewriting_previous_bytes() {
        let path = std::env::temp_dir().join(format!("nulis-add-store-{}.bin", std::process::id()));
        let _ = fs::remove_file(&path);
        let mut store = DiskAppendStore::create(&path).expect("create store");
        assert_eq!(store.append(b"abc").expect("first append"), 0);
        assert_eq!(store.append(b"DEF").expect("second append"), 3);
        store.sync().expect("sync");
        let mut bytes = [0; 4];
        store.read_exact_at(1, &mut bytes).expect("read");
        assert_eq!(&bytes, b"bcDE");
        drop(store);
        assert_eq!(fs::read(&path).expect("stored bytes"), b"abcDEF");
        let mut reopened = DiskAppendStore::open(&path).expect("reopen store");
        assert_eq!(reopened.append(b"!").expect("append after reopen"), 6);
        reopened.sync().expect("sync reopened store");
        drop(reopened);
        assert_eq!(fs::read(&path).expect("reopened bytes"), b"abcDEF!");
        fs::remove_file(path).expect("cleanup");
    }
}
