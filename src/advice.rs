//! Memory access pattern advice hints for optimization.

/// Memory advice hints to optimize I/O performance.
///
/// These hints inform the OS about expected memory access patterns,
/// allowing it to optimize page caching and prefetching strategies.
#[cfg_attr(
    feature = "serde-support",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MemoryAdvice {
    /// Normal access pattern (default)
    Normal,

    /// Expect random access
    Random,

    /// Expect sequential access
    Sequential,

    /// Will need these pages soon
    WillNeed,
}

impl Default for MemoryAdvice {
    fn default() -> Self {
        Self::Normal
    }
}
