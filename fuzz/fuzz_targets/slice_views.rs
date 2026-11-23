#![no_main]

use libfuzzer_sys::fuzz_target;
use rs_mmap::{MmapError, ReadOnlyMmap};

fuzz_target!(|data: &[u8]| {
    let Some((&[start, end], bytes)) = data.split_first_chunk::<2>() else {
        return;
    };
    let start = usize::from(start);
    let end = usize::from(end);
    let map = ReadOnlyMmap::from_vec(bytes.to_vec());
    let actual = map.slice(start..end);
    match bytes.get(start..end) {
        None => assert!(matches!(actual, Err(MmapError::SliceOutOfBounds { .. }))),
        Some(expected) => {
            let view = actual.unwrap();
            assert_eq!(view.offset(), start);
            assert_eq!(view.as_slice(), expected);
            assert_eq!(view.view_slice::<u8>().unwrap(), expected);

            let aligned = view.as_slice().as_ptr().cast::<u32>().is_aligned();
            let words = view.view_slice::<u32>();
            assert_eq!(words.is_ok(), aligned && expected.len().is_multiple_of(4));
            if let Ok(words) = words {
                for (word, bytes) in words.iter().zip(expected.as_chunks::<4>().0) {
                    assert_eq!(word.to_ne_bytes(), *bytes);
                }
            }
        }
    }
});
