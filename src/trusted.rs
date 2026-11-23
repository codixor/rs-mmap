//! The only module in this crate that contains `unsafe`.
//!
//! Everything here maps a file whose stability the operating system has not
//! verified. The promise that the file stays unchanged comes from the caller
//! of an `unsafe` constructor and is carried by a [`Stable`] token, which can
//! only be created inside this module.
#![allow(unsafe_code)]

use crate::read_only_file::Stability;
use crate::{HasPath, Mmap, MmapOptions, ReadOnly, ReadOnlyFile, Result};
use std::fs::File;

/// Proof that a caller accepted the file-stability contract.
pub(crate) struct Stable(());

/// Maps a checked range of a file covered by a stability promise.
pub(crate) fn map(_: &Stable, file: &File, offset: u64, len: usize) -> Result<memmap2::Mmap> {
    // SAFETY: a `Stable` token exists only when the caller of `new_stable` or
    // `map_stable` promised that the file is neither modified nor truncated
    // while the source or any mapping derived from it is alive. The range was
    // checked against the file length by `range_len`.
    Ok(unsafe {
        memmap2::MmapOptions::new()
            .offset(offset)
            .len(len)
            .map(file)?
    })
}

impl ReadOnlyFile {
    /// Prepares a file whose views always map the original pages. No bytes
    /// are copied and the snapshot limit does not apply.
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
        Self::with_stability(file, Stability::Trusted(Stable(())))
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
        self.map_with(Stability::Trusted(Stable(())))
    }
}
