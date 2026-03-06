//! Signature appearance rendering (ISO 32000-2 §12.8.6).
//!
//! Generates and parses signature appearance streams for visual
//! representation of digital signatures in PDF pages.

use crate::sig_dict::SigDict;
use crate::types::SignatureAppearanceStyle;

/// Information extracted from a signature for rendering its appearance.
#[derive(Debug, Clone)]
pub struct SignatureAppearanceInfo {
    /// Signer name.
    pub signer_name: Option<String>,
    /// Signing reason.
    pub reason: Option<String>,
    /// Signing location.
    pub location: Option<String>,
    /// Contact info.
    pub contact_info: Option<String>,
    /// Signing date.
    pub date: Option<String>,
    /// Whether the signature has a custom appearance stream.
    pub has_custom_appearance: bool,
}

impl SignatureAppearanceInfo {
    /// Extract appearance info from a signature dictionary.
    pub fn from_sig(sig: &SigDict<'_>) -> Self {
        Self {
            signer_name: sig.signer_name(),
            reason: sig.reason(),
            location: sig.location(),
            contact_info: sig.contact_info(),
            date: sig.signing_time(),
            has_custom_appearance: false,
        }
    }
}

/// Generate a standard signature appearance stream.
///
/// Returns raw PDF content stream operators for a text-based
/// signature appearance showing signer, date, reason, and location.
pub fn generate_appearance_stream(
    info: &SignatureAppearanceInfo,
    width: f64,
    height: f64,
    style: &SignatureAppearanceStyle,
) -> Vec<u8> {
    let mut ops = Vec::new();

    // Draw a border rectangle.
    ops.extend_from_slice(
        format!("0.5 0.5 {:.2} {:.2} re S\n", width - 1.0, height - 1.0).as_bytes(),
    );

    let font_size = 8.0f64;
    let line_height = font_size * 1.3;
    let margin = 4.0;

    // Begin text.
    ops.extend_from_slice(b"BT\n");
    ops.extend_from_slice(format!("/F1 {font_size:.1} Tf\n").as_bytes());

    let mut y = height - margin - font_size;

    match style {
        SignatureAppearanceStyle::Standard => {
            if let Some(name) = &info.signer_name {
                write_text_line(
                    &mut ops,
                    margin,
                    y,
                    font_size,
                    &format!("Signed by: {name}"),
                );
                y -= line_height;
            }
            if let Some(date) = &info.date {
                write_text_line(&mut ops, margin, y, font_size, &format!("Date: {date}"));
                y -= line_height;
            }
            if let Some(reason) = &info.reason {
                write_text_line(&mut ops, margin, y, font_size, &format!("Reason: {reason}"));
                y -= line_height;
            }
            if let Some(location) = &info.location {
                write_text_line(
                    &mut ops,
                    margin,
                    y,
                    font_size,
                    &format!("Location: {location}"),
                );
            }
        }
        SignatureAppearanceStyle::Description(text) => {
            for (i, line) in text.lines().enumerate() {
                write_text_line(
                    &mut ops,
                    margin,
                    y - (i as f64 * line_height),
                    font_size,
                    line,
                );
            }
        }
    }

    ops.extend_from_slice(b"ET\n");
    ops
}

fn write_text_line(ops: &mut Vec<u8>, x: f64, y: f64, _font_size: f64, text: &str) {
    // Escape PDF string characters.
    let escaped: String = text
        .chars()
        .flat_map(|c| match c {
            '(' => vec!['\\', '('],
            ')' => vec!['\\', ')'],
            '\\' => vec!['\\', '\\'],
            c => vec![c],
        })
        .collect();
    // Use Tm (text matrix) for absolute positioning instead of cumulative Td.
    // Scale is 1 (identity) because font size is already set via Tf.
    ops.extend_from_slice(format!("1 0 0 1 {x:.2} {y:.2} Tm ({escaped}) Tj\n").as_bytes());
}

/// Check if a signature field widget has an existing appearance stream.
pub fn has_appearance_stream(field_dict: &pdf_syntax::object::Dict<'_>) -> bool {
    field_dict
        .get::<pdf_syntax::object::Dict<'_>>(pdf_syntax::object::dict::keys::AP)
        .is_some()
}
