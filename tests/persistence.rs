#![cfg(any(unix, windows))]

use rs_mmap::{ByteArrayExt, MmapError, MmapOptions, ReadOnlyFile};
use std::io::{Seek, Write};
use tempfile::NamedTempFile;

#[test]
fn existing_windows_survive_overwrites_and_truncation() {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(b"0123456789").unwrap();
    let source = ReadOnlyFile::new(file.try_clone().unwrap()).unwrap();
    let window = source.view(2, 4).unwrap();
    assert!(!window.is_file_mapped());
    file.rewind().unwrap();
    file.write_all(b"abcdefghij").unwrap();
    assert_eq!(&*source.view(2, 4).unwrap(), b"cdef");
    file.set_len(1).unwrap();
    assert!(
        matches!(source.view(2, 4), Err(MmapError::Io(e)) if e.kind()==std::io::ErrorKind::UnexpectedEof)
    );
    drop(source);
    assert_eq!(&*window, b"2345");
}

#[test]
fn range_flush_preserves_bytes_outside_the_requested_region() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b"0123456789").unwrap();
    let mut map = MmapOptions::new()
        .path(file.path())
        .offset(2)
        .len(5)
        .map_readwrite()
        .unwrap();
    map.copy_from_slice(b"ABCDE");
    assert_eq!(std::fs::read(file.path()).unwrap(), b"0123456789");
    assert!(map.flush_range(5, 0).is_ok());
    assert!(map.flush_range(usize::MAX, 2).is_err());
    assert!(map.flush_range(5, 1).is_err());
    map.flush_range(1, 2).unwrap();
    drop(map);
    assert_eq!(std::fs::read(file.path()).unwrap(), b"012BC56789");
    let mut private = MmapOptions::new().path(file.path()).map_copy().unwrap();
    private.fill(0);
    drop(private);
    assert_eq!(std::fs::read(file.path()).unwrap(), b"012BC56789");
}

#[test]
fn fixed_width_utf8_validates_before_changing_bytes() {
    let mut field = [0; 4];
    field.set_str("🦀").unwrap();
    assert_eq!(field.as_slice().as_str().unwrap(), "🦀");
    assert!(field.set_str("extra").is_err());
    assert!(field.set_str("a\0").is_err());
    assert_eq!(&field, "🦀".as_bytes());
    field.set_str("é").unwrap();
    assert_eq!(&field, &[0xc3, 0xa9, 0, 0]);
    assert!([0xff].as_slice().as_str().is_err());
}

#[cfg(feature = "zerocopy-views")]
#[test]
fn typed_record_reopens_and_create_preserves_existing_files() {
    use rs_mmap::MmapStruct;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("record");
    let mut value = MmapStruct::create(&path, [1u32, 2, 3]).unwrap();
    value[2] = 19;
    value.flush().unwrap();
    drop(value);
    assert!(
        matches!(MmapStruct::create(&path, 0u32), Err(MmapError::Io(e)) if e.kind()==std::io::ErrorKind::AlreadyExists)
    );
    assert_eq!(*MmapStruct::<[u32; 3]>::open(&path).unwrap(), [1, 2, 19]);
    assert!(matches!(
        MmapStruct::<u32>::open(&path),
        Err(MmapError::InvalidViewSize { .. })
    ));
}

#[cfg(feature = "zerocopy-views")]
#[test]
fn atomic_flush_replaces_the_file_and_keeps_later_flushes_working() {
    use rs_mmap::MmapStruct;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("record");
    let mut value = MmapStruct::create(&path, [1u32, 2, 3]).unwrap();
    let earlier = std::fs::File::open(&path).unwrap();
    value[0] = 7;
    value.flush_atomic().unwrap();
    assert_eq!(*MmapStruct::<[u32; 3]>::open(&path).unwrap(), [7, 2, 3]);
    // Nothing but the record remains in the directory.
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["record"]);
    // A handle opened before the replacement still reads the old file.
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut old = Vec::new();
        (&earlier).read_to_end(&mut old).unwrap();
        assert_eq!(old, bytemuck::bytes_of(&[1u32, 2, 3]));
    }
    drop(earlier);
    // In-place and repeated atomic flushes reach the replacement file.
    value[1] = 8;
    value.flush().unwrap();
    assert_eq!(*MmapStruct::<[u32; 3]>::open(&path).unwrap(), [7, 8, 3]);
    value[2] = 9;
    value.flush_atomic().unwrap();
    value.flush_atomic().unwrap();
    drop(value);
    assert_eq!(*MmapStruct::<[u32; 3]>::open(&path).unwrap(), [7, 8, 9]);
}

#[cfg(all(feature = "zerocopy-views", unix))]
#[test]
fn atomic_flush_keeps_permissions_and_fails_cleanly() {
    use rs_mmap::MmapStruct;
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("record");
    let mut value = MmapStruct::create(&path, 5u64).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    *value = 6;
    value.flush_atomic().unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o640);
    // A vanished directory is an error and leaves the record usable in memory.
    drop(dir);
    assert!(value.flush_atomic().is_err());
    assert_eq!(*value, 6);
}

#[cfg(target_os = "linux")]
#[test]
fn kernel_sealed_files_use_direct_mapping() {
    use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
    let mut file =
        std::fs::File::from(memfd_create("rs-mmap-test", MemfdFlags::ALLOW_SEALING).unwrap());
    file.write_all(b"sealed data").unwrap();
    fcntl_add_seals(&file, SealFlags::WRITE | SealFlags::SHRINK).unwrap();
    let window = ReadOnlyFile::new(file).unwrap().view(1, 4).unwrap();
    assert!(window.is_file_mapped());
    assert_eq!(&*window, b"eale");
}

#[cfg(all(feature = "huge-pages", not(target_os = "linux")))]
#[test]
fn explicit_huge_page_requests_do_not_fall_back() {
    let options = MmapOptions::new_anonymous(2 << 20).huge_pages(rs_mmap::HugePageSize::Size2MB);
    assert!(matches!(
        options.map_readwrite(),
        Err(MmapError::UnsupportedPlatform)
    ));
}

#[test]
fn flush_rejects_a_truncated_destination() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b"abcdef").unwrap();
    let mut map = MmapOptions::new()
        .path(file.path())
        .offset(2)
        .len(4)
        .map_readwrite()
        .unwrap();
    file.as_file().set_len(2).unwrap();
    map.fill(1);
    assert!(matches!(map.flush(), Err(MmapError::InvalidRange { .. })));
    assert_eq!(std::fs::read(file.path()).unwrap(), b"ab");
}

#[test]
fn mutable_memmap2_handle_is_adopted_at_the_same_address() {
    let raw: memmap2::MmapMut = rs_mmap::compat::MmapMut::map_anon(4).unwrap();
    let addr = raw.as_ptr();
    let mut typed = rs_mmap::Mmap::<rs_mmap::ReadWrite>::from(raw);
    typed.copy_from_slice(b"data");
    assert_eq!(typed.as_ptr(), addr);
    assert_eq!(typed.as_slice(), b"data");
}

#[test]
fn anonymous_memory_has_no_writeback_destination() {
    let mut map = rs_mmap::Mmap::anonymous(12).unwrap();
    map.fill(7);
    map.flush().unwrap();
    assert_eq!(&*map, &[7; 12]);
}
