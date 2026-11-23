use crate::{MmapError, Result};
use std::{fs::File, ops::Deref};

/// An open source for independently owned read-only mapping windows.
///
/// Windows reuses one section across all views. Unix maps each range with
/// memmap2. Each view can outlive the source and is unmapped on drop.
pub struct ReadOnlyFile {
    len: u64,
    #[cfg(unix)]
    file: File,
    #[cfg(windows)]
    section: std::os::windows::io::OwnedHandle,
    #[cfg(windows)]
    file: std::sync::Arc<File>,
}

/// An independently owned read-only mapping of a nonempty file range.
pub struct FileView {
    #[cfg(unix)]
    inner: memmap2::Mmap,
    #[cfg(windows)]
    inner: crate::Mmap<crate::ReadOnly>,
}

impl ReadOnlyFile {
    /// Prepares a nonempty file for repeated range mappings.
    ///
    /// # Safety
    ///
    /// The caller must prevent modification or truncation of the file until
    /// the source and every view created from it have been dropped.
    pub unsafe fn new(file: File) -> Result<Self> {
        let len = file.metadata()?.len();
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }
        #[cfg(windows)]
        let section = crate::platform::PlatformMmap::section(&file, crate::Protection::READ)?;
        Ok(Self {
            len,
            #[cfg(unix)]
            file,
            #[cfg(windows)]
            section,
            #[cfg(windows)]
            file: std::sync::Arc::new(file),
        })
    }

    /// Takes ownership of an existing Windows file-mapping object and a
    /// handle to its backing file. Views use read-only access and keep the
    /// file alive, including files opened with delete-on-close.
    ///
    /// `len` is the accessible extent from the start of the section. A
    /// section may cover only a prefix of the file. Empty extents and
    /// extents larger than the backing file are rejected.
    ///
    /// # Safety
    ///
    /// `section` must be a readable data-file mapping created from `file`,
    /// covering at least `len` bytes. Prevent writes through any handle or
    /// mapped view and prevent truncation until the source and all its views
    /// have been dropped.
    #[cfg(windows)]
    pub unsafe fn from_section(
        section: std::os::windows::io::OwnedHandle,
        file: File,
        len: u64,
    ) -> Result<Self> {
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }
        if len > file.metadata()?.len() {
            return Err(MmapError::InvalidConfiguration(
                "section extent exceeds the backing file length".into(),
            ));
        }
        Ok(Self {
            len,
            section,
            file: std::sync::Arc::new(file),
        })
    }

    /// Reads up to the last `max_len` bytes and passes them to `inspect`.
    /// A short file yields its entire contents. A zero limit yields an empty
    /// slice. The file cursor is preserved.
    ///
    /// On Linux, suffixes up to 64 KiB use a positional read into a temporary
    /// buffer. Larger suffixes and other platforms use a read-only view.
    /// The slice lives for the duration of the callback.
    ///
    /// ```
    /// # #[cfg(any(unix, windows))] {
    /// use rs_mmap::ReadOnlyFile;
    /// use std::io::Write;
    /// let mut file = tempfile::tempfile()?;
    /// file.write_all(b"first\nlast\n")?;
    /// // SAFETY: this example owns the file and has finished writing.
    /// let log = unsafe { ReadOnlyFile::new(file)? };
    /// assert!(log.with_suffix(5, |bytes| bytes == b"last\n")?);
    /// # }
    /// # Ok::<(), rs_mmap::MmapError>(())
    /// ```
    pub fn with_suffix<T>(&self, max_len: usize, inspect: impl FnOnce(&[u8]) -> T) -> Result<T> {
        let len = self.len.min(max_len as u64);
        if len == 0 {
            return Ok(inspect(&[]));
        }
        let offset = self.len - len;
        // The minimum above is bounded by max_len, including on 32-bit targets.
        let len = len as usize;
        #[cfg(target_os = "linux")]
        if len <= 64 * 1024 {
            use std::os::unix::fs::FileExt;
            let mut bytes = vec![0; len];
            self.file.read_exact_at(&mut bytes, offset)?;
            return Ok(inspect(&bytes));
        }
        Ok(inspect(&self.view(offset, len)?))
    }

    /// Maps a checked range. Offsets need not be page aligned.
    pub fn view(&self, offset: u64, len: usize) -> Result<FileView> {
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }
        if len > isize::MAX as usize
            || offset
                .checked_add(len as u64)
                .is_none_or(|end| end > self.len)
        {
            return Err(MmapError::InvalidRange {
                offset,
                len,
                file_len: self.len,
            });
        }
        #[cfg(unix)]
        // SAFETY: new's file stability contract extends to all derived views.
        let inner = unsafe {
            memmap2::MmapOptions::new()
                .offset(offset)
                .len(len)
                .map(&self.file)?
        };
        #[cfg(windows)]
        let inner = crate::Mmap::from_platform(crate::platform::PlatformMmap::from_section(
            &self.section,
            self.file.clone(),
            self.len,
            offset,
            len,
            crate::Protection::READ,
        )?);
        Ok(FileView { inner })
    }
}

#[cfg(unix)]
impl FileView {
    /// Hints how this part of a read-only view will be accessed. The range
    /// is relative to the view, even when its file offset is unaligned.
    /// The kernel applies advice to whole pages intersecting the range.
    ///
    /// An empty in-bounds range succeeds without a system call. Invalid
    /// ranges return [`MmapError::SliceOutOfBounds`]. If the kernel rejects
    /// a hint, the original OS error is returned in [`MmapError::Io`].
    pub fn advise_range(
        &self,
        range: std::ops::Range<usize>,
        advice: crate::MemoryAdvice,
    ) -> Result<()> {
        let bytes = self.get(range.clone()).ok_or(MmapError::SliceOutOfBounds {
            len: self.len(),
            start: range.start,
            end: range.end,
        })?;
        if bytes.is_empty() {
            return Ok(());
        }
        let ptr = bytes.as_ptr();
        let prefix = ptr.addr() % crate::platform::page_size();
        let start = ptr.wrapping_sub(prefix).cast_mut().cast();
        // A slice is limited to isize::MAX bytes; prefix is less than one page.
        let len = prefix + bytes.len();
        // SAFETY: the range is inside this live, immutable file mapping. Its
        // first page also owns the alignment prefix. The final byte is unchanged.
        if unsafe { libc::madvise(start, len, advice.to_unix_advice()) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}

impl Deref for FileView {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.inner
    }
}

impl AsRef<[u8]> for FileView {
    fn as_ref(&self) -> &[u8] {
        self
    }
}
