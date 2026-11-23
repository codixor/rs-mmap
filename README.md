# rs-mmap

Checked byte windows, writable snapshots, and fixed-layout records for Rust. This checkout develops 0.3.0 and requires Rust 1.99. The latest published 0.2.x release is [0.2.3](https://crates.io/crates/rs-mmap/0.2.3).

## Reading a file

`ReadOnlyFile::view` reads a range from an open file. If memmap2 can verify that the OS prevents changes and truncation, the view maps the original pages directly. For an ordinary file, it copies only the requested range into private memory. Use `FileView::is_file_mapped()` to check which happened. Both kinds of view can outlive the file handle.

```rust
# #[cfg(any(unix, windows))] {
use rs_mmap::ReadOnlyFile;
use std::io::Write;

let mut file = tempfile::tempfile()?;
file.write_all(b"first\nlast\n")?;
let source = ReadOnlyFile::new(file)?;
let first = source.view(0, 5)?;
assert_eq!(&*first, b"first");
assert!(source.with_suffix(5, |tail| tail == b"last\n")?);
# }
# Ok::<(), rs_mmap::MmapError>(())
```

The source records the length at open. Shrinking the file during a snapshot can produce an I/O error. Concurrent writes can produce a mixture of old and new bytes during the read. After the read, the returned bytes are stable. For large logs, use windows or `with_suffix` to bound the amount copied. Opening the complete ordinary file with `ReadOnlyMmap::open` snapshots its entire contents.

## Writing and persistence

`MmapOptions::map_readwrite` takes a private snapshot and holds the original file handle for write-back. `flush_range(offset, len)` writes just that range and syncs the file. `flush()` writes the whole snapshot. Unflushed edits are discarded on drop. Concurrent writers require coordination, and multi-byte writes are not transactional. Use `map_copy()` for edits that should stay private.

```rust
# #[cfg(any(unix, windows))] {
use rs_mmap::MmapOptions;
use std::io::Write;

let mut file = tempfile::NamedTempFile::new()?;
file.write_all(b"abcdef")?;
let mut bytes = MmapOptions::new().path(file.path()).offset(1).len(3).map_readwrite()?;
bytes[1] = b'X';
bytes.flush_range(1, 1)?;
assert_eq!(std::fs::read(file.path())?, b"abXdef");
# }
# Ok::<(), rs_mmap::MmapError>(())
```

## Fixed-layout records

The `zerocopy-views` feature adds `MmapStruct<T>` and checked typed references and slices. A record occupies exactly the native byte layout of one `T: ZeroCopyView`. Choose a stable layout and define byte order when sharing files between machines. `create` refuses to overwrite an existing path. `ByteArrayExt` reads and writes bounded, zero-padded UTF-8 fields.

```rust
# #[cfg(all(feature = "zerocopy-views", any(unix, windows)))] {
use rs_mmap::{ByteArrayExt, MmapStruct};

let dir = tempfile::tempdir()?;
let path = dir.path().join("label");
let mut label = MmapStruct::create(&path, [0u8; 16])?;
label.set_str("Málaga")?;
label.flush()?;
drop(label);
let reopened = MmapStruct::<[u8; 16]>::open(path)?;
assert_eq!(reopened.as_slice().as_str()?, "Málaga");
# }
# Ok::<(), Box<dyn std::error::Error>>(())
```

## memmap2 compatibility

On native targets, `rs_mmap::compat` re-exports memmap2 0.9.12 and permits compatible 0.9.x updates. Existing memmap2 handles keep their type identity. `ReadOnlyMmap::from(map)` adopts an immutable map, and `Mmap::<ReadWrite>::from(map)` adopts a mutable map with native flush semantics. Both preserve the mapped address. The compatibility namespace exposes memmap2's original safety contracts.

```rust
# #[cfg(any(unix, windows))] {
use rs_mmap::{compat, ReadOnlyMmap};

let mut raw = compat::MmapMut::map_anon(4)?;
raw.copy_from_slice(b"data");
let map = raw.make_read_only()?;
let address = map.as_ptr();
let shared = ReadOnlyMmap::from(map);
assert_eq!(address, shared.as_ptr());
# }
# Ok::<(), std::io::Error>(())
```

## Platform support and migration from 0.2

- The crate forbids unsafe Rust in first-party code. Native mapping and typed casts use the safe interfaces of memmap2 and bytemuck. On WebAssembly, `ReadOnlyMmap::from_vec`, `from_bytes`, and checked slices work in memory. File mapping returns `UnsupportedPlatform`.
- The safe window API replaces imported Windows sections and external file-stability contracts. The original Windows repeated-window gains were measured on the 0.2.x backend. [Historical measurements](https://github.com/codixor/rs-mmap/tree/v0.2.3/benchmarks)
- The `AsyncMmap` adapter has been removed. Opening, snapshotting, and flushing are blocking operations. Tokio applications can offload the entire operation with `spawn_blocking`.
- Raw protection flags, live resizing, and raw-pointer NUMA binding are outside the safe API. `unstable` enables `numa` for Linux topology queries and `huge-pages` for explicit Linux anonymous allocations. Unsupported requests return errors. Serde configurations using retired options must be migrated.
- The development toolchain is Rust 1.100.0. CI checks the minimum version, native tests, mobile and WebAssembly builds, both dependency graphs, and bounded Linux fuzz targets with AddressSanitizer.
