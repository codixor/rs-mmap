#![allow(dead_code)]

use std::sync::OnceLock;

static PAGE_SIZE: OnceLock<usize> = OnceLock::new();
static MAPPING_GRANULARITY: OnceLock<usize> = OnceLock::new();

/// Returns the system page size.
#[must_use]
pub fn page_size() -> usize {
    *PAGE_SIZE.get_or_init(|| {
        #[cfg(unix)]
        {
            // SAFETY: `sysconf` is safe to call for `_SC_PAGESIZE`.
            unsafe {
                let size = libc::sysconf(libc::_SC_PAGESIZE);
                if size > 0 { size as usize } else { 4096 }
            }
        }

        #[cfg(windows)]
        {
            use windows::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};

            // SAFETY: `GetSystemInfo` fills the provided structure.
            unsafe {
                let mut info = SYSTEM_INFO::default();
                GetSystemInfo(&mut info);
                info.dwPageSize as usize
            }
        }

        #[cfg(not(any(unix, windows)))]
        {
            4096
        }
    })
}

/// Returns the file mapping offset granularity for the current platform.
#[must_use]
pub fn mapping_granularity() -> usize {
    *MAPPING_GRANULARITY.get_or_init(|| {
        #[cfg(unix)]
        {
            page_size()
        }

        #[cfg(windows)]
        {
            use windows::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};

            // SAFETY: `GetSystemInfo` fills the provided structure.
            unsafe {
                let mut info = SYSTEM_INFO::default();
                GetSystemInfo(&mut info);
                info.dwAllocationGranularity as usize
            }
        }

        #[cfg(not(any(unix, windows)))]
        {
            page_size()
        }
    })
}
