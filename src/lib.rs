#![allow(unsafe_code)]
#![allow(
    clippy::borrow_as_ptr,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::derivable_impls,
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::uninlined_format_args,
    clippy::unnecessary_wraps,
    clippy::unused_self
)]

mod advice;
#[cfg(all(feature = "async", not(target_arch = "wasm32")))]
mod async_mmap;
mod builder;
#[cfg(any(unix, windows))]
mod read_only_file;
#[cfg(any(unix, windows))]
pub use read_only_file::{FileView, ReadOnlyFile};
mod error;
mod file_lock;
mod huge_pages;
#[path = "mapping.rs"]
mod mmap;
#[cfg(feature = "numa")]
mod numa;
mod platform;
mod protection;
mod slice;
#[cfg(feature = "zerocopy-views")]
mod view;

use std::{fmt, ops::Deref, sync::Arc};

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

pub use advice::MemoryAdvice;
#[cfg(all(feature = "async", not(target_arch = "wasm32")))]
pub use async_mmap::AsyncMmap;
pub use builder::{HasPath, MmapOptions, NoPath};
pub use error::MmapError;
pub use file_lock::{FileLock, LockType};
pub use huge_pages::HugePageSize;
pub use mmap::{CopyOnWrite, Mmap, ReadOnly, ReadWrite};
#[cfg(feature = "numa")]
pub use numa::{NumaBinder, NumaNode, NumaPolicy, NumaTopology};
#[cfg(unix)]
pub use platform::MappingMode;
pub use protection::Protection;
pub use slice::MmapSlice;
#[cfg(feature = "zerocopy-views")]
pub use view::ZeroCopyView;

/// Result type alias for mmap operations.
pub type Result<T> = std::result::Result<T, MmapError>;

#[derive(Clone)]
enum Backing {
    Mapped(Arc<Mmap<ReadOnly>>),
    Owned(Arc<[u8]>),
    #[cfg(any(unix, windows))]
    Memmap2(Arc<memmap2::Mmap>),
}

/// Small read-only byte container backed by either a native memory map or
/// shared owned bytes.
///
/// Clones share the same bytes. Converting a native memmap2 mapping keeps
/// its mapped address and gives it the same shared ownership.
#[derive(Clone)]
pub struct ReadOnlyMmap {
    backing: Backing,
}

impl ReadOnlyMmap {
    /// Opens a file and creates a shared read-only memory mapping for its
    /// contents.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or the mapping fails.
    ///
    /// # Safety
    /// Prevent writes and truncation until every clone and view of the mapping is dropped.
    #[cfg(not(target_arch = "wasm32"))]
    pub unsafe fn open(path: impl AsRef<Path>) -> Result<Self> {
        // SAFETY: the caller guarantees a stable file.
        unsafe { Self::open_options(path, 0, None) }
    }

    /// Opens a file and creates a shared read-only memory mapping for its
    /// contents.
    ///
    /// # Errors
    ///
    /// Always returns [`MmapError::UnsupportedPlatform`] on `wasm32`.
    ///
    /// # Safety
    /// Prevent writes and truncation until every clone and view of the mapping is dropped.
    #[cfg(target_arch = "wasm32")]
    pub unsafe fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let _ = path;
        Err(MmapError::UnsupportedPlatform)
    }

    /// Opens a read-only file mapping for the configured byte range.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the file cannot be opened, the requested
    /// mapping is invalid, or the platform does not support file-backed
    /// mappings.
    ///
    /// # Safety
    /// Prevent writes and truncation until every clone and view of the mapping is dropped.
    #[cfg(not(target_arch = "wasm32"))]
    pub unsafe fn open_options(
        path: impl AsRef<Path>,
        offset: u64,
        len: Option<usize>,
    ) -> Result<Self> {
        let mut builder = MmapOptions::new().path(path).offset(offset);
        if let Some(len) = len {
            builder = builder.len(len);
        }
        // SAFETY: the caller guarantees a stable file.
        unsafe { builder.open() }
    }

    /// Opens a read-only file mapping for the configured byte range.
    ///
    /// # Errors
    ///
    /// Always returns [`MmapError::UnsupportedPlatform`] on `wasm32`.
    ///
    /// # Safety
    /// Prevent writes and truncation until every clone and view of the mapping is dropped.
    #[cfg(target_arch = "wasm32")]
    pub unsafe fn open_options(
        path: impl AsRef<std::path::Path>,
        offset: u64,
        len: Option<usize>,
    ) -> Result<Self> {
        let _ = (path, offset, len);
        Err(MmapError::UnsupportedPlatform)
    }

    /// Wraps an existing read-only mapping in the compatibility container.
    #[must_use]
    pub fn from_mmap(mmap: Mmap<ReadOnly>) -> Self {
        Self {
            backing: Backing::Mapped(Arc::new(mmap)),
        }
    }

    /// Creates a read-only view from shared in-memory bytes.
    #[must_use]
    pub fn from_bytes(bytes: Arc<[u8]>) -> Self {
        Self {
            backing: Backing::Owned(bytes),
        }
    }

    /// Creates a read-only view from owned in-memory bytes.
    #[must_use]
    pub fn from_vec(bytes: Vec<u8>) -> Self {
        Self::from_bytes(Arc::<[u8]>::from(bytes))
    }

    /// Returns the mapped bytes as an immutable slice.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        match &self.backing {
            Backing::Mapped(bytes) => bytes.as_slice(),
            Backing::Owned(bytes) => bytes,
            #[cfg(any(unix, windows))]
            Backing::Memmap2(map) => map,
        }
    }

    /// Returns the length of the mapped region.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Returns whether the mapped region is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }
}

impl fmt::Debug for ReadOnlyMmap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadOnlyMmap")
            .field("len", &self.len())
            .finish()
    }
}

impl AsRef<[u8]> for ReadOnlyMmap {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl Deref for ReadOnlyMmap {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

#[cfg(any(unix, windows))]
impl From<memmap2::Mmap> for ReadOnlyMmap {
    /// Shares an existing memmap2 mapping without copying or remapping its bytes.
    fn from(map: memmap2::Mmap) -> Self {
        Self {
            backing: Backing::Memmap2(Arc::new(map)),
        }
    }
}
