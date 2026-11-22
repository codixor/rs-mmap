use std::marker::PhantomData;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use crate::MappingMode;
use crate::advice::MemoryAdvice;
use crate::huge_pages::HugePageSize;
use crate::protection::Protection;
use crate::{MmapError, ReadOnly, ReadOnlyMmap, Result, mmap::Mmap};

/// Type state for [`MmapOptions`] before a path has been set.
#[cfg_attr(
    feature = "serde-support",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPath;

/// Type state for [`MmapOptions`] after a path has been set.
#[cfg_attr(
    feature = "serde-support",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Debug, Clone, Copy, Default)]
pub struct HasPath;

/// Type-safe builder for memory-mapped file operations.
// Deserialization only constructs options. Mapping validates sizes and file
// ranges, and the file stability contract is checked by the caller at use.
#[cfg_attr(feature = "serde-support", allow(clippy::unsafe_derive_deserialize))]
#[cfg_attr(
    feature = "serde-support",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Debug, Clone)]
pub struct MmapOptions<State = NoPath> {
    path: Option<PathBuf>,
    protection: Protection,
    offset: u64,
    len: Option<usize>,
    advice: MemoryAdvice,
    anonymous: bool,
    prefault: bool,
    huge_pages: Option<HugePageSize>,
    #[cfg(unix)]
    mapping_mode: MappingMode,
    _state: PhantomData<State>,
}

impl MmapOptions<NoPath> {
    /// Creates a new builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: None,
            protection: Protection::READ,
            offset: 0,
            len: None,
            advice: MemoryAdvice::Normal,
            anonymous: false,
            prefault: false,
            huge_pages: None,
            #[cfg(unix)]
            mapping_mode: MappingMode::Shared,
            _state: PhantomData,
        }
    }

    /// Creates a new anonymous mapping builder.
    #[must_use]
    pub fn new_anonymous(size: usize) -> MmapOptions<HasPath> {
        MmapOptions {
            path: None,
            protection: Protection::READ | Protection::WRITE,
            offset: 0,
            len: Some(size),
            advice: MemoryAdvice::Normal,
            anonymous: true,
            prefault: false,
            huge_pages: None,
            #[cfg(unix)]
            mapping_mode: MappingMode::Private,
            _state: PhantomData,
        }
    }

    /// Sets the file path to map.
    #[must_use]
    pub fn path(self, path: impl AsRef<Path>) -> MmapOptions<HasPath> {
        MmapOptions {
            path: Some(path.as_ref().to_path_buf()),
            protection: self.protection,
            offset: self.offset,
            len: self.len,
            advice: self.advice,
            anonymous: false,
            prefault: self.prefault,
            huge_pages: self.huge_pages,
            #[cfg(unix)]
            mapping_mode: MappingMode::Shared,
            _state: PhantomData,
        }
    }
}

impl Default for MmapOptions<NoPath> {
    fn default() -> Self {
        Self::new()
    }
}

impl MmapOptions<HasPath> {
    /// Sets the protection flags.
    #[must_use]
    pub fn protection(mut self, protection: Protection) -> Self {
        self.protection = protection;
        self
    }

    /// Sets the starting file offset for the mapping.
    #[must_use]
    pub fn offset(mut self, offset: u64) -> Self {
        self.offset = offset;
        self
    }

    /// Sets the requested mapping length.
    #[must_use]
    pub fn len(mut self, len: usize) -> Self {
        self.len = Some(len);
        self
    }

    /// Sets the OS memory advice hint.
    #[must_use]
    pub fn advice(mut self, advice: MemoryAdvice) -> Self {
        self.advice = advice;
        self
    }

    /// Enables or disables prefaulting.
    #[must_use]
    pub fn prefault(mut self, enable: bool) -> Self {
        self.prefault = enable;
        self
    }

    /// Convenience method for prefaulting all pages.
    #[must_use]
    pub fn populate(self) -> Self {
        self.prefault(true)
    }

    /// Enables huge page mapping when supported.
    #[must_use]
    pub fn huge_pages(mut self, size: HugePageSize) -> Self {
        self.huge_pages = Some(size);
        self
    }

    /// Marks the mapping as shared on Unix.
    #[cfg(unix)]
    #[must_use]
    pub fn shared(mut self) -> Self {
        self.mapping_mode = MappingMode::Shared;
        self
    }

    /// Marks the mapping as private on Unix.
    #[cfg(unix)]
    #[must_use]
    pub fn private(mut self) -> Self {
        self.mapping_mode = MappingMode::Private;
        self
    }

    /// Opens the file and creates a read-only compatibility mapping.
    ///
    /// This keeps the existing `ReadOnlyMmap`-based surface intact for the
    /// rest of the workspace.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the configured mapping cannot be created.
    ///
    /// # Safety
    /// The file must not be modified or truncated while the mapping or any of its views live.
    pub unsafe fn open(self) -> Result<ReadOnlyMmap> {
        // SAFETY: open has the same file stability contract.
        unsafe { self.map_readonly() }.map(ReadOnlyMmap::from_mmap)
    }

    /// Creates a read-only typed mapping.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the configured mapping cannot be created.
    ///
    /// # Safety
    /// The file must not be modified or truncated while the mapping or any of its views live.
    pub unsafe fn map_readonly(mut self) -> Result<Mmap<ReadOnly>> {
        self.protection = Protection::READ;
        self.map_internal()
    }

    /// Creates a read-write typed mapping.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the configured mapping cannot be created.
    ///
    /// # Safety
    /// The mapped bytes must not be accessed through other handles while this mapping lives, and the file must not be truncated.
    pub unsafe fn map_readwrite(mut self) -> Result<Mmap<crate::ReadWrite>> {
        self.protection = Protection::READ | Protection::WRITE;
        self.map_internal()
    }

    /// Creates a typed mapping with the configured options.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the configured mapping cannot be created.
    ///
    /// # Safety
    /// The protection must permit all accesses exposed by Mode. For file mappings, prevent truncation and conflicting access for the mapping's lifetime.
    pub unsafe fn map<Mode>(self) -> Result<Mmap<Mode>> {
        self.map_internal()
    }

    fn map_internal<Mode>(self) -> Result<Mmap<Mode>> {
        use crate::platform::PlatformMmap;
        use std::fs::OpenOptions;

        if self.len.is_some_and(|len| len > isize::MAX as usize) {
            return Err(MmapError::InvalidConfiguration(
                "mapping exceeds the maximum slice length".into(),
            ));
        }
        if self.prefault && !self.protection.can_read() {
            return Err(MmapError::InvalidConfiguration(
                "prefault requires read access".into(),
            ));
        }

        let platform_mmap = if self.anonymous {
            let len = self.len.ok_or_else(|| {
                MmapError::InvalidConfiguration(
                    "anonymous mapping requires an explicit size".to_string(),
                )
            })?;

            if len == 0 {
                return Err(MmapError::EmptyMapping);
            }

            if let Some(huge_page_size) = self.huge_pages {
                PlatformMmap::new_anonymous_huge(len, self.protection, huge_page_size)?
            } else {
                PlatformMmap::new_anonymous(len, self.protection)?
            }
        } else {
            let Some(path) = self.path else {
                return Err(MmapError::InvalidConfiguration(
                    "path must be set for file-backed mappings".to_string(),
                ));
            };

            let file = if self.protection.can_write() {
                OpenOptions::new().read(true).write(true).open(&path)?
            } else {
                OpenOptions::new().read(true).open(&path)?
            };

            let file_len = file.metadata()?.len();
            let len = match self.len {
                Some(0) => return Err(MmapError::EmptyMapping),
                Some(len) => len,
                None => {
                    let remaining =
                        file_len
                            .checked_sub(self.offset)
                            .ok_or(MmapError::InvalidRange {
                                offset: self.offset,
                                len: 0,
                                file_len,
                            })?;
                    usize::try_from(remaining).map_err(|_| MmapError::InvalidRange {
                        offset: self.offset,
                        len: usize::MAX,
                        file_len,
                    })?
                }
            };

            if len == 0 {
                return Err(MmapError::EmptyMapping);
            }

            if len > isize::MAX as usize {
                return Err(MmapError::InvalidRange {
                    offset: self.offset,
                    len,
                    file_len,
                });
            }
            #[cfg(unix)]
            let platform = if let Some(huge_page_size) = self.huge_pages {
                PlatformMmap::new_huge(&file, self.offset, len, self.protection, huge_page_size)?
            } else {
                PlatformMmap::new(&file, self.offset, len, self.protection, self.mapping_mode)?
            };

            #[cfg(windows)]
            let platform = if let Some(huge_page_size) = self.huge_pages {
                PlatformMmap::new_huge(file, self.offset, len, self.protection, huge_page_size)?
            } else {
                PlatformMmap::new(file, self.offset, len, self.protection)?
            };

            #[cfg(not(any(unix, windows)))]
            let platform = {
                let _ = (file, len);
                PlatformMmap::new_file_backed_unsupported()?
            };

            platform
        };

        if self.advice != MemoryAdvice::Normal {
            platform_mmap.advise(self.advice)?;
        }

        if self.prefault {
            platform_mmap.prefault()?;
        }

        Ok(Mmap::from_platform(platform_mmap))
    }
}
