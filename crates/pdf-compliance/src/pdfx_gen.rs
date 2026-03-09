//! PDF/X generation utilities.
//!
//! Functions to add TrimBox, BleedBox, OutputIntent, and XMP metadata
//! to make a PDF document PDF/X compliant.

use crate::PdfXLevel;
use lopdf::{dictionary, Document, Object};

/// Default bleed offset in points (3mm ≈ 8.5pt).
const DEFAULT_BLEED_OFFSET: f64 = 8.504;

/// Add TrimBox to all pages that don't have one.
///
/// Derives TrimBox from CropBox (if present) or MediaBox.
pub fn add_trim_boxes(doc: &mut Document) -> usize {
    let page_ids: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();
    let mut count = 0;

    for (_page_num, page_id) in page_ids {
        if let Ok(Object::Dictionary(ref page_dict)) = doc.get_object(page_id) {
            if page_dict.get(b"TrimBox").is_ok() {
                continue;
            }

            let media_box = page_dict
                .get(b"CropBox")
                .or_else(|_| page_dict.get(b"MediaBox"))
                .ok()
                .and_then(|obj| {
                    if let Object::Array(ref arr) = obj {
                        if arr.len() >= 4 {
                            return Some(arr.clone());
                        }
                    }
                    None
                });

            if let Some(bbox) = media_box {
                if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
                    page_dict.set("TrimBox", Object::Array(bbox));
                    count += 1;
                }
            }
        }
    }

    count
}

/// Add BleedBox to all pages that don't have one.
///
/// BleedBox extends TrimBox by the specified offset (default 3mm).
pub fn add_bleed_boxes(doc: &mut Document, offset: Option<f64>) -> usize {
    let bleed_offset = offset.unwrap_or(DEFAULT_BLEED_OFFSET);
    let page_ids: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();
    let mut count = 0;

    for (_page_num, page_id) in page_ids {
        if let Ok(Object::Dictionary(ref page_dict)) = doc.get_object(page_id) {
            if page_dict.get(b"BleedBox").is_ok() {
                continue;
            }

            let trim_box = page_dict
                .get(b"TrimBox")
                .or_else(|_| page_dict.get(b"CropBox"))
                .or_else(|_| page_dict.get(b"MediaBox"))
                .ok()
                .and_then(|obj| {
                    if let Object::Array(ref arr) = obj {
                        extract_rect(arr)
                    } else {
                        None
                    }
                });

            if let Some([x0, y0, x1, y1]) = trim_box {
                let bleed = vec![
                    Object::Real((x0 - bleed_offset) as f32),
                    Object::Real((y0 - bleed_offset) as f32),
                    Object::Real((x1 + bleed_offset) as f32),
                    Object::Real((y1 + bleed_offset) as f32),
                ];

                if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
                    page_dict.set("BleedBox", Object::Array(bleed));
                    count += 1;
                }
            }
        }
    }

    count
}

/// Add a GTS_PDFX OutputIntent with the specified ICC profile name.
///
/// If `icc_data` is provided, embeds the ICC profile as a DestOutputProfile stream.
/// Otherwise, only sets OutputConditionIdentifier.
pub fn add_output_intent(
    doc: &mut Document,
    _level: PdfXLevel,
    output_condition: &str,
    icc_data: Option<&[u8]>,
) -> Result<(), lopdf::Error> {
    let mut oi_dict = dictionary! {
        "Type" => "OutputIntent",
        "S" => Object::Name(b"GTS_PDFX".to_vec()),
        "OutputConditionIdentifier" => Object::String(output_condition.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        "RegistryName" => Object::String(b"http://www.color.org".to_vec(), lopdf::StringFormat::Literal),
        "Info" => Object::String(output_condition.as_bytes().to_vec(), lopdf::StringFormat::Literal),
    };

    if let Some(data) = icc_data {
        let icc_stream = lopdf::Stream::new(
            dictionary! {
                "N" => 4_i64,
            },
            data.to_vec(),
        )
        .with_compression(true);
        let icc_id = doc.add_object(Object::Stream(icc_stream));
        oi_dict.set("DestOutputProfile", Object::Reference(icc_id));
    }

    let oi_id = doc.add_object(Object::Dictionary(oi_dict));

    let catalog_id = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .ok_or(lopdf::Error::ObjectNotFound((0, 0)))?;

    if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
        let existing = catalog.get(b"OutputIntents").ok().and_then(|o| {
            if let Object::Array(ref arr) = o {
                Some(arr.clone())
            } else {
                None
            }
        });

        let mut intents = existing.unwrap_or_default();
        intents.push(Object::Reference(oi_id));
        catalog.set("OutputIntents", Object::Array(intents));
    }

    Ok(())
}

/// Add PDF/X identification to XMP metadata.
pub fn add_pdfx_xmp(doc: &mut Document, level: PdfXLevel) -> Result<(), lopdf::Error> {
    let xmp_xml = format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
      xmlns:pdfxid="http://www.npes.org/pdfx/ns/id/"
      xmlns:dc="http://purl.org/dc/elements/1.1/"
      xmlns:xmp="http://ns.adobe.com/xap/1.0/">
      <pdfxid:GTS_PDFXVersion>{version}</pdfxid:GTS_PDFXVersion>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        version = level.gts_version()
    );

    let xmp_stream = lopdf::Stream::new(
        dictionary! {
            "Type" => "Metadata",
            "Subtype" => "XML",
        },
        xmp_xml.into_bytes(),
    );
    let xmp_id = doc.add_object(Object::Stream(xmp_stream));

    let catalog_id = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .ok_or(lopdf::Error::ObjectNotFound((0, 0)))?;

    if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
        catalog.set("Metadata", Object::Reference(xmp_id));
    }

    Ok(())
}

fn extract_rect(arr: &[Object]) -> Option<[f64; 4]> {
    if arr.len() < 4 {
        return None;
    }
    let x0 = obj_to_f64(&arr[0])?;
    let y0 = obj_to_f64(&arr[1])?;
    let x1 = obj_to_f64(&arr[2])?;
    let y1 = obj_to_f64(&arr[3])?;
    Some([x0, y0, x1, y1])
}

fn obj_to_f64(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(f) => Some(*f as f64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object};

    fn make_test_doc() -> Document {
        let mut doc = Document::with_version("1.7");

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn add_trim_box() {
        let mut doc = make_test_doc();
        let count = add_trim_boxes(&mut doc);
        assert_eq!(count, 1);

        let count2 = add_trim_boxes(&mut doc);
        assert_eq!(count2, 0);
    }

    #[test]
    fn add_bleed_box() {
        let mut doc = make_test_doc();
        add_trim_boxes(&mut doc);
        let count = add_bleed_boxes(&mut doc, None);
        assert_eq!(count, 1);

        let pages = doc.get_pages();
        let page_id = pages[&1];
        if let Ok(Object::Dictionary(ref page)) = doc.get_object(page_id) {
            if let Ok(Object::Array(ref bleed)) = page.get(b"BleedBox") {
                let x0 = obj_to_f64(&bleed[0]).unwrap();
                assert!(x0 < 0.0);
            }
        }
    }

    #[test]
    fn add_output_intent_without_icc() {
        let mut doc = make_test_doc();
        add_output_intent(&mut doc, PdfXLevel::X1a2003, "FOGRA39", None).unwrap();

        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        if let Ok(Object::Dictionary(ref cat)) = doc.get_object(catalog_id) {
            assert!(cat.get(b"OutputIntents").is_ok());
        }
    }

    #[test]
    fn add_pdfx_metadata() {
        let mut doc = make_test_doc();
        add_pdfx_xmp(&mut doc, PdfXLevel::X4).unwrap();

        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        if let Ok(Object::Dictionary(ref cat)) = doc.get_object(catalog_id) {
            assert!(cat.get(b"Metadata").is_ok());
        }
    }
}
