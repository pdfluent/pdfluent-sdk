//! Mutool oracle-fault detection.
//!
//! Classifies whether a PNG file produced by `mutool draw` is a genuine
//! render result or a suspected oracle fault. Oracle faults are excluded from
//! the effective pass-rate denominator in the differential harness — they must
//! never be counted as rendering failures caused by our engine.
//!
//! Conservative by design: only confirmed faults are flagged. An uncertain
//! case defaults to "not a fault" to avoid masking real regressions.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::Path;

/// Result of checking whether a mutool-produced PNG is a valid oracle render.
#[derive(Debug, Clone)]
pub struct OracleFaultCheck {
    /// True only when the oracle output is clearly invalid.
    pub is_fault: bool,
    /// Human-readable explanation when `is_fault` is true.
    pub reason: Option<String>,
}

impl OracleFaultCheck {
    fn ok() -> Self {
        Self {
            is_fault: false,
            reason: None,
        }
    }

    fn fault(reason: impl Into<String>) -> Self {
        Self {
            is_fault: true,
            reason: Some(reason.into()),
        }
    }
}

/// Check whether a PNG file produced by `mutool draw` is a valid oracle render.
///
/// Fault conditions (confirmed, not suspected):
/// 1. File does not exist → mutool failed silently.
/// 2. File < 100 bytes → cannot encode any real image; binary header alone is ~8 bytes.
/// 3. PNG cannot be decoded by the `image` crate → corrupt / truncated output.
/// 4. Decoded image has 0×0 dimensions → degenerate render.
///
/// Deliberately NOT flagged as fault (conservative):
/// - Legitimate tiny 1×1 px or all-white pages (these are valid mutool output).
/// - Files between 100 and 500 bytes: a minimal 1×1 PNG is ~67 bytes compressed;
///   mutool adds metadata, so very small files are only corrupt if unreadable.
///
/// The `_input_pdf_bytes` argument is accepted for interface compatibility but is
/// not used: mutool output size does not correlate with input PDF size the way
/// pdfRest output does, so size-ratio heuristics are not applicable here.
pub fn check_mutool_output(png_path: &Path, _input_pdf_bytes: usize) -> OracleFaultCheck {
    // Condition 1: file missing
    if !png_path.exists() {
        return OracleFaultCheck::fault("mutool PNG not found");
    }

    // Condition 2: file too small to be any real PNG
    let file_size = match std::fs::metadata(png_path) {
        Ok(m) => m.len(),
        Err(e) => return OracleFaultCheck::fault(format!("cannot stat PNG: {e}")),
    };
    if file_size < 100 {
        return OracleFaultCheck::fault(format!("mutool PNG is only {file_size} bytes (< 100)"));
    }

    // Conditions 3 + 4: open with the `image` crate; 0×0 = degenerate
    match image::open(png_path) {
        Err(e) => OracleFaultCheck::fault(format!("PNG decode failed: {e}")),
        Ok(img) => {
            if img.width() == 0 || img.height() == 0 {
                OracleFaultCheck::fault("decoded PNG has zero dimensions")
            } else {
                OracleFaultCheck::ok()
            }
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn tmp_file(suffix: &str) -> tempfile::NamedTempFile {
        tempfile::Builder::new()
            .suffix(suffix)
            .tempfile()
            .expect("temp file")
    }

    fn write_valid_png() -> tempfile::NamedTempFile {
        // Encode a 32×32 white RGBA PNG. At 32×32 the compressed file is well
        // above 100 bytes and represents the smallest plausible mutool render
        // (mutool pages are at minimum 100s of pixels at 150 DPI).
        let mut f = tmp_file(".png");
        let img = image::RgbaImage::from_pixel(32, 32, image::Rgba([255, 255, 255, 255]));
        img.write_to(f.as_file_mut(), image::ImageFormat::Png)
            .expect("write png");
        f.flush().expect("flush");
        f
    }

    #[test]
    fn missing_file_is_fault() {
        let result = check_mutool_output(Path::new("/nonexistent/path.png"), 1000);
        assert!(result.is_fault);
        let reason = result.reason.unwrap();
        assert!(reason.contains("not found") || reason.contains("cannot stat"));
    }

    #[test]
    fn tiny_invalid_file_is_fault() {
        let mut f = tmp_file(".png");
        f.write_all(b"NOTAPNG").expect("write");
        // 7 bytes, clearly not a PNG
        let result = check_mutool_output(f.path(), 5000);
        assert!(result.is_fault);
    }

    #[test]
    fn valid_png_is_not_fault() {
        let f = write_valid_png();
        let result = check_mutool_output(f.path(), 5000);
        assert!(
            !result.is_fault,
            "valid 32×32 PNG should not be classified as fault: {:?}",
            result.reason
        );
    }

    #[test]
    fn corrupt_png_is_fault() {
        let mut f = tmp_file(".png");
        // Write PNG magic header + garbage body (>= 100 bytes so file-size check passes)
        let mut data = Vec::new();
        data.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]); // PNG magic
        data.extend(std::iter::repeat_n(0xFFu8, 200)); // garbage
        f.write_all(&data).expect("write");
        let result = check_mutool_output(f.path(), 5000);
        assert!(result.is_fault, "corrupt PNG body should be fault");
    }

    #[test]
    fn normal_png_passes() {
        let f = write_valid_png();
        let check = check_mutool_output(f.path(), 0);
        assert!(!check.is_fault);
        assert!(check.reason.is_none());
    }

    #[test]
    fn input_pdf_bytes_arg_ignored() {
        // Ensure the function signature accepts the arg without affecting result
        let f = write_valid_png();
        let r1 = check_mutool_output(f.path(), 0);
        let r2 = check_mutool_output(f.path(), 1_000_000);
        assert_eq!(r1.is_fault, r2.is_fault);
    }
}
