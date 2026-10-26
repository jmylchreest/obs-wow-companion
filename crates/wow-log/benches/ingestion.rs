use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use owc_wow_log::{Detector, Flavour, parse};
use std::hint::black_box;

fn ingestion(c: &mut Criterion) {
    let fixture = include_str!("../tests/fixtures/retail.log");
    let mut group = c.benchmark_group("synthetic_ingestion");
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(3));
    group.throughput(Throughput::Elements(10_000));
    group.bench_function("1000_complete_sessions_10000_lines", |b| {
        b.iter(|| {
            let mut count = 0;
            for _ in 0..1000 {
                let mut detector = Detector::new("benchmark".into(), Flavour::Retail);
                for line in fixture.lines() {
                    count += detector
                        .ingest(&parse(black_box(line), 0).expect("synthetic fixture"))
                        .len();
                }
            }
            assert_eq!(count, 3000);
            black_box(count)
        });
    });
    group.finish();
}
criterion_group!(benches, ingestion);
criterion_main!(benches);
