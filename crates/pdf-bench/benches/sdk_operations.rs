use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
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
    bench_render_page,
    bench_text_extract,
    bench_compliance_check,
    bench_memory_profile,
);
criterion_main!(benches);
