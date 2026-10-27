use criterion::{Criterion, criterion_group, criterion_main};
use owc_domain::{Activity, Kind};
use owc_library::Library;
use serde_json::Value;
use std::hint::black_box;

fn library(c: &mut Criterion) {
    let root = tempfile::tempdir().expect("fixture");
    let mut db = Library::open(&root.path().join("library.sqlite")).expect("library");
    let activities: Vec<_> = (0..1000)
        .map(|i| Activity::new(Kind::Raid, format!("Encounter {i}"), i, "fixture"))
        .collect();
    db.checkpoint("fixture", &Value::Null, &activities)
        .expect("seed");
    let mut group = c.benchmark_group("library_1000");
    group.sample_size(20);
    group.bench_function("checkpoint", |b| {
        b.iter(|| {
            db.checkpoint("fixture", &Value::Null, black_box(&activities))
                .expect("checkpoint");
        });
    });
    group.bench_function("list", |b| b.iter(|| black_box(db.list().expect("list"))));
    group.finish();
}
criterion_group!(benches, library);
criterion_main!(benches);
