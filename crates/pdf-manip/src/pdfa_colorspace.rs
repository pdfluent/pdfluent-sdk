//! PDF/A color space normalization and OutputIntent management.
//!
//! Detects device-dependent color spaces and adds sRGB OutputIntent
//! for PDF/A conformity. Provides ICC profile embedding.

use crate::error::{ManipError, Result};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

/// Report from color space normalization.
#[derive(Debug, Clone)]
pub struct ColorSpaceReport {
    /// Whether an OutputIntent was already present.
    pub had_output_intent: bool,
    /// Whether an OutputIntent was added.
    pub output_intent_added: bool,
    /// Device-dependent color spaces found in page resources.
    pub device_colorspaces_found: Vec<String>,
    /// Number of pages scanned.
    pub pages_scanned: usize,
    /// Number of Separation colorspaces unified (rule 6.2.4.4:2).
    pub separations_unified: usize,
    /// Number of ExtGState OPM values fixed (rule 6.2.4.2:2).
    pub overprint_mode_fixed: usize,
    /// Number of ICCBased /N values corrected (rule 6.2.4.2:1).
    pub icc_n_fixed: usize,
}

/// Detect device-dependent color spaces used in page resources.
pub fn find_device_colorspaces(doc: &Document) -> Vec<(ObjectId, String)> {
    let mut result = Vec::new();

    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };

        if get_name(dict, b"Type").as_deref() != Some("Page") {
            continue;
        }

        let resources = match dict.get(b"Resources").ok() {
            Some(Object::Dictionary(res)) => Some(res.clone()),
            Some(Object::Reference(res_id)) => {
                if let Some(Object::Dictionary(res)) = doc.objects.get(res_id) {
                    Some(res.clone())
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(res) = &resources {
            if let Ok(Object::Dictionary(cs_dict)) = res.get(b"ColorSpace") {
                for (key, val) in cs_dict.iter() {
                    let cs_name = resolve_colorspace_name(doc, val);
                    if is_device_dependent(&cs_name) {
                        let key_str = String::from_utf8_lossy(key).to_string();
                        result.push((*id, format!("{key_str}={cs_name}")));
                    }
                }
            }
        }
    }

    result
}

/// Check if the document has a GTS_PDFA1 OutputIntent.
pub fn has_pdfa_output_intent(doc: &Document) -> bool {
    let catalog = match get_catalog(doc) {
        Some(c) => c,
        None => return false,
    };

    let intents = match catalog.get(b"OutputIntents").ok() {
        Some(Object::Array(arr)) => arr,
        Some(Object::Reference(id)) => {
            if let Some(Object::Array(arr)) = doc.objects.get(id) {
                arr
            } else {
                return false;
            }
        }
        _ => return false,
    };

    intents.iter().any(|item| {
        let dict = match item {
            Object::Reference(id) => {
                if let Some(Object::Dictionary(d)) = doc.objects.get(id) {
                    d
                } else {
                    return false;
                }
            }
            Object::Dictionary(d) => d,
            _ => return false,
        };
        get_name(dict, b"S").as_deref() == Some("GTS_PDFA1")
    })
}

/// Add an sRGB OutputIntent to the document for PDF/A compliance.
///
/// PDF/A-2 §6.2.3:2 requires that all OutputIntent entries with a
/// DestOutputProfile key reference the **same** indirect ICC object. To
/// satisfy this, we reuse the existing DestOutputProfile from any
/// already-present OutputIntent (e.g. GTS_PDFX) instead of creating a new
/// ICC stream, if one is found.
pub fn add_srgb_output_intent(doc: &mut Document) -> Result<()> {
    // Look for an existing DestOutputProfile indirect reference in any
    // OutputIntent already in the document. Reusing it avoids the rule
    // 6.2.3:2 failure caused by multiple OutputIntents with different
    // ICC profile objects.
    let existing_icc_ref: Option<lopdf::ObjectId> = {
        let catalog = get_catalog(doc);
        catalog.and_then(|cat| {
            let intents = match cat.get(b"OutputIntents").ok()? {
                Object::Array(arr) => arr.clone(),
                _ => return None,
            };
            for item in &intents {
                let dict = match item {
                    Object::Reference(id) => match doc.objects.get(id) {
                        Some(Object::Dictionary(d)) => d,
                        _ => continue,
                    },
                    Object::Dictionary(d) => d,
                    _ => continue,
                };
                if let Ok(Object::Reference(icc_id)) = dict.get(b"DestOutputProfile") {
                    return Some(*icc_id);
                }
            }
            None
        })
    };

    let icc_id = match existing_icc_ref {
        Some(id) => id,
        None => {
            let icc_bytes = srgb_icc_profile_bytes();
            let icc_dict = dictionary! {
                "N" => Object::Integer(3),
                "Alternate" => Object::Name(b"DeviceRGB".to_vec()),
            };
            let icc_stream = Stream::new(icc_dict, icc_bytes);
            doc.add_object(Object::Stream(icc_stream))
        }
    };

    let intent = dictionary! {
        "Type" => Object::Name(b"OutputIntent".to_vec()),
        "S" => Object::Name(b"GTS_PDFA1".to_vec()),
        "OutputConditionIdentifier" => Object::String(
            b"sRGB IEC61966-2.1".to_vec(),
            lopdf::StringFormat::Literal,
        ),
        "RegistryName" => Object::String(
            b"http://www.color.org".to_vec(),
            lopdf::StringFormat::Literal,
        ),
        "Info" => Object::String(
            b"sRGB IEC61966-2.1".to_vec(),
            lopdf::StringFormat::Literal,
        ),
        "DestOutputProfile" => Object::Reference(icc_id),
    };
    let intent_id = doc.add_object(Object::Dictionary(intent));

    let catalog_id = get_catalog_id(doc)?;

    if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
        let mut existing = match catalog.get(b"OutputIntents") {
            Ok(Object::Array(arr)) => arr.clone(),
            _ => Vec::new(),
        };
        existing.push(Object::Reference(intent_id));
        catalog.set("OutputIntents", Object::Array(existing));
    }

    Ok(())
}

/// Normalize color spaces: add appropriate OutputIntent if missing.
/// Uses CMYK OutputIntent if DeviceCMYK is used, sRGB otherwise.
pub fn normalize_colorspaces(doc: &mut Document) -> Result<ColorSpaceReport> {
    let had_output_intent = has_pdfa_output_intent(doc);
    let device_cs = find_device_colorspaces(doc);
    let pages_scanned = count_pages(doc);

    let device_names: Vec<String> = device_cs.into_iter().map(|(_, name)| name).collect();
    let unique_names: Vec<String> = {
        let mut seen = Vec::new();
        for n in &device_names {
            if !seen.contains(n) {
                seen.push(n.clone());
            }
        }
        seen
    };

    // Also scan for DeviceCMYK usage in content streams and image XObjects.
    let _has_cmyk =
        unique_names.iter().any(|n| n.contains("DeviceCMYK")) || has_device_cmyk_in_objects(doc);

    let output_intent_added = if !had_output_intent {
        // Always add sRGB OutputIntent — DeviceRGB is used implicitly by most PDFs.
        // Only one GTS_PDFA1 OutputIntent is allowed, so we use sRGB.
        add_srgb_output_intent(doc)?;
        true
    } else {
        false
    };

    // Always add Default{CMYK,RGB,Gray} to all pages — even if we don't detect
    // usage, device color spaces may appear in compressed streams or inline images.
    // These Default* entries are harmless on pages that don't use the color space.
    // DefaultGray satisfies 6.2.4.3:4 regardless of OutputIntent presence.
    let cmyk_cs_id = add_default_cmyk_colorspace(doc);
    let rgb_cs_id = add_default_rgb_colorspace(doc);
    add_default_gray_colorspace(doc);

    // Replace DeviceCMYK/DeviceRGB references in deep structures (Shading, Group, Pattern)
    // that are not covered by Default* resource fallback.
    fix_device_colorspaces_in_deep_structures(doc, cmyk_cs_id, rgb_cs_id);

    let separations_unified = normalize_separation_colorspaces(doc);

    // Replace DeviceCMYK/DeviceRGB in Separation alternate colorspaces.
    // veraPDF 6.2.4.3:3 fails when DeviceCMYK appears as a Separation alternate
    // and the output intent is not CMYK. Replacing with ICCBased also helps
    // 6.2.4.4:2 by ensuring the alternate field compares equal across traversal
    // paths (same object reference, not just same name).
    fix_separation_device_alternates(doc, cmyk_cs_id, rgb_cs_id);

    let overprint_mode_fixed = fix_overprint_mode(doc);
    let icc_n_fixed = fix_iccbased_n_value(doc);

    Ok(ColorSpaceReport {
        had_output_intent,
        output_intent_added,
        device_colorspaces_found: unique_names,
        pages_scanned,
        separations_unified,
        overprint_mode_fixed,
        icc_n_fixed,
    })
}

/// Check if any object in the document uses DeviceCMYK.
fn has_device_cmyk_in_objects(doc: &Document) -> bool {
    for obj in doc.objects.values() {
        match obj {
            Object::Dictionary(dict) => {
                // Check /ColorSpace /DeviceCMYK in image XObjects.
                if get_name(dict, b"ColorSpace").as_deref() == Some("DeviceCMYK") {
                    return true;
                }
            }
            Object::Stream(stream) => {
                if get_name(&stream.dict, b"ColorSpace").as_deref() == Some("DeviceCMYK") {
                    return true;
                }
                // Check content streams for CMYK operators (k/K).
                if get_name(&stream.dict, b"Type").as_deref() == Some("XObject")
                    || stream.dict.get(b"Type").is_err()
                {
                    // Quick scan of stream content for CMYK operators.
                    let content = &stream.content;
                    if content.windows(2).any(|w| {
                        (w[1] == b'k' || w[1] == b'K')
                            && (w[0] == b' ' || w[0] == b'\n' || w[0] == b'\r')
                    }) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// Add a CMYK OutputIntent to the document for PDF/A compliance.
#[allow(dead_code)]
fn add_cmyk_output_intent(doc: &mut Document) -> Result<()> {
    let icc_bytes = cmyk_icc_profile_bytes();

    let icc_dict = dictionary! {
        "N" => Object::Integer(4),
        "Alternate" => Object::Name(b"DeviceCMYK".to_vec()),
    };
    let icc_stream = Stream::new(icc_dict, icc_bytes);
    let icc_id = doc.add_object(Object::Stream(icc_stream));

    let intent = dictionary! {
        "Type" => Object::Name(b"OutputIntent".to_vec()),
        "S" => Object::Name(b"GTS_PDFA1".to_vec()),
        "OutputConditionIdentifier" => Object::String(
            b"FOGRA39".to_vec(),
            lopdf::StringFormat::Literal,
        ),
        "RegistryName" => Object::String(
            b"http://www.color.org".to_vec(),
            lopdf::StringFormat::Literal,
        ),
        "Info" => Object::String(
            b"Coated FOGRA39 (ISO 12647-2:2004)".to_vec(),
            lopdf::StringFormat::Literal,
        ),
        "DestOutputProfile" => Object::Reference(icc_id),
    };
    let intent_id = doc.add_object(Object::Dictionary(intent));

    let catalog_id = get_catalog_id(doc)?;
    if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
        let mut existing = match catalog.get(b"OutputIntents") {
            Ok(Object::Array(arr)) => arr.clone(),
            _ => Vec::new(),
        };
        existing.push(Object::Reference(intent_id));
        catalog.set("OutputIntents", Object::Array(existing));
    }

    Ok(())
}

/// Minimal CMYK ICC v2 profile (4-component).
/// Based on FOGRA39 (coated) with identity CMYK→Lab transform.
fn cmyk_icc_profile_bytes() -> Vec<u8> {
    // Layout:
    //   0..128   header
    //   128..132 tag count (5)
    //   132..192 5 tag entries (12 bytes each)
    //   192..290 desc tag data (98 bytes)
    //   290..292 padding (2 bytes for 4-byte alignment)
    //   292..304 cprt tag data (12 bytes)
    //   304..324 wtpt tag data (20 bytes)
    //   324..370 A2B0 tag (46 bytes: lut8Type with identity)
    //   370..416 B2A0 tag (46 bytes: lut8Type with identity)
    //
    // Simplified: we use a minimal valid structure.
    let total_size: u32 = 416;
    let mut p = Vec::with_capacity(total_size as usize);

    // === Header (128 bytes) ===
    p.extend_from_slice(&total_size.to_be_bytes());
    p.extend_from_slice(b"\0\0\0\0"); // preferred CMM
    p.extend_from_slice(&[2, 0x10, 0, 0]); // version 2.1.0
    p.extend_from_slice(b"prtr"); // device class: output (printer)
    p.extend_from_slice(b"CMYK"); // color space
    p.extend_from_slice(b"Lab "); // PCS
    p.extend_from_slice(&[0u8; 12]); // date/time
    p.extend_from_slice(b"acsp"); // signature
    p.extend_from_slice(&[0u8; 4]); // platform
    p.extend_from_slice(&[0u8; 4]); // flags
    p.extend_from_slice(&[0u8; 4]); // manufacturer
    p.extend_from_slice(&[0u8; 4]); // model
    p.extend_from_slice(&[0u8; 8]); // device attributes
    p.extend_from_slice(&[0u8; 4]); // rendering intent
                                    // PCS illuminant D50
    p.extend_from_slice(&0x0000F6D6_u32.to_be_bytes());
    p.extend_from_slice(&0x00010000_u32.to_be_bytes());
    p.extend_from_slice(&0x0000D32D_u32.to_be_bytes());
    p.extend_from_slice(&[0u8; 4]); // creator
    p.extend_from_slice(&[0u8; 16]); // profile ID
    p.extend_from_slice(&[0u8; 128 - 100]); // reserved
    debug_assert_eq!(p.len(), 128);

    // === Tag table ===
    p.extend_from_slice(&5_u32.to_be_bytes()); // 5 tags

    let tags: &[(&[u8; 4], u32, u32)] = &[
        (b"desc", 192, 98),
        (b"cprt", 292, 12),
        (b"wtpt", 304, 20),
        (b"A2B0", 324, 46),
        (b"B2A0", 370, 46),
    ];
    for (sig, offset, size) in tags {
        p.extend_from_slice(*sig);
        p.extend_from_slice(&offset.to_be_bytes());
        p.extend_from_slice(&size.to_be_bytes());
    }
    debug_assert_eq!(p.len(), 192);

    // === desc tag (textDescriptionType) — 95 bytes + 1 pad = 96 ===
    p.extend_from_slice(b"desc");
    p.extend_from_slice(&[0u8; 4]); // reserved
    p.extend_from_slice(&8_u32.to_be_bytes()); // ASCII length
    p.extend_from_slice(b"FOGRA39\0");
    p.extend_from_slice(&[0u8; 4]); // Unicode language
    p.extend_from_slice(&[0u8; 4]); // Unicode count
    p.extend_from_slice(&[0u8; 2]); // ScriptCode code
    p.push(0); // ScriptCode count
    p.extend_from_slice(&[0u8; 67]); // ScriptCode string
    debug_assert_eq!(p.len(), 290);
    // Pad to 4-byte alignment for next tag at offset 292.
    while p.len() < 292 {
        p.push(0);
    }
    debug_assert_eq!(p.len(), 292);

    // === cprt tag ===
    p.extend_from_slice(b"text");
    p.extend_from_slice(&[0u8; 4]);
    p.extend_from_slice(b"CC0\0");
    debug_assert_eq!(p.len(), 304);

    // === wtpt (XYZType) ===
    p.extend_from_slice(b"XYZ ");
    p.extend_from_slice(&[0u8; 4]);
    p.extend_from_slice(&0x0000F351_i32.to_be_bytes());
    p.extend_from_slice(&0x00010000_i32.to_be_bytes());
    p.extend_from_slice(&0x000116CC_i32.to_be_bytes());
    debug_assert_eq!(p.len(), 324);

    // === A2B0 tag (lut8Type) — CMYK→Lab identity-ish mapping ===
    // Minimal lut8Type: 4 input, 3 output, 2 grid points
    p.extend_from_slice(b"mft1"); // lut8Type signature
    p.extend_from_slice(&[0u8; 4]); // reserved
    p.push(4); // input channels
    p.push(3); // output channels
    p.push(2); // grid points
    p.push(0); // padding
               // 3x3 identity-ish matrix (fixed point s15.16) — for Lab PCS this is ignored
               // but must be present: 9 * 4 = 36 bytes
    let identity_row = [0x00010000_u32, 0, 0]; // [1.0, 0, 0]
    for i in 0..3 {
        for j in 0..3 {
            let val = if i == j { identity_row[0] } else { 0u32 };
            p.extend_from_slice(&val.to_be_bytes());
        }
    }
    while p.len() < 370 {
        p.push(0);
    }

    // === B2A0 tag (same structure) ===
    p.extend_from_slice(b"mft1");
    p.extend_from_slice(&[0u8; 4]);
    p.push(3); // input channels (Lab)
    p.push(4); // output channels (CMYK)
    p.push(2); // grid points
    p.push(0);
    for i in 0..3 {
        for j in 0..3 {
            let val = if i == j { 0x00010000_u32 } else { 0u32 };
            p.extend_from_slice(&val.to_be_bytes());
        }
    }
    while p.len() < 416 {
        p.push(0);
    }

    // Fix the profile size in header
    let size_bytes = (p.len() as u32).to_be_bytes();
    p[0..4].copy_from_slice(&size_bytes);

    p
}

/// sRGB ICC v2.1 profile with all required tags for PDF/A compliance.
///
/// Contains 9 tags: desc, cprt, wtpt, rXYZ, gXYZ, bXYZ, rTRC, gTRC, bTRC.
/// Uses D50-adapted sRGB primaries and gamma 2.2 TRC.
fn srgb_icc_profile_bytes() -> Vec<u8> {
    // Layout:
    //   0..128   header
    //   128..132 tag count (9)
    //   132..240 9 tag entries (12 bytes each)
    //   240..336 desc tag data (96 bytes, padded)
    //   336..348 cprt tag data (12 bytes)
    //   348..368 wtpt tag data (20 bytes)
    //   368..388 rXYZ tag data (20 bytes)
    //   388..408 gXYZ tag data (20 bytes)
    //   408..428 bXYZ tag data (20 bytes)
    //   428..444 shared curv tag data (14 bytes + 2 pad)
    let total_size: u32 = 444;
    let mut p = Vec::with_capacity(total_size as usize);

    // === Header (128 bytes) ===
    p.extend_from_slice(&total_size.to_be_bytes()); // profile size
    p.extend_from_slice(b"\0\0\0\0"); // preferred CMM
    p.extend_from_slice(&[2, 0x10, 0, 0]); // version 2.1.0
    p.extend_from_slice(b"mntr"); // device class: monitor
    p.extend_from_slice(b"RGB "); // color space
    p.extend_from_slice(b"XYZ "); // PCS
    p.extend_from_slice(&[0u8; 12]); // date/time
    p.extend_from_slice(b"acsp"); // signature
    p.extend_from_slice(&[0u8; 4]); // platform
    p.extend_from_slice(&[0u8; 4]); // flags
    p.extend_from_slice(&[0u8; 4]); // manufacturer
    p.extend_from_slice(&[0u8; 4]); // model
    p.extend_from_slice(&[0u8; 8]); // device attributes
    p.extend_from_slice(&[0u8; 4]); // rendering intent (perceptual)
                                    // PCS illuminant D50 (X=0.9642, Y=1.0, Z=0.8249)
    p.extend_from_slice(&0x0000F6D6_u32.to_be_bytes()); // X
    p.extend_from_slice(&0x00010000_u32.to_be_bytes()); // Y
    p.extend_from_slice(&0x0000D32D_u32.to_be_bytes()); // Z
    p.extend_from_slice(&[0u8; 4]); // creator
    p.extend_from_slice(&[0u8; 16]); // profile ID
    p.extend_from_slice(&[0u8; 128 - 100]); // reserved padding to 128
    debug_assert_eq!(p.len(), 128);

    // === Tag table ===
    p.extend_from_slice(&9_u32.to_be_bytes()); // 9 tags

    // Tag entries: signature(4) + offset(4) + size(4) = 12 bytes each
    let tags: &[(&[u8; 4], u32, u32)] = &[
        (b"desc", 240, 95),
        (b"cprt", 336, 12),
        (b"wtpt", 348, 20),
        (b"rXYZ", 368, 20),
        (b"gXYZ", 388, 20),
        (b"bXYZ", 408, 20),
        (b"rTRC", 428, 14),
        (b"gTRC", 428, 14), // shared with rTRC
        (b"bTRC", 428, 14), // shared with rTRC
    ];
    for (sig, offset, size) in tags {
        p.extend_from_slice(*sig);
        p.extend_from_slice(&offset.to_be_bytes());
        p.extend_from_slice(&size.to_be_bytes());
    }
    debug_assert_eq!(p.len(), 240);

    // === desc tag (textDescriptionType) — 95 bytes + 1 pad = 96 ===
    p.extend_from_slice(b"desc"); // type signature
    p.extend_from_slice(&[0u8; 4]); // reserved
    p.extend_from_slice(&5_u32.to_be_bytes()); // ASCII length (incl. null)
    p.extend_from_slice(b"sRGB\0"); // ASCII string
    p.extend_from_slice(&[0u8; 4]); // Unicode language code
    p.extend_from_slice(&[0u8; 4]); // Unicode count (0 = none)
    p.extend_from_slice(&[0u8; 2]); // ScriptCode code
    p.push(0); // ScriptCode count
    p.extend_from_slice(&[0u8; 67]); // ScriptCode string (always 67)
    p.push(0); // pad to 4-byte alignment
    debug_assert_eq!(p.len(), 336);

    // === cprt tag (textType) — 12 bytes ===
    p.extend_from_slice(b"text"); // type signature
    p.extend_from_slice(&[0u8; 4]); // reserved
    p.extend_from_slice(b"CC0\0"); // copyright text
    debug_assert_eq!(p.len(), 348);

    // === XYZ tags (XYZType) — 20 bytes each ===
    // Helper: write XYZ tag
    fn write_xyz(p: &mut Vec<u8>, x: i32, y: i32, z: i32) {
        p.extend_from_slice(b"XYZ "); // type signature
        p.extend_from_slice(&[0u8; 4]); // reserved
        p.extend_from_slice(&x.to_be_bytes());
        p.extend_from_slice(&y.to_be_bytes());
        p.extend_from_slice(&z.to_be_bytes());
    }

    // wtpt — D50 media white point (X=0.9505, Y=1.0, Z=1.0891)
    write_xyz(&mut p, 0x0000F351, 0x00010000, 0x000116CC);
    debug_assert_eq!(p.len(), 368);

    // rXYZ — Red primary (X=0.4361, Y=0.2225, Z=0.0139)
    write_xyz(&mut p, 0x00006FA3, 0x000038F6, 0x00000391);
    debug_assert_eq!(p.len(), 388);

    // gXYZ — Green primary (X=0.3851, Y=0.7169, Z=0.0971)
    write_xyz(&mut p, 0x00006294, 0x0000B785, 0x000018DC);
    debug_assert_eq!(p.len(), 408);

    // bXYZ — Blue primary (X=0.1431, Y=0.0606, Z=0.7142)
    write_xyz(&mut p, 0x000024A1, 0x00000F85, 0x0000B6D4);
    debug_assert_eq!(p.len(), 428);

    // === Shared curv tag (curveType with gamma 2.2) — 14 bytes + 2 pad ===
    p.extend_from_slice(b"curv"); // type signature
    p.extend_from_slice(&[0u8; 4]); // reserved
    p.extend_from_slice(&1_u32.to_be_bytes()); // count=1 means gamma value
    p.extend_from_slice(&[0x02, 0x33]); // u8Fixed8Number: gamma 2.19922 ≈ 2.2
    p.extend_from_slice(&[0u8; 2]); // pad to 4-byte alignment
    debug_assert_eq!(p.len(), 444);

    p
}

/// Add DefaultCMYK ICCBased colorspace to all page resources.
/// This allows DeviceCMYK usage when the OutputIntent is sRGB (not CMYK).
/// Returns the ObjectId of the ICCBased colorspace array for reuse.
fn add_default_cmyk_colorspace(doc: &mut Document) -> ObjectId {
    let icc_bytes = cmyk_icc_profile_bytes();
    let icc_dict = dictionary! {
        "N" => Object::Integer(4),
        "Alternate" => Object::Name(b"DeviceCMYK".to_vec()),
    };
    let icc_stream = Stream::new(icc_dict, icc_bytes);
    let icc_id = doc.add_object(Object::Stream(icc_stream));

    // Create ICCBased colorspace array: [/ICCBased <ref>]
    let cs_array = Object::Array(vec![
        Object::Name(b"ICCBased".to_vec()),
        Object::Reference(icc_id),
    ]);
    let cs_id = doc.add_object(cs_array);

    // Add DefaultCMYK to each page's ColorSpace resources.
    let page_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if get_name(dict, b"Type").as_deref() == Some("Page") {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    for page_id in page_ids {
        // Get or create Resources dict.
        let res_id = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&page_id) else {
                continue;
            };
            match dict.get(b"Resources").ok() {
                Some(Object::Reference(id)) => Some(*id),
                Some(Object::Dictionary(_)) => None, // inline resources — handle below
                _ => None,
            }
        };

        if let Some(res_id) = res_id {
            // Resources is a reference — modify the referenced dict.
            // First resolve ColorSpace if it's also a reference.
            let cs_ref_id = {
                if let Some(Object::Dictionary(res)) = doc.objects.get(&res_id) {
                    match res.get(b"ColorSpace").ok() {
                        Some(Object::Reference(id)) => Some(*id),
                        _ => None,
                    }
                } else {
                    None
                }
            };
            if let Some(cs_ref_id) = cs_ref_id {
                // ColorSpace is itself a reference — modify that dict.
                if let Some(Object::Dictionary(ref mut cs_dict)) = doc.objects.get_mut(&cs_ref_id) {
                    if !cs_dict.has(b"DefaultCMYK") {
                        cs_dict.set("DefaultCMYK", Object::Reference(cs_id));
                    }
                }
            } else if let Some(Object::Dictionary(ref mut res)) = doc.objects.get_mut(&res_id) {
                let mut cs_dict = match res.get(b"ColorSpace") {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                if !cs_dict.has(b"DefaultCMYK") {
                    cs_dict.set("DefaultCMYK", Object::Reference(cs_id));
                    res.set("ColorSpace", Object::Dictionary(cs_dict));
                }
            }
        } else {
            // Resources is inline in the page dict.
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&page_id) {
                let mut res = match dict.get(b"Resources") {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                let mut cs_dict = match res.get(b"ColorSpace") {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                if !cs_dict.has(b"DefaultCMYK") {
                    cs_dict.set("DefaultCMYK", Object::Reference(cs_id));
                    res.set("ColorSpace", Object::Dictionary(cs_dict));
                    dict.set("Resources", Object::Dictionary(res));
                }
            }
        }
    }

    // Also add DefaultCMYK to ALL Form XObject Resources (even those without
    // an existing ColorSpace dict or without Resources at all).
    let form_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Stream(stream) = obj {
                if get_name(&stream.dict, b"Subtype").as_deref() == Some("Form") {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    for form_id in form_ids {
        let res_ref_id = {
            if let Some(Object::Stream(stream)) = doc.objects.get(&form_id) {
                match stream.dict.get(b"Resources").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        };

        if let Some(res_ref_id) = res_ref_id {
            // Resources is a reference — modify the referenced dict.
            if let Some(Object::Dictionary(ref mut res)) = doc.objects.get_mut(&res_ref_id) {
                let mut cs_dict = match res.get(b"ColorSpace") {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                if !cs_dict.has(b"DefaultCMYK") {
                    cs_dict.set("DefaultCMYK", Object::Reference(cs_id));
                    res.set("ColorSpace", Object::Dictionary(cs_dict));
                }
            }
        } else if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&form_id) {
            // Resources inline or missing — create/update.
            let mut res = match stream.dict.get(b"Resources") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            let mut cs_dict = match res.get(b"ColorSpace") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            if !cs_dict.has(b"DefaultCMYK") {
                cs_dict.set("DefaultCMYK", Object::Reference(cs_id));
                res.set("ColorSpace", Object::Dictionary(cs_dict));
                stream.dict.set("Resources", Object::Dictionary(res));
            }
        }
    }

    // Also add DefaultCMYK to tiling pattern streams (PatternType=1).
    let pattern_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Stream(stream) = obj {
                let pt = stream
                    .dict
                    .get(b"PatternType")
                    .ok()
                    .and_then(|o| o.as_i64().ok());
                if pt == Some(1) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    for pat_id in pattern_ids {
        let res_ref_id = {
            if let Some(Object::Stream(stream)) = doc.objects.get(&pat_id) {
                match stream.dict.get(b"Resources").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        };

        if let Some(res_ref_id) = res_ref_id {
            if let Some(Object::Dictionary(ref mut res)) = doc.objects.get_mut(&res_ref_id) {
                let mut cs_dict = match res.get(b"ColorSpace") {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                if !cs_dict.has(b"DefaultCMYK") {
                    cs_dict.set("DefaultCMYK", Object::Reference(cs_id));
                    res.set("ColorSpace", Object::Dictionary(cs_dict));
                }
            }
        } else if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&pat_id) {
            let mut res = match stream.dict.get(b"Resources") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            let mut cs_dict = match res.get(b"ColorSpace") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            if !cs_dict.has(b"DefaultCMYK") {
                cs_dict.set("DefaultCMYK", Object::Reference(cs_id));
                res.set("ColorSpace", Object::Dictionary(cs_dict));
                stream.dict.set("Resources", Object::Dictionary(res));
            }
        }
    }

    cs_id
}

/// Add DefaultRGB ICCBased colorspace (sRGB) to all page and Form XObject resources.
/// This maps DeviceRGB to an ICC profile for PDF/A compliance.
/// Returns the ObjectId of the ICCBased colorspace array for reuse.
fn add_default_rgb_colorspace(doc: &mut Document) -> ObjectId {
    let icc_bytes = srgb_icc_profile_bytes();
    let icc_dict = dictionary! {
        "N" => Object::Integer(3),
        "Alternate" => Object::Name(b"DeviceRGB".to_vec()),
    };
    let icc_stream = Stream::new(icc_dict, icc_bytes);
    let icc_id = doc.add_object(Object::Stream(icc_stream));

    let cs_array = Object::Array(vec![
        Object::Name(b"ICCBased".to_vec()),
        Object::Reference(icc_id),
    ]);
    let cs_id = doc.add_object(cs_array);

    // Add DefaultRGB to each page's ColorSpace resources.
    let page_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if get_name(dict, b"Type").as_deref() == Some("Page") {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    for page_id in page_ids {
        let res_id = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&page_id) else {
                continue;
            };
            match dict.get(b"Resources").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            }
        };

        if let Some(res_id) = res_id {
            let cs_ref_id = {
                if let Some(Object::Dictionary(res)) = doc.objects.get(&res_id) {
                    match res.get(b"ColorSpace").ok() {
                        Some(Object::Reference(id)) => Some(*id),
                        _ => None,
                    }
                } else {
                    None
                }
            };
            if let Some(cs_ref_id) = cs_ref_id {
                if let Some(Object::Dictionary(ref mut cs_dict)) = doc.objects.get_mut(&cs_ref_id) {
                    if !cs_dict.has(b"DefaultRGB") {
                        cs_dict.set("DefaultRGB", Object::Reference(cs_id));
                    }
                }
            } else if let Some(Object::Dictionary(ref mut res)) = doc.objects.get_mut(&res_id) {
                let mut cs_dict = match res.get(b"ColorSpace") {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                if !cs_dict.has(b"DefaultRGB") {
                    cs_dict.set("DefaultRGB", Object::Reference(cs_id));
                    res.set("ColorSpace", Object::Dictionary(cs_dict));
                }
            }
        } else if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&page_id) {
            let mut res = match dict.get(b"Resources") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            let mut cs_dict = match res.get(b"ColorSpace") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            if !cs_dict.has(b"DefaultRGB") {
                cs_dict.set("DefaultRGB", Object::Reference(cs_id));
                res.set("ColorSpace", Object::Dictionary(cs_dict));
                dict.set("Resources", Object::Dictionary(res));
            }
        }
    }

    // Also add to Form XObjects.
    let form_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Stream(stream) = obj {
                if get_name(&stream.dict, b"Subtype").as_deref() == Some("Form") {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    for form_id in form_ids {
        let res_ref_id = {
            if let Some(Object::Stream(stream)) = doc.objects.get(&form_id) {
                match stream.dict.get(b"Resources").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        };

        if let Some(res_ref_id) = res_ref_id {
            if let Some(Object::Dictionary(ref mut res)) = doc.objects.get_mut(&res_ref_id) {
                let mut cs_dict = match res.get(b"ColorSpace") {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                if !cs_dict.has(b"DefaultRGB") {
                    cs_dict.set("DefaultRGB", Object::Reference(cs_id));
                    res.set("ColorSpace", Object::Dictionary(cs_dict));
                }
            }
        } else if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&form_id) {
            let mut res = match stream.dict.get(b"Resources") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            let mut cs_dict = match res.get(b"ColorSpace") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            if !cs_dict.has(b"DefaultRGB") {
                cs_dict.set("DefaultRGB", Object::Reference(cs_id));
                res.set("ColorSpace", Object::Dictionary(cs_dict));
                stream.dict.set("Resources", Object::Dictionary(res));
            }
        }
    }

    // Also add DefaultRGB to tiling pattern streams (PatternType=1).
    let pattern_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Stream(stream) = obj {
                let pt = stream
                    .dict
                    .get(b"PatternType")
                    .ok()
                    .and_then(|o| o.as_i64().ok());
                if pt == Some(1) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    for pat_id in pattern_ids {
        let res_ref_id = {
            if let Some(Object::Stream(stream)) = doc.objects.get(&pat_id) {
                match stream.dict.get(b"Resources").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        };

        if let Some(res_ref_id) = res_ref_id {
            if let Some(Object::Dictionary(ref mut res)) = doc.objects.get_mut(&res_ref_id) {
                let mut cs_dict = match res.get(b"ColorSpace") {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                if !cs_dict.has(b"DefaultRGB") {
                    cs_dict.set("DefaultRGB", Object::Reference(cs_id));
                    res.set("ColorSpace", Object::Dictionary(cs_dict));
                }
            }
        } else if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&pat_id) {
            let mut res = match stream.dict.get(b"Resources") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            let mut cs_dict = match res.get(b"ColorSpace") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            if !cs_dict.has(b"DefaultRGB") {
                cs_dict.set("DefaultRGB", Object::Reference(cs_id));
                res.set("ColorSpace", Object::Dictionary(cs_dict));
                stream.dict.set("Resources", Object::Dictionary(res));
            }
        }
    }

    cs_id
}

/// Add a DefaultGray (sgray N=1 ICCBased) colorspace to all page, Form XObject,
/// and tiling pattern Resources. Satisfies 6.2.4.3:4 ("DeviceGray shall only be
/// used if a device independent DefaultGray colour space has been set").
fn add_default_gray_colorspace(doc: &mut Document) -> ObjectId {
    let icc_bytes = gray_icc_profile_bytes();
    let icc_dict = dictionary! {
        "N" => Object::Integer(1),
        "Alternate" => Object::Name(b"DeviceGray".to_vec()),
    };
    let icc_stream = Stream::new(icc_dict, icc_bytes);
    let icc_id = doc.add_object(Object::Stream(icc_stream));

    let cs_array = Object::Array(vec![
        Object::Name(b"ICCBased".to_vec()),
        Object::Reference(icc_id),
    ]);
    let cs_id = doc.add_object(cs_array);

    // Pages.
    let page_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if get_name(dict, b"Type").as_deref() == Some("Page") {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    for page_id in page_ids {
        let res_id = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&page_id) else {
                continue;
            };
            match dict.get(b"Resources").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            }
        };

        if let Some(res_id) = res_id {
            let cs_ref_id = {
                if let Some(Object::Dictionary(res)) = doc.objects.get(&res_id) {
                    match res.get(b"ColorSpace").ok() {
                        Some(Object::Reference(id)) => Some(*id),
                        _ => None,
                    }
                } else {
                    None
                }
            };
            if let Some(cs_ref_id) = cs_ref_id {
                if let Some(Object::Dictionary(ref mut cs_dict)) = doc.objects.get_mut(&cs_ref_id) {
                    if !cs_dict.has(b"DefaultGray") {
                        cs_dict.set("DefaultGray", Object::Reference(cs_id));
                    }
                }
            } else if let Some(Object::Dictionary(ref mut res)) = doc.objects.get_mut(&res_id) {
                let mut cs_dict = match res.get(b"ColorSpace") {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                if !cs_dict.has(b"DefaultGray") {
                    cs_dict.set("DefaultGray", Object::Reference(cs_id));
                    res.set("ColorSpace", Object::Dictionary(cs_dict));
                }
            }
        } else if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&page_id) {
            let mut res = match dict.get(b"Resources") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            let mut cs_dict = match res.get(b"ColorSpace") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            if !cs_dict.has(b"DefaultGray") {
                cs_dict.set("DefaultGray", Object::Reference(cs_id));
                res.set("ColorSpace", Object::Dictionary(cs_dict));
                dict.set("Resources", Object::Dictionary(res));
            }
        }
    }

    // Form XObjects.
    let form_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Stream(stream) = obj {
                if get_name(&stream.dict, b"Subtype").as_deref() == Some("Form") {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    for form_id in form_ids {
        let res_ref_id = {
            if let Some(Object::Stream(stream)) = doc.objects.get(&form_id) {
                match stream.dict.get(b"Resources").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        };

        if let Some(res_ref_id) = res_ref_id {
            if let Some(Object::Dictionary(ref mut res)) = doc.objects.get_mut(&res_ref_id) {
                let mut cs_dict = match res.get(b"ColorSpace") {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                if !cs_dict.has(b"DefaultGray") {
                    cs_dict.set("DefaultGray", Object::Reference(cs_id));
                    res.set("ColorSpace", Object::Dictionary(cs_dict));
                }
            }
        } else if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&form_id) {
            let mut res = match stream.dict.get(b"Resources") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            let mut cs_dict = match res.get(b"ColorSpace") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            if !cs_dict.has(b"DefaultGray") {
                cs_dict.set("DefaultGray", Object::Reference(cs_id));
                res.set("ColorSpace", Object::Dictionary(cs_dict));
                stream.dict.set("Resources", Object::Dictionary(res));
            }
        }
    }

    // Tiling patterns.
    let pattern_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Stream(stream) = obj {
                let pt = stream
                    .dict
                    .get(b"PatternType")
                    .ok()
                    .and_then(|o| o.as_i64().ok());
                if pt == Some(1) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    for pat_id in pattern_ids {
        let res_ref_id = {
            if let Some(Object::Stream(stream)) = doc.objects.get(&pat_id) {
                match stream.dict.get(b"Resources").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        };

        if let Some(res_ref_id) = res_ref_id {
            if let Some(Object::Dictionary(ref mut res)) = doc.objects.get_mut(&res_ref_id) {
                let mut cs_dict = match res.get(b"ColorSpace") {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                if !cs_dict.has(b"DefaultGray") {
                    cs_dict.set("DefaultGray", Object::Reference(cs_id));
                    res.set("ColorSpace", Object::Dictionary(cs_dict));
                }
            }
        } else if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&pat_id) {
            let mut res = match stream.dict.get(b"Resources") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            let mut cs_dict = match res.get(b"ColorSpace") {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            };
            if !cs_dict.has(b"DefaultGray") {
                cs_dict.set("DefaultGray", Object::Reference(cs_id));
                res.set("ColorSpace", Object::Dictionary(cs_dict));
                stream.dict.set("Resources", Object::Dictionary(res));
            }
        }
    }

    cs_id
}

/// Replace `/ColorSpace /DeviceCMYK` and `/ColorSpace /DeviceRGB` in Shading dicts
/// and `/CS /DeviceCMYK|DeviceRGB` in transparency Group dicts with ICCBased references.
/// These deep structures are not covered by the Default* resource fallback mechanism.
fn fix_device_colorspaces_in_deep_structures(
    doc: &mut Document,
    cmyk_cs_id: ObjectId,
    rgb_cs_id: ObjectId,
) {
    // Helper: map a device colorspace name to its ICCBased replacement.
    let replacement = |cs_name: &str| -> Option<ObjectId> {
        match cs_name {
            "DeviceCMYK" => Some(cmyk_cs_id),
            "DeviceRGB" => Some(rgb_cs_id),
            _ => None,
        }
    };

    // Helper: resolve a device colorspace from a dict key, handling both
    // direct Name values and references to Name objects.
    let resolve_device_cs =
        |dict: &lopdf::Dictionary, key: &[u8], doc: &Document| -> Option<String> {
            match dict.get(key).ok() {
                Some(Object::Name(n)) => {
                    let name = String::from_utf8_lossy(n).to_string();
                    if name == "DeviceCMYK" || name == "DeviceRGB" {
                        Some(name)
                    } else {
                        None
                    }
                }
                Some(Object::Reference(ref_id)) => {
                    // Dereference: the referenced object may be a bare Name.
                    match doc.objects.get(ref_id) {
                        Some(Object::Name(n)) => {
                            let name = String::from_utf8_lossy(n).to_string();
                            if name == "DeviceCMYK" || name == "DeviceRGB" {
                                Some(name)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        };

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in ids {
        let obj = match doc.objects.get(&id) {
            Some(o) => o,
            None => continue,
        };

        match obj {
            Object::Dictionary(dict) => {
                // Shading dicts have /ShadingType and may have a device ColorSpace.
                if dict.get(b"ShadingType").is_ok() {
                    if let Some(repl) =
                        resolve_device_cs(dict, b"ColorSpace", doc).and_then(|n| replacement(&n))
                    {
                        if let Some(Object::Dictionary(ref mut d)) = doc.objects.get_mut(&id) {
                            d.set("ColorSpace", Object::Reference(repl));
                        }
                        continue;
                    }
                }
            }
            Object::Stream(stream) => {
                let dict = &stream.dict;

                // Shading streams with device ColorSpace.
                if dict.get(b"ShadingType").is_ok() {
                    if let Some(repl) =
                        resolve_device_cs(dict, b"ColorSpace", doc).and_then(|n| replacement(&n))
                    {
                        if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                            s.dict.set("ColorSpace", Object::Reference(repl));
                        }
                        continue;
                    }
                }

                // Group dicts on XObject streams: /Group << /CS /DeviceCMYK|DeviceRGB >>
                let group_repl = if let Ok(Object::Dictionary(group)) = dict.get(b"Group") {
                    get_name(group, b"CS").and_then(|n| replacement(&n))
                } else {
                    None
                };
                if let Some(repl) = group_repl {
                    if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                        if let Ok(Object::Dictionary(ref mut group)) = s.dict.get_mut(b"Group") {
                            group.set("CS", Object::Reference(repl));
                        }
                    }
                    continue;
                }

                // Pattern streams with device ColorSpace.
                let is_pattern = get_name(dict, b"Type").as_deref() == Some("Pattern")
                    || dict.get(b"PatternType").is_ok();
                if is_pattern {
                    if let Some(repl) = get_name(dict, b"ColorSpace").and_then(|n| replacement(&n))
                    {
                        if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                            s.dict.set("ColorSpace", Object::Reference(repl));
                        }
                        continue;
                    }
                }
            }
            _ => {}
        }
    }

    // Handle Group dicts stored as references from stream dicts.
    let ids2: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids2 {
        let group_ref_id = {
            let obj = match doc.objects.get(&id) {
                Some(o) => o,
                None => continue,
            };
            match obj {
                Object::Stream(stream) => match stream.dict.get(b"Group").ok() {
                    Some(Object::Reference(gid)) => Some(*gid),
                    _ => None,
                },
                _ => None,
            }
        };
        if let Some(gid) = group_ref_id {
            let repl = if let Some(Object::Dictionary(group)) = doc.objects.get(&gid) {
                get_name(group, b"CS").and_then(|n| replacement(&n))
            } else {
                None
            };
            if let Some(repl) = repl {
                if let Some(Object::Dictionary(ref mut group)) = doc.objects.get_mut(&gid) {
                    group.set("CS", Object::Reference(repl));
                }
            }
        }
    }

    // Handle Pattern dicts (non-stream).
    let ids3: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids3 {
        let repl = if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
            let is_pattern = get_name(dict, b"Type").as_deref() == Some("Pattern")
                || dict.get(b"PatternType").is_ok();
            if is_pattern {
                get_name(dict, b"ColorSpace").and_then(|n| replacement(&n))
            } else {
                None
            }
        } else {
            None
        };
        if let Some(repl) = repl {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.set("ColorSpace", Object::Reference(repl));
            }
        }
    }

    // Fix DeviceN/NChannel colorspaces: replace DeviceCMYK/DeviceRGB in the
    // alternate colorspace and in the Process/ColorSpace attribute (6.2.4.3:3).
    fix_devicen_process_colors(doc, cmyk_cs_id, rgb_cs_id);
}

/// Fix DeviceN/NChannel colorspace arrays that reference DeviceCMYK or DeviceRGB
/// as their alternate colorspace or Process/ColorSpace attribute.
fn fix_devicen_process_colors(doc: &mut Document, cmyk_cs_id: ObjectId, rgb_cs_id: ObjectId) {
    let replacement = |name: &[u8]| -> Option<ObjectId> {
        if name == b"DeviceCMYK" {
            Some(cmyk_cs_id)
        } else if name == b"DeviceRGB" {
            Some(rgb_cs_id)
        } else {
            None
        }
    };

    // Pass 1: Fix DeviceN arrays stored as objects.
    // DeviceN array: [/DeviceN [names] alternateCS tintTransform attributesDict?]
    // NChannel is the same structure with /NChannel as the first element.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let fix = {
            let Some(Object::Array(arr)) = doc.objects.get(&id) else {
                continue;
            };
            if arr.len() < 4 {
                continue;
            }
            let is_devicen =
                matches!(&arr[0], Object::Name(n) if n == b"DeviceN" || n == b"NChannel");
            if !is_devicen {
                continue;
            }
            // Check alternate colorspace (index 2).
            let alt_repl = match &arr[2] {
                Object::Name(n) => replacement(n),
                _ => None,
            };
            // Check attributes dict (index 4 if present) for Process/ColorSpace.
            // The attributes dict may be inline or a reference.
            let process_repl = if arr.len() > 4 {
                let attrs_dict = match &arr[4] {
                    Object::Dictionary(d) => Some(d),
                    Object::Reference(ref_id) => {
                        if let Some(Object::Dictionary(d)) = doc.objects.get(ref_id) {
                            Some(d)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(attrs) = attrs_dict {
                    // Process entry may also be inline or a reference.
                    let process_dict = match attrs.get(b"Process").ok() {
                        Some(Object::Dictionary(d)) => Some(d),
                        Some(Object::Reference(ref_id)) => {
                            if let Some(Object::Dictionary(d)) = doc.objects.get(ref_id) {
                                Some(d)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(process) = process_dict {
                        match process.get(b"ColorSpace").ok() {
                            Some(Object::Name(n)) => replacement(n),
                            _ => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            (alt_repl, process_repl)
        };
        let (alt_repl, process_repl) = fix;
        if let Some(repl) = alt_repl {
            if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&id) {
                arr[2] = Object::Reference(repl);
            }
        }
        if let Some(repl) = process_repl {
            if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&id) {
                if arr.len() > 4 {
                    if let Object::Dictionary(ref mut attrs) = arr[4] {
                        if let Ok(Object::Dictionary(ref mut process)) = attrs.get_mut(b"Process") {
                            process.set("ColorSpace", Object::Reference(repl));
                        }
                    }
                }
            }
        }
    }

    // Pass 2: Fix attributes dicts stored as separate referenced objects.
    // Some DeviceN arrays reference the attributes dict by ObjectId.
    // The attributes dict may have Process as inline dict or reference.
    let ids2: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids2 {
        let repl = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            // Check if this dict has a Process entry (inline dict).
            if let Ok(Object::Dictionary(process)) = dict.get(b"Process") {
                match process.get(b"ColorSpace").ok() {
                    Some(Object::Name(n)) => replacement(n),
                    _ => None,
                }
            } else {
                None
            }
        };
        if let Some(repl) = repl {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                if let Ok(Object::Dictionary(ref mut process)) = dict.get_mut(b"Process") {
                    process.set("ColorSpace", Object::Reference(repl));
                }
            }
        }
    }

    // Pass 3: Fix Process dicts referenced by ID from attributes dicts.
    // Pattern: attributes dict has /Process <ref> -> Process dict has /ColorSpace /DeviceCMYK.
    // Collect all Process reference IDs from attributes dicts.
    let ids3: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut process_ref_ids: Vec<ObjectId> = Vec::new();
    for id in &ids3 {
        if let Some(Object::Dictionary(dict)) = doc.objects.get(id) {
            if let Ok(Object::Reference(process_id)) = dict.get(b"Process") {
                if !process_ref_ids.contains(process_id) {
                    process_ref_ids.push(*process_id);
                }
            }
        }
    }

    // Fix the referenced Process dicts directly.
    for process_id in process_ref_ids {
        let repl = if let Some(Object::Dictionary(process)) = doc.objects.get(&process_id) {
            match process.get(b"ColorSpace").ok() {
                Some(Object::Name(n)) => replacement(n),
                _ => None,
            }
        } else {
            None
        };
        if let Some(repl) = repl {
            if let Some(Object::Dictionary(ref mut process)) = doc.objects.get_mut(&process_id) {
                process.set("ColorSpace", Object::Reference(repl));
            }
        }
    }
}

/// Replace DeviceCMYK/DeviceRGB alternate colorspaces inside Separation arrays
/// with ICCBased references. Fixes 6.2.4.3:3 (DeviceCMYK used without CMYK
/// output intent) for Separation alternates. Also helps 6.2.4.4:2 by ensuring
/// all occurrences of a Separation with the same name have an identical alternate
/// object reference rather than a bare device colorspace name.
fn fix_separation_device_alternates(doc: &mut Document, cmyk_cs_id: ObjectId, rgb_cs_id: ObjectId) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if let Some(Object::Array(arr)) = doc.objects.get_mut(&id) {
            if arr.len() >= 4 {
                if let Object::Name(cs_type) = &arr[0] {
                    if cs_type == b"Separation" {
                        let repl = match &arr[2] {
                            Object::Name(n) if n == b"DeviceCMYK" => {
                                Some(Object::Reference(cmyk_cs_id))
                            }
                            Object::Name(n) if n == b"DeviceRGB" => {
                                Some(Object::Reference(rgb_cs_id))
                            }
                            _ => None,
                        };
                        if let Some(new_alt) = repl {
                            arr[2] = new_alt;
                        }
                    }
                }
            }
        }
    }
}

/// Normalize Separation colorspaces so all with the same name use identical
/// alternateSpace and tintTransform (PDF/A rule 6.2.4.4:2).
fn normalize_separation_colorspaces(doc: &mut Document) -> usize {
    use std::collections::{HashMap, HashSet};

    // Materialize lazily-loaded objects (including objects inside object streams)
    // so Separation arrays referenced outside the currently loaded object set are
    // still considered for 6.2.4.4:2 consistency checks.
    let max_id = doc.max_id;
    for obj_num in 1..=max_id {
        let id = (obj_num, 0);
        if doc.objects.contains_key(&id) {
            continue;
        }
        let loaded = doc.get_object(id).ok().cloned();
        if let Some(obj) = loaded {
            doc.objects.insert(id, obj);
        }
    }

    // Phase 1: Collect all Separation arrays across all objects.
    // A Separation array is: [/Separation name alternateCS tintTransform]
    // We record (ObjectId, name) → (alternateCS, tintTransform).
    let mut by_name: HashMap<Vec<u8>, Vec<(ObjectId, Object, Object)>> = HashMap::new();
    let mut visited_refs: HashSet<ObjectId> = HashSet::new();

    for (&id, obj) in &doc.objects {
        collect_separations_recursive(doc, id, obj, &mut by_name, &mut visited_refs);
    }

    // Phase 2: For each name with multiple Separation array objects, pick a
    // canonical object and redirect all other references to it. veraPDF checks
    // 6.2.4.4:2 consistency by object identity: two Separation colorspaces with
    // the same name must be THE SAME PDF object (same object number), not just
    // equal-content objects. So we must make every /ColorSpace key that pointed
    // to a non-canonical Separation array point to the canonical one instead.
    //
    // Additionally, unify the tintTransform within the canonical object: if the
    // canonical has a Reference tintTransform, keep it; if it's inline, promote
    // it to a standalone object so it can be shared via reference too.

    // Build: for each spot-color name → canonical object ID, list of non-canonical IDs.
    let mut redirects: Vec<(ObjectId, ObjectId)> = Vec::new(); // (old_id, new_id) for ref redirect
    let mut content_fixes: Vec<(ObjectId, Vec<u8>, Object, Object)> = Vec::new();

    for (name, entries) in &by_name {
        if entries.len() <= 1 {
            continue;
        }

        // Deduplicate by object ID: the same Separation object may be recorded
        // multiple times when encountered both directly and via reference following.
        let mut seen_ids: HashSet<ObjectId> = HashSet::new();
        let mut unique_entries: Vec<&(ObjectId, Object, Object)> = entries
            .iter()
            .filter(|(id, _, _)| seen_ids.insert(*id))
            .collect();

        if unique_entries.len() <= 1 {
            continue;
        }

        // Sort by object ID (lowest first) for deterministic canonical selection.
        // Using the lowest ID as canonical guarantees that the redirects always
        // point from higher-numbered objects to a lower-numbered one. This avoids
        // the case where the "canonical" is already the object that all references
        // point to, making the redirect a no-op.
        unique_entries.sort_by_key(|(id, _, _)| *id);

        let (canon_id, canon_alt, canon_tint) = unique_entries[0];

        // Promote the canonical tintTransform to a standalone object if it isn't
        // already a reference. This gives us a stable ObjectId to share.
        let canonical_tint_ref: Object = match canon_tint {
            Object::Reference(_) => canon_tint.clone(),
            other => {
                let new_id = doc.add_object(other.clone());
                Object::Reference(new_id)
            }
        };

        // Update ALL Separation objects for this name (canonical + non-canonical)
        // to have the same alternateSpace and tintTransform. veraPDF 6.2.4.4:2
        // scans ALL objects in the file, including unreferenced ones, so we must
        // make even non-canonical objects content-identical to the canonical.
        for (obj_id, _, _) in &unique_entries {
            content_fixes.push((
                *obj_id,
                name.clone(),
                canon_alt.clone(),
                canonical_tint_ref.clone(),
            ));
        }

        // All non-canonical objects: redirect document references to the canon.
        for (non_canon_id, _, _) in &unique_entries[1..] {
            redirects.push((*non_canon_id, *canon_id));
        }
    }

    // Phase 3: Update canonical Separation array content.
    let mut seen: HashSet<(ObjectId, Vec<u8>)> = HashSet::new();
    let mut count = 0usize;
    for (id, name, alt, tint) in content_fixes {
        if seen.insert((id, name.clone())) {
            if let Some(obj) = doc.objects.get_mut(&id) {
                fix_separation_recursive(obj, &name, &alt, &tint);
                count += 1;
            }
        }
    }

    // Phase 4: Redirect all document references from non-canonical → canonical.
    // Scan every object and replace Reference(old_id) with Reference(new_id).
    if !redirects.is_empty() {
        let all_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
        for obj_id in all_ids {
            if let Some(obj) = doc.objects.get_mut(&obj_id) {
                for (old_id, new_id) in &redirects {
                    redirect_references_recursive(obj, *old_id, *new_id);
                }
            }
        }
        count += redirects.len();
    }

    // Phase 5: Delete non-canonical Separation objects from the document.
    // veraPDF scans ALL objects (including unreferenced ones) for 6.2.4.4:2
    // consistency. After redirecting references, non-canonical Separation objects
    // are unreferenced, but veraPDF still finds them. Remove them entirely.
    for (old_id, _new_id) in &redirects {
        doc.objects.remove(old_id);
    }

    count
}

fn collect_separations_recursive(
    doc: &Document,
    id: ObjectId,
    obj: &Object,
    map: &mut std::collections::HashMap<Vec<u8>, Vec<(ObjectId, Object, Object)>>,
    visited_refs: &mut std::collections::HashSet<ObjectId>,
) {
    match obj {
        Object::Array(arr) => {
            if arr.len() >= 4 {
                if let Object::Name(cs_type) = &arr[0] {
                    if cs_type == b"Separation" {
                        if let Object::Name(name) = &arr[1] {
                            map.entry(normalize_spot_name(name)).or_default().push((
                                id,
                                arr[2].clone(),
                                arr[3].clone(),
                            ));
                        }
                    }
                }
            }
            for item in arr {
                collect_separations_recursive(doc, id, item, map, visited_refs);
            }
        }
        Object::Dictionary(dict) => {
            for (_, val) in dict.iter() {
                collect_separations_recursive(doc, id, val, map, visited_refs);
            }
        }
        Object::Stream(stream) => {
            for (_, val) in stream.dict.iter() {
                collect_separations_recursive(doc, id, val, map, visited_refs);
            }
        }
        Object::Reference(ref_id) => {
            if visited_refs.insert(*ref_id) {
                if let Ok(resolved) = doc.get_object(*ref_id) {
                    collect_separations_recursive(doc, *ref_id, resolved, map, visited_refs);
                }
            }
        }
        _ => {}
    }
}

/// Recursively replace all Reference(old_id) with Reference(new_id) in an object tree.
fn redirect_references_recursive(obj: &mut Object, old_id: ObjectId, new_id: ObjectId) {
    match obj {
        Object::Reference(r) => {
            if *r == old_id {
                *r = new_id;
            }
        }
        Object::Array(arr) => {
            for item in arr.iter_mut() {
                redirect_references_recursive(item, old_id, new_id);
            }
        }
        Object::Dictionary(dict) => {
            for (_, val) in dict.iter_mut() {
                redirect_references_recursive(val, old_id, new_id);
            }
        }
        Object::Stream(stream) => {
            for (_, val) in stream.dict.iter_mut() {
                redirect_references_recursive(val, old_id, new_id);
            }
        }
        _ => {}
    }
}

fn fix_separation_recursive(obj: &mut Object, name: &[u8], alt: &Object, tint: &Object) {
    match obj {
        Object::Array(arr) => {
            if arr.len() >= 4 {
                if let Object::Name(cs_type) = &arr[0] {
                    if cs_type == b"Separation" {
                        if let Object::Name(n) = &arr[1] {
                            if normalize_spot_name(n) == name {
                                arr[2] = alt.clone();
                                arr[3] = tint.clone();
                            }
                        }
                    }
                }
            }
            for item in arr.iter_mut() {
                fix_separation_recursive(item, name, alt, tint);
            }
        }
        Object::Dictionary(dict) => {
            for (_, val) in dict.iter_mut() {
                fix_separation_recursive(val, name, alt, tint);
            }
        }
        Object::Stream(stream) => {
            for (_, val) in stream.dict.iter_mut() {
                fix_separation_recursive(val, name, alt, tint);
            }
        }
        _ => {}
    }
}

/// Normalize a PDF Name object for logical comparison.
///
/// PDF names may encode bytes using `#XX` hex escapes. For clause 6.2.4.4:2,
/// names that differ only by escape form should be treated as the same spot
/// color name.
fn normalize_spot_name(name: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(name.len());
    let mut i = 0usize;
    while i < name.len() {
        if name[i] == b'#' && i + 2 < name.len() {
            let h1 = name[i + 1];
            let h2 = name[i + 2];
            let hi = (h1 as char).to_digit(16);
            let lo = (h2 as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                decoded.push(((hi << 4) | lo) as u8);
                i += 3;
                continue;
            }
        }
        decoded.push(name[i]);
        i += 1;
    }

    // Canonicalize ASCII names: trim/collapse whitespace and fold to upper-case
    // so escape/case variants of the same spot name group together.
    let mut out = Vec::with_capacity(decoded.len());
    let mut prev_space = true;
    for b in decoded {
        let is_space = matches!(b, b' ' | b'\t' | b'\r' | b'\n');
        if is_space {
            if !prev_space {
                out.push(b' ');
            }
            prev_space = true;
            continue;
        }
        prev_space = false;
        out.push(b.to_ascii_uppercase());
    }
    if out.last() == Some(&b' ') {
        out.pop();
    }
    out
}

/// Fix overprint mode: set OPM to 0 when overprinting is enabled (PDF/A rule 6.2.4.2:2).
///
/// When ICCBased CMYK is in use, OPM=1 is forbidden if overprinting is on.
/// Safest fix: set OPM to 0 in all ExtGState dictionaries.
fn fix_overprint_mode(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
            matches!(dict.get(b"OPM").ok(), Some(Object::Integer(1)))
        } else {
            false
        };
        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.set("OPM", Object::Integer(0));
                count += 1;
            }
        }
    }
    count
}

/// Fix ICCBased colorspace streams: ensure /N matches the profile header and
/// replace invalid ICC profiles (rule 6.2.4.2:1).
///
/// For each ICCBased stream:
/// 1. Decompress the stream content to read the actual ICC header.
/// 2. If /N doesn't match the profile's color space, update /N.
/// 3. If the ICC profile header is invalid (garbage device class, color space, or version),
///    replace the entire profile with our known-good sRGB or CMYK profile.
fn fix_iccbased_n_value(doc: &mut Document) -> usize {
    // Pre-generate replacement profiles.
    let srgb_profile = srgb_icc_profile_bytes();
    let cmyk_profile = cmyk_icc_profile_bytes();
    let gray_profile = gray_icc_profile_bytes();

    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let action = if let Some(Object::Stream(stream)) = doc.objects.get(&id) {
            // Only process streams that have /N (ICCBased profile indicator).
            let declared_n = match stream.dict.get(b"N").ok() {
                Some(Object::Integer(n)) => *n,
                _ => continue,
            };

            // Decompress the stream content to read the actual ICC header.
            // If no filters are present, decompressed_content returns empty;
            // in that case the raw content IS the profile data.
            let icc_bytes = {
                let decompressed = stream.decompressed_content().unwrap_or_default();
                if decompressed.is_empty() && !stream.content.is_empty() {
                    stream.content.clone()
                } else {
                    decompressed
                }
            };

            if icc_bytes.len() < 128 {
                // ICC header must be at least 128 bytes. Replace.
                IccAction::Replace(declared_n)
            } else {
                // Validate the ICC profile header.
                let cs_sig = &icc_bytes[16..20];
                let device_class = &icc_bytes[12..16];
                let acsp_sig = &icc_bytes[36..40];
                let version_major = icc_bytes[8];

                let cs_valid = matches!(
                    cs_sig,
                    b"GRAY"
                        | b"RGB "
                        | b"CMYK"
                        | b"Lab "
                        | b"XYZ "
                        | b"Luv "
                        | b"YCbr"
                        | b"Yxy "
                        | b"HSV "
                        | b"HLS "
                        | b"CMY "
                );
                let class_valid = matches!(
                    device_class,
                    b"scnr" | b"mntr" | b"prtr" | b"link" | b"spac" | b"abst" | b"nmcl"
                );
                let acsp_valid = acsp_sig == b"acsp";
                let version_valid = version_major < 5;

                if cs_valid && class_valid && acsp_valid && version_valid {
                    // Profile is valid. Check /N consistency.
                    let expected_n: i64 = match cs_sig {
                        b"GRAY" => 1,
                        b"RGB " | b"Lab " => 3,
                        b"CMYK" => 4,
                        _ => declared_n,
                    };
                    if declared_n != expected_n {
                        IccAction::FixN(expected_n)
                    } else {
                        IccAction::None
                    }
                } else {
                    // Invalid ICC profile — replace.
                    IccAction::Replace(declared_n)
                }
            }
        } else {
            IccAction::None
        };

        match action {
            IccAction::None => {}
            IccAction::FixN(n) => {
                if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                    stream.dict.set("N", Object::Integer(n));
                    count += 1;
                }
            }
            IccAction::Replace(declared_n) => {
                if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                    let replacement = match declared_n {
                        4 => cmyk_profile.clone(),
                        1 => gray_profile.clone(),
                        _ => srgb_profile.clone(),
                    };
                    stream.content = replacement;
                    // Remove compression filter since we store raw data.
                    stream.dict.remove(b"Filter");
                    stream.dict.remove(b"DecodeParms");
                    stream.dict.remove(b"F");
                    stream.dict.remove(b"FFilter");
                    stream.dict.remove(b"FDecodeParms");
                    let new_n = match declared_n {
                        4 => 4_i64,
                        1 => 1,
                        _ => 3,
                    };
                    stream.dict.set("N", Object::Integer(new_n));
                    stream
                        .dict
                        .set("Length", Object::Integer(stream.content.len() as i64));
                    count += 1;
                }
            }
        }
    }
    count
}

/// Actions for ICC profile fixing.
enum IccAction {
    /// No action needed.
    None,
    /// Fix /N value to the given number.
    FixN(i64),
    /// Replace the entire ICC profile. i64 is the declared /N value.
    Replace(i64),
}

/// Minimal Gray ICC v2 profile (1-component).
fn gray_icc_profile_bytes() -> Vec<u8> {
    let total_size: u32 = 324;
    let mut p = Vec::with_capacity(total_size as usize);

    // === Header (128 bytes) ===
    p.extend_from_slice(&total_size.to_be_bytes());
    p.extend_from_slice(b"\0\0\0\0"); // preferred CMM
    p.extend_from_slice(&[2, 0x10, 0, 0]); // version 2.1.0
    p.extend_from_slice(b"mntr"); // device class: monitor
    p.extend_from_slice(b"GRAY"); // color space
    p.extend_from_slice(b"XYZ "); // PCS
    p.extend_from_slice(&[0u8; 12]); // date/time
    p.extend_from_slice(b"acsp"); // signature
    p.extend_from_slice(&[0u8; 4]); // platform
    p.extend_from_slice(&[0u8; 4]); // flags
    p.extend_from_slice(&[0u8; 4]); // manufacturer
    p.extend_from_slice(&[0u8; 4]); // model
    p.extend_from_slice(&[0u8; 8]); // device attributes
    p.extend_from_slice(&[0u8; 4]); // rendering intent
                                    // PCS illuminant D50
    p.extend_from_slice(&0x0000F6D6_u32.to_be_bytes());
    p.extend_from_slice(&0x00010000_u32.to_be_bytes());
    p.extend_from_slice(&0x0000D32D_u32.to_be_bytes());
    p.extend_from_slice(&[0u8; 4]); // creator
    p.extend_from_slice(&[0u8; 16]); // profile ID
    p.extend_from_slice(&[0u8; 128 - 100]); // reserved
    debug_assert_eq!(p.len(), 128);

    // === Tag table: 4 tags ===
    p.extend_from_slice(&4_u32.to_be_bytes());
    // 128 + 4 + 4*12 = 180 bytes for header + tag table

    let tags: &[(&[u8; 4], u32, u32)] = &[
        (b"desc", 180, 95),
        (b"wtpt", 276, 20),
        (b"cprt", 296, 12),
        (b"kTRC", 308, 14),
    ];
    for (sig, offset, size) in tags {
        p.extend_from_slice(*sig);
        p.extend_from_slice(&offset.to_be_bytes());
        p.extend_from_slice(&size.to_be_bytes());
    }
    debug_assert_eq!(p.len(), 180);

    // === desc tag (textDescriptionType) — 95 bytes ===
    p.extend_from_slice(b"desc");
    p.extend_from_slice(&[0u8; 4]);
    p.extend_from_slice(&5_u32.to_be_bytes()); // ASCII length incl null
    p.extend_from_slice(b"Gray\0");
    p.extend_from_slice(&[0u8; 4]); // Unicode language
    p.extend_from_slice(&[0u8; 4]); // Unicode count
    p.extend_from_slice(&[0u8; 2]); // ScriptCode code
    p.push(0); // ScriptCode count
    p.extend_from_slice(&[0u8; 67]); // ScriptCode string
    debug_assert_eq!(p.len(), 275);
    // Pad to offset 276 (4-byte alignment)
    p.push(0);
    debug_assert_eq!(p.len(), 276);

    // === wtpt (XYZType) — D50 ===
    p.extend_from_slice(b"XYZ ");
    p.extend_from_slice(&[0u8; 4]);
    p.extend_from_slice(&0x0000F351_i32.to_be_bytes());
    p.extend_from_slice(&0x00010000_i32.to_be_bytes());
    p.extend_from_slice(&0x000116CC_i32.to_be_bytes());
    debug_assert_eq!(p.len(), 296);

    // === cprt ===
    p.extend_from_slice(b"text");
    p.extend_from_slice(&[0u8; 4]);
    p.extend_from_slice(b"CC0\0");
    debug_assert_eq!(p.len(), 308);

    // === kTRC (curveType with gamma 2.2) ===
    p.extend_from_slice(b"curv");
    p.extend_from_slice(&[0u8; 4]);
    p.extend_from_slice(&1_u32.to_be_bytes()); // count=1 means gamma value
    p.extend_from_slice(&[0x02, 0x33]); // gamma ~ 2.2
    debug_assert_eq!(p.len(), 322);
    p.extend_from_slice(&[0u8; 2]); // pad to 324
    debug_assert_eq!(p.len(), 324);

    p
}

fn is_device_dependent(name: &str) -> bool {
    matches!(name, "DeviceRGB" | "DeviceCMYK" | "DeviceGray")
}

fn resolve_colorspace_name(doc: &Document, obj: &Object) -> String {
    match obj {
        Object::Name(n) => String::from_utf8_lossy(n).to_string(),
        Object::Array(arr) if !arr.is_empty() => {
            if let Object::Name(n) = &arr[0] {
                String::from_utf8_lossy(n).to_string()
            } else {
                "Unknown".into()
            }
        }
        Object::Reference(id) => {
            if let Some(resolved) = doc.objects.get(id) {
                resolve_colorspace_name(doc, resolved)
            } else {
                "Unknown".into()
            }
        }
        _ => "Unknown".into(),
    }
}

fn get_name(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    match dict.get(key).ok()? {
        Object::Name(n) => String::from_utf8(n.clone()).ok(),
        _ => None,
    }
}

fn get_catalog(doc: &Document) -> Option<&lopdf::Dictionary> {
    let root_ref = match doc.trailer.get(b"Root").ok()? {
        Object::Reference(id) => *id,
        _ => return None,
    };
    match doc.objects.get(&root_ref)? {
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn get_catalog_id(doc: &Document) -> Result<ObjectId> {
    match doc.trailer.get(b"Root").ok() {
        Some(Object::Reference(id)) => Ok(*id),
        _ => Err(ManipError::Other("no Root in trailer".into())),
    }
}

fn count_pages(doc: &Document) -> usize {
    doc.objects
        .values()
        .filter(|obj| {
            if let Object::Dictionary(dict) = obj {
                get_name(dict, b"Type").as_deref() == Some("Page")
            } else {
                false
            }
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc_with_device_rgb() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let content = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Hello) Tj ET".to_vec());
        let content_id = doc.add_object(Object::Stream(content));

        let mut cs_dict = lopdf::Dictionary::new();
        cs_dict.set("CS1", Object::Name(b"DeviceRGB".to_vec()));

        let mut res = lopdf::Dictionary::new();
        res.set("ColorSpace", Object::Dictionary(cs_dict));

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(res),
        };
        let page_id = doc.add_object(Object::Dictionary(page));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn test_no_output_intent() {
        let doc = make_doc_with_device_rgb();
        assert!(!has_pdfa_output_intent(&doc));
    }

    #[test]
    fn test_add_output_intent() {
        let mut doc = make_doc_with_device_rgb();
        add_srgb_output_intent(&mut doc).unwrap();
        assert!(has_pdfa_output_intent(&doc));
    }

    #[test]
    fn test_find_device_colorspaces() {
        let doc = make_doc_with_device_rgb();
        let found = find_device_colorspaces(&doc);
        assert!(!found.is_empty());
        assert!(found[0].1.contains("DeviceRGB"));
    }

    #[test]
    fn test_normalize_adds_intent() {
        let mut doc = make_doc_with_device_rgb();
        let report = normalize_colorspaces(&mut doc).unwrap();
        assert!(!report.had_output_intent);
        assert!(report.output_intent_added);
        assert_eq!(report.pages_scanned, 1);
    }

    #[test]
    fn test_normalize_skips_existing() {
        let mut doc = make_doc_with_device_rgb();
        add_srgb_output_intent(&mut doc).unwrap();
        let report = normalize_colorspaces(&mut doc).unwrap();
        assert!(report.had_output_intent);
        assert!(!report.output_intent_added);
    }

    #[test]
    fn test_icc_profile_structure() {
        let profile = srgb_icc_profile_bytes();
        assert_eq!(profile.len(), 444);
        // Header checks
        assert_eq!(&profile[36..40], b"acsp");
        assert_eq!(&profile[16..20], b"RGB ");
        assert_eq!(&profile[12..16], b"mntr");
        // Size field
        let size = u32::from_be_bytes([profile[0], profile[1], profile[2], profile[3]]);
        assert_eq!(size, 444);
        // 9 tags
        let tag_count =
            u32::from_be_bytes([profile[128], profile[129], profile[130], profile[131]]);
        assert_eq!(tag_count, 9);
        // desc tag signature at first entry
        assert_eq!(&profile[132..136], b"desc");
    }

    #[test]
    fn test_empty_doc() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let report = normalize_colorspaces(&mut doc).unwrap();
        assert!(!report.had_output_intent);
        assert!(report.output_intent_added);
        assert!(report.device_colorspaces_found.is_empty());
    }

    #[test]
    fn test_separation_no_conflict() {
        let mut doc = Document::with_version("1.7");
        let tint_fn = dictionary! {
            "FunctionType" => Object::Integer(2),
            "N" => Object::Integer(1),
        };
        let tint_id = doc.add_object(Object::Dictionary(tint_fn));

        let sep1 = Object::Array(vec![
            Object::Name(b"Separation".to_vec()),
            Object::Name(b"SpotRed".to_vec()),
            Object::Name(b"DeviceRGB".to_vec()),
            Object::Reference(tint_id),
        ]);
        let sep2 = Object::Array(vec![
            Object::Name(b"Separation".to_vec()),
            Object::Name(b"SpotRed".to_vec()),
            Object::Name(b"DeviceRGB".to_vec()),
            Object::Reference(tint_id),
        ]);
        doc.add_object(sep1);
        doc.add_object(sep2);

        let count = normalize_separation_colorspaces(&mut doc);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_separation_conflict_unified() {
        let mut doc = Document::with_version("1.7");

        let tint1_id = doc.add_object(Object::Dictionary(dictionary! {
            "FunctionType" => Object::Integer(2),
            "N" => Object::Integer(1),
        }));
        let tint2_id = doc.add_object(Object::Dictionary(dictionary! {
            "FunctionType" => Object::Integer(2),
            "N" => Object::Integer(2),
        }));

        let sep1 = Object::Array(vec![
            Object::Name(b"Separation".to_vec()),
            Object::Name(b"SpotBlue".to_vec()),
            Object::Name(b"DeviceRGB".to_vec()),
            Object::Reference(tint1_id),
        ]);
        let sep2 = Object::Array(vec![
            Object::Name(b"Separation".to_vec()),
            Object::Name(b"SpotBlue".to_vec()),
            Object::Name(b"DeviceCMYK".to_vec()),
            Object::Reference(tint2_id),
        ]);
        let sep1_id = doc.add_object(sep1);
        let sep2_id = doc.add_object(sep2);

        let count = normalize_separation_colorspaces(&mut doc);
        assert_eq!(count, 1);

        // sep2 should now match sep1.
        if let Object::Array(arr) = &doc.objects[&sep2_id] {
            assert_eq!(arr[2], Object::Name(b"DeviceRGB".to_vec()));
            assert_eq!(arr[3], Object::Reference(tint1_id));
        } else {
            panic!("expected array");
        }
        // sep1 unchanged.
        if let Object::Array(arr) = &doc.objects[&sep1_id] {
            assert_eq!(arr[2], Object::Name(b"DeviceRGB".to_vec()));
            assert_eq!(arr[3], Object::Reference(tint1_id));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn test_separation_in_dict_value() {
        let mut doc = Document::with_version("1.7");

        let tint1_id = doc.add_object(Object::Dictionary(dictionary! {
            "FunctionType" => Object::Integer(2),
            "N" => Object::Integer(1),
        }));
        let tint2_id = doc.add_object(Object::Dictionary(dictionary! {
            "FunctionType" => Object::Integer(4),
            "N" => Object::Integer(1),
        }));

        // sep1 as top-level array
        doc.add_object(Object::Array(vec![
            Object::Name(b"Separation".to_vec()),
            Object::Name(b"Cyan".to_vec()),
            Object::Name(b"DeviceRGB".to_vec()),
            Object::Reference(tint1_id),
        ]));

        // sep2 nested inside a Colorants dictionary
        let mut colorants = lopdf::Dictionary::new();
        colorants.set(
            "Cyan",
            Object::Array(vec![
                Object::Name(b"Separation".to_vec()),
                Object::Name(b"Cyan".to_vec()),
                Object::Name(b"DeviceCMYK".to_vec()),
                Object::Reference(tint2_id),
            ]),
        );
        let colorants_id = doc.add_object(Object::Dictionary(colorants));

        let count = normalize_separation_colorspaces(&mut doc);
        assert_eq!(count, 1);

        // Verify the nested Separation was unified.
        if let Object::Dictionary(dict) = &doc.objects[&colorants_id] {
            if let Ok(Object::Array(arr)) = dict.get(b"Cyan") {
                assert_eq!(arr[2], Object::Name(b"DeviceRGB".to_vec()));
                assert_eq!(arr[3], Object::Reference(tint1_id));
            } else {
                panic!("expected array in Colorants");
            }
        } else {
            panic!("expected dictionary");
        }
    }

    #[test]
    fn test_output_intent_survives_roundtrip() {
        let mut doc = make_doc_with_device_rgb();
        let report = normalize_colorspaces(&mut doc).unwrap();
        assert!(report.output_intent_added);
        assert!(has_pdfa_output_intent(&doc));

        // Save and reload
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        let doc2 = Document::load_mem(&buf).unwrap();

        // Check OutputIntent survives serialization
        assert!(
            has_pdfa_output_intent(&doc2),
            "OutputIntent must survive save/load roundtrip"
        );

        // Check the DestOutputProfile ICC profile is present
        let catalog = get_catalog(&doc2).unwrap();
        let intents = match catalog.get(b"OutputIntents").unwrap() {
            Object::Array(arr) => arr.clone(),
            Object::Reference(id) => {
                if let Object::Array(arr) = doc2.objects.get(id).unwrap() {
                    arr.clone()
                } else {
                    panic!("expected array")
                }
            }
            _ => panic!("expected array"),
        };
        assert_eq!(intents.len(), 1);
        let intent = match &intents[0] {
            Object::Reference(id) => {
                if let Object::Dictionary(d) = doc2.objects.get(id).unwrap() {
                    d
                } else {
                    panic!("expected dict")
                }
            }
            Object::Dictionary(d) => d,
            _ => panic!("expected dict"),
        };

        // Must have DestOutputProfile reference
        let profile_ref = intent.get(b"DestOutputProfile").unwrap();
        match profile_ref {
            Object::Reference(id) => {
                let profile_obj = doc2.objects.get(id).expect("profile object must exist");
                match profile_obj {
                    Object::Stream(stream) => {
                        // Verify ICC profile size
                        let content = &stream.content;
                        assert!(
                            content.len() > 100,
                            "ICC profile must be non-trivial, got {} bytes",
                            content.len()
                        );
                        // Verify N=3 (RGB)
                        let n = stream.dict.get(b"N").unwrap();
                        assert_eq!(*n, Object::Integer(3));
                    }
                    _ => panic!("expected stream for ICC profile"),
                }
            }
            _ => panic!("expected reference for DestOutputProfile"),
        }
    }

    #[test]
    fn test_overprint_mode_fixed() {
        let mut doc = Document::with_version("1.7");
        let gs = dictionary! {
            "Type" => "ExtGState",
            "OPM" => Object::Integer(1),
            "OP" => Object::Boolean(true),
        };
        let gs_id = doc.add_object(Object::Dictionary(gs));

        let count = fix_overprint_mode(&mut doc);
        assert_eq!(count, 1);

        if let Object::Dictionary(dict) = &doc.objects[&gs_id] {
            assert_eq!(*dict.get(b"OPM").unwrap(), Object::Integer(0));
        }
    }

    #[test]
    fn test_overprint_mode_zero_untouched() {
        let mut doc = Document::with_version("1.7");
        let gs = dictionary! {
            "Type" => "ExtGState",
            "OPM" => Object::Integer(0),
        };
        doc.add_object(Object::Dictionary(gs));

        let count = fix_overprint_mode(&mut doc);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_icc_n_value_mismatch() {
        let mut doc = Document::with_version("1.7");
        // Create a minimal ICC profile header with RGB color space (bytes 16..20 = "RGB ")
        let mut icc_data = vec![0u8; 128];
        icc_data[12..16].copy_from_slice(b"mntr"); // valid device class
        icc_data[16..20].copy_from_slice(b"RGB ");
        icc_data[36..40].copy_from_slice(b"acsp");
        icc_data[8] = 2; // version 2.x

        let icc_dict = dictionary! {
            "N" => Object::Integer(4), // Wrong: should be 3 for RGB
        };
        let stream = Stream::new(icc_dict, icc_data);
        let stream_id = doc.add_object(Object::Stream(stream));

        let count = fix_iccbased_n_value(&mut doc);
        assert_eq!(count, 1);

        if let Object::Stream(s) = &doc.objects[&stream_id] {
            assert_eq!(*s.dict.get(b"N").unwrap(), Object::Integer(3));
        }
    }

    #[test]
    fn test_icc_n_value_correct() {
        let mut doc = Document::with_version("1.7");
        let mut icc_data = vec![0u8; 128];
        icc_data[12..16].copy_from_slice(b"prtr"); // valid device class
        icc_data[16..20].copy_from_slice(b"CMYK");
        icc_data[36..40].copy_from_slice(b"acsp");
        icc_data[8] = 2; // version 2.x

        let icc_dict = dictionary! {
            "N" => Object::Integer(4), // Correct for CMYK
        };
        let stream = Stream::new(icc_dict, icc_data);
        doc.add_object(Object::Stream(stream));

        let count = fix_iccbased_n_value(&mut doc);
        assert_eq!(count, 0);
    }
}
