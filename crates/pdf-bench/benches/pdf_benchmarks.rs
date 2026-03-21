//! Criterion benchmarks for core XFA SDK operations.
//!
//! Uses `tests/corpus-mini/` (always present in the repo) as input data so
//! the suite runs in CI without a local corpus.  Each benchmark group covers
//! one public API surface:
//!
//! | Group            | Crate         | What is measured                        |
//! |------------------|---------------|-----------------------------------------|
//! | `parse`          | pdf-syntax    | `Pdf::new` — zero-copy reader init      |
//! | `parse`          | lopdf         | `Document::load_mem` — full mutable load|
//! | `text_extract`   | pdf-extract   | All-page + single-page text + chars     |
//! | `search`         | pdf-extract   | Full-text search, case-{in}sensitive    |
//! | `pages`          | pdf-manip     | Page extraction and document merge      |
//! | `compliance`     | pdf-compliance| PDF/A-2b and PDF/UA-1 validation        |
//! | `forms`          | pdf-forms     | AcroForm parse + field iteration        |
//! | `images`         | pdf-extract   | Raster image extraction                 |
//! | `render`         | pdf-engine    | First-page render at 72 dpi             |

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/crates/pdf-bench
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .join("tests")
        .join("corpus-mini")
}

/// Load all PDFs from the mini corpus, sorted by name.
fn load_fixtures() -> Vec<(String, Vec<u8>)> {
    let dir = fixtures_dir();
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "pdf") {
                if let Ok(data) = std::fs::read(&path) {
                    let name = path
                        .file_stem()
                        .expect("file has stem")
                        .to_string_lossy()
                        .into_owned();
                    out.push((name, data));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Load a single named fixture (panics with the filename if missing).
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixtures_dir().join(name)).unwrap_or_else(|_| panic!("missing fixture: {name}"))
}

// ---------------------------------------------------------------------------
// Group 1: PDF parsing  — pdf_syntax vs lopdf
// ---------------------------------------------------------------------------

fn bench_parse(c: &mut Criterion) {
    let fixtures = load_fixtures();
    if fixtures.is_empty() {
        eprintln!("bench_parse: corpus-mini not found, skipping");
        return;
    }

    // pdf_syntax — hayro zero-copy parser
    let mut g = c.benchmark_group("parse/pdf_syntax");
    for (name, data) in &fixtures {
        g.throughput(Throughput::Bytes(data.len() as u64));
        g.bench_with_input(BenchmarkId::from_parameter(name), data, |b, data| {
            b.iter(|| {
                let _ = pdf_syntax::Pdf::new(data.clone());
            });
        });
    }
    g.finish();

    // lopdf — full mutable object graph
    let mut g = c.benchmark_group("parse/lopdf");
    for (name, data) in &fixtures {
        g.throughput(Throughput::Bytes(data.len() as u64));
        g.bench_with_input(BenchmarkId::from_parameter(name), data, |b, data| {
            b.iter(|| {
                let _ = lopdf::Document::load_mem(data);
            });
        });
    }
    g.finish();
}

// ---------------------------------------------------------------------------
// Group 2: Text extraction  — pdf_extract
// ---------------------------------------------------------------------------

fn bench_text_extract(c: &mut Criterion) {
    let data = fixture("multi-page.pdf");
    let doc = match lopdf::Document::load_mem(&data) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("bench_text_extract: failed to load multi-page.pdf: {e}");
            return;
        }
    };

    let mut g = c.benchmark_group("text_extract");
    g.throughput(Throughput::Bytes(data.len() as u64));

    // Extract every text block from every page.
    g.bench_function("all_pages", |b| {
        b.iter(|| {
            let _ = pdf_extract::extract_text(&doc);
        });
    });

    // Plain string for page 1 only.
    g.bench_function("page_1_plain", |b| {
        b.iter(|| {
            let _ = pdf_extract::extract_page_text(&doc, 1);
        });
    });

    // Per-character bounding boxes for page 1.
    g.bench_function("page_1_positioned_chars", |b| {
        b.iter(|| {
            let _ = pdf_extract::extract_positioned_chars(&doc, 1);
        });
    });

    g.finish();
}

// ---------------------------------------------------------------------------
// Group 3: Full-text search  — pdf_extract
// ---------------------------------------------------------------------------

fn bench_search(c: &mut Criterion) {
    let data = fixture("multi-page.pdf");
    let doc = match lopdf::Document::load_mem(&data) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("bench_search: failed to load multi-page.pdf: {e}");
            return;
        }
    };

    // Default is case-insensitive; create explicit variants.
    let opts_ci = pdf_extract::SearchOptions {
        case_insensitive: true,
        skip_bounding_boxes: false,
        ..Default::default()
    };
    let opts_cs = pdf_extract::SearchOptions {
        case_insensitive: false,
        skip_bounding_boxes: false,
        ..Default::default()
    };
    let opts_fast = pdf_extract::SearchOptions {
        case_insensitive: true,
        skip_bounding_boxes: true,
        ..Default::default()
    };

    let mut g = c.benchmark_group("search");

    // Substring search (common short word → many hits).
    g.bench_function("case_insensitive/the", |b| {
        b.iter(|| {
            let _ = pdf_extract::search_text(&doc, "the", &opts_ci);
        });
    });

    g.bench_function("case_sensitive/the", |b| {
        b.iter(|| {
            let _ = pdf_extract::search_text(&doc, "the", &opts_cs);
        });
    });

    // Skip bbox computation — useful when only match positions matter.
    g.bench_function("case_insensitive_no_bbox/the", |b| {
        b.iter(|| {
            let _ = pdf_extract::search_text(&doc, "the", &opts_fast);
        });
    });

    // Count-only — fastest path.
    g.bench_function("count_occurrences/the", |b| {
        b.iter(|| {
            let _ = pdf_extract::count_occurrences(&doc, "the");
        });
    });

    g.finish();
}

// ---------------------------------------------------------------------------
// Group 4: Page operations  — pdf_manip
// ---------------------------------------------------------------------------

fn bench_pages(c: &mut Criterion) {
    let multi_data = fixture("multi-page.pdf");
    let simple_data = fixture("simple.pdf");

    let multi_doc = match lopdf::Document::load_mem(&multi_data) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("bench_pages: {e}");
            return;
        }
    };
    let simple_doc = match lopdf::Document::load_mem(&simple_data) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("bench_pages: {e}");
            return;
        }
    };
    let page_count = multi_doc.get_pages().len() as u32;

    let mut g = c.benchmark_group("pages");

    // Extract a single page — minimal work (clone + delete N-1 pages).
    g.bench_function("extract_page_1", |b| {
        b.iter(|| {
            let _ = pdf_manip::pages::extract_pages(&multi_doc, &[1]);
        });
    });

    // Extract the last page.
    if page_count >= 2 {
        g.bench_with_input(
            BenchmarkId::new("extract_last_page", page_count),
            &page_count,
            |b, &last| {
                b.iter(|| {
                    let _ = pdf_manip::pages::extract_pages(&multi_doc, &[last]);
                });
            },
        );
    }

    // Extract a contiguous range (pages 1–3, if available).
    if page_count >= 3 {
        g.bench_function("extract_pages_1_to_3", |b| {
            b.iter(|| {
                let _ = pdf_manip::pages::extract_pages(&multi_doc, &[1, 2, 3]);
            });
        });
    }

    // Merge two documents.
    g.bench_function("merge_two_docs", |b| {
        b.iter(|| {
            let _ = pdf_manip::pages::merge_documents(&[multi_doc.clone(), simple_doc.clone()]);
        });
    });

    g.finish();
}

// ---------------------------------------------------------------------------
// Group 5: PDF/A and PDF/UA compliance validation  — pdf_compliance
// ---------------------------------------------------------------------------

fn bench_compliance(c: &mut Criterion) {
    let fixtures = load_fixtures();
    if fixtures.is_empty() {
        eprintln!("bench_compliance: corpus-mini not found, skipping");
        return;
    }

    // Validate every fixture against PDF/A-2b (most common enterprise level).
    let mut g = c.benchmark_group("compliance/pdfa_a2b");
    g.sample_size(10);

    for (name, data) in &fixtures {
        let pdf = match pdf_syntax::Pdf::new(data.clone()) {
            Ok(p) => p,
            Err(_) => continue,
        };
        g.throughput(Throughput::Bytes(data.len() as u64));
        g.bench_with_input(BenchmarkId::from_parameter(name), &(), |b, _| {
            b.iter(|| {
                let _ = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A2b);
            });
        });
    }
    g.finish();

    // PDF/A-1b (stricter subset, no transparency).
    let pdfa_data = fixture("pdfa-2b.pdf");
    if let Ok(pdf) = pdf_syntax::Pdf::new(pdfa_data.clone()) {
        let mut g = c.benchmark_group("compliance/pdfa_a1b");
        g.sample_size(10);
        g.throughput(Throughput::Bytes(pdfa_data.len() as u64));
        g.bench_function("pdfa-2b.pdf", |b| {
            b.iter(|| {
                let _ = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A1b);
            });
        });
        g.finish();
    }

    // PDF/UA-1 accessibility check.
    let ua_data = fixture("pdfa-2b.pdf");
    if let Ok(pdf) = pdf_syntax::Pdf::new(ua_data.clone()) {
        let mut g = c.benchmark_group("compliance/pdfua1");
        g.sample_size(10);
        g.throughput(Throughput::Bytes(ua_data.len() as u64));
        g.bench_function("pdfa-2b.pdf", |b| {
            b.iter(|| {
                let _ = pdf_compliance::validate_pdfua(&pdf);
            });
        });
        g.finish();
    }
}

// ---------------------------------------------------------------------------
// Group 6: AcroForm parsing  — pdf_forms
// ---------------------------------------------------------------------------

fn bench_forms(c: &mut Criterion) {
    let data = fixture("acroform.pdf");
    let pdf = match pdf_syntax::Pdf::new(data.clone()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("bench_forms: failed to parse acroform.pdf: {e:?}");
            return;
        }
    };

    let mut g = c.benchmark_group("forms");
    g.throughput(Throughput::Bytes(data.len() as u64));

    // Parse the entire AcroForm field tree.
    g.bench_function("parse_acroform", |b| {
        b.iter(|| {
            let _ = pdf_forms::parse_acroform(&pdf);
        });
    });

    // Parse + iterate every terminal field.
    g.bench_function("parse_and_iterate_fields", |b| {
        b.iter(|| {
            if let Some(tree) = pdf_forms::parse_acroform(&pdf) {
                let _ = tree.terminal_fields().len();
            }
        });
    });

    // Fully-qualified name resolution (commonly used in form processing).
    g.bench_function("fully_qualified_names", |b| {
        b.iter(|| {
            if let Some(tree) = pdf_forms::parse_acroform(&pdf) {
                for id in tree.terminal_fields() {
                    let _ = tree.fully_qualified_name(id);
                }
            }
        });
    });

    g.finish();
}

// ---------------------------------------------------------------------------
// Group 7: Image extraction  — pdf_extract
// ---------------------------------------------------------------------------

fn bench_images(c: &mut Criterion) {
    // scanned.pdf typically contains raster images.
    let data = fixture("scanned.pdf");
    let doc = match lopdf::Document::load_mem(&data) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("bench_images: failed to load scanned.pdf: {e}");
            return;
        }
    };

    let mut g = c.benchmark_group("images");
    g.sample_size(10);
    g.throughput(Throughput::Bytes(data.len() as u64));

    g.bench_function("extract_all_images", |b| {
        b.iter(|| {
            let _ = pdf_extract::extract_all_images(&doc);
        });
    });

    g.finish();
}

// ---------------------------------------------------------------------------
// Group 8: First-page render  — pdf_engine
// ---------------------------------------------------------------------------

fn bench_render(c: &mut Criterion) {
    let fixtures = load_fixtures();
    if fixtures.is_empty() {
        eprintln!("bench_render: corpus-mini not found, skipping");
        return;
    }

    let mut g = c.benchmark_group("render");
    g.sample_size(10);

    let opts = pdf_engine::RenderOptions {
        dpi: 72.0,
        ..Default::default()
    };

    for (name, data) in &fixtures {
        let doc = match pdf_engine::PdfDocument::open(data.clone()) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if doc.page_count() == 0 {
            continue;
        }
        g.throughput(Throughput::Bytes(data.len() as u64));
        g.bench_with_input(BenchmarkId::new("page_1/72dpi", name), &(), |b, _| {
            b.iter(|| {
                let _ = doc.render_page(0, &opts);
            });
        });
    }
    g.finish();
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_parse,
    bench_text_extract,
    bench_search,
    bench_pages,
    bench_compliance,
    bench_forms,
    bench_images,
    bench_render,
);
criterion_main!(benches);
