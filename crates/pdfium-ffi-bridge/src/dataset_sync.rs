//! Dataset synchronization — write Data DOM back into PDF.
//!
//! When a form is saved, the Data DOM must be serialized back to XML and
//! written into the PDF's XFA datasets packet. This module handles the
//! roundtrip: Data DOM → XML → PDF stream update.

use crate::error::{PdfError, Result};
use crate::pdf_reader::PdfReader;
use xfa_dom_resolver::data_dom::DataDom;

/// Wrap a Data DOM's XML in an `<xfa:datasets>` element.
///
/// The XFA spec requires datasets to be wrapped in:
/// ```xml
/// <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
///   <xfa:data>
///     ...data nodes...
///   </xfa:data>
/// </xfa:datasets>
/// ```
pub fn wrap_datasets_xml(data_dom: &DataDom) -> String {
    let data_xml = data_dom.to_xml();
    format!(
        r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
{data_xml}  </xfa:data>
</xfa:datasets>"#
    )
}

/// Update the XFA datasets packet in a PDF.
///
/// Finds the existing XFA stream or array in the PDF's AcroForm dictionary
/// and replaces the datasets content with the serialized Data DOM.
pub fn sync_datasets(reader: &mut PdfReader, data_dom: &DataDom) -> Result<()> {
    let datasets_xml = wrap_datasets_xml(data_dom);

    // Try to find and update the XFA entry in the AcroForm
    let doc = reader.document_mut();

    // Navigate: trailer → Root → AcroForm → XFA
    let catalog_ref = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|_| PdfError::XfaPacketNotFound("no Root in trailer".to_string()))?;

    let catalog = doc
        .get_object(catalog_ref)
        .and_then(|o| o.as_dict())
        .map_err(|_| PdfError::XfaPacketNotFound("Root is not a dictionary".to_string()))?
        .clone();

    let acroform_ref = catalog
        .get(b"AcroForm")
        .and_then(|o| o.as_reference())
        .map_err(|_| PdfError::XfaPacketNotFound("no AcroForm in catalog".to_string()))?;

    let acroform = doc
        .get_object(acroform_ref)
        .and_then(|o| o.as_dict())
        .map_err(|_| PdfError::XfaPacketNotFound("AcroForm is not a dictionary".to_string()))?
        .clone();

    let xfa_entry = acroform
        .get(b"XFA")
        .map_err(|_| PdfError::XfaPacketNotFound("no XFA entry in AcroForm".to_string()))?
        .clone();

    match &xfa_entry {
        lopdf::Object::Reference(r) => {
            // Single-stream XFA: rebuild the full XDP document
            let full_xdp = rebuild_xdp_with_datasets(doc, *r, &datasets_xml)?;
            let stream = lopdf::Stream::new(lopdf::dictionary! {}, full_xdp.into_bytes());
            doc.objects
                .insert(*r, lopdf::Object::Stream(stream));
            Ok(())
        }
        lopdf::Object::Array(arr) => {
            // Array-form XFA: find and replace the datasets stream
            update_datasets_in_array(doc, arr, &datasets_xml)
        }
        _ => Err(PdfError::XfaPacketNotFound(
            "XFA entry is not a reference or array".to_string(),
        )),
    }
}

/// Rebuild a full XDP document, replacing the datasets packet.
fn rebuild_xdp_with_datasets(
    doc: &lopdf::Document,
    xfa_ref: (u32, u16),
    new_datasets: &str,
) -> Result<String> {
    // Read the existing XFA XML
    let xfa_obj = doc
        .get_object(xfa_ref)
        .map_err(|e| PdfError::XfaPacketNotFound(format!("XFA object: {e}")))?;

    let existing_xml = match xfa_obj {
        lopdf::Object::Stream(stream) => {
            let content = stream
                .get_plain_content()
                .map_err(|e| PdfError::XfaPacketNotFound(format!("stream decode: {e}")))?;
            String::from_utf8(content)
                .map_err(|e| PdfError::XfaPacketNotFound(format!("not UTF-8: {e}")))?
        }
        _ => {
            return Err(PdfError::XfaPacketNotFound(
                "XFA object is not a stream".to_string(),
            ))
        }
    };

    // Replace the datasets section in the existing XML
    Ok(replace_datasets_section(&existing_xml, new_datasets))
}

/// Replace the `<xfa:datasets>...</xfa:datasets>` section in an XDP XML string.
fn replace_datasets_section(xdp_xml: &str, new_datasets: &str) -> String {
    // Find the datasets section
    if let Some(start) = xdp_xml.find("<xfa:datasets") {
        if let Some(end_tag_start) = xdp_xml[start..].find("</xfa:datasets>") {
            let end = start + end_tag_start + "</xfa:datasets>".len();
            let mut result = String::with_capacity(xdp_xml.len());
            result.push_str(&xdp_xml[..start]);
            result.push_str(new_datasets);
            result.push_str(&xdp_xml[end..]);
            return result;
        }
    }

    // If no datasets section found, insert before closing </xdp:xdp>
    if let Some(close_pos) = xdp_xml.rfind("</xdp:xdp>") {
        let mut result = String::with_capacity(xdp_xml.len() + new_datasets.len());
        result.push_str(&xdp_xml[..close_pos]);
        result.push_str("  ");
        result.push_str(new_datasets);
        result.push('\n');
        result.push_str(&xdp_xml[close_pos..]);
        return result;
    }

    // Fallback: return original (shouldn't happen with valid XFA)
    xdp_xml.to_string()
}

/// Update the datasets stream in an XFA array.
fn update_datasets_in_array(
    doc: &mut lopdf::Document,
    arr: &[lopdf::Object],
    new_datasets: &str,
) -> Result<()> {
    // Find the "datasets" entry in the name/stream-ref array
    let mut i = 0;
    while i + 1 < arr.len() {
        let is_datasets = match &arr[i] {
            lopdf::Object::String(s, _) => {
                String::from_utf8_lossy(s) == "datasets"
            }
            lopdf::Object::Name(n) => {
                String::from_utf8_lossy(n) == "datasets"
            }
            lopdf::Object::Reference(r) => {
                match doc.get_object(*r) {
                    Ok(lopdf::Object::String(s, _)) => String::from_utf8_lossy(s) == "datasets",
                    Ok(lopdf::Object::Name(n)) => String::from_utf8_lossy(n) == "datasets",
                    _ => false,
                }
            }
            _ => false,
        };

        if is_datasets {
            // Replace the stream at arr[i+1]
            if let lopdf::Object::Reference(stream_ref) = &arr[i + 1] {
                let stream =
                    lopdf::Stream::new(lopdf::dictionary! {}, new_datasets.as_bytes().to_vec());
                doc.objects
                    .insert(*stream_ref, lopdf::Object::Stream(stream));
                return Ok(());
            }
        }

        i += 2;
    }

    Err(PdfError::XfaPacketNotFound(
        "datasets entry not found in XFA array".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream, StringFormat};
    use xfa_dom_resolver::data_dom::DataDom;

    fn build_test_data_dom() -> DataDom {
        DataDom::from_xml(
            "<form1><Name>John Doe</Name><Amount>42.50</Amount></form1>",
        )
        .unwrap()
    }

    #[test]
    fn wrap_datasets_produces_valid_xml() {
        let dom = build_test_data_dom();
        let xml = wrap_datasets_xml(&dom);

        assert!(xml.starts_with("<xfa:datasets"));
        assert!(xml.contains("<xfa:data>"));
        assert!(xml.contains("</xfa:data>"));
        assert!(xml.contains("</xfa:datasets>"));
        assert!(xml.contains("<Name>John Doe</Name>"));
        assert!(xml.contains("<Amount>42.50</Amount>"));
    }

    #[test]
    fn replace_datasets_section_replaces() {
        let xdp = r#"<xdp:xdp>
  <template>...</template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><old>data</old></xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

        let new_ds = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data><new>data</new></xfa:data>
</xfa:datasets>"#;

        let result = replace_datasets_section(xdp, new_ds);
        assert!(result.contains("<new>data</new>"));
        assert!(!result.contains("<old>data</old>"));
        assert!(result.contains("<template>...</template>"));
    }

    #[test]
    fn replace_datasets_inserts_when_missing() {
        let xdp = r#"<xdp:xdp>
  <template>...</template>
</xdp:xdp>"#;

        let new_ds = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data><Name>Test</Name></xfa:data>
</xfa:datasets>"#;

        let result = replace_datasets_section(xdp, new_ds);
        assert!(result.contains("<Name>Test</Name>"));
        assert!(result.contains("</xdp:xdp>"));
    }

    #[test]
    fn sync_datasets_single_stream() {
        let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="Name"/></subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><Name>Old Value</Name></form1></xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

        let pdf_bytes = build_xfa_pdf(xfa_xml);
        let mut reader = PdfReader::from_bytes(&pdf_bytes).unwrap();

        // Build updated data
        let dom = build_test_data_dom();
        sync_datasets(&mut reader, &dom).unwrap();

        // Save and reload to verify
        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();
        let packets = reader2.extract_xfa().unwrap();

        let full = packets.full_xml.as_deref().unwrap();
        assert!(full.contains("John Doe"), "Updated data should be in PDF");
        assert!(!full.contains("Old Value"), "Old data should be replaced");
    }

    #[test]
    fn sync_datasets_array_form() {
        let template_xml = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="Name"/></subform>
</template>"#;

        let old_data = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><Name>Old</Name></form1></xfa:data>
</xfa:datasets>"#;

        let pdf_bytes = build_xfa_array_pdf(template_xml, old_data);
        let mut reader = PdfReader::from_bytes(&pdf_bytes).unwrap();

        let dom = build_test_data_dom();
        sync_datasets(&mut reader, &dom).unwrap();

        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();
        let packets = reader2.extract_xfa().unwrap();

        let ds = packets.get_packet("datasets").unwrap();
        assert!(ds.contains("John Doe"));
    }

    // --- Helpers ---

    fn build_xfa_pdf(xfa_xml: &str) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let xfa_stream = Stream::new(dictionary! {}, xfa_xml.as_bytes().to_vec());
        let xfa_id = doc.add_object(Object::Stream(xfa_stream));

        let acroform = dictionary! { "XFA" => xfa_id };
        let acroform_id = doc.add_object(Object::Dictionary(acroform));

        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );

        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
                "AcroForm" => acroform_id,
            }),
        );
        doc.trailer.set("Root", catalog_id);

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    fn build_xfa_array_pdf(template_xml: &str, datasets_xml: &str) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");

        let tmpl_stream = Stream::new(dictionary! {}, template_xml.as_bytes().to_vec());
        let tmpl_id = doc.add_object(Object::Stream(tmpl_stream));

        let ds_stream = Stream::new(dictionary! {}, datasets_xml.as_bytes().to_vec());
        let ds_id = doc.add_object(Object::Stream(ds_stream));

        let xfa_array = vec![
            Object::String(b"template".to_vec(), StringFormat::Literal),
            Object::Reference(tmpl_id),
            Object::String(b"datasets".to_vec(), StringFormat::Literal),
            Object::Reference(ds_id),
        ];

        let acroform = dictionary! { "XFA" => Object::Array(xfa_array) };
        let acroform_id = doc.add_object(Object::Dictionary(acroform));

        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );

        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
                "AcroForm" => acroform_id,
            }),
        );
        doc.trailer.set("Root", catalog_id);

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }
}
