#![cfg(any(unix, windows))]

use rs_mmap::{MmapError, ReadOnlyFile, ReadOnlyMmap};
use std::io::Write;

#[test]
fn suffix_reads_exact_bytes_without_moving_the_file_cursor() {
    use std::io::{Seek, SeekFrom};

    let data: Vec<_> = (0..160_000usize).map(|i| i.to_le_bytes()[0]).collect();
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&data).unwrap();
    file.seek(SeekFrom::Start(19)).unwrap();
    let mut cursor = file.try_clone().unwrap();
    // SAFETY: the file is stable and the duplicate only queries the cursor.
    let source = unsafe { ReadOnlyFile::new(file) }.unwrap();
    for max_len in [0, 1, 65_535, 65_536, 65_537, data.len(), usize::MAX] {
        let expected = &data[data.len().saturating_sub(max_len)..];
        source
            .with_suffix(max_len, |bytes| assert_eq!(bytes, expected))
            .unwrap();
        assert_eq!(cursor.stream_position().unwrap(), 19);
    }
}

#[test]
fn overlapping_views_outlive_their_source() {
    let mut file = tempfile::tempfile().unwrap();
    let data: Vec<_> = (0..150_000usize).map(|i| i.to_le_bytes()[0]).collect();
    file.write_all(&data).unwrap();
    // SAFETY: the test controls the backing file and never changes it again.
    let source = unsafe { ReadOnlyFile::new(file) }.unwrap();
    let left = source.view(65_533, 9).unwrap();
    let right = source.view(65_535, 15).unwrap();
    assert!(matches!(source.view(0, 0), Err(MmapError::EmptyMapping)));
    assert!(matches!(
        source.view(u64::MAX, 2),
        Err(MmapError::InvalidRange { .. })
    ));
    assert!(matches!(
        source.view(149_999, 2),
        Err(MmapError::InvalidRange { .. })
    ));
    drop(source);
    assert_eq!(left.as_ref(), &data[65_533..65_542]);
    drop(left);
    assert_eq!(right.as_ref(), &data[65_535..65_550]);
}

#[test]
fn imported_map_preserves_address_and_contents() {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(b"immutable").unwrap();
    // SAFETY: the sole writer makes no further changes.
    let map = unsafe { memmap2::Mmap::map(&file) }.unwrap();
    let address = map.as_ptr();
    let shared = ReadOnlyMmap::from(map);
    let second = shared.clone();
    drop(shared);
    assert_eq!(address, second.as_ptr());
    assert_eq!(second.as_slice(), b"immutable");
}

#[cfg(unix)]
#[test]
fn advice_uses_view_relative_ranges_on_unaligned_windows() {
    use rs_mmap::MemoryAdvice;

    let mut file = tempfile::tempfile().unwrap();
    let data: Vec<_> = (0..150_000usize).map(|i| i.to_le_bytes()[0]).collect();
    file.write_all(&data).unwrap();
    // SAFETY: the test owns the file and has finished writing.
    let source = unsafe { ReadOnlyFile::new(file) }.unwrap();
    let view = source.view(17, 130_001).unwrap();
    drop(source);
    view.advise_range(13..view.len(), MemoryAdvice::Sequential)
        .unwrap();
    view.advise_range(view.len() - 1..view.len(), MemoryAdvice::WillNeed)
        .unwrap();
    view.advise_range(view.len()..view.len(), MemoryAdvice::Normal)
        .unwrap();
    for (start, end) in [(12, 11), (0, usize::MAX), (view.len() + 1, view.len() + 1)] {
        assert!(matches!(
            view.advise_range(start..end, MemoryAdvice::Normal),
            Err(MmapError::SliceOutOfBounds { start: s, end: e, .. }) if (s,e) == (start,end)
        ));
    }
    assert_eq!(&*view, &data[17..130_018]);
}

#[cfg(target_os = "linux")]
#[test]
fn cold_and_pageout_preserve_file_contents() {
    use rs_mmap::MemoryAdvice;

    let mut file = tempfile::tempfile().unwrap();
    let data = vec![0x65; 32_768];
    file.write_all(&data).unwrap();
    file.sync_all().unwrap();
    // SAFETY: the test owns the file and has finished writing.
    let source = unsafe { ReadOnlyFile::new(file) }.unwrap();
    let view = source.view(7, 24_000).unwrap();
    for (advice, raw) in [
        (MemoryAdvice::Cold, libc::MADV_COLD),
        (MemoryAdvice::PageOut, libc::MADV_PAGEOUT),
    ] {
        // SAFETY: Linux documents this zero-length call as a capability probe.
        if unsafe { libc::madvise(std::ptr::null_mut(), 0, raw) } == 0 {
            view.advise_range(3..view.len(), advice).unwrap();
        } else {
            let expected = std::io::Error::last_os_error().raw_os_error();
            assert!(matches!(view.advise_range(3..view.len(), advice),
                Err(MmapError::Io(e)) if e.raw_os_error() == expected));
        }
        assert_eq!(&*view, &data[7..24_007]);
    }
}

#[cfg(windows)]
#[test]
fn imports_a_shorter_section_and_keeps_its_file_alive() {
    use std::fs::File;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows::Win32::{
        Foundation::{DUPLICATE_HANDLE_OPTIONS, DuplicateHandle, HANDLE},
        Storage::FileSystem::FILE_READ_ATTRIBUTES,
        System::Memory::{CreateFileMappingW, PAGE_READONLY},
        System::Threading::GetCurrentProcess,
    };

    const EXTENT: u32 = 90_000;
    let mut file = tempfile::tempfile().unwrap();
    let data: Vec<_> = (0..150_000usize).map(|i| i.to_le_bytes()[0]).collect();
    file.write_all(&data).unwrap();
    // SAFETY: file is valid, EXTENT is within it, and the returned section is owned.
    let section = unsafe {
        let handle = CreateFileMappingW(
            HANDLE(file.as_raw_handle()),
            None,
            PAGE_READONLY,
            0,
            EXTENT,
            None,
        )
        .unwrap();
        OwnedHandle::from_raw_handle(handle.0)
    };
    // Keep only a metadata handle. Reopening a read-only section from this
    // handle would fail, so a successful view must use the supplied section.
    // SAFETY: file is open in this process. A successful duplication transfers
    // one independently owned file handle into metadata_file.
    let metadata_file = unsafe {
        let process = GetCurrentProcess();
        let mut handle = HANDLE::default();
        DuplicateHandle(
            process,
            HANDLE(file.as_raw_handle()),
            process,
            &raw mut handle,
            FILE_READ_ATTRIBUTES.0,
            false,
            DUPLICATE_HANDLE_OPTIONS::default(),
        )
        .unwrap();
        File::from_raw_handle(handle.0)
    };
    drop(file);
    // SAFETY: the section above maps this file, and the test has finished writing.
    let source =
        unsafe { ReadOnlyFile::from_section(section, metadata_file, EXTENT.into()) }.unwrap();
    source
        .with_suffix(13, |bytes| assert_eq!(bytes, &data[89_987..90_000]))
        .unwrap();
    let view = source.view(65_535, 16).unwrap();
    assert!(matches!(
        source.view(89_999, 2),
        Err(MmapError::InvalidRange {
            file_len: 90_000,
            ..
        })
    ));
    drop(source);
    assert_eq!(&*view, &data[65_535..65_551]);
}
