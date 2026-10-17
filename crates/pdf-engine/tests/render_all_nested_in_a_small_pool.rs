//! `render_all` called from inside a rayon pool must complete, not abort (#636).
//!
//! With the `parallel` feature, `render_all` fans pages over the global pool,
//! and every page used to wait on vello's own pool in turn. A worker that
//! waits across pools steals further queued page jobs onto its own stack,
//! which grows with every queued page until the process aborts -- so a page
//! rendered from a worker stays on the single-threaded path.
//!
//! The abort-prone workload runs in a child process (a filtered re-run of this
//! test binary): an abort fails the parent through the child's exit status
//! instead of taking down the whole suite.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#[cfg(feature = "parallel")]
use pdf_engine::{PdfDocument, RenderOptions};
#[cfg(not(feature = "parallel"))]
use test_skip::skip_test;

/// Set on the child process that runs the workload inline.
#[cfg(feature = "parallel")]
const CHILD_MARKER: &str = "PDF_ENGINE_NESTED_RENDER_ALL_CHILD";

/// `render_all` over 300 pages from a one-thread pool with a small stack.
#[cfg(feature = "parallel")]
fn run_nested_render_all() {
    let bytes = three_hundred_page_pdf();
    let doc = PdfDocument::open(bytes).expect("generated PDF should open");
    assert_eq!(doc.page_count(), 300);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .stack_size(512 * 1024)
        .build()
        .expect("test pool should build");
    pool.install(|| {
        let rendered = doc.render_all(&RenderOptions::default());
        assert_eq!(rendered.len(), 300);
        for page in &rendered {
            assert_eq!((page.width, page.height), (72, 72));
        }
    });
}

/// 300 empty A4-ish pages sharing one content stream.
#[cfg(feature = "parallel")]
fn three_hundred_page_pdf() -> Vec<u8> {
    use lopdf::{dictionary, Document, Object, Stream};

    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let content = Stream::new(dictionary! {}, b"1 0 0 rg 10 10 52 52 re f".to_vec());
    let content_id = doc.add_object(content);
    let mut kids = Vec::new();
    for _ in 0..300 {
        let page_id = doc.new_object_id();
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Page".to_vec()),
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(72), Object::Integer(72),
                ]),
                "Contents" => Object::Reference(content_id),
            }),
        );
        kids.push(Object::Reference(page_id));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(300),
        }),
    );
    let catalog_id = doc.new_object_id();
    doc.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        }),
    );
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("lopdf save should succeed");
    bytes
}

#[cfg(feature = "parallel")]
#[test]
fn render_all_nested_in_a_small_pool_completes() {
    if std::env::var_os(CHILD_MARKER).is_some() {
        run_nested_render_all();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("test binary path"))
        .args([
            "--exact",
            "render_all_nested_in_a_small_pool_completes",
            "--nocapture",
        ])
        .env(CHILD_MARKER, "1")
        // Pin the configuration the child measures: an inherited override
        // would force the single-threaded path and mask the bug.
        .env_remove("PDF_RENDER_THREADS")
        .env_remove("RAYON_NUM_THREADS")
        .output()
        .expect("child process should spawn");
    assert!(
        output.status.success(),
        "nested render_all aborted the child ({}):\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Without the feature this file would compile to zero tests -- silence that
/// reads as a pass. This one says what it needs instead (#636).
#[cfg(not(feature = "parallel"))]
#[test]
fn render_all_nested_in_a_small_pool_needs_the_parallel_feature() {
    skip_test!("needs --features parallel");
}
