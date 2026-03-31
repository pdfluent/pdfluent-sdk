//! PDF/A XMP metadata repair and generation.
//!
//! Repairs or creates XMP metadata streams for PDF/A conformance.
//! Synchronizes /Info dictionary with XMP metadata.

use crate::error::{ManipError, Result};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use xmp_writer::XmpWriter;

/// PDF/A conformance level for XMP identification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfAConformance {
    A1b,
    A1a,
    A2b,
    A2a,
    A2u,
    A3b,
    A3a,
    A3u,
}

impl PdfAConformance {
    /// ISO 19005 part number.
    pub fn part(self) -> i32 {
        match self {
            Self::A1b | Self::A1a => 1,
            Self::A2b | Self::A2a | Self::A2u => 2,
            Self::A3b | Self::A3a | Self::A3u => 3,
        }
    }

    /// Conformance level letter.
    pub fn conformance(self) -> &'static str {
        match self {
            Self::A1b | Self::A2b | Self::A3b => "B",
            Self::A1a | Self::A2a | Self::A3a => "A",
            Self::A2u | Self::A3u => "U",
        }
    }
}

/// Metadata fields for XMP generation.
#[derive(Debug, Clone, Default)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub creator: Option<String>,
    pub description: Option<String>,
    pub producer: Option<String>,
    pub creator_tool: Option<String>,
    pub create_date: Option<String>,
    pub modify_date: Option<String>,
    /// Keywords from /Info /Keywords — written as pdf:Keywords in XMP (§6.7.3.5).
    pub keywords: Option<String>,
}

/// Report from XMP metadata repair.
#[derive(Debug, Clone)]
pub struct XmpRepairReport {
    /// Whether XMP was created (true) or updated (false).
    pub xmp_created: bool,
    /// Whether /Info dictionary was synchronized.
    pub info_synced: bool,
    /// Whether PDF/A identification was added.
    pub pdfa_id_set: bool,
}

/// Repair or create XMP metadata for PDF/A conformance.
///
/// - Creates or replaces the XMP metadata stream in the catalog
/// - Sets pdfaid:part and pdfaid:conformance
/// - Synchronizes /Info dictionary entries with XMP
pub fn repair_xmp_metadata(
    doc: &mut Document,
    conformance: PdfAConformance,
    metadata: Option<&PdfMetadata>,
) -> Result<XmpRepairReport> {
    let mut report = XmpRepairReport {
        xmp_created: false,
        info_synced: false,
        pdfa_id_set: false,
    };

    // Read existing /Info dictionary values.
    let existing_meta = read_info_dict(doc);
    let mut meta = merge_metadata(metadata, &existing_meta);

    // Filter out dates that can't be serialized to XMP — keeping them in /Info
    // while XMP lacks them causes §6.7.3.1/6.7.3.8 mismatches.
    if meta
        .create_date
        .as_ref()
        .and_then(|d| parse_xmp_date(d))
        .is_none()
    {
        meta.create_date = None;
    }
    if meta
        .modify_date
        .as_ref()
        .and_then(|d| parse_xmp_date(d))
        .is_none()
    {
        meta.modify_date = None;
    }

    // Generate XMP using xmp-writer.
    let xmp_bytes = generate_xmp(&meta, conformance);
    report.pdfa_id_set = true;

    // Get catalog reference.
    let catalog_id = get_catalog_id(doc)?;

    // Check if there's an existing /Metadata stream.
    let existing_metadata_id = {
        if let Some(Object::Dictionary(ref cat)) = doc.objects.get(&catalog_id) {
            match cat.get(b"Metadata").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            }
        } else {
            None
        }
    };

    // Create or update the metadata stream.
    // PDF/A §6.7.2: metadata stream must NOT be compressed.
    let mut metadata_stream = Stream::new(
        dictionary! {
            "Type" => "Metadata",
            "Subtype" => "XML",
            "Length" => Object::Integer(xmp_bytes.len() as i64),
        },
        xmp_bytes,
    );
    metadata_stream.allows_compression = false;

    if let Some(meta_id) = existing_metadata_id {
        doc.objects.insert(meta_id, Object::Stream(metadata_stream));
    } else {
        let meta_id = doc.add_object(Object::Stream(metadata_stream));
        if let Some(Object::Dictionary(ref mut cat)) = doc.objects.get_mut(&catalog_id) {
            cat.set("Metadata", Object::Reference(meta_id));
        }
        report.xmp_created = true;
    }

    // Synchronize /Info dictionary with XMP values.
    sync_info_dict(doc, &meta);
    report.info_synced = true;

    // Ensure metadata stream is not compressed and has no BOM (§6.7.2 + §6.7.3).
    sanitize_metadata_stream(doc);

    Ok(report)
}

/// Ensure the Catalog's /Metadata stream is not compressed and has no BOM.
///
/// PDF/A §6.7.2 requires metadata streams to be uncompressed.
/// BOM bytes at the start of XMP can confuse some validators.
fn sanitize_metadata_stream(doc: &mut Document) {
    let meta_id = {
        let catalog_id = match doc.trailer.get(b"Root").ok() {
            Some(Object::Reference(id)) => *id,
            _ => return,
        };
        let Some(Object::Dictionary(cat)) = doc.objects.get(&catalog_id) else {
            return;
        };
        match cat.get(b"Metadata").ok() {
            Some(Object::Reference(id)) => *id,
            _ => return,
        }
    };

    let Some(Object::Stream(stream)) = doc.objects.get_mut(&meta_id) else {
        return;
    };

    // Remove any compression filter (§6.7.2).
    if stream.dict.has(b"Filter") {
        let _ = stream.decompress();
        stream.dict.remove(b"Filter");
        stream.dict.remove(b"DecodeParms");
    }

    // Strip UTF-8 BOM (EF BB BF) from start of content.
    if stream.content.starts_with(&[0xEF, 0xBB, 0xBF]) {
        stream.content = stream.content[3..].to_vec();
    }

    // Update Length.
    stream
        .dict
        .set("Length", Object::Integer(stream.content.len() as i64));
    stream.allows_compression = false;
}

/// Generate XMP metadata bytes using xmp-writer.
fn generate_xmp(meta: &PdfMetadata, conformance: PdfAConformance) -> Vec<u8> {
    use xmp_writer::LangId;

    let mut writer = XmpWriter::new();

    // PDF/A identification.
    writer.pdfa_part(conformance.part());
    writer.pdfa_conformance(conformance.conformance());

    // Dublin Core — dc:title is required by 6.6.2.3.1:1.
    let title_str = meta.title.as_deref().unwrap_or("Untitled");
    writer.title([(None::<LangId>, title_str)]);
    // Only write dc:description if non-empty — an empty Subject from Info dict
    // maps to an empty dc:description, which veraPDF considers as "missing" and
    // emits 6.7.3.4. Skip empty values to keep the XMP consistent with Info.
    if let Some(ref description) = meta.description {
        if !description.trim().is_empty() {
            writer.description([(None::<LangId>, description.as_str())]);
        }
    }
    if let Some(ref creator) = meta.creator {
        writer.creator([creator.as_str()]);
    }

    // XMP Basic.
    if let Some(ref tool) = meta.creator_tool {
        writer.creator_tool(tool);
    }
    if let Some(ref date) = meta.create_date {
        if let Some(dt) = parse_xmp_date(date) {
            writer.create_date(dt);
        }
    }
    if let Some(ref date) = meta.modify_date {
        if let Some(dt) = parse_xmp_date(date) {
            writer.modify_date(dt);
        }
    }

    // PDF properties.
    if let Some(ref producer) = meta.producer {
        writer.producer(producer);
    }
    // Sync /Info Keywords → pdf:Keywords (§6.7.3.5).
    if let Some(ref kw) = meta.keywords {
        if !kw.trim().is_empty() {
            writer.pdf_keywords(kw);
        }
    }

    // PDF/A extension schema declarations (6.6.2.3.1).
    // Properties not in XMP 2004 core need extension schema descriptions.
    {
        let mut schemas = writer.extension_schemas();
        // pdfaid:part and pdfaid:conformance
        schemas.pdfaid(false);
        // pdf:Producer etc.
        schemas.pdf().properties().describe_all();
    }

    writer.finish(None).into_bytes()
}

/// Parse a date string to xmp_writer DateTime, preserving full time + timezone.
fn parse_xmp_date(date_str: &str) -> Option<xmp_writer::DateTime> {
    // Support ISO 8601: YYYY-MM-DDThh:mm:ss+hh:mm
    // Support PDF D: format: D:YYYYMMDDHHmmSS+HH'mm' or D:YYYYMMDDHHmmSS-HH'mm' or Z
    let s = date_str.strip_prefix("D:").unwrap_or(date_str);
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 4 {
        return None;
    }

    // Detect ISO vs PDF format by looking for '-' at position 4.
    let is_iso = chars.len() > 4 && chars[4] == '-';

    if is_iso {
        return parse_iso_date(&chars);
    }

    // PDF format: YYYYMMDDHHmmSS[+|-]HH'mm'  or  Z
    let year: u16 = chars[0..4].iter().collect::<String>().parse().ok()?;
    let month = parse_two_digits(&chars, 4);
    let day = parse_two_digits(&chars, 6);
    let hour = parse_two_digits(&chars, 8);
    let minute = parse_two_digits(&chars, 10);
    let second = parse_two_digits(&chars, 12);

    // Timezone starts at position 14: Z, +HH'mm', -HH'mm', +HHmm, -HHmm
    let timezone = parse_pdf_timezone(&chars, 14);

    Some(xmp_writer::DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        timezone,
    })
}

/// Parse ISO 8601 date: YYYY-MM-DDThh:mm:ss[+hh:mm|Z]
fn parse_iso_date(chars: &[char]) -> Option<xmp_writer::DateTime> {
    let year: u16 = chars[0..4].iter().collect::<String>().parse().ok()?;
    let month = if chars.len() >= 7 && chars[4] == '-' {
        chars[5..7].iter().collect::<String>().parse::<u8>().ok()
    } else {
        None
    };
    let day = if chars.len() >= 10 && chars[7] == '-' {
        chars[8..10].iter().collect::<String>().parse::<u8>().ok()
    } else {
        None
    };

    // Time part after 'T' at position 10
    let (hour, minute, second) = if chars.len() >= 13 && chars[10] == 'T' {
        let h = chars[11..13].iter().collect::<String>().parse::<u8>().ok();
        let m = if chars.len() >= 16 && chars[13] == ':' {
            chars[14..16].iter().collect::<String>().parse::<u8>().ok()
        } else {
            None
        };
        let s = if chars.len() >= 19 && chars.get(16) == Some(&':') {
            chars[17..19].iter().collect::<String>().parse::<u8>().ok()
        } else {
            None
        };
        (h, m, s)
    } else {
        (None, None, None)
    };

    // Timezone: look for Z, +, or - after the time part
    let tz_start = if second.is_some() {
        19
    } else if minute.is_some() {
        16
    } else if hour.is_some() {
        13
    } else {
        10
    };
    let timezone = parse_iso_timezone(chars, tz_start);

    Some(xmp_writer::DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        timezone,
    })
}

fn parse_two_digits(chars: &[char], offset: usize) -> Option<u8> {
    if chars.len() >= offset + 2 {
        chars[offset..offset + 2]
            .iter()
            .collect::<String>()
            .parse()
            .ok()
    } else {
        None
    }
}

/// Parse PDF timezone: Z, +HH'mm', -HH'mm', +HHmm, -HHmm
fn parse_pdf_timezone(chars: &[char], offset: usize) -> Option<xmp_writer::Timezone> {
    let ch = *chars.get(offset)?;
    if ch == 'Z' {
        return Some(xmp_writer::Timezone::Utc);
    }
    if ch != '+' && ch != '-' {
        return None;
    }
    let tz_hour = parse_two_digits(chars, offset + 1)? as i8;
    // Minutes: skip optional apostrophe
    let min_offset = if chars.get(offset + 3) == Some(&'\'') {
        offset + 4
    } else {
        offset + 3
    };
    let tz_min = parse_two_digits(chars, min_offset).unwrap_or(0) as i8;
    let h = if ch == '-' { -(tz_hour as i8) } else { tz_hour as i8 };
    Some(xmp_writer::Timezone::Local { hour: h, minute: tz_min })
}

/// Parse ISO timezone: Z, +hh:mm, -hh:mm
fn parse_iso_timezone(chars: &[char], offset: usize) -> Option<xmp_writer::Timezone> {
    let ch = *chars.get(offset)?;
    if ch == 'Z' {
        return Some(xmp_writer::Timezone::Utc);
    }
    if ch != '+' && ch != '-' {
        return None;
    }
    let tz_hour = parse_two_digits(chars, offset + 1)? as i8;
    let tz_min = if chars.get(offset + 3) == Some(&':') {
        parse_two_digits(chars, offset + 4).unwrap_or(0) as i8
    } else {
        parse_two_digits(chars, offset + 3).unwrap_or(0) as i8
    };
    let h = if ch == '-' { -(tz_hour as i8) } else { tz_hour as i8 };
    Some(xmp_writer::Timezone::Local {
        hour: h,
        minute: tz_min,
    })
}

/// Read metadata from /Info dictionary.
fn read_info_dict(doc: &Document) -> PdfMetadata {
    let mut meta = PdfMetadata::default();

    let info_id = match doc.trailer.get(b"Info").ok() {
        Some(Object::Reference(id)) => *id,
        _ => return meta,
    };

    let Some(Object::Dictionary(info)) = doc.objects.get(&info_id) else {
        return meta;
    };

    meta.title = get_string_value(info, b"Title");
    meta.creator = get_string_value(info, b"Author");
    meta.producer = get_string_value(info, b"Producer");
    // Trim whitespace: a space-only Creator like "( )" must not trigger §6.7.3.6.
    // (#FIX-6.7.3.6-whitespace-creator)
    meta.creator_tool = get_string_value(info, b"Creator")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    meta.description = get_string_value(info, b"Subject");
    meta.create_date = get_string_value(info, b"CreationDate");
    meta.modify_date = get_string_value(info, b"ModDate");
    // Sync /Keywords → pdf:Keywords (§6.7.3.5).
    meta.keywords = get_string_value(info, b"Keywords");

    meta
}

/// Encode a Rust string as a PDF string object.
///
/// Uses UTF-16BE with BOM for any string containing non-ASCII characters.
/// Pure-ASCII strings are stored as literal bytes (PDFDocEncoding-compatible).
/// This ensures that roundtripping through the Info dict preserves characters
/// like ® (U+00AE) which are not valid UTF-8 single bytes but are valid
/// PDFDocEncoding bytes. (#FIX-6.7.3-special-chars)
fn to_pdf_string(s: &str) -> Object {
    if s.is_ascii() {
        Object::String(s.as_bytes().to_vec(), lopdf::StringFormat::Literal)
    } else {
        // UTF-16BE with BOM: FE FF followed by big-endian UTF-16 code units.
        let mut bytes = vec![0xFE_u8, 0xFF];
        for unit in s.encode_utf16() {
            bytes.push((unit >> 8) as u8);
            bytes.push((unit & 0xFF) as u8);
        }
        Object::String(bytes, lopdf::StringFormat::Literal)
    }
}

fn get_string_value(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    let raw = match dict.get(key).ok()? {
        Object::String(bytes, _) => {
            // Handle UTF-16BE BOM.
            if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
                let utf16: Vec<u16> = bytes[2..]
                    .chunks(2)
                    .filter_map(|c| {
                        if c.len() == 2 {
                            Some(u16::from_be_bytes([c[0], c[1]]))
                        } else {
                            None
                        }
                    })
                    .collect();
                String::from_utf16(&utf16).ok()
            } else if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF
            {
                // UTF-8 BOM — decode the rest as UTF-8.
                match std::str::from_utf8(&bytes[3..]) {
                    Ok(s) => Some(s.to_string()),
                    Err(_) => Some(bytes[3..].iter().map(|&b| b as char).collect()),
                }
            } else {
                // Try UTF-8 first. For PDFDocEncoding/Latin-1 strings (e.g. 0xAE = ®),
                // from_utf8_lossy would replace bytes ≥0x80 that aren't valid UTF-8
                // sequences with U+FFFD, causing XMP/Info mismatches for strings with
                // special characters (§6.7.3). Fall back to ISO-8859-1 interpretation
                // (each byte maps to the same Unicode code point) to preserve the
                // original character. (#FIX-6.7.3-special-chars)
                match std::str::from_utf8(bytes) {
                    Ok(s) => Some(s.to_string()),
                    Err(_) => Some(bytes.iter().map(|&b| b as char).collect()),
                }
            }
        }
        _ => None,
    };
    // Strip BOM characters (U+FEFF) that survived decoding and treat
    // whitespace-only / empty strings as absent. This prevents BOM-only or
    // space-only /Info entries from producing XMP mismatches (§6.7.3).
    raw.map(|s| s.replace('\u{FEFF}', ""))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Merge user-provided metadata with existing /Info values.
fn merge_metadata(user: Option<&PdfMetadata>, existing: &PdfMetadata) -> PdfMetadata {
    if let Some(user) = user {
        PdfMetadata {
            title: user.title.clone().or_else(|| existing.title.clone()),
            creator: user.creator.clone().or_else(|| existing.creator.clone()),
            description: user
                .description
                .clone()
                .or_else(|| existing.description.clone()),
            producer: user.producer.clone().or_else(|| existing.producer.clone()),
            creator_tool: user
                .creator_tool
                .clone()
                .or_else(|| existing.creator_tool.clone()),
            create_date: user
                .create_date
                .clone()
                .or_else(|| existing.create_date.clone()),
            modify_date: user
                .modify_date
                .clone()
                .or_else(|| existing.modify_date.clone()),
            keywords: user.keywords.clone().or_else(|| existing.keywords.clone()),
        }
    } else {
        existing.clone()
    }
}

/// Synchronize /Info dictionary to match XMP values.
fn sync_info_dict(doc: &mut Document, meta: &PdfMetadata) {
    let info_id = match doc.trailer.get(b"Info").ok() {
        Some(Object::Reference(id)) => *id,
        _ => {
            // Create new /Info dictionary.
            let info = build_info_dict(meta);
            let id = doc.add_object(Object::Dictionary(info));
            doc.trailer.set("Info", Object::Reference(id));
            return;
        }
    };

    if let Some(Object::Dictionary(ref mut info)) = doc.objects.get_mut(&info_id) {
        // Sync each /Info key: set if present in metadata, remove if absent.
        // Removing BOM-only / whitespace-only entries prevents §6.7.3 mismatches.
        match &meta.title {
            Some(title) => info.set("Title", to_pdf_string(title)),
            None => { info.remove(b"Title"); }
        }
        match &meta.creator {
            Some(author) => info.set("Author", to_pdf_string(author)),
            None => { info.remove(b"Author"); }
        }
        match &meta.producer {
            Some(producer) => info.set("Producer", to_pdf_string(producer)),
            None => { info.remove(b"Producer"); }
        }
        match &meta.description {
            Some(subject) => info.set("Subject", to_pdf_string(subject)),
            None => { info.remove(b"Subject"); }
        }
        match &meta.keywords {
            Some(kw) => info.set("Keywords", to_pdf_string(kw)),
            None => { info.remove(b"Keywords"); }
        }
        // Sync /Creator with (trimmed) creator_tool. If None (e.g. was whitespace-only),
        // remove it so /Info and XMP agree and §6.7.3.6 does not fire.
        // (#FIX-6.7.3.6-whitespace-creator)
        match &meta.creator_tool {
            Some(tool) => info.set("Creator", to_pdf_string(tool)),
            None => {
                info.remove(b"Creator");
            }
        }
        // Sync dates: §6.7.3.1 (CreationDate) and §6.7.3.8 (ModDate).
        // If the date was filtered out (unparseable for XMP), remove from /Info too.
        match &meta.create_date {
            Some(date) => info.set("CreationDate", to_pdf_string(date)),
            None => {
                info.remove(b"CreationDate");
            }
        }
        match &meta.modify_date {
            Some(date) => info.set("ModDate", to_pdf_string(date)),
            None => {
                info.remove(b"ModDate");
            }
        }
    }
}

fn build_info_dict(meta: &PdfMetadata) -> lopdf::Dictionary {
    let mut dict = lopdf::Dictionary::new();
    if let Some(ref title) = meta.title {
        dict.set("Title", to_pdf_string(title));
    }
    if let Some(ref creator) = meta.creator {
        dict.set("Author", to_pdf_string(creator));
    }
    if let Some(ref producer) = meta.producer {
        dict.set("Producer", to_pdf_string(producer));
    }
    if let Some(ref description) = meta.description {
        dict.set("Subject", to_pdf_string(description));
    }
    if let Some(ref tool) = meta.creator_tool {
        dict.set("Creator", to_pdf_string(tool));
    }
    if let Some(ref kw) = meta.keywords {
        dict.set("Keywords", to_pdf_string(kw));
    }
    if let Some(ref date) = meta.create_date {
        dict.set("CreationDate", to_pdf_string(date));
    }
    if let Some(ref date) = meta.modify_date {
        dict.set("ModDate", to_pdf_string(date));
    }
    dict
}

fn get_catalog_id(doc: &Document) -> Result<ObjectId> {
    match doc.trailer.get(b"Root").ok() {
        Some(Object::Reference(id)) => Ok(*id),
        _ => Err(ManipError::Other("no catalog found in document".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_doc() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
        };
        let page_id = doc.add_object(Object::Dictionary(page));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn test_repair_creates_xmp() {
        let mut doc = make_test_doc();
        let report = repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None).unwrap();
        assert!(report.xmp_created);
        assert!(report.pdfa_id_set);
        assert!(report.info_synced);

        // Verify metadata stream exists in catalog.
        let catalog_id = get_catalog_id(&doc).unwrap();
        if let Some(Object::Dictionary(cat)) = doc.objects.get(&catalog_id) {
            assert!(cat.has(b"Metadata"), "catalog should have /Metadata");
        }
    }

    #[test]
    fn test_repair_with_metadata() {
        let mut doc = make_test_doc();
        let meta = PdfMetadata {
            title: Some("Test PDF".into()),
            creator: Some("Test Author".into()),
            producer: Some("XFA Engine".into()),
            ..Default::default()
        };
        let report = repair_xmp_metadata(&mut doc, PdfAConformance::A1b, Some(&meta)).unwrap();
        assert!(report.xmp_created);

        // Check the XMP stream contains our metadata.
        let catalog_id = get_catalog_id(&doc).unwrap();
        if let Some(Object::Dictionary(cat)) = doc.objects.get(&catalog_id) {
            if let Ok(Object::Reference(meta_id)) = cat.get(b"Metadata") {
                if let Some(Object::Stream(stream)) = doc.objects.get(meta_id) {
                    let xmp = String::from_utf8_lossy(&stream.content);
                    assert!(xmp.contains("Test PDF"), "XMP should contain title");
                    assert!(xmp.contains("pdfaid"), "XMP should contain pdfaid");
                }
            }
        }
    }

    #[test]
    fn test_repair_updates_existing_xmp() {
        let mut doc = make_test_doc();

        // First repair — creates XMP.
        repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None).unwrap();

        // Second repair — updates existing XMP.
        let meta = PdfMetadata {
            title: Some("Updated Title".into()),
            ..Default::default()
        };
        let report = repair_xmp_metadata(&mut doc, PdfAConformance::A3b, Some(&meta)).unwrap();
        assert!(!report.xmp_created); // Updated, not created.
    }

    #[test]
    fn test_info_dict_sync() {
        let mut doc = make_test_doc();

        // Add /Info with title.
        let info = dictionary! {
            "Title" => Object::String("Original Title".into(), lopdf::StringFormat::Literal),
            "Author" => Object::String("Original Author".into(), lopdf::StringFormat::Literal),
        };
        let info_id = doc.add_object(Object::Dictionary(info));
        doc.trailer.set("Info", Object::Reference(info_id));

        // Repair — should read existing info.
        let report = repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None).unwrap();
        assert!(report.info_synced);
    }

    #[test]
    fn test_conformance_levels() {
        assert_eq!(PdfAConformance::A1b.part(), 1);
        assert_eq!(PdfAConformance::A1b.conformance(), "B");
        assert_eq!(PdfAConformance::A2a.part(), 2);
        assert_eq!(PdfAConformance::A2a.conformance(), "A");
        assert_eq!(PdfAConformance::A3u.part(), 3);
        assert_eq!(PdfAConformance::A3u.conformance(), "U");
    }

    #[test]
    fn test_parse_xmp_date() {
        let dt = parse_xmp_date("2024-01-15").unwrap();
        assert_eq!(dt.year, 2024);

        let dt = parse_xmp_date("D:20240115").unwrap();
        assert_eq!(dt.year, 2024);
        assert_eq!(dt.month, Some(1));
        assert_eq!(dt.day, Some(15));
    }
}
