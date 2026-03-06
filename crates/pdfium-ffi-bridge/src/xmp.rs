//! XMP metadata generation for PDF/A-2b compliance.
//!
//! PDF/A requires an XMP metadata stream in the document catalog declaring
//! the conformance level. This module generates the required metadata packet
//! and injects it into the PDF.
//!
//! Reference: PDF/A-2b (ISO 19005-2), §6.6.2 XMP Metadata.

use crate::error::{PdfError, Result};
use lopdf::{Dictionary, Object, Stream};

/// Generate an XMP metadata packet declaring PDF/A-2b conformance.
///
/// The packet includes:
/// - `pdfaid:part = 2` (PDF/A-2)
/// - `pdfaid:conformance = B` (level B)
/// - `dc:format = application/pdf`
/// - Creation/modification dates
fn generate_pdfa2b_xmp(title: &str) -> Vec<u8> {
    let now = chrono_lite_now();
    format!(
        r#"<?xpacket begin="{bom}" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
        xmlns:dc="http://purl.org/dc/elements/1.1/"
        xmlns:xmp="http://ns.adobe.com/xap/1.0/"
        xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/"
        xmlns:pdf="http://ns.adobe.com/pdf/1.3/">
      <pdfaid:part>2</pdfaid:part>
      <pdfaid:conformance>B</pdfaid:conformance>
      <dc:format>application/pdf</dc:format>
      <dc:title>
        <rdf:Alt>
          <rdf:li xml:lang="x-default">{title}</rdf:li>
        </rdf:Alt>
      </dc:title>
      <xmp:CreateDate>{now}</xmp:CreateDate>
      <xmp:ModifyDate>{now}</xmp:ModifyDate>
      <xmp:MetadataDate>{now}</xmp:MetadataDate>
      <pdf:Producer>XFA-Native-Rust</pdf:Producer>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        bom = "\u{FEFF}",
        title = xml_escape(title),
        now = now,
    )
    .into_bytes()
}

/// Inject PDF/A-2b XMP metadata into the document catalog.
///
/// Creates an XMP metadata stream and sets it as the `Metadata` entry
/// on the document catalog. If a `Metadata` entry already exists, it is
/// replaced.
pub fn inject_pdfa2b_metadata(doc: &mut lopdf::Document, title: &str) -> Result<()> {
    let xmp_bytes = generate_pdfa2b_xmp(title);

    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"Metadata".to_vec()));
    dict.set("Subtype", Object::Name(b"XML".to_vec()));
    // XMP metadata streams must NOT be compressed (PDF/A requirement).
    let stream = Stream::new(dict, xmp_bytes);
    let metadata_id = doc.add_object(Object::Stream(stream));

    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return Err(PdfError::LoadFailed("no Root in trailer".to_string())),
    };

    if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
        catalog.set("Metadata", Object::Reference(metadata_id));
    }

    Ok(())
}

/// Minimal ISO 8601 timestamp without pulling in chrono.
fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert epoch seconds to a rough UTC date-time.
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Gregorian calendar from days since epoch (1970-01-01).
    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since 1970-01-01 to (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Shift to March-based year for easier leap-year handling.
    days += 719468; // days from 0000-03-01 to 1970-01-01
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
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
    use lopdf::dictionary;

    #[test]
    fn xmp_packet_contains_pdfa2b_declaration() {
        let xmp = generate_pdfa2b_xmp("Test Document");
        let text = String::from_utf8(xmp).unwrap();
        assert!(text.contains("<pdfaid:part>2</pdfaid:part>"));
        assert!(text.contains("<pdfaid:conformance>B</pdfaid:conformance>"));
        assert!(text.contains("application/pdf"));
        assert!(text.contains("Test Document"));
        assert!(text.contains("xpacket"));
    }

    #[test]
    fn xmp_title_is_escaped() {
        let xmp = generate_pdfa2b_xmp("A & B <test>");
        let text = String::from_utf8(xmp).unwrap();
        assert!(text.contains("A &amp; B &lt;test&gt;"));
    }

    #[test]
    fn inject_metadata_into_document() {
        let mut doc = lopdf::Document::new();
        let pages = lopdf::dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        let catalog = lopdf::dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        inject_pdfa2b_metadata(&mut doc, "Test").unwrap();

        let cat = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        let meta_ref = cat.get(b"Metadata").unwrap();
        assert!(matches!(meta_ref, Object::Reference(_)));

        // Verify stream content
        if let Object::Reference(id) = meta_ref {
            if let Ok(Object::Stream(s)) = doc.get_object(*id) {
                let text = String::from_utf8_lossy(&s.content);
                assert!(text.contains("pdfaid:part"));
            } else {
                panic!("Metadata should be a stream");
            }
        }
    }

    #[test]
    fn chrono_lite_produces_valid_timestamp() {
        let ts = chrono_lite_now();
        // Should be ISO 8601: YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn days_to_ymd_epoch() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        // 2024-01-01 is day 19723
        let (y, m, d) = days_to_ymd(19723);
        assert_eq!((y, m, d), (2024, 1, 1));
    }
}
