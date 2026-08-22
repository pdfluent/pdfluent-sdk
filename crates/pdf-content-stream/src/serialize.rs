//! Round-trip serializer for parsed content streams.
//!
//! For any content stream accepted by [`ContentStreamParser`], parsing followed
//! by `serialize` produces byte-identical output.  This is guaranteed because
//! each [`ParsedOp`] stores `(byte_start, byte_end)` covering all bytes — including
//! leading whitespace — from right after the previous operator to right after
//! this operator's keyword.  Adjacent ops are contiguous in the input.

use crate::parser::ParsedOp;

/// Reconstruct a byte-identical copy of the content stream from the original
/// `input` buffer and the parsed ops returned by [`ContentStreamParser`].
///
/// The function concatenates each op's raw byte slice (`input[byte_start..byte_end]`)
/// in order, then appends any trailing bytes not covered by the last op.
///
/// # Panics
/// Panics in debug builds if the byte ranges are not contiguous or out of
/// bounds for `input`.  In release builds this is undefined behaviour.
pub fn serialize(input: &[u8], ops: &[ParsedOp]) -> Vec<u8> {
    if ops.is_empty() {
        return input.to_vec();
    }

    let mut out = Vec::with_capacity(input.len());

    for op in ops {
        debug_assert!(op.byte_start <= op.byte_end, "byte_start > byte_end");
        debug_assert!(op.byte_end <= input.len(), "byte_end out of bounds");
        out.extend_from_slice(&input[op.byte_start..op.byte_end]);
    }

    // Append any bytes after the last op's byte_end (e.g. trailing whitespace).
    let last_end = ops.last().map(|op| op.byte_end).unwrap_or(0);
    if last_end < input.len() {
        out.extend_from_slice(&input[last_end..]);
    }

    out
}

/// Verify that `serialize(input, ops) == input`.
///
/// Returns `true` if the round-trip is byte-identical.
pub fn verify_round_trip(input: &[u8], ops: &[ParsedOp]) -> bool {
    serialize(input, ops) == input
}

/// Verify that the `ParsedOp` byte ranges are contiguous and cover the full
/// non-whitespace content of `input`.
///
/// Returns `Ok(())` if the invariant holds.
pub fn verify_contiguity(ops: &[ParsedOp]) -> Result<(), String> {
    // De index kwam uit `byte_start` in plaats van uit de positie in de lijst,
    // en `b.byte_start` stond er twee keer. De melding zei dus "op[1234]" met
    // een byte-offset waar een opnummer hoort -- misleidend op precies het
    // moment dat iemand hem nodig heeft.
    for (i, window) in ops.windows(2).enumerate() {
        let a = &window[0];
        let b = &window[1];
        if a.byte_end != b.byte_start {
            return Err(format!(
                "gap between op[{}] (byte_end={}) and op[{}] (byte_start={})",
                i,
                a.byte_end,
                i + 1,
                b.byte_start,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod contiguity_tests {
    use super::verify_contiguity;
    use crate::ops::ContentOp;
    use crate::parser::ParsedOp;

    fn op(start: usize, end: usize) -> ParsedOp {
        ParsedOp {
            byte_start: start,
            byte_end: end,
            op: ContentOp::BeginText,
        }
    }

    /// De geparste instructies moeten de invoer zonder gaten betegelen. Zit er
    /// een gat, dan vallen die bytes bij het terugschrijven weg — en dat is
    /// stille inhoudsverlies, niet een foutmelding.
    #[test]
    fn a_contiguous_run_is_accepted() {
        assert!(verify_contiguity(&[op(0, 3), op(3, 7), op(7, 12)]).is_ok());
    }

    #[test]
    fn a_gap_is_refused() {
        let r = verify_contiguity(&[op(0, 3), op(5, 9)]);
        assert!(r.is_err(), "een gat van byte 3 tot 5 hoort te weigeren");
    }

    /// De melding moet de plek in de lijst noemen, niet een byte-offset. Dit is
    /// de assertie die de oude tekst rood maakt.
    #[test]
    fn the_message_names_the_op_index_not_a_byte_offset() {
        let fout = verify_contiguity(&[op(100, 103), op(105, 109)]).unwrap_err();
        assert!(fout.contains("op[0]"), "verwacht de index 0, kreeg: {fout}");
        assert!(fout.contains("op[1]"), "verwacht de index 1, kreeg: {fout}");
        assert!(fout.contains("byte_end=103"), "kreeg: {fout}");
        assert!(fout.contains("byte_start=105"), "kreeg: {fout}");
    }

    #[test]
    fn overlap_is_a_gap_too() {
        // byte_end voorbij de volgende byte_start is net zo goed niet aaneengesloten.
        assert!(verify_contiguity(&[op(0, 8), op(5, 9)]).is_err());
    }

    #[test]
    fn zero_or_one_op_is_trivially_contiguous() {
        assert!(verify_contiguity(&[]).is_ok());
        assert!(verify_contiguity(&[op(4, 9)]).is_ok());
    }
}
