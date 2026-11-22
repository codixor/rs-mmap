#[cfg(not(target_arch = "wasm32"))]
use rs_mmap::MmapOptions;
use rs_mmap::{MmapError, ReadOnlyMmap};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn from_bytes_exposes_read_only_slice_metadata() {
    let mapped = ReadOnlyMmap::from_bytes(Arc::<[u8]>::from(*b"hello"));

    assert_eq!(mapped.as_slice(), b"hello");
    assert_eq!(mapped.len(), 5);
    assert!(!mapped.is_empty());
}

#[test]
fn from_vec_clone_keeps_the_same_bytes() {
    let mapped = ReadOnlyMmap::from_vec(vec![1, 2, 3, 4]);
    let cloned = mapped.clone();

    assert_eq!(mapped.as_slice(), &[1, 2, 3, 4]);
    assert_eq!(cloned.as_ref(), &[1, 2, 3, 4]);
    assert_eq!(&*mapped, &[1, 2, 3, 4]);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn open_reads_bytes_from_a_temp_file() {
    use std::{env, fs, process};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic enough for test naming")
        .as_nanos();
    let path = env::temp_dir().join(format!("mmap-open-{}-{unique}.bin", process::id()));

    fs::write(&path, b"mapped-data").expect("temp file should be writable");
    let mapped = unsafe { ReadOnlyMmap::open(&path) }.expect("open should map the file");

    assert_eq!(mapped.as_slice(), b"mapped-data");
    assert_eq!(mapped.len(), 11);

    let _ = fs::remove_file(path);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn options_can_map_a_file_subrange() {
    use std::{env, fs, process};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic enough for test naming")
        .as_nanos();
    let path = env::temp_dir().join(format!("mmap-open-range-{}-{unique}.bin", process::id()));

    fs::write(&path, b"0123456789abcdef").expect("temp file should be writable");
    // SAFETY: this fixture is exclusively controlled for the mapping lifetime.
    let mapped = unsafe { MmapOptions::new().path(&path).offset(4).len(6).open() }
        .expect("range open should map the file segment");

    assert_eq!(mapped.as_slice(), b"456789");
    assert_eq!(mapped.len(), 6);

    let _ = fs::remove_file(path);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn options_reject_invalid_range() {
    use std::{env, fs, process};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic enough for test naming")
        .as_nanos();
    let path = env::temp_dir().join(format!("mmap-invalid-range-{}-{unique}.bin", process::id()));

    fs::write(&path, b"tiny").expect("temp file should be writable");
    // SAFETY: this fixture is exclusively controlled for the mapping lifetime.
    let error = unsafe { MmapOptions::new().path(&path).offset(2).len(8).open() }
        .expect_err("range should be rejected");

    assert!(matches!(error, MmapError::InvalidRange { .. }));

    let _ = fs::remove_file(path);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn typed_map_readonly_reads_a_file_subrange() {
    use std::{env, fs, process};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic enough for test naming")
        .as_nanos();
    let path = env::temp_dir().join(format!("mmap-typed-ro-{}-{unique}.bin", process::id()));

    fs::write(&path, b"abcdefghijklmnop").expect("temp file should be writable");
    // SAFETY: this fixture is exclusively controlled for the mapping lifetime.
    let mapped = unsafe {
        MmapOptions::new()
            .path(&path)
            .offset(3)
            .len(5)
            .map_readonly()
    }
    .expect("typed read-only mapping should succeed");

    assert_eq!(mapped.as_slice(), b"defgh");
    assert_eq!(
        mapped
            .slice(1..4)
            .expect("slice should be in bounds")
            .as_slice(),
        b"efg"
    );

    let _ = fs::remove_file(path);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn typed_map_readwrite_persists_changes() {
    use std::{env, fs, process};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic enough for test naming")
        .as_nanos();
    let path = env::temp_dir().join(format!("mmap-typed-rw-{}-{unique}.bin", process::id()));

    fs::write(&path, b"000000").expect("temp file should be writable");
    {
        // SAFETY: this fixture is exclusively controlled for the mapping lifetime.
        let mut mapped = unsafe { MmapOptions::new().path(&path).map_readwrite() }
            .expect("typed read-write mapping should succeed");
        mapped.as_mut_slice()[1..4].copy_from_slice(b"123");
    }

    assert_eq!(
        fs::read(&path).expect("file should remain readable"),
        b"012300"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn slice_returns_lifetime_bound_subview() {
    let mapped = ReadOnlyMmap::from_vec((0_u8..16).collect());
    let slice = mapped.slice(4..10).expect("slice should be in bounds");

    assert_eq!(slice.offset(), 4);
    assert_eq!(slice.len(), 6);
    assert_eq!(slice.as_slice(), &[4, 5, 6, 7, 8, 9]);
}

#[test]
fn slice_rejects_out_of_bounds_range() {
    let mapped = ReadOnlyMmap::from_vec(vec![1, 2, 3, 4]);
    let error = mapped
        .slice(2..8)
        .expect_err("out-of-bounds slice should fail");

    assert!(matches!(error, MmapError::SliceOutOfBounds { .. }));
}
