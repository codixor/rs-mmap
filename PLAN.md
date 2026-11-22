# rs-mmap Plan

Goal: make `rs-mmap` the workspace source-of-truth mapping crate, with a safe
typed backend, a stable compatibility wrapper for existing callers, explicit
cross-target behavior, repeatable validation, and lightweight benchmarks.

## Direction

- [x] Keep the public API small enough to embed elsewhere in the workspace.
- [x] Preserve `ReadOnlyMmap` so existing callers like `svg` do not need a
  disruptive rewrite.
- [x] Replace `memmap2` fully with the integrated donor backend.
- [x] Keep safe `offset + len` subrange mapping behavior on the public builder.
- [x] Make `rs-mmap` itself the only source of truth.
- [x] Remove the standalone donor crate once the merge is validated.

## Stage 1: Identity And Docs

- [x] Add package metadata in [Cargo.toml](/Users/mwdghaz/rust-misc/mmap/Cargo.toml).
- [x] Add a crate [README.md](/Users/mwdghaz/rust-misc/mmap/README.md).
- [x] Keep this local [PLAN.md](/Users/mwdghaz/rust-misc/mmap/PLAN.md) in sync.

## Stage 2: Public API

- [x] Keep `ReadOnlyMmap`.
- [x] Keep `from_bytes(...)`.
- [x] Keep `from_vec(...)`.
- [x] Keep `open(...)` for native mappings.
- [x] Keep `MmapOptions::new().path(...).offset(...).len(...).open()`.
- [x] Expose typed mappings through `Mmap<ReadOnly>` and `Mmap<ReadWrite>`.
- [x] Keep lifetime-bound `MmapSlice`.
- [x] Keep `len()` and `is_empty()`.
- [x] Keep `AsRef<[u8]>` and `Deref<Target = [u8]>` on `ReadOnlyMmap`.

## Stage 3: Backend Merge

- [x] Import the donor platform/backend modules into `mmap/src`.
- [x] Reconcile the backend around this crate’s names and workspace deps.
- [x] Support aligned native mappings while preserving logical subrange views.
- [x] Replace `memmap2` in the crate manifest and implementation.
- [x] Keep optional `async` and `numa` support additive.

## Stage 4: Cross-Target Behavior

- [x] Keep native file mapping on supported non-`wasm32` targets.
- [x] Return a typed unsupported-platform error for file-backed mappings on
  `wasm32`.
- [x] Keep in-memory byte construction available on all targets.
- [x] Keep target-matrix checks for `wasm32`, Windows, iOS, and Android.

## Stage 5: Error Model

- [x] Use typed [MmapError](/Users/mwdghaz/rust-misc/mmap/src/error.rs).
- [x] Keep the old range and slice errors.
- [x] Add the integrated backend configuration and system-error cases.
- [x] Keep the error surface explicit instead of leaking raw platform details.

## Stage 6: Tests And Examples

- [x] Keep direct tests under `tests/`.
- [x] Cover in-memory construction.
- [x] Cover clone + slice access.
- [x] Cover native file open on host targets.
- [x] Cover file subrange mapping through `offset + len`.
- [x] Keep runnable examples for in-memory and file-backed usage.

## Stage 7: Validation And Benchmarks

- [x] Keep `scripts/validate.sh`.
- [x] Keep `scripts/target-matrix.sh`.
- [x] Keep the Criterion bench in `benches/mmap_core.rs`.
- [x] Revalidate after the backend merge.
- [x] Keep performance changes measurement-driven.

## Stage 8: Finish State

- [x] `rs-mmap` is the source of truth.
- [x] `memmap2` is gone from the crate.
- [x] The donor `safe-mmap` backend is merged into the main crate.
- [x] `ReadOnlyMmap` remains compatible for existing workspace callers.
- [x] The crate validates on host plus the target matrix.
- [x] The standalone `safe-mmap` folder can be removed.
