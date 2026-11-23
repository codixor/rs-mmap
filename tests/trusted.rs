#![cfg(any(unix, windows))]
#![allow(unsafe_code)]

use rs_mmap::{DEFAULT_MAX_SNAPSHOT, MmapError, MmapOptions, ReadOnlyFile};
use std::io::Write;
use tempfile::NamedTempFile;

fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i ^ (i >> 8)).to_le_bytes()[0]).collect()
}

#[test]
fn stable_views_map_the_file_and_outlive_their_source() {
    let data = pattern(150_000);
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&data).unwrap();
    // SAFETY: this test owns the only handle and has finished writing.
    let source = unsafe { ReadOnlyFile::new_stable(file) }.unwrap();
    let left = source.view(65_533, 9).unwrap();
    let right = source.view(65_535, 15).unwrap();
    assert!(left.is_file_mapped() && right.is_file_mapped());
    assert!(matches!(source.view(0, 0), Err(MmapError::EmptyMapping)));
    assert!(matches!(
        source.view(149_999, 2),
        Err(MmapError::InvalidRange { .. })
    ));
    assert!(matches!(
        source.view(u64::MAX, 2),
        Err(MmapError::InvalidRange { .. })
    ));
    source
        .with_suffix(7, |tail| assert_eq!(tail, &data[149_993..]))
        .unwrap();
    drop(source);
    assert_eq!(&*left, &data[65_533..65_542]);
    drop(left);
    assert_eq!(&*right, &data[65_535..65_550]);
}

#[test]
fn stable_sources_ignore_the_snapshot_limit() {
    let data = pattern(4096);
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&data).unwrap();
    // SAFETY: this test owns the only handle and has finished writing.
    let source = unsafe { ReadOnlyFile::new_stable(file) }
        .unwrap()
        .max_snapshot(1);
    assert_eq!(&*source.view(0, 4096).unwrap(), &data[..]);
}

#[test]
fn snapshot_limit_bounds_copied_windows() {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(b"0123456789").unwrap();
    let source = ReadOnlyFile::new(file).unwrap().max_snapshot(4);
    let window = source.view(3, 4).unwrap();
    assert!(!window.is_file_mapped());
    assert_eq!(&*window, b"3456");
    assert!(matches!(
        source.view(3, 5),
        Err(MmapError::SnapshotTooLarge { len: 5, limit: 4 })
    ));
    assert!(matches!(
        source.with_suffix(5, |_| ()),
        Err(MmapError::SnapshotTooLarge { len: 5, limit: 4 })
    ));
    assert_eq!(source.with_suffix(4, <[u8]>::to_vec).unwrap(), b"6789");
}

#[test]
fn builder_applies_the_snapshot_limit_to_every_copy() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b"0123456789").unwrap();
    let options = || MmapOptions::new().path(file.path()).max_snapshot(9);
    let too_large = |r: Option<MmapError>| {
        assert!(matches!(
            r,
            Some(MmapError::SnapshotTooLarge { len: 10, limit: 9 })
        ));
    };
    too_large(options().open().err());
    too_large(options().map_readonly().err());
    too_large(options().map_readwrite().err());
    too_large(options().map_copy().err());
    assert_eq!(&*options().offset(1).open().unwrap(), b"123456789");
    assert_eq!(&*options().len(9).map_readwrite().unwrap(), b"012345678");
    // SAFETY: the file is complete and only read for the rest of this test.
    let mapped = unsafe { options().max_snapshot(0).offset(2).len(5).map_stable() }.unwrap();
    assert_eq!(&*mapped, b"23456");
    assert_eq!(DEFAULT_MAX_SNAPSHOT, 64 << 20);
}

#[test]
fn stable_mapping_shares_bytes_with_read_only_handles() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b"shared bytes").unwrap();
    // SAFETY: the file is complete and only read for the rest of this test.
    let mapped = unsafe { MmapOptions::new().path(file.path()).map_stable() }.unwrap();
    let address = mapped.as_ptr();
    let shared = mapped.into_read_only();
    let second = shared.clone();
    drop(shared);
    assert_eq!(second.as_ptr(), address);
    assert_eq!(&*second, b"shared bytes");
}

#[cfg(unix)]
#[test]
fn stable_views_accept_view_relative_advice() {
    use rs_mmap::MemoryAdvice;
    let data = pattern(150_000);
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&data).unwrap();
    // SAFETY: this test owns the only handle and has finished writing.
    let view = unsafe { ReadOnlyFile::new_stable(file) }
        .unwrap()
        .view(17, 130_001)
        .unwrap();
    view.advise_range(13..view.len(), MemoryAdvice::Sequential)
        .unwrap();
    view.advise_range(0..1, MemoryAdvice::WillNeed).unwrap();
    assert!(matches!(
        view.advise_range(0..1, MemoryAdvice::DontNeed),
        Err(MmapError::UnsupportedPlatform)
    ));
    assert_eq!(&*view, &data[17..130_018]);
}

#[cfg(target_os = "linux")]
#[test]
fn verified_mappings_are_not_limited() {
    use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
    let mut file =
        std::fs::File::from(memfd_create("rs-mmap-limit", MemfdFlags::ALLOW_SEALING).unwrap());
    file.write_all(b"sealed data").unwrap();
    fcntl_add_seals(&file, SealFlags::WRITE | SealFlags::SHRINK).unwrap();
    let window = ReadOnlyFile::new(file)
        .unwrap()
        .max_snapshot(1)
        .view(0, 11)
        .unwrap();
    assert!(window.is_file_mapped());
    assert_eq!(&*window, b"sealed data");
}
