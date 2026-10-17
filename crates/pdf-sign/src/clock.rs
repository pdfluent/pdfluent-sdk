// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Wall-clock time for signing, on every target we ship to.
//!
//! `std::time::SystemTime::now()` compiles for `wasm32-unknown-unknown` and
//! then panics when it is reached; under `panic = "abort"` JavaScript sees a
//! bare `RuntimeError: unreachable`. Every signature stamps its signing time,
//! so before this module existed no signature could be made in the browser at
//! all (#617). Same pattern as `pdf_manip::clock`, which this crate cannot use:
//! pdf-manip depends on pdf-sign, not the other way round.

/// Seconds since the Unix epoch, or 0 if the platform clock is unavailable.
pub(crate) fn unix_now_secs() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let millis = js_sys::Date::now();
        if millis.is_finite() && millis > 0.0 {
            (millis / 1000.0) as u64
        } else {
            0
        }
    }
}

/// A PDF date string (ISO 32000-1 §7.9.4) in UTC: `D:YYYYMMDDHHmmSSZ`.
pub(crate) fn pdf_date(secs: u64) -> String {
    let (year, month, day, hour, min, sec) = crate::cms_builder::epoch_to_utc(secs);
    format!("D:{year:04}{month:02}{day:02}{hour:02}{min:02}{sec:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_a_plausible_present_day_timestamp() {
        // 2020-01-01: a zero/epoch value must not slip through.
        assert!(unix_now_secs() > 1_577_836_800);
    }

    #[test]
    fn pdf_date_is_iso_32000_utc() {
        // 2024-02-29 13:05:09 UTC.
        assert_eq!(pdf_date(1_709_211_909), "D:20240229130509Z");
    }
}
