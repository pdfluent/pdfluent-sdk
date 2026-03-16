//! Deep XMP metadata validation for PDF/A compliance.
//!
//! Implements checks from ISO 19005 sections 6.6 and 6.7:
//! - Extension schema parsing and validation (6.6.2.3.1, 6.6.2.3.3)
//! - Property namespace validation (6.7.9)
//! - Info/XMP consistency (6.7.3, 6.7.3.3, 6.7.3.4)
//! - Additional XMP property rules (6.7.4, 6.7.5, 6.7.8, 6.7.11)
//! - XMP stream and packet validation (6.6.2, 6.6.2.1)

use std::collections::HashSet;

use crate::check::{self, error, warning};
use crate::{ComplianceReport, PdfALevel};
use pdf_syntax::Pdf;

/// Well-known XMP value types (XMP Specification Part 1, Table 8).
const VALID_XMP_VALUE_TYPES: &[&str] = &[
    "Boolean",
    "Date",
    "Integer",
    "Real",
    "Text",
    "ProperName",
    "URI",
    "URL",
    "MIMEType",
    "AgentName",
    "RenditionClass",
    "ResourceEvent",
    "ResourceRef",
    "Version",
    "Rational",
    "XPath",
    "Locale",
    "GUID",
    "GPSCoordinate",
    "Dimensions",
    "Font",
    "Colorant",
    "Thumbnail",
    "Flash",
    "CFAPattern",
    "DeviceSettings",
    "OECF/SFR",
    // Container types
    "bag Text",
    "bag ProperName",
    "seq Text",
    "seq ResourceEvent",
    "seq ResourceRef",
    "alt Text",
    "Bag Text",
    "Bag ProperName",
    "Seq Text",
    "Seq ResourceEvent",
    "Seq ResourceRef",
    "Alt Text",
    // Generic containers
    "Lang Alt",
    "Ordered array of Text",
    "Unordered array of Text",
];

/// Predefined XMP namespace prefixes known in PDF/A.
const PREDEFINED_PREFIXES: &[&str] = &[
    "dc:",
    "xmp:",
    "xmpMM:",
    "xmpRights:",
    "xmpTPg:",
    "xmpDM:",
    "xmpidq:",
    "pdf:",
    "pdfaid:",
    "pdfuaid:",
    "pdfx:",
    "pdfxid:",
    "pdfa:",
    "pdfaExtension:",
    "pdfaSchema:",
    "pdfaProperty:",
    "pdfaType:",
    "pdfaField:",
    "photoshop:",
    "tiff:",
    "exif:",
    "exifEX:",
    "stRef:",
    "stEvt:",
    "stFnt:",
    "stDim:",
    "stArea:",
    "stVer:",
    "stJob:",
    "stMfs:",
    "xmpG:",
    "xmpBJ:",
    "xmpNote:",
    "rdf:",
    "xml:",
    "xmlns:", // reserved XML namespace-declaration attribute prefix (always valid)
    "x:",
    "Iptc4xmpCore:",
    "Iptc4xmpExt:",
    "plus:",
    "crs:",
    "lr:",
    "aux:",
];

/// An extension schema declared in pdfaExtension:schemas.
struct ExtensionSchema {
    namespace_uri: String,
    prefix: String,
    properties: Vec<ExtensionProperty>,
}

/// A property declared in an extension schema.
struct ExtensionProperty {
    name: String,
    value_type: String,
    category: String,
}

/// Run all deep XMP validation checks.
pub fn validate_xmp(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    let Some(xmp_data) = check::get_xmp_metadata(pdf) else {
        return; // Missing XMP is caught by check_xmp_metadata
    };
    let Ok(xmp_text) = std::str::from_utf8(&xmp_data) else {
        // §6.7.3 — XMP metadata stream must be UTF-8 encoded
        error(report, "6.7.3", "XMP metadata stream is not valid UTF-8");
        return;
    };

    // §6.7.3 — XMP stream must contain valid RDF structure
    check_xmp_rdf_structure(xmp_text, report);

    check_xmp_packet_header(xmp_text, report);
    let schemas = parse_extension_schemas(xmp_text);
    check_extension_schema_structure(xmp_text, &schemas, report);
    check_property_namespaces(xmp_text, &schemas, level, report);
    check_info_xmp_deep(pdf, xmp_text, report);
    check_date_formats(xmp_text, report);
    check_pdfa_id_properties(xmp_text, level, report);
    check_pdfa_version_match(xmp_text, level, report);
    check_dc_title_consistency(pdf, xmp_text, report);
    check_deprecated_types(xmp_text, report);
}

/// §6.6.2.1 — XMP must have a correct packet header.
fn check_xmp_packet_header(xmp: &str, report: &mut ComplianceReport) {
    // XMP packet must start with <?xpacket begin="..." id="W5M0MpCehiHzreSzNTczkc9d"?>
    if !xmp.contains("<?xpacket") {
        error(
            report,
            "6.6.2.1",
            "XMP stream missing required <?xpacket> processing instruction",
        );
        return;
    }

    // The packet header should contain the byte-order mark (BOM) or empty begin=""
    if let Some(begin_pos) = xmp.find("<?xpacket") {
        let header_end = xmp[begin_pos..].find("?>").unwrap_or(0);
        let header = &xmp[begin_pos..begin_pos + header_end + 2];

        if !header.contains("begin=") {
            error(
                report,
                "6.6.2.1",
                "XMP packet header missing 'begin' attribute",
            );
        }
        if !header.contains("id=") {
            warning(
                report,
                "6.6.2.1",
                "XMP packet header missing 'id' attribute",
            );
        }
    }
}

/// Extract top-level `<rdf:li>...</rdf:li>` blocks from XML content,
/// properly handling nested `rdf:li` elements by tracking depth.
fn extract_top_level_li_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let open_tag = "<rdf:li";
    let close_tag = "</rdf:li>";
    let mut search_from = 0;

    while let Some(rel_start) = content[search_from..].find(open_tag) {
        let abs_start = search_from + rel_start;
        let mut depth = 0;
        let mut pos = abs_start;

        let end_pos = loop {
            if pos >= content.len() {
                break None;
            }
            if content[pos..].starts_with(open_tag) {
                depth += 1;
                pos += open_tag.len();
            } else if content[pos..].starts_with(close_tag) {
                depth -= 1;
                if depth == 0 {
                    break Some(pos + close_tag.len());
                }
                pos += close_tag.len();
            } else {
                // Advance by one Unicode scalar to avoid splitting multi-byte UTF-8
                // sequences — `pos += 1` would leave `pos` inside a multi-byte char,
                // causing `content[pos..]` to panic on the next iteration. (#448)
                pos += content[pos..].chars().next().map_or(1, |c| c.len_utf8());
            }
        };

        if let Some(end) = end_pos {
            blocks.push(content[abs_start..end].to_string());
            search_from = end;
        } else {
            break;
        }
    }

    blocks
}

/// Parse pdfaExtension:schemas bag from XMP text.
///
/// Extracts schema URI, prefix, and declared properties for each
/// extension schema to validate §6.6.2.3.1 and §6.7.9.
fn parse_extension_schemas(xmp: &str) -> Vec<ExtensionSchema> {
    let mut schemas = Vec::new();

    // Find pdfaExtension:schemas bag
    let Some(bag_start) = xmp.find("pdfaExtension:schemas") else {
        return schemas;
    };

    // Find the containing rdf:Bag
    let search_region = &xmp[bag_start..];
    let Some(bag_open) = search_region.find("<rdf:Bag") else {
        return schemas;
    };
    let bag_end_tag = "</rdf:Bag>";
    let Some(bag_close) = search_region[bag_open..].find(bag_end_tag) else {
        return schemas;
    };
    let bag_content = &search_region[bag_open..bag_open + bag_close + bag_end_tag.len()];

    // Extract top-level <rdf:li>...</rdf:li> blocks (nesting-aware)
    for li_block in extract_top_level_li_blocks(bag_content) {
        if !li_block.contains("pdfaSchema:") {
            continue;
        }

        let namespace_uri =
            extract_nested_value(&li_block, "pdfaSchema:namespaceURI").unwrap_or_default();
        let prefix = extract_nested_value(&li_block, "pdfaSchema:prefix").unwrap_or_default();

        let properties = parse_extension_properties(&li_block);

        if !namespace_uri.is_empty() || !prefix.is_empty() {
            schemas.push(ExtensionSchema {
                namespace_uri,
                prefix,
                properties,
            });
        }
    }

    schemas
}

/// Parse pdfaProperty entries from a schema's property bag.
fn parse_extension_properties(schema_block: &str) -> Vec<ExtensionProperty> {
    let mut properties = Vec::new();

    // Find pdfaSchema:property sequence
    let Some(prop_start) = schema_block.find("pdfaSchema:property") else {
        return properties;
    };
    let prop_region = &schema_block[prop_start..];

    // Split on <rdf:li to find individual properties
    for li_block in prop_region.split("<rdf:li") {
        if li_block.trim().is_empty() || !li_block.contains("pdfaProperty:") {
            continue;
        }

        let name = extract_nested_value(li_block, "pdfaProperty:name").unwrap_or_default();
        let value_type =
            extract_nested_value(li_block, "pdfaProperty:valueType").unwrap_or_default();
        let category = extract_nested_value(li_block, "pdfaProperty:category").unwrap_or_default();

        if !name.is_empty() {
            properties.push(ExtensionProperty {
                name,
                value_type,
                category,
            });
        }
    }

    properties
}

/// Extract a value from nested XMP elements, handling both element and attribute forms.
fn extract_nested_value(block: &str, key: &str) -> Option<String> {
    // Element form: <key>value</key>
    let open_tag = format!("<{key}>");
    let close_tag = format!("</{key}>");
    if let Some(start) = block.find(&open_tag) {
        let val_start = start + open_tag.len();
        if let Some(end) = block[val_start..].find(&close_tag) {
            let value = block[val_start..val_start + end].trim();
            if !value.is_empty() && !value.starts_with('<') {
                return Some(value.to_string());
            }
        }
    }

    // Attribute form: key="value"
    let attr_pattern = format!("{key}=\"");
    if let Some(start) = block.find(&attr_pattern) {
        let val_start = start + attr_pattern.len();
        if let Some(end) = block[val_start..].find('"') {
            return Some(block[val_start..val_start + end].trim().to_string());
        }
    }

    None
}

/// §6.6.2.3.1 / §6.6.2.3.3 — Validate extension schema structure.
///
/// Each extension schema must have namespaceURI, prefix, schema name.
/// Each property must have name, valueType, category, description.
/// ValueType must be a valid XMP type or custom-declared type.
fn check_extension_schema_structure(
    xmp: &str,
    schemas: &[ExtensionSchema],
    report: &mut ComplianceReport,
) {
    if !xmp.contains("pdfaExtension:schemas") {
        return; // No extension schemas declared — fine, will be caught by namespace check
    }

    // Collect custom valueType names declared in pdfaType:type
    let custom_types: HashSet<String> = collect_custom_types(xmp);

    for schema in schemas {
        // §6.6.2.3.1: each schema must have required fields
        if schema.namespace_uri.is_empty() {
            error(
                report,
                "6.6.2.3.1",
                format!(
                    "Extension schema with prefix '{}' missing required pdfaSchema:namespaceURI",
                    schema.prefix
                ),
            );
        }
        if schema.prefix.is_empty() {
            error(
                report,
                "6.6.2.3.1",
                format!(
                    "Extension schema for '{}' missing required pdfaSchema:prefix",
                    schema.namespace_uri
                ),
            );
        }

        // §6.6.2.3.3: validate property valueTypes
        for prop in &schema.properties {
            if prop.value_type.is_empty() {
                error(
                    report,
                    "6.6.2.3.3",
                    format!(
                        "Extension property '{}:{}' missing required pdfaProperty:valueType",
                        schema.prefix, prop.name
                    ),
                );
            } else if !is_valid_value_type(&prop.value_type, &custom_types) {
                error(
                    report,
                    "6.6.2.3.3",
                    format!(
                        "Extension property '{}:{}' has invalid valueType '{}'",
                        schema.prefix, prop.name, prop.value_type
                    ),
                );
            }

            if prop.category.is_empty() {
                error(
                    report,
                    "6.6.2.3.3",
                    format!(
                        "Extension property '{}:{}' missing required pdfaProperty:category",
                        schema.prefix, prop.name
                    ),
                );
            } else if prop.category != "internal" && prop.category != "external" {
                error(
                    report,
                    "6.6.2.3.3",
                    format!(
                        "Extension property '{}:{}' has invalid category '{}' (must be 'internal' or 'external')",
                        schema.prefix, prop.name, prop.category
                    ),
                );
            }
        }
    }
}

/// Collect custom valueType names declared via pdfaType:type.
fn collect_custom_types(xmp: &str) -> HashSet<String> {
    let mut types = HashSet::new();
    let mut search_from = 0;
    while let Some(pos) = xmp[search_from..].find("pdfaType:type") {
        let abs_pos = search_from + pos;
        if let Some(val) = extract_nested_value(&xmp[abs_pos..], "pdfaType:type") {
            types.insert(val);
        }
        search_from = abs_pos + 1;
    }
    types
}

/// Check if a valueType is valid (predefined or custom-declared).
fn is_valid_value_type(vtype: &str, custom_types: &HashSet<String>) -> bool {
    // Direct match
    if VALID_XMP_VALUE_TYPES
        .iter()
        .any(|t| t.eq_ignore_ascii_case(vtype))
    {
        return true;
    }
    // Container pattern: "Bag <Type>", "Seq <Type>", "Alt <Type>"
    let stripped = vtype
        .strip_prefix("bag ")
        .or_else(|| vtype.strip_prefix("Bag "))
        .or_else(|| vtype.strip_prefix("seq "))
        .or_else(|| vtype.strip_prefix("Seq "))
        .or_else(|| vtype.strip_prefix("alt "))
        .or_else(|| vtype.strip_prefix("Alt "));
    if let Some(inner) = stripped {
        if VALID_XMP_VALUE_TYPES
            .iter()
            .any(|t| t.eq_ignore_ascii_case(inner))
        {
            return true;
        }
        if custom_types.contains(inner) {
            return true;
        }
    }
    // Custom declared type
    custom_types.contains(vtype)
}

/// §6.7.9 / §6.6.2.3.1 / §6.5.2 — Validate all XMP properties use known or declared namespaces.
///
/// Replaces the simpler check in check.rs with one that actually
/// parses extension schemas and validates specific properties.
///
/// Also checks that each used namespace prefix has a corresponding `xmlns:prefix`
/// declaration in the XMP document (per XML namespace spec, required by PDF/A).
fn check_property_namespaces(
    xmp: &str,
    schemas: &[ExtensionSchema],
    level: PdfALevel,
    report: &mut ComplianceReport,
) {
    let rule = match level.part() {
        1 => "6.7.9",
        4 => "6.5.2",
        _ => "6.6.2.3.1",
    };

    // Build set of valid prefixes: predefined + declared extensions
    let valid_prefixes: HashSet<&str> = PREDEFINED_PREFIXES.iter().copied().collect();
    let extension_prefixes: HashSet<String> = schemas
        .iter()
        .filter(|s| !s.prefix.is_empty())
        .map(|s| format!("{}:", s.prefix))
        .collect();

    // Collect all xmlns:prefix declarations present in the XMP document.
    // Per the XML namespace spec, every used prefix must be declared with xmlns:prefix=.
    // The only XML-predefined prefix is "xml:" (no declaration needed).
    let declared_prefixes: HashSet<String> = {
        let mut decls = HashSet::new();
        let mut search = 0;
        while let Some(pos) = xmp[search..].find("xmlns:") {
            let abs = search + pos + 6; // skip "xmlns:"
            if let Some(eq) = xmp[abs..].find('=') {
                if eq < 40 {
                    let prefix_name = &xmp[abs..abs + eq];
                    if prefix_name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    {
                        decls.insert(format!("{prefix_name}:"));
                    }
                }
            }
            search = abs;
        }
        decls
    };

    // Scan for namespace-prefixed properties
    let bytes = xmp.as_bytes();
    let mut pos = 0;
    let mut reported: HashSet<String> = HashSet::new();

    while pos < bytes.len() {
        if bytes[pos] == b'<' || bytes[pos] == b' ' {
            let start = pos + 1;
            if start < bytes.len() && bytes[start].is_ascii_alphabetic() {
                if let Some(colon_offset) = xmp[start..].find(':') {
                    if colon_offset < 30 {
                        let prefix_end = start + colon_offset + 1;
                        let prefix = &xmp[start..prefix_end];

                        // Skip closing tags and XML processing instructions
                        if prefix.starts_with('/')
                            || prefix.starts_with('?')
                            || prefix.starts_with('!')
                        {
                            pos = prefix_end;
                            continue;
                        }

                        // Skip xmlns: attribute declarations themselves
                        if prefix == "xmlns:" {
                            pos = prefix_end;
                            continue;
                        }

                        if prefix
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == ':')
                            && !reported.contains(prefix)
                        {
                            // Find the full property name
                            let prop_end = xmp[prefix_end..]
                                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                                .map(|i| prefix_end + i)
                                .unwrap_or(prefix_end);
                            let full_prop = &xmp[start..prop_end];

                            if !full_prop.is_empty() && full_prop.contains(':') {
                                // Check 1: prefix not in predefined/extension sets
                                let unknown_prefix = !valid_prefixes.contains(prefix)
                                    && !extension_prefixes.contains(prefix);
                                // Check 2: prefix is predefined but xmlns:prefix not declared
                                // (xml: is the only XML-spec pre-declared prefix)
                                let undeclared = prefix != "xml:"
                                    && !declared_prefixes.contains(prefix);

                                if unknown_prefix || undeclared {
                                    error(
                                        report,
                                        rule,
                                        format!(
                                            "XMP property '{}' uses undeclared namespace prefix '{}'",
                                            full_prop,
                                            prefix.trim_end_matches(':')
                                        ),
                                    );
                                    reported.insert(prefix.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        pos += 1;
    }
}

/// §6.7.3 — Deep Info dict / XMP consistency check.
///
/// Validates all mappings from Info dict to XMP, checking both presence and value:
/// - /Title ↔ dc:title
/// - /Author ↔ dc:creator
/// - /Subject ↔ dc:description
/// - /Creator ↔ xmp:CreatorTool
/// - /Producer ↔ pdf:Producer
/// - /CreationDate ↔ xmp:CreateDate
/// - /ModDate ↔ xmp:ModifyDate
fn check_info_xmp_deep(pdf: &Pdf, xmp: &str, report: &mut ComplianceReport) {
    let metadata = pdf.metadata();

    // /Title ↔ dc:title (§6.7.8)
    if let Some(ref title) = metadata.title {
        let dc_title = extract_rdf_alt_value(xmp, "dc:title");
        match dc_title {
            None => {
                error(
                    report,
                    "6.7.3",
                    "/Info has Title but XMP is missing dc:title",
                );
            }
            Some(ref xmp_val) => {
                let info_str = decode_pdf_string(title);
                if !values_match(&info_str, xmp_val) {
                    error(
                        report,
                        "6.7.3",
                        format!(
                            "Info /Title '{}' does not match XMP dc:title '{}'",
                            info_str, xmp_val
                        ),
                    );
                }
            }
        }
    }

    // /Author ↔ dc:creator
    if let Some(ref author) = metadata.author {
        let dc_creator = extract_rdf_seq_value(xmp, "dc:creator")
            .or_else(|| extract_nested_value(xmp, "dc:creator"));
        match dc_creator {
            None => {
                error(
                    report,
                    "6.7.3",
                    "/Info has Author but XMP is missing dc:creator",
                );
            }
            Some(ref xmp_val) => {
                let info_str = decode_pdf_string(author);
                if !values_match(&info_str, xmp_val) {
                    error(
                        report,
                        "6.7.3",
                        format!(
                            "Info /Author '{}' does not match XMP dc:creator '{}'",
                            info_str, xmp_val
                        ),
                    );
                }
            }
        }
    }

    // /Subject ↔ dc:description
    if let Some(ref subject) = metadata.subject {
        let dc_desc = extract_rdf_alt_value(xmp, "dc:description")
            .or_else(|| extract_nested_value(xmp, "dc:description"));
        match dc_desc {
            None => {
                error(
                    report,
                    "6.7.3",
                    "/Info has Subject but XMP is missing dc:description",
                );
            }
            Some(ref xmp_val) => {
                let info_str = decode_pdf_string(subject);
                if !values_match(&info_str, xmp_val) {
                    error(
                        report,
                        "6.7.3",
                        format!(
                            "Info /Subject '{}' does not match XMP dc:description '{}'",
                            info_str, xmp_val
                        ),
                    );
                }
            }
        }
    }

    // /Creator ↔ xmp:CreatorTool
    if let Some(ref creator) = metadata.creator {
        let xmp_creator = extract_nested_value(xmp, "xmp:CreatorTool");
        match xmp_creator {
            None => {
                error(
                    report,
                    "6.7.3",
                    "/Info has Creator but XMP is missing xmp:CreatorTool",
                );
            }
            Some(ref xmp_val) => {
                let info_str = decode_pdf_string(creator);
                if !values_match(&info_str, xmp_val) {
                    error(
                        report,
                        "6.7.3",
                        format!(
                            "Info /Creator '{}' does not match XMP xmp:CreatorTool '{}'",
                            info_str, xmp_val
                        ),
                    );
                }
            }
        }
    }

    // /Producer ↔ pdf:Producer
    if let Some(ref producer) = metadata.producer {
        let xmp_producer = extract_nested_value(xmp, "pdf:Producer");
        match xmp_producer {
            None => {
                error(
                    report,
                    "6.7.3",
                    "/Info has Producer but XMP is missing pdf:Producer",
                );
            }
            Some(ref xmp_val) => {
                let info_str = decode_pdf_string(producer);
                if !values_match(&info_str, xmp_val) {
                    error(
                        report,
                        "6.7.3",
                        format!(
                            "Info /Producer '{}' does not match XMP pdf:Producer '{}'",
                            info_str, xmp_val
                        ),
                    );
                }
            }
        }
    }

    // /CreationDate ↔ xmp:CreateDate
    if metadata.creation_date.is_some() {
        let xmp_create_date = extract_nested_value(xmp, "xmp:CreateDate");
        if xmp_create_date.is_none() {
            error(
                report,
                "6.7.3",
                "/Info has CreationDate but XMP is missing xmp:CreateDate",
            );
        }
    }

    // /ModDate ↔ xmp:ModifyDate
    if metadata.modification_date.is_some() {
        let xmp_mod_date = extract_nested_value(xmp, "xmp:ModifyDate");
        if xmp_mod_date.is_none() {
            error(
                report,
                "6.7.3",
                "/Info has ModDate but XMP is missing xmp:ModifyDate",
            );
        }
    }
}

/// Decode a PDF string (which may be UTF-16BE with BOM, or PDFDocEncoding).
fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        // UTF-16BE with BOM
        let u16s: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        // PDFDocEncoding (Latin-1 superset) — approximate as ISO 8859-1
        bytes.iter().map(|&b| b as char).collect()
    }
}

/// Compare Info dict value with XMP value, allowing for encoding differences.
fn values_match(info_val: &str, xmp_val: &str) -> bool {
    let info_trimmed = info_val.trim();
    let xmp_trimmed = xmp_val.trim();
    info_trimmed == xmp_trimmed
}

/// Extract a value from an rdf:Alt container (used for dc:title, dc:description).
fn extract_rdf_alt_value(xmp: &str, property: &str) -> Option<String> {
    // Look for <property><rdf:Alt><rdf:li ...>value</rdf:li></rdf:Alt></property>
    let open_tag = format!("<{property}>");
    let close_tag = format!("</{property}>");
    let start = xmp.find(&open_tag)?;
    let block_start = start + open_tag.len();
    let block_end = xmp[block_start..].find(&close_tag)? + block_start;
    let block = &xmp[block_start..block_end];

    // Find rdf:li value inside the Alt
    if let Some(li_start) = block.find("<rdf:li") {
        if let Some(content_start) = block[li_start..].find('>') {
            let val_start = li_start + content_start + 1;
            if let Some(val_end) = block[val_start..].find("</rdf:li>") {
                let value = block[val_start..val_start + val_end].trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }

    None
}

/// Extract a value from an rdf:Seq container (used for dc:creator).
fn extract_rdf_seq_value(xmp: &str, property: &str) -> Option<String> {
    let open_tag = format!("<{property}>");
    let close_tag = format!("</{property}>");
    let start = xmp.find(&open_tag)?;
    let block_start = start + open_tag.len();
    let block_end = xmp[block_start..].find(&close_tag)? + block_start;
    let block = &xmp[block_start..block_end];

    if let Some(li_start) = block.find("<rdf:li") {
        if let Some(content_start) = block[li_start..].find('>') {
            let val_start = li_start + content_start + 1;
            if let Some(val_end) = block[val_start..].find("</rdf:li>") {
                let value = block[val_start..val_start + val_end].trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }

    None
}

/// §6.7.9 — All XMP date/time values must be valid ISO 8601 format.
fn check_date_formats(xmp: &str, report: &mut ComplianceReport) {
    // All date-type XMP properties that must conform to ISO 8601
    let date_properties = [
        "xmp:CreateDate",
        "xmp:ModifyDate",
        "xmp:MetadataDate",
        "photoshop:DateCreated",
        "dc:date",
        "pdf:CreationDate",
        "pdf:ModDate",
        "xmpMM:CreateDate",
    ];

    for prop in &date_properties {
        if let Some(date) = extract_nested_value(xmp, prop) {
            if !is_valid_iso8601(&date) {
                error(
                    report,
                    "6.7.9",
                    format!("XMP date property '{}' value '{}' is not valid ISO 8601 format", prop, date),
                );
            }
        }
    }
}

/// Check if a string is a valid ISO 8601 date/time.
///
/// Accepts: YYYY, YYYY-MM, YYYY-MM-DD, YYYY-MM-DDThh:mm, YYYY-MM-DDThh:mm:ss,
/// YYYY-MM-DDThh:mm:ssTZD, YYYY-MM-DDThh:mm:ss.sTZD
fn is_valid_iso8601(date: &str) -> bool {
    let date = date.trim();
    if date.is_empty() {
        return false;
    }

    // Must start with 4 digits (year)
    if date.len() < 4 || !date[..4].chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    // Year only
    if date.len() == 4 {
        return true;
    }

    // Must have dash after year
    if date.as_bytes().get(4) != Some(&b'-') {
        return false;
    }

    // YYYY-MM
    if date.len() >= 7 {
        let month = &date[5..7];
        if !month.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        let m: u8 = month.parse().unwrap_or(0);
        if !(1..=12).contains(&m) {
            return false;
        }
    }

    // YYYY-MM-DD
    if date.len() >= 10 {
        if date.as_bytes().get(7) != Some(&b'-') {
            return false;
        }
        let day = &date[8..10];
        if !day.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        let d: u8 = day.parse().unwrap_or(0);
        if !(1..=31).contains(&d) {
            return false;
        }
    }

    // If there's a T, validate time portion
    if date.len() > 10 {
        if date.as_bytes().get(10) != Some(&b'T') {
            return false;
        }
        // At minimum hh:mm after T
        if date.len() < 16 {
            return false;
        }
        let hour = &date[11..13];
        let minute = &date[14..16];
        if !hour.chars().all(|c| c.is_ascii_digit()) || !minute.chars().all(|c| c.is_ascii_digit())
        {
            return false;
        }
        if date.as_bytes().get(13) != Some(&b':') {
            return false;
        }
    }

    true
}

/// §6.7.3 — XMP stream must contain a valid RDF root element.
///
/// The XMP specification requires that the payload be wrapped in
/// `<x:xmpmeta>` and contain an `<rdf:RDF>` element.  Absent these
/// elements the stream cannot carry any PDF/A metadata properties.
///
/// Also checks §6.7.11 — the RDF namespace URI must be canonical
/// (`http://www.w3.org/1999/02/22-rdf-syntax-ns#`).  A non-canonical
/// URI (e.g. with `1999/2` instead of `1999/02`) makes the XMP
/// non-conformant.
fn check_xmp_rdf_structure(xmp: &str, report: &mut ComplianceReport) {
    if !xmp.contains("<rdf:RDF") {
        error(
            report,
            "6.7.3",
            "XMP metadata stream is missing required <rdf:RDF> element",
        );
    }

    // Check that rdf:Description elements use rdf:about (qualified), not unqualified about=.
    // Per RDF/XML spec, the about attribute must be namespace-qualified as rdf:about.
    // Using bare `about=""` is invalid RDF/XML and triggers §6.7.3. (#467)
    if xmp.contains(" about=\"") || xmp.contains(" about='") {
        // Make sure this isn't just rdf:about (which is correct)
        // Look for about= that is NOT preceded by rdf:
        let bytes = xmp.as_bytes();
        let mut i = 0;
        while i + 6 < bytes.len() {
            if &bytes[i..i + 7] == b" about=" || &bytes[i..i + 7] == b"\tabout=" {
                // Check it's not "rdf:about=" pattern — look back for "rdf:"
                let prefix_start = i.saturating_sub(4);
                if &bytes[prefix_start..i] != b"rdf:" {
                    error(
                        report,
                        "6.7.3",
                        "rdf:Description uses unqualified 'about' attribute instead of 'rdf:about'",
                    );
                    break;
                }
            }
            i += 1;
        }
    }

    // Check that the RDF namespace URI is the canonical form.
    const CANONICAL_RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    if let Some(pos) = xmp.find("xmlns:rdf=") {
        // Extract the URI value (handles both double- and single-quoted)
        let after = &xmp[pos + 10..];
        let uri = if let Some(s) = after.strip_prefix('"') {
            s.split('"').next()
        } else if let Some(s) = after.strip_prefix('\'') {
            s.split('\'').next()
        } else {
            None
        };
        if let Some(uri) = uri {
            if uri != CANONICAL_RDF_NS {
                error(
                    report,
                    "6.7.11",
                    format!(
                        "XMP uses non-canonical RDF namespace URI '{}' (expected '{}')",
                        uri, CANONICAL_RDF_NS
                    ),
                );
            }
        }
    }
}

/// §6.7.11 — pdfaid:part and pdfaid:conformance must match the actual PDF/A level.
///
/// Checks that the values declared in the XMP PDF/A Identification Schema
/// are consistent with the level that the validator is validating against.
/// A mismatch means the document either claims a different PDF/A version
/// than it actually conforms to, or the identification properties are wrong.
fn check_pdfa_version_match(xmp: &str, level: PdfALevel, report: &mut ComplianceReport) {
    // Extract pdfaid:part — element form <pdfaid:part>N</pdfaid:part>
    // or attribute form pdfaid:part="N"
    let declared_part = extract_nested_value(xmp, "pdfaid:part")
        .or_else(|| {
            // Try attribute-style: pdfaid:part="N"
            let pat = "pdfaid:part=\"";
            xmp.find(pat).and_then(|s| {
                let rest = &xmp[s + pat.len()..];
                rest.find('"').map(|e| rest[..e].trim().to_string())
            })
        });

    if let Some(ref part_str) = declared_part {
        let expected = level.part().to_string();
        if part_str.trim() != expected {
            error(
                report,
                "6.7.11",
                format!(
                    "XMP pdfaid:part is '{}' but document is being validated as PDF/A-{}",
                    part_str.trim(),
                    level.part()
                ),
            );
            // If part already mismatches, conformance check is moot
            return;
        }
    }

    // Extract pdfaid:conformance and compare with level.conformance()
    // PDF/A-4 must NOT have pdfaid:conformance (checked separately in
    // check_pdfa4_conformance_absent); skip the comparison for part 4.
    if level.part() != 4 {
        let declared_conformance = extract_nested_value(xmp, "pdfaid:conformance").or_else(|| {
            let pat = "pdfaid:conformance=\"";
            xmp.find(pat).and_then(|s| {
                let rest = &xmp[s + pat.len()..];
                rest.find('"').map(|e| rest[..e].trim().to_string())
            })
        });

        if let Some(ref conf_str) = declared_conformance {
            let expected_conf = level.conformance();
            if !expected_conf.is_empty()
                && conf_str.trim().to_uppercase() != expected_conf.to_uppercase()
            {
                error(
                    report,
                    "6.7.11",
                    format!(
                        "XMP pdfaid:conformance is '{}' but document is being validated as PDF/A-{}{}",
                        conf_str.trim(),
                        level.part(),
                        level.conformance()
                    ),
                );
            }
        }
    }
}

/// §6.7.4 — pdfaid:amd must not be present in PDF/A-2, PDF/A-3, or PDF/A-4.
///
/// §6.7.5 — pdfaid:corr handling.
fn check_pdfa_id_properties(xmp: &str, level: PdfALevel, report: &mut ComplianceReport) {
    if level.part() >= 2 {
        // §6.7.4: pdfaid:amd forbidden in PDF/A-2, PDF/A-3, and PDF/A-4
        let has_amd =
            extract_nested_value(xmp, "pdfaid:amd").is_some() || xmp.contains("pdfaid:amd=");
        if has_amd {
            error(
                report,
                "6.7.4",
                format!("pdfaid:amd must not be present in PDF/A-{}", level.part()),
            );
        }
    }

    // §6.7.5: if pdfaid:corr is present, it must be a valid integer
    if let Some(corr) = extract_nested_value(xmp, "pdfaid:corr") {
        if corr.parse::<u32>().is_err() {
            error(
                report,
                "6.7.5",
                format!("pdfaid:corr value '{}' is not a valid integer", corr),
            );
        }
    }

    // §6.7.3: if pdfaid:rev is present, it must be a valid 4-digit year
    if let Some(rev) = extract_nested_value(xmp, "pdfaid:rev") {
        if rev.len() != 4 || rev.parse::<u32>().is_err() {
            error(
                report,
                "6.7.3",
                format!("pdfaid:rev value '{}' is not a valid four-digit year", rev),
            );
        }
    }
}

/// §6.7.8 — dc:title must be present if /Title exists in Info dict.
fn check_dc_title_consistency(pdf: &Pdf, xmp: &str, report: &mut ComplianceReport) {
    let metadata = pdf.metadata();
    if metadata.title.is_some() {
        let dc_title = extract_rdf_alt_value(xmp, "dc:title")
            .or_else(|| extract_nested_value(xmp, "dc:title"));
        if dc_title.is_none() {
            error(
                report,
                "6.7.8",
                "Info dict has /Title but XMP is missing dc:title (required by §6.7.8)",
            );
        }
    }
}

/// §6.7.11 — XMP properties must not use deprecated types.
///
/// Deprecated properties: xmp:Identifier (use xmpMM:Identifier instead),
/// xmpMM:SaveID, etc.
fn check_deprecated_types(xmp: &str, report: &mut ComplianceReport) {
    let deprecated_properties = [
        ("xmp:Identifier", "Use xmpMM:Identifier instead"),
        ("xmpMM:SaveID", "SaveID is deprecated"),
        ("xmpMM:LastURL", "LastURL is deprecated"),
        ("xmpMM:RenditionOf", "Use xmpMM:DerivedFrom instead"),
    ];

    for (prop, hint) in &deprecated_properties {
        if xmp.contains(&format!("<{prop}>")) || xmp.contains(&format!("{prop}=\"")) {
            error(
                report,
                "6.7.11",
                format!("Deprecated XMP property '{}' found. {}", prop, hint),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_valid_dates() {
        assert!(is_valid_iso8601("2024"));
        assert!(is_valid_iso8601("2024-01"));
        assert!(is_valid_iso8601("2024-01-15"));
        assert!(is_valid_iso8601("2024-01-15T10:30"));
        assert!(is_valid_iso8601("2024-01-15T10:30:00"));
        assert!(is_valid_iso8601("2024-01-15T10:30:00Z"));
        assert!(is_valid_iso8601("2024-01-15T10:30:00+01:00"));
        assert!(is_valid_iso8601("2024-01-15T10:30:00.123Z"));
    }

    #[test]
    fn iso8601_invalid_dates() {
        assert!(!is_valid_iso8601(""));
        assert!(!is_valid_iso8601("abc"));
        assert!(!is_valid_iso8601("2024-13")); // invalid month
        assert!(!is_valid_iso8601("2024-00")); // month 0
        assert!(!is_valid_iso8601("2024-01-32")); // day 32
    }

    #[test]
    fn parse_extension_schema_basic() {
        let xmp = r#"
        <pdfaExtension:schemas>
            <rdf:Bag>
                <rdf:li rdf:parseType="Resource">
                    <pdfaSchema:schema>Custom Schema</pdfaSchema:schema>
                    <pdfaSchema:namespaceURI>http://example.com/ns/</pdfaSchema:namespaceURI>
                    <pdfaSchema:prefix>custom</pdfaSchema:prefix>
                    <pdfaSchema:property>
                        <rdf:Seq>
                            <rdf:li rdf:parseType="Resource">
                                <pdfaProperty:name>myProp</pdfaProperty:name>
                                <pdfaProperty:valueType>Text</pdfaProperty:valueType>
                                <pdfaProperty:category>internal</pdfaProperty:category>
                                <pdfaProperty:description>A custom property</pdfaProperty:description>
                            </rdf:li>
                        </rdf:Seq>
                    </pdfaSchema:property>
                </rdf:li>
            </rdf:Bag>
        </pdfaExtension:schemas>"#;

        let schemas = parse_extension_schemas(xmp);
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].namespace_uri, "http://example.com/ns/");
        assert_eq!(schemas[0].prefix, "custom");
        assert_eq!(schemas[0].properties.len(), 1);
        assert_eq!(schemas[0].properties[0].name, "myProp");
        assert_eq!(schemas[0].properties[0].value_type, "Text");
        assert_eq!(schemas[0].properties[0].category, "internal");
    }

    #[test]
    fn valid_value_types() {
        let custom = HashSet::new();
        assert!(is_valid_value_type("Text", &custom));
        assert!(is_valid_value_type("Boolean", &custom));
        assert!(is_valid_value_type("Date", &custom));
        assert!(is_valid_value_type("URI", &custom));
        assert!(is_valid_value_type("bag Text", &custom));
        assert!(is_valid_value_type("Seq ResourceEvent", &custom));
        assert!(!is_valid_value_type("Nonexistent", &custom));
    }

    #[test]
    fn custom_value_types() {
        let mut custom = HashSet::new();
        custom.insert("MyCustomType".to_string());
        assert!(is_valid_value_type("MyCustomType", &custom));
        assert!(is_valid_value_type("Bag MyCustomType", &custom));
    }

    #[test]
    fn packet_header_check() {
        let mut report = ComplianceReport::default();
        check_xmp_packet_header(
            r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta/>"#,
            &mut report,
        );
        assert_eq!(report.error_count(), 0);
    }

    #[test]
    fn packet_header_missing() {
        let mut report = ComplianceReport::default();
        check_xmp_packet_header("<x:xmpmeta/>", &mut report);
        assert!(report.error_count() > 0);
    }

    #[test]
    fn rdf_alt_value_extraction() {
        let xmp = r#"<dc:title><rdf:Alt><rdf:li xml:lang="x-default">My Title</rdf:li></rdf:Alt></dc:title>"#;
        assert_eq!(
            extract_rdf_alt_value(xmp, "dc:title"),
            Some("My Title".to_string())
        );
    }

    #[test]
    fn rdf_seq_value_extraction() {
        let xmp = r#"<dc:creator><rdf:Seq><rdf:li>John Doe</rdf:li></rdf:Seq></dc:creator>"#;
        assert_eq!(
            extract_rdf_seq_value(xmp, "dc:creator"),
            Some("John Doe".to_string())
        );
    }

    /// Regression test for #448: multi-byte UTF-8 chars inside rdf:Bag content
    /// must not cause a `byte index N is not a char boundary` panic.
    #[test]
    fn extract_li_blocks_multibyte_utf8_no_panic() {
        // Simulate XMP where rdf:li text contains non-ASCII (é, Ü, 中文 …).
        let xmp = "<rdf:Bag>\
            <rdf:li>Héllo</rdf:li>\
            <rdf:li>Wörld — 日本語</rdf:li>\
            </rdf:Bag>";
        // Must not panic.
        let blocks = extract_top_level_li_blocks(xmp);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("Héllo"));
        assert!(blocks[1].contains("Wörld"));
    }

    #[test]
    fn xmlns_prefix_not_flagged() {
        // xmlns: is a reserved XML namespace-declaration attribute — should not
        // be reported as an undeclared namespace prefix (#443).
        let mut report = ComplianceReport::default();
        let xmp = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">
      <pdfaid:part>2</pdfaid:part>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;
        let schemas = parse_extension_schemas(xmp);
        check_property_namespaces(xmp, &schemas, crate::PdfALevel::A2b, &mut report);
        // `xmlns:` must not produce a false-positive namespace error
        let xmlns_errors: Vec<_> = report
            .issues
            .iter()
            .filter(|i| i.message.contains("xmlns"))
            .collect();
        assert!(
            xmlns_errors.is_empty(),
            "unexpected xmlns: errors: {xmlns_errors:?}"
        );
    }

    #[test]
    fn deprecated_property_detection() {
        let mut report = ComplianceReport::default();
        let xmp = r#"<xmpMM:SaveID>12345</xmpMM:SaveID>"#;
        check_deprecated_types(xmp, &mut report);
        assert!(report.error_count() > 0);
        assert!(report.issues[0].rule == "6.7.11");
    }
}
