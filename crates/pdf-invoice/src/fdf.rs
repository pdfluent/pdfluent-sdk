//! FDF (Forms Data Format) import/export — ISO 32000 §12.7.8.
//!
//! FDF is a binary format structurally similar to PDF that carries form field
//! data.  This module can parse FDF files, generate them, and apply/extract
//! field data to/from `lopdf::Document` instances.

use crate::error::{InvoiceError, Result};
use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};

/// A complete FDF document.
#[derive(Debug, Clone)]
pub struct FdfDocument {
    /// Form field values.
    pub fields: Vec<FdfField>,
    /// Optional reference to the source PDF file.
    pub file_spec: Option<String>,
}

/// A single FDF field entry.
#[derive(Debug, Clone)]
pub struct FdfField {
    /// Partial field name (one segment of the fully-qualified name).
    pub name: String,
    /// Field value as a string (None if this node is only a grouping parent).
    pub value: Option<String>,
    /// Child fields (for hierarchical field names).
    pub kids: Vec<FdfField>,
}

impl FdfDocument {
    /// Parse an FDF document from raw bytes.
    ///
    /// FDF uses the same object syntax as PDF.  We patch the header to
    /// `%PDF-1.4` so that `lopdf` can parse it, then extract the FDF
    /// dictionary from the catalog.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut buf = data.to_vec();
        // Replace %FDF-x.y header with %PDF-1.4 so lopdf can parse it.
        if buf.starts_with(b"%FDF") {
            let end = buf.iter().position(|&b| b == b'\n').unwrap_or(8);
            let replacement = b"%PDF-1.4";
            let pad = end.saturating_sub(replacement.len());
            buf[..replacement.len()].copy_from_slice(replacement);
            for b in &mut buf[replacement.len()..replacement.len() + pad] {
                *b = b' ';
            }
        }
        let doc = Document::load_mem(&buf)?;
        Self::extract_from_lopdf(&doc)
    }

    /// Serialize this FDF document to the binary FDF format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(512);
        out.extend_from_slice(b"%FDF-1.2\n");

        // Build FDF dictionary
        let mut fdf_dict = Dictionary::new();
        let fields_array = self.fields_to_objects();
        fdf_dict.set("Fields", fields_array);

        if let Some(ref spec) = self.file_spec {
            fdf_dict.set(
                "F",
                Object::String(spec.as_bytes().to_vec(), StringFormat::Literal),
            );
        }

        let mut catalog = Dictionary::new();
        catalog.set("FDF", Object::Dictionary(fdf_dict));

        // Object 1: catalog
        let obj_offset = out.len();
        out.extend_from_slice(b"1 0 obj\n");
        write_object(&mut out, &Object::Dictionary(catalog));
        out.extend_from_slice(b"\nendobj\n");

        // Cross-reference table
        let xref_offset = out.len();
        out.extend_from_slice(b"xref\n0 2\n");
        out.extend_from_slice(b"0000000000 65535 f \n");
        out.extend_from_slice(format!("{:010} 00000 n \n", obj_offset).as_bytes());

        // Trailer
        out.extend_from_slice(b"trailer\n");
        let mut trailer = Dictionary::new();
        trailer.set("Size", Object::Integer(2));
        trailer.set("Root", Object::Reference((1, 0)));
        write_object(&mut out, &Object::Dictionary(trailer));
        out.extend_from_slice(format!("\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes());

        out
    }

    /// Export form field data from a `lopdf::Document` into an `FdfDocument`.
    pub fn export_from(doc: &Document) -> Result<Self> {
        let fields = extract_acroform_fields(doc)?;
        Ok(FdfDocument {
            fields,
            file_spec: None,
        })
    }

    /// Import field values from this FDF into a `lopdf::Document`.
    pub fn import_into(&self, doc: &mut Document) -> Result<()> {
        let acroform_id = find_acroform_id(doc)?;
        let acroform = doc
            .get_object(acroform_id)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .ok_or_else(|| InvoiceError::Parse("cannot read AcroForm dict".into()))?;

        let field_ids: Vec<ObjectId> = acroform
            .get(b"Fields")
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|arr| arr.iter().filter_map(|o| o.as_reference().ok()).collect())
            .unwrap_or_default();

        let flat = flatten_fdf_fields(&self.fields, "");
        for (fqn, value) in &flat {
            set_field_value_by_name(doc, &field_ids, fqn, value)?;
        }
        Ok(())
    }

    // -- private helpers --

    fn extract_from_lopdf(doc: &Document) -> Result<Self> {
        let catalog_id = doc
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or_else(|| InvoiceError::Parse("no /Root in trailer".into()))?;

        let catalog = doc
            .get_object(catalog_id)
            .map_err(|_| InvoiceError::Parse("cannot read catalog".into()))?
            .as_dict()
            .map_err(|_| InvoiceError::Parse("catalog is not a dict".into()))?;

        let fdf_dict = catalog
            .get(b"FDF")
            .map_err(|_| InvoiceError::Parse("no /FDF in catalog".into()))?
            .as_dict()
            .map_err(|_| InvoiceError::Parse("/FDF is not a dict".into()))?;

        let file_spec = fdf_dict.get(b"F").ok().and_then(|o| match o {
            Object::String(s, _) => String::from_utf8(s.clone()).ok(),
            _ => None,
        });

        let fields_arr = fdf_dict
            .get(b"Fields")
            .ok()
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default();

        let fields = parse_fdf_field_array(&fields_arr, doc);

        Ok(FdfDocument { fields, file_spec })
    }

    fn fields_to_objects(&self) -> Object {
        Object::Array(self.fields.iter().map(field_to_object).collect())
    }
}

// -- PDF object serializer (subset needed for FDF) --

fn write_object(out: &mut Vec<u8>, obj: &Object) {
    match obj {
        Object::Null => out.extend_from_slice(b"null"),
        Object::Boolean(b) => {
            out.extend_from_slice(if *b { b"true" } else { b"false" });
        }
        Object::Integer(i) => {
            out.extend_from_slice(i.to_string().as_bytes());
        }
        Object::Real(f) => {
            out.extend_from_slice(format!("{f:.4}").as_bytes());
        }
        Object::Name(n) => {
            out.push(b'/');
            out.extend_from_slice(n);
        }
        Object::String(s, StringFormat::Literal) => {
            out.push(b'(');
            // Escape special chars in literal strings.
            for &byte in s {
                match byte {
                    b'(' | b')' | b'\\' => {
                        out.push(b'\\');
                        out.push(byte);
                    }
                    _ => out.push(byte),
                }
            }
            out.push(b')');
        }
        Object::String(s, _) => {
            out.push(b'<');
            for &byte in s {
                out.extend_from_slice(format!("{byte:02X}").as_bytes());
            }
            out.push(b'>');
        }
        Object::Array(arr) => {
            out.extend_from_slice(b"[ ");
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                write_object(out, item);
            }
            out.extend_from_slice(b" ]");
        }
        Object::Dictionary(dict) => {
            out.extend_from_slice(b"<< ");
            for (key, val) in dict.iter() {
                out.push(b'/');
                out.extend_from_slice(key);
                out.push(b' ');
                write_object(out, val);
                out.push(b' ');
            }
            out.extend_from_slice(b">>");
        }
        Object::Reference(id) => {
            out.extend_from_slice(format!("{} {} R", id.0, id.1).as_bytes());
        }
        Object::Stream(_) => {
            // Streams not used in FDF field data.
            out.extend_from_slice(b"null");
        }
    }
}

fn field_to_object(field: &FdfField) -> Object {
    let mut dict = Dictionary::new();
    dict.set(
        "T",
        Object::String(field.name.as_bytes().to_vec(), StringFormat::Literal),
    );
    if let Some(ref val) = field.value {
        dict.set(
            "V",
            Object::String(val.as_bytes().to_vec(), StringFormat::Literal),
        );
    }
    if !field.kids.is_empty() {
        dict.set(
            "Kids",
            Object::Array(field.kids.iter().map(field_to_object).collect()),
        );
    }
    Object::Dictionary(dict)
}

fn parse_fdf_field_array(arr: &[Object], doc: &Document) -> Vec<FdfField> {
    arr.iter()
        .filter_map(|obj| {
            let dict = match obj {
                Object::Dictionary(d) => d,
                Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok()?,
                _ => return None,
            };
            let name = dict
                .get(b"T")
                .ok()
                .and_then(|o| match o {
                    Object::String(s, _) => String::from_utf8(s.clone()).ok(),
                    _ => None,
                })
                .unwrap_or_default();

            let value = dict.get(b"V").ok().and_then(object_to_string);

            let kids = dict
                .get(b"Kids")
                .ok()
                .and_then(|o| o.as_array().ok())
                .map(|a| parse_fdf_field_array(a, doc))
                .unwrap_or_default();

            Some(FdfField { name, value, kids })
        })
        .collect()
}

pub(crate) fn object_to_string(obj: &Object) -> Option<String> {
    match obj {
        Object::String(s, _) => String::from_utf8(s.clone()).ok(),
        Object::Name(n) => String::from_utf8(n.clone()).ok(),
        Object::Integer(i) => Some(i.to_string()),
        Object::Real(f) => Some(f.to_string()),
        Object::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Flatten hierarchical FDF fields into (fully-qualified-name, value) pairs.
fn flatten_fdf_fields(fields: &[FdfField], prefix: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for f in fields {
        let fqn = if prefix.is_empty() {
            f.name.clone()
        } else {
            format!("{prefix}.{}", f.name)
        };
        if let Some(ref v) = f.value {
            out.push((fqn.clone(), v.clone()));
        }
        out.extend(flatten_fdf_fields(&f.kids, &fqn));
    }
    out
}

/// Locate the AcroForm dictionary object ID in the document catalog.
pub(crate) fn find_acroform_id(doc: &Document) -> Result<ObjectId> {
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .ok_or_else(|| InvoiceError::Parse("no /Root in trailer".into()))?;

    let catalog = doc
        .get_object(catalog_id)?
        .as_dict()
        .map_err(|_| InvoiceError::Parse("catalog not a dict".into()))?;

    catalog
        .get(b"AcroForm")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .ok_or_else(|| InvoiceError::Parse("no /AcroForm in catalog".into()))
}

/// Extract all AcroForm fields from a document as FdfFields.
fn extract_acroform_fields(doc: &Document) -> Result<Vec<FdfField>> {
    let acroform_id = find_acroform_id(doc)?;
    let acroform = doc
        .get_object(acroform_id)?
        .as_dict()
        .map_err(|_| InvoiceError::Parse("AcroForm not a dict".into()))?;

    let field_ids: Vec<ObjectId> = acroform
        .get(b"Fields")
        .ok()
        .and_then(|o| o.as_array().ok())
        .map(|arr| arr.iter().filter_map(|o| o.as_reference().ok()).collect())
        .unwrap_or_default();

    let mut fields = Vec::new();
    for id in field_ids {
        if let Some(field) = extract_field_recursive(doc, id) {
            fields.push(field);
        }
    }
    Ok(fields)
}

fn extract_field_recursive(doc: &Document, id: ObjectId) -> Option<FdfField> {
    let dict = doc.get_object(id).ok()?.as_dict().ok()?;
    let name = dict
        .get(b"T")
        .ok()
        .and_then(object_to_string)
        .unwrap_or_default();
    let value = dict.get(b"V").ok().and_then(object_to_string);

    let kids: Vec<FdfField> = dict
        .get(b"Kids")
        .ok()
        .and_then(|o| o.as_array().ok())
        .map(|arr| {
            arr.iter()
                .filter_map(|o| o.as_reference().ok())
                .filter_map(|kid_id| extract_field_recursive(doc, kid_id))
                .collect()
        })
        .unwrap_or_default();

    Some(FdfField { name, value, kids })
}

/// Set a field value in the document by its fully-qualified name.
pub(crate) fn set_field_value_by_name(
    doc: &mut Document,
    root_field_ids: &[ObjectId],
    fqn: &str,
    value: &str,
) -> Result<()> {
    let parts: Vec<&str> = fqn.split('.').collect();
    for &id in root_field_ids {
        if try_set_field(doc, id, &parts, 0, value)? {
            return Ok(());
        }
    }
    Ok(())
}

fn try_set_field(
    doc: &mut Document,
    id: ObjectId,
    parts: &[&str],
    depth: usize,
    value: &str,
) -> Result<bool> {
    if depth >= parts.len() {
        return Ok(false);
    }

    let dict = doc
        .get_object(id)?
        .as_dict()
        .map_err(|_| InvoiceError::Parse("field not a dict".into()))?;

    let name = dict
        .get(b"T")
        .ok()
        .and_then(object_to_string)
        .unwrap_or_default();

    if name != parts[depth] {
        return Ok(false);
    }

    if depth == parts.len() - 1 {
        let dict = doc
            .get_object_mut(id)?
            .as_dict_mut()
            .map_err(|_| InvoiceError::Parse("field not a dict".into()))?;
        dict.set(
            "V",
            Object::String(value.as_bytes().to_vec(), StringFormat::Literal),
        );
        return Ok(true);
    }

    let kid_ids: Vec<ObjectId> = dict
        .get(b"Kids")
        .ok()
        .and_then(|o| o.as_array().ok())
        .map(|arr| arr.iter().filter_map(|o| o.as_reference().ok()).collect())
        .unwrap_or_default();

    for kid_id in kid_ids {
        if try_set_field(doc, kid_id, parts, depth + 1, value)? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fdf() -> FdfDocument {
        FdfDocument {
            fields: vec![
                FdfField {
                    name: "name".into(),
                    value: Some("Alice".into()),
                    kids: vec![],
                },
                FdfField {
                    name: "address".into(),
                    value: None,
                    kids: vec![
                        FdfField {
                            name: "street".into(),
                            value: Some("123 Main St".into()),
                            kids: vec![],
                        },
                        FdfField {
                            name: "city".into(),
                            value: Some("Amsterdam".into()),
                            kids: vec![],
                        },
                    ],
                },
            ],
            file_spec: Some("form.pdf".into()),
        }
    }

    #[test]
    fn roundtrip_fdf_bytes() {
        let fdf = sample_fdf();
        let bytes = fdf.to_bytes();

        assert!(bytes.starts_with(b"%FDF-1.2"));
        let parsed = FdfDocument::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.fields.len(), 2);
        assert_eq!(parsed.fields[0].name, "name");
        assert_eq!(parsed.fields[0].value.as_deref(), Some("Alice"));
        assert_eq!(parsed.fields[1].name, "address");
        assert_eq!(parsed.fields[1].kids.len(), 2);
        assert_eq!(
            parsed.fields[1].kids[0].value.as_deref(),
            Some("123 Main St")
        );
        assert_eq!(parsed.file_spec.as_deref(), Some("form.pdf"));
    }

    #[test]
    fn flatten_hierarchical_fields() {
        let fdf = sample_fdf();
        let flat = flatten_fdf_fields(&fdf.fields, "");

        assert!(flat.iter().any(|(k, v)| k == "name" && v == "Alice"));
        assert!(flat
            .iter()
            .any(|(k, v)| k == "address.street" && v == "123 Main St"));
        assert!(flat
            .iter()
            .any(|(k, v)| k == "address.city" && v == "Amsterdam"));
    }

    #[test]
    fn empty_fdf_roundtrip() {
        let fdf = FdfDocument {
            fields: vec![],
            file_spec: None,
        };
        let bytes = fdf.to_bytes();
        let parsed = FdfDocument::from_bytes(&bytes).unwrap();
        assert!(parsed.fields.is_empty());
        assert!(parsed.file_spec.is_none());
    }
}
