//! XFDF (XML Forms Data Format) import/export.
//!
//! XFDF is the XML-based counterpart of FDF (see [`crate::fdf`]).  It uses
//! the `http://ns.adobe.com/xfdf/` namespace and carries form field values
//! plus optional annotation data.

use crate::error::{InvoiceError, Result};
use lopdf::{Document, ObjectId};

/// An XFDF document.
#[derive(Debug, Clone)]
pub struct XfdfDocument {
    /// Form field values (may be nested).
    pub fields: Vec<XfdfField>,
    /// Optional source PDF reference (the `href` attribute on `<f/>`).
    pub source: Option<String>,
}

/// A single XFDF field entry.
#[derive(Debug, Clone)]
pub struct XfdfField {
    /// Partial field name.
    pub name: String,
    /// Field value (leaf nodes).
    pub value: Option<String>,
    /// Nested child fields.
    pub children: Vec<XfdfField>,
}

const XFDF_NS: &str = "http://ns.adobe.com/xfdf/";

impl XfdfDocument {
    /// Parse an XFDF document from an XML string.
    pub fn from_xml(xml: &str) -> Result<Self> {
        let doc = roxmltree::Document::parse(xml)
            .map_err(|e| InvoiceError::Xml(format!("XFDF parse error: {e}")))?;
        let root = doc.root_element();

        let source = root
            .children()
            .find(|n| n.has_tag_name("f"))
            .and_then(|n| n.attribute("href").map(String::from));

        let fields_node = root.children().find(|n| n.has_tag_name("fields"));
        let fields = fields_node
            .map(|node| parse_field_children(&node))
            .unwrap_or_default();

        Ok(XfdfDocument { fields, source })
    }

    /// Serialize this XFDF document to an XML string.
    pub fn to_xml(&self) -> String {
        let mut out = String::with_capacity(512);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str(&format!(
            "<xfdf xmlns=\"{XFDF_NS}\" xml:space=\"preserve\">\n"
        ));

        if let Some(ref src) = self.source {
            out.push_str(&format!("  <f href=\"{}\"/>\n", xml_escape(src)));
        }

        out.push_str("  <fields>\n");
        for field in &self.fields {
            write_field(&mut out, field, 2);
        }
        out.push_str("  </fields>\n");
        out.push_str("</xfdf>\n");
        out
    }

    /// Export form field data from a `lopdf::Document` into an `XfdfDocument`.
    pub fn export_from(doc: &Document) -> Result<Self> {
        let fdf = crate::fdf::FdfDocument::export_from(doc)?;
        Ok(XfdfDocument {
            fields: fdf_fields_to_xfdf(&fdf.fields),
            source: None,
        })
    }

    /// Import field values from this XFDF into a `lopdf::Document`.
    pub fn import_into(&self, doc: &mut Document) -> Result<()> {
        let acroform_id = crate::fdf::find_acroform_id(doc)?;
        let acroform = doc
            .get_object(acroform_id)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .ok_or_else(|| InvoiceError::Parse("AcroForm not a dict".into()))?;

        let field_ids: Vec<ObjectId> = acroform
            .get(b"Fields")
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|arr| arr.iter().filter_map(|o| o.as_reference().ok()).collect())
            .unwrap_or_default();

        let flat = flatten_xfdf_fields(&self.fields, "");
        for (fqn, value) in &flat {
            crate::fdf::set_field_value_by_name(doc, &field_ids, fqn, value)?;
        }
        Ok(())
    }
}

// -- XML writing helpers --

fn write_field(out: &mut String, field: &XfdfField, indent: usize) {
    let pad = "  ".repeat(indent);
    out.push_str(&format!(
        "{pad}  <field name=\"{}\">\n",
        xml_escape(&field.name)
    ));
    if let Some(ref val) = field.value {
        out.push_str(&format!("{pad}    <value>{}</value>\n", xml_escape(val)));
    }
    for child in &field.children {
        write_field(out, child, indent + 1);
    }
    out.push_str(&format!("{pad}  </field>\n"));
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// -- XML parsing helpers --

fn parse_field_children(node: &roxmltree::Node) -> Vec<XfdfField> {
    node.children()
        .filter(|n| n.has_tag_name("field"))
        .filter_map(|n| {
            let name = n.attribute("name")?.to_string();
            let value = n
                .children()
                .find(|c| c.has_tag_name("value"))
                .and_then(|v| v.text().map(String::from));
            let children = parse_field_children(&n);
            Some(XfdfField {
                name,
                value,
                children,
            })
        })
        .collect()
}

// -- Conversion helpers --

fn fdf_fields_to_xfdf(fields: &[crate::fdf::FdfField]) -> Vec<XfdfField> {
    fields
        .iter()
        .map(|f| XfdfField {
            name: f.name.clone(),
            value: f.value.clone(),
            children: fdf_fields_to_xfdf(&f.kids),
        })
        .collect()
}

fn flatten_xfdf_fields(fields: &[XfdfField], prefix: &str) -> Vec<(String, String)> {
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
        out.extend(flatten_xfdf_fields(&f.children, &fqn));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_xfdf_xml() {
        let xfdf = XfdfDocument {
            fields: vec![
                XfdfField {
                    name: "name".into(),
                    value: Some("Alice".into()),
                    children: vec![],
                },
                XfdfField {
                    name: "address".into(),
                    value: None,
                    children: vec![XfdfField {
                        name: "city".into(),
                        value: Some("Amsterdam".into()),
                        children: vec![],
                    }],
                },
            ],
            source: Some("form.pdf".into()),
        };

        let xml = xfdf.to_xml();
        assert!(xml.contains("xmlns=\"http://ns.adobe.com/xfdf/\""));
        assert!(xml.contains("<f href=\"form.pdf\"/>"));

        let parsed = XfdfDocument::from_xml(&xml).unwrap();
        assert_eq!(parsed.fields.len(), 2);
        assert_eq!(parsed.fields[0].name, "name");
        assert_eq!(parsed.fields[0].value.as_deref(), Some("Alice"));
        assert_eq!(
            parsed.fields[1].children[0].value.as_deref(),
            Some("Amsterdam")
        );
        assert_eq!(parsed.source.as_deref(), Some("form.pdf"));
    }

    #[test]
    fn parse_xfdf_with_special_chars() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xfdf xmlns="http://ns.adobe.com/xfdf/" xml:space="preserve">
  <fields>
    <field name="company">
      <value>Smith &amp; Jones</value>
    </field>
  </fields>
</xfdf>"#;

        let parsed = XfdfDocument::from_xml(xml).unwrap();
        assert_eq!(parsed.fields[0].value.as_deref(), Some("Smith & Jones"));
    }

    #[test]
    fn flatten_nested_fields() {
        let fields = vec![XfdfField {
            name: "form".into(),
            value: None,
            children: vec![
                XfdfField {
                    name: "first".into(),
                    value: Some("A".into()),
                    children: vec![],
                },
                XfdfField {
                    name: "last".into(),
                    value: Some("B".into()),
                    children: vec![],
                },
            ],
        }];
        let flat = flatten_xfdf_fields(&fields, "");
        assert_eq!(flat.len(), 2);
        assert!(flat.iter().any(|(k, v)| k == "form.first" && v == "A"));
        assert!(flat.iter().any(|(k, v)| k == "form.last" && v == "B"));
    }

    #[test]
    fn empty_xfdf() {
        let xfdf = XfdfDocument {
            fields: vec![],
            source: None,
        };
        let xml = xfdf.to_xml();
        let parsed = XfdfDocument::from_xml(&xml).unwrap();
        assert!(parsed.fields.is_empty());
    }
}
