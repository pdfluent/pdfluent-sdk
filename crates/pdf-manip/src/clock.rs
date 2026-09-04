// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Wall-clock time that works on every target we ship to.
//!
//! # Why this exists
//!
//! `std::time::SystemTime::now()` is not implemented on
//! `wasm32-unknown-unknown`. It compiles — against the `unsupported` stub in
//! std — and then **panics at runtime** with "time not implemented on this
//! platform". Under the release profile's `panic = "abort"` that surfaces to
//! JavaScript as a bare `RuntimeError: unreachable`, with no message and no
//! location.
//!
//! That is not hypothetical: it took down `PdfDoc.convertToPdfa` for every
//! input and every conformance level in the browser build, because PDF/A
//! conversion always embeds fonts and the embedding path timestamps its
//! licence check. Native builds were unaffected throughout, so the whole
//! native test suite stayed green while the WASM binding was completely
//! broken.
//!
//! Anything in this workspace that needs the current time must go through
//! here rather than calling `SystemTime` directly.

/// Seconds since the Unix epoch.
///
/// Returns 0 rather than panicking if the platform clock is unavailable or
/// reports a time before the epoch. Callers use this for licence validity
/// windows and document timestamps, where a wrong-but-present value degrades
/// gracefully and a panic does not.
#[must_use]
pub fn unix_now_secs() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        // The host's clock, via the JS runtime that is by definition present
        // wherever this build runs. Milliseconds since the epoch as f64.
        let millis = js_sys::Date::now();
        if millis.is_finite() && millis > 0.0 {
            (millis / 1000.0) as u64
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_a_plausible_present_day_timestamp() {
        let now = unix_now_secs();
        // 2020-01-01 — guards against a zero/epoch value slipping through
        // without pinning the test to a date that will expire.
        assert!(
            now > 1_577_836_800,
            "expected a present-day timestamp, got {now}"
        );
    }
}
