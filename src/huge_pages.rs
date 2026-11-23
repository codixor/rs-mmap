//! Huge page support for better TLB performance.
//!
//! Huge pages (also called large pages or superpages) reduce TLB pressure
//! by using larger page sizes (2MB or 1GB instead of 4KB).

/// Huge page size options.
///
/// Explicit anonymous huge pages are supported on configured Linux hosts.
#[cfg_attr(
    feature = "serde-support",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HugePageSize {
    /// 2 MiB pages.
    Size2MB,

    /// 1 GiB pages.
    Size1GB,
}

impl HugePageSize {
    /// Get the size in bytes
    pub const fn size_bytes(self) -> usize {
        match self {
            Self::Size2MB => 2 * 1024 * 1024,
            Self::Size1GB => 1024 * 1024 * 1024,
        }
    }
}

impl Default for HugePageSize {
    fn default() -> Self {
        Self::Size2MB
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_huge_page_sizes() {
        assert_eq!(HugePageSize::Size2MB.size_bytes(), 2 * 1024 * 1024);
        assert_eq!(HugePageSize::Size1GB.size_bytes(), 1024 * 1024 * 1024);
    }

    #[test]
    fn test_default() {
        assert_eq!(HugePageSize::default(), HugePageSize::Size2MB);
    }
}
