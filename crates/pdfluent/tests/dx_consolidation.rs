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
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");

    // Both entry points must produce the same error (truth-gap:
    // watermark runtime tracked under #1223).
    let via_method = doc
        .add_watermark("DRAFT", WatermarkOptions::centered())
        .expect_err("unwired runtime");
    let via_enum = doc
        .add_decoration(PageDecoration::watermark(
            "DRAFT",
            WatermarkOptions::centered(),
        ))
        .expect_err("unwired runtime");

    assert_eq!(via_method.code(), via_enum.code());
    assert_eq!(via_method.code(), "E-ENV-MISSING-DEPENDENCY");
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
