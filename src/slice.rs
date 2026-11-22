use std::ops::{Deref, Range};

use crate::{Mmap, MmapError, ReadOnly, ReadOnlyMmap, ReadWrite, Result};

#[cfg(feature = "zerocopy-views")]
fn validate_typed_view<T>(bytes: &[u8], exact_len: bool) -> Result<()> {
    let element_size = std::mem::size_of::<T>();
    if element_size == 0 {
        return Err(MmapError::InvalidConfiguration(
            "zero-sized typed views are not supported".to_string(),
        ));
    }

    let address = bytes.as_ptr() as usize;
    let alignment = std::mem::align_of::<T>();
    if !address.is_multiple_of(alignment) {
        return Err(MmapError::InvalidViewAlignment { address, alignment });
    }

    let len = bytes.len();
    if exact_len {
        if len != element_size {
            return Err(MmapError::InvalidViewSize { len, element_size });
        }
    } else if !len.is_multiple_of(element_size) {
        return Err(MmapError::InvalidViewSize { len, element_size });
    }

    Ok(())
}

/// Lifetime-bound view into a sub-range of mapped bytes.
#[derive(Debug)]
pub struct MmapSlice<'a> {
    bytes: &'a [u8],
    offset: usize,
    len: usize,
}

impl<'a> MmapSlice<'a> {
    pub(crate) fn new(bytes: &'a [u8], range: Range<usize>) -> Result<Self> {
        let start = range.start;
        let end = range.end;

        if start > end || end > bytes.len() {
            return Err(MmapError::SliceOutOfBounds {
                len: bytes.len(),
                start,
                end,
            });
        }

        Ok(Self {
            bytes,
            offset: start,
            len: end - start,
        })
    }

    /// Returns the slice contents.
    #[must_use]
    pub fn as_slice(&self) -> &'a [u8] {
        &self.bytes[self.offset..self.offset + self.len]
    }

    /// Returns the view length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns whether the view is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the start offset within the parent mapping.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Returns the view contents as a typed reference.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the slice length or alignment are not valid
    /// for `T`.
    #[cfg(feature = "zerocopy-views")]
    pub fn view<T>(&self) -> Result<&T>
    where
        T: crate::ZeroCopyView,
    {
        let bytes = self.as_slice();
        validate_typed_view::<T>(bytes, true)?;
        Ok(bytemuck::from_bytes(bytes))
    }

    /// Returns the view contents as a typed slice.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the slice length or alignment are not valid
    /// for `T`.
    #[cfg(feature = "zerocopy-views")]
    pub fn view_slice<T>(&self) -> Result<&[T]>
    where
        T: crate::ZeroCopyView,
    {
        let bytes = self.as_slice();
        validate_typed_view::<T>(bytes, false)?;
        Ok(bytemuck::cast_slice(bytes))
    }
}

impl Deref for MmapSlice<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl ReadOnlyMmap {
    /// Returns a lifetime-bound sub-view into this mapping.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError::SliceOutOfBounds`] if the requested range lies
    /// outside the mapped byte slice.
    pub fn slice(&self, range: Range<usize>) -> Result<MmapSlice<'_>> {
        MmapSlice::new(self.as_slice(), range)
    }

    /// Returns a sub-view from `start` to the end of the mapping.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError::SliceOutOfBounds`] if `start` exceeds the mapped
    /// length.
    pub fn slice_from(&self, start: usize) -> Result<MmapSlice<'_>> {
        self.slice(start..self.len())
    }

    /// Returns a sub-view from the start of the mapping to `end`.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError::SliceOutOfBounds`] if `end` exceeds the mapped
    /// length.
    pub fn slice_to(&self, end: usize) -> Result<MmapSlice<'_>> {
        self.slice(0..end)
    }

    /// Returns the entire mapping as a typed reference.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the mapping length or alignment are not valid
    /// for `T`.
    #[cfg(feature = "zerocopy-views")]
    pub fn view<T>(&self) -> Result<&T>
    where
        T: crate::ZeroCopyView,
    {
        let bytes = self.as_slice();
        validate_typed_view::<T>(bytes, true)?;
        Ok(bytemuck::from_bytes(bytes))
    }

    /// Returns the entire mapping as a typed slice.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the mapping length or alignment are not valid
    /// for `T`.
    #[cfg(feature = "zerocopy-views")]
    pub fn view_slice<T>(&self) -> Result<&[T]>
    where
        T: crate::ZeroCopyView,
    {
        let bytes = self.as_slice();
        validate_typed_view::<T>(bytes, false)?;
        Ok(bytemuck::cast_slice(bytes))
    }
}

impl Mmap<ReadOnly> {
    /// Returns a lifetime-bound sub-view into this mapping.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError::SliceOutOfBounds`] if the requested range lies
    /// outside the mapped byte slice.
    pub fn slice(&self, range: Range<usize>) -> Result<MmapSlice<'_>> {
        MmapSlice::new(self.as_slice(), range)
    }

    /// Returns a sub-view from `start` to the end of the mapping.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError::SliceOutOfBounds`] if `start` exceeds the mapped
    /// length.
    pub fn slice_from(&self, start: usize) -> Result<MmapSlice<'_>> {
        self.slice(start..self.len())
    }

    /// Returns a sub-view from the start of the mapping to `end`.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError::SliceOutOfBounds`] if `end` exceeds the mapped
    /// length.
    pub fn slice_to(&self, end: usize) -> Result<MmapSlice<'_>> {
        self.slice(0..end)
    }

    /// Returns the entire mapping as a typed reference.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the mapping length or alignment are not valid
    /// for `T`.
    #[cfg(feature = "zerocopy-views")]
    pub fn view<T>(&self) -> Result<&T>
    where
        T: crate::ZeroCopyView,
    {
        let bytes = self.as_slice();
        validate_typed_view::<T>(bytes, true)?;
        Ok(bytemuck::from_bytes(bytes))
    }

    /// Returns the entire mapping as a typed slice.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the mapping length or alignment are not valid
    /// for `T`.
    #[cfg(feature = "zerocopy-views")]
    pub fn view_slice<T>(&self) -> Result<&[T]>
    where
        T: crate::ZeroCopyView,
    {
        let bytes = self.as_slice();
        validate_typed_view::<T>(bytes, false)?;
        Ok(bytemuck::cast_slice(bytes))
    }
}

impl Mmap<ReadWrite> {
    /// Returns a lifetime-bound immutable sub-view into this mapping.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError::SliceOutOfBounds`] if the requested range lies
    /// outside the mapped byte slice.
    pub fn slice(&self, range: Range<usize>) -> Result<MmapSlice<'_>> {
        MmapSlice::new(&self[..], range)
    }

    /// Returns a sub-view from `start` to the end of the mapping.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError::SliceOutOfBounds`] if `start` exceeds the mapped
    /// length.
    pub fn slice_from(&self, start: usize) -> Result<MmapSlice<'_>> {
        self.slice(start..self.len())
    }

    /// Returns a sub-view from the start of the mapping to `end`.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError::SliceOutOfBounds`] if `end` exceeds the mapped
    /// length.
    pub fn slice_to(&self, end: usize) -> Result<MmapSlice<'_>> {
        self.slice(0..end)
    }

    /// Returns the entire mapping as a typed reference.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the mapping length or alignment are not valid
    /// for `T`.
    #[cfg(feature = "zerocopy-views")]
    pub fn view<T>(&self) -> Result<&T>
    where
        T: crate::ZeroCopyView,
    {
        let bytes = &self[..];
        validate_typed_view::<T>(bytes, true)?;
        Ok(bytemuck::from_bytes(bytes))
    }

    /// Returns the entire mapping as a typed slice.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the mapping length or alignment are not valid
    /// for `T`.
    #[cfg(feature = "zerocopy-views")]
    pub fn view_slice<T>(&self) -> Result<&[T]>
    where
        T: crate::ZeroCopyView,
    {
        let bytes = &self[..];
        validate_typed_view::<T>(bytes, false)?;
        Ok(bytemuck::cast_slice(bytes))
    }

    /// Returns the entire mutable mapping as a typed mutable reference.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the mapping length or alignment are not valid
    /// for `T`.
    #[cfg(feature = "zerocopy-views")]
    pub fn view_mut<T>(&mut self) -> Result<&mut T>
    where
        T: crate::ZeroCopyView,
    {
        let bytes = self.as_mut_slice();
        validate_typed_view::<T>(bytes, true)?;
        Ok(bytemuck::from_bytes_mut(bytes))
    }

    /// Returns the entire mutable mapping as a typed mutable slice.
    ///
    /// # Errors
    ///
    /// Returns [`MmapError`] if the mapping length or alignment are not valid
    /// for `T`.
    #[cfg(feature = "zerocopy-views")]
    pub fn view_slice_mut<T>(&mut self) -> Result<&mut [T]>
    where
        T: crate::ZeroCopyView,
    {
        let bytes = self.as_mut_slice();
        validate_typed_view::<T>(bytes, false)?;
        Ok(bytemuck::cast_slice_mut(bytes))
    }
}
