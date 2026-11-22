use rs_mmap::ReadOnlyMmap;
use std::sync::Arc;

fn main() {
    let mapped = ReadOnlyMmap::from_bytes(Arc::<[u8]>::from(*b"hello from mmap"));
    println!("len={} bytes={:?}", mapped.len(), mapped.as_slice());
}
