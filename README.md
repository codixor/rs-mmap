# rs-mmap

`rs-mmap` is the workspace source-of-truth memory-mapping crate.

It now owns the integrated safe mapping backend directly instead of delegating
to `memmap2` or carrying a separate donor crate.

## Public Surface

The crate now has two layers:

- compatibility surface:
  - `ReadOnlyMmap::open(...)`
  - `ReadOnlyMmap::from_bytes(...)`
  - `ReadOnlyMmap::from_vec(...)`
  - `slice(...)`, `slice_from(...)`, `slice_to(...)`
- typed mapping surface:
  - `Mmap<ReadOnly>`
  - `Mmap<ReadWrite>`
  - `MmapOptions::new().path(...).map_readonly()`
  - `MmapOptions::new().path(...).map_readwrite()`
  - `MmapOptions::new().path(...).offset(...).len(...).open()`

Additive low-level helpers are also available:

- `Protection`
- `MemoryAdvice`
- `HugePageSize`
- `FileLock`
- `MappingMode` on Unix
- `AsyncMmap` behind the `async` feature
- NUMA helpers behind the `numa` feature

Additional optional feature gates:

- `zerocopy-views`
  - adds `ZeroCopyView`
  - adds `view(...)` / `view_slice(...)` on `ReadOnlyMmap`, `MmapSlice`, and
    typed mappings
  - adds `view_mut(...)` / `view_slice_mut(...)` on `Mmap<ReadWrite>`
- `serde-support`
  - adds `serde::{Serialize, Deserialize}` for the public configuration/value
    types such as `MmapOptions`, `Protection`, `MemoryAdvice`,
    `HugePageSize`, and `LockType`

## Platform Model

- native targets use the integrated platform backend in `src/platform`
- `wasm32` keeps the in-memory compatibility surface and returns a typed
  unsupported-platform error for file-backed mappings
- subrange mapping stays supported through aligned platform mappings, so
  `offset(...).len(...)` keeps working on the public builder surface

## Simple Example

```rust
use rs_mmap::{MmapOptions, ReadOnlyMmap};
use std::sync::Arc;

let mapped = ReadOnlyMmap::from_bytes(Arc::<[u8]>::from(*b"hello"));
assert_eq!(mapped.as_slice(), b"hello");

#[cfg(not(target_arch = "wasm32"))]
let _typed = MmapOptions::new()
    .path("data.bin")
    .offset(128)
    .len(4096)
    .map_readonly();
```

## Optional Zero-Copy Views

```rust
#[cfg(feature = "zerocopy-views")]
{
    use rs_mmap::ReadOnlyMmap;

    let mapped = ReadOnlyMmap::from_vec(0x1234_5678_u32.to_ne_bytes().to_vec());
    let value = mapped.view::<u32>()?;
    assert_eq!(*value, 0x1234_5678);
}
# Ok::<(), rs_mmap::MmapError>(())
```

## Examples

- `examples/from_bytes.rs`
- `examples/open_file.rs`
- `examples/slice_view.rs`

## Validation

- `scripts/validate.sh`
- `scripts/target-matrix.sh`
- `cargo bench -p rs-mmap --bench mmap_core -- --sample-size 20`

## Benchmarks

The current Criterion surface in `benches/mmap_core.rs` tracks:

- `from_vec_4k`
- `clone_handle_4k`
- `as_slice_len_4k`
- `slice_view_checked_len_4k`

This crate stays measurement-driven. We keep low-level complexity only when it
improves the actual mapping surface or preserves compatibility we rely on.
