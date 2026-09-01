// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! Regression tests for JBIG2 fixes taken from hayro upstream.
//!
//! The fixtures under `tests/fixtures/upstream/` come from the hayro test
//! corpus (Laurenz Stampfl, MIT/Apache-2.0) and are the exact inputs the
//! upstream fixes were written against.

struct NullDecoder {
    pixels: u64,
}

impl pdfluent_jbig2::Decoder for NullDecoder {
    fn push_pixel(&mut self, _black: bool) {
        self.pixels += 1;
    }

    fn push_pixel_chunk(&mut self, _black: bool, chunk_count: u32) {
        self.pixels += chunk_count as u64 * 8;
    }

    fn next_line(&mut self) {}
}

/// LaurenzV/hayro#1104, and a test that cannot bite -- stated here rather than
/// counted as proof.
///
/// Upstream's overflow was in `extract_3bits`, written `rbx + 1 >= WORD_BITS`,
/// inside the word-buffered `RefinementContextGatherer` that upstream added in
/// March 2026. Our fork branched before that: `generic_refinement.rs` still
/// refines pixel by pixel through `get_pixel(bitmap, x: i32, y: i32)`, which is
/// bounds-checked and does no word-boundary arithmetic at all. There is no
/// `extract_3bits` here to break, so no mutation of our code can make this test
/// fail for the reason upstream's fixture was written.
///
/// What it still buys: the fixture is a real malformed refinement region from
/// upstream's corpus, and decoding it must not take the process with it. That
/// is a smoke test, not a regression test, and it is worth exactly that much.
#[test]
fn refinement_tpgr_overflow() {
    let file = include_bytes!("fixtures/upstream/refinement_tpgr_overflow.jb2");
    let mut decoder = NullDecoder { pixels: 0 };

    // The only requirement is that a malformed file is refused or decoded, not
    // that it takes the process with it.
    if let Ok(image) = pdfluent_jbig2::decode(file) {
        image.decode(&mut decoder);
    }
}

/// LaurenzV/hayro#1262. A generic region declaring a zero-width bitmap.
///
/// Proven to bite: with `(bitmap.width - 1) / 8` reintroduced at the top of
/// `decode_bitmap_arithmetic_coding` -- the arithmetic upstream's fast path
/// introduced and this guard protects -- the (0, 4) case fails with "attempt to
/// subtract with overflow". Adding the guard back makes the same mutation pass,
/// so the guard is what protects, not the surrounding code.
///
/// A probe confirmed all three shapes reach `decode_bitmap_arithmetic_coding`
/// rather than being rejected earlier: w=0 h=4, w=4 h=0, w=0 h=0.
#[test]
fn zero_dimension_generic_region() {
    for (width, height) in [(0_u32, 4_u32), (4, 0), (0, 0)] {
        let file = zero_dimension_jbig2(width, height);
        let mut decoder = NullDecoder { pixels: 0 };

        if let Ok(image) = pdfluent_jbig2::decode(&file) {
            image.decode(&mut decoder);
        }
    }
}

/// Build a minimal sequential JBIG2 file holding one immediate lossless
/// generic region with the requested dimensions, arithmetic-coded.
fn zero_dimension_jbig2(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();

    // File header: ID string, flags (sequential, known page count), page count.
    out.extend_from_slice(&[0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A]);
    out.push(0b0000_0001);
    out.extend_from_slice(&1_u32.to_be_bytes());

    // Segment 0: page information (type 48), page association 1.
    let mut page_info = Vec::new();
    page_info.extend_from_slice(&width.max(1).to_be_bytes());
    page_info.extend_from_slice(&height.max(1).to_be_bytes());
    page_info.extend_from_slice(&0_u32.to_be_bytes());
    page_info.extend_from_slice(&0_u32.to_be_bytes());
    page_info.push(0);
    page_info.extend_from_slice(&0_u16.to_be_bytes());
    push_segment(&mut out, 0, 48, &page_info);

    // Segment 1: immediate lossless generic region (type 38).
    let mut region = Vec::new();
    region.extend_from_slice(&width.to_be_bytes());
    region.extend_from_slice(&height.to_be_bytes());
    region.extend_from_slice(&0_u32.to_be_bytes());
    region.extend_from_slice(&0_u32.to_be_bytes());
    region.push(0);
    // Generic region flags: arithmetic coding, template 0, no TPGDON.
    region.push(0);
    // Four adaptive template pixel pairs for template 0.
    region.extend_from_slice(&[3, 255, 253, 255, 2, 254, 254, 254]);
    // A short arithmetic-coded payload; the decoder must not read past it.
    region.extend_from_slice(&[0x00, 0x00, 0xFF, 0xAC]);
    push_segment(&mut out, 1, 38, &region);

    out
}

fn push_segment(out: &mut Vec<u8>, number: u32, seg_type: u8, payload: &[u8]) {
    out.extend_from_slice(&number.to_be_bytes());
    // Header flags: segment type in the low six bits, page association is one
    // byte.
    out.push(seg_type & 0x3F);
    // Referred-to segments: count 0, no retain bits.
    out.push(0);
    // Page association.
    out.push(1);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
}
