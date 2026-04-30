//! Conservative /ToUnicode CMap source-code extractor + safe-stub builder
//! for the M8-REDACT-03 redaction strip path (#1339).
//!
//! This module exists because PDF /ToUnicode CMaps cannot be safely turned
//! into a uniform U+FFFD-everything map with a single broad `bfrange`: the
//! Adobe Tech Note 5014 / pdf_font::cmap parser treat
//! `<srcLo> <srcHi> <dstLo>` as an *auto-incrementing* destination, so
//! `<0000> <FFFF> <FFFD>` actually maps 0x0000→U+FFFD, 0x0001→U+FFFE, ...
//! The verify-before-code smoke tests (see `tests/tounicode_stub_smoke.rs`)
//! confirm this; one of those tests is kept as `#[ignore]` documentation.
//!
//! What this module does instead:
//!
//! 1. `extract_tounicode_codes`: walk the existing /ToUnicode CMap with a
//!    small whitespace-aware PostScript-style scanner (NOT regex — too
//!    fragile per #1339 design discussion) and enumerate the source codes
//!    that the original CMap maps. We do NOT need a full CMap semantics;
//!    only the source-code byte sequences (and their byte length) matter.
//!
//! 2. `build_stub_cmap`: emit a /ToUnicode CMap that lists every collected
//!    source code as a `bfchar` entry pointing at U+FFFD. No auto-increment,
//!    no information leak. Codespace ranges are copied verbatim from the
//!    original so consumers continue to parse the byte stream the same way.
//!
//! Conservative fallback: if the scanner cannot make sense of a CMap shape,
//! we return `Err(...)`. The redaction caller is required to surface this
//! up the stack rather than silently fall back to "remove /ToUnicode".
//! Silently removing /ToUnicode would (a) regress PDF/A-2u/3u conformance
//! and (b) leave the door open for TrueType-cmap-inverse / Differences
//! decoding fallback paths in pdf-extract, defeating the redaction goal.

use crate::error::{RedactError, Result};

/// Maximum number of source codes a single bfrange entry may enumerate.
/// Caps memory use against malformed CMaps that claim huge ranges.
const MAX_BFRANGE_SPAN: u32 = 65_536;

/// Maximum number of bfchar entries emitted per block. PDF spec (Adobe TN
/// 5014) caps this at 100 — we honour the same limit when building stubs.
const MAX_BFCHAR_PER_BLOCK: usize = 100;

/// Cumulative cap on extracted source codes across ALL bfchar / bfrange
/// blocks in a single /ToUnicode stream. The per-bfrange cap above only
/// bounds an individual range; an adversarial CMap can supply many legal
/// individual ranges (or many bfchar blocks) whose cumulative size still
/// exhausts memory. 1 MiB-of-codes is several orders of magnitude above
/// what real-world fonts mapping use (~10^4 CIDs typical) — well clear
/// of false rejections, low enough to refuse DoS-shaped inputs (Codex
/// P2 #1379 fix).
const MAX_TOTAL_EXTRACTED_CODES: usize = 1_048_576;

/// Marker key set on every stub /ToUnicode stream. Treated as a HINT
/// only by the redaction caller — the actual idempotency check must
/// validate the stream content via [`is_safe_stub_cmap`], because a
/// crafted PDF can set this marker on an attacker-controlled
/// /ToUnicode stream to bypass the strip (Codex P1 #1379 fix).
pub(crate) const STUB_MARKER_KEY: &[u8] = b"XfaRedactionStub";

/// Codes extracted from an existing /ToUnicode CMap.
#[derive(Debug, Clone)]
pub(crate) struct TounicodeCodes {
    /// Codespace ranges as `(lo, hi)` raw byte pairs. Defines the byte
    /// length of subsequent codes; the stub keeps these verbatim so
    /// consumers parse the byte stream identically.
    pub(crate) codespace_ranges: Vec<(Vec<u8>, Vec<u8>)>,
    /// Source codes that the original CMap mapped to *something*. The
    /// stub will map each of these to U+FFFD via `bfchar`.
    pub(crate) codes: Vec<Vec<u8>>,
}

/// Extract the source codes mapped by an existing /ToUnicode CMap.
///
/// Supports the three operator families commonly used in PDF /ToUnicode:
/// - `beginbfchar` ... `endbfchar`
/// - `beginbfrange` ... `endbfrange` with single-hex-string destination
/// - `beginbfrange` ... `endbfrange` with array destination
///
/// Returns `Err(UnsupportedToUnicodeCMap { ... })` if no codespace was
/// declared, no codes were enumerated, or any required hex token is
/// malformed. The caller MUST surface this error rather than silently
/// strip the /ToUnicode key.
pub(crate) fn extract_tounicode_codes(
    bytes: &[u8],
    font_resource_name: &str,
) -> Result<TounicodeCodes> {
    let mut scanner = Scanner::new(bytes);
    let mut codespace_ranges: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut codes: Vec<Vec<u8>> = Vec::new();

    while let Some(tok) = scanner.next_token() {
        match tok.as_slice() {
            b"begincodespacerange" => {
                parse_codespace_block(&mut scanner, &mut codespace_ranges, font_resource_name)?
            }
            b"beginbfchar" => {
                parse_bfchar_block(&mut scanner, &mut codes, font_resource_name)?;
            }
            b"beginbfrange" => {
                parse_bfrange_block(&mut scanner, &mut codes, font_resource_name)?;
            }
            // We deliberately ignore everything else (CMap header tokens,
            // /CIDSystemInfo dict, defineresource preamble, etc.). Only the
            // three operator families above carry source codes.
            _ => {}
        }
    }

    if codespace_ranges.is_empty() {
        return Err(unsupported(
            font_resource_name,
            "no codespacerange declared",
        ));
    }
    if codes.is_empty() {
        return Err(unsupported(
            font_resource_name,
            "no bfchar/bfrange entries extracted",
        ));
    }

    // Sanity: every code's byte length must match at least one codespace.
    let codespace_lens: Vec<usize> = codespace_ranges.iter().map(|(lo, _)| lo.len()).collect();
    for code in &codes {
        if !codespace_lens.contains(&code.len()) {
            return Err(unsupported(
                font_resource_name,
                "code byte-length does not match any codespacerange",
            ));
        }
    }

    Ok(TounicodeCodes {
        codespace_ranges,
        codes,
    })
}

/// Build a /ToUnicode CMap that maps every code in `codes` to U+FFFD via
/// `bfchar` entries. The codespace ranges are copied verbatim from the
/// input so the byte-stream parsing rules stay identical.
pub(crate) fn build_stub_cmap(codes: &TounicodeCodes) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(512 + codes.codes.len() * 16);

    out.extend_from_slice(
        b"/CIDInit /ProcSet findresource begin\n\
          12 dict begin\n\
          begincmap\n\
          /CIDSystemInfo\n\
          << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
          /CMapName /Adobe-Identity-UCS def\n\
          /CMapType 2 def\n",
    );

    // Codespace ranges, verbatim from the original.
    out.extend_from_slice(
        format!("{} begincodespacerange\n", codes.codespace_ranges.len()).as_bytes(),
    );
    for (lo, hi) in &codes.codespace_ranges {
        out.push(b'<');
        for byte in lo {
            out.extend_from_slice(format!("{byte:02X}").as_bytes());
        }
        out.extend_from_slice(b"> <");
        for byte in hi {
            out.extend_from_slice(format!("{byte:02X}").as_bytes());
        }
        out.extend_from_slice(b">\n");
    }
    out.extend_from_slice(b"endcodespacerange\n");

    // Emit bfchar blocks of at most MAX_BFCHAR_PER_BLOCK entries each.
    for chunk in codes.codes.chunks(MAX_BFCHAR_PER_BLOCK) {
        out.extend_from_slice(format!("{} beginbfchar\n", chunk.len()).as_bytes());
        for code in chunk {
            out.push(b'<');
            for byte in code {
                out.extend_from_slice(format!("{byte:02X}").as_bytes());
            }
            // U+FFFD as a single 16-bit Unicode scalar per /ToUnicode rules.
            out.extend_from_slice(b"> <FFFD>\n");
        }
        out.extend_from_slice(b"endbfchar\n");
    }

    out.extend_from_slice(
        b"endcmap\n\
          CMapName currentdict /CMap defineresource pop\n\
          end\n\
          end\n",
    );
    out
}

/// Validate that `bytes` represent a /ToUnicode CMap that is structurally
/// safe to treat as an "already stubbed" stream — meaning every bfchar
/// destination is exactly U+FFFD and there are no bfrange blocks (our
/// stub builder emits bfchar only). This is the content-level
/// idempotency check that complements the [`STUB_MARKER_KEY`] hint:
/// a crafted PDF can set the marker without the content actually being
/// safe, so the marker alone must NOT short-circuit the strip.
///
/// Returns `true` only when the stream:
/// 1. Contains at least one `beginbfchar` block;
/// 2. Every `bfchar` entry's destination token is literally `<FFFD>`;
/// 3. Contains no `beginbfrange` block (our stub never emits one).
///
/// Any deviation — bfrange present, non-U+FFFD destination, malformed
/// tokens, premature EOF, etc. — returns `false` and the caller MUST
/// replace the /ToUnicode with a freshly-built stub.
pub(crate) fn is_safe_stub_cmap(bytes: &[u8]) -> bool {
    let mut scanner = Scanner::new(bytes);
    let mut had_any_bfchar = false;

    while let Some(tok) = scanner.next_token() {
        match tok.as_slice() {
            b"beginbfchar" => loop {
                let next = match scanner.next_token() {
                    Some(t) => t,
                    None => return false,
                };
                if next == b"endbfchar" {
                    break;
                }
                // `next` is the source code; the destination follows.
                let dst = match scanner.next_token() {
                    Some(t) => t,
                    None => return false,
                };
                if !is_replacement_char_destination(&dst) {
                    return false;
                }
                had_any_bfchar = true;
            },
            b"beginbfrange" => {
                // Our build_stub_cmap never emits bfrange. Any bfrange
                // means this stream wasn't produced by the stub builder.
                return false;
            }
            b"usecmap" => {
                // PDF CMap-spec `usecmap` imports the mapping table of a
                // referenced CMap into the current one. A crafted /ToUnicode
                // can carry the XfaRedactionStub marker, a single trivial
                // `<00> <FFFD>` bfchar entry, AND `/Base usecmap` to inherit
                // real mappings from another CMap — passing a marker-only
                // idempotency check while still leaking text. `pdf-font`'s
                // CMap parser supports usecmap, so the extraction layer
                // would honour it on lookup. Reject any usecmap presence
                // for "safe stub" classification (Codex P1 #1379 fix).
                return false;
            }
            // Header tokens, codespace ranges, /CMapName, etc. are fine —
            // they don't affect mappings. Note: `findresource` and
            // `defineresource` appear in every standard CMap preamble
            // (e.g. `/CIDInit /ProcSet findresource begin`,
            //       `CMapName currentdict /CMap defineresource pop`)
            // and are NOT on their own indicators of mapping inheritance,
            // so we do not reject them.
            _ => {}
        }
    }

    had_any_bfchar
}

/// Returns true iff `token` is exactly `<FFFD>` (case-insensitive). We
/// do NOT accept array-form `[<FFFD>]`, padded variants, or multi-codepoint
/// strings like `<FFFD0020>` — only a strict single-codepoint
/// REPLACEMENT CHARACTER mapping qualifies as "safe stub".
fn is_replacement_char_destination(token: &[u8]) -> bool {
    if token.len() < 2 || token[0] != b'<' || token[token.len() - 1] != b'>' {
        return false;
    }
    let inner: Vec<u8> = token[1..token.len() - 1]
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    inner.eq_ignore_ascii_case(b"FFFD")
}

// ─── Internal scanner ────────────────────────────────────────────────────

struct Scanner<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.bytes.len() {
            let c = self.bytes[self.pos];
            if c == b'%' {
                // PostScript line comment: consume to end-of-line.
                // PDF spec §7.2.3 / PostScript spec accept any of `\n`,
                // `\r`, or CRLF as line terminators. Stopping only on
                // `\n` causes a CR-only-encoded CMap to swallow the rest
                // of the stream into one comment, breaking redaction on
                // otherwise-valid PDFs (Codex P2 #1379 fix).
                while self.pos < self.bytes.len() {
                    let cc = self.bytes[self.pos];
                    if cc == b'\n' || cc == b'\r' {
                        break;
                    }
                    self.pos += 1;
                }
                // Don't consume the EOL byte itself; the outer loop's
                // is_ascii_whitespace branch handles it (and absorbs the
                // trailing `\n` of a CRLF naturally).
            } else if c.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Return the next token as raw bytes (unparsed). Recognises:
    /// - hex strings `<XX..>` (returned including the angle brackets)
    /// - arrays `[...]` (returned including brackets, supports nesting)
    /// - bare tokens (names, numbers, operator keywords)
    fn next_token(&mut self) -> Option<Vec<u8>> {
        self.skip_whitespace_and_comments();
        if self.pos >= self.bytes.len() {
            return None;
        }
        let start = self.pos;
        let c = self.bytes[self.pos];
        if c == b'<' {
            self.pos += 1;
            while self.pos < self.bytes.len() && self.bytes[self.pos] != b'>' {
                self.pos += 1;
            }
            if self.pos < self.bytes.len() {
                self.pos += 1; // consume '>'
            }
            return Some(self.bytes[start..self.pos].to_vec());
        }
        if c == b'[' {
            let mut depth = 1usize;
            self.pos += 1;
            while self.pos < self.bytes.len() && depth > 0 {
                match self.bytes[self.pos] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    _ => {}
                }
                self.pos += 1;
            }
            return Some(self.bytes[start..self.pos].to_vec());
        }
        // Bare token: until whitespace or special opener.
        while self.pos < self.bytes.len() {
            let cc = self.bytes[self.pos];
            if cc.is_ascii_whitespace() || cc == b'<' || cc == b'[' || cc == b'%' {
                break;
            }
            self.pos += 1;
        }
        Some(self.bytes[start..self.pos].to_vec())
    }
}

fn parse_hex_string(token: &[u8], font: &str) -> Result<Vec<u8>> {
    if token.len() < 2 || token[0] != b'<' || token[token.len() - 1] != b'>' {
        return Err(unsupported(font, "expected hex string"));
    }
    let inner = &token[1..token.len() - 1];
    // Strip any internal whitespace (some generators allow it).
    let mut stripped: Vec<u8> = inner
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    // PDF spec §7.3.4.3 / PostScript hex strings: an odd number of hex
    // digits is permitted and is implicitly padded with a final `0`
    // nibble. Hard-rejecting odd-length input would refuse a class of
    // spec-valid CMaps and abort redaction with UnsupportedToUnicodeCMap
    // (Codex P2 #1379 fix).
    if !stripped.len().is_multiple_of(2) {
        stripped.push(b'0');
    }
    let mut out = Vec::with_capacity(stripped.len() / 2);
    for pair in stripped.chunks_exact(2) {
        let hi = hex_digit(pair[0]).ok_or_else(|| unsupported(font, "non-hex char"))?;
        let lo = hex_digit(pair[1]).ok_or_else(|| unsupported(font, "non-hex char"))?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(10 + (c - b'a')),
        b'A'..=b'F' => Some(10 + (c - b'A')),
        _ => None,
    }
}

fn parse_codespace_block(
    scanner: &mut Scanner,
    out: &mut Vec<(Vec<u8>, Vec<u8>)>,
    font: &str,
) -> Result<()> {
    loop {
        let tok = scanner
            .next_token()
            .ok_or_else(|| unsupported(font, "unterminated codespacerange block"))?;
        if tok == b"endcodespacerange" {
            return Ok(());
        }
        let lo = parse_hex_string(&tok, font)?;
        let hi_tok = scanner
            .next_token()
            .ok_or_else(|| unsupported(font, "missing codespace hi value"))?;
        let hi = parse_hex_string(&hi_tok, font)?;
        if lo.is_empty() || lo.len() != hi.len() || lo.len() > 4 {
            return Err(unsupported(
                font,
                "codespace range with mismatched or out-of-bounds byte length",
            ));
        }
        out.push((lo, hi));
    }
}

/// Append a code to `out`, enforcing the cumulative `MAX_TOTAL_EXTRACTED_CODES`
/// cap so an adversarial CMap with many legal-size blocks cannot exhaust
/// memory (Codex P2 #1379 fix).
fn try_push_code(out: &mut Vec<Vec<u8>>, code: Vec<u8>, font: &str) -> Result<()> {
    if out.len() >= MAX_TOTAL_EXTRACTED_CODES {
        return Err(unsupported(
            font,
            "cumulative extracted-code count exceeds safety cap \
             (refused as malformed/adversarial)",
        ));
    }
    out.push(code);
    Ok(())
}

fn parse_bfchar_block(scanner: &mut Scanner, out: &mut Vec<Vec<u8>>, font: &str) -> Result<()> {
    loop {
        let tok = scanner
            .next_token()
            .ok_or_else(|| unsupported(font, "unterminated bfchar block"))?;
        if tok == b"endbfchar" {
            return Ok(());
        }
        let src = parse_hex_string(&tok, font)?;
        // Discard the destination token (single hex string per spec).
        let _dst = scanner
            .next_token()
            .ok_or_else(|| unsupported(font, "bfchar entry missing destination"))?;
        if src.is_empty() || src.len() > 4 {
            return Err(unsupported(
                font,
                "bfchar source has unsupported byte length",
            ));
        }
        try_push_code(out, src, font)?;
    }
}

fn parse_bfrange_block(scanner: &mut Scanner, out: &mut Vec<Vec<u8>>, font: &str) -> Result<()> {
    loop {
        let tok = scanner
            .next_token()
            .ok_or_else(|| unsupported(font, "unterminated bfrange block"))?;
        if tok == b"endbfrange" {
            return Ok(());
        }
        let src_lo = parse_hex_string(&tok, font)?;
        let src_hi_tok = scanner
            .next_token()
            .ok_or_else(|| unsupported(font, "bfrange entry missing srcHi"))?;
        let src_hi = parse_hex_string(&src_hi_tok, font)?;
        // Read and discard the destination token (single hex or array).
        let _dst = scanner
            .next_token()
            .ok_or_else(|| unsupported(font, "bfrange entry missing destination"))?;

        if src_lo.len() != src_hi.len() || src_lo.is_empty() || src_lo.len() > 4 {
            return Err(unsupported(
                font,
                "bfrange src bounds with mismatched or out-of-bounds byte length",
            ));
        }
        let lo_int = bytes_to_u32(&src_lo);
        let hi_int = bytes_to_u32(&src_hi);
        if hi_int < lo_int {
            return Err(unsupported(font, "bfrange srcHi < srcLo"));
        }
        // Saturating arithmetic to avoid overflow on adversarial 4-byte
        // ranges like <00000000> to <FFFFFFFF>.
        let span = hi_int.saturating_sub(lo_int).saturating_add(1);
        if span > MAX_BFRANGE_SPAN {
            return Err(unsupported(
                font,
                "bfrange span exceeds safety cap (refused as malformed/adversarial)",
            ));
        }
        let byte_len = src_lo.len();
        for code_int in lo_int..=hi_int {
            // Check the cumulative cap inside the loop so a
            // pathological many-range stream fails fast partway through
            // a range instead of fully enumerating each individually-
            // legal range.
            try_push_code(out, u32_to_bytes(code_int, byte_len), font)?;
        }
    }
}

fn bytes_to_u32(bytes: &[u8]) -> u32 {
    let mut v = 0u32;
    for &b in bytes {
        v = (v << 8) | b as u32;
    }
    v
}

fn u32_to_bytes(v: u32, byte_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(byte_len);
    for i in (0..byte_len).rev() {
        out.push(((v >> (i * 8)) & 0xFF) as u8);
    }
    out
}

fn unsupported(font: &str, reason: &str) -> RedactError {
    RedactError::UnsupportedToUnicodeCMap {
        font_resource_name: font.to_string(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmap_with_bfchar() -> &'static [u8] {
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
3 beginbfchar
<41> <0041>
<42> <0042>
<43> <0043>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end
"
    }

    fn cmap_with_bfrange_single_dest() -> &'static [u8] {
        b"\
/CIDInit /ProcSet findresource begin
begincmap
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
1 beginbfrange
<0010> <0014> <0050>
endbfrange
endcmap
"
    }

    fn cmap_with_bfrange_array_dest() -> &'static [u8] {
        b"\
/CIDInit /ProcSet findresource begin
begincmap
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
1 beginbfrange
<00A0> <00A2> [<0061> <0062> <0063>]
endbfrange
endcmap
"
    }

    fn cmap_empty() -> &'static [u8] {
        b"\
/CIDInit /ProcSet findresource begin
begincmap
endcmap
"
    }

    #[test]
    fn extract_codes_from_bfchar_block() {
        let codes = extract_tounicode_codes(cmap_with_bfchar(), "F1").unwrap();
        assert_eq!(codes.codespace_ranges.len(), 1);
        assert_eq!(codes.codespace_ranges[0].0, vec![0x00]);
        assert_eq!(codes.codespace_ranges[0].1, vec![0xFF]);
        assert_eq!(codes.codes, vec![vec![0x41], vec![0x42], vec![0x43]]);
    }

    #[test]
    fn extract_codes_from_bfrange_single_dest_enumerates_source() {
        // <0010> <0014> <0050> — source codes 0x0010..=0x0014 (5 codes).
        // We do NOT use the destination; just enumerate src.
        let codes = extract_tounicode_codes(cmap_with_bfrange_single_dest(), "F2").unwrap();
        assert_eq!(codes.codes.len(), 5);
        assert_eq!(codes.codes[0], vec![0x00, 0x10]);
        assert_eq!(codes.codes[4], vec![0x00, 0x14]);
    }

    #[test]
    fn extract_codes_from_bfrange_array_dest_enumerates_source() {
        // <00A0> <00A2> [...] — source codes 0x00A0..=0x00A2 (3 codes).
        let codes = extract_tounicode_codes(cmap_with_bfrange_array_dest(), "F3").unwrap();
        assert_eq!(codes.codes.len(), 3);
        assert_eq!(codes.codes[0], vec![0x00, 0xA0]);
        assert_eq!(codes.codes[2], vec![0x00, 0xA2]);
    }

    #[test]
    fn extract_codes_returns_err_on_empty_cmap() {
        let err = extract_tounicode_codes(cmap_empty(), "F4").unwrap_err();
        assert!(matches!(err, RedactError::UnsupportedToUnicodeCMap { .. }));
    }

    #[test]
    fn extract_codes_caps_huge_bfrange_span() {
        let big = b"\
/CIDInit /ProcSet findresource begin
begincmap
1 begincodespacerange
<00000000> <FFFFFFFF>
endcodespacerange
1 beginbfrange
<00000000> <FFFFFFFF> <FFFD>
endbfrange
endcmap
";
        let err = extract_tounicode_codes(big, "F5").unwrap_err();
        assert!(matches!(err, RedactError::UnsupportedToUnicodeCMap { .. }));
    }

    #[test]
    fn build_stub_cmap_emits_codespace_and_bfchar_only() {
        let codes = TounicodeCodes {
            codespace_ranges: vec![(vec![0x00], vec![0xFF])],
            codes: vec![vec![0x41], vec![0x42], vec![0xFE]],
        };
        let bytes = build_stub_cmap(&codes);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("1 begincodespacerange"));
        assert!(s.contains("<00> <FF>"));
        assert!(s.contains("3 beginbfchar"));
        assert!(s.contains("<41> <FFFD>"));
        assert!(s.contains("<42> <FFFD>"));
        assert!(s.contains("<FE> <FFFD>"));
        // No broad bfrange anywhere — that was the whole point of #1339.
        assert!(!s.contains("beginbfrange"));
    }

    #[test]
    fn build_stub_cmap_splits_into_blocks_of_100() {
        let codes = TounicodeCodes {
            codespace_ranges: vec![(vec![0x00], vec![0xFF])],
            codes: (0u8..=199).map(|b| vec![b]).collect(),
        };
        let bytes = build_stub_cmap(&codes);
        let s = std::str::from_utf8(&bytes).unwrap();
        // Two 100-entry blocks expected.
        let block_count = s.matches("beginbfchar").count();
        assert_eq!(block_count, 2, "expected 2 bfchar blocks for 200 codes");
        assert!(s.contains("100 beginbfchar"));
    }

    #[test]
    fn extract_codes_handles_cr_only_line_endings_in_comments() {
        // Codex P2 #1379 regression-guard: PDF/PostScript spec accepts
        // \n, \r, or CRLF as line terminators. Without the fix the
        // comment scanner would only stop on \n, so a CR-only-encoded
        // CMap would have its comment "swallow" the rest of the stream
        // and the parser would report UnsupportedToUnicodeCMap on a
        // perfectly valid input.
        let cmap_cr_only: Vec<u8> = b"\
% comment line that ends with carriage return only
1 begincodespacerange
<00> <FF>
endcodespacerange
2 beginbfchar
<41> <0041>
<42> <0042>
endbfchar
endcmap
"
        .iter()
        .map(|&b| if b == b'\n' { b'\r' } else { b })
        .collect();

        let codes = extract_tounicode_codes(&cmap_cr_only, "F1")
            .expect("CR-only-line CMap with leading comment must parse");
        assert_eq!(codes.codes, vec![vec![0x41], vec![0x42]]);
    }

    #[test]
    fn extract_codes_handles_crlf_line_endings() {
        // CRLF is two bytes (\r\n) and historically the most common
        // PDF line terminator. The fix needs to leave CRLF behaviour
        // intact while also accepting bare \r.
        let cmap_crlf: Vec<u8> = b"\
% comment with CRLF terminator
1 begincodespacerange
<00> <FF>
endcodespacerange
1 beginbfchar
<5A> <005A>
endbfchar
endcmap
"
        .iter()
        .flat_map(|&b| {
            if b == b'\n' {
                vec![b'\r', b'\n']
            } else {
                vec![b]
            }
        })
        .collect();

        let codes = extract_tounicode_codes(&cmap_crlf, "F2")
            .expect("CRLF-terminated CMap with leading comment must parse");
        assert_eq!(codes.codes, vec![vec![0x5A]]);
    }

    #[test]
    fn extract_codes_pads_odd_length_hex_source_codes() {
        // Codex P2 #1379 regression-guard: PDF spec §7.3.4.3 permits hex
        // strings with an odd number of digits, with implicit `0`
        // padding on the trailing nibble. The conservative parser used
        // to hard-reject odd-length hex, aborting redaction on
        // spec-valid CMaps.
        //
        // Here `<4>` should be interpreted as `<40>` = byte 0x40, and
        // `<5>` as `<50>` = byte 0x50. Source destinations are
        // discarded by the extractor, so we don't assert on them.
        let cmap = b"\
1 begincodespacerange
<00> <FF>
endcodespacerange
2 beginbfchar
<4> <0041>
<5> <0042>
endbfchar
endcmap
";
        let codes = extract_tounicode_codes(cmap, "F3")
            .expect("CMap with odd-length hex source codes must parse with `0` padding");
        assert_eq!(codes.codes, vec![vec![0x40], vec![0x50]]);
    }

    #[test]
    fn parse_hex_string_pads_odd_length_directly() {
        // Direct unit-test for the hex parser. `<F>` → `<F0>` = 0xF0.
        let parsed = parse_hex_string(b"<F>", "F4").unwrap();
        assert_eq!(parsed, vec![0xF0]);
        // `<ABC>` → `<ABC0>` = [0xAB, 0xC0].
        let parsed = parse_hex_string(b"<ABC>", "F5").unwrap();
        assert_eq!(parsed, vec![0xAB, 0xC0]);
        // Even-length still works exactly the same.
        let parsed = parse_hex_string(b"<ABCD>", "F6").unwrap();
        assert_eq!(parsed, vec![0xAB, 0xCD]);
    }

    #[test]
    fn safe_stub_validator_accepts_our_own_stub_output() {
        // build_stub_cmap → is_safe_stub_cmap roundtrip MUST succeed,
        // otherwise our own idempotency check would reject our own output.
        let codes = TounicodeCodes {
            codespace_ranges: vec![(vec![0x00], vec![0xFF])],
            codes: vec![vec![0x41], vec![0x42], vec![0x43], vec![0xFE]],
        };
        let stub = build_stub_cmap(&codes);
        assert!(
            is_safe_stub_cmap(&stub),
            "build_stub_cmap output must validate as safe stub"
        );
    }

    #[test]
    fn safe_stub_validator_rejects_original_tounicode_with_real_mappings() {
        // Codex P1 #1379 regression-guard: a /ToUnicode that maps codes to
        // real characters (not U+FFFD) must NOT be accepted as a safe stub
        // even if the caller mistakenly passes its bytes here.
        let original = b"\
/CIDInit /ProcSet findresource begin
begincmap
1 begincodespacerange
<00> <FF>
endcodespacerange
3 beginbfchar
<41> <0041>
<42> <0042>
<43> <0043>
endbfchar
endcmap
";
        assert!(
            !is_safe_stub_cmap(original),
            "real-mapping CMap must NOT validate as safe stub"
        );
    }

    #[test]
    fn safe_stub_validator_rejects_partial_replacement_with_one_real_mapping() {
        // Mostly-stub but ONE entry maps to a real char — must reject.
        let crafted = b"\
/CIDInit /ProcSet findresource begin
begincmap
1 begincodespacerange
<00> <FF>
endcodespacerange
3 beginbfchar
<41> <FFFD>
<42> <FFFD>
<43> <0043>
endbfchar
endcmap
";
        assert!(
            !is_safe_stub_cmap(crafted),
            "any single non-U+FFFD destination disqualifies the stream as safe stub"
        );
    }

    #[test]
    fn safe_stub_validator_rejects_bfrange_block_even_when_dest_is_replacement_char() {
        // Our stub never emits bfrange. A stream containing bfrange (even
        // bfrange that looks like it maps to U+FFFD) is by definition not
        // ours and must not be honored as a safe stub.
        let stream_with_bfrange = b"\
/CIDInit /ProcSet findresource begin
begincmap
1 begincodespacerange
<00> <FF>
endcodespacerange
1 beginbfrange
<00> <FF> <FFFD>
endbfrange
endcmap
";
        assert!(
            !is_safe_stub_cmap(stream_with_bfrange),
            "bfrange presence must disqualify the stream as safe stub (even with U+FFFD dest)"
        );
    }

    #[test]
    fn safe_stub_validator_rejects_empty_stream() {
        assert!(!is_safe_stub_cmap(b""));
        assert!(!is_safe_stub_cmap(b"begincmap\nendcmap\n"));
    }

    #[test]
    fn safe_stub_validator_rejects_usecmap_inheritance() {
        // Codex P1 #1379 regression-guard: a crafted /ToUnicode that
        // carries the XfaRedactionStub marker, a single trivial
        // <00> <FFFD> bfchar entry, AND a `usecmap` operator referencing
        // a base CMap can pass a marker-only idempotency check while
        // still inheriting real glyph→Unicode mappings via the base.
        // pdf-font's CMap parser supports `usecmap`, so the leak surfaces
        // through pdf-extract on lookup. The validator must refuse to
        // classify such streams as "safe stub".
        let crafted = b"\
/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo
<< /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def
/CMapType 2 def
/Identity-H usecmap
1 begincodespacerange
<00> <FF>
endcodespacerange
1 beginbfchar
<00> <FFFD>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end
";
        assert!(
            !is_safe_stub_cmap(crafted),
            "stream carrying `usecmap` must not be classified as safe stub even with single FFFD bfchar entry"
        );
    }

    #[test]
    fn safe_stub_validator_still_accepts_streams_with_findresource_and_defineresource() {
        // Standard CMap preamble has `findresource` and `defineresource`
        // operators that are NOT mapping inheritance — they're resource-
        // namespace plumbing. Our own build_stub_cmap emits both. Make
        // sure the usecmap rejection didn't accidentally also reject
        // these benign operators.
        let codes = TounicodeCodes {
            codespace_ranges: vec![(vec![0x00], vec![0xFF])],
            codes: vec![vec![0x41]],
        };
        let stub = build_stub_cmap(&codes);
        // Sanity: our stub does include findresource + defineresource
        // (this is part of the standard CMap header/footer).
        assert!(std::str::from_utf8(&stub).unwrap().contains("findresource"));
        assert!(std::str::from_utf8(&stub)
            .unwrap()
            .contains("defineresource"));
        // And it must still validate.
        assert!(
            is_safe_stub_cmap(&stub),
            "standard CMap preamble (findresource/defineresource) must not trigger usecmap rejection"
        );
    }

    #[test]
    fn extract_codes_caps_cumulative_total_across_many_legal_ranges() {
        // Codex P2 #1379 regression-guard: build a CMap with many
        // individually-legal bfranges (each within MAX_BFRANGE_SPAN)
        // whose cumulative code count exceeds MAX_TOTAL_EXTRACTED_CODES
        // (1_048_576). Without the global cap, the extractor allocates
        // unbounded; with the fix it must surface UnsupportedToUnicodeCMap.
        //
        // 17 ranges × 65 536 codes = 1 114 112 > 1 048 576.
        let mut cmap: Vec<u8> = Vec::new();
        cmap.extend_from_slice(
            b"/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
1 begincodespacerange
<00000000> <FFFFFFFF>
endcodespacerange
",
        );
        for i in 0u32..17 {
            let lo = i * 65_536;
            let hi = lo + 65_535;
            cmap.extend_from_slice(
                format!(
                    "1 beginbfrange\n<{lo:08X}> <{hi:08X}> <FFFD>\nendbfrange\n",
                    lo = lo,
                    hi = hi
                )
                .as_bytes(),
            );
        }
        cmap.extend_from_slice(b"endcmap\n");

        let err = extract_tounicode_codes(&cmap, "F1").unwrap_err();
        assert!(
            matches!(err, RedactError::UnsupportedToUnicodeCMap { .. }),
            "cumulative-cap overflow must surface UnsupportedToUnicodeCMap, got {err:?}"
        );
    }

    #[test]
    fn extract_codes_caps_cumulative_total_across_many_bfchar_blocks() {
        // Same cap, exercised via many bfchar blocks (each well under
        // MAX_BFCHAR_PER_BLOCK = 100). 11 000 blocks × 100 codes each =
        // 1 100 000 > 1 048 576. Constructed compactly so the test data
        // stays manageable.
        let mut cmap: Vec<u8> = Vec::new();
        cmap.extend_from_slice(
            b"/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
",
        );
        // 11 000 × 100 = 1.1M codes.
        for block in 0u32..11_000 {
            cmap.extend_from_slice(b"100 beginbfchar\n");
            for entry in 0u32..100 {
                let code = block * 100 + entry;
                cmap.extend_from_slice(
                    format!("<{code:04X}> <0041>\n", code = code & 0xFFFF).as_bytes(),
                );
            }
            cmap.extend_from_slice(b"endbfchar\n");
        }
        cmap.extend_from_slice(b"endcmap\n");

        let err = extract_tounicode_codes(&cmap, "F2").unwrap_err();
        assert!(
            matches!(err, RedactError::UnsupportedToUnicodeCMap { .. }),
            "many-bfchar-block cumulative overflow must surface UnsupportedToUnicodeCMap"
        );
    }

    #[test]
    fn extract_then_build_stub_parses_via_pdf_font_and_returns_replacement_char() {
        // End-to-end: take a realistic /ToUnicode using a mix of bfchar +
        // bfrange (single + array dest), extract source codes via our
        // conservative scanner, build the stub via build_stub_cmap, parse
        // the resulting bytes with the canonical pdf_font::cmap::CMap, and
        // assert every original source code lookup returns U+FFFD.
        let original = b"\
/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
2 beginbfchar
<0010> <0048>
<0011> <0065>
endbfchar
2 beginbfrange
<0020> <0023> <006C>
<0030> <0032> [<006F> <0070> <0071>]
endbfrange
endcmap
";
        let codes = extract_tounicode_codes(original, "F1").unwrap();
        // Source codes expected: 0x10, 0x11 (bfchar), 0x20..=0x23 (bfrange
        // single), 0x30..=0x32 (bfrange array). Total 9 codes.
        assert_eq!(codes.codes.len(), 9);

        let stub_bytes = build_stub_cmap(&codes);

        let cmap = pdf_font::cmap::CMap::parse(
            &stub_bytes,
            |_: pdf_font::cmap::CMapName<'_>| -> Option<&'static [u8]> { None },
        )
        .expect("build_stub_cmap output must parse via pdf_font::cmap::CMap");

        for code in &codes.codes {
            let mut code_int = 0u32;
            for &b in code {
                code_int = (code_int << 8) | b as u32;
            }
            let bf = cmap
                .lookup_bf_string(code_int)
                .unwrap_or_else(|| panic!("stub CMap missed lookup for code 0x{code_int:04X}"));
            let chars: Vec<char> = match bf {
                pdf_font::cmap::BfString::Char(c) => vec![c],
                pdf_font::cmap::BfString::String(s) => s.chars().collect(),
            };
            assert!(
                chars.iter().all(|c| *c == '\u{FFFD}'),
                "code 0x{code_int:04X} resolved to {chars:?}, expected U+FFFD"
            );
        }
    }
}
