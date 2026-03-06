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

fn bench_lopdf_parse(c: &mut Criterion) {
    let pdfs = load_corpus_samples();
    if pdfs.is_empty() {
        return;
    }
    let samples = pick_samples(&pdfs);

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

fn bench_pdf_syntax_parse(c: &mut Criterion) {
    let pdfs = load_corpus_samples();
    if pdfs.is_empty() {
        return;
    }
    let samples = pick_samples(&pdfs);

    let mut group = c.benchmark_group("pdf_syntax_parse");
    for (name, data) in &samples {
        group.bench_with_input(
            BenchmarkId::new("parse", format!("{} ({}KB)", name, data.len() / 1024)),
            data,
            |b, data| {
                b.iter(|| {
                    let _ = pdf_syntax::Pdf::new(data.clone());
                });
            },
        );
    }
    group.finish();

    let mut group = c.benchmark_group("pdf_syntax_pages");
    for (name, data) in &samples {
        if let Ok(pdf) = pdf_syntax::Pdf::new(data.clone()) {
            group.bench_with_input(BenchmarkId::new("iterate_pages", name), &pdf, |b, pdf| {
                b.iter(|| {
                    let pages = pdf.pages();
                    for page in pages.iter() {
                        for _op in page.typed_operations() {}
                    }
                });
            });
        }
    }
    group.finish();
}

fn bench_xfa_extract(c: &mut Criterion) {
    let pdfs = load_corpus_samples();
    if pdfs.is_empty() {
        return;
    }

    let mut xfa_pdfs: Vec<&(String, Vec<u8>)> = Vec::new();
    for pdf in &pdfs {
        if let Ok(Some(_)) = pdfium_ffi_bridge::xfa_extract::scan_pdf_for_xfa(&pdf.1) {
            xfa_pdfs.push(pdf);
            if xfa_pdfs.len() >= 5 {
                break;
            }
        }
    }

    let mut group = c.benchmark_group("xfa_extract");
    for (name, data) in &xfa_pdfs {
        group.bench_with_input(BenchmarkId::new("scan_xfa", name), data, |b, data| {
            b.iter(|| {
                let _ = pdfium_ffi_bridge::xfa_extract::scan_pdf_for_xfa(data);
            });
        });
    }
    group.finish();
}

fn bench_formcalc(c: &mut Criterion) {
    let scripts = [
        ("arithmetic", "1 + 2 * 3 - 4 / 2"),
        ("string_ops", "Concat(\"hello\", \" \", \"world\")"),
        ("conditional", "if (1 > 0) then \"yes\" else \"no\" endif"),
        (
            "loop_100",
            "var x = 0\nfor i = 1 upto 100 do\nx = x + i\nendfor\nx",
        ),
        (
            "string_heavy",
            "var s = \"\"\nfor i = 1 upto 50 do\ns = Concat(s, \"a\")\nendfor\nLen(s)",
        ),
    ];

    let mut group = c.benchmark_group("formcalc");
    for (name, script) in &scripts {
        group.bench_with_input(BenchmarkId::new("eval", name), script, |b, script| {
            b.iter(|| {
                let tokens = formcalc_interpreter::lexer::tokenize(script).unwrap();
                let ast = formcalc_interpreter::parser::parse(tokens).unwrap();
                let mut interp = formcalc_interpreter::interpreter::Interpreter::new();
                let _ = interp.exec(&ast);
            });
        });
    }
    group.finish();
}

fn bench_data_dom(c: &mut Criterion) {
    let small_xml = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
        <xfa:data><form><field1>value1</field1><field2>value2</field2></form></xfa:data>
    </xfa:datasets>"#;

    let mut large_xml = String::from(
        r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><form>"#,
    );
    for i in 0..500 {
        large_xml.push_str(&format!("<field{i}>value{i}</field{i}>"));
    }
    large_xml.push_str("</form></xfa:data></xfa:datasets>");

    let mut group = c.benchmark_group("data_dom");
    group.bench_function("parse_small", |b| {
        b.iter(|| {
            let _ = xfa_dom_resolver::data_dom::DataDom::from_xml(small_xml);
        });
    });
    group.bench_function("parse_large_500_fields", |b| {
        b.iter(|| {
            let _ = xfa_dom_resolver::data_dom::DataDom::from_xml(&large_xml);
        });
    });
    group.bench_function("to_xml_roundtrip", |b| {
        let dom = xfa_dom_resolver::data_dom::DataDom::from_xml(&large_xml).unwrap();
        b.iter(|| {
            let _ = dom.to_xml();
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_lopdf_parse,
    bench_pdf_syntax_parse,
    bench_xfa_extract,
    bench_formcalc,
    bench_data_dom,
);
criterion_main!(benches);
