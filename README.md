# rs-mmap

Safe file snapshots, verified and trusted memory maps, and fixed-layout records for Rust. Version 0.4.0 requires Rust 1.99 or later.

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

The source records the length at open. Shrinking the file during a snapshot can produce an I/O error. Concurrent writes can produce a mixture of old and new bytes during the read. After the read, the returned bytes are stable. Opening a complete ordinary file with `ReadOnlyMmap::open` snapshots its entire contents.

A snapshot larger than 64 MiB (`DEFAULT_MAX_SNAPSHOT`) returns `MmapError::SnapshotTooLarge` instead of copying silently. Read a smaller window, use `with_suffix`, raise the limit with `ReadOnlyFile::max_snapshot` or `MmapOptions::max_snapshot`, or map the file as described next. The limit also applies to `map_readwrite` and `map_copy`. Direct mappings and anonymous allocations are not limited.

## Mapping a stable file without copying

For a file that is complete and will not be rewritten, such as a closed log segment or a static asset, `ReadOnlyFile::new_stable` makes every view map the original pages. Nothing is copied, any size is accepted, and views can still outlive the source. On Windows the source creates one file-mapping object and maps every view from it, so repeated windows do not pay for a new mapping object each time. `MmapOptions::map_stable` does the same for one range of a path.

Both are `unsafe` because the OS does not enforce the promise: the file must not be modified or truncated, by any handle or process, while a mapping is alive. Changed bytes become visible in existing views, and reading past a truncated end terminates the process. An active log does not meet this contract; follow it with `ReadOnlyFile::new` and map segments after rotation closes them.

```rust
# #[cfg(any(unix, windows))] {
use rs_mmap::ReadOnlyFile;
use std::io::Write;

let mut file = tempfile::tempfile()?;
file.write_all(b"closed segment\n")?;
// SAFETY: this example owns the only handle and has finished writing.
let source = unsafe { ReadOnlyFile::new_stable(file)? };
let view = source.view(7, 7)?;
assert!(view.is_file_mapped());
assert_eq!(&*view, b"segment");
# }
# Ok::<(), rs_mmap::MmapError>(())
```

| File | Constructor | Copies? |
| --- | --- | --- |
| May still change (active log, config) | `ReadOnlyFile::new` | Yes, one window at a time |
| Sealed memfd or fs-verity file | `ReadOnlyFile::new` | No, the OS guarantees stability |
| Complete and never rewritten | `unsafe ReadOnlyFile::new_stable` | No, the caller guarantees stability |

## Writing and persistence

`MmapOptions::map_readwrite` takes a private snapshot and holds the original file handle for write-back. `flush_range(offset, len)` writes just that range and syncs the file's data. `flush()` writes the whole snapshot. Unflushed edits are discarded on drop. Concurrent writers require coordination, and multi-byte writes are not transactional. Use `map_copy()` for edits that should stay private.

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

The `zerocopy-views` feature adds `MmapStruct<T>` and checked typed references and slices. A record occupies exactly the native byte layout of one `T: ZeroCopyView`. Choose a stable layout and define byte order when sharing files between machines. `create` refuses to overwrite an existing path. `flush` writes in place; `flush_atomic` replaces the file through a synced temporary file and a rename, so a crash leaves either the old record or the new one. The rename creates a new file, so other open handles and locks keep the old one. `ByteArrayExt` reads and writes bounded, zero-padded UTF-8 fields.

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

## Migrating from 0.3

Ordinary files still use safe snapshots. Version 0.4 adds the unsafe stable-file constructors described above and a default 64 MiB snapshot limit. Applications matching on `MemoryAdvice` must remove uses of `DontNeed`, `Cold`, and `PageOut`. Use a smaller window or configure `max_snapshot` when upgrading callers that take larger snapshots.

## Platform support and migration from 0.2

- The crate denies unsafe Rust everywhere except `src/trusted.rs`, which holds the two `unsafe` stable-file constructors, their call into memmap2, and the Windows mapping-object code. CI fails if any other module opts out. All other native mapping and typed casts use the safe interfaces of memmap2 and bytemuck. On WebAssembly, `ReadOnlyMmap::from_vec`, `from_bytes`, and checked slices work in memory. File mapping returns `UnsupportedPlatform`.
- The 0.2 file-stability contract remains available through `new_stable` and `map_stable`. On Windows a stable source again reuses one file-mapping object across its views, the design behind the repeated-window gains measured on the 0.2.x backend; the 0.4 implementation has not been re-measured on Windows. Importing an externally created section (`from_section`) is not available. [Historical measurements](https://github.com/codixor/rs-mmap/tree/v0.2.3/benchmarks)
- The `AsyncMmap` adapter has been removed. Opening, snapshotting, and flushing are blocking operations. Tokio applications can offload the entire operation with `spawn_blocking`.
- `MemoryAdvice` keeps the hints the safe API can apply: `Normal`, `Random`, `Sequential` and `WillNeed`. `DontNeed`, `Cold` and `PageOut` are removed.
- Raw protection flags, live resizing, and raw-pointer NUMA binding are outside the safe API. `unstable` enables `numa` for Linux topology queries and `huge-pages` for explicit Linux anonymous allocations. Unsupported requests return errors. Serde configurations using retired options must be migrated.
- The development toolchain is Rust 1.100.0. CI checks the minimum version, native tests, mobile and WebAssembly builds, both dependency graphs, the confinement of unsafe code, and bounded Linux fuzz targets with AddressSanitizer. `cargo bench --bench file_windows` compares copied snapshots, stable mappings and positional `std::fs` reads on the current machine.
