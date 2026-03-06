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
        self.packets.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
    pub fn template(&self) -> Option<&str> { self.get_packet("template") }
    pub fn datasets(&self) -> Option<&str> { self.get_packet("datasets") }
    pub fn config(&self) -> Option<&str> { self.get_packet("config") }
    pub fn locale_set(&self) -> Option<&str> { self.get_packet("localeSet") }
}

pub fn extract_xfa(pdf: &Pdf) -> Result<XfaPackets> {
    if let Some(p) = extract_from_acroform(pdf) {
        if !p.packets.is_empty() || p.full_xml.is_some() { return Ok(p); }
    }
    scan_for_xfa(pdf)
}

pub fn extract_xfa_from_bytes(data: impl Into<pdf_syntax::PdfData>) -> Result<XfaPackets> {
    let pdf = Pdf::new(data).map_err(|e| XfaError::LoadFailed(format!("{e:?}")))?;
    extract_xfa(&pdf)
}

fn extract_from_acroform(pdf: &Pdf) -> Option<XfaPackets> {
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
            _ => { i += 1; continue; }
        };
        if let Some(c) = match &items[i + 1] {
            Object::Stream(s) => decode_stream(s),
            Object::String(s) => Some(std::string::String::from_utf8_lossy(s.as_bytes()).to_string()),
            _ => None,
        } { packets.packets.push((name, c)); }
        i += 2;
    }
    packets
}

fn scan_for_xfa(pdf: &Pdf) -> Result<XfaPackets> {
    for obj in pdf.objects() {
        if let Object::Stream(s) = obj {
            if let Some(d) = decode_stream(&s) {
                if d.contains("<xdp:xdp") { return Ok(parse_xfa_xml(&d)); }
            }
        }
    }
    Err(XfaError::PacketNotFound("no XFA content found".to_string()))
}

fn decode_stream(stream: &Stream<'_>) -> Option<String> {
    std::string::String::from_utf8(stream.decoded().ok()?).ok()
}

fn parse_xfa_xml(xml: &str) -> XfaPackets {
    let mut packets = XfaPackets { full_xml: Some(xml.to_string()), packets: Vec::new() };
    let t = xml.trim();
    let c = t.find("?>").map(|p| &t[p + 2..]).unwrap_or(t).trim();
    let inner = match c.find('>') {
        Some(s) => {
            let rest = &c[s + 1..];
            rest.rfind("</xdp:xdp>").map(|e| &rest[..e]).or_else(|| rest.rfind("</xdp>").map(|e| &rest[..e])).unwrap_or(rest)
        }
        None => return packets,
    };
    let mut pos = 0;
    let bytes = inner.as_bytes();
    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() { pos += 1; }
        if pos >= bytes.len() { break; }
        if bytes[pos] != b'<' { pos += 1; continue; }
        if inner[pos..].starts_with("<!--") { if let Some(e) = inner[pos..].find("-->") { pos += e + 3; continue; } }
        if inner[pos..].starts_with("<?") { if let Some(e) = inner[pos..].find("?>") { pos += e + 2; continue; } }
        let ts = pos;
        pos += 1;
        let ns = pos;
        while pos < bytes.len() && bytes[pos] != b'>' && bytes[pos] != b' ' && bytes[pos] != b'/' { pos += 1; }
        let ft = &inner[ns..pos];
        let pn = ft.split(':').next_back().unwrap_or(ft);
        let ct = format!("</{ft}>");
        let at = format!("</xfa:{pn}>");
        if let Some(cp) = inner[ts..].find(ct.as_str()) {
            let ee = ts + cp + ct.len();
            packets.packets.push((pn.to_string(), inner[ts..ee].to_string()));
            pos = ee;
        } else if let Some(cp) = inner[ts..].find(at.as_str()) {
            let ee = ts + cp + at.len();
            packets.packets.push((pn.to_string(), inner[ts..ee].to_string()));
            pos = ee;
        } else {
            while pos < bytes.len() && bytes[pos] != b'>' { pos += 1; }
            pos += 1;
        }
    }
    packets
}

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
        if !dict.get::<Name>(TYPE).is_some_and(|n| n.as_ref() == b"FontDescriptor") { continue; }
        let name = dict.get::<Name>(FONT_NAME).map(|n| std::string::String::from_utf8_lossy(n.as_ref()).to_string()).unwrap_or_default();
        for key in [FONT_FILE2, FONT_FILE, FONT_FILE3] {
            if let Some(s) = dict.get::<Stream<'_>>(key) {
                if let Ok(d) = s.decoded() { if !d.is_empty() { fonts.push((name.clone(), d)); break; } }
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
}
