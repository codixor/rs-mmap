use crate::Result;
use std::{fs::File, io};

/// Mode for an advisory file lock.
#[cfg_attr(
    feature = "serde-support",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LockType {
    Shared,
    Exclusive,
}

/// Unlocks the file when dropped.
pub struct FileLock {
    file: Option<File>,
    lock_type: LockType,
}
// The handle is taken only when consuming the guard in unlock.
#[allow(
    clippy::missing_panics_doc,
    reason = "a live guard always contains its file"
)]
impl FileLock {
    /// Blocks until the lock is acquired.
    pub fn lock(file: File, lock_type: LockType) -> Result<Self> {
        match lock_type {
            LockType::Shared => file.lock_shared()?,
            LockType::Exclusive => file.lock()?,
        }
        Ok(Self {
            file: Some(file),
            lock_type,
        })
    }
    /// Returns an I/O error with kind WouldBlock if the file is locked elsewhere.
    pub fn try_lock(file: File, lock_type: LockType) -> Result<Self> {
        match lock_type {
            LockType::Shared => file.try_lock_shared(),
            LockType::Exclusive => file.try_lock(),
        }
        .map_err(io::Error::from)?;
        Ok(Self {
            file: Some(file),
            lock_type,
        })
    }
    pub fn file(&self) -> &File {
        self.file.as_ref().expect("live file lock")
    }
    pub fn lock_type(&self) -> LockType {
        self.lock_type
    }
    /// Releases the lock and returns its handle.
    pub fn unlock(mut self) -> Result<File> {
        self.file().unlock()?;
        Ok(self.file.take().expect("live file lock"))
    }
}
impl Drop for FileLock {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            let _ = file.unlock();
        }
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"test data").unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_shared_lock() {
        let file = create_test_file();
        let file_handle = std::fs::File::open(file.path()).unwrap();

        let lock = FileLock::lock(file_handle, LockType::Shared).unwrap();
        assert_eq!(lock.lock_type(), LockType::Shared);
    }

    #[test]
    fn test_exclusive_lock() {
        let file = create_test_file();
        let file_handle = std::fs::File::open(file.path()).unwrap();

        let lock = FileLock::lock(file_handle, LockType::Exclusive).unwrap();
        assert_eq!(lock.lock_type(), LockType::Exclusive);
    }

    #[test]
    fn test_try_lock_success() {
        let file = create_test_file();
        let file_handle = std::fs::File::open(file.path()).unwrap();

        let lock = FileLock::try_lock(file_handle, LockType::Shared).unwrap();
        assert_eq!(lock.lock_type(), LockType::Shared);
    }

    #[test]
    fn test_lock_unlock() {
        let file = create_test_file();
        let file_handle = std::fs::File::open(file.path()).unwrap();

        let lock = FileLock::lock(file_handle, LockType::Exclusive).unwrap();
        let _file = lock.unlock().unwrap();
        // Lock should be released now
    }

    #[test]
    fn test_multiple_shared_locks() {
        let file = create_test_file();
        let path = file.path().to_path_buf();

        let file1 = std::fs::File::open(&path).unwrap();
        let file2 = std::fs::File::open(&path).unwrap();

        let _lock1 = FileLock::lock(file1, LockType::Shared).unwrap();
        let _lock2 = FileLock::lock(file2, LockType::Shared).unwrap();
        // Both shared locks should succeed
    }
}
