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

    /// Won't need these pages
    DontNeed,

    /// Mark these pages cold so the kernel can reclaim them sooner (Linux 5.4+).
    #[cfg(target_os = "linux")]
    Cold,

    /// Request reclamation of these pages (Linux 5.4+). Later accesses may
    /// fault the data back in. The kernel may ignore the request.
    #[cfg(target_os = "linux")]
    PageOut,
}

impl Default for MemoryAdvice {
    fn default() -> Self {
        Self::Normal
    }
}
