//! AcroForm XML export/import and XDP (XML Data Package) generation.
//!
//! Provides a lightweight XML representation of AcroForm field data for
//! integration with external systems, plus XDP envelope generation for
//! XFA payloads.

use crate::error::{InvoiceError, Result};
use lopdf::{Document, Object, ObjectId};

/// XML representation of AcroForm field data.
#[derive(Debug, Clone)]
pub struct AcroFormXml {
    /// All form fields.
    pub fields: Vec<XmlFormField>,
}

/// A single field in the XML form representation.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct XmlFormField {
    /// Fully qualified field name (dot-separated).
    pub name: String,
    /// Field type (Text, Button, Choice, Signature).
    pub field_type: String,
    /// Current value, if any.
    pub value: Option<String>,
}

impl AcroFormXml {
    /// Export form fields from a `lopdf::Document`.
    pub fn export_from(doc: &Document) -> Result<Self> {
        let catalog_id = doc
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or_else(|| InvoiceError::Parse("no /Root".into()))?;

        let catalog = doc
            .get_object(catalog_id)?
            .as_dict()
            .map_err(|_| InvoiceError::Parse("catalog not a dict".into()))?;

        let acroform_id = catalog
            .get(b"AcroForm")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or_else(|| InvoiceError::Parse("no /AcroForm".into()))?;

        let acroform = doc
            .get_object(acroform_id)?
            .as_dict()
            .map_err(|_| InvoiceError::Parse("AcroForm not a dict".into()))?;

        let field_ids: Vec<ObjectId> = acroform
            .get(b"Fields")
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
            .unwrap_or_default();

        let mut fields = Vec::new();
        for id in field_ids {
            collect_fields(doc, id, "", &mut fields);
        }

        Ok(AcroFormXml { fields })
    }

    /// Serialize to XML.
    pub fn to_xml(&self) -> String {
        let mut out = String::with_capacity(512);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str("<acroform-data>\n");
        for f in &self.fields {
            out.push_str(&format!(
                "  <field name=\"{}\" type=\"{}\">",
                xml_escape(&f.name),
                xml_escape(&f.field_type)
            ));
            if let Some(ref v) = f.value {
                out.push_str(&xml_escape(v));
            }
            out.push_str("</field>\n");
        }
        out.push_str("</acroform-data>\n");
        out
    }

    /// Parse from XML.
    pub fn from_xml(xml: &str) -> Result<Self> {
        let doc = roxmltree::Document::parse(xml)
            .map_err(|e| InvoiceError::Xml(format!("AcroForm XML parse: {e}")))?;

        let fields = doc
            .root_element()
            .children()
            .filter(|n| n.has_tag_name("field"))
            .filter_map(|n| {
                let name = n.attribute("name")?.to_string();
                let field_type = n.attribute("type").unwrap_or("Text").to_string();
                let value = n.text().map(String::from).filter(|s| !s.is_empty());
                Some(XmlFormField {
                    name,
                    field_type,
                    value,
                })
            })
            .collect();

        Ok(AcroFormXml { fields })
    }

    /// Import field values into a `lopdf::Document`.
    pub fn import_into(&self, doc: &mut Document) -> Result<()> {
        let fdf_fields: Vec<crate::fdf::FdfField> = self
            .fields
            .iter()
            .map(|f| {
                // Split fully-qualified name back into hierarchy.
                name_to_fdf_field(&f.name, f.value.as_deref())
            })
            .collect();

        let fdf = crate::fdf::FdfDocument {
            fields: fdf_fields,
            file_spec: None,
        };
        fdf.import_into(doc)
    }
}

/// Generate an XDP (XML Data Package) envelope wrapping XFA template and
/// optional datasets XML.
///
/// XDP is the standard container format for XFA forms when stored outside of
/// a PDF.  The resulting XML can be embedded back into a PDF or processed by
/// XFA-aware tools.
pub fn generate_xdp(template_xml: &str, datasets_xml: Option<&str>) -> String {
    let mut out = String::with_capacity(template_xml.len() + 512);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">\n");

    // Template packet
    out.push_str("  <template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">\n");
    out.push_str("    ");
    out.push_str(template_xml);
    out.push('\n');
    out.push_str("  </template>\n");

    // Datasets packet (optional)
    if let Some(ds) = datasets_xml {
        out.push_str("  <xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\">\n");
        out.push_str("    ");
        out.push_str(ds);
        out.push('\n');
        out.push_str("  </xfa:datasets>\n");
    }

    out.push_str("</xdp:xdp>\n");
    out
}

// -- helpers --

fn collect_fields(doc: &Document, id: ObjectId, prefix: &str, out: &mut Vec<XmlFormField>) {
    let Ok(obj) = doc.get_object(id) else { return };
    let Ok(dict) = obj.as_dict() else { return };

    let name = dict
        .get(b"T")
        .ok()
        .and_then(|o| match o {
            Object::String(s, _) => String::from_utf8(s.clone()).ok(),
            _ => None,
        })
        .unwrap_or_default();

    let fqn = if prefix.is_empty() {
        name.clone()
    } else {
        format!("{prefix}.{name}")
    };

    let field_type = dict
        .get(b"FT")
        .ok()
        .and_then(|o| match o {
            Object::Name(n) => String::from_utf8(n.clone()).ok(),
            _ => None,
        })
        .map(|ft| match ft.as_str() {
            "Tx" => "Text".to_string(),
            "Btn" => "Button".to_string(),
            "Ch" => "Choice".to_string(),
            "Sig" => "Signature".to_string(),
            _ => ft,
        })
        .unwrap_or_else(|| "Text".to_string());

    let value = dict.get(b"V").ok().and_then(|o| match o {
        Object::String(s, _) => String::from_utf8(s.clone()).ok(),
        Object::Name(n) => String::from_utf8(n.clone()).ok(),
        Object::Integer(i) => Some(i.to_string()),
        Object::Real(f) => Some(f.to_string()),
        Object::Boolean(b) => Some(b.to_string()),
        _ => None,
    });

    let kid_ids: Vec<ObjectId> = dict
        .get(b"Kids")
        .ok()
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
        .unwrap_or_default();

    if kid_ids.is_empty() || value.is_some() {
        out.push(XmlFormField {
            name: fqn.clone(),
            field_type,
            value,
        });
    }

    for kid_id in kid_ids {
        collect_fields(doc, kid_id, &fqn, out);
    }
}

fn name_to_fdf_field(fqn: &str, value: Option<&str>) -> crate::fdf::FdfField {
    let parts: Vec<&str> = fqn.split('.').collect();
    if parts.len() <= 1 {
        crate::fdf::FdfField {
            name: fqn.to_string(),
            value: value.map(String::from),
            kids: vec![],
        }
    } else {
        // Build nested structure from dot-separated name.
        let mut current = crate::fdf::FdfField {
            name: parts.last().unwrap().to_string(),
            value: value.map(String::from),
            kids: vec![],
        };
        for &part in parts[..parts.len() - 1].iter().rev() {
            current = crate::fdf::FdfField {
                name: part.to_string(),
                value: None,
                kids: vec![current],
            };
        }
        current
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acroform_xml_roundtrip() {
        let form = AcroFormXml {
            fields: vec![
                XmlFormField {
                    name: "name".into(),
                    field_type: "Text".into(),
                    value: Some("Alice".into()),
                },
                XmlFormField {
                    name: "agree".into(),
                    field_type: "Button".into(),
                    value: Some("true".into()),
                },
                XmlFormField {
                    name: "empty".into(),
                    field_type: "Text".into(),
                    value: None,
                },
            ],
        };

        let xml = form.to_xml();
        assert!(xml.contains("<acroform-data>"));
        assert!(xml.contains("name=\"name\""));

        let parsed = AcroFormXml::from_xml(&xml).unwrap();
        assert_eq!(parsed.fields.len(), 3);
        assert_eq!(parsed.fields[0].value.as_deref(), Some("Alice"));
        assert_eq!(parsed.fields[1].field_type, "Button");
        assert!(parsed.fields[2].value.is_none());
    }

    #[test]
    fn xdp_generation() {
        let xdp = generate_xdp("<subform name=\"root\"/>", Some("<data/>"));
        assert!(xdp.contains("xmlns:xdp=\"http://ns.adobe.com/xdp/\""));
        assert!(xdp.contains("<subform name=\"root\"/>"));
        assert!(xdp.contains("<data/>"));
    }

    #[test]
    fn xdp_without_datasets() {
        let xdp = generate_xdp("<subform/>", None);
        assert!(!xdp.contains("xfa:datasets"));
    }

    #[test]
    fn name_to_fdf_single_segment() {
        let f = name_to_fdf_field("name", Some("Alice"));
        assert_eq!(f.name, "name");
        assert_eq!(f.value.as_deref(), Some("Alice"));
        assert!(f.kids.is_empty());
    }

    #[test]
    fn name_to_fdf_multi_segment() {
        let f = name_to_fdf_field("form.address.city", Some("Amsterdam"));
        assert_eq!(f.name, "form");
        assert!(f.value.is_none());
        assert_eq!(f.kids[0].name, "address");
        assert_eq!(f.kids[0].kids[0].name, "city");
        assert_eq!(f.kids[0].kids[0].value.as_deref(), Some("Amsterdam"));
    }
}
