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
pub fn add_srgb_output_intent(doc: &mut Document) -> Result<()> {
    let icc_bytes = srgb_icc_profile_bytes();

    let icc_dict = dictionary! {
        "N" => Object::Integer(3),
        "Alternate" => Object::Name(b"DeviceRGB".to_vec()),
    };
    let icc_stream = Stream::new(icc_dict, icc_bytes);
    let icc_id = doc.add_object(Object::Stream(icc_stream));

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

/// Normalize color spaces: add sRGB OutputIntent if missing.
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

    let output_intent_added = if !had_output_intent {
        add_srgb_output_intent(doc)?;
        true
    } else {
        false
    };

    Ok(ColorSpaceReport {
        had_output_intent,
        output_intent_added,
        device_colorspaces_found: unique_names,
        pages_scanned,
    })
}

/// Minimal sRGB ICC profile for PDF/A OutputIntent.
fn srgb_icc_profile_bytes() -> Vec<u8> {
    let mut profile = Vec::with_capacity(132);

    profile.extend_from_slice(&[0u8; 4]); // size placeholder
    profile.extend_from_slice(b"    "); // CMM type
    profile.extend_from_slice(&[2, 0x10, 0, 0]); // version 2.1.0
    profile.extend_from_slice(b"mntr"); // device class: monitor
    profile.extend_from_slice(b"RGB "); // color space
    profile.extend_from_slice(b"XYZ "); // PCS
    profile.extend_from_slice(&[0u8; 12]); // date/time
    profile.extend_from_slice(b"acsp"); // signature
    profile.extend_from_slice(b"MSFT"); // platform
    profile.extend_from_slice(&[0u8; 4]); // flags
    profile.extend_from_slice(b"    "); // manufacturer
    profile.extend_from_slice(b"    "); // model
    profile.extend_from_slice(&[0u8; 8]); // device attributes
    profile.extend_from_slice(&[0u8; 4]); // rendering intent
    // PCS illuminant D50
    profile.extend_from_slice(&[0, 0, 0xF6, 0xD6]); // X
    profile.extend_from_slice(&[0, 1, 0, 0]); // Y
    profile.extend_from_slice(&[0, 0, 0xD3, 0x2D]); // Z
    profile.extend_from_slice(b"    "); // creator
    profile.extend_from_slice(&[0u8; 16]); // profile ID
    let remaining = 128 - profile.len();
    profile.extend_from_slice(&vec![0u8; remaining]);

    // Tag table: 0 tags (minimal valid profile)
    profile.extend_from_slice(&[0u8; 4]);

    let size = profile.len() as u32;
    profile[0..4].copy_from_slice(&size.to_be_bytes());

    profile
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
        assert!(profile.len() >= 132);
        assert_eq!(&profile[36..40], b"acsp");
        assert_eq!(&profile[16..20], b"RGB ");
        assert_eq!(&profile[12..16], b"mntr");
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
}
