//! Browser tools on PDF bytes, added for 1.1.0 (#613): password protection
//! and removal, a full text watermark, image extraction and signing.
//!
//! Each export is a thin wrapper around a plain Rust function that returns a
//! [`ToolError`] instead of a `JsValue`, so the logic runs under a native
//! `cargo test`; a `JsValue` cannot even be constructed off wasm32.
//!
//! Password removal and encryption are stateless functions on bytes, not
//! `PdfDoc` methods, for a reason: `PdfDoc.open` parses with pdf-syntax, which
//! has no password support and refuses a PDF with a user password outright. So
//! the flow is `decryptPdf(bytes, pw)` and then `PdfDoc.open` on the result.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::pdfluent_error::{code, legacy_code, pdfluent_error};
use crate::PdfDoc;

// ---------- errors -----------------------------------------------------------

/// A typed failure from one of these tools, convertible to a `PdfluentError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    /// C8 catalogue code (`E-<CATEGORY>-<SPECIFIC>`).
    pub code: &'static str,
    /// SCREAMING_SNAKE_CASE legacy code.
    pub legacy: &'static str,
    /// Human-readable message.
    pub message: String,
}

impl ToolError {
    fn new(code: &'static str, legacy: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            legacy,
            message: message.into(),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new(
            code::WASM_INVALID_ARGUMENT,
            legacy_code::INVALID_ARGUMENT,
            message,
        )
    }

    fn bad_pdf(e: impl std::fmt::Display) -> Self {
        Self::new(
            code::PARSE_INVALID_PDF,
            legacy_code::INVALID_PDF,
            format!("load failed: {e}"),
        )
    }

    fn password(message: impl Into<String>) -> Self {
        Self::new(
            code::SECURITY_DECRYPTION_FAILED,
            legacy_code::DECRYPTION_FAILED,
            message,
        )
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(code::INTERNAL, legacy_code::OPERATION_FAILED, message)
    }

    fn into_js(self, operation: &str) -> JsValue {
        pdfluent_error(operation, self.code, self.legacy, &self.message, "")
    }
}

fn save(doc: &mut lopdf::Document) -> Result<Vec<u8>, ToolError> {
    let mut buf = Vec::new();
    doc.save_to(&mut buf).map_err(|e| {
        ToolError::new(
            code::WASM_SAVE_FAILED,
            legacy_code::SAVE_FAILED,
            format!("save failed: {e}"),
        )
    })?;
    Ok(buf)
}

/// Parse an options object that arrived from JavaScript.
///
/// `JSON.stringify` then serde, rather than a hand-written walk over the
/// object: it gives one precise message for a mistyped or unknown field.
fn parse_options<T: serde::de::DeserializeOwned + Default>(
    value: &JsValue,
    what: &str,
) -> Result<T, ToolError> {
    if value.is_undefined() || value.is_null() {
        return Ok(T::default());
    }
    let json: String = js_sys::JSON::stringify(value)
        .map_err(|_| ToolError::invalid(format!("{what}: not a plain object")))?
        .into();
    serde_json::from_str(&json).map_err(|e| ToolError::invalid(format!("{what}: {e}")))
}

// ---------- #614 password protect / remove -----------------------------------

#[wasm_bindgen(typescript_custom_section)]
const TS_ENCRYPT: &'static str = r#"
/** What a user who opens the file with the user password may do. Every flag defaults to `true`. */
export interface EncryptPermissions {
  print?: boolean;
  printHighQuality?: boolean;
  modify?: boolean;
  copy?: boolean;
  annotate?: boolean;
  fillForms?: boolean;
  accessibility?: boolean;
  assemble?: boolean;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// `EncryptPermissions` in TypeScript.
    #[wasm_bindgen(typescript_type = "EncryptPermissions")]
    pub type EncryptPermissionsJs;
}

/// Permission flags for [`encrypt_bytes`]. `None` means allowed.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EncryptPermissions {
    /// Print (low quality unless `print_high_quality`).
    pub print: Option<bool>,
    /// Print at full quality.
    pub print_high_quality: Option<bool>,
    /// Change the page content.
    pub modify: Option<bool>,
    /// Copy text and images.
    pub copy: Option<bool>,
    /// Add or change annotations.
    pub annotate: Option<bool>,
    /// Fill in form fields.
    pub fill_forms: Option<bool>,
    /// Extract text for accessibility tools.
    pub accessibility: Option<bool>,
    /// Insert, rotate or delete pages.
    pub assemble: Option<bool>,
}

impl EncryptPermissions {
    fn to_manip(&self) -> pdf_manip::encrypt::Permissions {
        let allow = |f: Option<bool>| f.unwrap_or(true);
        pdf_manip::encrypt::Permissions {
            print: allow(self.print),
            modify_contents: allow(self.modify),
            extract_content: allow(self.copy),
            modify_annotations: allow(self.annotate),
            fill_forms: allow(self.fill_forms),
            extract_for_accessibility: allow(self.accessibility),
            assemble_document: allow(self.assemble),
            print_high_quality: allow(self.print_high_quality),
        }
    }
}

/// Encrypt a PDF with AES-256. An empty `owner_password` means the user
/// password is the owner password too.
pub fn encrypt_bytes(
    bytes: &[u8],
    user_password: &str,
    owner_password: &str,
    permissions: &EncryptPermissions,
) -> Result<Vec<u8>, ToolError> {
    if user_password.is_empty() && owner_password.is_empty() {
        return Err(ToolError::invalid(
            "encryptPdf: set a user password, an owner password, or both",
        ));
    }
    let mut doc = lopdf::Document::load_mem(bytes).map_err(ToolError::bad_pdf)?;
    // Refuse to re-encrypt. A file whose user password is empty loads already
    // decrypted, so re-encrypting it would replace the owner's restrictions
    // with the caller's -- lifting them without the owner password.
    if doc.encryption_state.is_some() || doc.trailer.has(b"Encrypt") {
        return Err(ToolError::invalid(
            "encryptPdf: the PDF is already encrypted; remove its password with decryptPdf first",
        ));
    }
    let owner = if owner_password.is_empty() {
        user_password
    } else {
        owner_password
    };
    let config = pdf_manip::encrypt::EncryptConfig {
        user_password: user_password.as_bytes().to_vec(),
        owner_password: owner.as_bytes().to_vec(),
        algorithm: pdf_manip::encrypt::EncryptionAlgorithm::Aes256,
        permissions: permissions.to_manip(),
    };
    let mut out = Vec::new();
    pdf_manip::encrypt::encrypt_and_save(&mut doc, &config, &mut out)
        .map_err(|e| ToolError::internal(format!("encryptPdf failed: {e}")))?;
    Ok(out)
}

/// Remove the password from a PDF, given a password that entitles the caller
/// to do so.
///
/// The owner password always does. The user password does only when the file
/// carries no permission restrictions: lifting restrictions is what the owner
/// password is for, and a file that opens without any password (empty user
/// password) therefore needs its owner password here. This removes a password
/// the caller knows; it never gets round one.
pub fn decrypt_bytes(bytes: &[u8], password: &str) -> Result<Vec<u8>, ToolError> {
    let mut doc =
        lopdf::Document::load_mem_with_options(bytes, lopdf::LoadOptions::with_password(password))
            .map_err(|e| match e {
                lopdf::Error::InvalidPassword => ToolError::password("wrong password"),
                other => ToolError::bad_pdf(other),
            })?;
    let Some(state) = doc.encryption_state.clone() else {
        return Err(ToolError::invalid(
            "decryptPdf: the PDF is not password-protected",
        ));
    };

    // The reader has already decrypted the objects and dropped /Encrypt, with
    // whichever password worked -- including the empty one it always tries
    // first. Rebuild the security handler on its own to learn which of the
    // two passwords `password` actually is.
    let mut handler = lopdf::Document::with_version("1.7");
    let encrypt = state
        .encode()
        .map_err(|e| ToolError::internal(format!("decryptPdf: {e}")))?;
    let encrypt_id = handler.add_object(lopdf::Object::Dictionary(encrypt));
    handler.trailer.set("Encrypt", encrypt_id);
    if let Ok(id) = doc.trailer.get(b"ID") {
        handler.trailer.set("ID", id.clone());
    }
    let is_owner = handler.authenticate_owner_password(password).is_ok();
    let is_user = !password.is_empty() && handler.authenticate_user_password(password).is_ok();
    let restricted = {
        let p = pdf_manip::encrypt::Permissions::from_p_value(state.permissions().bits() as i32);
        !(p.print
            && p.modify_contents
            && p.extract_content
            && p.modify_annotations
            && p.fill_forms
            && p.extract_for_accessibility
            && p.assemble_document
            && p.print_high_quality)
    };
    match (is_owner, is_user, restricted) {
        (true, _, _) | (false, true, false) => {}
        (false, true, true) => {
            return Err(ToolError::password(
                "this PDF has permission restrictions; removing them needs the owner password",
            ))
        }
        (false, false, _) if password.is_empty() => {
            return Err(ToolError::password(
                "this PDF opens without a password but has an owner password; \
                 removing its restrictions needs that owner password",
            ))
        }
        (false, false, _) => return Err(ToolError::password("wrong password")),
    }

    doc.encryption_state = None;
    doc.trailer.remove(b"Encrypt");
    save(&mut doc)
}

/// Encrypt a PDF with AES-256 and return the encrypted bytes.
///
/// `userPassword` is needed to open the file (may be empty: then anyone can
/// open it, and the permissions apply); `ownerPassword` lifts the permissions
/// (empty: the user password serves as both). Refuses a PDF that is already
/// encrypted. Throws `PdfluentError` with `E-WASM-INVALID-ARGUMENT` or
/// `E-PARSE-INVALID-PDF`.
#[wasm_bindgen(js_name = "encryptPdf")]
pub fn encrypt_pdf(
    bytes: &[u8],
    user_password: &str,
    owner_password: &str,
    permissions: Option<EncryptPermissionsJs>,
) -> Result<Vec<u8>, JsValue> {
    let op = "encryptPdf";
    let perms: EncryptPermissions = match permissions {
        Some(p) => parse_options(&p.into(), "permissions").map_err(|e| e.into_js(op))?,
        None => EncryptPermissions::default(),
    };
    encrypt_bytes(bytes, user_password, owner_password, &perms).map_err(|e| e.into_js(op))
}

/// Remove the password from a PDF whose password you know, and return the
/// unprotected bytes; open them with `PdfDoc.open`.
///
/// The owner password always works. The user password works only when the
/// file has no permission restrictions. Throws `PdfluentError` with
/// `E-SECURITY-DECRYPTION-FAILED` for a password that does not entitle the
/// caller, and `E-WASM-INVALID-ARGUMENT` for a PDF without a password.
#[wasm_bindgen(js_name = "decryptPdf")]
pub fn decrypt_pdf(bytes: &[u8], password: &str) -> Result<Vec<u8>, JsValue> {
    decrypt_bytes(bytes, password).map_err(|e| e.into_js("decryptPdf"))
}

// ---------- #615 text watermark ------------------------------------------------

#[wasm_bindgen(typescript_custom_section)]
const TS_WATERMARK: &'static str = r##"
/** A stamp with preset text, size, rotation, opacity and colour (red, 60pt, 45°, 25%). */
export type WatermarkPreset = "draft" | "confidential" | "approved" | "final" | "copy" | "notForDistribution";

/** Options for `PdfDoc.addWatermark`. Give `text`, `preset`, or both (then `text` wins). */
export interface WatermarkOptions {
  text?: string;
  preset?: WatermarkPreset;
  /** Points. Default 48 (60 with a preset). */
  fontSize?: number;
  /** Degrees counter-clockwise. Default 45. */
  rotation?: number;
  /** 0..1. Default 0.3 (0.25 with a preset). */
  opacity?: number;
  /** `"#rrggbb"`. Default mid grey (red with a preset). */
  color?: string;
  /** `"center"` (default), or a corner / exact point in PDF points. */
  position?: "center" | { anchor: "topLeft" | "topRight" | "bottomLeft" | "bottomRight" | "exact"; x: number; y: number };
  /** Default `"foreground"`. */
  layer?: "foreground" | "background";
  /** Repeat across the page. `true` = 200pt spacing. */
  tile?: boolean | { hSpacing?: number; vSpacing?: number };
  /** `"all"` (default), `"even"`, `"odd"`, 0-based page indices, or an inclusive 0-based range. */
  pages?: "all" | "even" | "odd" | number[] | { from: number; to: number };
}
"##;

#[wasm_bindgen]
extern "C" {
    /// `WatermarkOptions` in TypeScript.
    #[wasm_bindgen(typescript_type = "WatermarkOptions")]
    pub type WatermarkOptionsJs;
}

/// Where a watermark sits.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum WatermarkPosition {
    /// `"center"`.
    Named(String),
    /// A corner offset or an exact point, in PDF points.
    Anchored {
        /// `topLeft`, `topRight`, `bottomLeft`, `bottomRight` or `exact`.
        anchor: String,
        /// Horizontal offset or coordinate.
        x: f32,
        /// Vertical offset or coordinate.
        y: f32,
    },
}

/// Tiling for a watermark.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum WatermarkTile {
    /// `true` for the default spacing, `false` for none.
    On(bool),
    /// Explicit spacing in points.
    Spacing {
        /// Horizontal spacing.
        #[serde(rename = "hSpacing")]
        h_spacing: Option<f32>,
        /// Vertical spacing.
        #[serde(rename = "vSpacing")]
        v_spacing: Option<f32>,
    },
}

/// Which pages get a watermark.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum WatermarkPages {
    /// `all`, `even` or `odd`.
    Named(String),
    /// 0-based page indices.
    List(Vec<u32>),
    /// Inclusive 0-based range.
    Range {
        /// First page.
        from: u32,
        /// Last page.
        to: u32,
    },
}

/// Options for [`watermark_bytes`]; see `WatermarkOptions` in TypeScript.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WatermarkOptions {
    /// Text to stamp.
    pub text: Option<String>,
    /// A stamp preset.
    pub preset: Option<String>,
    /// Font size in points.
    pub font_size: Option<f32>,
    /// Rotation in degrees.
    pub rotation: Option<f32>,
    /// Opacity 0..=1.
    pub opacity: Option<f32>,
    /// `#rrggbb`.
    pub color: Option<String>,
    /// Position.
    pub position: Option<WatermarkPosition>,
    /// `foreground` or `background`.
    pub layer: Option<String>,
    /// Tiling.
    pub tile: Option<WatermarkTile>,
    /// Page selection.
    pub pages: Option<WatermarkPages>,
}

/// Apply a text watermark described by `opts` and return the new bytes.
pub fn watermark_bytes(bytes: &[u8], opts: &WatermarkOptions) -> Result<Vec<u8>, ToolError> {
    use pdf_manip::watermark::{
        apply_text_watermark_tiled, Color, Layer, PageSelection, Position, StampPreset,
        TextWatermark, TiledConfig,
    };

    let preset = match opts.preset.as_deref() {
        None => None,
        Some("draft") => Some(StampPreset::Draft),
        Some("confidential") => Some(StampPreset::Confidential),
        Some("approved") => Some(StampPreset::Approved),
        Some("final") => Some(StampPreset::Final),
        Some("copy") => Some(StampPreset::Copy),
        Some("notForDistribution") => Some(StampPreset::NotForDistribution),
        Some(other) => {
            return Err(ToolError::invalid(format!(
                "watermark: unknown preset {other:?}"
            )))
        }
    };
    let mut wm = match preset {
        Some(p) => p.to_watermark(),
        None => TextWatermark {
            text: String::new(),
            font_size: 48.0,
            rotation: 45.0,
            opacity: 0.3,
            color: Color::Gray(0.5),
            position: Position::Center,
            layer: Layer::Foreground,
        },
    };
    if let Some(text) = &opts.text {
        wm.text = text.clone();
    }
    if wm.text.is_empty() {
        return Err(ToolError::invalid(
            "watermark: give a non-empty text or a preset",
        ));
    }
    if let Some(size) = opts.font_size {
        if !(size.is_finite() && size > 0.0) {
            return Err(ToolError::invalid("watermark: fontSize must be > 0"));
        }
        wm.font_size = size;
    }
    if let Some(rotation) = opts.rotation {
        if !rotation.is_finite() {
            return Err(ToolError::invalid("watermark: rotation must be finite"));
        }
        wm.rotation = rotation;
    }
    if let Some(opacity) = opts.opacity {
        if !(0.0..=1.0).contains(&opacity) {
            return Err(ToolError::invalid(
                "watermark: opacity must be in 0.0..=1.0",
            ));
        }
        wm.opacity = opacity;
    }
    if let Some(hex) = &opts.color {
        let (r, g, b) = crate::edits::parse_color_hex(Some(hex)).ok_or_else(|| {
            ToolError::invalid(format!("watermark: color must be \"#rrggbb\", got {hex:?}"))
        })?;
        wm.color = Color::Rgb(r as f32, g as f32, b as f32);
    }
    if let Some(pos) = &opts.position {
        wm.position = match pos {
            WatermarkPosition::Named(n) if n == "center" => Position::Center,
            WatermarkPosition::Anchored { anchor, x, y } => match anchor.as_str() {
                "topLeft" => Position::TopLeft(*x, *y),
                "topRight" => Position::TopRight(*x, *y),
                "bottomLeft" => Position::BottomLeft(*x, *y),
                "bottomRight" => Position::BottomRight(*x, *y),
                "exact" => Position::Exact(*x, *y),
                other => {
                    return Err(ToolError::invalid(format!(
                        "watermark: unknown position anchor {other:?}"
                    )))
                }
            },
            WatermarkPosition::Named(other) => {
                return Err(ToolError::invalid(format!(
                    "watermark: unknown position {other:?}"
                )))
            }
        };
    }
    if let Some(layer) = opts.layer.as_deref() {
        wm.layer = match layer {
            "foreground" => Layer::Foreground,
            "background" => Layer::Background,
            other => {
                return Err(ToolError::invalid(format!(
                    "watermark: layer must be \"foreground\" or \"background\", got {other:?}"
                )))
            }
        };
    }
    let tiled = match &opts.tile {
        None | Some(WatermarkTile::On(false)) => None,
        Some(WatermarkTile::On(true)) => Some(TiledConfig::default()),
        Some(WatermarkTile::Spacing {
            h_spacing,
            v_spacing,
        }) => {
            let d = TiledConfig::default();
            let h = h_spacing.unwrap_or(d.h_spacing);
            let v = v_spacing.unwrap_or(d.v_spacing);
            // Zero spacing would never finish tiling a page.
            if !(h.is_finite() && v.is_finite() && h > 0.0 && v > 0.0) {
                return Err(ToolError::invalid("watermark: tile spacing must be > 0"));
            }
            Some(TiledConfig {
                h_spacing: h,
                v_spacing: v,
            })
        }
    };
    let selection = match &opts.pages {
        None => PageSelection::All,
        Some(WatermarkPages::Named(n)) => match n.as_str() {
            "all" => PageSelection::All,
            "even" => PageSelection::Even,
            "odd" => PageSelection::Odd,
            other => {
                return Err(ToolError::invalid(format!(
                    "watermark: unknown page selection {other:?}"
                )))
            }
        },
        Some(WatermarkPages::List(list)) => {
            PageSelection::Pages(list.iter().map(|p| p.saturating_add(1)).collect())
        }
        Some(WatermarkPages::Range { from, to }) => {
            PageSelection::Range(from.saturating_add(1), to.saturating_add(1))
        }
    };

    let mut doc = lopdf::Document::load_mem(bytes).map_err(ToolError::bad_pdf)?;
    apply_text_watermark_tiled(&mut doc, &wm, &selection, tiled.as_ref()).map_err(|e| match e {
        pdf_manip::error::ManipError::PageOutOfRange(..) => ToolError::new(
            code::WASM_PAGE_OUT_OF_RANGE,
            legacy_code::PAGE_OUT_OF_RANGE,
            format!("watermark: {e}"),
        ),
        other => ToolError::internal(format!("watermark failed: {other}")),
    })?;
    save(&mut doc)
}

#[wasm_bindgen]
impl PdfDoc {
    /// Apply a text watermark with full control over text or preset, size,
    /// rotation, opacity, colour, position, layer, tiling and pages. Returns
    /// the new PDF bytes. Text only: there is no image watermark.
    #[wasm_bindgen(js_name = "addWatermark")]
    pub fn add_watermark(&self, options: WatermarkOptionsJs) -> Result<Vec<u8>, JsValue> {
        let op = "PdfDoc.addWatermark";
        let opts: WatermarkOptions =
            parse_options(&options.into(), "options").map_err(|e| e.into_js(op))?;
        watermark_bytes(self.pdf.data().as_ref(), &opts).map_err(|e| e.into_js(op))
    }
}

// ---------- #616 image extraction ----------------------------------------------

#[wasm_bindgen(typescript_custom_section)]
const TS_IMAGES: &'static str = r#"
/** An image exported faithfully. `page` is 0-based: the first page it is used on. */
export interface ExtractedImage {
  page: number;
  width: number;
  height: number;
  mime: "image/png" | "image/jpeg";
  data: Uint8Array;
}

/** An image left out because it could not be exported without changing how it looks. */
export interface SkippedImage {
  page: number;
  width: number;
  height: number;
  reason: "unsupported_filter" | "transparency" | "decode_array" | "cmyk_jpeg" | "color_space" | "bit_depth" | "image_mask" | "undecodable";
  detail: string;
}

export interface ImageExtraction {
  images: ExtractedImage[];
  skipped: SkippedImage[];
}
"#;

#[wasm_bindgen]
extern "C" {
    /// `ImageExtraction` in TypeScript.
    #[wasm_bindgen(typescript_type = "ImageExtraction")]
    pub type ImageExtractionJs;
}

/// Export every image in the PDF, reporting the ones it cannot reproduce.
pub fn export_image_list(bytes: &[u8]) -> Result<pdf_extract::ImageExport, ToolError> {
    let doc = lopdf::Document::load_mem(bytes).map_err(ToolError::bad_pdf)?;
    Ok(pdf_extract::export_images(&doc))
}

/// Extract the images of a PDF as PNG or JPEG files.
///
/// Returns `{ images, skipped }`. An image that could not be exported exactly
/// as it looks -- JPEG 2000, JBIG2 or fax compression, a transparency mask, a
/// Decode array, CMYK, an ICC profile other than sRGB -- is listed in `skipped`
/// with the reason, never emitted wrong. Each image is listed once, on the
/// first page (0-based) that uses it.
#[wasm_bindgen(js_name = "extractImages")]
pub fn extract_images(bytes: &[u8]) -> Result<ImageExtractionJs, JsValue> {
    use js_sys::{Array, Object, Reflect, Uint8Array};

    let op = "extractImages";
    let export = export_image_list(bytes).map_err(|e| e.into_js(op))?;
    let set = |o: &Object, k: &str, v: &JsValue| {
        Reflect::set(o, &JsValue::from_str(k), v)
            .map(|_| ())
            .map_err(|_| {
                ToolError::internal("extractImages: building the result failed").into_js(op)
            })
    };

    let images = Array::new();
    for img in &export.images {
        let o = Object::new();
        set(&o, "page", &JsValue::from(img.page.saturating_sub(1)))?;
        set(&o, "width", &JsValue::from(img.width))?;
        set(&o, "height", &JsValue::from(img.height))?;
        set(&o, "mime", &JsValue::from_str(img.image.mime))?;
        set(
            &o,
            "data",
            &Uint8Array::from(img.image.data.as_slice()).into(),
        )?;
        images.push(&o);
    }
    let skipped = Array::new();
    for s in &export.skipped {
        let o = Object::new();
        set(&o, "page", &JsValue::from(s.page.saturating_sub(1)))?;
        set(&o, "width", &JsValue::from(s.width))?;
        set(&o, "height", &JsValue::from(s.height))?;
        set(&o, "reason", &JsValue::from_str(s.reason.code()))?;
        set(&o, "detail", &JsValue::from_str(&s.reason.detail()))?;
        skipped.push(&o);
    }
    let result = Object::new();
    set(&result, "images", &images)?;
    set(&result, "skipped", &skipped)?;
    Ok(JsValue::from(result).unchecked_into())
}

// ---------- #617 signing -----------------------------------------------------------

#[wasm_bindgen(typescript_custom_section)]
const TS_SIGN: &'static str = r#"
/** Options for `signPdf`. The signature is a cryptographic (PAdES) signature, not a drawn one. */
export interface SignPdfOptions {
  reason?: string;
  location?: string;
  contactInfo?: string;
  /** Default "Signature1". */
  fieldName?: string;
  /** Show the signature on a page (0-based) at a rectangle in PDF points. Invisible when absent. */
  visibleRect?: { page: number; x: number; y: number; width: number; height: number };
}
"#;

#[wasm_bindgen]
extern "C" {
    /// `SignPdfOptions` in TypeScript.
    #[wasm_bindgen(typescript_type = "SignPdfOptions")]
    pub type SignPdfOptionsJs;
}

/// Where a visible signature goes.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisibleRect {
    /// 0-based page index.
    pub page: u32,
    /// Left edge in points.
    pub x: f64,
    /// Bottom edge in points.
    pub y: f64,
    /// Width in points.
    pub width: f64,
    /// Height in points.
    pub height: f64,
}

/// Options for [`sign_bytes`]; see `SignPdfOptions` in TypeScript.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignPdfOptions {
    /// Reason for signing.
    pub reason: Option<String>,
    /// Location of signing.
    pub location: Option<String>,
    /// Contact information of the signer.
    pub contact_info: Option<String>,
    /// Signature field name.
    pub field_name: Option<String>,
    /// Visible signature placement.
    pub visible_rect: Option<VisibleRect>,
}

/// Sign a PDF with the identity in a PKCS#12 (.p12/.pfx) file, as an
/// incremental update, and return the signed bytes.
pub fn sign_bytes(
    bytes: &[u8],
    pkcs12: &[u8],
    password: &str,
    opts: &SignPdfOptions,
) -> Result<Vec<u8>, ToolError> {
    use pdf_sign::signer::{Pkcs12Signer, SignError};

    let signer = Pkcs12Signer::from_pkcs12(pkcs12, password).map_err(|e| match e {
        SignError::Pkcs12Load(msg) if msg.contains("MAC verification failed") => {
            ToolError::password("wrong password for the certificate file")
        }
        other => ToolError::invalid(format!("signPdf: certificate file: {other}")),
    })?;
    let visible_rect = match &opts.visible_rect {
        None => None,
        Some(r) => {
            let finite = [r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite());
            if !finite || r.width <= 0.0 || r.height <= 0.0 {
                return Err(ToolError::invalid(
                    "signPdf: visibleRect needs a positive width and height",
                ));
            }
            let pages = lopdf::Document::load_mem(bytes)
                .map_err(ToolError::bad_pdf)?
                .get_pages()
                .len();
            if r.page as usize >= pages {
                return Err(ToolError::new(
                    code::WASM_PAGE_OUT_OF_RANGE,
                    legacy_code::PAGE_OUT_OF_RANGE,
                    format!("signPdf: page {} of {pages}", r.page),
                ));
            }
            Some((r.page + 1, [r.x, r.y, r.x + r.width, r.y + r.height]))
        }
    };
    let options = pdf_sign::SignOptions {
        reason: opts.reason.clone(),
        location: opts.location.clone(),
        contact: opts.contact_info.clone(),
        field_name: opts.field_name.clone(),
        visible_rect,
        ..Default::default()
    };
    pdf_sign::sign_pdf_incremental(bytes, &signer, &options).map_err(|e| {
        ToolError::new(
            code::WASM_SIGN_FAILED,
            legacy_code::SIGN_FAILED,
            format!("signPdf failed: {e}"),
        )
    })
}

/// Sign a PDF with your own certificate (a .p12/.pfx file and its password)
/// and return the signed bytes. The private key never leaves the page.
///
/// This is a cryptographic PAdES signature (ETSI.CAdES.detached), appended as
/// an incremental update so earlier signatures stay valid. Without a
/// timestamp authority: no TSA or LTV in the browser. A self-signed
/// certificate verifies, but readers such as Acrobat show its validity as
/// unknown because they do not trust the issuer. Throws `PdfluentError` with
/// `E-SECURITY-DECRYPTION-FAILED` for a wrong certificate password.
#[wasm_bindgen(js_name = "signPdf")]
pub fn sign_pdf(
    bytes: &[u8],
    pkcs12: &[u8],
    password: &str,
    options: Option<SignPdfOptionsJs>,
) -> Result<Vec<u8>, JsValue> {
    let op = "signPdf";
    let opts: SignPdfOptions = match options {
        Some(o) => parse_options(&o.into(), "options").map_err(|e| e.into_js(op))?,
        None => SignPdfOptions::default(),
    };
    sign_bytes(bytes, pkcs12, password, &opts).map_err(|e| e.into_js(op))
}
