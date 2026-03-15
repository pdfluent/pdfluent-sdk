//! PDF document signing — two-pass signing, incremental updates, DocMDP.
//!
//! Provides [`sign_pdf`] and [`sign_pdf_incremental`] entry points that
//! produce signed PDF bytes using the [`PdfSigner`] trait.

use crate::signer::{PdfSigner, SignError};
use crate::types::{DocMdpPermission, SubFilter};
use lopdf::dictionary;
use lopdf::{Document, Object, ObjectId};

/// Options for signing a PDF document.
#[derive(Debug, Clone)]
pub struct SignOptions {
    /// Reason for signing (appears in signature panel).
    pub reason: Option<String>,
    /// Location of signing.
    pub location: Option<String>,
    /// Contact info of signer.
    pub contact: Option<String>,
    /// Signature field name. If `None`, creates "Signature1".
    pub field_name: Option<String>,
    /// Visible signature rectangle `(page_number, [x0, y0, x1, y1])`.
    /// If `None`, creates invisible signature.
    pub visible_rect: Option<(u32, [f64; 4])>,
    /// SubFilter to use. Default: `EtsiCadesDetached` (PAdES).
    pub sub_filter: SubFilter,
    /// DocMDP permission level for certification signature.
    /// `None` means an approval signature (not certification).
    pub certification: Option<DocMdpPermission>,
    /// Placeholder size in bytes for the CMS signature (default 8192).
    pub placeholder_size: usize,
}

impl Default for SignOptions {
    fn default() -> Self {
        Self {
            reason: None,
            location: None,
            contact: None,
            field_name: None,
            visible_rect: None,
            sub_filter: SubFilter::EtsiCadesDetached,
            certification: None,
            placeholder_size: 8192,
        }
    }
}

/// Placeholder positions within the prepared PDF buffer.
struct PlaceholderInfo {
    /// Byte offset of the '<' of /Contents hex string.
    contents_hex_start: usize,
    /// Byte offset after the '>' of /Contents hex string.
    contents_hex_end: usize,
    /// Byte offsets of the 4 ByteRange placeholder digit strings.
    byte_range_offsets: [(usize, usize); 4],
}

/// Sign a PDF document, returning the signed PDF bytes.
///
/// Uses a two-pass approach: first prepares the PDF with a placeholder
/// for the signature, then computes the hash and injects the CMS.
pub fn sign_pdf(
    pdf_bytes: &[u8],
    signer: &impl PdfSigner,
    options: &SignOptions,
) -> Result<Vec<u8>, SignError> {
    let mut doc =
        Document::load_mem(pdf_bytes).map_err(|e| SignError::CmsBuild(format!("load: {e}")))?;

    let (mut buffer, placeholder) = prepare_pdf_with_placeholder(&mut doc, signer, options)?;
    inject_signature(&mut buffer, &placeholder, signer)?;
    Ok(buffer)
}

/// Sign a PDF incrementally (append-only, preserves existing signatures).
pub fn sign_pdf_incremental(
    pdf_bytes: &[u8],
    signer: &impl PdfSigner,
    options: &SignOptions,
) -> Result<Vec<u8>, SignError> {
    let prev =
        Document::load_mem(pdf_bytes).map_err(|e| SignError::CmsBuild(format!("load: {e}")))?;
    let mut doc = Document::new_from_prev(&prev);

    let (buffer, placeholder) = prepare_pdf_with_placeholder(&mut doc, signer, options)?;

    // Prepend the original bytes.
    let mut result = pdf_bytes.to_vec();
    // Adjust placeholder offsets by the prefix length.
    let offset = result.len();
    result.extend_from_slice(&buffer);

    let adjusted = PlaceholderInfo {
        contents_hex_start: placeholder.contents_hex_start + offset,
        contents_hex_end: placeholder.contents_hex_end + offset,
        byte_range_offsets: [
            (
                placeholder.byte_range_offsets[0].0 + offset,
                placeholder.byte_range_offsets[0].1,
            ),
            (
                placeholder.byte_range_offsets[1].0 + offset,
                placeholder.byte_range_offsets[1].1,
            ),
            (
                placeholder.byte_range_offsets[2].0 + offset,
                placeholder.byte_range_offsets[2].1,
            ),
            (
                placeholder.byte_range_offsets[3].0 + offset,
                placeholder.byte_range_offsets[3].1,
            ),
        ],
    };

    inject_signature(&mut result, &adjusted, signer)?;
    Ok(result)
}

/// Pass 1 — Prepare the PDF with a signature placeholder.
fn prepare_pdf_with_placeholder(
    doc: &mut Document,
    signer: &impl PdfSigner,
    options: &SignOptions,
) -> Result<(Vec<u8>, PlaceholderInfo), SignError> {
    let placeholder_hex_len = options.placeholder_size * 2;

    // Build the /Contents hex placeholder string.
    let contents_placeholder = vec![0u8; options.placeholder_size];

    // Get signer name from certificate chain.
    let signer_name = signer
        .certificate_chain_der()
        .first()
        .and_then(|der| crate::x509::X509Certificate::from_der(der))
        .and_then(|cert| cert.subject_common_name());

    // Build ByteRange placeholder array with 10-digit integers.
    // We'll patch these after serialization with the real offsets.
    const PLACEHOLDER_INT: i64 = 1_000_000_000;

    // Build signature dictionary.
    let sub_filter_name = match options.sub_filter {
        SubFilter::EtsiCadesDetached => "ETSI.CAdES.detached",
        SubFilter::AdbePkcs7Detached => "adbe.pkcs7.detached",
        SubFilter::AdbePkcs7Sha1 => "adbe.pkcs7.sha1",
        _ => "adbe.pkcs7.detached",
    };

    let mut sig_dict = dictionary! {
        "Type" => Object::Name(b"Sig".to_vec()),
        "Filter" => Object::Name(b"Adobe.PPKLite".to_vec()),
        "SubFilter" => Object::Name(sub_filter_name.as_bytes().to_vec()),
        "ByteRange" => Object::Array(vec![
            Object::Integer(0),
            Object::Integer(PLACEHOLDER_INT),
            Object::Integer(PLACEHOLDER_INT),
            Object::Integer(PLACEHOLDER_INT),
        ]),
        "Contents" => Object::String(contents_placeholder, lopdf::StringFormat::Hexadecimal),
    };

    if let Some(name) = &signer_name {
        sig_dict.set("Name", Object::string_literal(name.as_bytes()));
    }
    if let Some(reason) = &options.reason {
        sig_dict.set("Reason", Object::string_literal(reason.as_bytes()));
    }
    if let Some(location) = &options.location {
        sig_dict.set("Location", Object::string_literal(location.as_bytes()));
    }
    if let Some(contact) = &options.contact {
        sig_dict.set("ContactInfo", Object::string_literal(contact.as_bytes()));
    }

    // DocMDP certification reference.
    if let Some(permission) = &options.certification {
        let transform_params = dictionary! {
            "Type" => Object::Name(b"TransformParams".to_vec()),
            "P" => Object::Integer(*permission as i64),
            "V" => Object::Name(b"1.2".to_vec()),
        };
        let sig_ref = dictionary! {
            "Type" => Object::Name(b"SigRef".to_vec()),
            "TransformMethod" => Object::Name(b"DocMDP".to_vec()),
            "TransformParams" => Object::Dictionary(transform_params),
        };
        sig_dict.set(
            "Reference",
            Object::Array(vec![Object::Dictionary(sig_ref)]),
        );
    }

    let sig_obj_id = doc.add_object(Object::Dictionary(sig_dict));

    // Build signature field widget annotation.
    let field_name = options
        .field_name
        .clone()
        .unwrap_or_else(|| "Signature1".to_string());

    let rect = if let Some((_, r)) = &options.visible_rect {
        vec![
            Object::Real(r[0] as f32),
            Object::Real(r[1] as f32),
            Object::Real(r[2] as f32),
            Object::Real(r[3] as f32),
        ]
    } else {
        vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(0),
        ]
    };

    // Get first page ID for /P reference.
    let page_id = doc
        .page_iter()
        .next()
        .ok_or_else(|| SignError::CmsBuild("PDF has no pages".into()))?;

    let mut field_dict = dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Widget".to_vec()),
        "FT" => Object::Name(b"Sig".to_vec()),
        "T" => Object::string_literal(field_name.as_bytes()),
        "V" => Object::Reference(sig_obj_id),
        "F" => Object::Integer(132), // Print + Locked
        "Rect" => Object::Array(rect),
        "P" => Object::Reference(page_id),
    };

    // Visible signature appearance.
    if let Some((page_num, r)) = &options.visible_rect {
        let width = r[2] - r[0];
        let height = r[3] - r[1];
        if width > 0.0 && height > 0.0 {
            let app_info = crate::appearance::SignatureAppearanceInfo {
                signer_name: signer_name.clone(),
                reason: options.reason.clone(),
                location: options.location.clone(),
                contact_info: options.contact.clone(),
                date: None,
                has_custom_appearance: false,
            };
            let stream_bytes = crate::appearance::generate_appearance_stream(
                &app_info,
                width,
                height,
                &crate::types::SignatureAppearanceStyle::Standard,
            );

            // Font resource for the appearance.
            let font = dictionary! {
                "Type" => Object::Name(b"Font".to_vec()),
                "Subtype" => Object::Name(b"Type1".to_vec()),
                "BaseFont" => Object::Name(b"Helvetica".to_vec()),
            };
            let font_id = doc.add_object(Object::Dictionary(font));

            let resources = dictionary! {
                "Font" => dictionary! {
                    "F1" => Object::Reference(font_id),
                },
            };

            let ap_stream = lopdf::Stream::new(
                dictionary! {
                    "Type" => Object::Name(b"XObject".to_vec()),
                    "Subtype" => Object::Name(b"Form".to_vec()),
                    "BBox" => Object::Array(vec![
                        Object::Real(0.0),
                        Object::Real(0.0),
                        Object::Real(width as f32),
                        Object::Real(height as f32),
                    ]),
                    "Resources" => Object::Dictionary(resources),
                },
                stream_bytes,
            );
            let ap_id = doc.add_object(Object::Stream(ap_stream));
            let ap_dict = dictionary! {
                "N" => Object::Reference(ap_id),
            };
            field_dict.set("AP", Object::Dictionary(ap_dict));

            // Override page reference to the target page.
            let pages = doc.get_pages();
            if let Some(&target_page_id) = pages.get(page_num) {
                field_dict.set("P", Object::Reference(target_page_id));
            }
        }
    }

    let field_id = doc.add_object(Object::Dictionary(field_dict));

    // Add field to page's /Annots.
    let annots_page_id = if let Some((page_num, _)) = &options.visible_rect {
        let pages = doc.get_pages();
        pages.get(page_num).copied().unwrap_or(page_id)
    } else {
        page_id
    };

    if let Ok(page_dict) = doc.get_dictionary_mut(annots_page_id) {
        let has_annots = page_dict
            .get_mut(b"Annots")
            .ok()
            .and_then(|a| a.as_array_mut().ok())
            .map(|arr| {
                arr.push(Object::Reference(field_id));
            })
            .is_some();
        if !has_annots {
            page_dict.set("Annots", Object::Array(vec![Object::Reference(field_id)]));
        }
    }

    // Ensure AcroForm exists with the field in /Fields.
    ensure_acroform(doc, field_id)?;

    // DocMDP: add /Perms to catalog.
    if options.certification.is_some() {
        let catalog = doc
            .catalog_mut()
            .map_err(|e| SignError::CmsBuild(format!("catalog: {e}")))?;
        let perms = dictionary! {
            "DocMDP" => Object::Reference(sig_obj_id),
        };
        catalog.set("Perms", Object::Dictionary(perms));
    }

    // Serialize.
    let mut buffer = Vec::new();
    doc.save_to(&mut buffer)
        .map_err(|e| SignError::CmsBuild(format!("save: {e}")))?;

    // Locate the placeholder positions in the serialized output.
    let placeholder = find_placeholders(&buffer, placeholder_hex_len)?;

    Ok((buffer, placeholder))
}

/// Locate the ByteRange and Contents placeholders in the serialized PDF.
fn find_placeholders(
    buffer: &[u8],
    placeholder_hex_len: usize,
) -> Result<PlaceholderInfo, SignError> {
    // Find /Contents hex string: look for '<' followed by all zeros '>' pattern.
    // The lopdf serializer outputs hex strings as <HEXCHARS>.
    let contents_hex_start = find_contents_hex(buffer, placeholder_hex_len)
        .ok_or_else(|| SignError::CmsBuild("cannot locate /Contents placeholder".into()))?;
    let contents_hex_end = contents_hex_start + 1 + placeholder_hex_len + 1; // < + hex + >

    // Find /ByteRange placeholder — search BACKWARD from /Contents so that
    // pre-existing /ByteRange entries (from signatures already in the PDF)
    // are not mistakenly patched instead of our placeholder.
    // Fixes #440: PDFs with pre-existing signatures contain earlier /ByteRange
    // arrays; find_byte_range_offsets must find the one for our new signature.
    let byte_range_offsets = find_byte_range_offsets(buffer, contents_hex_start)
        .ok_or_else(|| SignError::CmsBuild("cannot locate /ByteRange placeholder".into()))?;

    Ok(PlaceholderInfo {
        contents_hex_start,
        contents_hex_end,
        byte_range_offsets,
    })
}

/// Find the /Contents hex string placeholder (all-zero hex).
fn find_contents_hex(buffer: &[u8], hex_len: usize) -> Option<usize> {
    // Look for "<00000...>" pattern.
    let expected_len = 1 + hex_len + 1; // < + hex + >

    for i in 0..buffer.len().saturating_sub(expected_len) {
        if buffer[i] == b'<' && i + expected_len <= buffer.len() && buffer[i + 1 + hex_len] == b'>'
        {
            // Check if all characters between < and > are '0'.
            let hex_slice = &buffer[i + 1..i + 1 + hex_len];
            if hex_slice.iter().all(|&b| b == b'0') {
                return Some(i);
            }
        }
    }
    None
}

/// Find the ByteRange placeholder offsets.
///
/// Searches backward from `contents_pos` for the nearest `/ByteRange` entry,
/// which belongs to our signature dict (the /Contents placeholder always
/// appears after /ByteRange in the same sig dict, and any pre-existing
/// signature dicts appear earlier in the serialized output).
///
/// Returns `[(offset, len); 4]` where each element is the (byte offset, digit length)
/// of one of the four numbers in the ByteRange array.
fn find_byte_range_offsets(buffer: &[u8], contents_pos: usize) -> Option<[(usize, usize); 4]> {
    // Search the region before /Contents for the nearest /ByteRange.
    let needle = b"/ByteRange";
    let search_in = &buffer[..contents_pos];

    // Find the last occurrence (reverse scan).
    let br_pos = search_in
        .windows(needle.len())
        .enumerate()
        .rev()
        .find_map(|(i, w)| if w == needle { Some(i) } else { None })?;

    // Find the '[' after /ByteRange.
    let bracket_start = buffer[br_pos..].iter().position(|&b| b == b'[')? + br_pos;

    // Parse the 4 number positions inside [ ... ].
    let mut pos = bracket_start + 1;
    let mut offsets = [(0usize, 0usize); 4];

    for item in &mut offsets {
        // Skip whitespace.
        while pos < buffer.len() && buffer[pos].is_ascii_whitespace() {
            pos += 1;
        }
        // Record start of number.
        let start = pos;
        // Advance through digits.
        while pos < buffer.len() && buffer[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == start {
            return None;
        }
        *item = (start, pos - start);
    }

    Some(offsets)
}

/// Pass 2 — Compute hash and inject signature.
fn inject_signature(
    buffer: &mut [u8],
    placeholder: &PlaceholderInfo,
    signer: &impl PdfSigner,
) -> Result<(), SignError> {
    let file_len = buffer.len();

    // The actual byte range: [0, contents_start, contents_end, rest_len].
    let br = [
        0usize,
        placeholder.contents_hex_start,
        placeholder.contents_hex_end,
        file_len - placeholder.contents_hex_end,
    ];

    // Patch the ByteRange values.
    for (i, val) in br.iter().enumerate() {
        let (offset, len) = placeholder.byte_range_offsets[i];
        let val_str = format!("{val}");
        if val_str.len() > len {
            return Err(SignError::CmsBuild(format!(
                "ByteRange value {val} doesn't fit in {len} digits"
            )));
        }
        // Right-pad with spaces.
        let padded = format!("{val:<width$}", width = len);
        buffer[offset..offset + len].copy_from_slice(padded.as_bytes());
    }

    // Compute hash over the byte ranges.
    let range1 = &buffer[br[0]..br[0] + br[1]];
    let range2 = &buffer[br[2]..br[2] + br[3]];

    let data_to_hash = [range1, range2].concat();

    // Build CMS signature.
    let cms_der = signer.sign(&data_to_hash)?;

    // Hex-encode the CMS DER.
    let hex_signature = hex_encode(&cms_der);
    let placeholder_hex_len = placeholder.contents_hex_end - placeholder.contents_hex_start - 2;

    if hex_signature.len() > placeholder_hex_len {
        return Err(SignError::CmsBuild(format!(
            "CMS signature ({} bytes) exceeds placeholder ({} bytes)",
            cms_der.len(),
            placeholder_hex_len / 2,
        )));
    }

    // Pad with zeros.
    let mut padded_hex = hex_signature;
    padded_hex.extend(std::iter::repeat_n(
        b'0',
        placeholder_hex_len - padded_hex.len(),
    ));

    // Write into buffer (between < and >).
    let hex_start = placeholder.contents_hex_start + 1;
    buffer[hex_start..hex_start + placeholder_hex_len].copy_from_slice(&padded_hex);

    Ok(())
}

/// Ensure the document has an AcroForm with the field in /Fields.
///
/// Handles all four combinations of inline vs. indirect-reference for
/// /AcroForm and /Fields so that our signature field is reliably registered
/// regardless of how the source PDF structures its form dictionary.
/// Fixes #451: /Fields stored as an indirect array was silently skipped.
/// Fixes #458: silent if-let failures (e.g. /AcroForm null in catalog, or
/// inaccessible indirect AcroForm) now fall back to creating a fresh inline
/// AcroForm so the signature is always registered.
fn ensure_acroform(doc: &mut Document, field_id: ObjectId) -> Result<(), SignError> {
    // ── Phase 1: discover structure via temporary immutable borrows ──────────
    // All results are owned values (ObjectId = (u32, u16) is Copy).

    #[derive(Clone, Copy)]
    enum AfLoc {
        None,
        Inline,
        Ref(ObjectId),
    }
    #[derive(Clone, Copy)]
    enum FlLoc {
        None,
        Inline,
        Ref(ObjectId),
    }

    let af_loc: AfLoc = {
        let cat = doc
            .catalog()
            .map_err(|e| SignError::CmsBuild(format!("catalog: {e}")))?;
        match cat.get(b"AcroForm") {
            Ok(Object::Reference(id)) => AfLoc::Ref(*id),
            Ok(_) => AfLoc::Inline,
            Err(_) => AfLoc::None,
        }
    }; // catalog borrow dropped

    let fl_loc: FlLoc = match af_loc {
        AfLoc::None => FlLoc::None,
        AfLoc::Inline => {
            let cat = doc
                .catalog()
                .map_err(|e| SignError::CmsBuild(format!("catalog: {e}")))?;
            match cat.get(b"AcroForm") {
                Ok(Object::Dictionary(af)) => match af.get(b"Fields") {
                    Ok(Object::Reference(id)) => FlLoc::Ref(*id),
                    Ok(Object::Array(_)) => FlLoc::Inline,
                    _ => FlLoc::None,
                },
                _ => FlLoc::None,
            }
        } // catalog borrow dropped
        AfLoc::Ref(aid) => match doc.get_dictionary(aid) {
            Ok(af) => match af.get(b"Fields") {
                Ok(Object::Reference(id)) => FlLoc::Ref(*id),
                Ok(Object::Array(_)) => FlLoc::Inline,
                _ => FlLoc::None,
            },
            Err(_) => FlLoc::None,
        }, // dictionary borrow dropped
    };

    // ── Phase 2: mutate — all immutable borrows are now dropped ─────────────
    // Returns true if the field was successfully added, false on silent failure.
    let field_added = match (af_loc, fl_loc) {
        (AfLoc::None, _) => {
            // No AcroForm at all — create a new inline one.
            let cat = doc
                .catalog_mut()
                .map_err(|e| SignError::CmsBuild(format!("catalog: {e}")))?;
            let acroform = dictionary! {
                "Fields" => Object::Array(vec![Object::Reference(field_id)]),
                "SigFlags" => Object::Integer(3),
            };
            cat.set("AcroForm", Object::Dictionary(acroform));
            true
        }
        (AfLoc::Inline, FlLoc::Inline) => {
            let cat = doc
                .catalog_mut()
                .map_err(|e| SignError::CmsBuild(format!("catalog: {e}")))?;
            if let Ok(Object::Dictionary(af)) = cat.get_mut(b"AcroForm") {
                if let Ok(Object::Array(arr)) = af.get_mut(b"Fields") {
                    arr.push(Object::Reference(field_id));
                }
                af.set("SigFlags", Object::Integer(3));
                true
            } else {
                false
            }
        }
        (AfLoc::Inline, FlLoc::None) => {
            let cat = doc
                .catalog_mut()
                .map_err(|e| SignError::CmsBuild(format!("catalog: {e}")))?;
            if let Ok(Object::Dictionary(af)) = cat.get_mut(b"AcroForm") {
                af.set("Fields", Object::Array(vec![Object::Reference(field_id)]));
                af.set("SigFlags", Object::Integer(3));
                true
            } else {
                false
            }
        }
        (AfLoc::Inline, FlLoc::Ref(fid)) => {
            // /Fields is an indirect array — push directly into the referenced object.
            let pushed = if let Ok(Object::Array(arr)) = doc.get_object_mut(fid) {
                arr.push(Object::Reference(field_id));
                true
            } else {
                false
            }; // mutable borrow of fid object dropped
            let cat = doc
                .catalog_mut()
                .map_err(|e| SignError::CmsBuild(format!("catalog: {e}")))?;
            if let Ok(Object::Dictionary(af)) = cat.get_mut(b"AcroForm") {
                af.set("SigFlags", Object::Integer(3));
            }
            pushed
        }
        (AfLoc::Ref(aid), FlLoc::Inline) => {
            if let Ok(af) = doc.get_dictionary_mut(aid) {
                if let Ok(Object::Array(arr)) = af.get_mut(b"Fields") {
                    arr.push(Object::Reference(field_id));
                }
                af.set("SigFlags", Object::Integer(3));
                true
            } else {
                false
            }
        }
        (AfLoc::Ref(aid), FlLoc::None) => {
            if let Ok(af) = doc.get_dictionary_mut(aid) {
                af.set("Fields", Object::Array(vec![Object::Reference(field_id)]));
                af.set("SigFlags", Object::Integer(3));
                true
            } else {
                false
            }
        }
        (AfLoc::Ref(aid), FlLoc::Ref(fid)) => {
            // Both AcroForm and Fields are indirect — modify both referenced objects.
            let pushed = if let Ok(Object::Array(arr)) = doc.get_object_mut(fid) {
                arr.push(Object::Reference(field_id));
                true
            } else {
                false
            }; // mutable borrow of fid dropped
            if let Ok(af) = doc.get_dictionary_mut(aid) {
                af.set("SigFlags", Object::Integer(3));
            }
            pushed
        }
    };

    // Fallback: if the field was not successfully registered (e.g. /AcroForm null
    // in catalog, or an inaccessible indirect AcroForm object), create a fresh
    // inline AcroForm. This guarantees our signature field is always present
    // after signing, even for unusual/corrupt AcroForm structures. Fixes #458.
    if !field_added {
        let cat = doc
            .catalog_mut()
            .map_err(|e| SignError::CmsBuild(format!("catalog (fallback): {e}")))?;
        let acroform = dictionary! {
            "Fields" => Object::Array(vec![Object::Reference(field_id)]),
            "SigFlags" => Object::Integer(3),
        };
        cat.set("AcroForm", Object::Dictionary(acroform));
    }

    Ok(())
}

/// Hex-encode bytes to uppercase hex ASCII.
fn hex_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 2);
    for &b in data {
        out.push(HEX_CHARS[(b >> 4) as usize]);
        out.push(HEX_CHARS[(b & 0x0F) as usize]);
    }
    out
}

const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signer::Pkcs12Signer;

    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn corpus_path(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/corpus-mini")
            .join(name)
    }

    fn load_rsa_signer() -> Pkcs12Signer {
        let data = std::fs::read(fixture_path("test-rsa.p12")).unwrap();
        Pkcs12Signer::from_pkcs12(&data, "test123").unwrap()
    }

    fn load_ec_signer() -> Pkcs12Signer {
        let data = std::fs::read(fixture_path("test-ec-p256.p12")).unwrap();
        Pkcs12Signer::from_pkcs12(&data, "test123").unwrap()
    }

    #[test]
    fn sign_and_validate_roundtrip_rsa() {
        let pdf = std::fs::read(corpus_path("simple.pdf")).unwrap();
        let signer = load_rsa_signer();
        let signed = sign_pdf(&pdf, &signer, &SignOptions::default()).unwrap();

        // Validate with existing validation pipeline.
        let parsed = pdf_syntax::Pdf::new(signed).unwrap();
        let results = crate::validate_signatures(&parsed);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(results[0].status, crate::types::ValidationStatus::Valid),
            "expected Valid, got {:?}",
            results[0].status
        );
    }

    #[test]
    fn sign_and_validate_roundtrip_ec() {
        let pdf = std::fs::read(corpus_path("simple.pdf")).unwrap();
        let signer = load_ec_signer();
        let signed = sign_pdf(&pdf, &signer, &SignOptions::default()).unwrap();

        let parsed = pdf_syntax::Pdf::new(signed).unwrap();
        let results = crate::validate_signatures(&parsed);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(results[0].status, crate::types::ValidationStatus::Valid),
            "expected Valid, got {:?}",
            results[0].status
        );
    }

    #[test]
    fn sign_with_reason_and_location() {
        let pdf = std::fs::read(corpus_path("simple.pdf")).unwrap();
        let signer = load_rsa_signer();
        let options = SignOptions {
            reason: Some("Approved".into()),
            location: Some("Amsterdam".into()),
            ..Default::default()
        };
        let signed = sign_pdf(&pdf, &signer, &options).unwrap();
        assert!(signed.len() > pdf.len());

        let parsed = pdf_syntax::Pdf::new(signed).unwrap();
        let results = crate::validate_signatures(&parsed);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn sign_visible_signature() {
        let pdf = std::fs::read(corpus_path("simple.pdf")).unwrap();
        let signer = load_rsa_signer();
        let options = SignOptions {
            visible_rect: Some((1, [72.0, 700.0, 250.0, 750.0])),
            reason: Some("Approved".into()),
            ..Default::default()
        };
        let signed = sign_pdf(&pdf, &signer, &options).unwrap();
        assert!(signed.len() > pdf.len());
    }

    #[test]
    fn certification_signature_sets_docmdp() {
        let pdf = std::fs::read(corpus_path("simple.pdf")).unwrap();
        let signer = load_rsa_signer();
        let options = SignOptions {
            certification: Some(DocMdpPermission::FormFillAndSign),
            ..Default::default()
        };
        let signed = sign_pdf(&pdf, &signer, &options).unwrap();

        // Verify /Perms/DocMDP is present via lopdf.
        let doc = Document::load_mem(&signed).unwrap();
        let catalog = doc.catalog().unwrap();
        let perms = catalog.get(b"Perms").unwrap().as_dict().unwrap();
        assert!(
            perms.get(b"DocMDP").is_ok(),
            "catalog /Perms/DocMDP should be present"
        );

        // Verify sig dict has /Reference with TransformMethod /DocMDP.
        let sig_ref = perms.get(b"DocMDP").unwrap().as_reference().unwrap();
        let sig_dict = doc.get_dictionary(sig_ref).unwrap();
        let reference_arr = sig_dict.get(b"Reference").unwrap().as_array().unwrap();
        assert!(!reference_arr.is_empty());
        let ref_dict = reference_arr[0].as_dict().unwrap();
        assert_eq!(
            ref_dict.get(b"TransformMethod").unwrap().as_name().unwrap(),
            b"DocMDP"
        );
        let params = ref_dict.get(b"TransformParams").unwrap().as_dict().unwrap();
        assert_eq!(params.get(b"P").unwrap().as_i64().unwrap(), 2);
    }

    #[test]
    fn signed_pdf_is_valid_pdf() {
        let pdf = std::fs::read(corpus_path("simple.pdf")).unwrap();
        let signer = load_rsa_signer();
        let signed = sign_pdf(&pdf, &signer, &SignOptions::default()).unwrap();

        // Should be loadable by lopdf.
        let doc = Document::load_mem(&signed);
        assert!(doc.is_ok(), "signed PDF should be valid: {:?}", doc.err());
    }

    #[test]
    fn hex_encode_works() {
        assert_eq!(hex_encode(&[0xDE, 0xAD, 0xBE, 0xEF]), b"DEADBEEF");
        assert_eq!(hex_encode(&[0x00, 0xFF]), b"00FF");
    }
}
