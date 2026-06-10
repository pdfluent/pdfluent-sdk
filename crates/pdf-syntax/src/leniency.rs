//! Thread-local decode-leniency event collector.
//!
//! Inner parser and filter code calls [`emit`] to record a recovery or
//! leniency event without changing any behaviour. A caller that wants to
//! observe events wraps its work with [`activate`] / [`drain`]:
//!
//! ```rust,ignore
//! pdf_syntax::leniency::activate();
//! let pdf = Pdf::new(bytes)?;                 // events accumulate here
//! let events = pdf_syntax::leniency::drain(); // returns them and resets
//! ```
//!
//! When no collector is active every [`emit`] call is a no-op — zero heap
//! allocation on the hot path. Per-code deduplication prevents a single
//! corrupt stream from producing thousands of identical events.

/// Severity of a [`LeniencyEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeniencySeverity {
    /// A recovery succeeded; output may differ slightly from the source.
    Info,
    /// A degradation or non-standard recovery occurred; output may be partial.
    Warning,
    /// A hard limit or severe corruption required truncating the output.
    Critical,
}

/// A single decode-leniency or structural-recovery event.
///
/// All fields are `'static` — no heap allocation on the emit path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeniencyEvent {
    /// Short, stable, machine-readable identifier (e.g. `"FLATE_BROKEN_FALLBACK"`).
    pub code: &'static str,
    /// Severity classification.
    pub severity: LeniencySeverity,
    /// Human-readable description of the event.
    pub message: &'static str,
}

impl LeniencyEvent {
    const fn new(code: &'static str, severity: LeniencySeverity, message: &'static str) -> Self {
        Self {
            code,
            severity,
            message,
        }
    }
}

// ---------------------------------------------------------------------------
// Known event constants — constructed once, zero cost to reference.
// ---------------------------------------------------------------------------

/// Flate stream broken; decoded with a pure-Rust fallback.
pub const FLATE_BROKEN_FALLBACK: LeniencyEvent = LeniencyEvent::new(
    "FLATE_BROKEN_FALLBACK",
    LeniencySeverity::Warning,
    "A flate stream was broken; the pure-Rust fallback decoder was used. \
     Output may be truncated or differ from the original.",
);

/// A bad block header was encountered inside a flate stream.
pub const FLATE_BAD_BLOCK: LeniencyEvent = LeniencyEvent::new(
    "FLATE_BAD_BLOCK",
    LeniencySeverity::Warning,
    "A bad block header was encountered in a flate stream; the block was skipped.",
);

/// Premature EOF in an LZW stream; the EOD code was absent.
pub const LZW_PREMATURE_EOF: LeniencyEvent = LeniencyEvent::new(
    "LZW_PREMATURE_EOF",
    LeniencySeverity::Warning,
    "Premature EOF in an LZW stream; the end-of-data code was absent.",
);

/// An invalid code was encountered in an LZW stream.
pub const LZW_INVALID_CODE: LeniencyEvent = LeniencyEvent::new(
    "LZW_INVALID_CODE",
    LeniencySeverity::Warning,
    "An invalid LZW code was encountered; the stream may be truncated.",
);

/// ASCII-85: a 1-character terminal group was accepted leniently.
///
/// A 1-character group is technically malformed (spec requires ≥ 2 chars) but
/// is common in scanned PDFs. The group is treated as producing zero bytes.
pub const ASCII85_LENIENT_PARTIAL: LeniencyEvent = LeniencyEvent::new(
    "ASCII85_LENIENT_PARTIAL",
    LeniencySeverity::Info,
    "An ASCII-85 stream contained a 1-character terminal group; accepted leniently \
     (common in scanned PDFs). No bytes produced for that group.",
);

/// CCITT: partial row decode — at least one row decoded before an error.
pub const CCITT_PARTIAL_DECODE: LeniencyEvent = LeniencyEvent::new(
    "CCITT_PARTIAL_DECODE",
    LeniencySeverity::Warning,
    "The CCITT filter encountered an error after decoding at least one row; \
     partial output returned.",
);

/// Stream parse failed; a manual fallback parser was used.
pub const STREAM_PARSE_FALLBACK: LeniencyEvent = LeniencyEvent::new(
    "STREAM_PARSE_FALLBACK",
    LeniencySeverity::Warning,
    "Normal stream parsing failed; a manual fallback parser was used instead.",
);

/// A cycle was detected in indirect object references.
pub const INDIRECT_CYCLE: LeniencyEvent = LeniencyEvent::new(
    "INDIRECT_CYCLE",
    LeniencySeverity::Warning,
    "A cycle was detected in indirect object references; resolution was stopped.",
);

/// Indirect object resolution depth exceeded the 512-level limit.
pub const INDIRECT_DEPTH_EXCEEDED: LeniencyEvent = LeniencyEvent::new(
    "INDIRECT_DEPTH_EXCEEDED",
    LeniencySeverity::Warning,
    "Indirect object resolution depth exceeded 512; resolution was stopped.",
);

// ---------------------------------------------------------------------------
// Thread-local collector — std only.
// ---------------------------------------------------------------------------

#[cfg(feature = "std")]
use std::cell::RefCell;

#[cfg(feature = "std")]
thread_local! {
    static COLLECTOR: RefCell<Option<Vec<LeniencyEvent>>> = const { RefCell::new(None) };
}

/// Activate the thread-local collector on the current thread.
///
/// Any subsequent [`emit`] calls accumulate into the buffer until [`drain`]
/// is called. Calling `activate` while a collector is already active discards
/// the existing events and starts fresh.
#[cfg(feature = "std")]
pub fn activate() {
    COLLECTOR.with(|c| *c.borrow_mut() = Some(Vec::new()));
}

/// Drain all accumulated events and deactivate the collector.
///
/// Returns the events collected since the last [`activate`], then resets the
/// buffer. Calling `drain` without a prior `activate` returns an empty `Vec`.
#[cfg(feature = "std")]
pub fn drain() -> alloc::vec::Vec<LeniencyEvent> {
    COLLECTOR.with(|c| c.borrow_mut().take().unwrap_or_default())
}

/// Emit a leniency event to the active thread-local collector.
///
/// No-op if no collector is active on this thread. Per-code deduplication
/// ensures the same event code appears at most once per `activate`/`drain`
/// pair, regardless of how many times the recovery path is triggered.
#[cfg(feature = "std")]
pub(crate) fn emit(event: LeniencyEvent) {
    COLLECTOR.with(|c| {
        if let Some(vec) = c.borrow_mut().as_mut() {
            // Linear scan — ≤ ~15 distinct codes per document in practice.
            if !vec.iter().any(|e| e.code == event.code) {
                vec.push(event);
            }
        }
    });
}

/// No-op stub for `no_std` builds where thread-locals are unavailable.
#[cfg(not(feature = "std"))]
pub(crate) fn emit(_event: LeniencyEvent) {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activate_emit_drain_roundtrip() {
        activate();
        emit(FLATE_BROKEN_FALLBACK);
        let events = drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].code, "FLATE_BROKEN_FALLBACK");
    }

    #[test]
    fn per_code_deduplication() {
        activate();
        emit(LZW_PREMATURE_EOF);
        emit(LZW_PREMATURE_EOF);
        emit(LZW_PREMATURE_EOF);
        let events = drain();
        assert_eq!(events.len(), 1, "same code must appear at most once");
    }

    #[test]
    fn multiple_distinct_codes_all_collected() {
        activate();
        emit(FLATE_BROKEN_FALLBACK);
        emit(FLATE_BAD_BLOCK);
        emit(LZW_PREMATURE_EOF);
        let events = drain();
        assert_eq!(events.len(), 3);
        let codes: Vec<&str> = events.iter().map(|e| e.code).collect();
        assert!(codes.contains(&"FLATE_BROKEN_FALLBACK"));
        assert!(codes.contains(&"FLATE_BAD_BLOCK"));
        assert!(codes.contains(&"LZW_PREMATURE_EOF"));
    }

    #[test]
    fn emit_without_activate_is_noop() {
        // Ensure no collector is active by draining first.
        let _ = drain();
        // Now emit — should be a no-op.
        emit(INDIRECT_CYCLE);
        // Drain again — must return empty (collector was never activated).
        let events = drain();
        assert!(events.is_empty());
    }

    #[test]
    fn drain_without_activate_returns_empty() {
        let _ = drain(); // clear any leftover state
        let events = drain();
        assert!(events.is_empty());
    }

    #[test]
    fn activate_resets_prior_events() {
        activate();
        emit(FLATE_BROKEN_FALLBACK);
        // Second activate discards the first batch.
        activate();
        emit(INDIRECT_CYCLE);
        let events = drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].code, "INDIRECT_CYCLE");
    }

    #[test]
    fn all_known_constants_have_unique_codes() {
        let all = [
            FLATE_BROKEN_FALLBACK.code,
            FLATE_BAD_BLOCK.code,
            LZW_PREMATURE_EOF.code,
            LZW_INVALID_CODE.code,
            ASCII85_LENIENT_PARTIAL.code,
            CCITT_PARTIAL_DECODE.code,
            STREAM_PARSE_FALLBACK.code,
            INDIRECT_CYCLE.code,
            INDIRECT_DEPTH_EXCEEDED.code,
        ];
        let unique: std::collections::HashSet<&str> = all.iter().copied().collect();
        assert_eq!(unique.len(), all.len(), "duplicate event code found");
    }
}
