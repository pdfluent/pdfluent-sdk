//! Making an appended save into a real incremental update.
//!
//! `Document::new_from_prev` plus `save_to` produces a *standalone* PDF: its
//! own header, its own cross-reference table, and byte offsets counted from
//! its own first byte. Appending that to the original file leaves every offset
//! in the appended part short by the length of the original, and `startxref`
//! then points into the middle of the first revision. A reader that follows it
//! lands on rubbish.
//!
//! Two callers appended such a save without correcting anything:
//! `sign_pdf_incremental` and `embed_dss_incremental`. Both produced files that
//! a strict reader rejects outright ("invalid file trailer"). It stayed hidden
//! because the tests around them read the result with a scanning parser, which
//! rebuilds the table from the object bodies and never consults `startxref`
//! (#362).
//!
//! [`rebase`] shifts the offsets so the appended revision describes its own
//! position in the combined file.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// Shift every offset in `increment` by `base_len`.
///
/// `increment` is the output of a standalone save that is about to be appended
/// after `base_len` bytes. Both the in-use entries of its cross-reference table
/// and its `startxref` are rewritten to absolute positions in the combined
/// file. Free entries keep their offset, which is a link in the free list and
/// not a position.
///
/// The header the save writes (`%PDF-…`) is left where it is. It is a comment
/// and harmless mid-file, and removing it would move every object, which is the
/// problem this function exists to avoid.
///
/// Returns `Err` when the increment uses a cross-reference *stream* rather than
/// a table: the offsets then sit in compressed binary and cannot be rewritten
/// this way. Refusing is deliberate. Returning the increment untouched would
/// hand back exactly the broken file this function was called to prevent.
pub(crate) fn rebase(increment: &[u8], base_len: usize) -> Result<Vec<u8>, String> {
    let mut out = increment.to_vec();

    let xref_at = last_index_of(&out, b"\nxref\n")
        .map(|i| i + b"\nxref\n".len())
        .ok_or_else(|| {
            "increment has no cross-reference table (an xref stream cannot be rebased by \
             rewriting text)"
                .to_string()
        })?;
    let trailer_at =
        last_index_of(&out, b"trailer").ok_or_else(|| "increment has no trailer".to_string())?;
    if trailer_at < xref_at {
        return Err("increment trailer precedes its cross-reference table".into());
    }

    rebase_table(&mut out[xref_at..trailer_at], base_len)?;

    let startxref_at = last_index_of(&out, b"startxref")
        .ok_or_else(|| "increment has no startxref".to_string())?;
    let digits_at = startxref_at + b"startxref".len();
    let value_start = digits_at + count_leading_whitespace(&out[digits_at..]);
    let value_end = value_start + count_leading_digits(&out[value_start..]);
    if value_end == value_start {
        return Err("increment has a startxref without a value".into());
    }
    let value: usize = std::str::from_utf8(&out[value_start..value_end])
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "increment has an unreadable startxref value".to_string())?;

    // Only the tail follows, so a longer number here shifts nothing that any
    // offset points at.
    let mut rebased = out[..value_start].to_vec();
    rebased.extend_from_slice((value + base_len).to_string().as_bytes());
    rebased.extend_from_slice(&out[value_end..]);
    Ok(rebased)
}

/// Add `base_len` to every in-use entry of a classic cross-reference table.
///
/// Entries are exactly twenty bytes (`nnnnnnnnnn ggggg n` plus a two-byte end
/// of line), so a rewritten offset is the same width as the one it replaces and
/// the table does not move. An offset that no longer fits in ten digits is an
/// error rather than a truncation: a file over ten gigabytes would silently get
/// a wrong table.
fn rebase_table(table: &mut [u8], base_len: usize) -> Result<(), String> {
    let mut i = 0usize;
    while i < table.len() {
        i += count_leading_whitespace(&table[i..]);
        if i >= table.len() {
            break;
        }
        // A subsection header: "<first object> <count>".
        let first_len = count_leading_digits(&table[i..]);
        if first_len == 0 {
            return Err(format!(
                "unexpected byte {:?} in the cross-reference table",
                table[i] as char
            ));
        }
        i += first_len;
        i += count_leading_whitespace(&table[i..]);
        let count_len = count_leading_digits(&table[i..]);
        if count_len == 0 {
            return Err("cross-reference subsection without a count".into());
        }
        let count: usize = std::str::from_utf8(&table[i..i + count_len])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| "unreadable cross-reference subsection count".to_string())?;
        i += count_len;
        i += count_leading_whitespace(&table[i..]);

        for _ in 0..count {
            if i + 18 > table.len() {
                return Err("cross-reference table ends inside an entry".into());
            }
            let kind = table[i + 17];
            if kind == b'n' {
                let offset: usize = std::str::from_utf8(&table[i..i + 10])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| "unreadable cross-reference offset".to_string())?;
                let shifted = offset + base_len;
                let text = format!("{shifted:010}");
                if text.len() != 10 {
                    return Err(format!(
                        "offset {shifted} does not fit the ten digits a cross-reference entry has"
                    ));
                }
                table[i..i + 10].copy_from_slice(text.as_bytes());
            }
            // Entry bodies are eighteen bytes; the two-byte terminator is
            // whitespace and skipped by the loop below.
            i += 18;
            i += count_leading_whitespace(&table[i..]);
        }
    }
    Ok(())
}

fn last_index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .enumerate()
        .rev()
        .find_map(|(i, w)| (w == needle).then_some(i))
}

fn count_leading_whitespace(data: &[u8]) -> usize {
    data.iter().take_while(|b| b.is_ascii_whitespace()).count()
}

fn count_leading_digits(data: &[u8]) -> usize {
    data.iter().take_while(|b| b.is_ascii_digit()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table shaped exactly as lopdf writes one, offsets counted from zero.
    fn increment() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"%PDF-1.4\n5 0 obj\n<<>>\nendobj\nxref\n");
        v.extend_from_slice(b"0 1\n");
        v.extend_from_slice(b"0000000000 65535 f\r\n");
        v.extend_from_slice(b"5 2\n");
        v.extend_from_slice(b"0000000015 00000 n\r\n");
        v.extend_from_slice(b"0000000989 00000 n\r\n");
        v.extend_from_slice(b"trailer\n<</Size 11/Prev 17174>>\nstartxref\n1023\n%%EOF");
        v
    }

    #[test]
    fn in_use_offsets_move_and_the_free_entry_does_not() {
        let out = rebase(&increment(), 17426).unwrap();
        let text = String::from_utf8_lossy(&out).into_owned();

        assert!(
            text.contains("0000017441 00000 n"),
            "15 + 17426 = 17441, and an entry that keeps its old offset points \
             into the first revision: {text}"
        );
        assert!(text.contains("0000018415 00000 n"), "989 + 17426 = 18415");
        assert!(
            text.contains("0000000000 65535 f"),
            "a free entry carries a link in the free list, not a position",
        );
    }

    #[test]
    fn startxref_points_at_the_table_in_the_combined_file() {
        let out = rebase(&increment(), 17426).unwrap();
        let text = String::from_utf8_lossy(&out).into_owned();
        assert!(
            text.contains("startxref\n18449"),
            "1023 + 17426 = 18449; leaving it at 1023 is what made a reader \
             land in the middle of the first revision: {text}"
        );
    }

    #[test]
    fn the_table_keeps_its_length_so_nothing_it_describes_moves() {
        let before = increment();
        let out = rebase(&before, 17426).unwrap();
        let grew = out.len() - before.len();
        assert_eq!(
            grew,
            "18449".len() - "1023".len(),
            "only the startxref value may change width, and only because \
             nothing but the end of file follows it",
        );
    }

    #[test]
    fn an_xref_stream_is_refused_rather_than_returned_unchanged() {
        let stream_increment = b"%PDF-2.0\n5 0 obj\n<</Type/XRef>>\nstream\n\x00\x01\
            \nendstream\nendobj\nstartxref\n15\n%%EOF";
        let err = rebase(stream_increment, 100).unwrap_err();
        assert!(
            err.contains("xref stream"),
            "the caller has to learn that the offsets could not be corrected, \
             got: {err}",
        );
    }
}
