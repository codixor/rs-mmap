#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
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
mod builder;
#[cfg(any(unix, windows))]
mod read_only_file;
#[cfg(any(unix, windows))]
pub use read_only_file::{FileView, ReadOnlyFile};
mod byte_field;
mod error;
mod file_lock;
#[cfg(feature = "huge-pages")]
mod huge_pages;
#[path = "mapping.rs"]
mod mmap;
#[cfg(feature = "numa")]
mod numa;
pub use byte_field::ByteArrayExt;
#[cfg(all(feature = "zerocopy-views", any(unix, windows)))]
mod record;
/// Native memmap2 0.9 types with their original APIs and type identity.
#[cfg(any(unix, windows))]
pub use memmap2 as compat;
#[cfg(all(feature = "zerocopy-views", any(unix, windows)))]
pub use record::MmapStruct;
mod slice;
#[cfg(feature = "zerocopy-views")]
mod view;

use std::{fmt, ops::Deref, sync::Arc};

pub use advice::MemoryAdvice;
pub use builder::{HasPath, MmapOptions, NoPath};
pub use error::MmapError;
pub use file_lock::{FileLock, LockType};
#[cfg(feature = "huge-pages")]
pub use huge_pages::HugePageSize;
pub use mmap::{CopyOnWrite, Mmap, ReadOnly, ReadWrite};
#[cfg(feature = "numa")]
pub use numa::{NumaNode, NumaTopology};
pub use slice::MmapSlice;
#[cfg(feature = "zerocopy-views")]
pub use view::ZeroCopyView;

/// Result type alias for mmap operations.
pub type Result<T> = std::result::Result<T, MmapError>;

#[derive(Clone)]
enum Backing {
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
    /// Opens the complete file as immutable bytes. Ordinary files are copied.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Self::open_options(path, 0, None)
    }

    /// Opens a file range. An omitted length selects the remaining bytes.
    pub fn open_options(
        path: impl AsRef<std::path::Path>,
        offset: u64,
        len: Option<usize>,
    ) -> Result<Self> {
        let options = MmapOptions::new().path(path).offset(offset);
        match len {
            Some(len) => options.len(len).open(),
            None => options.open(),
        }
    }

    /// Takes ownership of a typed read-only mapping without copying.
    pub fn from_mmap(map: Mmap<ReadOnly>) -> Self {
        map.into_read_only()
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
