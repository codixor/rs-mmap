//! The only module in this crate that contains `unsafe`.
//!
//! Everything here maps a file whose stability the operating system has not
//! verified. The promise that the file stays unchanged comes from the caller
//! of an `unsafe` constructor and is carried by a [`Stable`] token, which can
//! only be created inside this module.
//!
//! On Unix each view is one memmap2 mapping. On Windows a stable source owns
//! one file-mapping object and every view is a `MapViewOfFile` of it, which
//! avoids creating and destroying a mapping object per view.
#![allow(unsafe_code)]

use crate::read_only_file::Stability;
use crate::{HasPath, Mmap, MmapOptions, ReadOnly, ReadOnlyFile, ReadOnlyMmap, Result};
use std::fs::File;

/// Proof that a caller accepted the file-stability contract.
pub(crate) struct Stable {
    #[cfg(windows)]
    section: section::Section,
}

impl Stable {
    /// # Safety
    ///
    /// `file` must stay unmodified and untruncated while any mapping made
    /// through the returned token is alive.
    unsafe fn new(file: &File) -> Result<Self> {
        #[cfg(windows)]
        {
            Ok(Self {
                section: section::Section::new(file)?,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = file;
            Ok(Self {})
        }
    }
}

/// Maps a checked, nonempty range of a file covered by a stability promise.
#[cfg(not(windows))]
pub(crate) fn map(_: &Stable, file: &File, offset: u64, len: usize) -> Result<ReadOnlyMmap> {
    // SAFETY: a `Stable` token exists only when the caller of `new_stable` or
    // `map_stable` promised that the file is neither modified nor truncated
    // while the source or any mapping derived from it is alive. The range was
    // checked against the file length by `range_len`.
    let map = unsafe {
        memmap2::MmapOptions::new()
            .offset(offset)
            .len(len)
            .map(file)?
    };
    Ok(map.into())
}

/// Maps a checked, nonempty range through the source's shared mapping object.
#[cfg(windows)]
pub(crate) fn map(stable: &Stable, _: &File, offset: u64, len: usize) -> Result<ReadOnlyMmap> {
    let view = stable.section.view(offset, len)?;
    Ok(ReadOnlyMmap {
        backing: crate::Backing::Section(std::sync::Arc::new(view)),
    })
}

impl ReadOnlyFile {
    /// Prepares a file whose views always map the original pages. No bytes
    /// are copied and the snapshot limit does not apply. On Windows all views
    /// share one file-mapping object created here.
    ///
    /// Use this for closed log segments, static assets and other files that
    /// are complete and will not be rewritten.
    ///
    /// # Safety
    ///
    /// The file must not be modified or truncated, through any handle or by
    /// any process, until this source and every view created from it have
    /// been dropped. Otherwise views can observe changing bytes, and access
    /// beyond a truncated end terminates the process.
    pub unsafe fn new_stable(file: File) -> Result<Self> {
        // SAFETY: the caller's contract is the one `Stable::new` requires.
        let stable = unsafe { Stable::new(&file)? };
        Self::with_stability(file, Stability::Trusted(stable))
    }
}

impl MmapOptions<HasPath> {
    /// Maps the selected range of a stable file without copying it.
    ///
    /// # Safety
    ///
    /// The file must not be modified or truncated, through any handle or by
    /// any process, until the returned mapping and everything sharing its
    /// bytes have been dropped.
    pub unsafe fn map_stable(self) -> Result<Mmap<ReadOnly>> {
        self.map_with(|file| {
            // SAFETY: the caller's contract is the one `Stable::new` requires.
            Ok(Stability::Trusted(unsafe { Stable::new(file)? }))
        })
    }
}

#[cfg(windows)]
pub(crate) use section::SectionView;

#[cfg(windows)]
mod section {
    use crate::{MmapError, Result};
    use std::{
        ffi::c_void,
        fs::File,
        io,
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
        sync::{Arc, OnceLock},
    };
    use windows_sys::Win32::System::{
        Memory::{
            CreateFileMappingW, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
            PAGE_READONLY, UnmapViewOfFile,
        },
        SystemInformation::{GetSystemInfo, SYSTEM_INFO},
    };

    /// File offsets of views must be multiples of this value.
    fn granularity() -> u64 {
        static GRANULARITY: OnceLock<u64> = OnceLock::new();
        *GRANULARITY.get_or_init(|| {
            let mut info = SYSTEM_INFO::default();
            // SAFETY: GetSystemInfo only writes the provided structure.
            unsafe { GetSystemInfo(&mut info) };
            u64::from(info.dwAllocationGranularity).max(1)
        })
    }

    /// A read-only mapping object covering a whole stable file.
    pub(super) struct Section {
        handle: OwnedHandle,
        // Views keep the file open, including delete-on-close files.
        file: Arc<File>,
    }

    impl Section {
        pub(super) fn new(file: &File) -> Result<Self> {
            if file.metadata()?.len() == 0 {
                return Err(MmapError::EmptyMapping);
            }
            let file = Arc::new(file.try_clone()?);
            // SAFETY: the handle is a valid open file. A zero maximum size
            // makes the object cover the current length of the file.
            let handle = unsafe {
                CreateFileMappingW(
                    file.as_raw_handle(),
                    std::ptr::null(),
                    PAGE_READONLY,
                    0,
                    0,
                    std::ptr::null(),
                )
            };
            if handle.is_null() {
                return Err(io::Error::last_os_error().into());
            }
            // SAFETY: the successful call returned a handle that this
            // process owns and must close exactly once.
            let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
            Ok(Self { handle, file })
        }

        /// Maps `len` bytes at `offset`. The caller has checked that the
        /// range is nonempty and lies inside the file.
        pub(super) fn view(&self, offset: u64, len: usize) -> Result<SectionView> {
            let delta = offset % granularity();
            let aligned = offset - delta;
            // The remainder is smaller than the 32-bit granularity.
            let delta = delta as usize;
            let map_len = len.checked_add(delta).ok_or_else(|| {
                MmapError::InvalidConfiguration("mapping length overflowed".into())
            })?;
            // SAFETY: the handle is a live read-only mapping object, the
            // offset is aligned to the allocation granularity, and the range
            // lies inside the file the object covers.
            let base = unsafe {
                MapViewOfFile(
                    self.handle.as_raw_handle(),
                    FILE_MAP_READ,
                    (aligned >> 32) as u32,
                    (aligned & 0xFFFF_FFFF) as u32,
                    map_len,
                )
            };
            if base.Value.is_null() {
                return Err(io::Error::last_os_error().into());
            }
            Ok(SectionView {
                base: base.Value,
                delta,
                len,
                _file: self.file.clone(),
            })
        }
    }

    /// One mapped view. It stays valid after its section handle is closed.
    pub(crate) struct SectionView {
        base: *mut c_void,
        delta: usize,
        len: usize,
        _file: Arc<File>,
    }

    impl SectionView {
        pub(crate) fn as_slice(&self) -> &[u8] {
            // SAFETY: `base` is a live view of `delta + len` readable bytes
            // that is unmapped only when this value is dropped. The stability
            // contract keeps the bytes unchanged, and `len` was checked
            // against `isize::MAX`.
            unsafe { std::slice::from_raw_parts(self.base.cast::<u8>().add(self.delta), self.len) }
        }
    }

    impl Drop for SectionView {
        fn drop(&mut self) {
            // SAFETY: `base` came from MapViewOfFile and is unmapped once.
            unsafe {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: self.base });
            }
        }
    }

    // SAFETY: the view is immutable memory owned by this value. Reading it
    // from, or dropping it on, another thread is sound.
    unsafe impl Send for SectionView {}
    // SAFETY: shared access only reads immutable bytes.
    unsafe impl Sync for SectionView {}
}
