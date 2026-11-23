use crate::{
    CopyOnWrite, MemoryAdvice, Mmap, MmapError, ReadOnly, ReadOnlyMmap, ReadWrite, Result,
};
use std::{
    marker::PhantomData,
    path::{Path, PathBuf},
};

/// Builder without a selected source.
#[cfg_attr(
    feature = "serde-support",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPath;
/// Builder with a file or anonymous source.
#[cfg_attr(
    feature = "serde-support",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Debug, Clone, Copy, Default)]
pub struct HasPath;

/// Selects a byte range and its access mode.
#[cfg_attr(
    feature = "serde-support",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[derive(Debug, Clone)]
pub struct MmapOptions<State = NoPath> {
    path: Option<PathBuf>,
    offset: u64,
    len: Option<usize>,
    advice: MemoryAdvice,
    prefault: bool,
    #[cfg(feature = "huge-pages")]
    huge_pages: Option<crate::HugePageSize>,
    #[cfg_attr(feature = "serde-support", serde(skip))]
    state: PhantomData<State>,
}
impl Default for MmapOptions<NoPath> {
    fn default() -> Self {
        Self::new()
    }
}
impl MmapOptions<NoPath> {
    pub fn new() -> Self {
        Self {
            path: None,
            offset: 0,
            len: None,
            advice: MemoryAdvice::Normal,
            prefault: false,
            #[cfg(feature = "huge-pages")]
            huge_pages: None,
            state: PhantomData,
        }
    }
    pub fn path(self, path: impl AsRef<Path>) -> MmapOptions<HasPath> {
        MmapOptions {
            path: Some(path.as_ref().to_owned()),
            ..self.select()
        }
    }
    fn select(self) -> MmapOptions<HasPath> {
        MmapOptions {
            path: self.path,
            offset: self.offset,
            len: self.len,
            advice: self.advice,
            prefault: self.prefault,
            #[cfg(feature = "huge-pages")]
            huge_pages: self.huge_pages,
            state: PhantomData,
        }
    }
    pub fn new_anonymous(size: usize) -> MmapOptions<HasPath> {
        Self::new().select().len(size)
    }
}
impl MmapOptions<HasPath> {
    #[must_use]
    pub fn offset(mut self, offset: u64) -> Self {
        self.offset = offset;
        self
    }
    #[must_use]
    pub fn len(mut self, len: usize) -> Self {
        self.len = Some(len);
        self
    }
    #[must_use]
    pub fn advice(mut self, advice: MemoryAdvice) -> Self {
        self.advice = advice;
        self
    }
    #[must_use]
    pub fn prefault(mut self, enable: bool) -> Self {
        self.prefault = enable;
        self
    }
    #[must_use]
    pub fn populate(self) -> Self {
        self.prefault(true)
    }
    /// Requires anonymous Linux huge pages of this size. An unavailable pool is an error.
    #[cfg(feature = "huge-pages")]
    #[must_use]
    pub fn huge_pages(mut self, size: crate::HugePageSize) -> Self {
        self.huge_pages = Some(size);
        self
    }

    pub fn open(self) -> Result<ReadOnlyMmap> {
        self.map_readonly().map(Mmap::into_read_only)
    }

    pub fn map_readonly(self) -> Result<Mmap<ReadOnly>> {
        #[cfg(any(unix, windows))]
        {
            let bytes = if let Some(path) = &self.path {
                self.check_file_options()?;
                let source = crate::ReadOnlyFile::new(std::fs::File::open(path)?)?;
                let view = source.window(self.offset, self.len)?;
                view.advise_range(0..view.len(), self.advice)?;
                view.into_read_only()
            } else {
                self.allocate()?.make_read_only()?.into()
            };
            self.touch(&bytes);
            Ok(Mmap(ReadOnly(bytes)))
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(MmapError::UnsupportedPlatform)
        }
    }

    /// Opens a writable snapshot. Call `flush` to persist changes.
    pub fn map_readwrite(self) -> Result<Mmap<ReadWrite>> {
        self.writable(true)
    }
    /// Creates a private writable copy. The original file only needs read access.
    pub fn map_copy(self) -> Result<Mmap<CopyOnWrite>> {
        self.writable(false).map(|m| Mmap(CopyOnWrite(m.0)))
    }
    fn writable(self, write_back: bool) -> Result<Mmap<ReadWrite>> {
        #[cfg(any(unix, windows))]
        {
            if self.advice != MemoryAdvice::Normal {
                return Err(MmapError::UnsupportedPlatform);
            }
            let map = if let Some(path) = &self.path {
                self.check_file_options()?;
                let file = std::fs::File::options()
                    .read(true)
                    .write(write_back)
                    .open(path)?;
                Mmap::snapshot(file, self.offset, self.len, write_back)?
            } else {
                Mmap::from_anonymous(self.allocate()?)
            };
            self.touch(&map);
            Ok(map)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = write_back;
            Err(MmapError::UnsupportedPlatform)
        }
    }
    #[cfg(any(unix, windows))]
    fn check_file_options(&self) -> Result<()> {
        #[cfg(feature = "huge-pages")]
        if self.huge_pages.is_some() {
            return Err(MmapError::UnsupportedPlatform);
        }
        Ok(())
    }
    #[cfg(any(unix, windows))]
    fn touch(&self, bytes: &[u8]) {
        if self.prefault {
            for chunk in bytes.chunks(4096) {
                std::hint::black_box(chunk[0]);
            }
        }
    }
    #[cfg(any(unix, windows))]
    fn allocate(&self) -> Result<memmap2::MmapMut> {
        if self.offset != 0 || self.advice != MemoryAdvice::Normal {
            return Err(MmapError::InvalidConfiguration(
                "anonymous mappings require offset zero and normal advice".into(),
            ));
        }
        let len = self.len.unwrap_or(0);
        if len == 0 {
            return Err(MmapError::EmptyMapping);
        }
        if len > isize::MAX as usize {
            return Err(MmapError::InvalidConfiguration(
                "mapping exceeds the maximum slice length".into(),
            ));
        }
        let mut opts = memmap2::MmapOptions::new();
        opts.len(len);
        #[cfg(feature = "huge-pages")]
        if let Some(size) = self.huge_pages {
            #[cfg(target_os = "linux")]
            {
                if !len.is_multiple_of(size.size_bytes()) {
                    return Err(MmapError::InvalidConfiguration(
                        "length must be a multiple of the requested huge page size".into(),
                    ));
                }
                opts.huge(Some(size.size_bytes().trailing_zeros() as u8));
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = size;
                return Err(MmapError::UnsupportedPlatform);
            }
        }
        Ok(opts.map_anon()?)
    }
}
