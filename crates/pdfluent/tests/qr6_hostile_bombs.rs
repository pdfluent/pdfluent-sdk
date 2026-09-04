//! QR-6 release-recheck: hostile / decompression-bomb input shapes.
//!
//! Tiny synthetic, safe-to-commit byte payloads (no real bomb files). For
//! each shape the invariant is: `from_bytes` (and a follow-up read if it
//! parses) **never panics** and returns **bounded** (a typed error or a safe
//! parse) within a wall-clock budget — i.e. no hang, no OOM, no panic.
//!
//! Stream-size / pixel / depth limit *enforcement* is already proven by
//! `processing_limits.rs`; this lane pins the specific adversarial shapes.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::panic::catch_unwind;
use std::time::{Duration, Instant};

use pdfluent::PdfDocument;

/// Build a minimal PDF wrapper around a body, with a header + trailer.
fn wrap(body: &str) -> Vec<u8> {
    format!("%PDF-1.7\n{body}\ntrailer\n<< /Root 1 0 R >>\nstartxref\n0\n%%EOF\n").into_bytes()
}

fn deep_nested_dict(depth: usize) -> Vec<u8> {
    // 1 0 obj << /A << /A << ... >> >> endobj
    let mut s = String::from("1 0 obj\n");
    for _ in 0..depth {
        s.push_str("<< /A ");
    }
    s.push('0');
    for _ in 0..depth {
        s.push_str(" >>");
    }
    s.push_str("\nendobj\n");
    wrap(&s)
}

fn deep_nested_array(depth: usize) -> Vec<u8> {
    let mut s = String::from("1 0 obj\n");
    s.push_str(&"[".repeat(depth));
    s.push('0');
    s.push_str(&"]".repeat(depth));
    s.push_str("\nendobj\n");
    wrap(&s)
}

fn object_cycle() -> Vec<u8> {
    // 1 -> 2 -> 1 reference cycle.
    wrap("1 0 obj\n<< /Next 2 0 R >>\nendobj\n2 0 obj\n<< /Next 1 0 R >>\nendobj")
}

fn huge_declared_length() -> Vec<u8> {
    // Stream claims a 4 GiB length but carries 4 bytes — must not allocate it.
    wrap("1 0 obj\n<< /Length 4294967296 >>\nstream\nabcd\nendstream\nendobj")
}

fn nested_filters() -> Vec<u8> {
    // Many stacked filters on a tiny stream.
    wrap("1 0 obj\n<< /Length 4 /Filter [/FlateDecode /FlateDecode /FlateDecode /ASCIIHexDecode] >>\nstream\nabcd\nendstream\nendobj")
}

fn many_objects() -> Vec<u8> {
    let mut s = String::new();
    for i in 1..2000 {
        s.push_str(&format!("{i} 0 obj\n<< /N {i} >>\nendobj\n"));
    }
    wrap(&s)
}

#[test]
fn qr6_hostile_shapes_are_bounded_and_never_panic() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("deep_nested_dict", deep_nested_dict(5000)),
        ("deep_nested_array", deep_nested_array(5000)),
        ("object_cycle", object_cycle()),
        ("huge_declared_length", huge_declared_length()),
        ("nested_filters", nested_filters()),
        ("many_objects", many_objects()),
    ];
    let budget = Duration::from_secs(10);
    for (tag, bytes) in cases {
        let start = Instant::now();
        let res = catch_unwind(|| {
            let d = PdfDocument::from_bytes(&bytes);
            if let Ok(doc) = d {
                // touch the heavy read paths too
                let _ = doc.page_count();
                let _ = doc.extract_text();
                let _ = doc.to_bytes();
            }
        });
        let elapsed = start.elapsed();
        assert!(res.is_ok(), "{tag}: hostile input must not panic");
        assert!(
            elapsed < budget,
            "{tag}: hostile input took {elapsed:?} (> {budget:?}) — not bounded"
        );
    }
}
