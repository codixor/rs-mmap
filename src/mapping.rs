use crate::{MmapError, ReadOnlyMmap, Result};
use std::ops::{Deref, DerefMut};
#[cfg(any(unix, windows))]
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    sync::Mutex,
};

/// Immutable storage shared with read-only views.
pub struct ReadOnly(pub(crate) ReadOnlyMmap);
/// Writable snapshot, anonymous memory, or an imported native map.
pub struct ReadWrite {
    #[cfg(any(unix, windows))]
    bytes: memmap2::MmapMut,
    #[cfg(any(unix, windows))]
    destination: Destination,
    #[cfg(not(any(unix, windows)))]
    bytes: Vec<u8>,
}
#[cfg(any(unix, windows))]
enum Destination {
    Private,
    Native,
    File { handle: Mutex<File>, offset: u64 },
}
/// Private writable snapshot. Its changes are never written back.
pub struct CopyOnWrite(pub(crate) ReadWrite);
/// A region whose mode determines which operations are available.
pub struct Mmap<Mode = ReadOnly>(pub(crate) Mode);

impl AsRef<[u8]> for ReadOnly {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl AsRef<[u8]> for ReadWrite {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}
impl AsRef<[u8]> for CopyOnWrite {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
impl<Mode: AsRef<[u8]>> Mmap<Mode> {
    pub fn len(&self) -> usize {
        self.0.as_ref().len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.as_ref().is_empty()
    }
    pub fn as_ptr(&self) -> *const u8 {
        self.0.as_ref().as_ptr()
    }
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_ref()
    }
}
impl<Mode: AsRef<[u8]>> Deref for Mmap<Mode> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}
impl Mmap<ReadOnly> {
    pub fn into_read_only(self) -> ReadOnlyMmap {
        self.0.0
    }
}
impl Mmap<ReadWrite> {
    /// Allocates a zeroed, private anonymous mapping.
    pub fn anonymous(len: usize) -> Result<Self> {
        crate::MmapOptions::new_anonymous(len).map_readwrite()
    }
    #[cfg(any(unix, windows))]
    pub(crate) fn from_anonymous(bytes: memmap2::MmapMut) -> Self {
        Self(ReadWrite {
            bytes,
            destination: Destination::Private,
        })
    }
    #[cfg(any(unix, windows))]
    pub(crate) fn snapshot(
        mut file: File,
        offset: u64,
        length: Option<usize>,
        write_back: bool,
        limit: usize,
    ) -> Result<Self> {
        let len = crate::read_only_file::range_len(file.metadata()?.len(), offset, length)?;
        if len > limit {
            return Err(MmapError::SnapshotTooLarge { len, limit });
        }
        let mut bytes = memmap2::MmapMut::map_anon(len)?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut bytes)?;
        Ok(Self(ReadWrite {
            bytes,
            destination: if write_back {
                Destination::File {
                    handle: Mutex::new(file),
                    offset,
                }
            } else {
                Destination::Private
            },
        }))
    }
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0.bytes
    }

    /// Persists the complete region. Dropping a writable file snapshot discards
    /// unflushed edits. Coordinate concurrent writers at the application level.
    pub fn flush(&self) -> Result<()> {
        self.flush_range(0, self.len())
    }

    /// Persists a byte range relative to this region. File snapshots write only
    /// those bytes and sync the file. Imported native maps use memmap2's flush.
    pub fn flush_range(&self, offset: usize, len: usize) -> Result<()> {
        let end = offset.checked_add(len).filter(|&n| n <= self.len()).ok_or(
            MmapError::SliceOutOfBounds {
                start: offset,
                end: offset.saturating_add(len),
                len: self.len(),
            },
        )?;
        if len == 0 {
            return Ok(());
        }
        #[cfg(any(unix, windows))]
        {
            match &self.0.destination {
                Destination::File {
                    handle: file,
                    offset: base,
                } => {
                    let mut file = file
                        .lock()
                        .map_err(|_| std::io::Error::other("file lock poisoned"))?;
                    let start = *base + offset as u64;
                    crate::read_only_file::range_len(file.metadata()?.len(), start, Some(len))?;
                    file.seek(SeekFrom::Start(start))?;
                    file.write_all(&self[offset..end])?;
                    // The length is unchanged, so data sync makes the bytes durable.
                    file.sync_data()?;
                }
                Destination::Native => self.0.bytes.flush_range(offset, len)?,
                Destination::Private => (),
            }
            Ok(())
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = end;
            Err(MmapError::UnsupportedPlatform)
        }
    }
}
#[cfg(all(feature = "zerocopy-views", any(unix, windows)))]
impl Mmap<ReadWrite> {
    /// Replaces the file at `path` with the complete region through a synced
    /// temporary file and a rename, then writes back to the new file.
    pub(crate) fn replace_file(&self, path: &std::path::Path) -> Result<()> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let Destination::File { handle, offset: 0 } = &self.0.destination else {
            return Err(MmapError::UnsupportedPlatform);
        };
        let mut current = handle
            .lock()
            .map_err(|_| std::io::Error::other("file lock poisoned"))?;
        let name = path.file_name().ok_or_else(|| {
            MmapError::InvalidConfiguration("record path has no file name".into())
        })?;
        let directory = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => std::path::Path::new("."),
        };
        let (temporary, mut file) = loop {
            let mut candidate = std::ffi::OsString::from(".");
            candidate.push(name);
            candidate.push(format!(
                ".{}.{}.tmp",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let candidate = directory.join(candidate);
            match File::options()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => break (candidate, file),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        };
        let written = (|| {
            file.set_permissions(current.metadata()?.permissions())?;
            file.write_all(self)?;
            file.sync_all()?;
            std::fs::rename(&temporary, path)
        })();
        if let Err(error) = written {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.into());
        }
        // The replacement is in place. Later flushes must reach the new file.
        *current = file;
        #[cfg(unix)]
        File::open(directory)?.sync_all()?;
        Ok(())
    }
}
impl DerefMut for Mmap<ReadWrite> {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}
impl DerefMut for Mmap<CopyOnWrite> {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.0.0.bytes
    }
}
#[cfg(any(unix, windows))]
impl From<memmap2::MmapMut> for Mmap<ReadWrite> {
    fn from(bytes: memmap2::MmapMut) -> Self {
        Self(ReadWrite {
            bytes,
            destination: Destination::Native,
        })
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use crate::MmapOptions;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_file(size: usize, pattern: u8) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![pattern; size];
        file.write_all(&data).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_mmap_len() {
        let file = create_test_file(4096, 0x42);

        let mmap = MmapOptions::new().path(file.path()).map_readonly().unwrap();

        assert_eq!(mmap.len(), 4096);
        assert!(!mmap.is_empty());
    }

    #[test]
    fn test_readonly_as_slice() {
        let file = create_test_file(1024, 0x55);

        let mmap = MmapOptions::new().path(file.path()).map_readonly().unwrap();

        let slice = mmap.as_slice();
        assert_eq!(slice.len(), 1024);
        assert_eq!(slice[0], 0x55);
        assert_eq!(slice[512], 0x55);
        assert_eq!(slice[1023], 0x55);
    }

    #[test]
    fn test_readonly_deref() {
        let file = create_test_file(2048, 0xAA);

        let mmap = MmapOptions::new().path(file.path()).map_readonly().unwrap();

        // Test Deref implementation
        assert_eq!(mmap[0], 0xAA);
        assert_eq!(mmap[1024], 0xAA);
        assert_eq!(mmap.len(), 2048);
    }

    #[test]
    fn test_readwrite_as_mut_slice() {
        let file = create_test_file(4096, 0x00);

        let mut mmap = MmapOptions::new()
            .path(file.path())
            .map_readwrite()
            .unwrap();

        let slice = mmap.as_mut_slice();
        slice[0] = 0xFF;
        slice[100] = 0xEE;
        slice[4095] = 0xDD;

        assert_eq!(slice[0], 0xFF);
        assert_eq!(slice[100], 0xEE);
        assert_eq!(slice[4095], 0xDD);
    }

    #[test]
    fn test_readwrite_deref_mut() {
        let file = create_test_file(1024, 0x00);

        let mut mmap = MmapOptions::new()
            .path(file.path())
            .map_readwrite()
            .unwrap();

        // Test DerefMut implementation
        mmap[0] = 0x11;
        mmap[512] = 0x22;

        assert_eq!(mmap[0], 0x11);
        assert_eq!(mmap[512], 0x22);
    }

    #[test]
    fn test_readwrite_immutable_access() {
        let file = create_test_file(2048, 0xBB);

        let mmap = MmapOptions::new()
            .path(file.path())
            .map_readwrite()
            .unwrap();

        // ReadWrite should also allow immutable access via Deref
        assert_eq!(mmap[0], 0xBB);
        assert_eq!(mmap[1024], 0xBB);
    }

    #[test]
    fn test_as_ptr() {
        let file = create_test_file(512, 0x33);

        let mmap = MmapOptions::new().path(file.path()).map_readonly().unwrap();

        let ptr = mmap.as_ptr();
        assert!(!ptr.is_null());

        assert_eq!(ptr, mmap.as_slice().as_ptr());
        assert_eq!(mmap[0], 0x33);
    }

    #[test]
    fn test_multiple_reads() {
        let file = create_test_file(8192, 0x77);

        let mmap = MmapOptions::new().path(file.path()).map_readonly().unwrap();

        // Multiple immutable borrows should work
        let slice1 = mmap.as_slice();
        let slice2 = mmap.as_slice();

        assert_eq!(slice1[0], 0x77);
        assert_eq!(slice2[0], 0x77);
    }

    #[test]
    fn test_zero_sized_mapping() {
        let file = create_test_file(0, 0x00);

        let result = MmapOptions::new().path(file.path()).map_readonly();

        // Zero-sized mappings should error (platform limitation)
        assert!(result.is_err());
    }

    #[test]
    fn test_large_mapping() {
        let size = 1024 * 1024; // 1MB
        let file = create_test_file(size, 0x99);

        let mmap = MmapOptions::new().path(file.path()).map_readonly().unwrap();

        assert_eq!(mmap.len(), size);
        assert_eq!(mmap[0], 0x99);
        assert_eq!(mmap[size / 2], 0x99);
        assert_eq!(mmap[size - 1], 0x99);
    }

    #[test]
    fn test_persistence_after_write() {
        let file = create_test_file(4096, 0x00);
        let path = file.path().to_path_buf();

        {
            let mut mmap = MmapOptions::new().path(&path).map_readwrite().unwrap();

            mmap[0] = 0xAA;
            mmap[100] = 0xBB;
            mmap[4095] = 0xCC;
            mmap.flush().unwrap();
        }

        // Read back to verify persistence

        let mmap = MmapOptions::new().path(&path).map_readonly().unwrap();

        assert_eq!(mmap[0], 0xAA);
        assert_eq!(mmap[100], 0xBB);
        assert_eq!(mmap[4095], 0xCC);
    }
}
