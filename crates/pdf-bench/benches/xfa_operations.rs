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
                let _ = pdf_xfa::extract::extract_xfa_from_bytes(data.to_vec());
            });
        });
    }
    group.finish();

    // TODO: migrate pdf_to_json benchmark from pdfium-ffi-bridge (#622)
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

/// Benchmark `flatten_xfa_to_pdf` on a minimal synthetic XFA document.
///
/// This measures the end-to-end throughput of the XFA flatten pipeline on a
/// small but real XFA form (template + datasets embedded in a valid PDF).
///
/// P95 performance target: ≤ 5 seconds for 50-page documents (see
/// `docs/XFA_SUCCESS_CRITERIA.md`).  This benchmark uses a single-page
/// minimal form to establish a baseline; regression detection is the primary
/// goal.
fn bench_flatten_xfa_minimal(c: &mut Criterion) {
    use lopdf::{dictionary, Document, Object, Stream};

    // Build a minimal PDF containing a one-page XFA form.
    fn build_minimal_xfa_pdf() -> Vec<u8> {
        let xdp = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template>
    <subform name="root" layout="tb" w="612pt" h="792pt">
      <pageSet>
        <pageArea name="page1" w="612pt" h="792pt">
          <contentArea x="36pt" y="36pt" w="540pt" h="720pt"/>
        </pageArea>
      </pageSet>
      <subform name="body" layout="tb">
        <field name="title" w="540pt" h="20pt">
          <ui><textEdit/></ui>
          <caption><value><text>Name</text></value></caption>
        </field>
        <field name="value1" w="540pt" h="20pt">
          <ui><textEdit/></ui>
        </field>
      </subform>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <root><body><value1>benchmark data</value1></body></root>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

        let xdp_bytes = xdp.as_bytes().to_vec();
        let mut doc = Document::with_version("1.4");
        let xfa_stream = Stream::new(
            dictionary! { "Length" => Object::Integer(xdp_bytes.len() as i64) },
            xdp_bytes,
        );
        let xfa_id = doc.add_object(Object::Stream(xfa_stream));
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! { "Length" => Object::Integer(0_i64) },
            vec![],
        )));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"     => Object::Name(b"Page".to_vec()),
            "Parent"   => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id)
        }));
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type"  => Object::Name(b"Pages".to_vec()),
                "Kids"  => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1)
            }),
        );
        let acroform_id = doc.add_object(Object::Dictionary(dictionary! {
            "XFA"    => Object::Reference(xfa_id),
            "Fields" => Object::Array(vec![])
        }));
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"     => Object::Name(b"Catalog".to_vec()),
            "Pages"    => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id)
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut out = Vec::new();
        doc.save_to(&mut out).expect("save minimal xfa pdf");
        out
    }

    let pdf_bytes = build_minimal_xfa_pdf();

    let mut group = c.benchmark_group("flatten_xfa");
    group.sample_size(10);
    group.bench_function("minimal_1page", |b| {
        b.iter(|| {
            let _ = pdf_xfa::flatten_xfa_to_pdf(criterion::black_box(&pdf_bytes));
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_full_pipeline,
    bench_lopdf_parse,
    bench_flatten_xfa_minimal
);
criterion_main!(benches);
