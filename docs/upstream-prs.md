# memmap2 pull requests reviewed on 2026-11-23

This review documents the native backend in the [v0.2.3 release](https://github.com/codixor/rs-mmap/tree/v0.2.3). See the [migration notes](../README.md) for the current API.

We reviewed all eight open pull requests for their fit with independently mapped windows of immutable files, including rotated 1 GB logs. The adaptations use the existing `memmap2 0.9.12` dependency.

| Upstream proposal | Decision for rs-mmap |
| --- | --- |
| [#139 · Map an existing Windows file-mapping object](https://github.com/RazrFalcon/memmap2-rs/pull/139) | Added an unsafe Windows constructor that takes ownership of a readable section and its backing file. The caller specifies the section extent. Each independently owned view keeps the file alive. |
| [#179 · Check advice bounds and align page-lock operations](https://github.com/RazrFalcon/memmap2-rs/pull/179) | Added checked, view-relative Unix advice. Empty ranges inside the view succeed. Other ranges are validated before converting them to page addresses. Page locking is outside the log reader's requirements. |
| [#133 · Extend Linux memory advice](https://github.com/RazrFalcon/memmap2-rs/pull/133) | 0.2.3 added `Cold` and `PageOut` for readers that have finished with a region. 0.3 removed them together with `DontNeed`: memmap2 exposes reclamation hints only through unsafe calls, so the safe API could never honor them. Fork inheritance, hardware-error handling and huge-page collapse address different workloads. |
| [#136 · Windows large pages](https://github.com/RazrFalcon/memmap2-rs/pull/136) | Deferred. The existing anonymous allocator already attempts large pages, and this proposal does not apply to ordinary disk-backed log views. |
| [#150 · Named page-file-backed Windows mappings](https://github.com/RazrFalcon/memmap2-rs/pull/150) | Deferred. Named shared memory serves interprocess communication and needs its own ownership and synchronization contract. |
| [#164 · macOS mapped-file read failures](https://github.com/RazrFalcon/memmap2-rs/pull/164) | Deferred. Substituting zeroes on a backing-store fault can silently change the bytes seen by a log parser. |
| [#181 · Emulate file mappings on WASI](https://github.com/RazrFalcon/memmap2-rs/pull/181) | Deferred. It copies the requested range into a heap buffer. A whole-file request for a 1 GB log would allocate a 1 GB buffer. |
| [#182 · QNX advice portability](https://github.com/RazrFalcon/memmap2-rs/pull/182) | Deferred until a QNX target and SDK are available to validate both rs-mmap and its Unix mapping dependency. |

Linux callers that exhaustively match `MemoryAdvice` need arms for the two new variants. Other targets retain the existing variants.

The 1 GB comparison exercises the default read paths. Timing external Windows sections and measuring reclamation under Linux memory pressure would require separate workloads. [Benchmark workloads and results](../benchmarks/log-1gb/README.md)
