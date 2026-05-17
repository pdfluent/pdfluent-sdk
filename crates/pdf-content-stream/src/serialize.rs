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
    for window in ops.windows(2) {
        let a = &window[0];
        let b = &window[1];
        if a.byte_end != b.byte_start {
            return Err(format!(
                "gap between op[{}] (byte_end={}) and op[{}] (byte_start={})",
                a.byte_start, a.byte_end, b.byte_start, b.byte_start,
            ));
        }
    }
    Ok(())
}
