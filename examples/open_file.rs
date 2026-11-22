#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), rs_mmap::MmapError> {
    use rs_mmap::MmapOptions;
    use std::{
        env, fs, process,
        time::{SystemTime, UNIX_EPOCH},
    };

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be available")
        .as_nanos();
    let path = env::temp_dir().join(format!("mmap-example-{}-{unique}.bin", process::id()));
    fs::write(&path, b"example-bytes")?;

    // SAFETY: this fixture is exclusively controlled for the mapping lifetime.
    let mapped = unsafe { MmapOptions::new().path(&path).offset(2).len(7).open() }?;
    let view = mapped.slice(1..5)?;
    println!(
        "len={} bytes={:?} view={:?}",
        mapped.len(),
        mapped.as_slice(),
        view.as_slice()
    );

    let _ = fs::remove_file(path);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {
    eprintln!("open_file example is not available on wasm32");
}
