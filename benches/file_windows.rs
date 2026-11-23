//! Windowed, scattered and tail reads of one file through three paths: copied snapshots,
//! trusted direct mappings, and positional `std::fs` reads into one buffer.
//!
//! `cargo bench --bench file_windows`. Set `RS_MMAP_BENCH_MIB` to change the
//! 64 MiB file size.
#![allow(unsafe_code)]

#[cfg(any(unix, windows))]
mod native {
    use criterion::{Criterion, Throughput};
    use rs_mmap::ReadOnlyFile;
    use std::{fs::File, hint::black_box, io::Write};

    const WINDOW: usize = 4 << 20;
    const TAIL: usize = 64 << 10;
    const REGION: usize = 64 << 10;
    const REGIONS: u64 = 256;

    fn fixture() -> (File, u64) {
        let mib: usize = std::env::var("RS_MMAP_BENCH_MIB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(64);
        let line = [b'x'; 99];
        let mut block = Vec::with_capacity(1 << 20);
        while block.len() + 100 <= 1 << 20 {
            block.extend_from_slice(&line);
            block.push(b'\n');
        }
        let mut file = tempfile::tempfile().unwrap();
        for _ in 0..mib {
            file.write_all(&block).unwrap();
        }
        file.sync_all().unwrap();
        let len = file.metadata().unwrap().len();
        (file, len)
    }

    // Every arm uses this same scanner, so its speed does not bias the comparison.
    #[allow(clippy::naive_bytecount)]
    fn lines(bytes: &[u8]) -> usize {
        bytes.iter().filter(|&&b| b == b'\n').count()
    }

    fn window(len: u64, offset: u64) -> usize {
        usize::try_from(len - offset).map_or(WINDOW, |rest| rest.min(WINDOW))
    }

    fn scan(source: &ReadOnlyFile, len: u64) -> usize {
        let mut total = 0;
        let mut offset = 0;
        while offset < len {
            let size = window(len, offset);
            total += lines(&source.view(offset, size).unwrap());
            offset += size as u64;
        }
        total
    }

    #[cfg(unix)]
    fn read_at(file: &File, buffer: &mut [u8], offset: u64) {
        std::os::unix::fs::FileExt::read_exact_at(file, buffer, offset).unwrap();
    }

    #[cfg(windows)]
    fn read_at(file: &File, buffer: &mut [u8], offset: u64) {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = file;
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.read_exact(buffer).unwrap();
    }

    pub fn run(c: &mut Criterion) {
        let (file, len) = fixture();
        let snapshot = ReadOnlyFile::new(file.try_clone().unwrap()).unwrap();
        // SAFETY: the private temporary file is fully written above and no
        // handle writes to it or truncates it for the rest of the process.
        let stable = unsafe { ReadOnlyFile::new_stable(file.try_clone().unwrap()) }.unwrap();
        let expected = scan(&stable, len);
        assert_eq!(scan(&snapshot, len), expected);

        let mut group = c.benchmark_group("scan_4mib_windows");
        group.throughput(Throughput::Bytes(len));
        group.sample_size(10);
        group.bench_function("snapshot", |b| b.iter(|| black_box(scan(&snapshot, len))));
        group.bench_function("stable_map", |b| b.iter(|| black_box(scan(&stable, len))));
        group.bench_function("std_read_at", |b| {
            let mut buffer = vec![0; WINDOW];
            b.iter(|| {
                let (mut total, mut offset) = (0, 0);
                while offset < len {
                    let size = window(len, offset);
                    read_at(&file, &mut buffer[..size], offset);
                    total += lines(&buffer[..size]);
                    offset += size as u64;
                }
                assert_eq!(total, expected);
            });
        });
        group.finish();

        // Independent, unaligned 64 KiB regions spread over the file. Each is
        // created, scanned and dropped. On Windows the stable source maps
        // every region from one shared mapping object.
        let step = (len - REGION as u64) / REGIONS;
        let offsets: Vec<u64> = (0..REGIONS).map(|i| i * step + i % 7).collect();
        let mut group = c.benchmark_group("scattered_64kib_regions");
        group.bench_function("snapshot", |b| {
            b.iter(|| {
                for &offset in &offsets {
                    black_box(lines(&snapshot.view(offset, REGION).unwrap()));
                }
            });
        });
        group.bench_function("stable_map", |b| {
            b.iter(|| {
                for &offset in &offsets {
                    black_box(lines(&stable.view(offset, REGION).unwrap()));
                }
            });
        });
        group.bench_function("std_read_at", |b| {
            let mut buffer = vec![0; REGION];
            b.iter(|| {
                for &offset in &offsets {
                    read_at(&file, &mut buffer, offset);
                    black_box(lines(&buffer));
                }
            });
        });
        group.finish();

        let mut group = c.benchmark_group("tail_64kib");
        group.bench_function("snapshot", |b| {
            b.iter(|| snapshot.with_suffix(TAIL, |t| black_box(lines(t))).unwrap());
        });
        group.bench_function("stable_map", |b| {
            b.iter(|| stable.with_suffix(TAIL, |t| black_box(lines(t))).unwrap());
        });
        group.bench_function("std_read_at", |b| {
            let mut buffer = vec![0; TAIL];
            b.iter(|| {
                read_at(&file, &mut buffer, len - TAIL as u64);
                black_box(lines(&buffer))
            });
        });
        group.finish();
    }
}

#[cfg(any(unix, windows))]
criterion::criterion_group!(benches, native::run);
#[cfg(any(unix, windows))]
criterion::criterion_main!(benches);

#[cfg(not(any(unix, windows)))]
fn main() {}
