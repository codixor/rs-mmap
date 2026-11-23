use crate::{MemoryAdvice, MmapError, ReadOnlyMmap, Result};
use std::{
    fs::File,
    io,
    ops::{Deref, Range},
};

/// An open source for independent windows. Its initial length bounds every request.
/// Ordinary files are read into owned snapshots. A concurrent write can affect
/// bytes being read, but later writes cannot change an existing view.
pub struct ReadOnlyFile {
    file: File,
    len: u64,
    #[cfg(windows)]
    cursor: std::sync::Mutex<()>,
}

/// A window which can outlive the source and the backing file.
pub struct FileView {
    bytes: ReadOnlyMmap,
    file_mapped: bool,
}

pub(crate) fn range_len(file_len: u64, offset: u64, length: Option<usize>) -> Result<usize> {
    if length == Some(0) {
        return Err(MmapError::EmptyMapping);
    }
    let invalid = || MmapError::InvalidRange {
        offset,
        len: length.unwrap_or(usize::MAX),
        file_len,
    };
    let remaining = file_len.checked_sub(offset).ok_or_else(invalid)?;
    let len = match length {
        Some(n) => n,
        None => usize::try_from(remaining).map_err(|_| invalid())?,
    };
    if len as u64 > remaining || len > isize::MAX as usize {
        return Err(invalid());
    }
    if len == 0 {
        return Err(MmapError::EmptyMapping);
    }
    Ok(len)
}

impl ReadOnlyFile {
    /// Captures the current length. The handle must support reading and seeking.
    /// On Windows, use a separate handle for concurrent cursor-based I/O.
    pub fn new(file: File) -> Result<Self> {
        let len = file.metadata()?.len();
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }
        Ok(Self {
            file,
            len,
            #[cfg(windows)]
            cursor: std::sync::Mutex::new(()),
        })
    }

    pub(crate) fn window(&self, offset: u64, length: Option<usize>) -> Result<FileView> {
        let len = range_len(self.len, offset, length)?;
        match memmap2::MmapOptions::new()
            .offset(offset)
            .len(len)
            .map_if_safe(&self.file)
        {
            Ok(map) => {
                return Ok(FileView {
                    bytes: map.into(),
                    file_mapped: true,
                });
            }
            Err(e) if e.not_safe() || e.not_supported() => {}
            Err(e) => return Err(io::Error::from(e).into()),
        }
        let mut snapshot = memmap2::MmapMut::map_anon(len)?;
        #[cfg(unix)]
        std::os::unix::fs::FileExt::read_exact_at(&self.file, &mut snapshot, offset)?;
        #[cfg(windows)]
        {
            use std::io::{Read, Seek, SeekFrom};
            let _lock = self
                .cursor
                .lock()
                .map_err(|_| io::Error::other("file cursor lock poisoned"))?;
            let mut file = &self.file;
            let previous = file.stream_position()?;
            file.seek(SeekFrom::Start(offset))?;
            let read = file.read_exact(&mut snapshot);
            let restore = file.seek(SeekFrom::Start(previous));
            read?;
            restore?;
        }
        Ok(FileView {
            bytes: snapshot.make_read_only()?.into(),
            file_mapped: false,
        })
    }

    /// Returns exactly the requested bytes or an error on a short read.
    pub fn view(&self, offset: u64, len: usize) -> Result<FileView> {
        self.window(offset, Some(len))
    }

    /// Reads at most `max_len` bytes ending at the original end of file.
    pub fn with_suffix<T>(&self, max_len: usize, inspect: impl FnOnce(&[u8]) -> T) -> Result<T> {
        let len = self.len.min(max_len as u64) as usize;
        if len == 0 {
            return Ok(inspect(&[]));
        }
        Ok(inspect(&self.view(self.len - len as u64, len)?))
    }
}

impl FileView {
    /// True when memmap2 verified immutability and mapped the original file.
    pub fn is_file_mapped(&self) -> bool {
        self.file_mapped
    }

    /// Shares the same bytes without copying.
    pub fn into_read_only(self) -> ReadOnlyMmap {
        self.bytes
    }

    /// Passes a supported access hint to the mapping containing these bytes.
    /// Destructive hints and platforms without safe advice return an error.
    pub fn advise_range(&self, range: Range<usize>, advice: MemoryAdvice) -> Result<()> {
        let part = self.get(range.clone()).ok_or(MmapError::SliceOutOfBounds {
            len: self.len(),
            start: range.start,
            end: range.end,
        })?;
        if part.is_empty() || advice == MemoryAdvice::Normal {
            return Ok(());
        }
        #[cfg(unix)]
        if let crate::Backing::Memmap2(map) = &self.bytes.backing {
            let hint = match advice {
                MemoryAdvice::Sequential => memmap2::Advice::Sequential,
                MemoryAdvice::Random => memmap2::Advice::Random,
                MemoryAdvice::WillNeed => memmap2::Advice::WillNeed,
                _ => return Err(MmapError::UnsupportedPlatform),
            };
            return Ok(map.advise_range(hint, range.start, part.len())?);
        }
        Err(MmapError::UnsupportedPlatform)
    }
}
impl Deref for FileView {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}
impl AsRef<[u8]> for FileView {
    fn as_ref(&self) -> &[u8] {
        self
    }
}
