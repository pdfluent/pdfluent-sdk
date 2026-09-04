//! Append a single blank glyph to a CFF (Type1C) font program.
//!
//! # Why this exists
//!
//! PDF producers routinely emit subset fonts that omit the space glyph: it
//! draws nothing, so subsetters drop it. The content stream still shows the
//! character code, and the PDF font dictionary still declares a width for it.
//! That is tolerated by ordinary readers but violates PDF/A:
//!
//! - ISO 19005-2 §6.2.11.4.1 test 2 — every glyph referenced for rendering
//!   must be defined in the embedded program;
//! - ISO 19005-2 §6.2.11.5 test 1 — the width in the font dictionary must
//!   match the width in the embedded program. When the code falls through to
//!   `.notdef`, veraPDF compares `.notdef`'s advance against the declared
//!   width and reports a mismatch.
//!
//! Substituting the whole font would fix both, but silently changes glyph
//! shapes — unacceptable for an archival format, and destructive for symbol
//! subsets (a ZapfDingbats subset carrying a single dingbat would lose it).
//! So we splice in exactly one new glyph and leave every existing charstring
//! byte-identical.
//!
//! # Approach
//!
//! Nothing already in the file moves. The new charset and CharStrings INDEX are
//! written after the original bytes, and only the two offset operands in the Top
//! DICT are edited in place, preserving their encoded width. The Encoding,
//! Private DICT, local Subrs and String INDEX therefore keep their exact
//! offsets and cannot be disturbed.
//!
//! Callers keep the original bytes whenever [`append_blank_glyph`] returns
//! `None`, which it does for CID-keyed fonts, predefined charsets, an offset
//! that will not fit the operand it replaces, or any post-check failure.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// Standard CFF SID for the glyph name `space`.
const SID_SPACE: u16 = 1;

/// A parsed CFF INDEX: the raw entries plus the byte range it occupied.
struct Index {
    entries: Vec<Vec<u8>>,
    end: usize,
}

fn read_u8(d: &[u8], p: usize) -> Option<u8> {
    d.get(p).copied()
}

fn read_u16(d: &[u8], p: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(p)?, *d.get(p + 1)?]))
}

fn read_offset(d: &[u8], p: usize, off_size: u8) -> Option<usize> {
    let mut v: usize = 0;
    for i in 0..off_size as usize {
        v = (v << 8) | *d.get(p + i)? as usize;
    }
    Some(v)
}

/// Parse a CFF INDEX starting at `pos`.
fn parse_index(d: &[u8], pos: usize) -> Option<Index> {
    let count = read_u16(d, pos)? as usize;
    if count == 0 {
        return Some(Index {
            entries: Vec::new(),
            end: pos + 2,
        });
    }
    let off_size = read_u8(d, pos + 2)?;
    if !(1..=4).contains(&off_size) {
        return None;
    }
    let offsets_start = pos + 3;
    let data_start = offsets_start + (count + 1) * off_size as usize - 1;
    let mut offsets = Vec::with_capacity(count + 1);
    for i in 0..=count {
        offsets.push(read_offset(
            d,
            offsets_start + i * off_size as usize,
            off_size,
        )?);
    }
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let s = data_start.checked_add(offsets[i])?;
        let e = data_start.checked_add(offsets[i + 1])?;
        if s > e || e > d.len() {
            return None;
        }
        entries.push(d[s..e].to_vec());
    }
    Some(Index {
        entries,
        end: data_start + offsets[count],
    })
}

/// Serialise entries as a CFF INDEX with a fixed 4-byte offset size.
fn write_index(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    if entries.is_empty() {
        return out;
    }
    out.push(4); // offSize
    let mut off: u32 = 1;
    out.extend_from_slice(&off.to_be_bytes());
    for e in entries {
        off += e.len() as u32;
        out.extend_from_slice(&off.to_be_bytes());
    }
    for e in entries {
        out.extend_from_slice(e);
    }
    out
}

/// One Top DICT entry: its operands (raw bytes) and operator bytes.
struct DictEntry {
    operands: Vec<u8>,
    operator: Vec<u8>,
}

/// Split a DICT into (operands, operator) pairs without interpreting operands,
/// so entries we do not care about survive untouched.
fn parse_dict(d: &[u8]) -> Option<Vec<DictEntry>> {
    let mut out = Vec::new();
    let mut operands: Vec<u8> = Vec::new();
    let mut i = 0usize;
    while i < d.len() {
        let b0 = d[i];
        match b0 {
            // operators
            0..=21 => {
                let op = if b0 == 12 {
                    let o = d.get(i..i + 2)?.to_vec();
                    i += 2;
                    o
                } else {
                    i += 1;
                    vec![b0]
                };
                out.push(DictEntry {
                    operands: std::mem::take(&mut operands),
                    operator: op,
                });
            }
            28 => {
                operands.extend_from_slice(d.get(i..i + 3)?);
                i += 3;
            }
            29 => {
                operands.extend_from_slice(d.get(i..i + 5)?);
                i += 5;
            }
            30 => {
                // real number: nibble-encoded, terminated by 0xf nibble
                let start = i;
                i += 1;
                loop {
                    let b = *d.get(i)?;
                    i += 1;
                    if b & 0x0f == 0x0f || b & 0xf0 == 0xf0 {
                        break;
                    }
                }
                operands.extend_from_slice(d.get(start..i)?);
            }
            32..=246 => {
                operands.push(b0);
                i += 1;
            }
            247..=254 => {
                operands.extend_from_slice(d.get(i..i + 2)?);
                i += 2;
            }
            _ => return None, // reserved
        }
    }
    Some(out)
}

/// Decode a DICT operand sequence as integers.
fn decode_operands(d: &[u8]) -> Vec<i32> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < d.len() {
        let b0 = d[i];
        match b0 {
            28 => {
                if i + 2 < d.len() {
                    out.push(i16::from_be_bytes([d[i + 1], d[i + 2]]) as i32);
                }
                i += 3;
            }
            29 => {
                if i + 4 < d.len() {
                    out.push(i32::from_be_bytes([d[i + 1], d[i + 2], d[i + 3], d[i + 4]]));
                }
                i += 5;
            }
            30 => {
                // Nibble-encoded real number (CFF spec §6). Private DICTs of
                // TeX-derived CFFs store nominalWidthX/defaultWidthX as reals
                // (e.g. 501.4375); skipping them made `private_widths` read 0
                // and write a blank glyph whose advance came out nominalWidthX
                // too wide. Decode and truncate toward zero — the same
                // arithmetic veraPDF applies, and within ±1 of the full value.
                i += 1;
                let mut s = String::new();
                while i < d.len() {
                    let b = d[i];
                    i += 1;
                    let mut done = false;
                    for nib in [b >> 4, b & 0x0f] {
                        match nib {
                            0x0..=0x9 => s.push((b'0' + nib) as char),
                            0xa => s.push('.'),
                            0xb => s.push('E'),
                            0xc => s.push_str("E-"),
                            0xe => s.push('-'),
                            0xf => {
                                done = true;
                                break;
                            }
                            _ => {}
                        }
                    }
                    if done {
                        break;
                    }
                }
                if let Ok(v) = s.parse::<f64>() {
                    out.push(v.trunc() as i32);
                }
            }
            32..=246 => {
                out.push(b0 as i32 - 139);
                i += 1;
            }
            247..=250 => {
                if i + 1 < d.len() {
                    out.push((b0 as i32 - 247) * 256 + d[i + 1] as i32 + 108);
                }
                i += 2;
            }
            251..=254 => {
                if i + 1 < d.len() {
                    out.push(-(b0 as i32 - 251) * 256 - (d[i + 1] as i32) - 108);
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    out
}

/// Encode an integer as a Type 2 charstring operand.
fn t2_int(v: i32) -> Vec<u8> {
    if (-107..=107).contains(&v) {
        vec![(v + 139) as u8]
    } else if (108..=1131).contains(&v) {
        let w = v - 108;
        vec![((w >> 8) + 247) as u8, (w & 0xff) as u8]
    } else if (-1131..=-108).contains(&v) {
        let w = -v - 108;
        vec![((w >> 8) + 251) as u8, (w & 0xff) as u8]
    } else {
        vec![28, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8]
    }
}

/// Read a charset into the list of SIDs for glyphs 1..nGlyphs-1.
fn parse_charset(d: &[u8], off: usize, n_glyphs: usize) -> Option<Vec<u16>> {
    // Predefined charsets (0=ISOAdobe, 1=Expert, 2=ExpertSubset) are identity
    // over the standard strings; we only handle embedded charsets, which is
    // what subset fonts in PDFs always carry.
    if off <= 2 {
        return None;
    }
    let fmt = read_u8(d, off)?;
    let mut sids = Vec::with_capacity(n_glyphs.saturating_sub(1));
    match fmt {
        0 => {
            for i in 0..n_glyphs.saturating_sub(1) {
                sids.push(read_u16(d, off + 1 + i * 2)?);
            }
        }
        1 | 2 => {
            let mut p = off + 1;
            while sids.len() < n_glyphs.saturating_sub(1) {
                let first = read_u16(d, p)?;
                let n_left = if fmt == 1 {
                    read_u8(d, p + 2)? as usize
                } else {
                    read_u16(d, p + 2)? as usize
                };
                p += if fmt == 1 { 3 } else { 4 };
                for k in 0..=n_left {
                    if sids.len() >= n_glyphs.saturating_sub(1) {
                        break;
                    }
                    sids.push(first.checked_add(k as u16)?);
                }
            }
        }
        _ => return None,
    }
    Some(sids)
}

/// Serialise a charset in format 0.
fn write_charset(sids: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + sids.len() * 2);
    out.push(0);
    for s in sids {
        out.extend_from_slice(&s.to_be_bytes());
    }
    out
}

/// Append a blank glyph named `space` with the given advance width.
///
/// Everything already in the file keeps its exact byte offset: the new charset
/// and CharStrings INDEX are written *after* the original bytes, and only the
/// two offset operands in the Top DICT are edited in place, preserving their
/// encoded width. Nothing else moves, so the Encoding, Private DICT, local
/// Subrs and String INDEX cannot be disturbed. An earlier version rebuilt the
/// whole file and relocated those structures, which silently corrupted the
/// code-to-glyph mapping of an unrelated font.
///
/// Returns `None` when the font already has the glyph, when anything about the
/// structure is not fully understood, when a new offset will not fit the width
/// of the operand it replaces, or when the result fails its own sanity check —
/// the caller then keeps the original bytes. Use [`try_append_blank_glyph`]
/// when you need to know which of those it was.
pub fn append_blank_glyph(data: &[u8], width: f64) -> Option<Vec<u8>> {
    try_append_blank_glyph(data, width).ok()
}

/// Why a font could not be given a `space` glyph.
///
/// Every variant means "left the font untouched". They are worth
/// distinguishing only when investigating why a document still fails
/// validation after the repair pass ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendRefused {
    /// The Top DICT is missing, malformed, or names a CID-keyed font.
    TopDict,
    /// The CharStrings INDEX could not be parsed.
    CharStrings,
    /// The charset is predefined or in a format this code does not read.
    Charset,
    /// The charset does not describe exactly one SID per non-`.notdef` glyph.
    CharsetGlyphCountMismatch,
    /// The font already has a `space` glyph.
    AlreadyPresent,
    /// The Private DICT's width defaults could not be read.
    PrivateDict,
    /// A new offset does not fit the byte width of the operand it replaces.
    OffsetTooWide,
    /// The rewritten font failed its own re-parse check.
    SanityCheck,
}

/// [`append_blank_glyph`], but reporting which precondition failed.
pub fn try_append_blank_glyph(
    data: &[u8],
    width: f64,
) -> std::result::Result<Vec<u8>, AppendRefused> {
    use AppendRefused as R;

    let (charset_span, charstrings_span, charset_off, charstrings_off) =
        top_dict_spans(data).ok_or(R::TopDict)?;

    let charstrings = parse_index(data, charstrings_off).ok_or(R::CharStrings)?;
    let n_glyphs = charstrings.entries.len();
    let sids = parse_charset(data, charset_off, n_glyphs).ok_or(R::Charset)?;
    if sids.len() + 1 != n_glyphs {
        return Err(R::CharsetGlyphCountMismatch);
    }
    if sids.contains(&SID_SPACE) {
        return Err(R::AlreadyPresent);
    }

    // Width operand for the blank glyph, relative to nominalWidthX.
    let (nominal_width, default_width) = private_widths(data).ok_or(R::PrivateDict)?;
    let w = width.round() as i32;
    let mut charstring = Vec::new();
    if w != default_width {
        charstring.extend_from_slice(&t2_int(w - nominal_width));
    }
    charstring.push(14); // endchar

    let mut new_charstrings = charstrings.entries.clone();
    new_charstrings.push(charstring);
    let mut new_sids = sids.clone();
    new_sids.push(SID_SPACE);

    let mut out = data.to_vec();
    let new_charset_off = out.len();
    out.extend_from_slice(&write_charset(&new_sids));
    let new_charstrings_off = out.len();
    out.extend_from_slice(&write_index(&new_charstrings));

    // Preferred path: rewrite both offsets without changing the Top DICT's
    // length, so nothing in the file moves.
    let in_place = patch_operand_in_place(&mut out, &charset_span, new_charset_off as i32)
        .and_then(|()| {
            patch_operand_in_place(&mut out, &charstrings_span, new_charstrings_off as i32)
        });
    let out = match in_place {
        Some(()) => out,
        // The operands are encoded too narrowly to hold an offset near the end
        // of the file — the common case for small subsets, where the original
        // offsets fit in one or two bytes. Rebuild the Top DICT with full-width
        // operands and shift everything after it. (60 of the 60 fonts blocked
        // here on the govdocs sample were this case.)
        None => {
            rebuild_with_wide_offsets(data, &new_sids, &new_charstrings).ok_or(R::OffsetTooWide)?
        }
    };
    let (new_charset_off, new_charstrings_off) = {
        let (_, _, cs, chs) = top_dict_spans(&out).ok_or(R::SanityCheck)?;
        (cs, chs)
    };

    // Sanity: the result must re-parse, expose exactly one more glyph, and keep
    // every original charstring byte-identical.
    let check = parse_index(&out, new_charstrings_off).ok_or(R::SanityCheck)?;
    if check.entries.len() != n_glyphs + 1 {
        return Err(R::SanityCheck);
    }
    for (a, b) in charstrings.entries.iter().zip(check.entries.iter()) {
        if a != b {
            return Err(R::SanityCheck);
        }
    }
    if parse_charset(&out, new_charset_off, n_glyphs + 1).ok_or(R::SanityCheck)? != new_sids {
        return Err(R::SanityCheck);
    }
    Ok(out)
}

/// Inline a CFF Encoding's supplement section into the main table.
///
/// Encoding supplements (format flag 0x80) map code → SID, and the consumer
/// must resolve SID → GID through the charset. veraPDF 1.28.2 does not: it
/// stores the raw SID where a GID−1 belongs, so every supplement-defined code
/// falls back to the .notdef advance in its width check (measured on govdocs
/// 002_002202 — all nine failing codes across five TeX subset fonts were
/// supplement codes). Folding the supplements into the main encoding table
/// changes nothing about the mapping — only its representation moves.
///
/// A supplement whose glyph already has a main-table code is a *duplicate*:
/// format 0 holds one code per gid, so one of the two has to win. `preferred`
/// is the set of codes the document actually uses with this font; a
/// supplement only replaces the main code when the supplement's code is used
/// and the main code is not. Otherwise the supplement is left in place
/// (govdocs 002_002202's Times-Roman subset maps both 32 and 160 to the
/// space glyph — inlining 160 over 32 broke code 32).
///
/// Returns `None` for anything that is not a supplemented custom encoding,
/// when no supplement needs inlining, or when the merged mapping cannot be
/// represented as one format 0 table (a gid without a code — format 0 cannot
/// skip gids).
pub fn inline_encoding_supplements(
    data: &[u8],
    preferred: &std::collections::HashSet<u8>,
) -> Option<Vec<u8>> {
    let (enc_span, enc_off) = top_dict_encoding_span(data)?;
    if enc_off <= 1 {
        return None; // predefined Standard/Expert encoding: no supplements
    }
    let fmt = *data.get(enc_off)? as usize;
    if fmt & 0x80 == 0 {
        return None; // no supplement section
    }

    // Main table: gid (1-based) → code, per spec index+1.
    let mut gid_code: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    let mut p = enc_off + 1;
    match fmt & 0x7f {
        0 => {
            let n = *data.get(p)? as usize;
            p += 1;
            for (i, code) in data.get(p..p + n)?.iter().enumerate() {
                gid_code.insert(i + 1, *code as usize);
            }
            p += n;
        }
        1 => {
            let n_ranges = *data.get(p)? as usize;
            p += 1;
            let mut gid = 1usize;
            for _ in 0..n_ranges {
                let first = *data.get(p)? as usize;
                let n_left = *data.get(p + 1)? as usize;
                p += 2;
                for j in 0..=n_left {
                    gid_code.insert(gid, first + j);
                    gid += 1;
                }
            }
        }
        _ => return None,
    }

    // Supplements: code → SID, resolved to a GID through the charset.
    let cff = cff_parser::Table::parse(data)?;
    let n_sups = *data.get(p)? as usize;
    p += 1;
    let mut remaining_sups: Vec<(u8, u16)> = Vec::new();
    let mut inlined = 0usize;
    for _ in 0..n_sups {
        let code = *data.get(p)? as usize;
        let sid = u16::from_be_bytes([*data.get(p + 1)?, *data.get(p + 2)?]);
        p += 3;
        let gid = cff.charset.sid_to_gid(cff_parser::StringId(sid))?;
        let gid = gid.0 as usize;
        match gid_code.get(&gid) {
            // Pure addition: the glyph has no main-table code.
            None => {
                gid_code.insert(gid, code);
                inlined += 1;
            }
            // Duplicate. Inline only when the supplement's code is the used
            // one; otherwise keep the original supplement entry untouched.
            Some(&main_code) => {
                if main_code != code
                    && preferred.contains(&(code as u8))
                    && !preferred.contains(&(main_code as u8))
                {
                    gid_code.insert(gid, code);
                    inlined += 1;
                } else {
                    remaining_sups.push((code as u8, sid));
                }
            }
        }
    }
    if inlined == 0 {
        return None;
    }

    // Emit the main table as format 0: codes[gid-1] = code. A gid without a
    // code cannot be skipped in this format — refuse rather than guess.
    let max_gid = *gid_code.keys().next_back()?;
    if max_gid > 255 {
        return None;
    }
    let mut codes = Vec::with_capacity(max_gid);
    for gid in 1..=max_gid {
        codes.push(*gid_code.get(&gid)? as u8);
    }
    let mut block = vec![0u8, codes.len() as u8];
    block.extend_from_slice(&codes);
    if !remaining_sups.is_empty() {
        block[0] = 0x80;
        block.push(remaining_sups.len() as u8);
        for (code, sid) in &remaining_sups {
            block.push(*code);
            block.extend_from_slice(&sid.to_be_bytes());
        }
    }

    // Preferred path: point the Top DICT's encoding operand at an appended
    // block without moving anything. Fall back to rebuilding the Top DICT
    // with wide offsets when the operand is too narrow to reach EOF.
    let mut out = data.to_vec();
    let new_off = out.len();
    out.extend_from_slice(&block);
    let out = match patch_operand_in_place(&mut out, &enc_span, new_off as i32) {
        Some(()) => out,
        None => rebuild_top_dict_with_encoding(data, &block)?,
    };

    // Sanity: the result must re-parse and resolve every inlined code to the
    // gid the charset gave it, and every kept main-table code must survive.
    let check = cff_parser::Table::parse(&out)?;
    for (gid, code) in &gid_code {
        let got = check.encoding.code_to_gid(&check.charset, *code as u8)?;
        if got.0 as usize != *gid {
            return None;
        }
    }
    for (code, sid) in &remaining_sups {
        let want = cff.charset.sid_to_gid(cff_parser::StringId(*sid))?;
        let got = check.encoding.code_to_gid(&check.charset, *code)?;
        if got != want {
            return None;
        }
    }
    Some(out)
}

/// Locate the encoding operand (Top DICT op 16) inside the file.
fn top_dict_encoding_span(data: &[u8]) -> Option<(OperandSpan, usize)> {
    let hdr_size = *data.get(2)? as usize;
    let name_index = parse_index(data, hdr_size)?;

    let td_pos = name_index.end;
    let count = read_u16(data, td_pos)? as usize;
    if count != 1 {
        return None;
    }
    let off_size = read_u8(data, td_pos + 2)?;
    if !(1..=4).contains(&off_size) {
        return None;
    }
    let offsets_start = td_pos + 3;
    let data_start = offsets_start + 2 * off_size as usize - 1;
    let first = read_offset(data, offsets_start, off_size)?;
    let last = read_offset(data, offsets_start + off_size as usize, off_size)?;
    let dict_start = data_start.checked_add(first)?;
    let dict_end = data_start.checked_add(last)?;
    let dict = data.get(dict_start..dict_end)?;

    let entries = parse_dict(dict)?;
    let mut cursor = dict_start;
    for e in &entries {
        let span = OperandSpan {
            start: cursor,
            len: e.operands.len(),
        };
        cursor += e.operands.len() + e.operator.len();
        if e.operator.as_slice() == [16] {
            let vals = decode_operands(&e.operands);
            return Some((span, *vals.first()? as usize));
        }
    }
    None
}

/// Rebuild the file with a Top DICT whose operands are wide enough for the
/// appended encoding block. Mirrors `rebuild_with_wide_offsets`: every
/// absolute offset is re-encoded at 5 bytes and the tail shifts.
fn rebuild_top_dict_with_encoding(data: &[u8], new_encoding: &[u8]) -> Option<Vec<u8>> {
    let hdr_size = *data.get(2)? as usize;
    let name_index = parse_index(data, hdr_size)?;
    let top_index = parse_index(data, name_index.end)?;
    let tail_start = top_index.end;
    let entries = parse_dict(top_index.entries.first()?)?;

    let build = |delta: usize, enc_off: usize| -> Option<Vec<u8>> {
        let mut dict = Vec::new();
        for e in &entries {
            let vals = decode_operands(&e.operands);
            match e.operator.as_slice() {
                // charset ≤ 2 and Encoding ≤ 1 are predefined constants, not
                // offsets; Private's first operand is a size.
                [15] => {
                    let v = *vals.first()?;
                    let shifted = if v <= 2 { v } else { v + delta as i32 };
                    dict.extend_from_slice(&wide_int(shifted));
                }
                [16] => dict.extend_from_slice(&wide_int(enc_off as i32)),
                [17] => {
                    let v = *vals.first()?;
                    dict.extend_from_slice(&wide_int(v + delta as i32));
                }
                [18] => {
                    let size = *vals.first()?;
                    let off = *vals.get(1)?;
                    dict.extend_from_slice(&wide_int(size));
                    dict.extend_from_slice(&wide_int(off + delta as i32));
                }
                _ => dict.extend_from_slice(&e.operands),
            }
            dict.extend_from_slice(&e.operator);
        }
        Some(dict)
    };

    let probe = build(0, 0)?;
    let new_top_index = write_index(&[probe]);
    let delta = new_top_index
        .len()
        .checked_sub(top_index.end - name_index.end)?;

    let prefix_len = name_index.end;
    let tail_len = data.len() - tail_start;
    let enc_off = prefix_len + new_top_index.len() + tail_len;

    let dict = build(delta, enc_off)?;
    let top_index = write_index(&[dict]);
    if top_index.len() != new_top_index.len() {
        return None;
    }

    let mut out = Vec::with_capacity(data.len() + top_index.len() + new_encoding.len());
    out.extend_from_slice(&data[..prefix_len]);
    out.extend_from_slice(&top_index);
    out.extend_from_slice(&data[tail_start..]);
    out.extend_from_slice(new_encoding);
    Some(out)
}

/// Rebuild the file with a Top DICT whose offsets are wide enough to reach the
/// appended charset and CharStrings.
///
/// The in-place path is preferred because it moves nothing. When the original
/// operands are too narrow, every absolute offset in the Top DICT is re-encoded
/// as a 5-byte integer and everything after the Top DICT INDEX shifts by the
/// resulting size delta. Only the Top DICT holds absolute offsets: the Private
/// DICT's local-Subrs offset is relative to the Private DICT itself, and the
/// String and Global Subr INDEXes hold none, so shifting the tail wholesale is
/// safe.
fn rebuild_with_wide_offsets(
    data: &[u8],
    new_sids: &[u16],
    new_charstrings: &[Vec<u8>],
) -> Option<Vec<u8>> {
    let hdr_size = *data.get(2)? as usize;
    let name_index = parse_index(data, hdr_size)?;
    let top_index = parse_index(data, name_index.end)?;
    let tail_start = top_index.end;
    let entries = parse_dict(top_index.entries.first()?)?;

    // Operators whose operands are absolute file offsets. Encoding (16) and
    // charset (15) also accept small predefined constants, which must not be
    // shifted; charset ≤ 2 is already refused upstream, and Encoding 0/1 is
    // handled below.
    let build = |delta: usize, charset_off: usize, charstrings_off: usize| -> Option<Vec<u8>> {
        let mut dict = Vec::new();
        for e in &entries {
            let vals = decode_operands(&e.operands);
            match e.operator.as_slice() {
                [15] => dict.extend_from_slice(&wide_int(charset_off as i32)),
                [17] => dict.extend_from_slice(&wide_int(charstrings_off as i32)),
                [16] => {
                    let v = *vals.first()?;
                    // 0 = Standard, 1 = Expert: constants, not offsets.
                    let shifted = if v <= 1 { v } else { v + delta as i32 };
                    dict.extend_from_slice(&wide_int(shifted));
                }
                [18] => {
                    // Private: size then offset. Only the offset shifts.
                    let size = *vals.first()?;
                    let off = *vals.get(1)?;
                    dict.extend_from_slice(&wide_int(size));
                    dict.extend_from_slice(&wide_int(off + delta as i32));
                }
                _ => dict.extend_from_slice(&e.operands),
            }
            dict.extend_from_slice(&e.operator);
        }
        Some(dict)
    };

    // The delta depends on the rebuilt dict's length, which depends on the
    // delta. Every offset is written at a fixed 5 bytes, so one pass with
    // placeholder values gives the final length.
    let probe = build(0, 0, 0)?;
    let new_top_index = write_index(&[probe]);
    let delta = new_top_index
        .len()
        .checked_sub(top_index.end - name_index.end)?;

    // Layout: header + Name INDEX + new Top DICT INDEX + shifted tail +
    // new charset + new CharStrings.
    let prefix_len = name_index.end;
    let tail_len = data.len() - tail_start;
    let charset_off = prefix_len + new_top_index.len() + tail_len;
    let charset_bytes = write_charset(new_sids);
    let charstrings_off = charset_off + charset_bytes.len();

    let dict = build(delta, charset_off, charstrings_off)?;
    let top_index = write_index(&[dict]);
    // The two builds must agree in length or the offsets above are wrong.
    if top_index.len() != new_top_index.len() {
        return None;
    }

    let mut out = Vec::with_capacity(data.len() + top_index.len() + charset_bytes.len() + 256);
    out.extend_from_slice(&data[..prefix_len]);
    out.extend_from_slice(&top_index);
    out.extend_from_slice(&data[tail_start..]);
    out.extend_from_slice(&charset_bytes);
    out.extend_from_slice(&write_index(new_charstrings));
    Some(out)
}

/// A DICT integer in the always-5-byte form, so re-encoding never changes the
/// operand's width between passes.
fn wide_int(v: i32) -> [u8; 5] {
    let b = v.to_be_bytes();
    [29, b[0], b[1], b[2], b[3]]
}

/// Byte span of a DICT operand inside the file.
struct OperandSpan {
    start: usize,
    len: usize,
}

/// Locate the charset and CharStrings operands inside the Top DICT, returning
/// their file spans and current values.
fn top_dict_spans(data: &[u8]) -> Option<(OperandSpan, OperandSpan, usize, usize)> {
    let hdr_size = *data.get(2)? as usize;
    let name_index = parse_index(data, hdr_size)?;

    // Re-derive where the single Top DICT entry starts inside the file.
    let td_pos = name_index.end;
    let count = read_u16(data, td_pos)? as usize;
    if count != 1 {
        return None;
    }
    let off_size = read_u8(data, td_pos + 2)?;
    if !(1..=4).contains(&off_size) {
        return None;
    }
    let offsets_start = td_pos + 3;
    let data_start = offsets_start + 2 * off_size as usize - 1;
    let first = read_offset(data, offsets_start, off_size)?;
    let last = read_offset(data, offsets_start + off_size as usize, off_size)?;
    let dict_start = data_start.checked_add(first)?;
    let dict_end = data_start.checked_add(last)?;
    let dict = data.get(dict_start..dict_end)?;

    // CID-keyed fonts (ROS, 12 30) index the charset by CID, not SID.
    let entries = parse_dict(dict)?;
    if entries.iter().any(|e| e.operator == [12, 30]) {
        return None;
    }

    // Walk again tracking offsets so operand spans can be expressed in file
    // coordinates.
    let mut cursor = dict_start;
    let mut charset: Option<(OperandSpan, usize)> = None;
    let mut charstrings: Option<(OperandSpan, usize)> = None;
    for e in &entries {
        let span = OperandSpan {
            start: cursor,
            len: e.operands.len(),
        };
        cursor += e.operands.len() + e.operator.len();
        let vals = decode_operands(&e.operands);
        match e.operator.as_slice() {
            [15] => charset = Some((span, *vals.first()? as usize)),
            [17] => charstrings = Some((span, *vals.first()? as usize)),
            _ => {}
        }
    }
    let (cs_span, cs_off) = charset?;
    let (chs_span, chs_off) = charstrings?;
    if cs_off <= 2 || chs_off == 0 {
        return None; // predefined charset is out of scope
    }
    Some((cs_span, chs_span, cs_off, chs_off))
}

/// Read nominalWidthX / defaultWidthX from the Private DICT.
fn private_widths(data: &[u8]) -> Option<(i32, i32)> {
    let hdr_size = *data.get(2)? as usize;
    let name_index = parse_index(data, hdr_size)?;
    let top_index = parse_index(data, name_index.end)?;
    let entries = parse_dict(top_index.entries.first()?)?;
    for e in &entries {
        if e.operator.as_slice() == [18] {
            let vals = decode_operands(&e.operands);
            if vals.len() < 2 {
                return None;
            }
            let (size, off) = (vals[0] as usize, vals[1] as usize);
            let priv_bytes = data.get(off..off.checked_add(size)?)?;
            let mut nominal = 0i32;
            let mut default = 0i32;
            for pe in parse_dict(priv_bytes)? {
                let v = decode_operands(&pe.operands);
                match pe.operator.as_slice() {
                    [20] => default = *v.first().unwrap_or(&0),
                    [21] => nominal = *v.first().unwrap_or(&0),
                    _ => {}
                }
            }
            return Some((nominal, default));
        }
    }
    Some((0, 0))
}

/// Overwrite a DICT operand in place, keeping its encoded byte width so the
/// enclosing DICT does not change length. Fails when the value does not fit.
fn patch_operand_in_place(buf: &mut [u8], span: &OperandSpan, value: i32) -> Option<()> {
    let slot = buf.get_mut(span.start..span.start + span.len)?;
    let encoded: Vec<u8> = match span.len {
        5 => {
            let b = value.to_be_bytes();
            vec![29, b[0], b[1], b[2], b[3]]
        }
        3 => {
            if !(i16::MIN as i32..=i16::MAX as i32).contains(&value) {
                return None;
            }
            let b = (value as i16).to_be_bytes();
            vec![28, b[0], b[1]]
        }
        2 => {
            // Two-byte forms only reach 1131; a real offset rarely fits.
            if !(108..=1131).contains(&value) {
                return None;
            }
            let w = value - 108;
            vec![((w >> 8) + 247) as u8, (w & 0xff) as u8]
        }
        1 => {
            if !(-107..=107).contains(&value) {
                return None;
            }
            vec![(value + 139) as u8]
        }
        _ => return None,
    };
    slot.copy_from_slice(&encoded);
    Some(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Build a minimal but structurally real CFF: header, Name INDEX, Top DICT
    /// INDEX, String INDEX, Global Subr INDEX, charset, CharStrings, Private
    /// DICT.
    ///
    /// `charstring_pad` inflates each charstring. That is what decides whether
    /// the appended data lands beyond the reach of the existing offset
    /// operands: the charset sits near the start of the file and encodes in one
    /// byte, while a fat CharStrings INDEX pushes the append site past 1131,
    /// where DICT integers need three. That is exactly the shape of the real
    /// subsets this path exists for.
    pub(crate) fn synth_cff(glyph_names: &[u16], charstring_pad: usize) -> Vec<u8> {
        let mut charset_off = 0usize;
        let mut charstrings_off = 0usize;
        let mut private_off = 0usize;
        let mut out = Vec::new();

        // The offsets feed back into the operand widths, so iterate to a fixed
        // point rather than assuming one pass converges.
        for _ in 0..4 {
            out = vec![1, 0, 4, 2]; // major, minor, hdrSize, offSize
            out.extend_from_slice(&write_index(&[b"Test".to_vec()]));

            let mut dict = Vec::new();
            dict.extend_from_slice(&t2_int(charset_off as i32));
            dict.push(15);
            dict.extend_from_slice(&t2_int(charstrings_off as i32));
            dict.push(17);
            dict.extend_from_slice(&t2_int(4)); // Private DICT size
            dict.extend_from_slice(&t2_int(private_off as i32));
            dict.push(18);
            out.extend_from_slice(&write_index(&[dict]));

            out.extend_from_slice(&write_index(&[])); // String INDEX
            out.extend_from_slice(&write_index(&[])); // Global Subr INDEX

            charset_off = out.len();
            out.extend_from_slice(&write_charset(glyph_names));
            charstrings_off = out.len();
            // .notdef plus one charstring per named glyph, each ending in endchar.
            let entries: Vec<Vec<u8>> = (0..=glyph_names.len())
                .map(|_| {
                    let mut cs = vec![139u8; charstring_pad];
                    cs.push(14);
                    cs
                })
                .collect();
            out.extend_from_slice(&write_index(&entries));
            private_off = out.len();
            // nominalWidthX = 0 (op 21), defaultWidthX = 0 (op 20).
            out.extend_from_slice(&[139, 21, 139, 20]);
        }
        out
    }

    /// A CFF with a custom encoding whose supplement section maps `supp.0`
    /// to `supp.1` (a SID). `main_codes[i]` is the code for gid i+1.
    fn synth_cff_with_supplement(
        glyph_names: &[u16],
        main_codes: &[u8],
        supp: (u8, u16),
    ) -> Vec<u8> {
        let mut charset_off = 0usize;
        let mut charstrings_off = 0usize;
        let mut private_off = 0usize;
        let mut encoding_off = 0usize;
        let mut out = Vec::new();
        for _ in 0..4 {
            out = vec![1, 0, 4, 2];
            out.extend_from_slice(&write_index(&[b"Test".to_vec()]));
            let mut dict = Vec::new();
            dict.extend_from_slice(&t2_int(charset_off as i32));
            dict.push(15);
            dict.extend_from_slice(&t2_int(charstrings_off as i32));
            dict.push(17);
            dict.extend_from_slice(&t2_int(encoding_off as i32));
            dict.push(16);
            dict.extend_from_slice(&t2_int(4));
            dict.extend_from_slice(&t2_int(private_off as i32));
            dict.push(18);
            out.extend_from_slice(&write_index(&[dict]));
            out.extend_from_slice(&write_index(&[]));
            out.extend_from_slice(&write_index(&[]));
            charset_off = out.len();
            out.extend_from_slice(&write_charset(glyph_names));
            charstrings_off = out.len();
            let entries: Vec<Vec<u8>> = (0..=glyph_names.len()).map(|_| vec![139u8, 14]).collect();
            out.extend_from_slice(&write_index(&entries));
            private_off = out.len();
            out.extend_from_slice(&[139, 21, 139, 20]);
            encoding_off = out.len();
            // Format 0 with one supplement.
            out.push(0x80);
            out.push(main_codes.len() as u8);
            out.extend_from_slice(main_codes);
            out.push(1); // nSups
            out.push(supp.0);
            out.extend_from_slice(&supp.1.to_be_bytes());
        }
        out
    }

    #[test]
    fn inlines_a_supplement_for_an_uncoded_glyph() {
        // gid 3 (SID 7) has no main-table code; supplement 161 -> SID 7.
        let font = synth_cff_with_supplement(&[5, 6, 7], &[65, 66], (161, 7));
        let out = inline_encoding_supplements(&font, &Default::default())
            .expect("pure addition must inline");
        let cff = cff_parser::Table::parse(&out).unwrap();
        let gid = cff.encoding.code_to_gid(&cff.charset, 161).unwrap();
        assert_eq!(gid.0, 3);
    }

    #[test]
    fn inlines_a_duplicate_only_when_the_supplement_code_is_used() {
        // gid 2 (SID 6) has main code 66 and supplement code 161.
        let font = synth_cff_with_supplement(&[5, 6], &[65, 66], (161, 6));

        // Document uses 161, not 66: inline (replaces the main code).
        let preferred: std::collections::HashSet<u8> = [161].into_iter().collect();
        let out = inline_encoding_supplements(&font, &preferred).expect("inline");
        let cff = cff_parser::Table::parse(&out).unwrap();
        assert_eq!(cff.encoding.code_to_gid(&cff.charset, 161).unwrap().0, 2);

        // Document uses 66 (or we cannot tell): leave the font untouched.
        let preferred: std::collections::HashSet<u8> = [66].into_iter().collect();
        assert!(inline_encoding_supplements(&font, &preferred).is_none());
        let preferred: std::collections::HashSet<u8> = Default::default();
        assert!(inline_encoding_supplements(&font, &preferred).is_none());
    }

    #[test]
    fn appends_space_in_place_when_the_operands_have_room() {
        let font = synth_cff(&[5, 6, 7], 0);
        let out = try_append_blank_glyph(&font, 250.0).expect("should append");

        // In-place means nothing moved: only the two offset operands inside the
        // Top DICT changed, and everything after it is byte-identical.
        let name_index = parse_index(&font, font[2] as usize).unwrap();
        let tail = parse_index(&font, name_index.end).unwrap().end;
        assert_eq!(
            &out[tail..font.len()],
            &font[tail..],
            "in-place path must not disturb anything after the Top DICT"
        );
        assert!(out.len() > font.len(), "the new glyph has to go somewhere");

        let (_, _, charset_off, charstrings_off) =
            top_dict_spans(&out).expect("patched font must re-parse");
        assert_eq!(parse_index(&out, charstrings_off).unwrap().entries.len(), 5);
        assert_eq!(
            parse_charset(&out, charset_off, 5).unwrap(),
            vec![5, 6, 7, SID_SPACE]
        );
    }

    /// The case that blocked 60 fonts on the govdocs sample: the offset
    /// operands are encoded too narrowly to reach the end of the file, so the
    /// Top DICT has to be rebuilt with wide operands and the tail shifted.
    #[test]
    fn rebuilds_top_dict_when_offsets_are_too_narrow() {
        let font = synth_cff(&[5, 6, 7], 800);

        // Confirm the premise before testing the remedy.
        let (cs_span, _, _, _) = top_dict_spans(&font).unwrap();
        let mut probe = font.clone();
        assert!(
            patch_operand_in_place(&mut probe, &cs_span, font.len() as i32).is_none(),
            "fixture must have operands too narrow for the in-place path"
        );

        let out = try_append_blank_glyph(&font, 250.0).expect("rebuild path should append");
        let (_, _, charset_off, charstrings_off) = top_dict_spans(&out).unwrap();
        let charstrings = parse_index(&out, charstrings_off).unwrap();
        assert_eq!(charstrings.entries.len(), 5);
        assert_eq!(
            parse_charset(&out, charset_off, 5).unwrap(),
            vec![5, 6, 7, SID_SPACE]
        );

        // Every original charstring must survive byte-identical.
        let original = parse_index(&font, top_dict_spans(&font).unwrap().3).unwrap();
        assert_eq!(&charstrings.entries[..4], &original.entries[..]);

        // The Private DICT offset must have shifted with the tail, or the
        // font's width defaults would be read from the wrong bytes.
        assert_eq!(
            private_widths(&out),
            private_widths(&font),
            "Private DICT must still resolve after the shift"
        );
    }

    #[test]
    fn refuses_a_font_that_already_has_space() {
        let font = synth_cff(&[SID_SPACE, 6], 0);
        assert_eq!(
            try_append_blank_glyph(&font, 250.0),
            Err(AppendRefused::AlreadyPresent)
        );
    }

    #[test]
    fn t2_int_round_trips_known_encodings() {
        assert_eq!(t2_int(0), vec![139]);
        assert_eq!(t2_int(107), vec![246]);
        assert_eq!(t2_int(278), vec![247, 170]);
        assert_eq!(t2_int(-108), vec![251, 0]);
        assert_eq!(t2_int(5000), vec![28, 0x13, 0x88]);
    }

    #[test]
    fn write_index_round_trips() {
        let entries = vec![vec![1u8, 2, 3], vec![4u8], vec![]];
        let bytes = write_index(&entries);
        let parsed = parse_index(&bytes, 0).expect("index must re-parse");
        assert_eq!(parsed.entries, entries);
        assert_eq!(parsed.end, bytes.len());
    }

    #[test]
    fn empty_index_round_trips() {
        let bytes = write_index(&[]);
        let parsed = parse_index(&bytes, 0).expect("empty index must re-parse");
        assert!(parsed.entries.is_empty());
    }

    #[test]
    fn charset_round_trips() {
        let sids = vec![5u16, 9, 200];
        let bytes = write_charset(&sids);
        let parsed = parse_charset(&bytes, 0, sids.len() + 1);
        // offset 0 is treated as predefined, so parse directly for the check
        assert!(parsed.is_none());
        assert_eq!(bytes[0], 0);
        assert_eq!(bytes.len(), 1 + sids.len() * 2);
    }

    #[test]
    fn garbage_input_is_refused_not_panicked() {
        assert!(append_blank_glyph(&[], 278.0).is_none());
        assert!(append_blank_glyph(&[1, 0, 4, 2, 0, 0], 278.0).is_none());
        let noise: Vec<u8> = (0..512).map(|i| (i * 37 % 251) as u8).collect();
        assert!(append_blank_glyph(&noise, 278.0).is_none());
    }
}
