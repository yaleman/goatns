use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box; // ,Bencher

// use std::process::Termination;
use goatns::get_question_qname;

fn criterion_benchmark(c: &mut Criterion) {
    let input = [7, 101, 120, 97, 109, 112, 108, 101, 3, 99, 111, 109, 0].to_vec();
    c.bench_function("name_as_bytes", |b| {
        b.iter(|| get_question_qname(black_box(&input)))
    });
}

// fn bench_get_question_qname(rdata: Vec<u8>) {
//     assert!(get_question_qname(&[23, 0]).is_err());

//     let sample_data = vec![7, 101, 120, 97, 109, 112, 108, 101, 3, 99, 111, 109, 0];
//     eprintln!("{:?}", sample_data);
//     let result = get_question_qname(&sample_data);
//     assert_eq!(
//         result,
//         Ok(vec![101, 120, 97, 109, 112, 108, 101, 46, 99, 111, 109])
//     );
// }

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
