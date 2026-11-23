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
    let source = ReadOnlyFile::new(file).unwrap();
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
    let source = ReadOnlyFile::new(file).unwrap();
    let left = source.view(65_533, 9).unwrap();
    let right = source.view(65_535, 15).unwrap();
    assert!(matches!(source.view(0, 0), Err(MmapError::EmptyMapping)));
    assert!(matches!(
        source.view(u64::MAX, 0),
        Err(MmapError::EmptyMapping)
    ));
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
    let mut map = rs_mmap::compat::MmapMut::map_anon(9).unwrap();
    map.copy_from_slice(b"immutable");
    let map = map.make_read_only().unwrap();
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
    let source = ReadOnlyFile::new(file).unwrap();
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
