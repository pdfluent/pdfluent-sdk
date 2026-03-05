//! Native PDF reader — extract XFA packets using `lopdf` (pure Rust).
//!
//! Reads the PDF structure to find XFA streams in the AcroForm dictionary.
//! No C/C++ dependencies required. WASM-compatible.

use crate::error::{PdfError, Result};
use crate::xfa_extract::{parse_xfa_xml, XfaPackets};
use lopdf::Document;
use std::path::Path;

/// A native PDF reader using `lopdf` for PDF structure parsing.
pub struct PdfReader {
    document: Document,
}

impl PdfReader {
    /// Load a PDF document from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let document =
            Document::load_mem(bytes).map_err(|e| PdfError::LoadFailed(format!("{e}")))?;
        Ok(Self { document })
    }

    /// Load a PDF document from a file path.
    pub fn from_file(path: &Path) -> Result<Self> {
        let document =
            Document::load(path).map_err(|e| PdfError::LoadFailed(format!("{e}")))?;
        Ok(Self { document })
    }

    /// Get the number of pages in the PDF.
    pub fn page_count(&self) -> usize {
        self.document.get_pages().len()
    }

    /// Extract XFA packets from the PDF.
    ///
    /// Looks for XFA data in the AcroForm dictionary's XFA entry,
    /// which can be either a single stream or an array of name/stream pairs.
    pub fn extract_xfa(&self) -> Result<XfaPackets> {
        // Try the proper PDF structure first
        if let Ok(packets) = self.extract_xfa_from_acroform() {
            if !packets.packets.is_empty() || packets.full_xml.is_some() {
                return Ok(packets);
            }
        }

        // Fallback: scan raw content for XFA markers
        self.extract_xfa_by_scanning()
    }

    /// Resolve a PDF object: dereference if it's a Reference, return as-is otherwise.
    fn resolve<'a>(&'a self, obj: &'a lopdf::Object) -> Result<&'a lopdf::Object> {
        match obj {
            lopdf::Object::Reference(r) => self
                .document
                .get_object(*r)
                .map_err(|e| PdfError::XfaPacketNotFound(format!("resolve: {e}"))),
            _ => Ok(obj),
        }
    }

    /// Extract XFA from the AcroForm dictionary's XFA entry.
    fn extract_xfa_from_acroform(&self) -> Result<XfaPackets> {
        // Navigate: trailer → Root → AcroForm → XFA
        // Use get_deref to auto-resolve references
        let catalog = self
            .document
            .trailer
            .get_deref(b"Root", &self.document)
            .and_then(|o| o.as_dict())
            .map_err(|_| PdfError::XfaPacketNotFound("no Root catalog".to_string()))?;

        let acroform = catalog
            .get_deref(b"AcroForm", &self.document)
            .and_then(|o| o.as_dict())
            .map_err(|_| PdfError::XfaPacketNotFound("no AcroForm in catalog".to_string()))?;

        let xfa_entry = acroform
            .get(b"XFA")
            .map_err(|_| PdfError::XfaPacketNotFound("no XFA entry in AcroForm".to_string()))?;

        // Resolve the XFA entry itself (may be a reference to a stream)
        let resolved_xfa = self.resolve(xfa_entry)?;

        match resolved_xfa {
            lopdf::Object::Stream(stream) => {
                let content = stream.get_plain_content()
                    .map_err(|e| PdfError::XfaPacketNotFound(format!("stream decode: {e}")))?;
                let xml = String::from_utf8(content)
                    .map_err(|e| PdfError::XfaPacketNotFound(format!("not UTF-8: {e}")))?;
                parse_xfa_xml(&xml)
            }
            lopdf::Object::Array(arr) => {
                self.extract_xfa_from_array(arr)
            }
            _ => Err(PdfError::XfaPacketNotFound(
                "XFA entry is not a stream or array".to_string(),
            )),
        }
    }

    /// Extract XFA from an array of name/stream-ref pairs.
    ///
    /// Per PDF spec, the XFA array alternates: [name1, stream-ref1, name2, stream-ref2, ...]
    fn extract_xfa_from_array(&self, arr: &[lopdf::Object]) -> Result<XfaPackets> {
        let mut packets = XfaPackets::default();
        let mut full_xml = String::new();

        let mut i = 0;
        while i + 1 < arr.len() {
            let name = match &arr[i] {
                lopdf::Object::String(s, _) => {
                    String::from_utf8_lossy(s).to_string()
                }
                lopdf::Object::Name(n) => {
                    String::from_utf8_lossy(n).to_string()
                }
                _ => {
                    i += 1;
                    continue;
                }
            };

            let content = match &arr[i + 1] {
                lopdf::Object::Reference(r) => self.read_stream_as_string(*r)?,
                lopdf::Object::Stream(s) => {
                    let bytes = s.get_plain_content()
                        .map_err(|e| PdfError::XfaPacketNotFound(format!("decode: {e}")))?;
                    String::from_utf8(bytes)
                        .map_err(|e| PdfError::XfaPacketNotFound(format!("not UTF-8: {e}")))?
                }
                _ => {
                    i += 2;
                    continue;
                }
            };

            full_xml.push_str(&content);
            packets.packets.push((name, content));
            i += 2;
        }

        if !full_xml.is_empty() {
            packets.full_xml = Some(full_xml);
        }

        Ok(packets)
    }

    /// Read a PDF stream object as a UTF-8 string.
    fn read_stream_as_string(&self, obj_ref: (u32, u16)) -> Result<String> {
        let obj = self
            .document
            .get_object(obj_ref)
            .map_err(|e| PdfError::XfaPacketNotFound(format!("object not found: {e}")))?;

        match obj {
            lopdf::Object::Stream(stream) => {
                let content = stream.get_plain_content()
                    .map_err(|e| PdfError::XfaPacketNotFound(format!("decompress: {e}")))?;
                String::from_utf8(content)
                    .map_err(|e| PdfError::XfaPacketNotFound(format!("not UTF-8: {e}")))
            }
            lopdf::Object::String(bytes, _) => {
                String::from_utf8(bytes.clone())
                    .map_err(|e| PdfError::XfaPacketNotFound(format!("not UTF-8: {e}")))
            }
            _ => Err(PdfError::XfaPacketNotFound(
                "expected stream or string object".to_string(),
            )),
        }
    }

    /// Fallback: scan all streams for XFA XML markers.
    fn extract_xfa_by_scanning(&self) -> Result<XfaPackets> {
        for obj in self.document.objects.values() {
            if let lopdf::Object::Stream(stream) = obj {
                if let Ok(content) = stream.get_plain_content() {
                    if let Ok(text) = std::str::from_utf8(&content) {
                        if text.contains("<xdp:xdp") {
                            return parse_xfa_xml(text);
                        }
                    }
                }
            }
        }

        Err(PdfError::XfaPacketNotFound(
            "no XFA content found in PDF".to_string(),
        ))
    }

    /// Get a reference to the underlying lopdf Document.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Get a mutable reference to the underlying lopdf Document.
    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    /// Save the PDF document to bytes.
    pub fn save_to_bytes(&mut self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.document
            .save_to(&mut buf)
            .map_err(|e| PdfError::Io(std::io::Error::other(format!("{e}"))))?;
        Ok(buf)
    }

    /// Save the PDF document to a file.
    pub fn save_to_file(&mut self, path: &Path) -> Result<()> {
        self.document
            .save(path)
            .map_err(|e| PdfError::Io(std::io::Error::other(format!("{e}"))))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn load_nonexistent_file_fails() {
        let result = PdfReader::from_file(Path::new("/nonexistent.pdf"));
        assert!(result.is_err());
    }

    #[test]
    fn load_invalid_bytes_fails() {
        let result = PdfReader::from_bytes(b"not a pdf");
        assert!(result.is_err());
    }

    #[test]
    fn minimal_pdf_no_xfa() {
        // Create a minimal valid PDF using lopdf
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, lopdf::Object::Dictionary(pages));

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        };
        doc.objects.insert(page_id, lopdf::Object::Dictionary(page));

        let catalog_id = doc.new_object_id();
        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        };
        doc.objects.insert(catalog_id, lopdf::Object::Dictionary(catalog));
        doc.trailer.set("Root", catalog_id);

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();

        let reader = PdfReader::from_bytes(&buf).unwrap();
        assert_eq!(reader.page_count(), 1);
        // No XFA in this PDF
        let xfa_result = reader.extract_xfa();
        assert!(xfa_result.is_err());
    }

    #[test]
    fn pdf_with_xfa_stream() {
        // Create a PDF with an XFA stream in AcroForm
        let mut doc = Document::with_version("1.7");

        let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1">
      <field name="Name"/>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <form1><Name>Test</Name></form1>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

        // Create XFA stream
        let xfa_stream = lopdf::Stream::new(
            dictionary! {},
            xfa_xml.as_bytes().to_vec(),
        );
        let xfa_id = doc.add_object(lopdf::Object::Stream(xfa_stream));

        // Create AcroForm with XFA reference
        let acroform = dictionary! {
            "XFA" => xfa_id,
        };
        let acroform_id = doc.add_object(lopdf::Object::Dictionary(acroform));

        // Create pages
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, lopdf::Object::Dictionary(pages));
        let page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        };
        doc.objects.insert(page_id, lopdf::Object::Dictionary(page));

        // Create catalog with AcroForm
        let catalog_id = doc.new_object_id();
        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform_id,
        };
        doc.objects.insert(catalog_id, lopdf::Object::Dictionary(catalog));
        doc.trailer.set("Root", catalog_id);

        // Save and reload
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();

        let reader = PdfReader::from_bytes(&buf).unwrap();
        let packets = reader.extract_xfa().unwrap();

        assert!(packets.template().is_some());
        assert!(packets.datasets().is_some());
        assert_eq!(packets.packets.len(), 2);
    }

    #[test]
    fn pdf_with_xfa_array() {
        // Create a PDF with XFA as array of name/stream pairs
        let mut doc = Document::with_version("1.7");

        let template_xml = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="F1"/></subform>
</template>"#;

        let data_xml = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><F1>Hello</F1></form1></xfa:data>
</xfa:datasets>"#;

        let template_stream = lopdf::Stream::new(
            dictionary! {},
            template_xml.as_bytes().to_vec(),
        );
        let template_id = doc.add_object(lopdf::Object::Stream(template_stream));

        let data_stream = lopdf::Stream::new(
            dictionary! {},
            data_xml.as_bytes().to_vec(),
        );
        let data_id = doc.add_object(lopdf::Object::Stream(data_stream));

        // XFA array: ["template", stream-ref, "datasets", stream-ref]
        let xfa_array = vec![
            lopdf::Object::String(b"template".to_vec(), lopdf::StringFormat::Literal),
            lopdf::Object::Reference(template_id),
            lopdf::Object::String(b"datasets".to_vec(), lopdf::StringFormat::Literal),
            lopdf::Object::Reference(data_id),
        ];

        let acroform = dictionary! {
            "XFA" => lopdf::Object::Array(xfa_array),
        };
        let acroform_id = doc.add_object(lopdf::Object::Dictionary(acroform));

        // Pages
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, lopdf::Object::Dictionary(pages));
        let page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        };
        doc.objects.insert(page_id, lopdf::Object::Dictionary(page));

        let catalog_id = doc.new_object_id();
        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform_id,
        };
        doc.objects.insert(catalog_id, lopdf::Object::Dictionary(catalog));
        doc.trailer.set("Root", catalog_id);

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();

        let reader = PdfReader::from_bytes(&buf).unwrap();
        let packets = reader.extract_xfa().unwrap();

        assert_eq!(packets.packets.len(), 2);
        assert!(packets.get_packet("template").is_some());
        assert!(packets.get_packet("datasets").is_some());
    }

    #[test]
    fn save_and_reload() {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, lopdf::Object::Dictionary(pages));
        let page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        };
        doc.objects.insert(page_id, lopdf::Object::Dictionary(page));
        let catalog_id = doc.new_object_id();
        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        };
        doc.objects.insert(catalog_id, lopdf::Object::Dictionary(catalog));
        doc.trailer.set("Root", catalog_id);

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();

        let mut reader = PdfReader::from_bytes(&buf).unwrap();
        assert_eq!(reader.page_count(), 1);

        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();
        assert_eq!(reader2.page_count(), 1);
    }
}
