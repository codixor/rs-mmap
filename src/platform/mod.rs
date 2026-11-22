#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

mod common;

#[cfg(not(any(unix, windows)))]
#[allow(dead_code)]
mod unsupported {
    use std::fs::File;

    use crate::huge_pages::HugePageSize;
    use crate::protection::Protection;
    use crate::{MmapError, Result};

    /// Unsupported-platform placeholder mapping handle.
    pub struct PlatformMmap;

    impl PlatformMmap {
        pub fn new_anonymous(_len: usize, _protection: Protection) -> Result<Self> {
            Err(MmapError::UnsupportedPlatform)
        }

        pub fn new_anonymous_huge(
            _len: usize,
            _protection: Protection,
            _huge_page_size: HugePageSize,
        ) -> Result<Self> {
            Err(MmapError::UnsupportedPlatform)
        }

        pub fn new_file_backed_unsupported() -> Result<Self> {
            Err(MmapError::UnsupportedPlatform)
        }

        pub fn new(
            _file: &File,
            _offset: u64,
            _len: usize,
            _protection: Protection,
        ) -> Result<Self> {
            Err(MmapError::UnsupportedPlatform)
        }

        pub fn new_huge(
            _file: &File,
            _offset: u64,
            _len: usize,
            _protection: Protection,
            _huge_page_size: HugePageSize,
        ) -> Result<Self> {
            Err(MmapError::UnsupportedPlatform)
        }

        #[must_use]
        pub fn as_ptr(&self) -> *const u8 {
            std::ptr::null()
        }

        #[must_use]
        pub fn as_mut_ptr(&mut self) -> *mut u8 {
            std::ptr::null_mut()
        }

        #[must_use]
        pub fn len(&self) -> usize {
            0
        }

        #[must_use]
        pub fn is_empty(&self) -> bool {
            true
        }

        pub fn advise(&self, _advice: crate::advice::MemoryAdvice) -> Result<()> {
            Err(MmapError::UnsupportedPlatform)
        }

        pub fn prefault(&self) -> Result<()> {
            Err(MmapError::UnsupportedPlatform)
        }

        pub fn resize(&mut self, _new_size: usize) -> Result<()> {
            Err(MmapError::UnsupportedPlatform)
        }
    }
}

#[allow(unused_imports)]
pub use common::*;
#[cfg(unix)]
pub use unix::*;
#[cfg(not(any(unix, windows)))]
pub use unsupported::*;
#[cfg(windows)]
pub use windows::*;
