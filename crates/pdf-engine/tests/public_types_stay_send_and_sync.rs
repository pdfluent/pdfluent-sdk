//! The public types the C4/C5a work touched keep `Send` and `Sync` (spec
//! `specs/text/circlecopyrt.md`, review 14-10 00:20).
//!
//! Each assertion states only what held at the base commit, `611ecf0~1`,
//! checked there by a probe before this file was written: the extraction
//! device and the span and block types were `Send` and `Sync`. The
//! interpreter's `Glyph`, `OutlineGlyph`, `Type3Glyph` and `Context` were
//! neither there -- they hold the interpreter's reference-counted fonts -- so
//! there is nothing to assert for them; the work adds only plain numbers to
//! them, and keeps the page's placement record in a thread-local registry.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

fn send_and_sync<T: Send + Sync>() {}

#[test]
fn the_extraction_device_and_its_output_stay_send_and_sync() {
    send_and_sync::<pdf_engine::TextExtractionDevice>();
    send_and_sync::<pdf_engine::TextSpan>();
    send_and_sync::<pdf_engine::TextBlock>();
}

/// The place the extraction device keeps in place of its last glyph (spec
/// review 14-10 07:18): plain numbers, so the device stays `Send` and `Sync`
/// for as long as it holds one.
#[test]
fn the_glyph_place_stays_send_and_sync() {
    send_and_sync::<pdf_render::pdf_interpret::font::GlyphPlace>();
}
