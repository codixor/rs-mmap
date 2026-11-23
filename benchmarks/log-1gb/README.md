# Reading a 1 GB log

The results and reproduction commands on this page refer to the [0.2.x native backend and harness](https://github.com/codixor/rs-mmap/tree/v0.2.3). Version 0.3 snapshots ordinary file windows. Its timings have not been measured here.

## Results

On this PC, both crates took about the same time to count all ten million log lines. Scanning with rs-mmap in 4 MiB windows reached a similar elapsed time with a substantially smaller peak process working set.

| Workload | Windows rs-mmap | Windows memmap2 | WSL2 rs-mmap | WSL2 memmap2 |
|---|---:|---:|---:|---:|
| Whole-map line count | 632–698 ms | 627–709 ms | 113–130 ms | 112–129 ms |
| 4 MiB windowed line count | 613–706 ms | 633–715 ms | 118–131 ms | 115–128 ms |
| Open and read the last 100 lines | 184–185 µs | 179–183 µs | 18.7–19.5 µs | 17.8–19.8 µs |
| Independent 64 KiB region, including map and unmap | 53.7–57.2 µs | 68.2–73.4 µs | 20.9–21.2 µs | 20.1–23.4 µs |
| Borrowed 64 KiB region from a resident map | 6.1–6.3 µs | 6.1–6.5 µs | 6.2–6.4 µs | 6.3–6.4 µs |

Ranges span the two run medians. Independently mapping and scanning a 64 KiB region gave rs-mmap a repeatable Windows advantage of **1.27–1.30×**. All other comparisons included parity in the paired median intervals in both runs. The timing script calculates ratios within each round before taking the median.

### Peak process memory with rs-mmap

| Access pattern | Windows peak working set | WSL2 peak resident memory |
|---|---:|---:|
| Whole-map line count | 959.5 MiB | 955.4 MiB |
| 4 MiB windowed line count | 9.3 MiB | 6.0 MiB |
| Last 100 lines from a 64 KiB window | 6.2 MiB | 2.9 MiB |

Each cell comes from one fresh process, including fixture creation. These are process high-water marks. The OS can retain additional file pages in its cache after a view is unmapped. The memory probes used Windows PeakWorkingSetSize and Linux VmHWM. [Windows probe data](windows-peak.csv) · [WSL2 probe data](linux-wsl2-peak.csv) · [Windows counter definition](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters) · [Linux counter definition](https://docs.kernel.org/filesystems/proc.html)

For live logging, use these mappings on a closed, immutable segment. An active writer or a truncate-and-rewrite rotation violates the file-stability contract of `ReadOnlyFile`. A buffered file reader can follow the active segment while older rotated segments are scanned through mappings.

## Fixture

The benchmark uses a fully written 1,000,000,000-byte synthetic log containing 10,000,000 newline-terminated 100-byte records with distinct request IDs. Each executable creates a private file, flushes it to storage, leaves it unchanged, and removes it on exit. Each timed implementation uses the same file, known length, and byte scanner. The checks verify the line count, the contents of the final 100 records, and the newline counts at the sampled offsets.

## Workloads

- **Scan the whole map.** Open the file, map it once, count newlines across every byte with `memchr`, and drop the map and file. The timing includes new-view page faults against a warm file cache.
- **Scan in 4 MiB windows.** Open once and count exactly the same newlines, dropping each window before mapping the next. The final window is shortened to end exactly at EOF.
- **Read the last 100 lines.** Open the file anew, map the final 64 KiB, search backward for the boundary, and checksum every byte in the last 100 lines. Each round averages 64 requests. The 64 KiB bound is specific to this fixture; a general tail reader would extend its search for longer lines.
- **Scan independently mapped 64 KiB regions.** Prepare one source per backend, then create, scan, and drop one view at each of 256 deterministic offsets spread across the log. The reported time is per region.
- **Scan borrowed 64 KiB regions.** Retain a whole-file map, pre-touch the same regions, then scan their slices. The reported time excludes opening, mapping, and pre-touching. This measures repeated queries on an already resident working set.

## Reproduce

A full run creates one 1,000,000,000-byte temporary file. The size is explicit in the command so a routine test or the old `--measure` invocation cannot create it. Windows deletes the log when the last handle closes, including after forced termination. On Linux the log has no directory entry and both implementations reopen it through `/proc/self/fd`. The tables above were collected with the earlier named-file implementation.

```sh
cargo +1.100.0 bench --locked --bench log_1gb -- --measure-1gb > run.csv
python scripts/summarize.py run.csv
cargo +1.100.0 bench --locked --bench log_1gb -- --peak-rss-1gb chunked
```

For the optional Windows and Linux peak-memory probe, choose `full`, `chunked`, or `tail`. Run each in its own process. The output fields are the case name, peak bytes immediately after setup, and peak bytes after the read.

Each invocation warms both implementations before 24 paired rounds with alternating measurement order. Each platform ran twice on the Intel Core i9-9820X, with Windows and WSL2 runs scheduled sequentially. Both crates were built into one release-profile executable using Rust 1.100.0, fat LTO, and one codegen unit. Windows used a native NTFS temporary directory. WSL2 used Debian 13 on ext4 inside its virtual disk; the default WSL2 `/tmp` on this machine is tmpfs, so `TMPDIR` was explicitly placed under the Linux home directory. The results describe warm-cache reads on this host. Cold storage, live writers, and multi-reader contention were outside the experiment.

[Windows run 1](windows-x64.csv) · [Windows run 2](windows-x64-repeat.csv) · [WSL2 run 1](linux-wsl2-x64.csv) · [WSL2 run 2](linux-wsl2-x64-repeat.csv)

## After the upstream PR review

On 2026-11-23, one additional run per platform exercised the same five default workloads on the same PC with Rust 1.100.0 and memmap2 0.9.12. Each value below is the median of 24 paired memmap2/rs-mmap time ratios. Values above 1 favor rs-mmap. Brackets give the distribution-free interval for the median calculated by `scripts/summarize.py`.

| Workload | Windows x64 | Linux x64 (WSL2) |
| --- | ---: | ---: |
| Whole-map line count | 0.994 [0.971, 1.016] | 1.014 [0.997, 1.030] |
| 4 MiB windowed line count | 1.012 [0.994, 1.027] | 0.997 [0.986, 1.014] |
| Last 100 lines | 0.982 [0.953, 1.049] | 0.950 [0.922, 0.999] |
| Independently mapped 64 KiB region | 1.272 [1.231, 1.319] | 1.027 [0.995, 1.035] |
| Borrowed 64 KiB region | 0.998 [0.986, 1.015] | 0.986 [0.964, 1.011] |

The Windows independent-region advantage repeated. The WSL2 tail-read result favored memmap2 in this run. The default benchmark does not exercise the new section-import or reclamation APIs. [Windows paired samples](upstream-pr-windows-x64.csv) · [WSL2 paired samples](upstream-pr-linux-wsl2-x64.csv)
