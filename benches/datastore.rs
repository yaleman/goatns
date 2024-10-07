use criterion::{black_box, criterion_group, criterion_main, Criterion}; // ,Bencher

// use std::process::Termination;
use goatns::utils::name_as_bytes;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("name as bytes", |b| {
        b.iter(|| bench_resourcerecord_short_name_to_bytes(black_box("cheese".as_bytes().to_vec())))
    });
}

fn bench_resourcerecord_short_name_to_bytes(rdata: Vec<u8>) {
    assert_eq!(
        name_as_bytes(rdata, None, None).expect("failed to convert to bytes"),
        [6, 99, 104, 101, 101, 115, 101, 0]
    );
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
