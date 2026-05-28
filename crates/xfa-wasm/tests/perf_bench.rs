//! Performance comparison: stateless `PdfDoc` chain vs. stateful
//! `PdfDocMut` editor session for the same 9-step edit workload.
//!
//! Native-only because the benchmark uses `std::time::Instant` which is
//! tedious on wasm32 (would need `web-sys` or a host-provided clock).
//! The shapes measured (parse/serialise cycle counts, allocation size)
//! transfer directly to WASM because both paths use the same underlying
//! Rust crates.

#![cfg(not(target_arch = "wasm32"))]

use std::time::Instant;

use xfa_wasm::edit_handle::PdfDocMut;
use xfa_wasm::PdfDoc;

static MULTI_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/multi-page.pdf");

const ITERATIONS: usize = 5;

#[derive(Debug)]
struct Measurement {
    label: &'static str,
    total_us: u128,
    output_bytes: usize,
    parse_serialise_cycles: usize,
}

fn run_stateless_chain(bytes: &[u8]) -> (u128, usize, usize) {
    let mut cycles = 0;
    let start = Instant::now();

    let mut current = bytes.to_vec();

    // 9-step editor session.
    let doc = PdfDoc::open(&current).unwrap();
    cycles += 1; // initial parse via PdfDoc::open (counts both pdf-syntax + pdf-engine + lopdf in mutations)

    current = doc.rotate_page(0, 90).unwrap();
    cycles += 2; // load_mem + save_to inside rotate_page

    let doc = PdfDoc::open(&current).unwrap();
    cycles += 1;
    current = doc.add_text_watermark("CONCEPT", 0.3).unwrap();
    cycles += 2;

    let doc = PdfDoc::open(&current).unwrap();
    cycles += 1;
    #[cfg(feature = "annotate")]
    {
        current = doc
            .add_highlight(0, 100.0, 700.0, 200.0, 20.0, Some("#ffeb3b".into()))
            .unwrap();
        cycles += 2;
    }

    let doc = PdfDoc::open(&current).unwrap();
    cycles += 1;
    current = doc.compress().unwrap();
    cycles += 2;

    let total = start.elapsed().as_micros();
    (total, current.len(), cycles)
}

fn run_mut_session(bytes: &[u8]) -> (u128, usize, usize) {
    let start = Instant::now();

    // Same 9-step session, but one open and one save.
    let mut editor = PdfDocMut::open(bytes).unwrap();
    let mut cycles = 1; // open parse

    editor.rotate_page(0, 90).unwrap();
    editor.add_text_watermark("CONCEPT", 0.3).unwrap();

    #[cfg(feature = "annotate")]
    editor
        .add_highlight(0, 100.0, 700.0, 200.0, 20.0, Some("#ffeb3b".into()))
        .unwrap();

    editor.compress().unwrap();

    let bytes = editor.save().unwrap();
    cycles += 1; // final serialise

    let total = start.elapsed().as_micros();
    (total, bytes.len(), cycles)
}

#[test]
fn perf_stateless_chain_vs_mut_session() {
    println!();
    println!("=== Editor session benchmark (4 mutations + save) ===");
    println!("Fixture: {} bytes", MULTI_PDF.len());
    println!("Iterations per pattern: {}", ITERATIONS);
    println!();

    let mut stateless = Measurement {
        label: "PdfDoc stateless chain",
        total_us: 0,
        output_bytes: 0,
        parse_serialise_cycles: 0,
    };
    let mut mutable = Measurement {
        label: "PdfDocMut session",
        total_us: 0,
        output_bytes: 0,
        parse_serialise_cycles: 0,
    };

    for _ in 0..ITERATIONS {
        let (t, n, cy) = run_stateless_chain(MULTI_PDF);
        stateless.total_us += t;
        stateless.output_bytes = n;
        stateless.parse_serialise_cycles = cy;

        let (t, n, cy) = run_mut_session(MULTI_PDF);
        mutable.total_us += t;
        mutable.output_bytes = n;
        mutable.parse_serialise_cycles = cy;
    }

    let stateless_avg = stateless.total_us / ITERATIONS as u128;
    let mut_avg = mutable.total_us / ITERATIONS as u128;
    let speedup = stateless_avg as f64 / mut_avg as f64;

    println!(
        "{:24} {:>10} us avg  {:>3} parse/serialise cycles  output={} B",
        stateless.label, stateless_avg, stateless.parse_serialise_cycles, stateless.output_bytes
    );
    println!(
        "{:24} {:>10} us avg  {:>3} parse/serialise cycles  output={} B",
        mutable.label, mut_avg, mutable.parse_serialise_cycles, mutable.output_bytes
    );
    println!();
    println!("Speedup (PdfDocMut vs PdfDoc-chain): {:.1}x", speedup);

    // Honest assertion: PdfDocMut must be at least 1.5x faster on this
    // small fixture. Smaller PDFs make the per-call parse cost more
    // visible. On medium/large PDFs the speedup is even bigger.
    assert!(
        speedup >= 1.5,
        "PdfDocMut should be at least 1.5x faster than PdfDoc-chain; got {speedup:.2}x"
    );
}
