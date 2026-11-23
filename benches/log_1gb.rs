#[cfg(any(unix, windows))]
#[path = "support/log_file.rs"]
mod log_file;

#[cfg(any(unix, windows))]
mod native {
    use memchr::{memchr_iter, memrchr_iter};
    use std::fs::File;
    use std::hint::black_box;
    use std::io::{BufWriter, Write};
    use std::ops::Deref;
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    const FILE_SIZE: usize = 1_000_000_000;
    const RECORD_SIZE: usize = 100;
    const RECORDS: usize = FILE_SIZE / RECORD_SIZE;
    const WINDOW: usize = 64 * 1024;
    const CHUNK: usize = 4 * 1024 * 1024;
    const PICKS: usize = 256;
    const TAIL_READS: usize = 64;
    const ROUNDS: usize = 24;

    struct Fixture {
        // Hold the file open until all benchmark reads finish.
        _log: File,
        path: PathBuf,
        offsets: Vec<usize>,
        tail_checksum: u64,
    }

    fn fixture() -> Fixture {
        let (mut log, path) = crate::log_file::create().unwrap();
        let mut record = [b' '; RECORD_SIZE];
        let prefix = b"2026-11-23T00:00:00Z INFO request_id=";
        let fields = b" method=GET path=/api/items status=200 duration_ms=12";
        record[..prefix.len()].copy_from_slice(prefix);
        let digit_end = prefix.len() + 8;
        record[digit_end..digit_end + fields.len()].copy_from_slice(fields);
        record[RECORD_SIZE - 1] = b'\n';
        let mut tail_checksum = 0;
        {
            let mut writer = BufWriter::with_capacity(1 << 20, &mut log);
            for id in 0..RECORDS {
                let mut number = id;
                for digit in record[prefix.len()..digit_end].iter_mut().rev() {
                    *digit = b'0' + u8::try_from(number % 10).unwrap();
                    number /= 10;
                }
                writer.write_all(&record).unwrap();
                if id >= RECORDS - 100 {
                    tail_checksum += byte_sum(&record);
                }
            }
            writer.flush().unwrap();
        }
        log.sync_all().unwrap();
        assert_eq!(log.metadata().unwrap().len(), FILE_SIZE as u64);

        let mut seed = 0x5eed_a41b_u64;
        let offsets = (0..PICKS)
            .map(|_| {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                usize::try_from(seed % (FILE_SIZE - WINDOW + 1) as u64).unwrap()
            })
            .collect();
        Fixture {
            _log: log,
            path,
            offsets,
            tail_checksum,
        }
    }

    trait Backend {
        type Source;
        type View: Deref<Target = [u8]>;
        fn open(path: &Path) -> Self::Source;
        fn view(source: &Self::Source, offset: usize, len: usize) -> Self::View;
        fn suffix_checksum(source: &Self::Source) -> u64 {
            tail(&Self::view(source, FILE_SIZE - WINDOW, WINDOW))
        }
    }
    struct Ours;
    struct Upstream;

    // All backend calls below use only the private, closed-to-writes fixture.
    impl Backend for Ours {
        type Source = rs_mmap::ReadOnlyFile;
        type View = rs_mmap::FileView;
        fn open(path: &Path) -> Self::Source {
            // SAFETY: the fixture remains immutable until all views are dropped.
            unsafe { rs_mmap::ReadOnlyFile::new(File::open(path).unwrap()) }.unwrap()
        }
        fn view(source: &Self::Source, offset: usize, len: usize) -> Self::View {
            source.view(offset as u64, len).unwrap()
        }
        fn suffix_checksum(source: &Self::Source) -> u64 {
            source.with_suffix(WINDOW, tail).unwrap()
        }
    }
    impl Backend for Upstream {
        type Source = File;
        type View = memmap2::Mmap;
        fn open(path: &Path) -> File {
            File::open(path).unwrap()
        }
        fn view(source: &File, offset: usize, len: usize) -> Self::View {
            // SAFETY: the same fixture and checked ranges are used for both backends.
            unsafe {
                memmap2::MmapOptions::new()
                    .offset(offset as u64)
                    .len(len)
                    .map(source)
            }
            .unwrap()
        }
    }

    fn lines(bytes: &[u8]) -> u64 {
        memchr_iter(b'\n', black_box(bytes)).count() as u64
    }
    fn byte_sum(bytes: &[u8]) -> u64 {
        bytes.iter().copied().map(u64::from).sum()
    }
    fn tail(bytes: &[u8]) -> u64 {
        // Ignore the terminal newline while locating the preceding 100 lines.
        let boundary = memrchr_iter(b'\n', &bytes[..bytes.len() - 1])
            .nth(99)
            .unwrap();
        byte_sum(black_box(&bytes[boundary + 1..]))
    }

    #[derive(Clone, Copy)]
    enum Case {
        Full,
        Chunked,
        Tail,
        MappedTail,
        Independent,
        Borrowed,
    }

    impl Case {
        fn name(self) -> &'static str {
            match self {
                Self::Full => "count_lines_full",
                Self::Chunked => "count_lines_4m_windows",
                Self::Tail => "open_tail_last_100",
                Self::MappedTail => "open_tail_last_100_mapped_control",
                Self::Independent => "random_64k_independent",
                Self::Borrowed => "random_64k_borrowed",
            }
        }
        fn iterations(self) -> usize {
            match self {
                Self::Full | Self::Chunked => 1,
                Self::Tail | Self::MappedTail => TAIL_READS,
                Self::Independent | Self::Borrowed => PICKS,
            }
        }
        fn expected(self, f: &Fixture) -> u64 {
            match self {
                Self::Full | Self::Chunked => RECORDS as u64,
                Self::Tail | Self::MappedTail => TAIL_READS as u64 * f.tail_checksum,
                Self::Borrowed | Self::Independent => f
                    .offsets
                    .iter()
                    .map(|offset| ((offset + WINDOW) / RECORD_SIZE - offset / RECORD_SIZE) as u64)
                    .sum(),
            }
        }
    }

    fn timed(f: impl FnOnce() -> u64) -> (u128, u64) {
        let start = Instant::now();
        let result = black_box(f());
        (start.elapsed().as_nanos(), result)
    }

    fn batch<B: Backend>(case: Case, f: &Fixture) -> (u128, u64) {
        let path = &f.path;
        match case {
            Case::Full => timed(|| {
                let source = B::open(path);
                lines(&B::view(&source, 0, FILE_SIZE))
            }),
            Case::Chunked => timed(|| {
                let source = B::open(path);
                (0..FILE_SIZE)
                    .step_by(CHUNK)
                    .map(|off| lines(&B::view(&source, off, CHUNK.min(FILE_SIZE - off))))
                    .sum()
            }),
            Case::Tail => timed(|| {
                (0..TAIL_READS)
                    .map(|_| B::suffix_checksum(&B::open(path)))
                    .sum()
            }),
            Case::MappedTail => timed(|| {
                (0..TAIL_READS)
                    .map(|_| {
                        let source = B::open(path);
                        tail(&B::view(&source, FILE_SIZE - WINDOW, WINDOW))
                    })
                    .sum()
            }),
            Case::Independent => {
                let source = B::open(path);
                timed(|| {
                    f.offsets
                        .iter()
                        .map(|&off| lines(&B::view(&source, off, WINDOW)))
                        .sum()
                })
            }
            Case::Borrowed => {
                let source = B::open(path);
                let map = B::view(&source, 0, FILE_SIZE);
                // Resolve page faults for these exact windows before timing steady-state reads.
                for &off in &f.offsets {
                    black_box(lines(&map[off..off + WINDOW]));
                }
                timed(|| {
                    f.offsets
                        .iter()
                        .map(|&off| lines(&map[off..off + WINDOW]))
                        .sum()
                })
            }
        }
    }

    #[cfg(windows)]
    fn peak_bytes() -> u64 {
        use windows::Win32::System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::GetCurrentProcess,
        };
        let size = u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS>()).unwrap();
        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: size,
            ..Default::default()
        };
        // SAFETY: counters is writable for size bytes, and the pseudo-handle
        // refers to the calling process.
        unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &raw mut counters, size) }.unwrap();
        counters.PeakWorkingSetSize as u64
    }

    #[cfg(target_os = "linux")]
    fn peak_bytes() -> u64 {
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        let high_water = status
            .lines()
            .find_map(|line| line.strip_prefix("VmHWM:"))
            .unwrap();
        let kib: u64 = high_water
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        kib * 1024
    }

    #[cfg(any(target_os = "linux", windows))]
    pub fn memory(name: &str) {
        let case = match name {
            "full" => Case::Full,
            "chunked" => Case::Chunked,
            "tail" => Case::Tail,
            _ => panic!("choose full, chunked, or tail"),
        };
        let f = fixture();
        let setup_peak = peak_bytes();
        assert_eq!(batch::<Ours>(case, &f).1, case.expected(&f));
        println!("{},{},{}", case.name(), setup_peak, peak_bytes());
    }

    pub fn run(mapped_tail_control: bool) {
        let f = fixture();
        let cases = [
            Case::Full,
            Case::Chunked,
            if mapped_tail_control {
                Case::MappedTail
            } else {
                Case::Tail
            },
            Case::Independent,
            Case::Borrowed,
        ];
        println!("case,round,first,iterations,rs_ns,memmap2_ns");
        for &case in &cases {
            assert_eq!(batch::<Ours>(case, &f).1, case.expected(&f));
            assert_eq!(batch::<Upstream>(case, &f).1, case.expected(&f));
        }
        for round in 0..ROUNDS {
            for k in 0..cases.len() {
                let index = (round + k) % cases.len();
                let case = cases[index];
                let ours_first = (round + index).is_multiple_of(2);
                let (ours, upstream) = if ours_first {
                    let ours = batch::<Ours>(case, &f);
                    (ours, batch::<Upstream>(case, &f))
                } else {
                    let upstream = batch::<Upstream>(case, &f);
                    (batch::<Ours>(case, &f), upstream)
                };
                assert_eq!(ours.1, case.expected(&f));
                assert_eq!(upstream.1, ours.1);
                println!(
                    "{},{},{},{},{},{}",
                    case.name(),
                    round + 1,
                    if ours_first { "rs" } else { "memmap2" },
                    case.iterations(),
                    ours.0,
                    upstream.0
                );
            }
        }
    }
}

fn main() {
    if std::env::args().any(|arg| arg == "--measure" || arg == "--peak-rss") {
        eprintln!(
            "This benchmark creates a 1 GB log. Use --measure-1gb or --peak-rss-1gb CASE to request it."
        );
        std::process::exit(2);
    }
    #[cfg(any(target_os = "linux", windows))]
    if let Some(name) = std::env::args()
        .skip_while(|arg| arg != "--peak-rss-1gb")
        .nth(1)
    {
        native::memory(&name);
        return;
    }
    #[cfg(any(unix, windows))]
    if std::env::args().any(|arg| arg == "--measure-1gb") {
        native::run(std::env::args().any(|arg| arg == "--mapped-tail-control"));
    }
}
