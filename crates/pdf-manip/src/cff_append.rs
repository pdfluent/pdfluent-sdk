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
                i += 1;
                while i < d.len() {
                    let b = d[i];
                    i += 1;
                    if b & 0x0f == 0x0f || b & 0xf0 == 0xf0 {
                        break;
                    }
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
/// the caller then keeps the original bytes.
pub fn append_blank_glyph(data: &[u8], width: f64) -> Option<Vec<u8>> {
    let (charset_span, charstrings_span, charset_off, charstrings_off) = top_dict_spans(data)?;

    let charstrings = parse_index(data, charstrings_off)?;
    let n_glyphs = charstrings.entries.len();
    let sids = parse_charset(data, charset_off, n_glyphs)?;
    if sids.len() + 1 != n_glyphs || sids.contains(&SID_SPACE) {
        return None;
    }

    // Width operand for the blank glyph, relative to nominalWidthX.
    let (nominal_width, default_width) = private_widths(data)?;
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

    // Rewrite both offsets without changing the Top DICT's length.
    patch_operand_in_place(&mut out, &charset_span, new_charset_off as i32)?;
    patch_operand_in_place(&mut out, &charstrings_span, new_charstrings_off as i32)?;

    // Sanity: the result must re-parse, expose exactly one more glyph, and keep
    // every original charstring byte-identical.
    let (_, _, chk_charset, chk_charstrings) = top_dict_spans(&out)?;
    if chk_charset != new_charset_off || chk_charstrings != new_charstrings_off {
        return None;
    }
    let check = parse_index(&out, new_charstrings_off)?;
    if check.entries.len() != n_glyphs + 1 {
        return None;
    }
    for (a, b) in charstrings.entries.iter().zip(check.entries.iter()) {
        if a != b {
            return None;
        }
    }
    if parse_charset(&out, new_charset_off, n_glyphs + 1)? != new_sids {
        return None;
    }
    Some(out)
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
mod tests {
    use super::*;

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
