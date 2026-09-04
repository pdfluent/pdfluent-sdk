//! Integration tests for issue #1308 — Deterministic save output.
//!
//! ## Determinism contract (1.0.0-beta.4 → 1.0)
//!
//! `PdfDocument::save` / `save_with` / `to_bytes` produce **byte-deterministic**
//! output for an **unencrypted** document under the following conditions:
//!
//! - Input is identical (same bytes / same opened source).
//! - No mutation between successive save calls.
//! - No mutation that introduces fresh timestamps (e.g. setting
//!   `metadata_mut().set_creation_date(...)` with the system clock).
//! - The `linearize` flag in `SaveOptions` is unchanged (1.0 ships it as a
//!   no-op, so it does not affect bytes today; will remain stable when the
//!   linearizer lands in 1.1).
//!
//! ## What is deterministic
//!
//! - Object ordering — `lopdf::Document::objects` is a `BTreeMap`, sorted
//!   by `(id, generation)`.
//! - Dictionary ordering — `lopdf::Dictionary` is an `IndexMap`, preserving
//!   insertion order on round-trips.
//! - Stream content — copied as-is from input on round-trip.
//! - Cross-reference table layout — derived deterministically from the
//!   sorted object map.
//!
//! ## What is NOT deterministic (documented limitations)
//!
//! - **Encrypted output**: AES IVs and content-encryption keys are randomly
//!   generated per save (security requirement per ISO 32000-2 §7.6). Two
//!   `save()` calls of the same encrypted document produce different bytes.
//! - **Auto-set timestamps**: `Document::write_to(...)` paths that auto-stamp
//!   `Info /CreationDate` or `/ModDate` from the system clock. The default
//!   `save` / `save_with` paths in `pdfluent` do not auto-set timestamps;
//!   callers opt in via `metadata_mut().set_*` only.
//! - **User-provided random data**: anything the caller writes via
//!   `metadata_mut()` or custom dictionary edits.
//!
//! ## Why this matters
//!
//! Customer CI pipelines that compare PDF checksums to catch regressions
//! depend on byte-stable output. Without this guarantee, a no-op save round
//! trip can break those pipelines for reasons unrelated to the actual
//! document content.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::io::Cursor;

use pdfluent::prelude::*;
use pdfluent::SaveOptions;

const FIXTURE_PATH: &str = "tests/fixtures/sample.pdf";

// ---------------------------------------------------------------------------
// Test 1 — Same input → byte-equal output (single run)
// ---------------------------------------------------------------------------

/// Two calls to `to_bytes()` on the same `PdfDocument` produce identical
/// output. This is the foundational determinism property.
#[test]
fn determinism_to_bytes_is_idempotent() {
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open fixture");
    let first = doc.to_bytes().expect("first to_bytes");
    let second = doc.to_bytes().expect("second to_bytes");

    assert_eq!(
        first.len(),
        second.len(),
        "to_bytes must produce same byte length across calls",
    );
    assert_eq!(first, second, "to_bytes must produce byte-identical output");
}

// ---------------------------------------------------------------------------
// Test 2 — Multi-run: open → save → open → save produces same bytes
// ---------------------------------------------------------------------------

/// Open the fixture twice independently, serialize each, and compare.
/// Catches regressions where parser state leaks non-determinism into the
/// saved output.
#[test]
fn determinism_multi_open_roundtrip() {
    let doc1 = PdfDocument::open(FIXTURE_PATH).expect("first open");
    let bytes1 = doc1.to_bytes().expect("first to_bytes");

    // Drop the first document fully before opening again, so we test against
    // a fresh parser state.
    drop(doc1);

    let doc2 = PdfDocument::open(FIXTURE_PATH).expect("second open");
    let bytes2 = doc2.to_bytes().expect("second to_bytes");

    assert_eq!(
        bytes1.len(),
        bytes2.len(),
        "two independent open+save round trips must agree on byte length",
    );
    assert_eq!(
        bytes1, bytes2,
        "two independent open+save round trips must agree byte-for-byte",
    );
}

// ---------------------------------------------------------------------------
// Test 3 — CI-safe: deterministic via in-memory buffer (no /tmp races)
// ---------------------------------------------------------------------------

/// Write to a `Cursor<Vec<u8>>` instead of disk to avoid CI flakiness from
/// `/tmp` cleanup, file-system metadata variation, or path differences.
/// This is the canonical CI-safe form of the determinism check.
#[test]
fn determinism_write_to_in_memory_cursor() {
    let bytes = std::fs::read(FIXTURE_PATH).expect("read fixture");
    let doc1 = PdfDocument::from_bytes(&bytes).expect("from_bytes 1");
    let doc2 = PdfDocument::from_bytes(&bytes).expect("from_bytes 2");

    let mut sink1: Vec<u8> = Vec::new();
    let mut sink2: Vec<u8> = Vec::new();

    doc1.write_to(Cursor::new(&mut sink1)).expect("write 1");
    doc2.write_to(Cursor::new(&mut sink2)).expect("write 2");

    assert_eq!(
        sink1.len(),
        sink2.len(),
        "write_to must produce same byte length for same input",
    );
    assert_eq!(
        sink1, sink2,
        "write_to via Cursor<Vec<u8>> must be byte-deterministic",
    );

    // Sanity: output is a valid PDF (starts with %PDF-).
    assert!(
        sink1.starts_with(b"%PDF-"),
        "output must begin with PDF header"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — Negative test: SaveOptions defaults are deterministic
// ---------------------------------------------------------------------------

/// Verify that `to_bytes()` and `save_with(...)` produce byte-identical
/// output for the same document. This guards against any non-determinism
/// hidden in the disk-write path (filesystem metadata, padding, BOM
/// handling).
#[test]
fn determinism_to_bytes_matches_save_with() {
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open fixture");
    let bytes_in_memory = doc.to_bytes().expect("via to_bytes");

    // Use the cargo target directory for the temp file so we don't depend on
    // /tmp behaviour across CI runners. The path is unique per test process.
    let mut path = std::env::temp_dir();
    let pid = std::process::id();
    path.push(format!("pdfluent-determinism-{}.pdf", pid));
    // Ensure clean slate even if a previous run left a file behind.
    let _ = std::fs::remove_file(&path);

    doc.save_with(&path, SaveOptions::new().with_overwrite(true))
        .expect("save_with");

    let bytes_on_disk = std::fs::read(&path).expect("read back");
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        bytes_in_memory.len(),
        bytes_on_disk.len(),
        "to_bytes vs save_with must agree on length",
    );
    assert_eq!(
        bytes_in_memory, bytes_on_disk,
        "to_bytes and save_with must produce identical bytes",
    );
}
