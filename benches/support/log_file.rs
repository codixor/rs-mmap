use std::{fs::File, io, path::PathBuf};

#[cfg(windows)]
pub(super) fn create() -> io::Result<(File, PathBuf)> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::FILE_FLAG_DELETE_ON_CLOSE;

    // Only the kernel removes the name, even if the process is killed.
    let named = tempfile::Builder::new()
        .prefix("rs-mmap-log-")
        .disable_cleanup(true)
        .make(|path| {
            File::options()
                .read(true)
                .write(true)
                .create_new(true)
                .custom_flags(FILE_FLAG_DELETE_ON_CLOSE.0)
                .open(path)
        })?;
    let path = named.path().to_owned();
    Ok((named.into_file(), path))
}

#[cfg(unix)]
pub(super) fn create() -> io::Result<(File, PathBuf)> {
    use std::os::fd::AsRawFd;

    let file = tempfile::tempfile()?;
    let fd_dir = if cfg!(target_os = "linux") {
        "/proc/self/fd"
    } else {
        "/dev/fd"
    };
    let path = PathBuf::from(fd_dir).join(file.as_raw_fd().to_string());
    // Check that both backends can reopen the unnamed file before filling it.
    File::open(&path)?;
    Ok((file, path))
}
