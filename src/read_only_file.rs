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
