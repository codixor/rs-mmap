#![cfg(any(unix, windows))]

use rs_mmap::{MmapError, ReadOnlyFile, ReadOnlyMmap};
use std::io::Write;

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
