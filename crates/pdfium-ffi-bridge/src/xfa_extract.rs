//! XFA packet extraction from PDF bytes.
//!
//! XFA data is stored in the PDF as an XFA stream referenced from the AcroForm
//! dictionary. This module extracts the XML packets without needing the PDFium
//! library, by scanning the raw PDF bytes for XFA XML content.

use crate::error::{PdfError, Result};

/// Extracted XFA packets from a PDF.
#[derive(Debug, Clone, Default)]
pub struct XfaPackets {
    /// The full XFA XML (if stored as a single stream).
    pub full_xml: Option<String>,
    /// Individual packets keyed by name (template, datasets, config, etc.).
    pub packets: Vec<(String, String)>,
}

impl XfaPackets {
    /// Get a specific packet by name.
    pub fn get_packet(&self, name: &str) -> Option<&str> {
        self.packets
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// Get the template packet.
    pub fn template(&self) -> Option<&str> {
        self.get_packet("template")
    }

    /// Get the datasets packet.
    pub fn datasets(&self) -> Option<&str> {
        self.get_packet("datasets")
    }

    /// Get the config packet.
    pub fn config(&self) -> Option<&str> {
        self.get_packet("config")
    }
}

/// Extract XFA packets from a full XFA XML document.
///
/// The XFA XML wraps individual packets in `<xdp:xdp>` root element.
/// Each child element is a packet (template, datasets, config, etc.).
pub fn parse_xfa_xml(xml: &str) -> Result<XfaPackets> {
    let mut packets = XfaPackets {
        full_xml: Some(xml.to_string()),
        packets: Vec::new(),
    };

    // Simple extraction: find top-level elements within xdp:xdp
    // This avoids pulling in a full XML parser for this step
    let xml = xml.trim();

    // If it starts with <?xml, skip the declaration
    let content = if let Some(pos) = xml.find("?>") {
        &xml[pos + 2..]
    } else {
        xml
    };
    let content = content.trim();

    // Find the xdp:xdp root
    let inner = if let Some(start) = content.find('>') {
        let rest = &content[start + 1..];
        // Find closing </xdp:xdp>
        if let Some(end) = rest.rfind("</xdp:xdp>") {
            &rest[..end]
        } else if let Some(end) = rest.rfind("</xdp>") {
            &rest[..end]
        } else {
            rest
        }
    } else {
        return Ok(packets);
    };

    // Extract each top-level element as a packet
    let mut pos = 0;
    let bytes = inner.as_bytes();

    while pos < bytes.len() {
        // Skip whitespace
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }

        // Look for opening tag
        if bytes[pos] != b'<' {
            pos += 1;
            continue;
        }

        // Skip comments and processing instructions
        if inner[pos..].starts_with("<!--") {
            if let Some(end) = inner[pos..].find("-->") {
                pos += end + 3;
                continue;
            }
        }
        if inner[pos..].starts_with("<?") {
            if let Some(end) = inner[pos..].find("?>") {
                pos += end + 2;
                continue;
            }
        }

        // Extract element name
        let tag_start = pos;
        pos += 1; // skip '<'

        // Skip namespace prefix if present
        let name_start = pos;
        while pos < bytes.len() && bytes[pos] != b'>' && bytes[pos] != b' ' && bytes[pos] != b'/' {
            pos += 1;
        }
        let full_tag = &inner[name_start..pos];
        // Strip namespace prefix
        let packet_name = full_tag.split(':').next_back().unwrap_or(full_tag);

        // Find the end of this element
        // Look for matching closing tag
        let close_tag = format!("</{full_tag}>");
        let alt_close = format!("</{}:{}>", "xfa", packet_name);

        if let Some(close_pos) = inner[tag_start..].find(close_tag.as_str()) {
            let element_end = tag_start + close_pos + close_tag.len();
            let element = &inner[tag_start..element_end];
            packets
                .packets
                .push((packet_name.to_string(), element.to_string()));
            pos = element_end;
        } else if let Some(close_pos) = inner[tag_start..].find(alt_close.as_str()) {
            let element_end = tag_start + close_pos + alt_close.len();
            let element = &inner[tag_start..element_end];
            packets
                .packets
                .push((packet_name.to_string(), element.to_string()));
            pos = element_end;
        } else {
            // Self-closing or can't find end — skip to next '>'
            while pos < bytes.len() && bytes[pos] != b'>' {
                pos += 1;
            }
            pos += 1;
        }
    }

    Ok(packets)
}

/// Scan raw PDF bytes for XFA XML content.
///
/// This is a best-effort extraction that looks for XFA XML markers
/// in the PDF stream data. For production use, PDFium's API should
/// be used instead.
pub fn scan_pdf_for_xfa(pdf_bytes: &[u8]) -> Result<Option<XfaPackets>> {
    let content = std::str::from_utf8(pdf_bytes)
        .map_err(|_| PdfError::XfaPacketNotFound("not UTF-8 PDF".to_string()))?;

    // Look for XDP/XFA markers
    if let Some(start) = content.find("<xdp:xdp") {
        if let Some(end) = content[start..].find("</xdp:xdp>") {
            let xfa_xml = &content[start..start + end + 10];
            let packets = parse_xfa_xml(xfa_xml)?;
            return Ok(Some(packets));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_xfa_packets() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1">
      <field name="TextField1"/>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <form1><TextField1>Hello</TextField1></form1>
    </xfa:data>
  </xfa:datasets>
  <config xmlns="http://www.xfa.org/schema/xci/3.1/">
    <present><pdf><version>1.7</version></pdf></present>
  </config>
</xdp:xdp>"#;

        let packets = parse_xfa_xml(xml).unwrap();
        assert!(packets.full_xml.is_some());
        assert_eq!(packets.packets.len(), 3);

        assert!(packets.template().is_some());
        assert!(packets.get_packet("datasets").is_some());
        assert!(packets.config().is_some());
    }

    #[test]
    fn empty_xfa() {
        let xml = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"></xdp:xdp>"#;
        let packets = parse_xfa_xml(xml).unwrap();
        assert_eq!(packets.packets.len(), 0);
    }

    #[test]
    fn scan_no_xfa() {
        let pdf = b"%PDF-1.4\nsome content\n%%EOF";
        let result = scan_pdf_for_xfa(pdf).unwrap();
        assert!(result.is_none());
    }
}
