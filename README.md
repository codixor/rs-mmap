# rs-mmap

A Rust crate for typed memory mappings, checked subranges, and independently owned read-only file views.

```sh
cargo add rs-mmap@0.2.2
```

## Repeated file windows

`ReadOnlyFile` accepts an open file and checks each requested range against its length. Windows creates one file-mapping object and reuses it across views. Unix maps each range with `memmap2`. Each view unmaps independently and can outlive its source.

```rust
use rs_mmap::ReadOnlyFile;
use std::fs::File;

/// Reads the first four bytes of a stable file.
///
/// # Safety
/// Prevent writes and truncation for the duration of this call.
unsafe fn header(file: File) -> rs_mmap::Result<[u8; 4]> {
    // SAFETY: the caller keeps the file stable. Both handles are local.
    let source = unsafe { ReadOnlyFile::new(file)? };
    let view = source.view(0, 4)?;
    Ok([view[0], view[1], view[2], view[3]])
}
```

Prevent modifications and truncation until the source and its last view have been dropped. If a complete file is already mapped and a borrowed range is sufficient, index its byte slice directly. Convert an existing `memmap2::Mmap` with `ReadOnlyMmap::from(map)` to share or slice the original bytes without remapping.

On Windows, callers that already own a readable file-mapping object can pass it to `unsafe { ReadOnlyFile::from_section(section, file, len)? }`. Supply the backing file and the usable byte length starting at offset zero. Views reuse the supplied object and keep the file alive.

On Unix, `FileView::advise_range` accepts byte offsets relative to the view. For example, `view.advise_range(0..view.len(), MemoryAdvice::Sequential)?` hints at a forward scan. On Linux 5.4 or later, a reader can pass `MemoryAdvice::Cold` after scanning a log window to make its pages candidates for earlier reclamation. Use `PageOut` to request reclamation explicitly. The kernel acts on whole pages, may ignore hints, and can return an error for unsupported advice. [Upstream PR review](https://github.com/codixor/rs-mmap/blob/v0.2.2/docs/upstream-prs.md)

`ReadOnlyFile::with_suffix(max_len, inspect)` passes the last bytes of an immutable file to a callback. Short files yield their entire contents. Linux uses a positional read for suffixes up to 64 KiB. Larger Linux suffixes and Windows suffixes use read-only mappings.

## Other APIs

- `MmapOptions` creates typed read-only and read-write mappings for paths and subranges. File-backed constructors are unsafe because external file access is outside Rust's borrowing rules.
- `Mmap::<ReadWrite>::anonymous(len)` creates a safe private, zero-filled mapping. `ReadOnlyMmap::from_vec` and `from_bytes` provide shareable in-memory bytes, including on WebAssembly.
- `zerocopy-views` enables checked typed views, `async` enables the Tokio adapter, and `serde-support` enables serialization of configuration. The original platform-specific huge-page, file-lock and NUMA APIs are included.

## Development

The development toolchain is pinned to Rust 1.100.0. The crate supports Rust 1.95 or later. Local x86-64 GNU/Linux builds use Clang and LLD. Release benchmarks use fat LTO and one codegen unit for both implementations.

Run `cargo test --locked --all-features` and `cargo clippy --locked --all-features --all-targets -- -D warnings`. Run the paired comparison with `cargo bench --locked --bench compare -- --measure`. The original Criterion byte-container benchmarks remain under `mmap_core`. [Measured results and methodology](https://github.com/codixor/rs-mmap/blob/v0.2.2/benchmarks/README.md)

The initial commit imports only the `rs-mmap` subtree from [codixor/rust-misc at ae68abb](https://github.com/codixor/rust-misc/tree/ae68abb54c655a8fcc009123fbc31821b82435e4/rs-mmap). The standalone crate integrates [memmap2 0.9.12](https://github.com/RazrFalcon/memmap2-rs) as a dependency. Both projects use the MIT or Apache-2.0 license.
