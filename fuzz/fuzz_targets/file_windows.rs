#![no_main]

use libfuzzer_sys::fuzz_target;
use rs_mmap::{MmapError, ReadOnlyFile};
use std::{io::Write, sync::OnceLock};

struct Fixture {
    source: ReadOnlyFile,
    bytes: Vec<u8>,
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let bytes: Vec<_> = (0..65_537_u32).map(|i| (i ^ (i >> 8)) as u8).collect();
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&bytes).unwrap();
        let source = ReadOnlyFile::new(file).unwrap();
        Fixture { source, bytes }
    })
}

fuzz_target!(|input: (u16, u16, u8)| {
    let (offset, len, mode) = input;
    let f = fixture();
    let len = usize::from(len);
    let offset = match mode & 3 {
        0 => u64::from(offset),
        1 => f.bytes.len() as u64 - u64::from(offset),
        2 => u64::MAX - u64::from(offset),
        _ => 0,
    };
    let view = f.source.view(offset, len);
    if len == 0 {
        assert!(matches!(view, Err(MmapError::EmptyMapping)));
    } else if let Some(end) = offset
        .checked_add(len as u64)
        .filter(|&n| n <= f.bytes.len() as u64)
    {
        assert_eq!(
            view.unwrap().as_ref(),
            &f.bytes[offset as usize..end as usize]
        );
    } else {
        assert!(matches!(view, Err(MmapError::InvalidRange { .. })));
    }

    let cap = match (mode >> 2) & 3 {
        0 => len,
        1 => 65_536,
        2 => 65_537,
        _ => usize::MAX,
    };
    f.source
        .with_suffix(cap, |suffix| {
            assert_eq!(suffix, &f.bytes[f.bytes.len().saturating_sub(cap)..]);
        })
        .unwrap();
});
