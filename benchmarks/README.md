# rs-mmap compared with memmap2 0.9.12

On this Intel Core i9-9820X, the Windows implementation won three of five workloads in both runs. The WSL2 runs did not establish a repeatable advantage for either implementation. The crate remains unpublished.

Each number below is the median paired speedup under the repository's release settings, calculated as memmap2 elapsed time divided by rs-mmap elapsed time. A value above 1 favors rs-mmap.

| Workload | Windows run 1 | Windows run 2 | Linux WSL2 run 1 | Linux WSL2 run 2 |
|---|---:|---:|---:|---:|
| Open file, map 8 MiB, read ends, drop all handles | 1.003× | 1.074× | 0.988× | 1.012× |
| Map and drop independent aligned 4 KiB windows | 6.871× | 6.648× | 1.000× | 1.009× |
| Map and drop independent unaligned 64 KiB windows | 5.998× | 5.610× | 1.068× | 1.043× |
| Allocate 64 KiB anonymously, write and drop | 2.184× | 2.129× | 1.095× | 1.000× |
| Scan an existing 8 MiB map at 4 KiB intervals | 1.059× | 0.988× | 1.020× | 1.069× |

The three Windows wins have per-run median intervals entirely above 1. The other workloads' intervals include 1. The one-shot case includes setup, where reusing a section provides no repeatable advantage. The window cases create and unmap a separate view each iteration. Applications that can retain a whole-file mapping can take cheap borrowed subslices with either crate.

## Reproduce

```sh
cargo +1.100.0 bench --locked --bench compare -- --measure > samples.csv
python scripts/summarize.py samples.csv
```

The harness builds both implementations into the same optimized executable with Rust 1.100.0, fat LTO and one codegen unit. Two earlier runs per platform used Cargo's default bench profile and showed the same three Windows wins. [Cargo-default samples](cargo-default)

The hosts were Windows x86-64 MSVC (build 26300) and Debian 13 x86-64 GNU under WSL2 (kernel 6.18.58.1) on the same PC. Each invocation creates a private 8 MiB file in its native temporary directory, warms the file and code paths, and compares checksums. The two environments ran sequentially.

Each of the two runs contains 24 paired rounds per workload. The first implementation alternates, and the workload order rotates. In the repeated-window cases both implementations start with an open file. rs-mmap prepares its reusable source once. Direct memmap2 receives an explicit offset and length on an already open file. Each implementation touches both ends of each range before unmapping. The scan times only reads on pre-existing, warm mappings. Anonymous maps are zero-initialized in both cases.

The script reports nanoseconds per operation, the median of the 24 within-round ratios, and the seventh and eighteenth ordered ratios. Under independent pairs, these bounds form a 97.7% sign-based interval for the median. Shared-host measurements can be correlated, so the second invocation is reported separately. Cold storage, throughput under contention, and other CPUs and operating systems were outside this measurement.

[Windows run 1](windows-x64.csv) · [Windows run 2](windows-x64-repeat.csv) · [WSL2 run 1](linux-wsl2-x64.csv) · [WSL2 run 2](linux-wsl2-x64-repeat.csv)
