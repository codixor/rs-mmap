use std::fs::File;
use std::os::windows::io::AsRawHandle;
use std::ptr;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_EXECUTE, FILE_MAP_READ, FILE_MAP_WRITE, MEM_COMMIT,
    MEM_LARGE_PAGES, MEM_RELEASE, MEM_RESERVE, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
    PAGE_EXECUTE_READ, PAGE_PROTECTION_FLAGS, PAGE_READONLY, PAGE_READWRITE, UnmapViewOfFile,
    VirtualAlloc, VirtualFree,
};

use crate::huge_pages::HugePageSize;
use crate::protection::Protection;
use crate::{MmapError, Result};

/// Platform-specific mapping handle.
pub struct PlatformMmap {
    map_ptr: *mut u8,
    ptr: *mut u8,
    len: usize,
    file_mapping: HANDLE,
    file_backed: bool,
}

impl PlatformMmap {
    pub fn new(file: &File, offset: u64, len: usize, protection: Protection) -> Result<Self> {
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }

        let file_size = file.metadata()?.len();
        let (aligned_offset, delta, map_len) = Self::mapping_window(offset, len, file_size)?;
        let page_protection = Self::protection_to_windows_page(protection);

        // SAFETY: The file handle is valid for the duration of the call and we
        // request a file-mapping object with OS-defined protection flags.
        let file_mapping = unsafe {
            CreateFileMappingW(
                HANDLE(file.as_raw_handle()),
                None,
                page_protection,
                0,
                0,
                None,
            )
            .map_err(|error| {
                MmapError::MappingFailed(format!("CreateFileMappingW failed: {error:?}"))
            })?
        };

        let desired_access = Self::protection_to_windows_access(protection);

        // SAFETY: We map a validated aligned window from the mapping object.
        let view = unsafe {
            MapViewOfFile(
                file_mapping,
                windows::Win32::System::Memory::FILE_MAP(desired_access),
                (aligned_offset >> 32) as u32,
                (aligned_offset & 0xFFFF_FFFF) as u32,
                map_len,
            )
        };

        if view.Value.is_null() {
            // SAFETY: Closing the handle is safe here because the mapping view
            // failed and the handle is otherwise unused.
            unsafe {
                let _ = CloseHandle(file_mapping);
            }
            return Err(MmapError::MappingFailed("MapViewOfFile failed".to_string()));
        }

        let map_ptr = view.Value.cast::<u8>();
        // SAFETY: `delta` is strictly within the mapped view returned above.
        let ptr = unsafe { map_ptr.add(delta) };

        Ok(Self {
            map_ptr,
            ptr,
            len,
            file_mapping,
            file_backed: true,
        })
    }

    pub fn new_anonymous(len: usize, protection: Protection) -> Result<Self> {
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }

        let page_protection = Self::protection_to_windows_page(protection);

        // SAFETY: `VirtualAlloc` is called with a validated non-zero length and
        // standard reserve+commit flags.
        let ptr = unsafe { VirtualAlloc(None, len, MEM_COMMIT | MEM_RESERVE, page_protection) };
        if ptr.is_null() {
            return Err(MmapError::MappingFailed(
                "VirtualAlloc failed for anonymous mapping".to_string(),
            ));
        }

        let map_ptr = ptr.cast::<u8>();
        Ok(Self {
            map_ptr,
            ptr: map_ptr,
            len,
            file_mapping: HANDLE(std::ptr::null_mut()),
            file_backed: false,
        })
    }

    pub fn new_huge(
        file: &File,
        offset: u64,
        len: usize,
        protection: Protection,
        _huge_page_size: HugePageSize,
    ) -> Result<Self> {
        Self::new(file, offset, len, protection)
    }

    pub fn new_anonymous_huge(
        len: usize,
        protection: Protection,
        _huge_page_size: HugePageSize,
    ) -> Result<Self> {
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }

        let page_protection = Self::protection_to_windows_page(protection);

        // SAFETY: Large-page allocation mirrors the regular anonymous path.
        let ptr = unsafe {
            VirtualAlloc(
                None,
                len,
                MEM_COMMIT | MEM_RESERVE | MEM_LARGE_PAGES,
                page_protection,
            )
        };

        if ptr.is_null() {
            return Self::new_anonymous(len, protection);
        }

        let map_ptr = ptr.cast::<u8>();
        Ok(Self {
            map_ptr,
            ptr: map_ptr,
            len,
            file_mapping: HANDLE(std::ptr::null_mut()),
            file_backed: false,
        })
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

    pub fn advise(&self, _advice: crate::advice::MemoryAdvice) -> Result<()> {
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

    pub fn resize(&mut self, _new_size: usize) -> Result<()> {
        Err(MmapError::InvalidConfiguration(
            "resize is not supported on Windows".to_string(),
        ))
    }

    fn protection_to_windows_page(protection: Protection) -> PAGE_PROTECTION_FLAGS {
        if protection.can_execute() {
            if protection.can_write() {
                PAGE_PROTECTION_FLAGS(0x40)
            } else if protection.can_read() {
                PAGE_EXECUTE_READ
            } else {
                PAGE_PROTECTION_FLAGS(0x10)
            }
        } else if protection.can_write() {
            PAGE_READWRITE
        } else if protection.can_read() {
            PAGE_READONLY
        } else {
            PAGE_PROTECTION_FLAGS(0x01)
        }
    }

    fn protection_to_windows_access(protection: Protection) -> u32 {
        let mut access = 0;
        if protection.can_read() {
            access |= FILE_MAP_READ.0;
        }
        if protection.can_write() {
            access |= FILE_MAP_WRITE.0;
        }
        if protection.can_execute() {
            access |= FILE_MAP_EXECUTE.0;
        }
        access
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
        if !self.map_ptr.is_null() {
            // SAFETY: The pointer and drop path match the allocation path.
            unsafe {
                if self.file_backed {
                    let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                        Value: self.map_ptr.cast(),
                    });
                } else {
                    let _ = VirtualFree(self.map_ptr.cast(), 0, MEM_RELEASE);
                }
            }
        }

        if self.file_backed && !self.file_mapping.is_invalid() {
            // SAFETY: The mapping handle belongs to this object.
            unsafe {
                let _ = CloseHandle(self.file_mapping);
            }
        }
    }
}

// SAFETY: The raw mapping handle can move across threads. Mutability is
// controlled by the higher-level typed wrapper.
unsafe impl Send for PlatformMmap {}
