use crate::{Mmap, MmapError, ReadWrite, Result, ZeroCopyView};
use std::{
    fs::File,
    io::Write,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    path::Path,
};

/// A file containing exactly one plain-data value in its native byte layout.
/// Use an explicit stable layout and byte order for records shared across applications.
/// Changes become durable after a successful flush. Coordinate concurrent writers.
pub struct MmapStruct<T: ZeroCopyView> {
    map: Mmap<ReadWrite>,
    record: PhantomData<T>,
}
impl<T: ZeroCopyView> MmapStruct<T> {
    /// Creates a new file containing `initial`. An existing path is never overwritten.
    pub fn create(path: impl AsRef<Path>, initial: T) -> Result<Self> {
        Self::size()?;
        let mut file = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(bytemuck::bytes_of(&initial))?;
        Self::from_file(file)
    }
    /// Opens an existing record for explicit write-back.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_file(File::options().read(true).write(true).open(path)?)
    }
    fn size() -> Result<usize> {
        let size = size_of::<T>();
        if size == 0 {
            return Err(MmapError::InvalidConfiguration(
                "zero-sized records are unsupported".into(),
            ));
        }
        Ok(size)
    }
    fn from_file(file: File) -> Result<Self> {
        let size = Self::size()?;
        let len = file.metadata()?.len();
        if len != size as u64 {
            return Err(MmapError::InvalidViewSize {
                len: usize::try_from(len).unwrap_or(usize::MAX),
                element_size: size,
            });
        }
        let map = Mmap::snapshot(file, 0, Some(size), true, usize::MAX)?;
        map.view::<T>()?;
        Ok(Self {
            map,
            record: PhantomData,
        })
    }
    /// Writes the record back and synchronizes its file.
    pub fn flush(&self) -> Result<()> {
        self.map.flush()
    }
}
impl<T: ZeroCopyView> Deref for MmapStruct<T> {
    type Target = T;
    fn deref(&self) -> &T {
        bytemuck::from_bytes(&self.map)
    }
}
impl<T: ZeroCopyView> DerefMut for MmapStruct<T> {
    fn deref_mut(&mut self) -> &mut T {
        bytemuck::from_bytes_mut(&mut self.map)
    }
}
