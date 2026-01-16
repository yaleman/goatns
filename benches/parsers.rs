use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use goatns::get_question_qname;

fn criterion_benchmark(c: &mut Criterion) {
    let input = [7, 101, 120, 97, 109, 112, 108, 101, 3, 99, 111, 109, 0].to_vec();
    c.bench_function("name_as_bytes", |b| {
        b.iter(|| get_question_qname(black_box(&input)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
