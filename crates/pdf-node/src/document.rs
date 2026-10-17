//! PdfDocument class exposed to Node.js.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::annotation::{self, AnnotationInfo};
use crate::error;
use crate::form::{FormEngine, FormFieldInfo};
use crate::page::PdfPage;
use lopdf::Document as LopdfDocument;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use pdf_engine::{
    Block as PdfBlock, BlockKind, ExtractOptions, OcrMode, PageSeparators,
    PdfDocument as RustDocument, Quality, RenderOptions, RenderedPage, TatrOptions,
    ThumbnailOptions, Word as PdfWord,
};
use pdf_forms::{apply_choice_multi, apply_field_value, WriteOutcome, WriteValue, WritebackError};
use std::sync::{Arc, Mutex};

/// Apply a string value with type-aware dispatch, mirroring the CLI's
/// `fill_one` (crates/xfa-cli/src/cmd_fill.rs): try Text first (the common
/// case), then on a `/FT` type mismatch fall through to Radio, Choice, and
/// finally Checkbox with a bool-ish string.
///
/// All writes go through [`pdf_forms::apply_field_value`] — the single SDK
/// writeback chain — keeping `/V` encoding, per-widget `/AS`, and `/AP`
/// regeneration consistent.
fn apply_string_value(
    doc: &mut LopdfDocument,
    name: &str,
    value: &str,
) -> std::result::Result<WriteOutcome, WritebackError> {
    match apply_field_value(doc, name, WriteValue::Text(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    match apply_field_value(doc, name, WriteValue::Radio(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    match apply_field_value(doc, name, WriteValue::Choice(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    // Checkbox via bool-ish string ("true"/"Yes"/"Off"/"false").
    let on = !matches!(value, "false" | "Off" | "0" | "");
    apply_field_value(doc, name, WriteValue::Checkbox(on))
}

/// A PDF document handle.
///
/// Open a document with `PdfDocument.open(buffer)` or
/// `PdfDocument.openAsync(buffer)`.
#[napi]
pub struct PdfDocument {
    inner: Arc<RustDocument>,
    form_engine: Option<Arc<FormEngine>>,
    /// Mutable lopdf document for write operations (annotations, redact, encrypt, save).
    /// `None` if the PDF could not be loaded by lopdf (rare edge case).
    doc: Option<Arc<Mutex<LopdfDocument>>>,
    /// Why `doc` is `None`, when it is. Kept so write operations can say what
    /// actually went wrong instead of only that the document is unwritable —
    /// a corrupted checkout used to surface here as a bare "not writable".
    doc_load_error: Option<String>,
}

impl PdfDocument {
    /// Apply a closure to the mutable lopdf document, returning an error if
    /// the document is not writable.
    fn with_doc_mut<T, F>(&self, operation: &str, f: F) -> napi::Result<T>
    where
        F: FnOnce(&mut LopdfDocument) -> napi::Result<T>,
    {
        let arc = self.writable(operation)?;
        let mut doc = arc.lock().unwrap();
        f(&mut doc)
    }

    /// Open from owned bytes; `operation` names the JavaScript call for the
    /// error, since `openPdf` lands here too.
    pub(crate) fn open_bytes(bytes: Vec<u8>, operation: &str) -> napi::Result<PdfDocument> {
        let doc = RustDocument::open(bytes.clone()).map_err(error::engine(operation))?;
        let form_engine = FormEngine::from_pdf(doc.pdf()).map(Arc::new);
        let (lopdf_doc, doc_load_error) = match LopdfDocument::load_mem(&bytes) {
            Ok(d) => (Some(Arc::new(Mutex::new(d))), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Ok(PdfDocument {
            inner: Arc::new(doc),
            form_engine,
            doc: lopdf_doc,
            doc_load_error,
        })
    }

    /// Shared body of `setFormField` and its deprecated alias `setFieldValue`;
    /// `operation` is whichever of the two the caller used, so an error names it.
    fn set_form_field_as(&mut self, name: &str, value: &str, operation: &str) -> Result<()> {
        self.with_doc_mut(operation, |doc| {
            apply_string_value(doc, name, value)
                .map(|_| ())
                .map_err(|e| error::writeback(operation, format!("{operation} '{name}': {e}"), &e))
        })?;
        // Keep the form engine's in-memory view in sync so getFieldValue()
        // reflects the write without reloading the document.
        if let Some(fe) = &self.form_engine {
            fe.set_value(name, value, operation)?;
        }
        self.rebuild_inner(operation)
    }

    /// The mutable lopdf document, or `E-PARSE-INVALID-PDF` saying why lopdf
    /// could not load it.
    fn writable(&self, operation: &str) -> napi::Result<&Arc<Mutex<LopdfDocument>>> {
        self.doc.as_ref().ok_or_else(|| {
            error::invalid_pdf(
                operation,
                match &self.doc_load_error {
                    Some(why) => format!("document is not writable: {why}"),
                    None => "document is not writable".to_string(),
                },
            )
        })
    }

    /// Rebuild the rendering engine from the current lopdf state.
    ///
    /// Call this after any mutation so that `render_page` reflects the change.
    /// Outstanding `PdfPage` handles obtained before this call are stale.
    fn rebuild_inner(&mut self, operation: &str) -> napi::Result<()> {
        if let Some(arc) = &self.doc {
            let mut buf = Vec::with_capacity(64 * 1024);
            arc.lock()
                .unwrap()
                .clone()
                .save_to(&mut buf)
                .map_err(|e| error::internal(operation, format!("engine refresh failed: {e}")))?;
            self.inner = Arc::new(RustDocument::open(buf).map_err(error::engine(operation))?);
        }
        Ok(())
    }
}

/// Document metadata.
#[napi(object)]
pub struct DocumentInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
}

/// A bookmark / outline item.
#[napi(object)]
pub struct BookmarkItem {
    pub title: String,
    pub page: Option<u32>,
    pub children: Vec<BookmarkItem>,
}

/// Render options passed from JavaScript.
#[napi(object)]
#[derive(Default)]
pub struct RenderOpts {
    /// DPI (default: 72.0).
    pub dpi: Option<f64>,
    /// Background RGBA [r, g, b, a] each 0.0–1.0.
    pub background: Option<Vec<f64>>,
    /// Force output width in pixels.
    pub width: Option<u32>,
    /// Force output height in pixels.
    pub height: Option<u32>,
}

/// Rendered page result returned to JavaScript.
#[napi(object)]
pub struct RenderResult {
    /// RGBA pixel data.
    pub data: Buffer,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Page geometry information.
#[napi(object)]
pub struct PageGeometry {
    /// Width in PDF points.
    pub width: f64,
    /// Height in PDF points.
    pub height: f64,
    /// Rotation in degrees.
    pub rotation: u32,
}

/// A text span at a specific position.
///
/// Field *values* are derived from the canonical [`pdf_engine::TextSpanInfo`]
/// (the SDK single source of truth) via the `From` impl below, so the Node
/// binding cannot drift from the Tauri/serde wire form in its value logic.
/// (JS key casing follows napi's own camelCase convention.)
#[napi(object)]
pub struct TextSpanInfo {
    /// The extracted text.
    pub text: String,
    /// X position in user space.
    pub x: f64,
    /// Y position in user space.
    pub y: f64,
    /// Span width in user space.
    pub width: f64,
    /// Span height in user space (font-size / ascent extent).
    pub height: f64,
    /// Font size in points.
    pub font_size: f64,
    /// PostScript font name with subset prefix stripped, if known.
    pub font_name: Option<String>,
    /// Inferred bold style.
    pub is_bold: bool,
    /// Inferred italic style.
    pub is_italic: bool,
    /// Fill colour as sRGB `[r, g, b]` in 0.0–1.0, if resolved.
    pub color: Option<Vec<f64>>,
    /// Width provenance: "Metric" or "Estimate".
    pub width_source: String,
    /// Per-glyph bounds `[x0, y0, x1, y1]` (y up), if available.
    pub char_bounds: Option<Vec<Vec<f64>>>,
    /// Full affine transform `[a, b, c, d, e, f]` of the first glyph, if any.
    pub transform: Option<Vec<f64>>,
    /// Numeric font weight (~100–900) from embedded font data, if available.
    pub font_weight: Option<u32>,
    /// Serif flag from embedded font data, if available.
    pub is_serif: Option<bool>,
    /// Monospace flag from embedded font data, if available.
    pub is_monospace: Option<bool>,
    /// Coarse PDF text render mode: 0 fill, 1 stroke, 3 invisible.
    pub render_mode: Option<u32>,
    /// Vertical font metrics (/1000 em) from the embedded font, if available.
    pub font_metrics: Option<FontMetricsInfo>,
}

/// Vertical font metrics returned to JavaScript (values in /1000 em).
#[napi(object)]
pub struct FontMetricsInfo {
    /// Ascent above the baseline.
    pub ascent: f64,
    /// Descent below the baseline (negative).
    pub descent: f64,
    /// Cap height, if present in the font.
    pub cap_height: Option<f64>,
    /// x-height, if present in the font.
    pub x_height: Option<f64>,
}

impl From<pdf_engine::TextSpan> for TextSpanInfo {
    /// Re-shape the canonical [`pdf_engine::TextSpanInfo`] into the napi object.
    /// All value logic lives in the canonical DTO; this only adapts container
    /// types to napi-friendly ones.
    fn from(span: pdf_engine::TextSpan) -> Self {
        let c = pdf_engine::TextSpanInfo::from(span);
        TextSpanInfo {
            text: c.text,
            x: c.x,
            y: c.y,
            width: c.width,
            height: c.height,
            font_size: c.font_size,
            font_name: c.font_name,
            is_bold: c.is_bold,
            is_italic: c.is_italic,
            color: c.color.map(|[r, g, b]| vec![r as f64, g as f64, b as f64]),
            width_source: c.width_source.as_str().to_string(),
            char_bounds: if c.char_bounds.is_empty() {
                None
            } else {
                Some(c.char_bounds.iter().map(|b| b.to_vec()).collect())
            },
            transform: c.transform.map(|t| t.to_vec()),
            font_weight: c.font_weight.map(|w| w as u32),
            is_serif: c.is_serif,
            is_monospace: c.is_monospace,
            render_mode: c.render_mode.map(|m| m as u32),
            font_metrics: c.font_metrics.map(|m| FontMetricsInfo {
                ascent: m.ascent,
                descent: m.descent,
                cap_height: m.cap_height,
                x_height: m.x_height,
            }),
        }
    }
}

#[cfg(test)]
mod text_span_info_tests {
    use super::TextSpanInfo;

    #[test]
    fn napi_span_derives_from_canonical_dto() {
        let span = pdf_engine::TextSpan {
            text: "Hi".to_string(),
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 9.0,
            font_size: 4.0,
            complete_run: false,
            font_name: Some("Arial".to_string()),
            is_bold: true,
            is_italic: false,
            color: Some([255, 128, 0, 255]),
            width_source: pdf_engine::WidthSource::Metric,
            char_bounds: vec![[1.0, 2.0, 3.0, 6.0]],
            geometry_mode: pdf_engine::GeometryMode::Basic,
            bounds_source: pdf_engine::BoundsSource::default(),
            tight_char_bounds: vec![],
            glyph_advances: vec![],
            glyph_bounds_sources: vec![],
            transform: Some([0.5, 0.0, 0.0, 0.5, 1.0, 2.0]),
            font_weight: Some(400),
            is_serif: Some(false),
            is_monospace: Some(true),
            render_mode: Some(3),
            is_ocr_layer: false,
            font_metrics: Some(pdf_engine::FontMetrics {
                ascent: 750.0,
                descent: -250.0,
                cap_height: Some(700.0),
                x_height: Some(500.0),
            }),
        };
        let canonical = pdf_engine::TextSpanInfo::from(span.clone());
        let napi = TextSpanInfo::from(span);

        assert_eq!(napi.text, canonical.text);
        assert_eq!(napi.x, canonical.x);
        assert_eq!(napi.y, canonical.y);
        assert_eq!(napi.width, canonical.width);
        // height mirrors font_size in the canonical contract.
        assert_eq!(napi.height, canonical.height);
        assert_eq!(napi.font_size, canonical.font_size);
        assert_eq!(napi.font_name, canonical.font_name);
        assert_eq!(napi.is_bold, canonical.is_bold);
        assert_eq!(napi.is_italic, canonical.is_italic);
        assert_eq!(napi.width_source, canonical.width_source.as_str());
        assert_eq!(
            napi.color,
            canonical
                .color
                .map(|[r, g, b]| vec![r as f64, g as f64, b as f64])
        );
        assert_eq!(
            napi.char_bounds,
            Some(
                canonical
                    .char_bounds
                    .iter()
                    .map(|b| b.to_vec())
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(napi.transform, canonical.transform.map(|t| t.to_vec()));
        assert_eq!(napi.font_weight, canonical.font_weight.map(|w| w as u32));
        assert_eq!(napi.is_serif, canonical.is_serif);
        assert_eq!(napi.is_monospace, canonical.is_monospace);
        assert_eq!(napi.render_mode, canonical.render_mode.map(|m| m as u32));
        assert_eq!(
            napi.font_metrics.is_some(),
            canonical.font_metrics.is_some()
        );
        if let (Some(n), Some(cm)) = (&napi.font_metrics, &canonical.font_metrics) {
            assert_eq!(n.ascent, cm.ascent);
            assert_eq!(n.descent, cm.descent);
            assert_eq!(n.cap_height, cm.cap_height);
            assert_eq!(n.x_height, cm.x_height);
        }
    }
}

#[cfg(test)]
mod extract_opts_tests {
    //! Pin the `ExtractOpts` conversion: the window and the option mappings.

    use super::{convert_extract_opts, ExtractOpts};
    use pdf_engine::{OcrMode, PageSeparators};

    fn empty() -> ExtractOpts {
        ExtractOpts {
            first_page: None,
            page_count: None,
            pages: None,
            ocr: None,
            ocr_language: None,
            ocr_dpi: None,
            include_headers: None,
            include_footers: None,
            page_separators: None,
            code: None,
            quality: None,
            model_dir: None,
            table_model_dir: None,
            keep_furniture: None,
        }
    }

    fn code_of(e: napi::Error) -> String {
        let payload: serde_json::Value = serde_json::from_str(&e.reason).expect("JSON payload");
        payload["code"].as_str().expect("code").to_string()
    }

    #[test]
    fn missing_options_are_the_whole_document() {
        let converted = convert_extract_opts(&None, "extractMarkdown").expect("default");
        assert_eq!(converted, pdf_engine::ExtractOptions::default());
        let converted = convert_extract_opts(&Some(empty()), "extractMarkdown").expect("{}");
        assert_eq!(converted, pdf_engine::ExtractOptions::default());
    }

    #[test]
    fn switching_code_off_passes_through() {
        let mut o = empty();
        o.code = Some(false);
        let converted = convert_extract_opts(&Some(o), "extractMarkdown").expect("code off");
        assert!(!converted.code);
    }

    #[test]
    fn a_page_list_passes_through() {
        let mut o = empty();
        o.pages = Some(vec![2, 0]);
        let converted = convert_extract_opts(&Some(o), "extractBlocks").expect("pages");
        assert_eq!(converted.pages, Some(vec![2, 0]));
        assert_eq!(converted.first_page, 0);
        assert_eq!(converted.page_count, None);
    }

    #[test]
    fn furniture_and_separators_pass_through() {
        let mut o = empty();
        o.include_headers = Some(true);
        o.include_footers = Some(true);
        o.page_separators = Some("none".to_string());
        let converted = convert_extract_opts(&Some(o), "extractMarkdown").expect("switches");
        assert!(converted.include_headers);
        assert!(converted.include_footers);
        assert_eq!(converted.page_separators, PageSeparators::None);
    }

    #[test]
    fn headers_without_footers_pass_through() {
        let mut o = empty();
        o.include_headers = Some(true);
        let converted = convert_extract_opts(&Some(o), "extractMarkdown").expect("headers only");
        assert!(converted.include_headers);
        assert!(!converted.include_footers);
    }

    #[test]
    fn unknown_ocr_and_separator_values_are_unsupported() {
        let mut bad_ocr = empty();
        bad_ocr.ocr = Some("sometimes".to_string());
        let mut zero_dpi = empty();
        zero_dpi.ocr = Some("auto".to_string());
        zero_dpi.ocr_dpi = Some(0);
        let mut bad_separators = empty();
        bad_separators.page_separators = Some("rule".to_string());
        for o in [bad_ocr, zero_dpi, bad_separators] {
            let err = convert_extract_opts(&Some(o), "extractMarkdown").expect_err("must fail");
            assert_eq!(code_of(err), "E-UNSUPPORTED");
        }
    }

    #[test]
    fn the_ocr_modes_pass_through() {
        for (asked, mode) in [
            ("never", OcrMode::Never),
            ("auto", OcrMode::Auto),
            ("force", OcrMode::Force),
        ] {
            let mut o = empty();
            o.ocr = Some(asked.to_string());
            o.ocr_language = Some("deu".to_string());
            o.ocr_dpi = Some(300);
            let converted = convert_extract_opts(&Some(o), "extractMarkdown").expect("ocr mode");
            assert_eq!(converted.ocr, mode);
            assert_eq!(converted.ocr_language, "deu");
            assert_eq!(converted.ocr_dpi, 300);
        }
    }

    #[test]
    fn the_tiers_and_the_model_dirs_pass_through() {
        let mut o = empty();
        o.quality = Some("balanced".to_string());
        o.model_dir = Some("/models/layout".to_string());
        let converted = convert_extract_opts(&Some(o), "extractMarkdown").expect("tier");
        assert_eq!(converted.quality, pdf_engine::Quality::Balanced);
        assert_eq!(
            converted.model_dir,
            Some(std::path::PathBuf::from("/models/layout"))
        );
        // And the default is the shipping engine with no dirs.
        let converted = convert_extract_opts(&Some(empty()), "extractMarkdown").expect("{}");
        assert_eq!(converted.quality, pdf_engine::Quality::Fast);
        assert_eq!(converted.model_dir, None);
    }

    #[test]
    fn an_unknown_quality_is_unsupported() {
        let mut o = empty();
        o.quality = Some("ultra".to_string());
        let err = convert_extract_opts(&Some(o), "extractMarkdown").expect_err("must fail");
        assert_eq!(code_of(err), "E-UNSUPPORTED");
    }

    #[test]
    fn keep_furniture_passes_through() {
        // Off unless asked: the empty options read as the engine default,
        // which drops the balanced tier's furniture lines.
        let converted = convert_extract_opts(&Some(empty()), "extractMarkdown").expect("{}");
        assert!(!converted.keep_furniture);
        let mut o = empty();
        o.keep_furniture = Some(true);
        let converted = convert_extract_opts(&Some(o), "extractMarkdown").expect("keep furniture");
        assert!(converted.keep_furniture);
    }

    #[test]
    fn table_model_dir_opts_into_tatr_over_the_library_defaults() {
        // The flag's directory over the library defaults, exactly as a
        // library caller builds them: equal options into the same
        // `extract_markdown` are equal markdown.
        let mut o = empty();
        o.table_model_dir = Some("/models/tatr".to_string());
        let converted = convert_extract_opts(&Some(o), "extractMarkdown").expect("tatr opt-in");
        let mut tatr = pdf_engine::TatrOptions::default();
        tatr.model_dir = Some(std::path::PathBuf::from("/models/tatr"));
        assert_eq!(
            converted,
            pdf_engine::ExtractOptions::default().with_tatr(tatr)
        );
    }
}

/// A block of text (grouped by vertical proximity).
#[napi(object)]
pub struct TextBlockInfo {
    /// Concatenated text of the block.
    pub text: String,
    /// Individual spans within this block.
    pub spans: Vec<TextSpanInfo>,
}

/// One image stored on a page, as `pageImages` lists it.
#[napi(object)]
pub struct PageImageInfo {
    /// Handle for `extractImageData` and `decodeImagePixels`.
    pub id: u32,
    /// Image width, in pixels.
    pub width: u32,
    /// Image height, in pixels.
    pub height: u32,
    /// Bits per component.
    pub bits_per_component: u32,
    /// Components per pixel.
    pub components: u32,
    /// Decoded pixel layout: `DeviceGray`, `DeviceRGB` or `DeviceCMYK`.
    pub color_space: String,
    /// Colour-space name recorded in the image dictionary.
    pub color_space_name: String,
    /// First filter name, normalised, or empty for raw samples.
    pub filter: String,
    /// Resource name the page lists the image under.
    pub name: String,
    /// Whether the image carries a soft mask.
    pub has_mask: bool,
}

/// One extracted image: header facts plus encoded bytes.
#[napi(object)]
pub struct ExtractedImageInfo {
    /// Image width, in pixels.
    pub width: u32,
    /// Image height, in pixels.
    pub height: u32,
    /// Bits per component.
    pub bits_per_component: u32,
    /// Components per pixel.
    pub components: u32,
    /// Decoded pixel layout: `DeviceGray`, `DeviceRGB` or `DeviceCMYK`.
    pub color_space: String,
    /// Colour-space name recorded in the image dictionary.
    pub color_space_name: String,
    /// Encoding of `data`: `jpeg` or `png`.
    pub encoding: String,
    /// Encoded image bytes.
    pub data: Buffer,
    /// Whether the image is masked.
    pub masked: bool,
    /// Horizontal resolution, in dots per inch.
    pub xres: u32,
    /// Vertical resolution, in dots per inch.
    pub yres: u32,
}

/// One decoded image: raw samples, top row first.
#[napi(object)]
pub struct DecodedImageInfo {
    /// Image width, in pixels.
    pub width: u32,
    /// Image height, in pixels.
    pub height: u32,
    /// Components per pixel.
    pub components: u32,
    /// Decoded pixel layout: `DeviceGray`, `DeviceRGB` or `DeviceCMYK`.
    pub color_space: String,
    /// Raw samples, `components` bytes per pixel.
    pub pixels: Buffer,
}

/// Which pages `extractBlocks` and `extractMarkdown` hand back (#573),
/// and how `extractMarkdown` treats figures.
///
/// Every field is optional, so `{}` and a missing argument mean the same
/// thing they mean in Rust: the whole document, without figures.
#[napi(object)]
pub struct ExtractOpts {
    /// First page of the window, counted from zero. Default 0.
    pub first_page: Option<u32>,
    /// How many pages from `firstPage`; absent for the rest of the document.
    pub page_count: Option<u32>,
    /// An explicit list of pages, counted from zero, instead of the window.
    /// Absent for the window. Pages come back in reading order whatever
    /// order lists them; a page past the last one, or a list beside a
    /// window, is an error.
    pub pages: Option<Vec<u32>>,
    /// `"never"` (default: read what the document draws), `"auto"`
    /// (recognize the pages that need it) or `"force"` (recognize every page
    /// in the window, replacing a born-digital page's own text rather than
    /// doubling it).
    pub ocr: Option<String>,
    /// The Tesseract language tag OCR runs in. Default `"eng"`.
    pub ocr_language: Option<String>,
    /// The resolution OCR renders pages at, in dots per inch. Default 150.
    pub ocr_dpi: Option<u32>,
    /// Write the running heads as plain paragraphs at their reading-order
    /// position. Default `false`. Only `extractMarkdown` reads it.
    pub include_headers: Option<bool>,
    /// Write the page numbers and running footers as plain paragraphs at
    /// their reading-order position. Default `false`. Only `extractMarkdown`
    /// reads it.
    pub include_footers: Option<bool>,
    /// `"comment"` (default: `<!-- page N -->` where the page changes) or
    /// `"none"`: no marker, paragraphs separated by a blank line. Only
    /// `extractMarkdown` reads it.
    pub page_separators: Option<String>,
    pub code: Option<bool>,
    /// `"fast"` (default: the shipping engine) or `"balanced"` (the layout
    /// model tier, markdown only). Anything else is `E-UNSUPPORTED`.
    pub quality: Option<String>,
    /// Where the model tier looks for its weights. Absent resolves through
    /// `PDFLUENT_MODEL_DIR` to the per-user model dir. Read only with
    /// `quality: 'balanced'`.
    pub model_dir: Option<String>,
    /// Where the TATR table model lives: set, the balanced tier's table
    /// regions are structured with TATR first; absent, TATR stays off.
    pub table_model_dir: Option<String>,
    /// Keep the balanced tier's running heads and feet as paragraphs
    /// instead of dropping them. Default `false`. Read only with
    /// `quality: 'balanced'`.
    pub keep_furniture: Option<bool>,
}

/// One block of a document's structure: what a caller chunks, cites or embeds
/// (#573).
///
/// `kind` is one of `paragraph`, `heading`, `listItem`, `tableCell`, `header`,
/// `footer` or `caption` -- the same seven names the JSON serializer writes -- and the
/// fields belonging to that kind carry a value while the rest are absent.
#[napi(object)]
pub struct GridDoubtInfo {
    /// Rows admitted with too few runs of ink to be a row on their own.
    pub thin_rows: u32,
    /// The share of the grid's cells the page drew ink in, 0 to 1.
    pub fill: f64,
    /// Which attempt produced the grid; above 0 means rows had to be trimmed.
    pub retried_from: u32,
    /// Rows with ink in every column, which the column bounds rest on.
    pub full_rows: u32,
    /// How many columns the grid has, so the facts above have their scale.
    pub columns: u32,
}

#[napi(object)]
pub struct BlockInfo {
    /// `paragraph`, `heading`, `listItem`, `tableCell`, `header`, `footer` or `caption`.
    pub kind: String,
    /// The block's text, the line breaks of the measure taken out.
    pub text: String,
    /// 0-based page index the block was drawn on.
    pub page: u32,
    /// `[x0, y0, x1, y1]` in PDF user space, y up.
    pub bbox: Vec<f64>,
    /// Place in the document's reading order, counted from the first block of
    /// the first page -- the document's numbering, not the window's.
    pub index: u32,
    /// Whether the block continues the one before it, because the page broke a
    /// paragraph in two.
    pub continues_previous: bool,
    /// How sure the learned model is of `kind`, between 0 and 1, or absent where
    /// no model spoke about this block.
    ///
    /// ABSENT IS NOT LOW. A block no model has an opinion about carries the
    /// heuristics' verdict, and the heuristics produce a decision rather than a
    /// probability, so an absent value means nobody measured this block. Code
    /// that routes the doubtful cases must treat it as unknown and not as zero.
    pub confidence: Option<f64>,
    /// What the heuristic builder had to go on when it found this cell's table,
    /// absent for a block that is not a table cell.
    ///
    /// Five facts and not a score, and a property of the table: every cell of one
    /// table carries the same values.
    pub grid: Option<GridDoubtInfo>,
    /// A heading's level (1, 2 or 3) or a list item's nesting depth.
    pub level: Option<u32>,
    /// The number the document wrote on a list item, absent for a bullet.
    pub ordinal: Option<u32>,
    /// A table cell's 0-based row.
    pub row: Option<u32>,
    /// A table cell's 0-based column, counted across the whole table.
    pub column: Option<u32>,
    /// How many rows a table cell covers.
    pub row_span: Option<u32>,
    /// How many columns a table cell covers.
    pub column_span: Option<u32>,
    /// Whether a table cell is in the table's header row.
    pub header: Option<bool>,
}

impl From<PdfBlock> for BlockInfo {
    fn from(block: PdfBlock) -> Self {
        let (kind, level, ordinal, row, column, row_span, column_span, header) = match block.kind {
            BlockKind::Paragraph => ("paragraph", None, None, None, None, None, None, None),
            BlockKind::Heading { level } => (
                "heading",
                Some(level as u32),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            BlockKind::ListItem { level, ordinal } => (
                "listItem",
                Some(level as u32),
                ordinal,
                None,
                None,
                None,
                None,
                None,
            ),
            BlockKind::TableCell {
                row,
                column,
                row_span,
                column_span,
                header,
            } => (
                "tableCell",
                None,
                None,
                Some(row as u32),
                Some(column as u32),
                Some(row_span as u32),
                Some(column_span as u32),
                Some(header),
            ),
            BlockKind::Header => ("header", None, None, None, None, None, None, None),
            BlockKind::Footer => ("footer", None, None, None, None, None, None, None),
            BlockKind::Caption => ("caption", None, None, None, None, None, None, None),
            // `BlockKind` is `#[non_exhaustive]`: a kind added to the engine
            // reaches Node as a paragraph rather than stopping the build of a
            // crate that has not been taught about it yet.
            _ => ("paragraph", None, None, None, None, None, None, None),
        };
        BlockInfo {
            kind: kind.to_string(),
            text: block.text,
            page: block.page as u32,
            bbox: block.bbox.to_vec(),
            index: block.index as u32,
            continues_previous: block.continues_previous,
            confidence: block.confidence,
            grid: block.grid.map(|g| GridDoubtInfo {
                thin_rows: g.thin_rows as u32,
                fill: g.fill,
                retried_from: g.retried_from as u32,
                full_rows: g.full_rows as u32,
                columns: g.columns as u32,
            }),
            level,
            ordinal,
            row,
            column,
            row_span,
            column_span,
            header,
        }
    }
}

/// One word of a document's text, with the box it was drawn in.
///
/// `block` is the position in the list `extractBlocks` returns under the same
/// options -- the window's numbering, not the document's -- and `line` the
/// 0-based line within that block.
#[napi(object)]
pub struct WordInfo {
    /// The word, without surrounding whitespace.
    pub text: String,
    /// 0-based page index the word was drawn on.
    pub page: u32,
    /// `[x0, y0, x1, y1]` in PDF user space, y up.
    pub bbox: Vec<f64>,
    /// Position of the word's block in the `extractBlocks` answer.
    pub block: u32,
    /// 0-based line within the word's block.
    pub line: u32,
    /// 0-based word within the word's line, in the order the line reads.
    pub word: u32,
    /// Baseline angle in degrees, counter-clockwise from +x; 0 upright.
    pub dir: f64,
    /// Whether the word's span is set bold.
    pub bold: bool,
    /// Whether the word's span is set italic.
    pub italic: bool,
    /// Font size of the word's span, in points.
    pub font_size: f64,
}

impl From<PdfWord> for WordInfo {
    fn from(word: PdfWord) -> Self {
        WordInfo {
            text: word.text,
            page: word.page as u32,
            bbox: word.bbox.to_vec(),
            block: word.block as u32,
            line: word.line as u32,
            word: word.word as u32,
            dir: word.dir,
            bold: word.bold,
            italic: word.italic,
            font_size: word.font_size,
        }
    }
}

/// Result of a text redaction operation.
#[napi(object)]
pub struct RedactionResult {
    /// Number of text matches found.
    pub matches_found: u32,
    /// Number of page areas redacted.
    pub areas_redacted: u32,
    /// Number of pages affected.
    pub pages_affected: u32,
}

/// Signature validation result.
#[napi(object)]
pub struct SignatureResult {
    /// Validation status: "valid", "invalid", or "unknown".
    pub status: String,
    /// Reason for invalid/unknown status.
    pub reason: Option<String>,
    /// Fully qualified field name.
    pub field_name: String,
    /// Signer common name, if available.
    pub signer: Option<String>,
    /// Signing timestamp, if available.
    pub timestamp: Option<String>,
}

/// PDF/A compliance issue.
#[napi(object)]
pub struct ComplianceIssueInfo {
    /// Rule identifier.
    pub rule: String,
    /// Severity: "error", "warning", or "info".
    pub severity: String,
    /// Human-readable description.
    pub message: String,
}

/// PDF/A compliance report.
#[napi(object)]
pub struct ComplianceReportInfo {
    /// Whether the document is compliant.
    pub compliant: bool,
    /// Number of errors.
    pub error_count: u32,
    /// Number of warnings.
    pub warning_count: u32,
    /// All issues found.
    pub issues: Vec<ComplianceIssueInfo>,
}

/// The window a caller asked for, or the whole document when it asked for
/// nothing, plus the figure options `extractMarkdown` reads.
///
/// parsed, so a typo fails the same way on all three, but read by neither.
fn convert_extract_opts(
    opts: &Option<ExtractOpts>,
    operation: &str,
) -> napi::Result<ExtractOptions> {
    let Some(o) = opts else {
        return Ok(ExtractOptions::default());
    };
    let ocr = match o.ocr.as_deref().unwrap_or("never") {
        "never" => OcrMode::Never,
        "auto" => OcrMode::Auto,
        "force" => OcrMode::Force,
        other => {
            return Err(error::unsupported(
                operation,
                format!("unknown ocr option '{other}'; use 'never', 'auto' or 'force'"),
            ));
        }
    };
    if ocr != OcrMode::Never && o.ocr_dpi == Some(0) {
        return Err(error::unsupported(
            operation,
            "ocrDpi must be positive".to_string(),
        ));
    }
    let page_separators = match o
        .page_separators
        .as_deref()
        .unwrap_or("comment")
        .to_lowercase()
        .as_str()
    {
        "comment" => PageSeparators::Comment,
        "none" => PageSeparators::None,
        other => {
            return Err(error::unsupported(
                operation,
                format!("unknown pageSeparators '{other}'; use 'comment' or 'none'"),
            ));
        }
    };
    let quality = match o.quality.as_deref().unwrap_or("fast") {
        "fast" => Quality::Fast,
        "balanced" => Quality::Balanced,
        other => {
            return Err(error::unsupported(
                operation,
                format!("unknown quality '{other}'; use 'fast' or 'balanced'"),
            ));
        }
    };
    // No struct literal: `ExtractOptions` is `#[non_exhaustive]`, so the
    // asked-for fields are assigned onto the default.
    let mut options = ExtractOptions::default();
    options.first_page = o.first_page.unwrap_or(0) as usize;
    options.page_count = o.page_count.map(|count| count as usize);
    options.pages = o
        .pages
        .clone()
        .map(|pages| pages.into_iter().map(|page| page as usize).collect());
    options.ocr = ocr;
    options.ocr_language = o.ocr_language.clone().unwrap_or_else(|| "eng".to_string());
    options.ocr_dpi = o.ocr_dpi.unwrap_or(150);
    options.include_headers = o.include_headers.unwrap_or(false);
    options.include_footers = o.include_footers.unwrap_or(false);
    options.page_separators = page_separators;
    options.code = o.code.unwrap_or_else(|| ExtractOptions::default().code);
    options.quality = quality;
    options.model_dir = o.model_dir.clone().map(std::path::PathBuf::from);
    options.keep_furniture = o.keep_furniture.unwrap_or(false);
    if let Some(dir) = o.table_model_dir.clone() {
        let mut tatr = TatrOptions::default();
        tatr.model_dir = Some(std::path::PathBuf::from(dir));
        options = options.with_tatr(tatr);
    }
    // AND WHATEVER THE ENGINE ADDS NEXT stays at its default rather than
    // left out: this is a binding, and a field it forgets is a feature this
    // language silently does not get. `learned` arrived that way (#599) and
    // broke the build here rather than in a test, which is the lucky version
    // of the same mistake.
    Ok(options)
}

pub(crate) fn convert_render_opts_inner(opts: &Option<RenderOpts>) -> RenderOptions {
    convert_render_opts(opts)
}

fn convert_render_opts(opts: &Option<RenderOpts>) -> RenderOptions {
    let mut ro = RenderOptions::default();
    if let Some(o) = opts {
        if let Some(dpi) = o.dpi {
            ro.dpi = dpi;
        }
        if let Some(ref bg) = o.background {
            if bg.len() >= 4 {
                ro.background = [bg[0] as f32, bg[1] as f32, bg[2] as f32, bg[3] as f32];
            }
        }
        if let Some(w) = o.width {
            ro.width = Some(w as u16);
        }
        if let Some(h) = o.height {
            ro.height = Some(h as u16);
        }
    }
    ro
}

pub(crate) fn rendered_to_result_inner(rp: RenderedPage) -> RenderResult {
    rendered_to_result(rp)
}

fn rendered_to_result(rp: RenderedPage) -> RenderResult {
    RenderResult {
        data: Buffer::from(rp.pixels),
        width: rp.width,
        height: rp.height,
    }
}

fn convert_bookmarks(items: Vec<pdf_engine::BookmarkItem>) -> Vec<BookmarkItem> {
    items
        .into_iter()
        .map(|b| BookmarkItem {
            title: b.title,
            page: b.page.map(|p| p as u32),
            children: convert_bookmarks(b.children),
        })
        .collect()
}

#[napi]
impl PdfDocument {
    /// Open a PDF from a Buffer (synchronous).
    #[napi(factory)]
    pub fn open(data: Buffer) -> Result<PdfDocument> {
        Self::open_bytes(data.to_vec(), "open")
    }

    /// Open a PDF from a Buffer (async — runs on worker thread).
    #[napi(factory)]
    pub async fn open_async(data: Buffer) -> Result<PdfDocument> {
        let bytes: Vec<u8> = data.to_vec();
        let doc = tokio::task::spawn_blocking(move || {
            let pdf = RustDocument::open(bytes.clone())?;
            let lopdf = LopdfDocument::load_mem(&bytes).map_err(|e| e.to_string());
            Ok::<_, pdf_engine::EngineError>((pdf, lopdf))
        })
        .await
        .map_err(error::join("openAsync"))?
        .map_err(error::engine("openAsync"))?;
        let (pdf_doc, lopdf_result) = doc;
        let form_engine = FormEngine::from_pdf(pdf_doc.pdf()).map(Arc::new);
        let (lopdf_doc, doc_load_error) = match lopdf_result {
            Ok(d) => (Some(Arc::new(Mutex::new(d))), None),
            Err(e) => (None, Some(e)),
        };
        Ok(PdfDocument {
            inner: Arc::new(pdf_doc),
            form_engine,
            doc: lopdf_doc,
            doc_load_error,
        })
    }

    /// Open a password-protected PDF.
    #[napi(factory)]
    pub fn open_with_password(data: Buffer, password: String) -> Result<PdfDocument> {
        let bytes: Vec<u8> = data.to_vec();
        let doc = RustDocument::open_with_password(bytes.clone(), &password)
            .map_err(error::engine("openWithPassword"))?;
        let form_engine = FormEngine::from_pdf(doc.pdf()).map(Arc::new);
        let (lopdf_doc, doc_load_error) = match LopdfDocument::load_mem_with_options(
            &bytes,
            lopdf::LoadOptions::with_password(&password),
        ) {
            Ok(d) => (Some(Arc::new(Mutex::new(d))), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Ok(PdfDocument {
            inner: Arc::new(doc),
            form_engine,
            doc: lopdf_doc,
            doc_load_error,
        })
    }

    /// Number of pages in the document.
    #[napi(getter)]
    pub fn page_count(&self) -> u32 {
        self.inner.page_count() as u32
    }

    /// Get document metadata.
    #[napi]
    pub fn info(&self) -> DocumentInfo {
        let i = self.inner.info();
        DocumentInfo {
            title: i.title,
            author: i.author,
            subject: i.subject,
            keywords: i.keywords,
            creator: i.creator,
            producer: i.producer,
        }
    }

    /// Get a page handle (0-based index).
    #[napi]
    pub fn page(&self, index: u32) -> Result<PdfPage> {
        let count = self.inner.page_count();
        if (index as usize) >= count {
            return Err(error::page_out_of_range(
                "page",
                format!("page {index} out of range (document has {count} pages)"),
            ));
        }
        Ok(PdfPage::new(self.inner.clone(), index))
    }

    /// Render a single page to RGBA pixels (synchronous).
    #[napi]
    pub fn render_page(&self, index: u32, options: Option<RenderOpts>) -> Result<RenderResult> {
        let ro = convert_render_opts(&options);
        let rp = self
            .inner
            .render_page(index as usize, &ro)
            .map_err(error::engine("renderPage"))?;
        Ok(rendered_to_result(rp))
    }

    /// Render a single page to RGBA pixels (async — worker thread).
    #[napi]
    pub async fn render_page_async(
        &self,
        index: u32,
        options: Option<RenderOpts>,
    ) -> Result<RenderResult> {
        let inner = self.inner.clone();
        let ro = convert_render_opts(&options);
        let rp = tokio::task::spawn_blocking(move || inner.render_page(index as usize, &ro))
            .await
            .map_err(error::join("renderPageAsync"))?
            .map_err(error::engine("renderPageAsync"))?;
        Ok(rendered_to_result(rp))
    }

    /// Generate a thumbnail for a page (async).
    #[napi]
    pub async fn thumbnail(&self, index: u32, max_dimension: Option<u32>) -> Result<RenderResult> {
        let inner = self.inner.clone();
        let opts = ThumbnailOptions {
            max_dimension: max_dimension.unwrap_or(256),
        };
        let rp = tokio::task::spawn_blocking(move || inner.thumbnail(index as usize, &opts))
            .await
            .map_err(error::join("thumbnail"))?
            .map_err(error::engine("thumbnail"))?;
        Ok(rendered_to_result(rp))
    }

    /// Extract text from a page (synchronous).
    #[napi]
    pub fn extract_text(&self, index: u32) -> Result<String> {
        self.inner
            .extract_text(index as usize)
            .map_err(error::engine("extractText"))
    }

    /// Extract text from a page (async — worker thread).
    #[napi]
    pub async fn extract_text_async(&self, index: u32) -> Result<String> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.extract_text(index as usize))
            .await
            .map_err(error::join("extractTextAsync"))?
            .map_err(error::engine("extractTextAsync"))
    }

    /// Search for text across all pages. Returns 0-based page indices.
    #[napi]
    pub fn search_text(&self, query: String) -> Vec<u32> {
        self.inner
            .search_text(&query)
            .into_iter()
            .map(|i| i as u32)
            .collect()
    }

    /// Get document bookmarks / outline.
    #[napi]
    pub fn bookmarks(&self) -> Vec<BookmarkItem> {
        convert_bookmarks(self.inner.bookmarks())
    }

    /// Get page geometry (dimensions, rotation).
    #[napi]
    pub fn page_geometry(&self, index: u32) -> Result<PageGeometry> {
        let g = self
            .inner
            .page_geometry(index as usize)
            .map_err(error::engine("pageGeometry"))?;
        let (w, h) = g.effective_dimensions();
        Ok(PageGeometry {
            width: w,
            height: h,
            rotation: g.rotation.degrees(),
        })
    }

    /// Render all pages in parallel (async).
    #[napi]
    pub async fn render_all(&self, options: Option<RenderOpts>) -> Result<Vec<RenderResult>> {
        let inner = self.inner.clone();
        let ro = convert_render_opts(&options);
        let results = tokio::task::spawn_blocking(move || inner.render_all(&ro))
            .await
            .map_err(error::join("renderAll"))?;
        Ok(results.into_iter().map(rendered_to_result).collect())
    }

    /// Extract structured text blocks from a page.
    #[napi]
    pub fn extract_text_blocks(&self, index: u32) -> Result<Vec<TextBlockInfo>> {
        let blocks = self
            .inner
            .extract_text_blocks(index as usize)
            .map_err(error::engine("extractTextBlocks"))?;
        Ok(blocks
            .into_iter()
            .map(|b| TextBlockInfo {
                text: b.text(),
                spans: b.spans.into_iter().map(TextSpanInfo::from).collect(),
            })
            .collect())
    }

    /// The document as markdown, in reading order (#573).
    ///
    /// Preview: output quality is still improving and the API may change in a minor release.
    ///
    /// Headings become `#` to `###`, list items `-` and `1.`, a table a pipe
    /// table or an HTML one where markdown cannot hold its shape, and a change
    /// of page an HTML comment. The running heads a document repeats are not in
    /// it unless `includeHeaders` or `includeFooters` is set; they are in
    /// {@link extractBlocks}.
    ///
    /// `options.images` is `"placeholder"` (default: an `<!-- image -->` marker
    /// where each figure stands), `"none"`, `"embed"` or `"write"`:
    /// `"embed"` links each figure as a base64 data URI, `"write"` saves each
    /// figure under `imageDir` as `<stem>-p<page>-<n>.<ext>`. See
    /// `tableOutput`, `imageDpi`, `imageSizeLimit`, `includeHeaders`,
    ///
    /// @experimental
    #[napi]
    pub fn extract_markdown(&self, options: Option<ExtractOpts>) -> Result<String> {
        self.inner
            .extract_markdown(&convert_extract_opts(&options, "extractMarkdown")?)
            .map_err(error::engine("extractMarkdown"))
    }

    /// The document as a Docling `DoclingDocument`, as JSON (#629).
    ///
    /// Preview: output quality is still improving and the API may change in a minor release.
    ///
    /// The schema version the engine pins, which docling-core's
    /// `DoclingDocument.model_validate_json` loads as it is. `name` is
    /// `DoclingDocument.name`, which Docling fills with the file's stem;
    /// absent means `"document"`.
    ///
    /// @experimental
    #[napi]
    pub fn extract_docling(
        &self,
        options: Option<ExtractOpts>,
        name: Option<String>,
    ) -> Result<String> {
        self.inner
            .extract_docling(
                name.as_deref().unwrap_or("document"),
                &convert_extract_opts(&options, "extractDocling")?,
            )
            .map_err(error::engine("extractDocling"))
    }

    /// The document's blocks, in reading order (#573).
    ///
    /// Preview: output quality is still improving and the API may change in a minor release.
    ///
    /// One {@link BlockInfo} per paragraph, heading, list item, table cell and
    /// running head, each with the page and the box it was drawn in.
    ///
    /// @experimental
    #[napi]
    pub fn extract_blocks(&self, options: Option<ExtractOpts>) -> Result<Vec<BlockInfo>> {
        Ok(self
            .inner
            .extract_blocks(&convert_extract_opts(&options, "extractBlocks")?)
            .map_err(error::engine("extractBlocks"))?
            .into_iter()
            .map(BlockInfo::from)
            .collect())
    }

    /// Every word of the document, with the box it was drawn in.
    ///
    /// One {@link WordInfo} per word, in reading order, block by block and
    /// line by line -- what a caller cites, highlights or grounds with. Built
    /// from the blocks {@link extractBlocks} returns under the same options,
    /// so `blocks[word.block]` is the block the word came from.
    ///
    /// Preview: output quality is still improving and the API may change in a minor release.
    #[napi]
    pub fn extract_words(&self, options: Option<ExtractOpts>) -> Result<Vec<WordInfo>> {
        Ok(self
            .inner
            .extract_words(&convert_extract_opts(&options, "extractWords")?)
            .map_err(error::engine("extractWords"))?
            .into_iter()
            .map(WordInfo::from)
            .collect())
    }

    /// Get all form fields in the document.
    #[napi]
    pub fn get_form_fields(&self) -> Vec<FormFieldInfo> {
        match &self.form_engine {
            Some(fe) => fe.fields(),
            None => Vec::new(),
        }
    }

    /// Get all form fields in the document.
    ///
    /// @deprecated Use {@link getFormFields} instead — the canonical
    /// cross-language name (snake_case `get_form_fields` in Rust/Python).
    /// Kept for backward compatibility; will be removed in 1.0.0.
    #[napi]
    pub fn form_fields(&self) -> Vec<FormFieldInfo> {
        self.get_form_fields()
    }

    /// Get the value of a form field by its fully qualified name.
    #[napi]
    pub fn get_field_value(&self, name: String) -> Option<String> {
        self.form_engine.as_ref()?.get_value(&name)
    }

    /// Set the value of a form field by its fully qualified name.
    ///
    /// Routes through [`pdf_forms::apply_field_value`] — the single SDK
    /// writeback chain — so `/V` encoding (ASCII literal else UTF-16BE+BOM),
    /// per-widget `/AS` sync, and `/AP` regeneration stay consistent, and
    /// read-only fields are rejected. The change is persisted to the
    /// document — a subsequent `save()` will write the updated value.
    #[napi]
    pub fn set_form_field(&mut self, name: String, value: String) -> Result<()> {
        self.set_form_field_as(&name, &value, "setFormField")
    }

    /// Set the value of a form field by its fully qualified name.
    ///
    /// @deprecated Use {@link setFormField} instead — the canonical
    /// cross-language name (snake_case `set_form_field` in Rust/Python).
    /// Kept for backward compatibility; will be removed in 1.0.0.
    #[napi]
    pub fn set_field_value(&mut self, name: String, value: String) -> Result<()> {
        self.set_form_field_as(&name, &value, "setFieldValue")
    }

    /// Set multiple selected values on a multi-select list box.
    ///
    /// Routes through [`pdf_forms::apply_choice_multi`]: writes `/V` as an
    /// array of text strings and rebuilds `/I` (the sorted selected-index
    /// cache) to match what Adobe Acrobat produces. Pass an empty array to
    /// clear the selection. The field must be a multi-select list box
    /// (`/Ff` MultiSelect flag); for non-editable list boxes every value must
    /// be one of the field's `/Opt` options.
    #[napi]
    pub fn set_multi_select(&mut self, name: String, values: Vec<String>) -> Result<()> {
        self.with_doc_mut("setMultiSelect", |doc| {
            apply_choice_multi(doc, &name, &values)
                .map(|_| ())
                .map_err(|e| {
                    error::writeback(
                        "setMultiSelect",
                        format!("setMultiSelect '{name}': {e}"),
                        &e,
                    )
                })
        })?;
        self.rebuild_inner("setMultiSelect")
    }

    /// Get annotations on a specific page (0-based index).
    #[napi]
    pub fn annotations(&self, page_index: u32) -> Result<Vec<AnnotationInfo>> {
        annotation::page_annotations(self.inner.pdf(), page_index as usize, "annotations")
    }

    /// Validate all digital signatures in the document.
    #[napi]
    pub fn validate_signatures(&self) -> Vec<SignatureResult> {
        let results = pdf_sign::validate_signatures(self.inner.pdf());
        results
            .into_iter()
            .map(|r| {
                let (status, reason) = match r.status {
                    pdf_sign::ValidationStatus::Valid => ("valid".into(), None),
                    pdf_sign::ValidationStatus::Invalid(msg) => ("invalid".into(), Some(msg)),
                    pdf_sign::ValidationStatus::Unknown(msg) => ("unknown".into(), Some(msg)),
                };
                SignatureResult {
                    status,
                    reason,
                    field_name: r.field_name,
                    signer: r.signer,
                    timestamp: r.timestamp,
                }
            })
            .collect()
    }

    /// Save the document to a file path.
    ///
    /// Writes the current (possibly modified) document to disk. Any changes
    /// from `setFormField`, `addAnnotation`, or `redactText` are included.
    #[napi]
    pub fn save(&self, path: String) -> Result<()> {
        if let Some(arc) = &self.doc {
            let mut doc = arc.lock().unwrap();
            doc.save(&path)
                .map(|_| ())
                .map_err(|e| error::io("save", format!("cannot save to '{path}': {e}"), &e))
        } else {
            // Fallback: write original bytes unchanged.
            let bytes: &[u8] = self.inner.pdf().data().as_ref();
            std::fs::write(&path, bytes)
                .map_err(|e| error::io("save", format!("cannot write '{path}': {e}"), &e))
        }
    }

    /// Add an annotation to a page (0-based index).
    ///
    /// `annot_type` must be one of: `"highlight"`, `"freetext"`, `"note"`,
    /// `"underline"`, `"strikeout"`, `"squiggly"`.
    ///
    /// `rect` is `[x0, y0, x1, y1]` in PDF user-space coordinates.
    /// For `"freetext"`, `content` becomes the visible text.
    #[napi]
    pub fn add_annotation(
        &mut self,
        page: u32,
        annot_type: String,
        rect: Vec<f64>,
        content: Option<String>,
    ) -> Result<()> {
        if rect.len() < 4 {
            return Err(error::unsupported(
                "addAnnotation",
                "rect must have 4 elements [x0,y0,x1,y1]",
            ));
        }
        let ar = pdf_annot::builder::AnnotRect::new(rect[0], rect[1], rect[2], rect[3]);
        self.with_doc_mut("addAnnotation", |doc| {
            let builder = match annot_type.to_lowercase().as_str() {
                "highlight" => pdf_annot::builder::AnnotationBuilder::highlight(ar),
                "underline" => pdf_annot::builder::AnnotationBuilder::underline(ar),
                "strikeout" => pdf_annot::builder::AnnotationBuilder::strikeout(ar),
                "squiggly" => pdf_annot::builder::AnnotationBuilder::squiggly(ar),
                "freetext" => {
                    let text = content.as_deref().unwrap_or("");
                    pdf_annot::builder::AnnotationBuilder::free_text(ar, text, 12.0)
                }
                "note" => {
                    pdf_annot::builder::AnnotationBuilder::sticky_note(
                        ar,
                        pdf_annot::builder::TextIcon::Note,
                    )
                }
                other => {
                    return Err(error::unsupported("addAnnotation", format!(
                        "unknown annotation type '{other}'; expected: highlight, freetext, note, underline, strikeout, squiggly"
                    )))
                }
            };
            let builder = if let Some(c) = &content {
                builder.contents(c.clone())
            } else {
                builder
            };
            // build() adds the annotation object to the document
            let annot_id = builder
                .build(doc)
                .map_err(|e| error::annot("addAnnotation", format!("build annotation: {e}"), e))?;
            // page is 0-based in our API; lopdf uses 1-based
            pdf_annot::builder::add_annotation_to_page(doc, page + 1, annot_id)
                .map_err(|e| {
                    error::annot("addAnnotation", format!("add annotation to page: {e}"), e)
                })
        })?;
        self.rebuild_inner("addAnnotation")
    }

    /// Redact all occurrences of `search_term` on a page (0-based index).
    ///
    /// Pass `page = u32::MAX` (or omit via a wrapper) to redact across all pages.
    /// The document is modified in-place; call `save()` to persist.
    ///
    /// Returns a summary of what was redacted.
    #[napi]
    pub fn redact_text(
        &mut self,
        search_term: String,
        page: Option<u32>,
    ) -> Result<RedactionResult> {
        let result = self.with_doc_mut("redactText", |doc| {
            let mut opts = pdf_redact::RedactSearchOptions::exact(&search_term);
            if let Some(p) = page {
                opts = opts.pages(vec![p + 1]); // convert to 1-based
            }
            let report = pdf_redact::search_and_redact(doc, &search_term, &opts)
                .map_err(|e| error::redact("redactText", format!("redact failed: {e}"), e))?;
            Ok(RedactionResult {
                matches_found: report.matches_found as u32,
                areas_redacted: report.areas_redacted as u32,
                pages_affected: report.pages_affected as u32,
            })
        })?;
        self.rebuild_inner("redactText")?;
        Ok(result)
    }

    /// Encrypt the document and write it to `output_path`.
    ///
    /// Uses AES-256 with `password` as both the user and owner password.
    /// The in-memory document is not modified — encryption is applied to a
    /// temporary clone and only the written file is encrypted.
    #[napi]
    pub fn encrypt(&self, output_path: String, password: String) -> Result<()> {
        // Clone before encrypting so the shared document stays in plaintext state.
        let mut doc_clone = {
            let arc = self.writable("encrypt")?;
            arc.lock().unwrap().clone()
        };
        let config = pdf_manip::encrypt::EncryptConfig {
            user_password: password.as_bytes().to_vec(),
            owner_password: password.as_bytes().to_vec(),
            ..Default::default() // AES-256, all permissions
        };
        let file = std::fs::File::create(&output_path)
            .map_err(|e| error::io("encrypt", format!("cannot create '{output_path}': {e}"), &e))?;
        let mut writer = std::io::BufWriter::new(file);
        pdf_manip::encrypt::encrypt_and_save(&mut doc_clone, &config, &mut writer)
            .map_err(|e| error::manip("encrypt", format!("encrypt failed: {e}"), e))
    }

    /// Remove encryption and write the decrypted document to `output_path`.
    ///
    /// Only useful if the document was opened with `openWithPassword`.
    /// After this call the saved file has no password protection.
    #[napi]
    pub fn decrypt(&self, output_path: String) -> Result<()> {
        self.with_doc_mut("decrypt", |doc| {
            pdf_manip::encrypt::remove_encryption(doc);
            doc.save(&output_path)
                .map(|_| ())
                .map_err(|e| error::io("decrypt", format!("cannot save '{output_path}': {e}"), &e))
        })
    }

    /// Validate the document against a PDF/A conformance level.
    ///
    /// Level is specified as a string: "1a", "1b", "2a", "2b", "2u", "3a", "3b", "3u".
    #[napi]
    pub fn validate_pdfa(&self, level: String) -> Result<ComplianceReportInfo> {
        let pdfa_level = parse_pdfa_level(&level, "validatePdfa")?;
        let report = pdf_compliance::validate_pdfa(self.inner.pdf(), pdfa_level);
        Ok(compliance_to_info(report))
    }

    /// Convert the document to a Word `.docx` package.
    ///
    /// Requires a Business licence or higher. Without one the call throws,
    /// and the message points at the free 30-day evaluation key rather than
    /// the price list.
    #[napi]
    pub fn to_docx(&self) -> Result<Buffer> {
        self.office_export("toDocx", |b| {
            pdf_docx::convert_pdf_bytes_to_docx(b).map_err(|e| e.to_string())
        })
    }

    /// Convert the document to an Excel `.xlsx` workbook. See `toDocx`.
    #[napi]
    pub fn to_xlsx(&self) -> Result<Buffer> {
        self.office_export("toXlsx", |b| {
            pdf_xlsx::convert_pdf_bytes_to_xlsx(b).map_err(|e| e.to_string())
        })
    }

    /// Convert the document to a PowerPoint `.pptx` deck, one slide per page.
    /// See `toDocx`.
    #[napi]
    pub fn to_pptx(&self) -> Result<Buffer> {
        self.office_export("toPptx", |b| {
            pdf_pptx::convert_pdf_bytes_to_pptx(b).map_err(|e| e.to_string())
        })
    }

    /// List the images stored on one page (0-based index).
    ///
    /// One entry per image XObject in the page's resources, in resource
    /// order. Past the last page the error carries
    /// `E-PAGE-OUT-OF-RANGE`.
    #[napi]
    pub fn page_images(&self, index: u32) -> Result<Vec<PageImageInfo>> {
        let facade = self.facade("pageImages")?;
        let images = facade
            .page_images(index as usize + 1)
            .map_err(error::facade("pageImages"))?;
        Ok(images.into_iter().map(page_image_to_info).collect())
    }

    /// Extract one stored image: header facts plus encoded bytes.
    ///
    /// JPEG payloads pass through untouched; everything else arrives
    /// decoded and re-encoded as PNG.
    #[napi]
    pub fn extract_image_data(&self, image_id: u32) -> Result<ExtractedImageInfo> {
        let facade = self.facade("extractImageData")?;
        let image = facade
            .extract_image_data(pdfluent::ImageId::from(image_id))
            .map_err(error::facade("extractImageData"))?;
        Ok(ExtractedImageInfo {
            width: image.width,
            height: image.height,
            bits_per_component: u32::from(image.bits_per_component),
            components: u32::from(image.components),
            color_space: image.color_space.name().to_string(),
            color_space_name: image.color_space_name,
            encoding: image.encoding,
            data: Buffer::from(image.data),
            masked: image.masked,
            xres: image.xres,
            yres: image.yres,
        })
    }

    /// Decode one stored image to raw samples, top row first.
    ///
    /// Palettes and stencils are expanded.
    #[napi]
    pub fn decode_image_pixels(&self, image_id: u32) -> Result<DecodedImageInfo> {
        let facade = self.facade("decodeImagePixels")?;
        let image = facade
            .decode_image_pixels(pdfluent::ImageId::from(image_id))
            .map_err(error::facade("decodeImagePixels"))?;
        Ok(DecodedImageInfo {
            width: image.width,
            height: image.height,
            components: u32::from(image.components),
            color_space: image.color_space.name().to_string(),
            pixels: Buffer::from(image.pixels),
        })
    }
}

/// One listed image as the `pageImages` entry.
fn page_image_to_info(image: pdfluent::ImageInfo) -> PageImageInfo {
    PageImageInfo {
        id: image.id.as_u32(),
        width: image.width,
        height: image.height,
        bits_per_component: u32::from(image.bits_per_component),
        components: u32::from(image.components),
        color_space: image.color_space.name().to_string(),
        color_space_name: image.color_space_name,
        filter: image.filter,
        name: image.name,
        has_mask: image.has_mask,
    }
}

impl PdfDocument {
    /// The plain facade over the engine's source bytes.
    ///
    /// The plain image calls route through the facade so this binding
    /// shares its implementation and error codes; `operation` names the
    /// JavaScript call for the error.
    fn facade(&self, operation: &str) -> Result<pdfluent::PdfDocument> {
        let raw = self.inner.pdf().data().as_ref().to_vec();
        pdfluent::PdfDocument::from_bytes(&raw).map_err(error::facade(operation))
    }

    /// Shared body for the three Office exports.
    fn office_export(
        &self,
        operation: &str,
        convert: fn(&[u8]) -> std::result::Result<Vec<u8>, String>,
    ) -> Result<Buffer> {
        let raw = self.inner.pdf().data().as_ref().to_vec();
        convert(&raw)
            .map(Buffer::from)
            .map_err(|message| error::unsupported(operation, message))
    }
}

pub(crate) fn parse_pdfa_level(s: &str, operation: &str) -> Result<pdf_compliance::PdfALevel> {
    match s.to_lowercase().as_str() {
        "1a" => Ok(pdf_compliance::PdfALevel::A1a),
        "1b" => Ok(pdf_compliance::PdfALevel::A1b),
        "2a" => Ok(pdf_compliance::PdfALevel::A2a),
        "2b" => Ok(pdf_compliance::PdfALevel::A2b),
        "2u" => Ok(pdf_compliance::PdfALevel::A2u),
        "3a" => Ok(pdf_compliance::PdfALevel::A3a),
        "3b" => Ok(pdf_compliance::PdfALevel::A3b),
        "3u" => Ok(pdf_compliance::PdfALevel::A3u),
        _ => Err(error::unsupported(
            operation,
            format!("unknown PDF/A level '{s}'; expected one of: 1a, 1b, 2a, 2b, 2u, 3a, 3b, 3u"),
        )),
    }
}

pub(crate) fn compliance_to_info(report: pdf_compliance::ComplianceReport) -> ComplianceReportInfo {
    ComplianceReportInfo {
        compliant: report.is_compliant(),
        error_count: report.error_count() as u32,
        warning_count: report.warning_count() as u32,
        issues: report
            .issues
            .into_iter()
            .map(|i| ComplianceIssueInfo {
                rule: i.rule,
                severity: match i.severity {
                    pdf_compliance::Severity::Error => "error".into(),
                    pdf_compliance::Severity::Warning => "warning".into(),
                    pdf_compliance::Severity::Info => "info".into(),
                },
                message: i.message,
            })
            .collect(),
    }
}

#[cfg(test)]
mod writeback_dispatch_tests {
    //! Pin the new writeback behavior of `setFormField`'s dispatch helper:
    //! correct /V encoding (ASCII literal else UTF-16BE+BOM), /V-as-Name +
    //! /AS sync for buttons, and read-only rejection — replacing the old
    //! raw `string_literal(value.as_bytes())` write (mojibake, no /AS).

    use super::apply_string_value;
    use lopdf::{dictionary, Document, Object, Stream};
    use pdf_forms::WritebackError;

    /// Minimal indirect-AcroForm document: text field, read-only text
    /// field, and a checkbox whose on-state (`On1`) lives on a kid widget.
    fn form_doc() -> Document {
        let mut doc = Document::with_version("1.4");
        let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
        let pages_id = doc.new_object_id();

        let text_field = doc.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("first_name"),
            "V" => Object::string_literal(""),
        });
        // /Ff bit 1 = ReadOnly.
        let readonly_field = doc.add_object(dictionary! {
            "FT" => "Tx",
            "Ff" => 1i64,
            "T" => Object::string_literal("locked"),
            "V" => Object::string_literal("frozen"),
        });
        let checkbox_kid = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => vec![100.into(), 700.into(), 115.into(), 715.into()],
            "AP" => dictionary! {
                "N" => dictionary! {
                    "Off" => Object::Null,
                    "On1" => Object::Null,
                },
            },
        });
        let checkbox_field = doc.add_object(dictionary! {
            "FT" => "Btn",
            "T" => Object::string_literal("subscribe"),
            "V" => Object::Name(b"Off".to_vec()),
            "Kids" => vec![checkbox_kid.into()],
        });

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {},
            "Annots" => vec![checkbox_kid.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let acroform_id = doc.add_object(dictionary! {
            "Fields" => vec![
                text_field.into(),
                readonly_field.into(),
                checkbox_field.into(),
            ],
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    fn field_v(doc: &Document, name: &str) -> Object {
        doc.objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .find(|d| {
                d.get(b"T")
                    .ok()
                    .and_then(|t| lopdf::decode_text_string(t).ok())
                    .as_deref()
                    == Some(name)
            })
            .and_then(|d| d.get(b"V").ok())
            .cloned()
            .unwrap_or_else(|| panic!("field '{name}' has no /V"))
    }

    #[test]
    fn ascii_text_stays_literal() {
        let mut doc = form_doc();
        apply_string_value(&mut doc, "first_name", "Jane").expect("apply ASCII");
        match field_v(&doc, "first_name") {
            Object::String(v, _) => assert_eq!(v, b"Jane"),
            other => panic!("expected /V string, got {other:?}"),
        }
    }

    #[test]
    fn non_ascii_text_writes_utf16be_bom() {
        let mut doc = form_doc();
        apply_string_value(&mut doc, "first_name", "Café").expect("apply non-ASCII");
        match field_v(&doc, "first_name") {
            Object::String(v, _) => assert!(
                v.starts_with(&[0xFE, 0xFF]),
                "non-ASCII /V must be UTF-16BE with BOM, got {v:02X?}"
            ),
            other => panic!("expected /V string, got {other:?}"),
        }
    }

    #[test]
    fn checkbox_dispatch_sets_name_value_and_widget_as() {
        let mut doc = form_doc();
        let outcome = apply_string_value(&mut doc, "subscribe", "true").expect("apply checkbox");
        assert_eq!(field_v(&doc, "subscribe"), Object::Name(b"On1".to_vec()));
        assert!(
            outcome.appearance_states_set >= 1,
            "kid widget /AS must be synced"
        );
        let widget_synced = doc
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .filter(|d| d.has(b"AP") && !d.has(b"T"))
            .any(|d| matches!(d.get(b"AS"), Ok(Object::Name(n)) if n == b"On1"));
        assert!(widget_synced, "kid widget /AS must be the on-state On1");
    }

    #[test]
    fn checkbox_dispatch_bool_ish_off_strings() {
        let mut doc = form_doc();
        apply_string_value(&mut doc, "subscribe", "true").expect("check");
        apply_string_value(&mut doc, "subscribe", "false").expect("uncheck");
        assert_eq!(field_v(&doc, "subscribe"), Object::Name(b"Off".to_vec()));
    }

    #[test]
    fn readonly_field_is_rejected() {
        let mut doc = form_doc();
        let err = apply_string_value(&mut doc, "locked", "new value")
            .expect_err("read-only field must be rejected");
        assert!(
            matches!(err, WritebackError::ReadOnly(ref n) if n == "locked"),
            "expected ReadOnly error, got {err:?}"
        );
        match field_v(&doc, "locked") {
            Object::String(v, _) => assert_eq!(v, b"frozen", "value must be unchanged"),
            other => panic!("expected /V string, got {other:?}"),
        }
    }
}
