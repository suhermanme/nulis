use crate::ReadAt;
use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

/// An immutable file handle that supports positional reads without changing a
/// shared cursor. Opening captures length from metadata and reads no file data.
#[derive(Debug)]
pub struct FileSource {
    path: PathBuf,
    file: File,
    len: u64,
}

impl FileSource {
    /// Opens an existing file as a random-access source.
    ///
    /// # Errors
    /// Returns an I/O error when the file or its metadata cannot be opened.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let file = OpenOptions::new().read(true).open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "source is not a regular file",
            ));
        }
        Ok(Self {
            path: path.to_path_buf(),
            file,
            len: metadata.len(),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ReadAt for FileSource {
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

#[cfg(not(any(unix, windows)))]
compile_error!("Nulis file sources currently require a Unix or Windows target");

#[cfg(test)]
mod tests {
    use super::FileSource;
    use crate::ReadAt;
    use std::fs;
    #[cfg(unix)]
    use std::{
        fs::OpenOptions,
        io::{Seek, SeekFrom, Write},
    };

    #[cfg(unix)]
    #[test]
    fn sparse_multi_gibibyte_file_opens_and_reads_bounded_ranges() {
        let path =
            std::env::temp_dir().join(format!("nulis-sparse-source-{}.bin", std::process::id()));
        let _ = fs::remove_file(&path);
        let boundary = 5 * 1024_u64 * 1024 * 1024;
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .expect("create sparse fixture");
        file.set_len(boundary + 16).expect("set sparse length");
        file.seek(SeekFrom::Start(boundary + 7)).expect("seek");
        file.write_all(b"NULIS").expect("write marker");
        file.sync_all().expect("sync fixture");
        drop(file);

        let source = FileSource::open(&path).expect("open source");
        assert_eq!(source.len(), boundary + 16);
        let mut marker = [0; 5];
        source
            .read_exact_at(boundary + 7, &mut marker)
            .expect("read marker");
        assert_eq!(&marker, b"NULIS");
        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn file_source_uses_positional_reads() {
        let path =
            std::env::temp_dir().join(format!("nulis-file-source-{}.bin", std::process::id()));
        let _ = fs::remove_file(&path);
        fs::write(&path, b"0123456789").expect("fixture");
        let source = FileSource::open(&path).expect("open source");
        let mut first = [0; 3];
        let mut second = [0; 2];
        source.read_exact_at(7, &mut first).expect("tail");
        source.read_exact_at(1, &mut second).expect("head");
        assert_eq!(&first, b"789");
        assert_eq!(&second, b"12");
        fs::remove_file(path).expect("cleanup");
    }
}
