#[cfg(not(target_arch = "wasm32"))]
use std::hint::black_box;

#[cfg(not(target_arch = "wasm32"))]
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(not(target_arch = "wasm32"))]
fn mmap_surface(c: &mut Criterion) {
    let bytes = vec![7_u8; 4096];
    let mapped = rs_mmap::ReadOnlyMmap::from_vec(bytes.clone());

    c.bench_function("from_vec_4k", |b| {
        b.iter(|| {
            let value = rs_mmap::ReadOnlyMmap::from_vec(bytes.clone());
            black_box(value.len());
        });
    });

    c.bench_function("clone_handle_4k", |b| {
        b.iter(|| {
            let cloned = mapped.clone();
            black_box(cloned.len());
        });
    });

    c.bench_function("as_slice_len_4k", |b| {
        b.iter(|| {
            black_box(mapped.as_slice().len());
        });
    });

    c.bench_function("slice_view_checked_len_4k", |b| {
        b.iter(|| {
            let start = black_box(128_usize);
            let end = black_box(2048_usize);
            let view = mapped.slice(start..end).expect("view should be in bounds");
            black_box(view.offset());
            black_box(view.len());
        });
    });
}

#[cfg(not(target_arch = "wasm32"))]
criterion_group!(benches, mmap_surface);
#[cfg(not(target_arch = "wasm32"))]
criterion_main!(benches);

#[cfg(target_arch = "wasm32")]
fn main() {}
