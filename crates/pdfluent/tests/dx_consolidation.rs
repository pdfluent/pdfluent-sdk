//! Integration tests for Epic 3 #1225 — DX consolidation.
//!
//! Covers the three consolidation moves:
//!
//! 1. Compress presets — `strict()` / `lossy()` / `archival()` return
//!    structurally sensible option sets.
//! 2. `PageDecoration` enum + `add_decoration(...)` is the
//!    consolidated entry point; `add_watermark(...)` delegates to it
//!    so both paths produce the same observable error shape today
//!    (pre-runtime wiring).
//! 3. Prelude hygiene — `use pdfluent::prelude::*;` brings the new
//!    types into scope without ambiguity.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;

// ---------------------------------------------------------------------------
// Compress presets
// ---------------------------------------------------------------------------

#[test]
fn compress_strict_preset_enables_full_stack() {
    let opts = CompressOptions::strict();
    assert!(opts.subset_fonts);
    assert!(opts.compress_streams);
    assert!(opts.deduplicate_streams);
    assert!(opts.remove_unused);
}

#[test]
fn compress_archival_preset_preserves_unused_objects() {
    let opts = CompressOptions::archival();
    // Subsetting + stream compression are reversible / deterministic,
    // so archival keeps them on.
    assert!(opts.subset_fonts);
    assert!(opts.compress_streams);
    assert!(opts.deduplicate_streams);
    // But archival keeps unused objects so incremental updates and
    // signed appearance streams stay addressable.
    assert!(!opts.remove_unused);
}

#[test]
fn compress_lossy_preset_exists_and_matches_strict_today() {
    // Lossy downsampling is a 1.1 pass. Until then, `lossy()` and
    // `strict()` are the same pass set. The preset exists so callers
    // commit to intent now and stay forward-compatible.
    let lossy = CompressOptions::lossy();
    let strict = CompressOptions::strict();
    assert_eq!(lossy.subset_fonts, strict.subset_fonts);
    assert_eq!(lossy.compress_streams, strict.compress_streams);
    assert_eq!(lossy.deduplicate_streams, strict.deduplicate_streams);
    assert_eq!(lossy.remove_unused, strict.remove_unused);
}

#[test]
fn compress_with_strict_preset_runs_end_to_end() {
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let before = doc.page_count();

    let report = doc
        .compress(CompressOptions::strict())
        .expect("compress strict");

    assert_eq!(doc.page_count(), before);
    // Preset ran all four passes — at minimum the font_subset field is
    // Some (pass enabled).
    assert!(report.font_subset.is_some());
}

// ---------------------------------------------------------------------------
// PageDecoration consolidation
// ---------------------------------------------------------------------------

#[test]
fn add_watermark_delegates_to_add_decoration() {
    let _ = pdfluent::set_license_key("tier:business");

    // Both entry points must reach the same runtime. Until 23-08-2026 this
    // asserted that both failed identically, because the facade refused while
    // the watermark runtime it claimed to be waiting for was already there and
    // already reachable from the C ABI.
    let mut via_method = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    via_method
        .add_watermark("DRAFT", WatermarkOptions::centered())
        .expect("add_watermark");

    let mut via_enum = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    via_enum
        .add_decoration(PageDecoration::watermark(
            "DRAFT",
            WatermarkOptions::centered(),
        ))
        .expect("add_decoration");

    assert_eq!(
        via_method.to_bytes().expect("serialise method"),
        via_enum.to_bytes().expect("serialise enum"),
        "the convenience wrapper and the enum produced different documents"
    );
}

#[test]
fn add_decoration_watermark_carries_text_through_enum() {
    // Construct-only test: we can't observe option internals (the
    // accessors are crate-private) and the runtime is deferred, but
    // the enum variant pattern-matches as documented.
    let dec = PageDecoration::watermark("© ACME", WatermarkOptions::centered().rotated(30.0));

    match dec {
        PageDecoration::Watermark { text, options: _ } => {
            assert_eq!(text, "© ACME");
        }
        // `PageDecoration` is non-exhaustive so future variants can
        // land without breaking this test.
        _ => unreachable!("no other variants in 1.0"),
    }
}

// ---------------------------------------------------------------------------
// Prelude hygiene
// ---------------------------------------------------------------------------

#[test]
fn prelude_star_import_exposes_new_types_without_collision() {
    // If this test compiles, every type the prelude re-exports is
    // unambiguously usable via `use pdfluent::prelude::*;`.
    let _: CompressOptions = CompressOptions::strict();
    let _: ImageFormat = ImageFormat::Png;
    let _: InsertImageFormat = InsertImageFormat::Png;
    let _: PageDecoration = PageDecoration::watermark("x", WatermarkOptions::centered());
    let _: Rotation = Rotation::Clockwise90;

    // `ImageFormat` (parity, for to_images output) and
    // `InsertImageFormat` (parity, for insert_image input) are two
    // distinct types. This line would fail to compile if they collided.
    fn takes_output(_: ImageFormat) {}
    fn takes_input(_: InsertImageFormat) {}
    takes_output(ImageFormat::Jpeg);
    takes_input(InsertImageFormat::Jpeg);
}

// ---------------------------------------------------------------------------
// std::fs next to pdfluent, in one function
// ---------------------------------------------------------------------------
//
// Landed here rather than in `the_api_contract_holds.rs`, where it was written
// on 30-08-2026: that file is not on master and the repair it carried never got
// there either, while docs/KWALITEITSSPOOR.md records it as done and counts the
// blocks it fixed. Four examples on pdfluent.com were still failing on it on
// 05-09-2026 (#164).

/// `std::fs` next to `pdfluent` in the same function.
///
/// Every recipe that reads bytes, writes the result, or lists a directory sits
/// in a function returning `pdfluent::Result`, and reaches for `?` on both
/// kinds of error. Without `From<std::io::Error>` the second one refuses. Ten
/// Rust examples on pdfluent.com failed to compile for that reason and no
/// other, which is a trait-bound error as a visitor's first impression of the
/// SDK (#245).
///
/// Written as a function that is compiled rather than run: the assertion is
/// that `?` accepts an `std::io::Error` here, and that is a question for the
/// compiler. The body is executed anyway so the mapping is checked too — a
/// `From` that panics or picks the wrong variant would pass a compile-only
/// test.
#[test]
fn an_io_error_reaches_the_facade_error() {
    fn reads_and_opens() -> pdfluent::Result<usize> {
        let bytes = std::fs::read("/nonexistent/pdfluent-api-contract.pdf")?;
        Ok(bytes.len())
    }

    let err = reads_and_opens().expect_err("the path does not exist");
    assert!(
        matches!(err, pdfluent::Error::Io { .. }),
        "an std::io::Error must land in Error::Io, not in a catch-all: {err:?}"
    );
    // The chain stays intact: `source()` still hands back the std error, which
    // is what a caller matches on to tell "file missing" from "permission
    // denied".
    let source = std::error::Error::source(&err).expect("Error::Io chains its source");
    assert!(
        source.downcast_ref::<std::io::Error>().is_some(),
        "the source of Error::Io must still be the std::io::Error"
    );
}
