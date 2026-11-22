use rs_mmap::ReadOnlyMmap;

fn main() -> Result<(), rs_mmap::MmapError> {
    let mapped = ReadOnlyMmap::from_vec(b"hello mapped world".to_vec());
    let view = mapped.slice(6..12)?;

    println!("mapped={:?}", mapped.as_slice());
    println!("view={:?} offset={}", view.as_slice(), view.offset());

    Ok(())
}
