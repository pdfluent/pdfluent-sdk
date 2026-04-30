//! M8-REDACT-03 stub-compatibility smoke test.
//!
//! Verifies that the candidate /ToUnicode stub CMaps we plan to write into
//! redacted pages are actually accepted by the `pdf_font::cmap::CMap` parser
//! that `pdf-extract` and `pdf-manip` use to decode text. If this test fails
//! we must NOT proceed with the page-scoped clone-on-write strategy: the
//! stub would silently parse to nothing or to a non-U+FFFD lookup, defeating
//! the whole point of the redaction strip.
//!
//! The test runs BEFORE any production code for #1339 is wired up, so it
//! acts as the verify-before-code gate.

use pdf_font::cmap::{BfString, CMap};

/// Stub /ToUnicode CMap for a 2-byte (Type0/CID-keyed) font: every code in
/// the codespace 0000..FFFF maps to U+FFFD (REPLACEMENT CHARACTER). This is
/// the candidate we want to drop into Type0 fonts on redacted pages.
const STUB_TOUNICODE_CMAP_2BYTE: &[u8] = b"\
/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo
<< /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def
/CMapType 2 def
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
1 beginbfrange
<0000> <FFFF> <FFFD>
endbfrange
endcmap
CMapName currentdict /CMap defineresource pop
end
end
";

/// Stub /ToUnicode CMap for a 1-byte (Simple/Type1/TrueType) font: every
/// code in 00..FF maps to U+FFFD.
const STUB_TOUNICODE_CMAP_1BYTE: &[u8] = b"\
/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo
<< /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def
/CMapType 2 def
1 begincodespacerange
<00> <FF>
endcodespacerange
1 beginbfrange
<00> <FF> <FFFD>
endbfrange
endcmap
CMapName currentdict /CMap defineresource pop
end
end
";

fn no_embedded(_: pdf_font::cmap::CMapName<'_>) -> Option<&'static [u8]> {
    None
}

fn assert_lookup_returns_replacement_char(cmap: &CMap, code: u32, byte_len: u8) {
    let lookup = cmap.lookup_bf_string(code);
    let bf = lookup.unwrap_or_else(|| {
        panic!(
            "stub CMap rejected lookup for code 0x{:0width$X} (byte_len={})",
            code,
            byte_len,
            width = (byte_len as usize) * 2
        )
    });
    let chars: Vec<char> = match bf {
        BfString::Char(c) => vec![c],
        BfString::String(s) => s.chars().collect(),
    };
    assert!(
        !chars.is_empty(),
        "stub CMap returned empty BfString for code 0x{:0width$X}",
        code,
        width = (byte_len as usize) * 2
    );
    assert!(
        chars.iter().all(|c| *c == '\u{FFFD}'),
        "stub CMap lookup for code 0x{:0width$X} returned {:?}, expected only U+FFFD",
        code,
        chars,
        width = (byte_len as usize) * 2
    );
}

// ─── Negative documentation: broad bfrange does NOT yield uniform U+FFFD ──
//
// Adobe Tech Note 5014 specifies that `bfrange <srcLo> <srcHi> <dstLo>` with
// a single hex-string destination AUTO-INCREMENTS the destination per source
// code: <0000> <FFFF> <FFFD> means 0x0000→U+FFFD, 0x0001→U+FFFE, etc. The
// canonical pdf_font::cmap::CMap parser implements this correctly, so a
// broad bfrange cannot be used as a uniform "everything → U+FFFD" stub for
// the M8-REDACT-03 redaction strip. These two tests are kept as #[ignore]
// regression-guards: they should ALWAYS fail-when-not-ignored, documenting
// why the M8-REDACT-03 implementation uses bfchar instead.

#[test]
#[ignore = "documents auto-increment trap; do NOT use broad bfrange in the M8-REDACT-03 stub"]
fn negative_doc_broad_bfrange_2byte_auto_increments_not_uniform() {
    let cmap = CMap::parse(STUB_TOUNICODE_CMAP_2BYTE, no_embedded).unwrap();
    for code in [
        0x0000_u32, 0x0001, 0x0042, 0x00FF, 0x1234, 0xABCD, 0xFFFE, 0xFFFF,
    ] {
        assert_lookup_returns_replacement_char(&cmap, code, 2);
    }
}

#[test]
#[ignore = "documents auto-increment trap; do NOT use broad bfrange in the M8-REDACT-03 stub"]
fn negative_doc_broad_bfrange_1byte_auto_increments_not_uniform() {
    let cmap = CMap::parse(STUB_TOUNICODE_CMAP_1BYTE, no_embedded).unwrap();
    for code in [0x00_u32, 0x01, 0x20, 0x41, 0x7E, 0x80, 0xFE, 0xFF] {
        assert_lookup_returns_replacement_char(&cmap, code, 1);
    }
}

/// ALTERNATIVE A — many bfchar entries (256 mappings, no auto-increment).
/// PDF spec: bfchar destination is taken literally; each src→dst pair is
/// independent. This avoids the bfrange auto-increment trap.
fn build_bfchar_1byte_stub() -> Vec<u8> {
    let mut out = Vec::with_capacity(8 * 1024);
    out.extend_from_slice(
        b"\
/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo
<< /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def
/CMapType 2 def
1 begincodespacerange
<00> <FF>
endcodespacerange
",
    );
    // PDF spec caps bfchar blocks at 100 entries; emit 3 blocks of <=100.
    for chunk_start in (0u32..=255u32).step_by(100) {
        let chunk_end = (chunk_start + 99).min(255);
        let n = chunk_end - chunk_start + 1;
        out.extend_from_slice(format!("{n} beginbfchar\n").as_bytes());
        for code in chunk_start..=chunk_end {
            out.extend_from_slice(format!("<{:02X}> <FFFD>\n", code).as_bytes());
        }
        out.extend_from_slice(b"endbfchar\n");
    }
    out.extend_from_slice(
        b"endcmap
CMapName currentdict /CMap defineresource pop
end
end
",
    );
    out
}

/// ALTERNATIVE C — bfrange with array-form destination.
/// `<00> <FF> [<FFFD> <FFFD> ... 256x]` — array length matches the range
/// length, each src code gets the corresponding array entry. No auto-
/// increment: every entry is U+FFFD literal.
fn build_bfrange_array_1byte_stub() -> Vec<u8> {
    let mut out = Vec::with_capacity(2 * 1024);
    out.extend_from_slice(
        b"\
/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo
<< /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def
/CMapType 2 def
1 begincodespacerange
<00> <FF>
endcodespacerange
1 beginbfrange
<00> <FF> [",
    );
    for _ in 0..=255u32 {
        out.extend_from_slice(b"<FFFD>");
    }
    out.extend_from_slice(
        b"]
endbfrange
endcmap
CMapName currentdict /CMap defineresource pop
end
end
",
    );
    out
}

#[test]
fn alternative_a_bfchar_1byte_stub_parses_and_maps_all_codes_to_replacement_char() {
    let bytes = build_bfchar_1byte_stub();
    let cmap =
        CMap::parse(&bytes, no_embedded).expect("Alternative A — 1-byte bfchar stub must parse");

    for code in [0x00_u32, 0x01, 0x20, 0x41, 0x7E, 0x80, 0xFE, 0xFF] {
        assert_lookup_returns_replacement_char(&cmap, code, 1);
    }
}

#[test]
fn alternative_c_bfrange_array_1byte_stub_parses_and_maps_all_codes_to_replacement_char() {
    let bytes = build_bfrange_array_1byte_stub();
    let cmap = CMap::parse(&bytes, no_embedded)
        .expect("Alternative C — 1-byte bfrange-array stub must parse");

    for code in [0x00_u32, 0x01, 0x20, 0x41, 0x7E, 0x80, 0xFE, 0xFF] {
        assert_lookup_returns_replacement_char(&cmap, code, 1);
    }
}
