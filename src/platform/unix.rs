use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::ptr;

use crate::huge_pages::HugePageSize;
use crate::protection::Protection;
use crate::{MmapError, Result};

/// Mapping mode flags.
#[cfg_attr(
    feature = "serde-support",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingMode {
    /// Shared mapping, visible to other processes and backed by the file.
    Shared,
    /// Private copy-on-write mapping.
    Private,
}

/// Platform-specific mapping handle.
pub struct PlatformMmap {
    map_ptr: *mut u8,
    ptr: *mut u8,
    map_len: usize,
    len: usize,
}

impl PlatformMmap {
    pub fn new(
        file: &File,
        offset: u64,
        len: usize,
        protection: Protection,
        mode: MappingMode,
    ) -> Result<Self> {
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }

        let file_size = file.metadata()?.len();
        let (aligned_offset, delta, map_len) = Self::mapping_window(offset, len, file_size)?;
        let prot = Self::protection_to_unix(protection);
        let flags = match mode {
            MappingMode::Shared => libc::MAP_SHARED,
            MappingMode::Private => libc::MAP_PRIVATE,
        };

        // SAFETY: `mmap` is called with a validated file descriptor, aligned
        // file offset, non-zero length, and OS-provided protection/flag bits.
        let raw = unsafe {
            libc::mmap(
                ptr::null_mut(),
                map_len,
                prot,
                flags,
                file.as_raw_fd(),
                aligned_offset as libc::off_t,
            )
        };

        if raw == libc::MAP_FAILED {
            return Err(MmapError::MappingFailed(format!(
                "mmap failed: {}",
                std::io::Error::last_os_error()
            )));
        }

        let map_ptr = raw.cast::<u8>();
        // SAFETY: `delta` is strictly within the mapped region returned above.
        let ptr = unsafe { map_ptr.add(delta) };

        Ok(Self {
            map_ptr,
            ptr,
            map_len,
            len,
        })
    }

    pub fn new_anonymous(len: usize, protection: Protection) -> Result<Self> {
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }

        let prot = Self::protection_to_unix(protection);
        let flags = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS;

        // SAFETY: Anonymous mapping with validated non-zero length.
        let raw = unsafe { libc::mmap(ptr::null_mut(), len, prot, flags, -1, 0) };
        if raw == libc::MAP_FAILED {
            return Err(MmapError::MappingFailed(format!(
                "anonymous mmap failed: {}",
                std::io::Error::last_os_error()
            )));
        }

        let map_ptr = raw.cast::<u8>();
        Ok(Self {
            map_ptr,
            ptr: map_ptr,
            map_len: len,
            len,
        })
    }

    #[cfg(target_os = "linux")]
    pub fn new_huge(
        file: &File,
        offset: u64,
        len: usize,
        protection: Protection,
        huge_page_size: HugePageSize,
    ) -> Result<Self> {
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }

        let file_size = file.metadata()?.len();
        let (aligned_offset, delta, map_len) = Self::mapping_window(offset, len, file_size)?;
        let prot = Self::protection_to_unix(protection);
        let flags = libc::MAP_SHARED | libc::MAP_HUGETLB | huge_page_size.to_linux_flags();

        // SAFETY: Huge-page mapping uses the same validated aligned window as
        // the regular file-backed mapping path.
        let raw = unsafe {
            libc::mmap(
                ptr::null_mut(),
                map_len,
                prot,
                flags,
                file.as_raw_fd(),
                aligned_offset as libc::off_t,
            )
        };

        if raw == libc::MAP_FAILED {
            return Err(MmapError::MappingFailed(format!(
                "mmap with huge pages failed: {}",
                std::io::Error::last_os_error()
            )));
        }

        let map_ptr = raw.cast::<u8>();
        // SAFETY: `delta` is strictly within the mapped region returned above.
        let ptr = unsafe { map_ptr.add(delta) };

        Ok(Self {
            map_ptr,
            ptr,
            map_len,
            len,
        })
    }

    #[cfg(not(target_os = "linux"))]
    pub fn new_huge(
        file: &File,
        offset: u64,
        len: usize,
        protection: Protection,
        _huge_page_size: HugePageSize,
    ) -> Result<Self> {
        Self::new(file, offset, len, protection, MappingMode::Shared)
    }

    #[cfg(target_os = "linux")]
    pub fn new_anonymous_huge(
        len: usize,
        protection: Protection,
        huge_page_size: HugePageSize,
    ) -> Result<Self> {
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }

        let prot = Self::protection_to_unix(protection);
        let flags = libc::MAP_PRIVATE
            | libc::MAP_ANONYMOUS
            | libc::MAP_HUGETLB
            | huge_page_size.to_linux_flags();

        // SAFETY: Anonymous huge-page mapping with validated non-zero length.
        let raw = unsafe { libc::mmap(ptr::null_mut(), len, prot, flags, -1, 0) };
        if raw == libc::MAP_FAILED {
            return Err(MmapError::HugePagesUnsupported);
        }

        let map_ptr = raw.cast::<u8>();
        Ok(Self {
            map_ptr,
            ptr: map_ptr,
            map_len: len,
            len,
        })
    }

    #[cfg(not(target_os = "linux"))]
    pub fn new_anonymous_huge(
        len: usize,
        protection: Protection,
        _huge_page_size: HugePageSize,
    ) -> Result<Self> {
        Self::new_anonymous(len, protection)
    }

    #[must_use]
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.cast_const()
    }

    #[must_use]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn advise(&self, advice: crate::advice::MemoryAdvice) -> Result<()> {
        if self.len == 0 {
            return Ok(());
        }

        let advice_flag = advice.to_unix_advice();
        // SAFETY: `ptr` points to a valid mapped region of `len` bytes.
        let result = unsafe { libc::madvise(self.map_ptr.cast(), self.map_len, advice_flag) };
        if result != 0 {
            return Err(MmapError::SystemError(format!(
                "madvise failed: {}",
                std::io::Error::last_os_error()
            )));
        }

        Ok(())
    }

    pub fn prefault(&self) -> Result<()> {
        if self.len == 0 {
            return Ok(());
        }

        let page_size = super::page_size();
        // SAFETY: `ptr` points to a valid mapped region of `len` bytes.
        unsafe {
            let mut offset = 0;
            while offset < self.len {
                ptr::read_volatile(self.ptr.add(offset));
                offset += page_size;
            }
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub fn resize(&mut self, new_size: usize) -> Result<()> {
        if new_size == 0 {
            return Err(MmapError::EmptyMapping);
        }

        if self.ptr != self.map_ptr {
            return Err(MmapError::InvalidConfiguration(
                "resize is not supported for offset subrange mappings".to_string(),
            ));
        }

        if new_size == self.len {
            return Ok(());
        }

        // SAFETY: `map_ptr`/`map_len` describe the current valid mapping, and
        // `new_size` is validated above.
        let new_raw = unsafe {
            libc::mremap(
                self.map_ptr.cast(),
                self.map_len,
                new_size,
                libc::MREMAP_MAYMOVE,
            )
        };

        if new_raw == libc::MAP_FAILED {
            return Err(MmapError::SystemError(format!(
                "mremap failed: {}",
                std::io::Error::last_os_error()
            )));
        }

        let map_ptr = new_raw.cast::<u8>();
        self.map_ptr = map_ptr;
        self.ptr = map_ptr;
        self.map_len = new_size;
        self.len = new_size;
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn resize(&mut self, _new_size: usize) -> Result<()> {
        Err(MmapError::InvalidConfiguration(
            "resize is not supported on this platform".to_string(),
        ))
    }

    fn protection_to_unix(protection: Protection) -> libc::c_int {
        let mut prot = libc::PROT_NONE;
        if protection.can_read() {
            prot |= libc::PROT_READ;
        }
        if protection.can_write() {
            prot |= libc::PROT_WRITE;
        }
        if protection.can_execute() {
            prot |= libc::PROT_EXEC;
        }
        prot
    }

    fn mapping_window(offset: u64, len: usize, file_size: u64) -> Result<(u64, usize, usize)> {
        let end = offset
            .checked_add(len as u64)
            .ok_or(MmapError::InvalidRange {
                offset,
                len,
                file_len: file_size,
            })?;

        if end > file_size {
            return Err(MmapError::InvalidRange {
                offset,
                len,
                file_len: file_size,
            });
        }

        let granularity = super::mapping_granularity() as u64;
        let aligned_offset = offset / granularity * granularity;
        let delta = usize::try_from(offset - aligned_offset).map_err(|_| {
            MmapError::InvalidConfiguration("mapping offset delta exceeds usize".to_string())
        })?;
        let map_len = len.checked_add(delta).ok_or_else(|| {
            MmapError::InvalidConfiguration("mapping length overflowed usize".to_string())
        })?;

        Ok((aligned_offset, delta, map_len))
    }
}

impl Drop for PlatformMmap {
    fn drop(&mut self) {
        if !self.map_ptr.is_null() && self.map_len > 0 {
            // SAFETY: `map_ptr`/`map_len` describe the original mapped region.
            unsafe {
                let _ = libc::munmap(self.map_ptr.cast(), self.map_len);
            }
        }
    }
}

// SAFETY: The raw mapping handle can move across threads. Mutability is
// controlled by the higher-level typed wrapper.
unsafe impl Send for PlatformMmap {}
