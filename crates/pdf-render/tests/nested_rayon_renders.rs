//! Pages rendered from inside a rayon pool must complete, not abort (#636).
//!
//! The rasterizer used to dispatch every page to vello's own rayon pool. When
//! the caller was itself a worker of another pool, it waited across pools
//! while rayon stole further queued page jobs onto the same stack, which grew
//! with every queued page until the process aborted. Rendering from a worker
//! must therefore stay on the single-threaded path.
//!
//! The abort-prone workload runs in a child process (a filtered re-run of this
//! test binary): an abort fails the parent through the child's exit status
//! instead of taking down the whole suite.

#![cfg(not(target_arch = "wasm32"))]

use pdf_render::pdf_interpret::InterpreterSettings;
use pdf_render::pdf_syntax::Pdf;
use pdf_render::{RenderSettings, render};
use rayon::prelude::*;
use std::process::Command;

/// Set on the child process that runs the workload inline.
const CHILD_MARKER: &str = "PDF_RENDER_NESTED_CHILD";

/// Renders queued on a one-thread pool with a small stack: without the worker
/// guard every render waits on vello's own pool while stealing the next queued
/// render onto the same stack, until the worker aborts (#636).
fn run_nested_renders() {
    let bytes = fixture_pdf_bytes();
    let pdf = Pdf::new(bytes).expect("fixture PDF should load");
    let pages = pdf.pages();
    let page = &pages[0];
    let settings = InterpreterSettings::default();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .stack_size(512 * 1024)
        .build()
        .expect("test pool should build");
    pool.install(|| {
        (0..400usize).into_par_iter().for_each(|_| {
            let pixmap = render(
                page,
                &settings,
                &RenderSettings {
                    x_scale: 2.0,
                    y_scale: 2.0,
                    ..Default::default()
                },
            );
            assert_eq!(pixmap.width(), 144);
            assert_eq!(pixmap.height(), 144);
        });
    });
}

/// One text line plus a filled rectangle, so rasterization does real work.
fn fixture_pdf_bytes() -> Vec<u8> {
    use lopdf::{Document, Object, Stream, dictionary};

    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let content = Stream::new(
        dictionary! {},
        b"1 0 0 rg 10 10 52 52 re f BT /F1 12 Tf 10 40 Td (hello) Tj ET".to_vec(),
    );
    let content_id = doc.add_object(content);
    let font_id = doc.new_object_id();
    doc.objects.insert(
        font_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        }),
    );
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
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "F1" => Object::Reference(font_id),
                }),
            }),
        }),
    );
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
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

/// Re-run this test binary filtered to one test, with the workload marker set.
/// Returns the child's captured output; the caller decides what it proves.
fn run_child(test_name: &str, extra_env: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(std::env::current_exe().expect("test binary path"));
    command
        .args(["--exact", test_name, "--nocapture"])
        .env(CHILD_MARKER, "1")
        // Pin the configuration the child measures: an inherited override
        // would force the single-threaded path and mask the bug.
        .env_remove("PDF_RENDER_THREADS")
        .env_remove("RAYON_NUM_THREADS");
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command.output().expect("child process should spawn")
}

#[test]
fn nested_pool_renders_complete() {
    if std::env::var_os(CHILD_MARKER).is_some() {
        run_nested_renders();
        return;
    }
    let output = run_child("nested_pool_renders_complete", &[]);
    assert!(
        output.status.success(),
        "nested renders aborted the child ({}):\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// `PDF_RENDER_THREADS=1` must take the single-threaded path, observed through
/// the trace line a plain (non-nested) render prints. Without the mapping the
/// child reports `threads=1`.
#[test]
fn env_threads_one_selects_single_threaded_path() {
    if std::env::var_os(CHILD_MARKER).is_some() {
        let bytes = fixture_pdf_bytes();
        let pdf = Pdf::new(bytes).expect("fixture PDF should load");
        let pages = pdf.pages();
        render(
            &pages[0],
            &InterpreterSettings::default(),
            &RenderSettings {
                x_scale: 2.0,
                y_scale: 2.0,
                ..Default::default()
            },
        );
        return;
    }
    let output = run_child(
        "env_threads_one_selects_single_threaded_path",
        &[("PDF_RENDER_THREADS", "1"), ("PDF_RENDER_TRACE", "1")],
    );
    assert!(
        output.status.success(),
        "trace child failed ({}):\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("threads=0"),
        "expected the trace line to report threads=0, child stderr was:\n{stderr}"
    );
}
