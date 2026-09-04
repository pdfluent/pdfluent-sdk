// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use lopdf::{dictionary, Document, Object, Stream};
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("corpus")
}

fn load_corpus_samples() -> Vec<(String, Vec<u8>)> {
    let corpus = corpus_dir();
    if !corpus.exists() {
        return Vec::new();
    }

    let mut pdfs: Vec<(String, Vec<u8>)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&corpus) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "pdf") {
                if let Ok(data) = std::fs::read(&path) {
                    let name = path.file_stem().unwrap().to_string_lossy().to_string();
                    pdfs.push((name, data));
                }
            }
        }
    }
    pdfs.sort_by(|a, b| a.1.len().cmp(&b.1.len()));
    pdfs
}

fn pick_samples(pdfs: &[(String, Vec<u8>)]) -> Vec<&(String, Vec<u8>)> {
    if pdfs.len() >= 3 {
        vec![&pdfs[0], &pdfs[pdfs.len() / 2], &pdfs[pdfs.len() - 1]]
    } else {
        pdfs.iter().collect()
    }
}

fn blank_pdf(page_count: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::with_capacity(page_count);

    for _ in 0..page_count {
        let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
            "Contents" => content_id,
        });
        kids.push(page_id.into());
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => page_count as i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf)
        .expect("serialising an in-memory benchmark PDF should succeed");
    buf
}

fn bench_settings_reuse(c: &mut Criterion) {
    let doc = pdf_engine::PdfDocument::open(blank_pdf(100)).expect("open benchmark PDF");
    let indices: Vec<usize> = (0..100).collect();

    let mut group = c.benchmark_group("settings_reuse");
    group.sample_size(20);

    group.bench_function("clone_per_page", |b| {
        b.iter(|| {
            let texts: Vec<String> = indices
                .iter()
                .map(|&index| doc.extract_text(std::hint::black_box(index)).unwrap())
                .collect();
            std::hint::black_box(texts.iter().map(String::len).sum::<usize>())
        });
    });

    group.bench_function("pooled_settings", |b| {
        b.iter(|| {
            let texts = doc
                .extract_text_pages_reusing_settings(indices.iter().copied())
                .unwrap();
            std::hint::black_box(texts.iter().map(String::len).sum::<usize>())
        });
    });

    group.finish();
}

fn bench_render_page(c: &mut Criterion) {
    let pdfs = load_corpus_samples();
    if pdfs.is_empty() {
        return;
    }
    let samples = pick_samples(&pdfs);

    let mut group = c.benchmark_group("render_page");
    group.sample_size(10);

    for (name, data) in &samples {
        let doc = match pdf_engine::PdfDocument::open(data.clone()) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if doc.page_count() == 0 {
            continue;
        }

        let label = format!("{} ({}KB)", name, data.len() / 1024);
        group.bench_with_input(BenchmarkId::new("72dpi", &label), &doc, |b, doc| {
            let opts = pdf_engine::RenderOptions::default();
            b.iter(|| {
                let _ = doc.render_page(0, &opts);
            });
        });
    }
    group.finish();
}

fn bench_text_extract(c: &mut Criterion) {
    let pdfs = load_corpus_samples();
    if pdfs.is_empty() {
        return;
    }
    let samples = pick_samples(&pdfs);

    let mut group = c.benchmark_group("text_extract");
    for (name, data) in &samples {
        let doc = match pdf_engine::PdfDocument::open(data.clone()) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let page_count = doc.page_count().min(10);
        if page_count == 0 {
            continue;
        }

        let label = format!("{} ({}p)", name, page_count);
        group.bench_with_input(BenchmarkId::new("extract", &label), &doc, |b, doc| {
            b.iter(|| {
                for i in 0..page_count {
                    let _ = doc.extract_text(i);
                }
            });
        });
    }
    group.finish();
}

fn bench_compliance_check(c: &mut Criterion) {
    let pdfs = load_corpus_samples();
    if pdfs.is_empty() {
        return;
    }
    let samples = pick_samples(&pdfs);

    let mut group = c.benchmark_group("compliance_check");
    group.sample_size(10);

    for (name, data) in &samples {
        let pdf = match pdf_syntax::Pdf::new(data.clone()) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let level =
            pdf_compliance::detect_pdfa_level(&pdf).unwrap_or(pdf_compliance::PdfALevel::A2b);

        let label = format!("{} ({}KB)", name, data.len() / 1024);
        group.bench_with_input(BenchmarkId::new("pdfa", &label), &(), |b, _| {
            let pdf = pdf_syntax::Pdf::new(data.clone()).unwrap();
            b.iter(|| {
                let _ = pdf_compliance::validate_pdfa(&pdf, level);
            });
        });
    }
    group.finish();
}

fn bench_memory_profile(c: &mut Criterion) {
    let pdfs = load_corpus_samples();
    if pdfs.is_empty() {
        return;
    }

    // Use the largest PDF for memory profiling.
    let largest = pdfs.last().unwrap();

    let mut group = c.benchmark_group("memory_profile");
    group.sample_size(10);

    let label = format!("{} ({}KB)", largest.0, largest.1.len() / 1024);
    group.bench_with_input(
        BenchmarkId::new("full_page_access", &label),
        &largest.1,
        |b, data| {
            b.iter(|| {
                let pdf = pdf_syntax::Pdf::new(data.clone()).unwrap();
                let pages = pdf.pages();
                for page in pages.iter() {
                    let _ = page.page_stream();
                }
            });
        },
    );
    group.finish();
}

criterion_group!(
    benches,
    bench_settings_reuse,
    bench_render_page,
    bench_text_extract,
    bench_compliance_check,
    bench_memory_profile,
);
criterion_main!(benches);
