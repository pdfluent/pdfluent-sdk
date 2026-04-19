//! XFA packet extraction from PDF via pdf-syntax.
use crate::error::{Result, XfaError};
use pdf_syntax::object::dict::keys::{ACRO_FORM, XFA};
use pdf_syntax::object::{Array, Dict, Object, Stream};
use pdf_syntax::Pdf;

#[derive(Debug, Clone, Default)]
pub struct XfaPackets {
    pub full_xml: Option<String>,
    pub packets: Vec<(String, String)>,
}

impl XfaPackets {
    pub fn get_packet(&self, name: &str) -> Option<&str> {
        self.packets
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
    pub fn template(&self) -> Option<&str> {
        self.get_packet("template")
    }
    pub fn datasets(&self) -> Option<&str> {
        // When multiple "datasets" packets exist (e.g. from incremental saves),
        // prefer the largest one — the small/empty one is the original blank form
        // and the larger one contains the filled data.
        self.packets
            .iter()
            .filter(|(n, _)| n == "datasets")
            .max_by_key(|(_, v)| v.len())
            .map(|(_, v)| v.as_str())
    }
    pub fn config(&self) -> Option<&str> {
        self.get_packet("config")
    }
    pub fn locale_set(&self) -> Option<&str> {
        self.get_packet("localeSet")
    }
}

pub fn extract_xfa(pdf: &Pdf) -> Result<XfaPackets> {
    if let Some(mut p) = extract_xfa_from_acroform(pdf) {
        if !p.packets.is_empty() || p.full_xml.is_some() {
            // If the datasets packet is empty/tiny (common with incremental saves
            // where Adobe Reader writes a new datasets object but doesn't update
            // the XFA array reference), scan all objects for a larger one.
            let current_ds_len = p.datasets().map(|s| s.len()).unwrap_or(0);
            if current_ds_len < 200 {
                if let Some(better_ds) = scan_for_datasets(pdf, current_ds_len) {
                    p.packets.push(("datasets".to_string(), better_ds));
                }
            }
            return Ok(p);
        }
    }
    scan_for_xfa(pdf)
}

/// Scan all PDF stream objects for a datasets packet larger than `min_len`.
/// Returns the largest found, if any.
fn scan_for_datasets(pdf: &Pdf, min_len: usize) -> Option<String> {
    let mut best: Option<String> = None;
    for obj in pdf.objects() {
        if let Object::Stream(s) = obj {
            if let Some(d) = decode_stream(&s) {
                if d.len() > min_len
                    && d.contains("<xfa:datasets")
                    && best.as_ref().is_none_or(|b| d.len() > b.len())
                {
                    best = Some(d);
                }
            }
        }
    }
    best
}

pub fn extract_xfa_from_bytes(data: impl Into<pdf_syntax::PdfData>) -> Result<XfaPackets> {
    let pdf = Pdf::new(data).map_err(|e| XfaError::LoadFailed(format!("{e:?}")))?;
    extract_xfa(&pdf)
}

pub fn extract_xfa_from_acroform(pdf: &Pdf) -> Option<XfaPackets> {
    let xref = pdf.xref();
    let catalog: Dict<'_> = xref.get(xref.root_id())?;
    let acroform: Dict<'_> = catalog.get(ACRO_FORM)?;
    if let Some(stream) = acroform.get::<Stream<'_>>(XFA) {
        return Some(parse_xfa_xml(&decode_stream(&stream)?));
    }
    if let Some(array) = acroform.get::<Array<'_>>(XFA) {
        return Some(extract_from_array(&array));
    }
    None
}

fn extract_from_array(array: &Array<'_>) -> XfaPackets {
    let mut packets = XfaPackets::default();
    let items: Vec<Object<'_>> = array.iter::<Object<'_>>().collect();
    let mut i = 0;
    while i + 1 < items.len() {
        let name = match &items[i] {
            Object::String(s) => std::string::String::from_utf8_lossy(s.as_bytes()).to_string(),
            Object::Name(n) => std::string::String::from_utf8_lossy(n.as_ref()).to_string(),
            _ => {
                i += 1;
                continue;
            }
        };
        if let Some(c) = match &items[i + 1] {
            Object::Stream(s) => decode_stream(s),
            Object::String(s) => {
                Some(std::string::String::from_utf8_lossy(s.as_bytes()).to_string())
            }
            _ => None,
        } {
            packets.packets.push((name, c));
        }
        i += 2;
    }
    packets
}

fn scan_for_xfa(pdf: &Pdf) -> Result<XfaPackets> {
    // Cap the number of streams we decompress to avoid multi-second stalls on
    // large non-XFA PDFs. XFA XDP streams are typically among the first few
    // hundred objects. If we haven't found one after 2000 streams, give up.
    let mut streams_checked = 0u32;
    for obj in pdf.objects() {
        if let Object::Stream(s) = obj {
            streams_checked += 1;
            if streams_checked > 2000 {
                break;
            }
            if let Some(d) = decode_stream(&s) {
                if d.contains("<xdp:xdp") {
                    return Ok(parse_xfa_xml(&d));
                }
            }
        }
    }
    Err(XfaError::PacketNotFound("no XFA content found".to_string()))
}

fn decode_stream(stream: &Stream<'_>) -> Option<String> {
    std::string::String::from_utf8(stream.decoded().ok()?).ok()
}

fn parse_xfa_xml(xml: &str) -> XfaPackets {
    let mut packets = XfaPackets {
        full_xml: Some(xml.to_string()),
        packets: Vec::new(),
    };
    let t = xml.trim();
    let c = t.find("?>").map(|p| &t[p + 2..]).unwrap_or(t).trim();
    let inner = match c.find('>') {
        Some(s) => {
            let rest = &c[s + 1..];
            rest.rfind("</xdp:xdp>")
                .map(|e| &rest[..e])
                .or_else(|| rest.rfind("</xdp>").map(|e| &rest[..e]))
                .unwrap_or(rest)
        }
        None => return packets,
    };
    let mut pos = 0;
    let bytes = inner.as_bytes();
    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }
        if bytes[pos] != b'<' {
            pos += 1;
            continue;
        }
        if inner[pos..].starts_with("<!--") {
            if let Some(e) = inner[pos..].find("-->") {
                pos += e + 3;
                continue;
            }
        }
        if inner[pos..].starts_with("<?") {
            if let Some(e) = inner[pos..].find("?>") {
                pos += e + 2;
                continue;
            }
        }
        let ts = pos;
        pos += 1;
        let ns = pos;
        while pos < bytes.len() && bytes[pos] != b'>' && bytes[pos] != b' ' && bytes[pos] != b'/' {
            pos += 1;
        }
        let ft = &inner[ns..pos];
        let pn = ft.split(':').next_back().unwrap_or(ft);
        let ct = format!("</{ft}>");
        let at = format!("</xfa:{pn}>");
        if let Some(cp) = inner[ts..].find(ct.as_str()) {
            let ee = ts + cp + ct.len();
            packets
                .packets
                .push((pn.to_string(), inner[ts..ee].to_string()));
            pos = ee;
        } else if let Some(cp) = inner[ts..].find(at.as_str()) {
            let ee = ts + cp + at.len();
            packets
                .packets
                .push((pn.to_string(), inner[ts..ee].to_string()));
            pos = ee;
        } else {
            while pos < bytes.len() && bytes[pos] != b'>' {
                pos += 1;
            }
            pos += 1;
        }
    }
    packets
}

// ─── Packet validation ───────────────────────────────────────────────────────

/// Summary of what was found (or missing) in a set of [`XfaPackets`].
///
/// Returned by [`validate_xfa_packets`].  Intended for diagnostics, logging,
/// and deciding how to handle unusual or incomplete XFA documents.
#[derive(Debug, Clone, Default)]
pub struct PacketValidation {
    /// `true` when a `template` packet is present.
    pub has_template: bool,
    /// `true` when at least one `datasets` packet is present.
    pub has_datasets: bool,
    /// `true` when a `config` packet is present.
    pub has_config: bool,
    /// Byte length of the template packet (0 if absent).
    pub template_bytes: usize,
    /// Byte length of the largest datasets packet (0 if absent).
    pub datasets_bytes: usize,
    /// Names of all packets in document order.
    pub packet_names: Vec<String>,
    /// Human-readable warnings about missing or suspicious content.
    pub warnings: Vec<String>,
}

/// Validate the contents of [`XfaPackets`] and return a [`PacketValidation`].
///
/// This function never panics and never fails — it always returns a result,
/// even for empty or degenerate packet sets.
pub fn validate_xfa_packets(packets: &XfaPackets) -> PacketValidation {
    let has_template = packets.template().is_some();
    let has_datasets = packets.datasets().is_some();
    let has_config = packets.config().is_some();

    let template_bytes = packets.template().map(|s| s.len()).unwrap_or(0);
    let datasets_bytes = packets.datasets().map(|s| s.len()).unwrap_or(0);
    let packet_names = packets.packets.iter().map(|(n, _)| n.clone()).collect();

    let mut warnings = Vec::new();

    if !has_template {
        warnings.push("No template packet found".to_string());
    } else if template_bytes < 100 {
        warnings.push(format!(
            "Template packet is empty (< 100 bytes) — only {template_bytes} bytes"
        ));
    }

    if !has_datasets {
        warnings.push("No datasets packet".to_string());
    } else if datasets_bytes < 50 {
        warnings.push(format!(
            "Datasets packet is suspiciously small (< 50 bytes) — only {datasets_bytes} bytes"
        ));
    }

    PacketValidation {
        has_template,
        has_datasets,
        has_config,
        template_bytes,
        datasets_bytes,
        packet_names,
        warnings,
    }
}

// ─── Embedded font extraction ────────────────────────────────────────────────

pub fn extract_embedded_fonts(pdf: &Pdf) -> Vec<(String, Vec<u8>)> {
    use pdf_syntax::object::dict::keys::{FONT_FILE, FONT_FILE2, FONT_FILE3, FONT_NAME, TYPE};
    use pdf_syntax::object::Name;
    let mut fonts = Vec::new();
    for obj in pdf.objects() {
        let dict = match &obj {
            Object::Dict(d) => d.clone(),
            Object::Stream(s) => s.dict().clone(),
            _ => continue,
        };
        if dict
            .get::<Name>(TYPE)
            .is_none_or(|n| n.as_ref() != b"FontDescriptor")
        {
            continue;
        }
        let name = dict
            .get::<Name>(FONT_NAME)
            .map(|n| std::string::String::from_utf8_lossy(n.as_ref()).to_string())
            .unwrap_or_default();
        for key in [FONT_FILE2, FONT_FILE, FONT_FILE3] {
            if let Some(s) = dict.get::<Stream<'_>>(key) {
                if let Ok(d) = s.decoded() {
                    if !d.is_empty() {
                        fonts.push((name.clone(), d));
                        break;
                    }
                }
            }
        }
    }
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_xfa_packets() {
        let xml = r#"<?xml version="1.0"?><xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="f1"><field name="T1"/></subform></template><xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><f1><T1>Hi</T1></f1></xfa:data></xfa:datasets></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        assert_eq!(p.packets.len(), 2);
        assert!(p.template().is_some());
        assert!(p.datasets().is_some());
    }
    #[test]
    fn empty_xfa() {
        let p = parse_xfa_xml(r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"></xdp:xdp>"#);
        assert_eq!(p.packets.len(), 0);
    }

    #[test]
    fn get_packet_missing_returns_none() {
        let p = parse_xfa_xml(r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"></xdp:xdp>"#);
        assert!(p.get_packet("template").is_none());
        assert!(p.get_packet("nonexistent").is_none());
        assert!(p.config().is_none());
        assert!(p.locale_set().is_none());
    }

    #[test]
    fn full_xml_preserved() {
        // full_xml should always capture the entire input string.
        let xml =
            r#"<?xml version="1.0"?><xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        let stored = p.full_xml.as_deref().unwrap_or("");
        assert!(stored.contains("xdp:xdp"));
    }

    #[test]
    fn config_packet_parsed() {
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><config xmlns="http://www.xfa.org/schema/xci/3.1/"><present><xdp><packets>*</packets></xdp></present></config></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        assert_eq!(p.packets.len(), 1);
        assert!(p.config().is_some());
        assert!(p.template().is_none());
    }

    #[test]
    fn multiple_packets_order_preserved() {
        // template must come before datasets — order matches the XDP source order.
        let xml = r#"<?xml version="1.0"?><xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"/></template><xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data/></xfa:datasets></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        assert_eq!(p.packets.len(), 2);
        assert_eq!(p.packets[0].0, "template");
        assert_eq!(p.packets[1].0, "datasets");
        assert!(p.template().is_some());
        assert!(p.datasets().is_some());
    }

    // ── PacketValidation tests (issue #1085) ──────────────────────────────

    #[test]
    fn validate_complete_packets_no_warnings() {
        let xml = r#"<?xml version="1.0"?><xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><field name="firstName" xmlns:ui="http://www.xfa.org/schema/xfa-template/3.3/"><ui><textEdit/></ui></field></subform></template><xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><root><firstName>Alice</firstName></root></xfa:data></xfa:datasets></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        let v = validate_xfa_packets(&p);
        assert!(v.has_template);
        assert!(v.has_datasets);
        assert!(v.template_bytes > 0);
        assert!(v.datasets_bytes > 0);
        assert!(v.warnings.is_empty(), "expected no warnings, got: {:?}", v.warnings);
    }

    #[test]
    fn validate_missing_template_produces_warning() {
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data/></xfa:datasets></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        let v = validate_xfa_packets(&p);
        assert!(!v.has_template);
        assert!(v.warnings.iter().any(|w| w.contains("No template packet found")));
    }

    #[test]
    fn validate_missing_datasets_produces_warning() {
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><field name="x"/><field name="y"/><field name="z"/><field name="w"/></subform></template></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        let v = validate_xfa_packets(&p);
        assert!(!v.has_datasets);
        assert!(v.warnings.iter().any(|w| w.contains("No datasets packet")));
    }

    #[test]
    fn validate_tiny_template_produces_warning() {
        let mut p = XfaPackets::default();
        p.packets.push(("template".to_string(), "<t/>".to_string()));
        p.packets.push((
            "datasets".to_string(),
            "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data/></xfa:datasets>".to_string(),
        ));
        let v = validate_xfa_packets(&p);
        assert!(v.warnings.iter().any(|w| w.contains("< 100 bytes")));
    }

    #[test]
    fn validate_tiny_datasets_produces_warning() {
        let mut p = XfaPackets::default();
        // Give a substantial template so that warning comes from datasets only.
        p.packets.push((
            "template".to_string(),
            "<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\"><field name=\"a\"/><field name=\"b\"/><field name=\"c\"/></subform></template>".to_string(),
        ));
        p.packets.push(("datasets".to_string(), "<ds/>".to_string()));
        let v = validate_xfa_packets(&p);
        assert!(v.warnings.iter().any(|w| w.contains("< 50 bytes")));
    }

    #[test]
    fn validate_packet_names_list() {
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><config xmlns="http://www.xfa.org/schema/xci/3.1/"><present/></config><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><field name="f1"/><field name="f2"/><field name="f3"/></subform></template></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        let v = validate_xfa_packets(&p);
        assert!(v.packet_names.contains(&"config".to_string()));
        assert!(v.packet_names.contains(&"template".to_string()));
        assert!(v.has_config);
    }

    // ── XFA corpus tests (issue #1086) ────────────────────────────────────
    // Ten synthetic tests covering representative XFA document patterns.
    // Each test uses small in-memory XML strings — no real PDFs required.

    /// 1. Static XFA form detection via baseProfile.
    #[test]
    fn corpus_01_static_xfa_form_detection() {
        use crate::classify::{detect_xfa_type_from_packets, XfaType};
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/" baseProfile="interactiveForms"><subform name="Page1"><field name="LastName"/><field name="FirstName"/></subform></template></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        assert_eq!(detect_xfa_type_from_packets(&p), XfaType::Static);
    }

    /// 2. Dynamic XFA form detection (no baseProfile constraint).
    #[test]
    fn corpus_02_dynamic_xfa_form_detection() {
        use crate::classify::{detect_xfa_type_from_packets, XfaType};
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><occur min="0" max="-1"/><field name="item"/></subform></template></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        assert_eq!(detect_xfa_type_from_packets(&p), XfaType::Dynamic);
    }

    /// 3. XFA with multiple packets (template + datasets + config).
    #[test]
    fn corpus_03_multiple_packets() {
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><config xmlns="http://www.xfa.org/schema/xci/3.1/"><present><xdp><packets>*</packets></xdp></present></config><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"/></template><xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data/></xfa:datasets></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        assert_eq!(p.packets.len(), 3, "should have config, template, datasets");
        assert!(p.config().is_some());
        assert!(p.template().is_some());
        assert!(p.datasets().is_some());
    }

    /// 4. XFA with no datasets (template-only — blank form, no data bound).
    #[test]
    fn corpus_04_template_only_no_datasets() {
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><field name="LastName"/><field name="FirstName"/><field name="DOB"/></subform></template></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        assert!(p.template().is_some());
        assert!(p.datasets().is_none());
        let v = validate_xfa_packets(&p);
        assert!(!v.has_datasets);
        assert!(v.warnings.iter().any(|w| w.contains("No datasets")));
    }

    /// 5. XFA with binary-like image data embedded in datasets (base64 blob).
    #[test]
    fn corpus_05_xfa_with_image_data_in_datasets() {
        // Simulate datasets containing a base64-encoded image field.
        let b64_image = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let xml = format!(
            r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><field name="photo"><ui><imageEdit/></ui></field></subform></template><xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><root><photo contentType="image/png" href="">{b64_image}</photo></root></xfa:data></xfa:datasets></xdp:xdp>"#
        );
        let p = parse_xfa_xml(&xml);
        assert!(p.template().is_some());
        assert!(p.datasets().is_some());
        let ds = p.datasets().unwrap();
        assert!(ds.contains(b64_image), "datasets should contain image data");
    }

    /// 6. Non-XFA PDF (empty bytes) returns XfaType::None.
    #[test]
    fn corpus_06_non_xfa_pdf_returns_none() {
        use crate::classify::{detect_xfa_type, XfaType};
        // A plain PDF header with no AcroForm/XFA.
        let not_xfa: &[u8] = b"%PDF-1.4\n%%EOF";
        assert_eq!(detect_xfa_type(not_xfa), XfaType::None);
    }

    /// 7. XFA with config packet — config is correctly parsed and accessible.
    #[test]
    fn corpus_07_xfa_with_config_packet() {
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><config xmlns="http://www.xfa.org/schema/xci/3.1/"><present><xdp><packets>*</packets></xdp></present><pdf><version>1.6</version></pdf></config><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"/></template></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        assert!(p.config().is_some());
        let cfg = p.config().unwrap();
        assert!(cfg.contains("packets"));
        let v = validate_xfa_packets(&p);
        assert!(v.has_config);
    }

    /// 8. Empty datasets packet (incremental save pattern — original blank form).
    #[test]
    fn corpus_08_empty_datasets_incremental_save_pattern() {
        // Two datasets entries: original blank (small) and filled (larger).
        let mut p = XfaPackets::default();
        p.packets.push((
            "template".to_string(),
            "<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\"><field name=\"qty\"/><field name=\"price\"/><field name=\"total\"/></subform></template>".to_string(),
        ));
        // Blank (incremental save artefact — very small):
        p.packets.push(("datasets".to_string(), "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"/>".to_string()));
        // Filled (the real data):
        p.packets.push(("datasets".to_string(), "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root><qty>3</qty><price>9.99</price><total>29.97</total></root></xfa:data></xfa:datasets>".to_string()));
        // datasets() must return the LARGEST entry.
        let ds = p.datasets().expect("datasets should exist");
        assert!(ds.contains("29.97"), "should return the larger/filled datasets");
    }

    /// 9. Large template with many fields — validation should have no warnings.
    #[test]
    fn corpus_09_large_template_many_fields() {
        // Build a template with 20 fields to ensure validation handles size correctly.
        let fields: String = (1..=20)
            .map(|i| format!("<field name=\"field{i}\"><ui><textEdit/></ui></field>"))
            .collect();
        let xml = format!(
            r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root">{fields}</subform></template><xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><root>{}</root></xfa:data></xfa:datasets></xdp:xdp>"#,
            (1..=20).map(|i| format!("<field{i}>val{i}</field{i}>")).collect::<String>()
        );
        let p = parse_xfa_xml(&xml);
        let v = validate_xfa_packets(&p);
        assert!(v.has_template);
        assert!(v.has_datasets);
        assert!(v.template_bytes >= 100, "large template should exceed 100 bytes");
        assert!(v.warnings.is_empty(), "no warnings expected: {:?}", v.warnings);
    }

    /// 10. XFA with localeSet packet — localeSet is correctly accessible.
    #[test]
    fn corpus_10_xfa_with_locale_set_packet() {
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><localeSet xmlns="http://www.xfa.org/schema/xfa-locale-set/2.7/"><locale name="en_US" desc="English (United States)"><calendarSymbols name="gregorian"/></locale></localeSet><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><field name="date"/></subform></template></xdp:xdp>"#;
        let p = parse_xfa_xml(xml);
        assert!(p.locale_set().is_some(), "localeSet packet should be accessible");
        assert!(p.template().is_some());
        let ls = p.locale_set().unwrap();
        assert!(ls.contains("en_US"));
    }
}
