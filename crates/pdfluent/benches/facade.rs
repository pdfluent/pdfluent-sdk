#![allow(missing_docs)]

//! Epic 4 #1234 — lightweight facade benchmarks.
//!
//! Baselines the 8 most common operations in `pdfluent::PdfDocument`
//! on the shipped `sample.pdf` fixture. **Relative SLA statements
//! only** — we deliberately do not publish absolute millisecond
//! claims because hardware variance across contributor machines is
//! wide.
//!
//! See `PERFORMANCE.md` for the SLA policy.
//!
//! Run:
//! ```bash
//! cargo bench -p pdfluent --bench facade
//! ```

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pdfluent::prelude::*;

const FIXTURE: &str = "tests/fixtures/sample.pdf";

fn bench_open_from_bytes(c: &mut Criterion) {
    let bytes = std::fs::read(FIXTURE).expect("fixture");
    c.bench_function("open_from_bytes", |b| {
        b.iter(|| {
            let doc = PdfDocument::from_bytes(black_box(&bytes)).expect("parse");
            black_box(doc)
        })
    });
}

fn bench_page_count(c: &mut Criterion) {
    let bytes = std::fs::read(FIXTURE).expect("fixture");
    let doc = PdfDocument::from_bytes(&bytes).expect("parse");
    c.bench_function("page_count", |b| b.iter(|| black_box(doc.page_count())));
}

fn bench_text(c: &mut Criterion) {
    let bytes = std::fs::read(FIXTURE).expect("fixture");
    let doc = PdfDocument::from_bytes(&bytes).expect("parse");
    c.bench_function("text", |b| b.iter(|| black_box(doc.text().expect("text"))));
}

fn bench_form_fields(c: &mut Criterion) {
    let bytes = std::fs::read(FIXTURE).expect("fixture");
    let doc = PdfDocument::from_bytes(&bytes).expect("parse");
    c.bench_function("form_fields", |b| {
        b.iter(|| black_box(doc.form_fields().expect("form_fields")))
    });
}

fn bench_to_bytes(c: &mut Criterion) {
    let bytes = std::fs::read(FIXTURE).expect("fixture");
    let doc = PdfDocument::from_bytes(&bytes).expect("parse");
    c.bench_function("to_bytes", |b| {
        b.iter(|| black_box(doc.to_bytes().expect("serialise")))
    });
}

fn bench_compress_strict(c: &mut Criterion) {
    let bytes = std::fs::read(FIXTURE).expect("fixture");
    c.bench_function("compress_strict", |b| {
        b.iter_with_setup(
            || PdfDocument::from_bytes(&bytes).expect("parse"),
            |mut doc| {
                let _ = doc
                    .compress(black_box(CompressOptions::strict()))
                    .expect("compress");
                black_box(doc)
            },
        )
    });
}

fn bench_subset_fonts(c: &mut Criterion) {
    let bytes = std::fs::read(FIXTURE).expect("fixture");
    c.bench_function("subset_fonts", |b| {
        b.iter_with_setup(
            || PdfDocument::from_bytes(&bytes).expect("parse"),
            |mut doc| {
                let _ = doc.subset_fonts().expect("subset");
                black_box(doc)
            },
        )
    });
}

fn bench_extract_pages_all(c: &mut Criterion) {
    let bytes = std::fs::read(FIXTURE).expect("fixture");
    let doc = PdfDocument::from_bytes(&bytes).expect("parse");
    let total = doc.page_count();
    c.bench_function("extract_pages_all", |b| {
        b.iter(|| black_box(doc.extract_pages(1..=total).expect("extract")))
    });
}

criterion_group!(
    benches,
    bench_open_from_bytes,
    bench_page_count,
    bench_text,
    bench_form_fields,
    bench_to_bytes,
    bench_compress_strict,
    bench_subset_fonts,
    bench_extract_pages_all,
);
criterion_main!(benches);
