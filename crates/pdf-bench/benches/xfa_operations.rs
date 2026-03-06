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

fn bench_full_pipeline(c: &mut Criterion) {
    let corpus = corpus_dir();
    if !corpus.exists() {
        eprintln!("Corpus not found, skipping pipeline benchmarks");
        return;
    }

    // Select a few representative PDFs for end-to-end benchmarking
    let test_files = ["f1040.pdf", "f1065.pdf", "i-485.pdf"];
    let mut pdfs: Vec<(String, Vec<u8>)> = Vec::new();

    for name in &test_files {
        let path = corpus.join(name);
        if let Ok(data) = std::fs::read(&path) {
            pdfs.push((name.to_string(), data));
        }
    }

    if pdfs.is_empty() {
        eprintln!("No test PDFs found in corpus, skipping");
        return;
    }

    // Extract XFA + parse
    let mut group = c.benchmark_group("pipeline_extract");
    group.sample_size(20);
    for (name, data) in &pdfs {
        group.bench_with_input(BenchmarkId::new("extract_xfa", name), data, |b, data| {
            b.iter(|| {
                let _ = pdfium_ffi_bridge::xfa_extract::scan_pdf_for_xfa(data);
            });
        });
    }
    group.finish();

    // PDF → JSON (full pipeline)
    let mut group = c.benchmark_group("pipeline_pdf_to_json");
    group.sample_size(10);
    for (name, data) in &pdfs {
        group.bench_with_input(BenchmarkId::new("pdf_to_json", name), data, |b, data| {
            b.iter(|| {
                let _ = pdfium_ffi_bridge::pipeline::pdf_to_json(data);
            });
        });
    }
    group.finish();
}

fn bench_lopdf_parse(c: &mut Criterion) {
    let corpus = corpus_dir();
    if !corpus.exists() {
        return;
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

    let samples: Vec<&(String, Vec<u8>)> = if pdfs.len() >= 3 {
        vec![&pdfs[0], &pdfs[pdfs.len() / 2], &pdfs[pdfs.len() - 1]]
    } else {
        pdfs.iter().collect()
    };

    let mut group = c.benchmark_group("lopdf_parse");
    for (name, data) in &samples {
        group.bench_with_input(
            BenchmarkId::new("load_mem", format!("{} ({}KB)", name, data.len() / 1024)),
            data,
            |b, data| {
                b.iter(|| {
                    let _ = lopdf::Document::load_mem(data);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_full_pipeline, bench_lopdf_parse);
criterion_main!(benches);
