use criterion::{black_box, criterion_group, criterion_main, Criterion};
use demo::apply_markup_cents;

fn bench_markup(c: &mut Criterion) {
    c.bench_function("apply_markup_cents/1999+13", |b| {
        b.iter(|| apply_markup_cents(black_box(1999), black_box(13)))
    });
}

criterion_group!(benches, bench_markup);
criterion_main!(benches);
