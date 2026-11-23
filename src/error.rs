use std::io;

use thiserror::Error;

/// Errors exposed by the `rs-mmap` crate.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MmapError {
    /// Underlying I/O failure while opening or inspecting a file.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// File-backed mapping is unavailable on the current target.
    #[error("operation is unavailable on this platform or backing")]
    UnsupportedPlatform,
    /// The requested mapping would be empty.
    #[error("mapping length must be greater than zero")]
    EmptyMapping,
    /// The requested file range is outside the current file length.
    #[error(
        "requested mapping range is invalid: offset {offset} + len {len} exceeds file length {file_len}"
    )]
    InvalidRange {
        /// Requested starting offset.
        offset: u64,
        /// Requested mapping length.
        len: usize,
        /// Current file length.
        file_len: u64,
    },
    /// A requested sub-slice is outside the mapped byte range.
    #[error("requested slice {start}..{end} is out of bounds for mapping length {len}")]
    SliceOutOfBounds {
        /// Total mapped length.
        len: usize,
        /// Requested slice start.
        start: usize,
        /// Requested slice end.
        end: usize,
    },
    /// Typed view length does not match the requested element layout.
    #[error("typed view length {len} is invalid for element size {element_size}")]
    InvalidViewSize {
        /// Total byte length being reinterpreted.
        len: usize,
        /// Size of the target element type.
        element_size: usize,
    },
    /// Typed view start address is not properly aligned.
    #[error("typed view address {address:#x} is not aligned to {alignment}")]
    InvalidViewAlignment {
        /// Raw starting address of the view.
        address: usize,
        /// Required alignment for the target element type.
        alignment: usize,
    },
    /// Invalid mapping configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),
}

impl From<MmapError> for io::Error {
    fn from(err: MmapError) -> Self {
        match err {
            MmapError::Io(error) => error,
            MmapError::UnsupportedPlatform => io::Error::new(io::ErrorKind::Unsupported, err),
            other => io::Error::new(io::ErrorKind::InvalidInput, other),
        }
    }
}
