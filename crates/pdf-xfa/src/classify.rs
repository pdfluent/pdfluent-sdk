//! XFA form type classification — static vs dynamic vs none.
//!
//! XFA Spec 3.3 §9.1 distinguishes two XFA form profiles:
//!
//! - **Static (XFAF)**: `baseProfile="interactiveForms"` on `<template>`.
//!   Layout is fixed; form fields are baked in as AcroForm widgets.
//! - **Dynamic**: full XFA grammar; layout is re-computed from data at render
//!   time.  No `baseProfile` restriction.
//!
//! Use [`detect_xfa_type_from_packets`] when you already have [`XfaPackets`],
//! or [`detect_xfa_type`] to parse raw PDF bytes end-to-end.
//!
//! # Example
//! ```rust,ignore
//! use pdf_xfa::classify::{detect_xfa_type, XfaType};
//! match detect_xfa_type(&pdf_bytes) {
//!     XfaType::Static  => println!("XFAF / static form"),
//!     XfaType::Dynamic => println!("full dynamic XFA"),
//!     XfaType::None    => println!("not an XFA PDF"),
//! }
//! ```

use crate::extract::{extract_xfa_from_bytes, XfaPackets};

/// Classification of an XFA form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfaType {
    /// Not an XFA PDF (no XFA content found).
    None,
    /// Static XFA form (`baseProfile="interactiveForms"`).
    /// Layout is fixed; content is pre-rendered in the PDF page streams.
    Static,
    /// Dynamic XFA form — full XFA grammar, layout computed at runtime.
    Dynamic,
}

impl std::fmt::Display for XfaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XfaType::None => write!(f, "None"),
            XfaType::Static => write!(f, "Static"),
            XfaType::Dynamic => write!(f, "Dynamic"),
        }
    }
}

/// Detect XFA type from already-extracted [`XfaPackets`].
///
/// Returns [`XfaType::None`] when there are no packets and no full XML.
pub fn detect_xfa_type_from_packets(packets: &XfaPackets) -> XfaType {
    // No packets and no monolithic XML → not an XFA document.
    if packets.packets.is_empty() && packets.full_xml.is_none() {
        return XfaType::None;
    }

    // Look for baseProfile="interactiveForms" in the template packet first,
    // then fall back to searching the full XDP XML blob.
    let template_xml: Option<&str> = packets.template();
    let full_xml: Option<&str> = packets.full_xml.as_deref();

    let search_text: &str = template_xml.or(full_xml).unwrap_or("");

    if search_text.contains(r#"baseProfile="interactiveForms""#) {
        XfaType::Static
    } else if !search_text.is_empty() || packets.packets.iter().any(|(name, _)| name == "template")
    {
        XfaType::Dynamic
    } else {
        // We have something (datasets-only, config-only, etc.) but no template.
        // Treat as Dynamic — it is at minimum an XFA artefact.
        XfaType::Dynamic
    }
}

/// Detect XFA type from raw PDF bytes.
///
/// Parses and extracts XFA packets, then classifies the form.
/// Returns [`XfaType::None`] when the PDF has no XFA content.
pub fn detect_xfa_type(pdf_bytes: &[u8]) -> XfaType {
    // extract_xfa_from_bytes requires Into<PdfData>; Vec<u8> satisfies that.
    match extract_xfa_from_bytes(pdf_bytes.to_vec()) {
        Ok(packets) => detect_xfa_type_from_packets(&packets),
        Err(_) => XfaType::None,
    }
}

// ─── unit tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::XfaPackets;

    fn packets_from_xml(xml: &str) -> XfaPackets {
        // Use the internal parse helper via extract module's public surface.
        // We build XfaPackets by hand to keep tests self-contained.
        let mut p = XfaPackets {
            full_xml: Some(xml.to_string()),
            ..Default::default()
        };
        // Manually push the template packet so get_packet("template") works.
        if xml.contains("<template") {
            let start = xml.find("<template").unwrap();
            // Find the closing tag — handle both qualified and plain versions.
            let end = xml
                .find("</template>")
                .map(|i| i + "</template>".len())
                .or({
                    // self-closing or other variant — take to end of xml as fallback
                    Some(xml.len())
                })
                .unwrap();
            p.packets
                .push(("template".to_string(), xml[start..end].to_string()));
        }
        p
    }

    #[test]
    fn empty_packets_returns_none() {
        let p = XfaPackets::default();
        assert_eq!(detect_xfa_type_from_packets(&p), XfaType::None);
    }

    #[test]
    fn static_form_detected_via_base_profile() {
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/" baseProfile="interactiveForms"><subform name="root"/></template></xdp:xdp>"#;
        let p = packets_from_xml(xml);
        assert_eq!(detect_xfa_type_from_packets(&p), XfaType::Static);
    }

    #[test]
    fn dynamic_form_detected_when_no_base_profile() {
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><occur min="0" max="-1"/></subform></template></xdp:xdp>"#;
        let p = packets_from_xml(xml);
        assert_eq!(detect_xfa_type_from_packets(&p), XfaType::Dynamic);
    }

    #[test]
    fn xfa_type_none_on_empty_pdf_bytes() {
        // Empty slice cannot be a valid PDF → XfaType::None.
        assert_eq!(detect_xfa_type(&[]), XfaType::None);
    }

    #[test]
    fn xfa_type_display() {
        assert_eq!(XfaType::None.to_string(), "None");
        assert_eq!(XfaType::Static.to_string(), "Static");
        assert_eq!(XfaType::Dynamic.to_string(), "Dynamic");
    }

    #[test]
    fn packets_with_only_full_xml_static() {
        // Simulate a monolithic XDP stream (full_xml set, packets empty).
        let xml = r#"<?xml version="1.0"?><xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template baseProfile="interactiveForms"><subform/></template></xdp:xdp>"#;
        let p = XfaPackets {
            full_xml: Some(xml.to_string()),
            ..Default::default()
        };
        // No individual packets — detect from full_xml fallback.
        assert_eq!(detect_xfa_type_from_packets(&p), XfaType::Static);
    }

    #[test]
    fn datasets_only_packet_treated_as_dynamic() {
        let mut p = XfaPackets::default();
        p.packets
            .push(("datasets".to_string(), "<xfa:datasets/>".to_string()));
        // datasets-only is still an XFA artefact — Dynamic.
        assert_eq!(detect_xfa_type_from_packets(&p), XfaType::Dynamic);
    }
}
