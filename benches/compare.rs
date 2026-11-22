#[cfg(any(unix, windows))]
mod native {
    use std::{
        fs::File,
        hint::black_box,
        io::Write,
        time::{Duration, Instant},
    };

    use rs_mmap::{Mmap, ReadOnlyFile, ReadWrite};
    use tempfile::NamedTempFile;

    const FILE_LEN: usize = 8 << 20;
    const PAGE: usize = 4096;
    const ROUNDS: usize = 24;

    #[derive(Clone, Copy)]
    enum Case {
        OneShot,
        Aligned,
        Unaligned,
        Anonymous,
        Scan,
    }

    impl Case {
        fn name(self) -> &'static str {
            match self {
                Self::OneShot => "open_map_read_drop_8m",
                Self::Aligned => "independent_window_4k",
                Self::Unaligned => "independent_window_64k_unaligned",
                Self::Anonymous => "anon_write_drop_64k",
                Self::Scan => "mapped_scan_8m_page_stride",
            }
        }

        fn iterations(self) -> usize {
            match self {
                Self::OneShot => 192,
                Self::Aligned | Self::Unaligned | Self::Anonymous => 1024,
                Self::Scan => 512,
            }
        }
    }

    struct Fixture {
        temp: NamedTempFile,
        file: File,
        source: ReadOnlyFile,
        ours: rs_mmap::FileView,
        upstream: memmap2::Mmap,
    }

    fn fixture() -> Fixture {
        let mut temp = NamedTempFile::new().unwrap();
        let block: Vec<u8> = (0..65536).map(|i| u8::try_from(i % 251).unwrap()).collect();
        for _ in 0..FILE_LEN / block.len() {
            temp.write_all(&block).unwrap();
        }
        temp.flush().unwrap();
        let file = temp.reopen().unwrap();
        // SAFETY: the private fixture is complete and stays immutable for all views.
        let source = unsafe { ReadOnlyFile::new(temp.reopen().unwrap()) }.unwrap();
        let ours = source.view(0, FILE_LEN).unwrap();
        let upstream = unsafe { memmap2::Mmap::map(&file) }.unwrap();
        assert_eq!(ours.as_ref(), &upstream[..]);
        Fixture {
            temp,
            file,
            source,
            ours,
            upstream,
        }
    }

    #[inline]
    fn edges(bytes: &[u8]) -> u64 {
        u64::from(bytes[0]) + u64::from(bytes[bytes.len() - 1])
    }
    #[inline]
    fn scan(bytes: &[u8]) -> u64 {
        bytes.iter().step_by(PAGE).map(|b| u64::from(*b)).sum()
    }

    fn batch<const OURS: bool>(case: Case, f: &Fixture) -> (Duration, u64) {
        let mut checksum = 0;
        let start = Instant::now();
        for i in 0..case.iterations() {
            checksum += match case {
                Case::OneShot => {
                    let file = File::open(f.temp.path()).unwrap();
                    if OURS {
                        // SAFETY: this benchmark never writes after fixture creation.
                        let source = unsafe { ReadOnlyFile::new(file) }.unwrap();
                        edges(black_box(&*source.view(0, FILE_LEN).unwrap()))
                    } else {
                        let map = unsafe { memmap2::Mmap::map(&file) }.unwrap();
                        edges(black_box(&map[..]))
                    }
                }
                Case::Aligned | Case::Unaligned => {
                    let (n, delta) = if matches!(case, Case::Aligned) {
                        (PAGE, 0)
                    } else {
                        (65536, 37)
                    };
                    let offset = ((i * 127 % ((FILE_LEN - n) / PAGE)) * PAGE + delta) as u64;
                    if OURS {
                        let view = f.source.view(offset, n).unwrap();
                        edges(black_box(&view))
                    } else {
                        // SAFETY: immutable fixture, identical offset and length for both backends.
                        let view = unsafe {
                            memmap2::MmapOptions::new()
                                .offset(offset)
                                .len(n)
                                .map(&f.file)
                        }
                        .unwrap();
                        edges(black_box(&view))
                    }
                }
                Case::Anonymous => {
                    let byte = i.to_le_bytes()[0];
                    if OURS {
                        let mut map = Mmap::<ReadWrite>::anonymous(65536).unwrap();
                        map[0] = byte;
                        edges(black_box(&map))
                    } else {
                        let mut map = memmap2::MmapMut::map_anon(65536).unwrap();
                        map[0] = byte;
                        edges(black_box(&map))
                    }
                }
                Case::Scan => {
                    if OURS {
                        scan(black_box(&f.ours))
                    } else {
                        scan(black_box(&f.upstream))
                    }
                }
            };
        }
        (start.elapsed(), black_box(checksum))
    }

    pub fn run() {
        let f = fixture();
        println!("case,round,first,iterations,rs_ns,memmap2_ns");
        let cases = [
            Case::OneShot,
            Case::Aligned,
            Case::Unaligned,
            Case::Anonymous,
            Case::Scan,
        ];
        for &c in &cases {
            assert_eq!(batch::<true>(c, &f).1, batch::<false>(c, &f).1);
        }
        for round in 0..ROUNDS {
            for k in 0..cases.len() {
                let index = (k + round) % cases.len();
                let c = cases[index];
                let ours_first = (round + index).is_multiple_of(2);
                let (a, b) = if ours_first {
                    let a = batch::<true>(c, &f);
                    (a, batch::<false>(c, &f))
                } else {
                    let b = batch::<false>(c, &f);
                    (batch::<true>(c, &f), b)
                };
                assert_eq!(a.1, b.1);
                println!(
                    "{},{},{},{},{},{}",
                    c.name(),
                    round + 1,
                    if ours_first { "rs" } else { "memmap2" },
                    c.iterations(),
                    a.0.as_nanos(),
                    b.0.as_nanos()
                );
            }
        }
    }
}

fn main() {
    #[cfg(any(unix, windows))]
    if std::env::args().any(|arg| arg == "--measure") {
        native::run();
    }
}
