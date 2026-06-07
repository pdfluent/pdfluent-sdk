//! Canonical, serializable wire form of an extracted text span.
//!
//! [`TextSpanInfo`] is the **single source of truth** for the shape of a
//! positioned text span as delivered to every SDK binding (Tauri/serde,
//! Node/napi, WASM). Bindings construct it from a [`TextSpan`] via [`From`]
//! and then serialize or re-shape it for their target runtime, instead of each
//! maintaining a separate field list and conversion logic.
//!
//! The `serde` derive is feature-gated so the core engine stays
//! serialization-agnostic; the struct and the `From` conversion are always
//! available, so non-serde bindings (e.g. napi) consume the same source.

use crate::text::{FontMetrics, TextSpan, WidthSource};

/// A positioned text span in PDF user space (origin bottom-left, y up).
///
/// JSON key renames mirror the established wire contract consumed by the
/// editor; all optional metadata is omitted from JSON when absent, so existing
/// clients are unaffected.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TextSpanInfo {
    /// The extracted text of the span.
    pub text: String,
    /// X position in PDF user space.
    pub x: f64,
    /// Y position in PDF user space (baseline origin).
    pub y: f64,
    /// Span width in user space. Falls back to an estimate (½ em per character)
    /// when no measured width is available.
    pub width: f64,
    /// Span height in user space.
    ///
    /// Mirrors the existing wire contract, which reports the font size (ascent
    /// extent) here. NOTE: this preserves prior behaviour exactly; richer
    /// ascent/descent metrics arrive in a later milestone.
    pub height: f64,
    /// Font size in points.
    pub font_size: f64,

    /// PostScript font name with any 6-character subset prefix stripped.
    /// Omitted from JSON when unknown.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "fontName", skip_serializing_if = "Option::is_none")
    )]
    pub font_name: Option<String>,
    /// Inferred bold style.
    #[cfg_attr(feature = "serde", serde(rename = "isBold"))]
    pub is_bold: bool,
    /// Inferred italic style.
    #[cfg_attr(feature = "serde", serde(rename = "isItalic"))]
    pub is_italic: bool,
    /// Fill colour as sRGB `[r, g, b]` normalised to 0.0–1.0. Omitted from JSON
    /// for pattern/shading paints.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub color: Option<[f32; 3]>,

    /// Whether glyph widths came from real font metrics or an estimate.
    /// Serialised as `"Metric"` / `"Estimate"`.
    #[cfg_attr(feature = "serde", serde(rename = "widthSource"))]
    pub width_source: WidthSource,
    /// Per-glyph bounding boxes `[x0, y0, x1, y1]` (y up). Omitted when empty.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "charBounds", skip_serializing_if = "Vec::is_empty")
    )]
    pub char_bounds: Vec<[f64; 4]>,

    // ---- Golf 1 typographic metadata ----
    /// Full affine transform `[a, b, c, d, e, f]` of the span's first glyph.
    /// Captures rotation/shear discarded by `(x, y, fontSize)`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub transform: Option<[f64; 6]>,
    /// Numeric font weight (~100–900) from embedded font data, if available.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "fontWeight", skip_serializing_if = "Option::is_none")
    )]
    pub font_weight: Option<u16>,
    /// Serif flag from embedded font data, if available.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "isSerif", skip_serializing_if = "Option::is_none")
    )]
    pub is_serif: Option<bool>,
    /// Monospace flag from embedded font data, if available.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "isMonospace", skip_serializing_if = "Option::is_none")
    )]
    pub is_monospace: Option<bool>,
    /// Coarse PDF text render mode: `0` fill, `1` stroke, `3` invisible.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "renderMode", skip_serializing_if = "Option::is_none")
    )]
    pub render_mode: Option<u8>,

    // ---- Golf 2 font metrics ----
    /// Vertical font metrics (/1000 em) from the embedded font, if available.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "fontMetrics", skip_serializing_if = "Option::is_none")
    )]
    pub font_metrics: Option<FontMetrics>,
}

impl From<TextSpan> for TextSpanInfo {
    /// Build the canonical wire form from an owned engine [`TextSpan`].
    ///
    /// Centralises the conversions that previously lived in each binding: the
    /// ½-em width fallback, the RGBA-u8 → RGB-f32 colour normalisation, and the
    /// height-reports-font-size contract.
    fn from(s: TextSpan) -> Self {
        let char_count = s.text.chars().count() as f64;
        let width = if s.width > 0.0 {
            s.width
        } else {
            s.font_size * 0.5 * char_count
        };
        let color = s
            .color
            .map(|[r, g, b, _a]| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]);
        Self {
            x: s.x,
            y: s.y,
            width,
            height: s.font_size,
            font_size: s.font_size,
            font_name: s.font_name,
            is_bold: s.is_bold,
            is_italic: s.is_italic,
            color,
            width_source: s.width_source,
            char_bounds: s.char_bounds,
            transform: s.transform,
            font_weight: s.font_weight,
            is_serif: s.is_serif,
            is_monospace: s.is_monospace,
            render_mode: s.render_mode,
            font_metrics: s.font_metrics,
            text: s.text,
        }
    }
}

impl From<&TextSpan> for TextSpanInfo {
    /// Build the canonical wire form from a borrowed [`TextSpan`] (clones).
    fn from(s: &TextSpan) -> Self {
        TextSpanInfo::from(s.clone())
    }
}
