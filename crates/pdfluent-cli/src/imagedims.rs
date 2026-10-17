//! Image dimensions for `--image` watermarks.
//!
//! The facade inserts JPEG/PNG bytes at caller-chosen coordinates, so the CLI
//! needs the pixel size to centre and scale. Parsed from the file headers
//! (PNG IHDR, JPEG SOF) — plain image-format reading, no PDF logic.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// A watermark image format the facade can insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    /// PNG (`InsertImageFormat::Png`).
    Png,
    /// JPEG (`InsertImageFormat::Jpeg`).
    Jpeg,
}

/// Pixel dimensions and kind of PNG/JPEG `bytes`.
pub(crate) fn dimensions(bytes: &[u8]) -> Result<(u32, u32, ImageKind), String> {
    if let Some(dims) = png_dimensions(bytes) {
        return Ok((dims.0, dims.1, ImageKind::Png));
    }
    if let Some(dims) = jpeg_dimensions(bytes) {
        return Ok((dims.0, dims.1, ImageKind::Jpeg));
    }
    Err("not a PNG or JPEG image (watermarks need one of the two)".to_string())
}

/// Width/height from the PNG IHDR chunk: 8-byte signature, then length(4) +
/// `IHDR` + width(4) + height(4), all big-endian.
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 26 || bytes[0..8] != *SIG {
        return None;
    }
    if &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// Width/height from the first JPEG start-of-frame marker (SOF0–SOF3):
/// scan segments (`FF xx len-hi len-lo …`), read height/width after SOF.
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0..2] != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    while i + 9 < bytes.len() {
        if bytes[i] != 0xFF {
            return None;
        }
        let marker = bytes[i + 1];
        // Standalone markers (RSTn, SOI, EOI, TEM) carry no length.
        if marker == 0x00 || (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if len < 2 || i + 2 + len > bytes.len() {
            return None;
        }
        // SOF0–SOF3 (baseline/extended/progressive/lossless) hold the size;
        // SOF4+ (differential/arithmetic variants) are vanishingly rare and
        // skipped like any other segment.
        if (0xC0..=0xC3).contains(&marker) {
            if len < 7 {
                return None;
            }
            let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
            return (w > 0 && h > 0).then_some((w, h));
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 2, 0, 0, 0]); // bit depth etc.; unread
        v
    }

    fn jpeg_header(w: u16, h: u16) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8]; // SOI
        v.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]); // APP0, len 16
        v.extend_from_slice(&[0u8; 14]);
        v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 8]); // SOF0, len 11, precision
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&[1, 1, 0x11, 0x00]); // 1 component spec; unread
        v
    }

    #[test]
    fn png_reports_its_size() {
        assert_eq!(
            dimensions(&png_header(640, 480)),
            Ok((640, 480, ImageKind::Png))
        );
    }

    #[test]
    fn jpeg_skips_app_segments_to_sof() {
        assert_eq!(
            dimensions(&jpeg_header(320, 200)),
            Ok((320, 200, ImageKind::Jpeg))
        );
    }

    #[test]
    fn garbage_is_neither() {
        assert!(dimensions(b"not an image").is_err());
        assert!(dimensions(b"%PDF-1.7 hi").is_err());
        assert!(dimensions(&[]).is_err());
        // Truncated headers read as failure, not a panic.
        assert!(dimensions(b"\x89PNG\r\n\x1a\n").is_err());
        assert!(dimensions(&[0xFF, 0xD8, 0xFF]).is_err());
    }
}
