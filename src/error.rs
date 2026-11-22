use std::{io, path::PathBuf};

use thiserror::Error;

/// Errors exposed by the `rs-mmap` crate.
#[derive(Debug, Error)]
pub enum MmapError {
    /// Underlying I/O failure while opening or inspecting a file.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// File-backed mapping is unavailable on the current target.
    #[error("read-only file mapping is not available on this platform")]
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
    /// Invalid alignment.
    #[error("invalid alignment: size {size} not aligned to {alignment}")]
    InvalidAlignment {
        /// The size that failed alignment.
        size: usize,
        /// The required alignment.
        alignment: usize,
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
    /// Permission denied for an operation.
    #[error("permission denied for {operation}")]
    PermissionDenied {
        /// Description of the operation that was denied.
        operation: String,
    },
    /// Huge pages are not supported on this platform or host.
    #[error("huge pages not supported on this platform")]
    HugePagesUnsupported,
    /// Address space or system allocation failure.
    #[error("address space exhausted")]
    OutOfMemory,
    /// The file backing a mapping was truncated.
    #[error("file truncated while mapped: {0}")]
    FileTruncated(PathBuf),
    /// Invalid mapping configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),
    /// Platform-specific system error.
    #[error("system error: {0}")]
    SystemError(String),
    /// Mapping failed.
    #[error("mapping failed: {0}")]
    MappingFailed(String),
}

impl From<MmapError> for io::Error {
    fn from(err: MmapError) -> Self {
        match err {
            MmapError::Io(error) => error,
            other => io::Error::other(other),
        }
    }
}
