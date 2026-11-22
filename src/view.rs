//! Typed zero-copy view helpers.

/// Marker trait for plain-old-data types that are safe to reinterpret directly
/// from mapped bytes.
///
/// The bounds intentionally require both ecosystems used by the workspace:
/// `zerocopy` for layout/immutability semantics and `bytemuck` for ergonomic
/// slice casting helpers.
pub trait ZeroCopyView:
    zerocopy::KnownLayout + zerocopy::Immutable + zerocopy::FromBytes + bytemuck::Pod
{
}

impl<T> ZeroCopyView for T where
    T: zerocopy::KnownLayout + zerocopy::Immutable + zerocopy::FromBytes + bytemuck::Pod
{
}
