//! PDF/A-3 embedding for ZUGFeRD/Factur-X XML.
//!
//! Provides functions to embed CII XML as an Associated File in a PDF
//! document (PDF/A-3 §6.8) and to extract previously embedded invoice XML.

use crate::error::{InvoiceError, Result};
use crate::zugferd::ZugferdProfile;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};

/// AF relationship type for embedded files (ISO 32000-2 §7.11.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfRelationship {
    /// The embedded file is the source data (used for ZUGFeRD).
    Data,
    /// The embedded file is an alternative representation.
    Alternative,
    /// The embedded file is a supplement.
    Supplement,
    /// Unspecified relationship.
    Unspecified,
}

impl AfRelationship {
    fn as_name(&self) -> &'static str {
        match self {
            Self::Data => "Data",
            Self::Alternative => "Alternative",
            Self::Supplement => "Supplement",
            Self::Unspecified => "Unspecified",
        }
    }
}

/// Embed an XML file as an associated file in a PDF document.
///
/// This creates the EmbeddedFiles name tree entry and the catalog /AF array
/// required by PDF/A-3.
pub fn embed_xml_attachment(
    doc: &mut Document,
    filename: &str,
    xml_data: &[u8],
    relationship: AfRelationship,
) -> Result<()> {
    // Create the embedded file stream (with FlateDecode compression).
    let compressed = compress(xml_data);
    let mut ef_stream_dict = Dictionary::new();
    ef_stream_dict.set("Type", Object::Name(b"EmbeddedFile".to_vec()));
    ef_stream_dict.set("Subtype", Object::Name(b"text/xml".to_vec()));
    ef_stream_dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    ef_stream_dict.set(
        "Params",
        Object::Dictionary({
            let mut params = Dictionary::new();
            params.set("Size", Object::Integer(xml_data.len() as i64));
            params
        }),
    );
    let ef_stream = Stream::new(ef_stream_dict, compressed);
    let ef_stream_id = doc.add_object(ef_stream);

    // Create the file specification dictionary.
    let mut filespec = Dictionary::new();
    filespec.set("Type", Object::Name(b"Filespec".to_vec()));
    filespec.set(
        "F",
        Object::String(filename.as_bytes().to_vec(), StringFormat::Literal),
    );
    filespec.set(
        "UF",
        Object::String(filename.as_bytes().to_vec(), StringFormat::Hexadecimal),
    );
    filespec.set(
        "AFRelationship",
        Object::Name(relationship.as_name().as_bytes().to_vec()),
    );

    let mut ef_dict = Dictionary::new();
    ef_dict.set("F", Object::Reference(ef_stream_id));
    ef_dict.set("UF", Object::Reference(ef_stream_id));
    filespec.set("EF", Object::Dictionary(ef_dict));

    let filespec_id = doc.add_object(filespec);

    // Add to catalog /AF array.
    let catalog_id = catalog_id(doc)?;
    let catalog = doc
        .get_object_mut(catalog_id)?
        .as_dict_mut()
        .map_err(|_| InvoiceError::Parse("catalog not a dict".into()))?;

    match catalog.get_mut(b"AF") {
        Ok(Object::Array(ref mut arr)) => {
            arr.push(Object::Reference(filespec_id));
        }
        _ => {
            catalog.set("AF", Object::Array(vec![Object::Reference(filespec_id)]));
        }
    }

    // Add to /Names/EmbeddedFiles name tree.
    add_to_embedded_files_nametree(doc, filename, filespec_id)?;

    Ok(())
}

/// Add ZUGFeRD-specific XMP metadata to the document.
///
/// Sets `fx:ConformanceLevel` and the Factur-X document type in the XMP
/// metadata stream.
pub fn add_zugferd_xmp(doc: &mut Document, profile: ZugferdProfile) -> Result<()> {
    let conformance = match profile {
        ZugferdProfile::Minimum => "MINIMUM",
        ZugferdProfile::BasicWL => "BASIC WL",
        ZugferdProfile::Basic => "BASIC",
        ZugferdProfile::EN16931 => "EN 16931",
        ZugferdProfile::Extended => "EXTENDED",
    };

    let xmp = format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/"
    xmlns:fx="urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#">
    <pdfaid:part>3</pdfaid:part>
    <pdfaid:conformance>B</pdfaid:conformance>
    <fx:DocumentType>INVOICE</fx:DocumentType>
    <fx:DocumentFileName>factur-x.xml</fx:DocumentFileName>
    <fx:Version>1.0</fx:Version>
    <fx:ConformanceLevel>{conformance}</fx:ConformanceLevel>
  </rdf:Description>
</rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#
    );

    let mut stream_dict = Dictionary::new();
    stream_dict.set("Type", Object::Name(b"Metadata".to_vec()));
    stream_dict.set("Subtype", Object::Name(b"XML".to_vec()));
    stream_dict.set("Length", Object::Integer(xmp.len() as i64));

    let stream = Stream::new(stream_dict, xmp.into_bytes());
    let meta_id = doc.add_object(stream);

    let catalog_id = catalog_id(doc)?;
    let catalog = doc
        .get_object_mut(catalog_id)?
        .as_dict_mut()
        .map_err(|_| InvoiceError::Parse("catalog not a dict".into()))?;
    catalog.set("Metadata", Object::Reference(meta_id));

    Ok(())
}

/// Extract an embedded XML attachment by filename.
///
/// Returns `None` if no attachment with the given name is found.
pub fn extract_xml_attachment(doc: &Document, filename: &str) -> Result<Option<Vec<u8>>> {
    let catalog_id = catalog_id(doc)?;
    let catalog = doc
        .get_object(catalog_id)?
        .as_dict()
        .map_err(|_| InvoiceError::Parse("catalog not a dict".into()))?;

    // Search /Names/EmbeddedFiles name tree.
    let Some(names_dict) = catalog
        .get(b"Names")
        .ok()
        .and_then(|o| resolve_dict(doc, o))
    else {
        return Ok(None);
    };

    let Some(ef_tree) = names_dict
        .get(b"EmbeddedFiles")
        .ok()
        .and_then(|o| resolve_dict(doc, o))
    else {
        return Ok(None);
    };

    // Walk the name tree /Names array: [name1, ref1, name2, ref2, ...]
    let Some(names_arr) = ef_tree.get(b"Names").ok().and_then(|o| o.as_array().ok()) else {
        return Ok(None);
    };

    for pair in names_arr.chunks(2) {
        if pair.len() < 2 {
            break;
        }
        let key = match &pair[0] {
            Object::String(s, _) => String::from_utf8_lossy(s).into_owned(),
            _ => continue,
        };
        if key != filename {
            continue;
        }

        let filespec_id = pair[1]
            .as_reference()
            .map_err(|_| InvoiceError::Parse("EmbeddedFiles ref invalid".into()))?;

        let filespec = doc
            .get_object(filespec_id)?
            .as_dict()
            .map_err(|_| InvoiceError::Parse("filespec not a dict".into()))?;

        let ef = filespec
            .get(b"EF")
            .map_err(|_| InvoiceError::Parse("no /EF in filespec".into()))?
            .as_dict()
            .map_err(|_| InvoiceError::Parse("/EF not a dict".into()))?;

        let stream_id = ef
            .get(b"F")
            .map_err(|_| InvoiceError::Parse("no /F in /EF".into()))?
            .as_reference()
            .map_err(|_| InvoiceError::Parse("/EF/F not a reference".into()))?;

        let stream_obj = doc.get_object(stream_id)?;
        let stream = stream_obj
            .as_stream()
            .map_err(|_| InvoiceError::Parse("embedded file not a stream".into()))?;

        let data = stream
            .decompressed_content()
            .map_err(|e| InvoiceError::Parse(format!("decompress: {e}")))?;

        return Ok(Some(data));
    }

    Ok(None)
}

// -- helpers --

fn catalog_id(doc: &Document) -> Result<ObjectId> {
    doc.trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .ok_or_else(|| InvoiceError::Parse("no /Root in trailer".into()))
}

fn resolve_dict<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Dictionary> {
    match obj {
        Object::Dictionary(d) => Some(d),
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok(),
        _ => None,
    }
}

fn add_to_embedded_files_nametree(
    doc: &mut Document,
    filename: &str,
    filespec_id: ObjectId,
) -> Result<()> {
    let catalog_id = catalog_id(doc)?;

    // Get or create /Names dict.
    let names_id = {
        let catalog = doc
            .get_object(catalog_id)?
            .as_dict()
            .map_err(|_| InvoiceError::Parse("catalog not a dict".into()))?;

        match catalog.get(b"Names") {
            Ok(Object::Reference(id)) => *id,
            Ok(Object::Dictionary(_)) => {
                // Inline dict — need to externalize it.
                let d = catalog.get(b"Names").unwrap().as_dict().unwrap().clone();
                let id = doc.add_object(d);
                let catalog = doc
                    .get_object_mut(catalog_id)?
                    .as_dict_mut()
                    .map_err(|_| InvoiceError::Parse("catalog not a dict".into()))?;
                catalog.set("Names", Object::Reference(id));
                id
            }
            _ => {
                let id = doc.add_object(Dictionary::new());
                let catalog = doc
                    .get_object_mut(catalog_id)?
                    .as_dict_mut()
                    .map_err(|_| InvoiceError::Parse("catalog not a dict".into()))?;
                catalog.set("Names", Object::Reference(id));
                id
            }
        }
    };

    // Get or create /EmbeddedFiles name tree.
    let names_dict = doc
        .get_object(names_id)?
        .as_dict()
        .map_err(|_| InvoiceError::Parse("names not a dict".into()))?;

    let ef_exists = names_dict.get(b"EmbeddedFiles").is_ok();

    if ef_exists {
        // Append to existing /Names array.
        let ef_ref = names_dict
            .get(b"EmbeddedFiles")
            .ok()
            .and_then(|o| o.as_reference().ok());

        if let Some(ef_id) = ef_ref {
            let ef_dict = doc
                .get_object_mut(ef_id)?
                .as_dict_mut()
                .map_err(|_| InvoiceError::Parse("EmbeddedFiles not a dict".into()))?;
            match ef_dict.get_mut(b"Names") {
                Ok(Object::Array(ref mut arr)) => {
                    arr.push(Object::String(
                        filename.as_bytes().to_vec(),
                        StringFormat::Literal,
                    ));
                    arr.push(Object::Reference(filespec_id));
                }
                _ => {
                    ef_dict.set(
                        "Names",
                        Object::Array(vec![
                            Object::String(filename.as_bytes().to_vec(), StringFormat::Literal),
                            Object::Reference(filespec_id),
                        ]),
                    );
                }
            }
        } else {
            // Inline dict.
            let names_dict = doc
                .get_object_mut(names_id)?
                .as_dict_mut()
                .map_err(|_| InvoiceError::Parse("names not a dict".into()))?;

            let ef_dict_obj = names_dict
                .get_mut(b"EmbeddedFiles")
                .map_err(|_| InvoiceError::Parse("EmbeddedFiles missing".into()))?;

            if let Object::Dictionary(ref mut d) = ef_dict_obj {
                match d.get_mut(b"Names") {
                    Ok(Object::Array(ref mut arr)) => {
                        arr.push(Object::String(
                            filename.as_bytes().to_vec(),
                            StringFormat::Literal,
                        ));
                        arr.push(Object::Reference(filespec_id));
                    }
                    _ => {
                        d.set(
                            "Names",
                            Object::Array(vec![
                                Object::String(filename.as_bytes().to_vec(), StringFormat::Literal),
                                Object::Reference(filespec_id),
                            ]),
                        );
                    }
                }
            }
        }
    } else {
        // Create new EmbeddedFiles name tree.
        let mut ef_dict = Dictionary::new();
        ef_dict.set(
            "Names",
            Object::Array(vec![
                Object::String(filename.as_bytes().to_vec(), StringFormat::Literal),
                Object::Reference(filespec_id),
            ]),
        );
        let ef_id = doc.add_object(ef_dict);

        let names_dict = doc
            .get_object_mut(names_id)?
            .as_dict_mut()
            .map_err(|_| InvoiceError::Parse("names not a dict".into()))?;
        names_dict.set("EmbeddedFiles", Object::Reference(ef_id));
    }

    Ok(())
}

fn compress(data: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("compression write");
    encoder.finish().expect("compression finish")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_pdf() -> Document {
        let mut doc = Document::with_version("1.7");

        let pages_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Pages".to_vec())),
            ("Kids", Object::Array(vec![])),
            ("Count", Object::Integer(0)),
        ]));

        let catalog_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Catalog".to_vec())),
            ("Pages", Object::Reference(pages_id)),
        ]));

        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc
    }

    #[test]
    fn embed_and_extract_xml() {
        let mut doc = make_minimal_pdf();
        let xml_data = b"<invoice>test</invoice>";

        embed_xml_attachment(&mut doc, "factur-x.xml", xml_data, AfRelationship::Data).unwrap();

        let extracted = extract_xml_attachment(&doc, "factur-x.xml")
            .unwrap()
            .expect("should find embedded file");

        assert_eq!(extracted, xml_data);
    }

    #[test]
    fn extract_missing_returns_none() {
        let doc = make_minimal_pdf();
        let result = extract_xml_attachment(&doc, "nonexistent.xml").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn embed_multiple_files() {
        let mut doc = make_minimal_pdf();

        embed_xml_attachment(&mut doc, "file1.xml", b"<a/>", AfRelationship::Data).unwrap();
        embed_xml_attachment(&mut doc, "file2.xml", b"<b/>", AfRelationship::Supplement).unwrap();

        assert_eq!(
            extract_xml_attachment(&doc, "file1.xml").unwrap().unwrap(),
            b"<a/>"
        );
        assert_eq!(
            extract_xml_attachment(&doc, "file2.xml").unwrap().unwrap(),
            b"<b/>"
        );
    }

    #[test]
    fn zugferd_xmp_metadata() {
        let mut doc = make_minimal_pdf();
        add_zugferd_xmp(&mut doc, ZugferdProfile::EN16931).unwrap();

        // Verify catalog has /Metadata.
        let catalog_id = catalog_id(&doc).unwrap();
        let catalog = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        assert!(catalog.get(b"Metadata").is_ok());
    }

    #[test]
    fn af_relationship_names() {
        assert_eq!(AfRelationship::Data.as_name(), "Data");
        assert_eq!(AfRelationship::Alternative.as_name(), "Alternative");
        assert_eq!(AfRelationship::Supplement.as_name(), "Supplement");
        assert_eq!(AfRelationship::Unspecified.as_name(), "Unspecified");
    }
}
