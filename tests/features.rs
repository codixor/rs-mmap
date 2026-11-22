#[cfg(feature = "serde-support")]
use rs_mmap::MmapOptions;
#[cfg(feature = "zerocopy-views")]
use rs_mmap::{MmapError, ReadOnlyMmap};

#[cfg(feature = "serde-support")]
#[test]
fn mmap_options_round_trip_with_serde() {
    let options = MmapOptions::new()
        .path("data.bin")
        .offset(128)
        .len(4096)
        .populate();

    let json = serde_json::to_string(&options).expect("serialize mmap options");
    let decoded: MmapOptions<rs_mmap::HasPath> =
        serde_json::from_str(&json).expect("deserialize mmap options");

    let round_trip = serde_json::to_value(decoded).expect("round-trip to value");
    assert_eq!(round_trip["path"], "data.bin");
    assert_eq!(round_trip["offset"], 128);
    assert_eq!(round_trip["len"], 4096);
    assert_eq!(round_trip["prefault"], true);
}

#[cfg(feature = "zerocopy-views")]
#[test]
fn readonly_map_can_expose_typed_views() {
    let bytes = 0x1234_5678_u32.to_ne_bytes().to_vec();
    let mapped = ReadOnlyMmap::from_vec(bytes);

    let value = mapped.view::<u32>().expect("typed view should succeed");
    assert_eq!(*value, 0x1234_5678);
}

#[cfg(feature = "zerocopy-views")]
#[test]
fn readonly_map_can_expose_typed_slices() {
    let mut bytes = Vec::new();
    for value in [1_u32, 2, 3, 4] {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }

    let mapped = ReadOnlyMmap::from_vec(bytes);
    let values = mapped
        .view_slice::<u32>()
        .expect("typed slice view should succeed");

    assert_eq!(values, &[1, 2, 3, 4]);
}

#[cfg(feature = "zerocopy-views")]
#[test]
fn typed_view_rejects_invalid_lengths() {
    let mapped = ReadOnlyMmap::from_vec(vec![1, 2, 3]);
    let error = mapped
        .view::<u32>()
        .expect_err("typed view with the wrong length should fail");

    assert!(matches!(error, MmapError::InvalidViewSize { .. }));
}

#[cfg(feature = "zerocopy-views")]
#[test]
fn typed_view_rejects_misaligned_slices() {
    let mut bytes = vec![0_u8];
    bytes.extend_from_slice(&42_u32.to_ne_bytes());
    let mapped = ReadOnlyMmap::from_vec(bytes);
    let slice = mapped.slice(1..5).expect("sub-slice should be in bounds");
    let error = slice
        .view::<u32>()
        .expect_err("unaligned typed view should fail");

    assert!(matches!(error, MmapError::InvalidViewAlignment { .. }));
}
