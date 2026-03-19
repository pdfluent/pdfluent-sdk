//! Deep XMP metadata validation for PDF/A compliance.
//!
//! Implements checks from ISO 19005 sections 6.6 and 6.7:
//! - Extension schema parsing and validation (6.6.2.3.1, 6.6.2.3.3)
//! - Property namespace validation (6.7.9)
//! - Info/XMP consistency (6.7.3, 6.7.3.3, 6.7.3.4)
//! - Additional XMP property rules (6.7.4, 6.7.5, 6.7.8, 6.7.11)
//! - XMP stream and packet validation (6.6.2, 6.6.2.1)

use std::collections::{HashMap, HashSet};

use crate::check::{self, error, warning};
use crate::{ComplianceReport, PdfALevel};
use pdf_syntax::object::dict::keys;
use pdf_syntax::object::{Array, Dict, Name, ObjRef, Object};
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
    // --- Structural checks that run regardless of XMP presence ---
    // §6.9 (PDF/A-2/3) / §6.10 (PDF/A-4): OCProperties/D must not have /AS.
    // §6.10 (PDF/A-2/3): OCG Order must contain all referenced OCGs.
    check_oc_d_as_restriction(pdf, level, report);
    check_ocg_order_completeness(pdf, level, report);
    // §6.10 (PDF/A-2/3) / §6.11 (PDF/A-4): Names/AlternatePresentations and /PresSteps forbidden.
    check_alternate_presentations_absent(pdf, level, report);
    // §6.12 (PDF/A-2/3/4): /Requirements key in catalog is forbidden.
    check_requirements_absent(pdf, level, report);
    // §6.6.2 (PDF/A-2/3): Widget annotations must not have /AA entry.
    check_widget_aa_pdfa23(pdf, level, report);
    // §6.7.2.2 (PDF/A-2/3): MarkInfo/Marked required for tagged conformance.
    check_mark_info_required(pdf, level, report);
    // §6.7.3.3 (PDF/A-2/3/4): StructTreeRoot required for tagged conformance.
    check_struct_tree_root_required(pdf, level, report);
    // §6.7.3.4: RoleMap must not contain cycles.
    check_role_map_no_cycles(pdf, report);

    let Some(xmp_data) = check::get_xmp_metadata(pdf) else {
        return; // Missing XMP is caught by check_xmp_metadata
    };
    let Ok(xmp_text) = std::str::from_utf8(&xmp_data) else {
        // §6.7.3 — XMP metadata stream must be UTF-8 encoded
        error(report, "6.7.3", "XMP metadata stream is not valid UTF-8");
        // §6.7.9 — XMP namespace prefixes cannot be validated when XMP is not valid UTF-8.
        // veraPDF reports §6.7.9 for non-UTF-8 XMP in addition to §6.7.3. Fixes #467 (PDFBOX-1760-11).
        error(
            report,
            "6.7.9",
            "XMP namespace prefixes cannot be validated: stream is not valid UTF-8",
        );
        return;
    };

    // §6.7.3 — XMP stream must contain valid RDF structure
    check_xmp_rdf_structure(xmp_text, level, report);

    check_xmp_packet_header(xmp_text, report);
    // §6.6.2.1 (PDF/A-2/3), §6.7.2.1 (PDF/A-4), §6.7.5 (PDF/A-1):
    // The 'bytes' attribute is forbidden in the xpacket PI for all PDF/A parts.
    // veraPDF uses the part-specific clause. Fixes #FN-6.6.2.1.
    // Note: PDF/A-1 maps to §6.7.5 (isartor-6-7-5-t01-fail-a tests this). (#FN-6.7.5)
    if let Some(xp_start) = xmp_text.find("<?xpacket") {
        let xp_end = xmp_text[xp_start..].find("?>").unwrap_or(0);
        let xp_header = &xmp_text[xp_start..xp_start + xp_end + 2];
        if xp_header.contains("bytes=") {
            let bytes_rule = match level.part() {
                1 => "6.7.5", // veraPDF uses §6.7.5 for xpacket bytes= in PDF/A-1 (#FN-6.7.5)
                4 => "6.7.2.1",
                _ => "6.6.2.1", // PDF/A-2/3
            };
            error(
                report,
                bytes_rule,
                "XMP packet header contains forbidden 'bytes' attribute",
            );
        }
        // §6.6.2.1 (PDF/A-2/3), §6.7.2.1 (PDF/A-4), §6.7.5 (PDF/A-1):
        // The 'encoding' attribute is forbidden in the xpacket PI for all PDF/A parts.
        // veraPDF uses the part-specific clause for this violation. Fixes #FN-6.6.2.1.
        if xp_header.contains("encoding=") {
            let enc_rule = match level.part() {
                1 => "6.7.5", // veraPDF uses §6.7.5 for xpacket encoding= in PDF/A-1 (#FN-6.7.5)
                4 => "6.7.2.1",
                _ => "6.6.2.1", // PDF/A-2/3
            };
            error(
                report,
                enc_rule,
                "XMP packet header contains forbidden 'encoding' attribute",
            );
        }
    }
    let schemas = parse_extension_schemas(xmp_text);
    check_extension_schema_structure(xmp_text, &schemas, report);
    let ns_violations_before = report.issues.len();
    check_property_namespaces(xmp_text, &schemas, level, report);
    // PDF/A-1 §6.7.11: undeclared namespace prefix violations (§6.7.9.1/§6.7.9.2) in the
    // XMP also trigger §6.7.11 because the identification schema cannot be reliably parsed
    // when prefixes are missing. veraPDF reports BOTH §6.7.9 AND §6.7.11 in these cases.
    // Fixes #467 (poppler-106863-0.pdf).
    if level.part() == 1
        && report.issues[ns_violations_before..]
            .iter()
            .any(|i| i.rule.starts_with("6.7.9"))
    {
        error(
            report,
            "6.7.11",
            "XMP namespace violations affect PDF/A identification schema reliability",
        );
    }
    check_info_xmp_deep(pdf, xmp_text, report);
    check_date_formats(xmp_text, level, report);
    check_pdfa_id_properties(xmp_text, level, report);
    check_pdfa_version_match(xmp_text, level, report);
    // Note: dc:title consistency is covered by check_info_xmp_deep (§6.7.3.2).
    // The separate check_dc_title_consistency was removed to avoid emitting
    // the wrong clause "6.7.8" for a case where veraPDF uses "6.7.3.2". (#467)
    check_deprecated_types(xmp_text, report);
    // §6.6.2.3.1 test=2 / §6.7.9 test=3 — predefined property value types
    check_predefined_property_types(xmp_text, level, report);
    // §6.7.9 test=3 / §6.6.2.3.1 test=3 — non-standard properties in pdf: namespace
    check_pdf_namespace_properties(xmp_text, level, report);
    // §6.7.9.2 (PDF/A-1) / §6.6.2.3.1 — unknown properties in closed XMP namespaces (#489)
    check_xmp_closed_schema_properties(xmp_text, level, report);
    check_closed_namespace_properties(
        xmp_text,
        &schemas,
        "photoshop:",
        VALID_PHOTOSHOP_PROPERTIES,
        level,
        report,
    );
    check_closed_namespace_properties(
        xmp_text,
        &schemas,
        "xmpRights:",
        VALID_XMPRIGHTS_PROPERTIES,
        level,
        report,
    );
    check_closed_namespace_properties(
        xmp_text,
        &schemas,
        "pdfaid:",
        VALID_PDFAID_PROPERTIES,
        level,
        report,
    );
    // §6.7.9.2 (PDF/A-1) / §6.6.2.3.1 — properties not predefined in XMP 2004 per veraPDF (#489)
    check_not_predefined_properties(xmp_text, &schemas, level, report);
    // PDF/A-1 §6.7.9: rdf:li with bare 'lang=' attribute (not 'xml:lang=') uses a
    // property from an unregistered namespace. veraPDF reports §6.7.9 in addition to
    // the §6.7.11 type violation. Fixes #467 (PDFBOX-3017-0.pdf).
    if level.part() == 1 && xmp_text.contains("<rdf:li") {
        let has_bare_lang = xmp_text
            .find("<rdf:li")
            .map(|start| {
                // Check all rdf:li opening tags for bare lang= (not xml:lang=)
                let mut pos = start;
                let mut found = false;
                while let Some(li) = xmp_text[pos..].find("<rdf:li") {
                    let abs = pos + li;
                    let tag_end = xmp_text[abs..].find('>').map(|e| abs + e).unwrap_or(abs);
                    let tag = &xmp_text[abs..=tag_end];
                    // bare lang= without xml:lang=
                    if !tag.contains("xml:lang") {
                        let after_li = &tag[7..]; // skip "<rdf:li"
                        if after_li.contains(" lang=") || after_li.contains("\tlang=") {
                            found = true;
                            break;
                        }
                    }
                    pos = tag_end + 1;
                }
                found
            })
            .unwrap_or(false);
        if has_bare_lang {
            error(
                report,
                "6.7.9",
                "rdf:li uses bare 'lang' attribute instead of 'xml:lang' (unregistered attribute namespace)",
            );
        }
    }
    // §6.7.11 test=4/5 — pdfaid namespace must use 'pdfaid' prefix
    check_pdfaid_prefix(xmp_text, report);
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
/// Deprecated XMP namespace prefixes that were renamed in XMP Spec Part 1 (2012 ed.).
///
/// When one of these prefixes is used, veraPDF flags §6.7.9.2 (deprecated alias)
/// rather than §6.7.9.1 (completely unknown/undeclared namespace). (#467)
const DEPRECATED_XMP_PREFIXES: &[&str] = &[
    "xap:",       // renamed → xmp:
    "xapMM:",     // renamed → xmpMM:
    "xapBJ:",     // renamed → xmpBJ:
    "xapTPg:",    // renamed → xmpTPg:
    "xapDM:",     // renamed → xmpDM:
    "xapRights:", // renamed → xmpRights:
    "xapidq:",    // renamed → xmpidq:
];

fn check_property_namespaces(
    xmp: &str,
    schemas: &[ExtensionSchema],
    level: PdfALevel,
    report: &mut ComplianceReport,
) {
    // Use veraPDF subclause numbers for PDF/A-1 to eliminate false negatives. (#467)
    // §6.7.9.1 = malformed XMP / undeclared namespace
    // §6.7.9.2 = deprecated namespace alias
    // §6.7.9.3 = predefined property value type violation
    // For PDF/A-2/3: §6.6.2.3.1; PDF/A-4: §6.5.2
    let (rule_undeclared, rule_deprecated) = match level.part() {
        1 => ("6.7.9.1", "6.7.9.2"),
        4 => ("6.5.2", "6.5.2"),
        _ => ("6.6.2.3.1", "6.6.2.3.1"),
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
                                let undeclared =
                                    prefix != "xml:" && !declared_prefixes.contains(prefix);

                                if unknown_prefix || undeclared {
                                    // Deprecated aliases (xap:, xapMM:, etc.) get §6.7.9.2;
                                    // completely unknown/undeclared prefixes get §6.7.9.1. (#467)
                                    let is_deprecated = DEPRECATED_XMP_PREFIXES.contains(&prefix);
                                    let violation_rule = if is_deprecated {
                                        rule_deprecated
                                    } else {
                                        rule_undeclared
                                    };
                                    error(
                                        report,
                                        violation_rule,
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
/// - /Title ↔ dc:title         (§6.7.3.2)
/// - /Author ↔ dc:creator      (§6.7.3.3)
/// - /Subject ↔ dc:description (§6.7.3.4)
/// - /Keywords ↔ pdf:Keywords  (§6.7.3.5 — covered by check_info_xmp_consistency)
/// - /Creator ↔ xmp:CreatorTool (§6.7.3.6)
/// - /Producer ↔ pdf:Producer  (§6.7.3.7)
/// - /ModDate ↔ xmp:ModifyDate (§6.7.3.8)
///
/// Uses veraPDF subclause numbers so the comparison matches exactly. (#467)
fn check_info_xmp_deep(pdf: &Pdf, xmp: &str, report: &mut ComplianceReport) {
    let metadata = pdf.metadata();

    // /Title ↔ dc:title (§6.7.3.2)
    if let Some(ref title) = metadata.title {
        let dc_title = extract_rdf_alt_value(xmp, "dc:title");
        match dc_title {
            None => {
                error(
                    report,
                    "6.7.3.2",
                    "/Info has Title but XMP is missing dc:title",
                );
            }
            Some(ref xmp_val) => {
                let info_str = decode_pdf_string(title);
                if !values_match(&info_str, xmp_val) {
                    error(
                        report,
                        "6.7.3.2",
                        format!(
                            "Info /Title '{}' does not match XMP dc:title '{}'",
                            info_str, xmp_val
                        ),
                    );
                }
            }
        }
    }

    // /Author ↔ dc:creator (§6.7.3.3)
    if let Some(ref author) = metadata.author {
        let dc_creator = extract_rdf_seq_value(xmp, "dc:creator")
            .or_else(|| extract_nested_value(xmp, "dc:creator"));
        match dc_creator {
            None => {
                error(
                    report,
                    "6.7.3.3",
                    "/Info has Author but XMP is missing dc:creator",
                );
            }
            Some(ref xmp_val) => {
                let info_str = decode_pdf_string(author);
                if !values_match(&info_str, xmp_val) {
                    error(
                        report,
                        "6.7.3.3",
                        format!(
                            "Info /Author '{}' does not match XMP dc:creator '{}'",
                            info_str, xmp_val
                        ),
                    );
                }
            }
        }
    }

    // /Subject ↔ dc:description (§6.7.3.4)
    if let Some(ref subject) = metadata.subject {
        // Empty /Subject is trivially consistent with no dc:description.
        // veraPDF does not flag 6.7.3.4 for /Subject () with no dc:description.
        let info_str = decode_pdf_string(subject);
        if info_str.trim().is_empty() {
            // nothing to check
        } else {
            let dc_desc = extract_rdf_alt_value(xmp, "dc:description")
                .or_else(|| extract_nested_value(xmp, "dc:description"));
            match dc_desc {
                None => {
                    error(
                        report,
                        "6.7.3.4",
                        "/Info has Subject but XMP is missing dc:description",
                    );
                }
                Some(ref xmp_val) => {
                    if !values_match(&info_str, xmp_val) {
                        error(
                            report,
                            "6.7.3.4",
                            format!(
                                "Info /Subject '{}' does not match XMP dc:description '{}'",
                                info_str, xmp_val
                            ),
                        );
                    }
                }
            }
        } // end non-empty subject check
    }

    // /Creator ↔ xmp:CreatorTool (§6.7.3.6)
    if let Some(ref creator) = metadata.creator {
        let xmp_creator = extract_nested_value(xmp, "xmp:CreatorTool");
        match xmp_creator {
            None => {
                error(
                    report,
                    "6.7.3.6",
                    "/Info has Creator but XMP is missing xmp:CreatorTool",
                );
            }
            Some(ref xmp_val) => {
                let info_str = decode_pdf_string(creator);
                if !values_match(&info_str, xmp_val) {
                    error(
                        report,
                        "6.7.3.6",
                        format!(
                            "Info /Creator '{}' does not match XMP xmp:CreatorTool '{}'",
                            info_str, xmp_val
                        ),
                    );
                }
            }
        }
    }

    // /Producer ↔ pdf:Producer (§6.7.3.7)
    if let Some(ref producer) = metadata.producer {
        let xmp_producer = extract_nested_value(xmp, "pdf:Producer");
        match xmp_producer {
            None => {
                error(
                    report,
                    "6.7.3.7",
                    "/Info has Producer but XMP is missing pdf:Producer",
                );
            }
            Some(ref xmp_val) => {
                let info_str = decode_pdf_string(producer);
                if !values_match(&info_str, xmp_val) {
                    error(
                        report,
                        "6.7.3.7",
                        format!(
                            "Info /Producer '{}' does not match XMP pdf:Producer '{}'",
                            info_str, xmp_val
                        ),
                    );
                }
            }
        }
    }

    // /ModDate ↔ xmp:ModifyDate (§6.7.3.8)
    // Check both directions: /Info→XMP and XMP→/Info.
    // veraPDF fires §6.7.3 when XMP has a date that /Info doesn't (reverse direction). (#FN-6.7.3)
    let xmp_mod_date = extract_nested_value(xmp, "xmp:ModifyDate");
    if metadata.modification_date.is_some() && xmp_mod_date.is_none() {
        error(
            report,
            "6.7.3",
            "/Info has ModDate but XMP is missing xmp:ModifyDate",
        );
    } else if metadata.modification_date.is_none() && xmp_mod_date.is_some() {
        error(
            report,
            "6.7.3",
            "XMP has xmp:ModifyDate but /Info is missing /ModDate",
        );
    }

    // /CreationDate ↔ xmp:CreateDate (§6.7.3.1) — also check reverse direction.
    let xmp_create_date = extract_nested_value(xmp, "xmp:CreateDate");
    if metadata.creation_date.is_some() && xmp_create_date.is_none() {
        error(
            report,
            "6.7.3",
            "/Info has CreationDate but XMP is missing xmp:CreateDate",
        );
    } else if metadata.creation_date.is_none() && xmp_create_date.is_some() {
        error(
            report,
            "6.7.3",
            "XMP has xmp:CreateDate but /Info is missing /CreationDate",
        );
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
fn check_date_formats(xmp: &str, level: PdfALevel, report: &mut ComplianceReport) {
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

    // For PDF/A-2/3/4 veraPDF reports invalid date values under §6.6.2.3.1
    // ("property not used in accordance with definition"), not §6.7.9.
    // PDF/A-1 uses §6.7.9 (XMP property namespace violations). Fixes #477.
    let rule = match level.part() {
        1 => "6.7.9",
        4 => "6.5.2",
        _ => "6.6.2.3.1",
    };

    for prop in &date_properties {
        if let Some(date) = extract_nested_value(xmp, prop) {
            if !is_valid_iso8601(&date) {
                error(
                    report,
                    rule,
                    format!(
                        "XMP date property '{}' value '{}' is not valid ISO 8601 format",
                        prop, date
                    ),
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
fn check_xmp_rdf_structure(xmp: &str, level: PdfALevel, report: &mut ComplianceReport) {
    if !xmp.contains("<rdf:RDF") {
        error(
            report,
            "6.7.3",
            "XMP metadata stream is missing required <rdf:RDF> element",
        );
    }

    // Check that rdf:Description elements use rdf:about (qualified), not unqualified about=.
    // Per RDF/XML spec, the about attribute must be namespace-qualified as rdf:about.
    // Using bare `about=""` is invalid RDF/XML — veraPDF maps this to §6.7.9 for PDF/A-1
    // (isartor-6-7-9-t01-fail-a) as "malformed XMP metadata". (#467)
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
                    // veraPDF uses §6.7.9.1 for all PDF/A versions when XMP is malformed
                    // (unqualified bare `about` attribute is invalid per RDF/XML spec,
                    // which is part of the XMP specification). (#467)
                    let rule = match level.part() {
                        4 => "6.5.2", // PDF/A-4 normalized equivalent
                        _ => "6.7.9.1",
                    };
                    error(
                        report,
                        rule,
                        "rdf:Description uses unqualified 'about' attribute instead of 'rdf:about'",
                    );
                    // §6.7.11 cascade: malformed XMP means pdfaid cannot be verified.
                    // veraPDF always flags 6.7.11 when 6.7.9.1 is present. (#467)
                    error(
                        report,
                        "6.7.11",
                        "XMP is malformed (6.7.9.1 violation) — PDF/A identification cannot be verified",
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
    // §6.7.11 test 1: XMP must use the correct pdfaid namespace URI.
    // The canonical URI is "http://www.aiim.org/pdfa/ns/id/" (trailing slash).
    // A wrong URI (e.g. with .html suffix) means the identification schema is
    // not recognised by conforming processors. (#467)
    let has_correct_pdfaid_ns = xmp.contains("http://www.aiim.org/pdfa/ns/id/");
    let has_pdfaid_part = xmp.contains("pdfaid:part");
    if has_pdfaid_part && !has_correct_pdfaid_ns {
        error(
            report,
            "6.7.11",
            "XMP pdfaid namespace URI is wrong or missing (must be 'http://www.aiim.org/pdfa/ns/id/')",
        );
        return;
    }
    if !has_pdfaid_part {
        error(
            report,
            "6.7.11",
            "XMP does not contain pdfaid:part (PDF/A Identification Schema is absent)",
        );
        return;
    }

    // Extract pdfaid:part — element form <pdfaid:part>N</pdfaid:part>
    // or attribute form pdfaid:part="N"
    let declared_part = extract_nested_value(xmp, "pdfaid:part").or_else(|| {
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
            let actual = conf_str.trim();
            if !expected_conf.is_empty() {
                // Case-sensitive comparison: spec requires uppercase letter (e.g. "A", "B", "U").
                // A lowercase value (e.g. "a") is a §6.7.11 violation. (#467)
                if actual != expected_conf {
                    error(
                        report,
                        "6.7.11",
                        format!(
                            "XMP pdfaid:conformance is '{}' but expected '{}' (PDF/A-{}{})",
                            actual,
                            expected_conf,
                            level.part(),
                            level.conformance()
                        ),
                    );
                }
            }
        } else {
            // §6.7.11: pdfaid:conformance is required for PDF/A-1/2/3.
            // Its absence (when pdfaid:part is present) is a violation. Fixes #FN-6.7.11.
            let expected_conf = level.conformance();
            if !expected_conf.is_empty() {
                error(
                    report,
                    "6.7.11",
                    format!(
                        "XMP pdfaid:conformance is absent (expected '{}' for PDF/A-{}{})",
                        expected_conf,
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

/// Category of a predefined XMP property's value type, used to validate
/// how the property is serialised in RDF/XML (§6.6.2.3.1 test=2, §6.7.9 test=3).
#[derive(Clone, Copy, PartialEq, Eq)]
enum PropValueKind {
    /// Scalar: Text, Boolean, URI, Date, … — must NOT be wrapped in rdf container.
    Scalar,
    /// Lang Alt — must use <rdf:Alt><rdf:li xml:lang="…">
    LangAlt,
    /// Ordered array — must use <rdf:Seq>
    Seq,
    /// Unordered array — must use <rdf:Bag>
    Bag,
    /// Scalar Real — like Scalar but additionally the value must not use rational
    /// notation (slash-separated numerator/denominator). XMP Real is a floating-point
    /// number; Rational is a separate XMP type. Fixes §6.6.2.3.1 t04. (#477)
    Real,
    /// Scalar Integer — like Scalar but additionally the value must not
    /// contain a decimal point or exponent (veraPDF 6.6.2.3.1 test=2).
    Integer,
    /// Scalar Date — like Scalar (no rdf container) and additionally the value
    /// must be a valid ISO 8601 date string. Fixes §6.6.2.3.1 t01: properties
    /// like xmpDM:shotDate with values like "Date: 2016-..." are invalid. (#FN-6.6.2.3.1-t01)
    Date,
    /// Scalar Boolean — must be exactly "true" or "false" (XMP spec: case-sensitive).
    /// "TRUE", "False", "1" etc. are invalid XMP Boolean values. (#FN-6.6.2.3.1-t08)
    Boolean,
    /// Scalar Capitalized Boolean — must be exactly "True" or "False".
    /// Used by the Adobe CRS (Camera Raw Settings) schema, which follows Adobe's own
    /// convention rather than the XMP Boolean type. "true"/"false" (lowercase) are invalid.
    /// (#FN-6.6.2.3.1-t04)
    CapBoolean,
    /// Structure — must use rdf:parseType="Resource" or contain child elements.
    /// Plain text is invalid for struct types (e.g. xmpDM:startTimecode = Timecode struct).
    Struct,
    /// Ordered array of Integer items — the Seq container must hold only valid integer
    /// values in each rdf:li. Used for tiff:BitsPerSample, exif:ISOSpeedRatings.
    /// Catches "8.0" decimal and "1/1" rational items. (#489)
    SeqInteger,
}

/// Look up the expected value kind for a well-known predefined XMP property.
///
/// Returns `None` for properties not in our table (we don't flag those).
/// The table is built from the XMP specification part 2 (predefined schemas)
/// and covers the properties actually tested by veraPDF's 6.6.2.3.1 / 6.7.9
/// test suite.
fn predefined_prop_kind(qualified_name: &str) -> Option<PropValueKind> {
    use PropValueKind::*;
    match qualified_name {
        // ── dc: (Dublin Core) ────────────────────────────────────────────────
        "dc:contributor" => Some(Bag),
        "dc:coverage" => Some(Scalar),
        "dc:creator" => Some(Seq),
        "dc:date" => Some(Seq),
        "dc:description" => Some(LangAlt),
        "dc:format" => Some(Scalar),
        "dc:identifier" => Some(Scalar),
        "dc:language" => Some(Bag),
        "dc:publisher" => Some(Bag),
        "dc:relation" => Some(Bag),
        "dc:rights" => Some(LangAlt),
        "dc:source" => Some(Scalar),
        "dc:subject" => Some(Bag),
        "dc:title" => Some(LangAlt),
        "dc:type" => Some(Bag),

        // ── xmp: (XMP Basic) ─────────────────────────────────────────────────
        "xmp:Advisory" => Some(Bag),
        "xmp:BaseURL" => Some(Scalar),
        "xmp:CreateDate" => Some(Date),
        "xmp:CreatorTool" => Some(Scalar),
        "xmp:Identifier" => Some(Bag),
        "xmp:Label" => Some(Scalar),
        "xmp:MetadataDate" => Some(Date),
        "xmp:ModifyDate" => Some(Date),
        "xmp:Nickname" => Some(Scalar),
        "xmp:Rating" => Some(Scalar),
        "xmp:Thumbnails" => Some(Bag),

        // ── xmpRights: ───────────────────────────────────────────────────────
        "xmpRights:Certificate" => Some(Scalar),
        "xmpRights:Marked" => Some(Boolean),
        "xmpRights:Owner" => Some(Bag),
        "xmpRights:UsageTerms" => Some(LangAlt),
        "xmpRights:WebStatement" => Some(Scalar),

        // ── xmpMM: (Media Management) ────────────────────────────────────────
        "xmpMM:DerivedFrom" => Some(Struct), // ResourceRef struct type, not scalar. (#FN-6.6.2.3.1-t09)
        "xmpMM:DocumentID" => Some(Scalar),
        "xmpMM:History" => Some(Seq),
        "xmpMM:Ingredients" => Some(Bag),
        "xmpMM:InstanceID" => Some(Scalar),
        "xmpMM:ManagedFrom" => Some(Struct), // ResourceRef structure, not scalar. (#FN-6.7.9-t09)
        "xmpMM:Manager" => Some(Scalar),
        "xmpMM:ManageTo" => Some(Scalar),
        "xmpMM:ManageUI" => Some(Scalar),
        "xmpMM:ManagerVariant" => Some(Scalar),
        "xmpMM:OriginalDocumentID" => Some(Scalar),
        "xmpMM:Pantry" => Some(Bag),
        "xmpMM:RenditionClass" => Some(Scalar),
        "xmpMM:RenditionOf" => Some(Struct), // ResourceRef struct type, not scalar. (#FN-6.6.2.3.1-t09)
        "xmpMM:RenditionParams" => Some(Scalar),
        "xmpMM:VersionID" => Some(Scalar),
        "xmpMM:Versions" => Some(Seq),
        // LastURL is deprecated but veraPDF still validates it as Scalar.
        // Using it wrapped in rdf:Seq is a §6.6.2.3.1 violation. (#FN-6.6.2.3.1-t09)
        "xmpMM:LastURL" => Some(Scalar),
        // SaveID is deprecated but veraPDF still validates T3 for it. (#489)
        "xmpMM:SaveID" => Some(Scalar),
        // Manifest is defined in xmpMM schema but veraPDF treats it as T2 not predefined. (#489)
        "xmpMM:Manifest" => Some(Bag),

        // ── xmpTPg: (Paged-text) ─────────────────────────────────────────────
        "xmpTPg:Colorants" => Some(Seq),
        "xmpTPg:Fonts" => Some(Bag),
        "xmpTPg:MaxPageSize" => Some(Struct), // Dimensions struct, not scalar. (#FN-6.6.2.3.1-t11)
        "xmpTPg:NPages" => Some(Integer),
        "xmpTPg:PlateNames" => Some(Seq),
        "xmpTPg:SwatchGroups" => Some(Seq),

        // ── xmpBJ: (Basic Job Ticket) ────────────────────────────────────────
        "xmpBJ:JobRef" => Some(Bag), // Bag of Job structs. (#FN-6.6.2.3.1-t10)

        // ── xmpDM: (Dynamic Media) ───────────────────────────────────────────
        "xmpDM:absPeakAudioFilePath" => Some(Scalar), // Absolute path URI. (#489)
        "xmpDM:altTimecode" => Some(Struct),          // Timecode structure. (#489)
        "xmpDM:artist" => Some(Scalar),
        "xmpDM:album" => Some(Scalar),
        "xmpDM:altTapeName" => Some(Scalar),
        "xmpDM:audioChannelType" => Some(Scalar),
        "xmpDM:audioCompressor" => Some(Scalar),
        "xmpDM:audioSampleRate" => Some(Integer),
        "xmpDM:audioSampleType" => Some(Scalar),
        "xmpDM:cameraAngle" => Some(Scalar),
        "xmpDM:cameraLabel" => Some(Scalar),
        "xmpDM:cameraModel" => Some(Scalar),
        "xmpDM:cameraMove" => Some(Scalar),
        "xmpDM:client" => Some(Scalar),
        "xmpDM:comment" => Some(Scalar),
        "xmpDM:composer" => Some(Scalar),
        "xmpDM:contributedMedia" => Some(Bag),
        "xmpDM:copyright" => Some(Scalar),
        "xmpDM:director" => Some(Scalar),
        "xmpDM:directorPhotography" => Some(Scalar),
        "xmpDM:discNumber" => Some(Scalar),
        "xmpDM:duration" => Some(Struct),
        "xmpDM:engineer" => Some(Scalar),
        "xmpDM:fileDataRate" => Some(Scalar),
        "xmpDM:genre" => Some(Scalar),
        "xmpDM:good" => Some(Scalar),
        "xmpDM:instrument" => Some(Scalar),
        "xmpDM:introTime" => Some(Struct), // Time struct type (timeValue, scale, etc.), not scalar. (#FN-6.6.2.3.1-t02)
        "xmpDM:key" => Some(Scalar),
        "xmpDM:logComment" => Some(Scalar),
        "xmpDM:loop" => Some(Scalar),
        "xmpDM:markers" => Some(Seq),
        "xmpDM:audioModDate" => Some(Date), // Date type; missing from table → FN t02
        "xmpDM:beatSpliceParams" => Some(Struct), // BeatSpliceStretch structure. (#489)
        "xmpDM:metadataModDate" => Some(Date), // Date type, was Scalar
        "xmpDM:numberOfBeats" => Some(Scalar),
        "xmpDM:outCue" => Some(Struct),
        "xmpDM:partOfCompilation" => Some(Scalar),
        "xmpDM:pick" => Some(Scalar),
        "xmpDM:projectName" => Some(Scalar),
        "xmpDM:projectRef" => Some(Struct), // ProjectRef structure, not scalar. (#489)
        "xmpDM:pullDown" => Some(Scalar),
        "xmpDM:relativePeakAudio" => Some(Scalar),
        "xmpDM:relativeTimestamp" => Some(Struct), // Time structure. (#489)
        "xmpDM:relativePeakAudioFilePath" => Some(Scalar), // URI (scalar); wrong container → violation. (#FN-6.6.2.3.1-t02)
        "xmpDM:relativeTapeOffset" => Some(Scalar),
        "xmpDM:releaseDate" => Some(Date), // Date type, was Scalar
        "xmpDM:resampleParams" => Some(Struct), // ResampleParams structure (#477)
        "xmpDM:resizeType" => Some(Scalar),
        "xmpDM:scaleType" => Some(Scalar),
        "xmpDM:stretchMode" => Some(Scalar), // Open-choice text. (#489)
        "xmpDM:scene" => Some(Scalar),
        "xmpDM:shotDate" => Some(Date),
        "xmpDM:shotDay" => Some(Scalar),
        "xmpDM:shotLocation" => Some(Scalar),
        "xmpDM:shotName" => Some(Scalar),
        "xmpDM:shotNumber" => Some(Scalar),
        "xmpDM:shotSize" => Some(Scalar),
        "xmpDM:speakerPlacement" => Some(Scalar),
        "xmpDM:startTimecode" => Some(Struct),
        "xmpDM:stretch" => Some(Scalar),
        "xmpDM:takeNumber" => Some(Integer),
        "xmpDM:tapeName" => Some(Scalar),
        "xmpDM:tempo" => Some(Scalar),
        "xmpDM:timeScaleParams" => Some(Struct), // TimeScaleParams structure, not scalar. (#489)
        "xmpDM:timeSignature" => Some(Scalar),
        "xmpDM:trackNumber" => Some(Integer),
        "xmpDM:Tracks" => Some(Bag),
        "xmpDM:videoAlphaMode" => Some(Scalar),
        "xmpDM:videoAlphaPremultipleColor" => Some(Scalar),
        "xmpDM:videoAlphaUnityIsTransparent" => Some(Boolean), // Boolean, not scalar. (#489)
        "xmpDM:videoColorSpace" => Some(Scalar),
        "xmpDM:videoCompressor" => Some(Scalar),
        "xmpDM:videoFieldOrder" => Some(Scalar),
        "xmpDM:videoFrameRate" => Some(Scalar),
        "xmpDM:videoFrameSize" => Some(Scalar),
        "xmpDM:videoModDate" => Some(Date), // Date type, was Scalar
        "xmpDM:videoPixelAspectRatio" => Some(Scalar),
        "xmpDM:videoPixelDepth" => Some(Scalar),

        // ── photoshop: ───────────────────────────────────────────────────────
        "photoshop:AncestorID" => Some(Scalar),
        "photoshop:AuthorsPosition" => Some(Scalar),
        "photoshop:CaptionWriter" => Some(Scalar),
        "photoshop:Category" => Some(Scalar),
        "photoshop:City" => Some(Scalar),
        "photoshop:ColorMode" => Some(Integer),
        "photoshop:ColorProfile" => Some(Scalar),
        "photoshop:Country" => Some(Scalar),
        "photoshop:Credit" => Some(Scalar),
        "photoshop:DateCreated" => Some(Scalar),
        "photoshop:DocumentAncestors" => Some(Bag),
        "photoshop:Headline" => Some(Scalar),
        "photoshop:History" => Some(Scalar),
        "photoshop:ICCProfile" => Some(Scalar),
        "photoshop:Instructions" => Some(Scalar),
        "photoshop:LegacyIPTCDigest" => Some(Scalar),
        "photoshop:SidecarForExtension" => Some(Scalar),
        "photoshop:Source" => Some(Scalar),
        "photoshop:State" => Some(Scalar),
        "photoshop:SupplementalCategories" => Some(Bag),
        "photoshop:TextLayers" => Some(Seq),
        "photoshop:TransmissionReference" => Some(Scalar),
        "photoshop:Urgency" => Some(Integer),

        // ── tiff: (EXIF/TIFF) ────────────────────────────────────────────────
        "tiff:Artist" => Some(Scalar),
        "tiff:BitsPerSample" => Some(SeqInteger), // Seq of integer values; "8.0" is invalid. (#489)
        "tiff:CellLength" => Some(Integer),
        "tiff:CellWidth" => Some(Integer),
        "tiff:ColorMap" => Some(Seq),
        "tiff:Compression" => Some(Integer),
        "tiff:Copyright" => Some(LangAlt),
        "tiff:DateTime" => Some(Scalar),
        "tiff:DocumentName" => Some(Scalar),
        "tiff:ExifIFD" => Some(Scalar),
        "tiff:ExtraSamples" => Some(Seq),
        "tiff:FillOrder" => Some(Integer),
        "tiff:FreeByteCounts" => Some(Integer),
        "tiff:FreeOffsets" => Some(Integer),
        "tiff:GrayResponseCurve" => Some(Seq),
        "tiff:GrayResponseUnit" => Some(Integer),
        "tiff:HostComputer" => Some(Scalar),
        "tiff:ImageDescription" => Some(LangAlt),
        "tiff:ImageLength" => Some(Integer),
        "tiff:ImageWidth" => Some(Integer),
        "tiff:InkNames" => Some(Scalar),
        "tiff:InkSet" => Some(Integer),
        "tiff:JPEGInterchangeFormat" => Some(Integer),
        "tiff:JPEGInterchangeFormatLength" => Some(Integer),
        "tiff:JPEGProc" => Some(Integer),
        "tiff:Make" => Some(Scalar),
        "tiff:MaxSampleValue" => Some(Seq),
        "tiff:MinSampleValue" => Some(Seq),
        "tiff:Model" => Some(Scalar),
        "tiff:NewSubfileType" => Some(Integer),
        "tiff:Orientation" => Some(Integer),
        "tiff:PhotometricInterpretation" => Some(Integer),
        "tiff:PlanarConfiguration" => Some(Integer),
        "tiff:PrimaryChromaticities" => Some(Seq),
        "tiff:ReferenceBlackWhite" => Some(Seq),
        "tiff:ResolutionUnit" => Some(Integer),
        "tiff:RowsPerStrip" => Some(Integer),
        "tiff:SamplesPerPixel" => Some(Integer),
        "tiff:SampleFormat" => Some(Seq),
        "tiff:Software" => Some(Scalar),
        "tiff:StripByteCounts" => Some(Seq),
        "tiff:StripOffsets" => Some(Seq),
        "tiff:SubfileType" => Some(Integer),
        "tiff:TransferFunction" => Some(Seq),
        "tiff:TransferRange" => Some(Scalar),
        "tiff:WhitePoint" => Some(Seq),
        "tiff:XResolution" => Some(Scalar),
        "tiff:YCbCrCoefficients" => Some(Seq),
        "tiff:YCbCrPositioning" => Some(Integer),
        "tiff:YCbCrSubSampling" => Some(Seq),
        "tiff:YResolution" => Some(Scalar),

        // ── exif: (EXIF) ─────────────────────────────────────────────────────
        "exif:ApertureValue" => Some(Scalar),
        "exif:BrightnessValue" => Some(Scalar),
        "exif:CFAPattern" => Some(Struct), // OECF/SFR structure, not scalar. (#FN-6.7.9-t16)
        "exif:ColorSpace" => Some(Integer),
        "exif:ComponentsConfiguration" => Some(Seq),
        "exif:CompressedBitsPerPixel" => Some(Scalar),
        "exif:Contrast" => Some(Integer),
        "exif:CustomRendered" => Some(Integer),
        "exif:DateTimeDigitized" => Some(Scalar),
        "exif:DateTimeOriginal" => Some(Date), // ISO 8601 Date per XMP spec. (#FN-6.7.9-t15)
        "exif:DeviceSettingDescription" => Some(Struct), // DeviceSettings struct, not scalar. (#FN-6.6.2.3.1-t17)
        "exif:DigitalZoomRatio" => Some(Scalar),
        "exif:ExifVersion" => Some(Scalar),
        "exif:ExposureBiasValue" => Some(Scalar),
        "exif:ExposureIndex" => Some(Scalar),
        "exif:ExposureMode" => Some(Integer),
        "exif:ExposureProgram" => Some(Integer),
        "exif:ExposureTime" => Some(Scalar),
        "exif:FileSource" => Some(Integer),
        "exif:Flash" => Some(Struct), // Flash structure (Fired/Return/Mode/etc.), not scalar. (#FN-6.7.9-t17)
        "exif:FlashEnergy" => Some(Scalar),
        "exif:FlashpixVersion" => Some(Scalar),
        "exif:FNumber" => Some(Scalar),
        "exif:FocalLength" => Some(Scalar),
        "exif:FocalLengthIn35mmFilm" => Some(Integer),
        "exif:FocalPlaneResolutionUnit" => Some(Integer),
        "exif:FocalPlaneXResolution" => Some(Scalar),
        "exif:FocalPlaneYResolution" => Some(Scalar),
        "exif:GainControl" => Some(Integer),
        "exif:GPSAltitude" => Some(Scalar),
        "exif:GPSAltitudeRef" => Some(Integer),
        "exif:GPSAreaInformation" => Some(Scalar),
        "exif:GPSDestBearing" => Some(Scalar),
        "exif:GPSDestBearingRef" => Some(Scalar),
        "exif:GPSDestDistance" => Some(Scalar),
        "exif:GPSDestDistanceRef" => Some(Scalar),
        "exif:GPSDestLatitude" => Some(Scalar),
        "exif:GPSDestLongitude" => Some(Scalar),
        "exif:GPSDifferential" => Some(Integer),
        "exif:GPSDOP" => Some(Scalar),
        "exif:GPSImgDirection" => Some(Scalar),
        "exif:GPSImgDirectionRef" => Some(Scalar),
        "exif:GPSLatitude" => Some(Scalar),
        "exif:GPSLongitude" => Some(Scalar),
        "exif:GPSMapDatum" => Some(Scalar),
        "exif:GPSMeasureMode" => Some(Integer), // XMP spec: Integer (EXIF tag 10), not Scalar (#484)
        "exif:GPSProcessingMethod" => Some(Scalar),
        "exif:GPSSatellites" => Some(Scalar),
        "exif:GPSSpeed" => Some(Scalar),
        "exif:GPSSpeedRef" => Some(Scalar),
        "exif:GPSStatus" => Some(Scalar),
        "exif:GPSTimeStamp" => Some(Date), // ISO 8601 Date per XMP spec. (#FN-6.7.9-t15)
        "exif:GPSTrack" => Some(Scalar),
        "exif:GPSTrackRef" => Some(Scalar),
        "exif:GPSVersionID" => Some(Scalar),
        "exif:ImageUniqueID" => Some(Scalar),
        "exif:ISOSpeedRatings" => Some(SeqInteger), // Seq of integer values; "1/1" rational is invalid. (#489)
        "exif:InteroperabilityIndex" => Some(Scalar),
        "exif:LightSource" => Some(Integer),
        "exif:MakerNote" => Some(Scalar),
        "exif:MaxApertureValue" => Some(Scalar),
        "exif:MeteringMode" => Some(Integer),
        "exif:OECF" => Some(Struct), // OECF/SFR structure, not plain text. (#489)
        "exif:PixelXDimension" => Some(Integer),
        "exif:PixelYDimension" => Some(Integer),
        "exif:RelatedSoundFile" => Some(Scalar),
        "exif:Saturation" => Some(Integer),
        "exif:SceneCaptureType" => Some(Integer),
        "exif:SceneType" => Some(Integer),
        "exif:SensingMethod" => Some(Integer),
        "exif:Sharpness" => Some(Integer),
        "exif:ShutterSpeedValue" => Some(Scalar),
        "exif:SpatialFrequencyResponse" => Some(Struct), // OECF/SFR struct type, not scalar. (#FN-6.6.2.3.1-t16)
        "exif:SpectralSensitivity" => Some(Scalar),
        "exif:SubjectArea" => Some(Seq),
        "exif:SubjectDistance" => Some(Scalar),
        "exif:SubjectDistanceRange" => Some(Integer),
        "exif:SubjectLocation" => Some(Seq),
        "exif:UserComment" => Some(LangAlt),
        "exif:WhiteBalance" => Some(Integer),

        // ── aux: (Auxiliary EXIF) ────────────────────────────────────────────
        "aux:Lens" => Some(Scalar), // Text type. (#FN-6.6.2.3.1-t19)
        "aux:SerialNumber" => Some(Scalar), // Camera serial number. (#489)

        // ── crs: (Camera Raw Settings) ───────────────────────────────────────
        "crs:AutoBrightness" => Some(CapBoolean), // CRS uses "True"/"False" (capitalized). (#FN-6.6.2.3.1-t04)
        "crs:AutoContrast" => Some(CapBoolean), // CRS uses "True"/"False" (capitalized). (#FN-6.6.2.3.1-t04)
        "crs:AutoExposure" => Some(CapBoolean), // CRS uses "True"/"False" (capitalized). (#FN-6.6.2.3.1-t04)
        "crs:AutoShadows" => Some(CapBoolean), // CRS uses "True"/"False" (capitalized). (#FN-6.6.2.3.1-t04)
        "crs:BlueHue" => Some(Integer),
        "crs:BlueSaturation" => Some(Integer),
        "crs:Brightness" => Some(Integer),
        "crs:CameraProfile" => Some(Scalar),
        "crs:ChromaticAberrationB" => Some(Integer),
        "crs:ChromaticAberrationR" => Some(Integer),
        "crs:ColorNoiseReduction" => Some(Integer),
        "crs:Contrast" => Some(Integer),
        // Real (floating-point) type — reject Rational (slash) notation. (#477)
        "crs:CropTop" => Some(Real),
        "crs:CropLeft" => Some(Real),
        "crs:CropBottom" => Some(Real),
        "crs:CropRight" => Some(Real),
        "crs:CropAngle" => Some(Real),
        "crs:CropWidth" => Some(Real),
        "crs:CropHeight" => Some(Real),
        "crs:CropUnits" => Some(Integer),
        "crs:Exposure" => Some(Real),
        "crs:GreenHue" => Some(Integer),
        "crs:GreenSaturation" => Some(Integer),
        "crs:HasCrop" => Some(CapBoolean), // CRS uses "True"/"False" (capitalized). (#FN-6.6.2.3.1-t04)
        "crs:HasSettings" => Some(CapBoolean), // CRS uses "True"/"False" (capitalized). (#FN-6.6.2.3.1-t04)
        "crs:LuminanceSmoothing" => Some(Integer),
        "crs:RawFileName" => Some(Scalar),
        "crs:RedHue" => Some(Integer),
        "crs:RedSaturation" => Some(Integer),
        "crs:Saturation" => Some(Integer),
        "crs:Shadows" => Some(Integer),
        "crs:ShadowTint" => Some(Integer),
        "crs:Sharpness" => Some(Integer),
        "crs:Temperature" => Some(Integer),
        "crs:Tint" => Some(Integer),
        "crs:ToneCurve" => Some(Seq),
        "crs:ToneCurveName" => Some(Scalar),
        "crs:Version" => Some(Scalar),
        "crs:VignetteAmount" => Some(Integer), // Vignette amount setting. (#489)
        "crs:VignetteMidpoint" => Some(Integer), // Vignette midpoint setting. (#489)
        "crs:Vignetting" => Some(Integer),
        "crs:VignettingMidpoint" => Some(Integer),
        "crs:WhiteBalance" => Some(Scalar),

        // ── pdf: ─────────────────────────────────────────────────────────────
        "pdf:Keywords" => Some(Scalar),
        "pdf:PDFVersion" => Some(Scalar),
        "pdf:Producer" => Some(Scalar),
        "pdf:Trapped" => Some(Scalar),

        _ => None,
    }
}

/// Valid property names in the `pdf:` XMP namespace (ISO 16684 / XMP Specification Part 2,
/// Section 8.4).  Any `pdf:X` property not in this set is non-standard.
const VALID_PDF_PROPERTIES: &[&str] = &[
    "pdf:Keywords",
    "pdf:PDFVersion",
    "pdf:Producer",
    // pdf:Trapped is intentionally OMITTED — veraPDF does not consider it predefined
    // in XMP 2004 and fires §6.7.9 test=2 / §6.6.2.3.1 test=2 for it. (#489)
];

/// Valid property names in the `photoshop:` XMP namespace.
/// Properties outside this set (e.g. photoshop:Copyright, photoshop:Author,
/// photoshop:Title) are not defined in the photoshop schema and fire T2. (#489)
const VALID_PHOTOSHOP_PROPERTIES: &[&str] = &[
    "photoshop:AncestorID",
    "photoshop:AuthorsPosition",
    "photoshop:CaptionWriter",
    "photoshop:Category",
    "photoshop:City",
    "photoshop:ColorMode",
    "photoshop:ColorProfile",
    "photoshop:Country",
    "photoshop:Credit",
    "photoshop:DateCreated",
    "photoshop:DocumentAncestors",
    "photoshop:Headline",
    "photoshop:History",
    "photoshop:ICCProfile",
    "photoshop:Instructions",
    "photoshop:LegacyIPTCDigest",
    "photoshop:SidecarForExtension",
    "photoshop:Source",
    "photoshop:State",
    "photoshop:SupplementalCategories",
    "photoshop:TextLayers",
    "photoshop:TransmissionReference",
    "photoshop:Urgency",
];

/// Valid property names in the `xmpRights:` XMP namespace.
/// xmpRights:Copyright is NOT in this schema — xmpRights:Marked is the boolean marker. (#489)
const VALID_XMPRIGHTS_PROPERTIES: &[&str] = &[
    "xmpRights:Certificate",
    "xmpRights:Marked",
    "xmpRights:Owner",
    "xmpRights:UsageTerms",
    "xmpRights:WebStatement",
];

/// Valid property names in the `pdfaid:` XMP namespace per ISO 19005.
/// pdfaid:rev is NOT valid (not defined in the pdfaid schema). (#489)
const VALID_PDFAID_PROPERTIES: &[&str] = &[
    "pdfaid:part",
    "pdfaid:conformance",
    "pdfaid:amd",
    "pdfaid:corr",
];

/// Properties that are NOT predefined in XMP 2004 per veraPDF's strict internal list.
///
/// Any occurrence of these properties fires §6.7.9 test=2 / §6.6.2.3.1 test=2, unless
/// the property is explicitly declared in a pdfaExtension schema in the current PDF.
/// These properties ARE defined in some version of XMP or an extension schema, but
/// veraPDF's XMP 2004 predefined list excludes them. (#489)
const VERAPDF_NOT_PREDEFINED_PROPS: &[&str] = &[
    // XMP Basic — added after XMP 2004 initial release
    "xmp:Rating",
    "xmp:Label",
    // XMP Paged-Text — not in veraPDF's XMP 2004 predefined list
    "xmpTPg:Fonts",
    "xmpTPg:PlateNames",
    // PDF namespace — pdf:Trapped is Adobe-defined but not XMP 2004 predefined
    "pdf:Trapped",
    // Camera Raw Settings — Adobe Lightroom extension, not XMP 2004 predefined
    "crs:AutoBrightness",
    "crs:AutoContrast",
    "crs:AutoExposure",
    "crs:AutoShadows",
    "crs:VignetteMidpoint",
    "crs:VignetteAmount",
    // Auxiliary EXIF — not in XMP 2004 predefined list
    "aux:Lens",
    "aux:SerialNumber",
    // XMP Dynamic Media — not in veraPDF's XMP 2004 predefined list
    "xmpDM:videoColorSpace",
    "xmpDM:loop",
    "xmpDM:videoAlphaPremultipleColor",
    "xmpDM:videoAlphaUnityIsTransparent",
    "xmpDM:beatSpliceParams",
    "xmpDM:projectRef",
    "xmpDM:altTimecode",
    "xmpDM:absPeakAudioFilePath",
    "xmpDM:tempo",
    "xmpDM:relativeTimestamp",
    "xmpDM:stretchMode",
    "xmpDM:timeScaleParams",
    // XMP Media Management — not in veraPDF's XMP 2004 predefined list
    "xmpMM:Manifest",
];

/// §6.7.9.2 (PDF/A-1) / §6.6.2.3.1 (PDF/A-2/3/4) — Non-standard property in restricted XMP namespace.
///
/// The `pdf:` namespace is a "closed" namespace with exactly four defined properties.
/// Any other `pdf:X` property (e.g. `pdf:ModDate`) is not defined in the Adobe PDF
/// Schema and triggers §6.7.9.2 for PDF/A-1 and §6.6.2.3.1 for PDF/A-2+. (#467, #484)
fn check_pdf_namespace_properties(xmp: &str, level: PdfALevel, report: &mut ComplianceReport) {
    let rule = match level.part() {
        // PDF/A-1: unknown property in predefined schema → §6.7.9.2 (isDefinedInProperty == false).
        // veraPDF maps these to §6.7.9.2, not §6.7.2. (#467, #484)
        1 => "6.7.9.2",
        4 => "6.5.2",
        _ => "6.6.2.3.1",
    };

    // Scan for <pdf:Name or pdf:Name= patterns
    let bytes = xmp.as_bytes();
    let mut pos = 0;
    let mut reported: std::collections::HashSet<String> = std::collections::HashSet::new();

    while pos + 4 < bytes.len() {
        // Look for 'pdf:' preceded by '<' or whitespace
        if &bytes[pos..pos + 4] == b"pdf:" {
            // Check what precedes: must be '<' (element start) or whitespace (attribute)
            let preceded_by_tag_start = pos == 0
                || bytes[pos - 1] == b'<'
                || bytes[pos - 1] == b' '
                || bytes[pos - 1] == b'\t'
                || bytes[pos - 1] == b'\n';
            if preceded_by_tag_start {
                // Extract property name
                let name_start = pos;
                let name_end = xmp[pos..]
                    .find(['>', '/', ' ', '\t', '\n', '=', '\r'])
                    .map(|i| pos + i)
                    .unwrap_or(xmp.len());
                let prop_name = &xmp[name_start..name_end];

                // Skip closing tags, xmlns: declarations, and rdf:container
                if !prop_name.starts_with('/')
                    && !prop_name.is_empty()
                    && !reported.contains(prop_name)
                {
                    // Check if this is a known pdf: property
                    if !VALID_PDF_PROPERTIES.contains(&prop_name) {
                        error(
                            report,
                            rule,
                            format!(
                                "XMP property '{}' is not defined in the predefined pdf: schema",
                                prop_name
                            ),
                        );
                        reported.insert(prop_name.to_string());
                    }
                }
            }
            pos += 4;
        } else {
            pos += 1;
        }
    }
}

/// §6.7.9.2 (PDF/A-1) / §6.6.2.3.1 (PDF/A-2/3/4) — Unknown property in the closed `xmp:` schema.
///
/// The XMP Basic (`xmp:`) namespace is a closed schema with exactly 11 defined properties.
/// Any `xmp:X` property not in that set (e.g. `xmp:Author`, `xmp:Title`) is not defined
/// in the XMP 2004 specification and triggers §6.7.9.2 for PDF/A-1 and §6.6.2.3.1 for PDF/A-2+.
///
/// Both element-form (`<xmp:Author>…</xmp:Author>`) and attribute-form
/// (`xmp:Author="SomeAuthor"`) are scanned.  (#FN-6.7.9-t03)
fn check_xmp_closed_schema_properties(xmp: &str, level: PdfALevel, report: &mut ComplianceReport) {
    let rule = match level.part() {
        1 => "6.7.9.2",
        4 => "6.5.2",
        _ => "6.6.2.3.1",
    };

    let bytes = xmp.as_bytes();
    let mut pos = 0;
    let mut reported: std::collections::HashSet<String> = std::collections::HashSet::new();

    while pos + 4 < bytes.len() {
        // Scan positions preceded by '<' (element start) or space/tab/newline (attribute).
        if bytes[pos] == b'<' || bytes[pos] == b' ' || bytes[pos] == b'\t' || bytes[pos] == b'\n' {
            let start = pos + 1;
            if start + 4 < bytes.len() && &bytes[start..start + 4] == b"xmp:" {
                // Skip closing tags: </xmp:...
                if pos < bytes.len()
                    && bytes[pos] == b'<'
                    && start < bytes.len()
                    && bytes[start] == b'/'
                {
                    pos += 1;
                    continue;
                }
                // Extract property name (xmp:PropName)
                let name_end = xmp[start..]
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-' && c != ':')
                    .map(|i| start + i)
                    .unwrap_or(xmp.len());
                let prop_name = &xmp[start..name_end];

                // Skip xmlns: and closing-tag markers
                if prop_name == "xmp:" || prop_name.contains("xmlns") {
                    pos = name_end;
                    continue;
                }

                if prop_name.starts_with("xmp:")
                    && prop_name.len() > 4
                    && !reported.contains(prop_name)
                    && predefined_prop_kind(prop_name).is_none()
                {
                    error(
                        report,
                        rule,
                        format!(
                            "XMP property '{}' is not defined in the predefined xmp: schema",
                            prop_name
                        ),
                    );
                    reported.insert(prop_name.to_string());
                }
                pos = name_end;
                continue;
            }
        }
        pos += 1;
    }
}

/// §6.7.2 (PDF/A-1) / §6.6.2.3.1 (PDF/A-2/3/4) — Validate value types of predefined XMP properties.
///
/// For every well-known predefined XMP property found in the XMP stream,
/// verify that its value is serialised in the correct RDF/XML form:
/// - Scalar properties (Text, Integer, Real, URI, …) must NOT contain an
///   rdf:Seq / rdf:Bag / rdf:Alt container.
/// - Lang Alt properties (e.g. dc:title, xmpRights:UsageTerms) must use
///   rdf:Alt, and each rdf:li must carry an xml:lang attribute.
/// - Seq / Bag properties must use the correct container type.
/// - Integer properties must additionally have an integer value (no decimal
///   point and no exponent notation).
///
/// This check covers the veraPDF test suite 6-6-2-3-1-tXX-fail cases and
/// the isartor-6-7-2-tXX-fail cases. (#467)
///
/// Note: veraPDF uses §6.7.2 (not §6.7.9) for property type/definition
/// violations in PDF/A-1. §6.7.9 is reserved for undeclared schema namespaces. (#467)
fn check_predefined_property_types(xmp: &str, level: PdfALevel, report: &mut ComplianceReport) {
    let rule = match level.part() {
        // PDF/A-1: property value type violations → §6.7.9.3 (isValueTypeCorrect == false).
        // veraPDF maps these to §6.7.9.3, not §6.7.2. (#467, #484)
        1 => "6.7.9.3",
        4 => "6.5.2",
        _ => "6.6.2.3.1",
    };

    // We need to find all occurrences of known properties and inspect their content.
    // Strategy: scan for "<prefix:name" patterns, extract the element body, then
    // check whether it contains an rdf:container or a plain value.

    // Build a lookup set of all properties we care about.  This avoids a full
    // XMP parse — we do a targeted scan matching "qualified-name>" or
    // "qualified-name " followed by an attribute.
    let mut reported: std::collections::HashSet<&str> = std::collections::HashSet::new();

    // Look for element-form properties: <prefix:name>...</prefix:name>
    // We scan for "<" + known-property-name, then extract the element body.
    // To avoid false matches inside attribute values we only look at positions
    // where bytes[pos-1] == b'<' or is whitespace (start of element).
    let bytes = xmp.as_bytes();
    let mut pos = 0;

    while pos < bytes.len().saturating_sub(5) {
        // Fast-path: only start checking at '<'
        if bytes[pos] != b'<' {
            pos += 1;
            continue;
        }
        // Skip '</' closing tags
        if pos + 1 < bytes.len() && bytes[pos + 1] == b'/' {
            pos += 1;
            continue;
        }
        // Extract the tag name (up to '>', '/' or space)
        let name_start = pos + 1;
        let name_end = xmp[name_start..]
            .find(['>', '/', ' ', '\t', '\n', '\r'])
            .map(|i| name_start + i)
            .unwrap_or(xmp.len());
        let tag_name = &xmp[name_start..name_end];

        // Only process known predefined properties
        if let Some(kind) = predefined_prop_kind(tag_name) {
            // Properties in VERAPDF_NOT_PREDEFINED_PROPS are handled by
            // check_not_predefined_properties (T2), not here (T3). (#489)
            if VERAPDF_NOT_PREDEFINED_PROPS.contains(&tag_name) {
                pos = name_end.max(pos + 1);
                continue;
            }
            if !reported.contains(tag_name) {
                // Find the element body between '>' and '</tag_name>'
                let close_start = match xmp[name_end..].find('>') {
                    Some(i) => name_end + i + 1,
                    None => {
                        pos = name_end;
                        continue;
                    }
                };
                let close_tag = format!("</{}>", tag_name);
                let body_end = match xmp[close_start..].find(&close_tag) {
                    Some(i) => close_start + i,
                    None => {
                        pos = close_start;
                        continue;
                    }
                };
                let body = &xmp[close_start..body_end];

                // Check for container violations
                let has_seq = body.contains("<rdf:Seq") || body.contains("<rdf:seq");
                let has_bag = body.contains("<rdf:Bag") || body.contains("<rdf:bag");
                let has_alt = body.contains("<rdf:Alt") || body.contains("<rdf:alt");
                let has_container = has_seq || has_bag || has_alt;

                let violation = match kind {
                    PropValueKind::Scalar | PropValueKind::Real | PropValueKind::Integer => {
                        if has_container {
                            Some(format!(
                                "XMP property '{}' is a scalar type but is wrapped in an rdf container",
                                tag_name
                            ))
                        } else if kind == PropValueKind::Real {
                            // Real value must not use rational (slash) notation. (#477)
                            let val = body.trim();
                            if !val.is_empty() && !val.starts_with('<') && val.contains('/') {
                                Some(format!(
                                    "XMP property '{}' requires a Real value but got '{}' \
                                     (Rational notation not allowed)",
                                    tag_name,
                                    &val[..val.len().min(40)]
                                ))
                            } else {
                                None
                            }
                        } else if kind == PropValueKind::Integer {
                            // Integer value must not have decimal point, exponent, or
                            // non-digit characters. (#477)
                            let val = body.trim();
                            if !val.is_empty() && !val.starts_with('<') && is_non_integer_value(val)
                            {
                                Some(format!(
                                    "XMP property '{}' requires an Integer value but got '{}'",
                                    tag_name,
                                    &val[..val.len().min(40)]
                                ))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    PropValueKind::LangAlt => {
                        if !has_alt {
                            // Lang Alt must use rdf:Alt
                            if has_seq || has_bag {
                                Some(format!(
                                    "XMP property '{}' requires 'Lang Alt' (rdf:Alt) but uses wrong container",
                                    tag_name
                                ))
                            } else if !body.trim().is_empty() && !body.trim().starts_with('<') {
                                // Plain text where Lang Alt required
                                Some(format!(
                                    "XMP property '{}' requires 'Lang Alt' (rdf:Alt) but is plain text",
                                    tag_name
                                ))
                            } else {
                                None
                            }
                        } else {
                            // Has rdf:Alt — check that rdf:li elements have xml:lang
                            // Find first rdf:li without xml:lang
                            if has_rdf_li_without_lang(body) {
                                Some(format!(
                                    "XMP property '{}' has rdf:Alt but rdf:li is missing required xml:lang attribute",
                                    tag_name
                                ))
                            } else {
                                None
                            }
                        }
                    }
                    PropValueKind::Seq => {
                        if has_bag || has_alt {
                            Some(format!(
                                "XMP property '{}' requires 'seq' (rdf:Seq) but uses rdf:Bag or rdf:Alt",
                                tag_name
                            ))
                        } else if !has_seq
                            && !body.trim().is_empty()
                            && !body.trim().starts_with('<')
                        {
                            // Plain text where Seq required (e.g. dc:creator "text", exif:ComponentsConfiguration "1.0 2.0")
                            Some(format!(
                                "XMP property '{}' requires 'seq' (rdf:Seq) but is plain text",
                                tag_name
                            ))
                        } else {
                            None
                        }
                    }
                    PropValueKind::Bag => {
                        if has_seq || has_alt {
                            Some(format!(
                                "XMP property '{}' requires 'bag' (rdf:Bag) but uses rdf:Seq or rdf:Alt",
                                tag_name
                            ))
                        } else if !has_bag
                            && !body.trim().is_empty()
                            && !body.trim().starts_with('<')
                        {
                            // Plain text where Bag required (e.g. xmpRights:Owner "Some owner")
                            Some(format!(
                                "XMP property '{}' requires 'bag' (rdf:Bag) but is plain text",
                                tag_name
                            ))
                        } else {
                            None
                        }
                    }
                    PropValueKind::Date => {
                        // Date properties must not be wrapped in an rdf container and
                        // the plain-text value must be a valid ISO 8601 date string.
                        // Catches values like "Date: 2016-02-01T13:19:21+01:00" that have
                        // an invalid prefix. (#FN-6.6.2.3.1-t01)
                        if has_container {
                            Some(format!(
                                "XMP property '{}' is a scalar date type but is wrapped in an rdf container",
                                tag_name
                            ))
                        } else {
                            let val = body.trim();
                            if !val.is_empty() && !val.starts_with('<') && !is_valid_iso8601(val) {
                                Some(format!(
                                    "XMP property '{}' value '{}' is not valid ISO 8601 format",
                                    tag_name,
                                    &val[..val.len().min(60)]
                                ))
                            } else {
                                None
                            }
                        }
                    }
                    PropValueKind::Boolean => {
                        // Boolean properties must not be wrapped in an rdf container and
                        // the value must be exactly "true" or "false" (XMP spec §8.2.1,
                        // case-sensitive). "TRUE", "False", "1", "0" etc. are invalid.
                        // (#FN-6.6.2.3.1-t08)
                        if has_container {
                            Some(format!(
                                "XMP property '{}' is a boolean type but is wrapped in an rdf container",
                                tag_name
                            ))
                        } else {
                            let val = body.trim();
                            if !val.is_empty()
                                && !val.starts_with('<')
                                && val != "true"
                                && val != "false"
                            {
                                Some(format!(
                                    "XMP property '{}' has invalid Boolean value '{}' (must be 'true' or 'false')",
                                    tag_name,
                                    &val[..val.len().min(40)]
                                ))
                            } else {
                                None
                            }
                        }
                    }
                    PropValueKind::CapBoolean => {
                        // Capitalized Boolean — Adobe CRS schema uses "True"/"False" (capital first
                        // letter) rather than the XMP spec's "true"/"false". Lowercase values are
                        // invalid for CRS boolean properties. (#FN-6.6.2.3.1-t04)
                        if has_container {
                            Some(format!(
                                "XMP property '{}' is a boolean type but is wrapped in an rdf container",
                                tag_name
                            ))
                        } else {
                            let val = body.trim();
                            if !val.is_empty()
                                && !val.starts_with('<')
                                && val != "True"
                                && val != "False"
                            {
                                Some(format!(
                                    "XMP property '{}' has invalid Boolean value '{}' (must be 'True' or 'False')",
                                    tag_name,
                                    &val[..val.len().min(40)]
                                ))
                            } else {
                                None
                            }
                        }
                    }
                    PropValueKind::Struct => {
                        // Struct types must be serialised as an RDF resource (with sub-elements),
                        // not as a plain text value or wrapped in an rdf:Seq/Bag/Alt container.
                        // A valid struct uses rdf:parseType="Resource" or contains child
                        // namespace-prefixed elements. (#FN-6.7.9-t09/t16/t17)
                        let trimmed = body.trim();
                        let is_plain_text = !trimmed.is_empty()
                            && !trimmed.starts_with('<')
                            && !body.contains("rdf:parseType");
                        if is_plain_text {
                            Some(format!(
                                "XMP property '{}' is a structure type but contains plain text value",
                                tag_name
                            ))
                        } else if has_container {
                            Some(format!(
                                "XMP property '{}' is a structure type but is wrapped in an rdf container",
                                tag_name
                            ))
                        } else {
                            None
                        }
                    }
                    PropValueKind::SeqInteger => {
                        // Ordered array of integer items — must use rdf:Seq container, and each
                        // rdf:li must be a valid integer (no decimal point, no rational).
                        // Catches tiff:BitsPerSample "8.0" and exif:ISOSpeedRatings "1/1". (#489)
                        if has_bag || has_alt {
                            Some(format!(
                                "XMP property '{}' requires 'seq' (rdf:Seq) but uses wrong container",
                                tag_name
                            ))
                        } else if !has_seq
                            && !body.trim().is_empty()
                            && !body.trim().starts_with('<')
                        {
                            Some(format!(
                                "XMP property '{}' requires 'seq' (rdf:Seq) but is plain text",
                                tag_name
                            ))
                        } else if has_seq {
                            check_seq_integer_items(body, tag_name)
                        } else {
                            None
                        }
                    }
                };

                if let Some(msg) = violation {
                    error(report, rule, msg);
                    reported.insert(tag_name);
                }
            }
        }

        pos = name_end.max(pos + 1);
    }
}

/// Check whether a text value is a non-integer (decimal / rational / non-numeric).
///
/// Returns `true` when the value is not a valid XMP Integer (optional sign
/// followed by ASCII digits only). Catches decimal, rational, scientific and
/// arbitrary non-numeric strings like "Pos - 1". Fixes §6.6.2.3.1 t14. (#477)
fn is_non_integer_value(val: &str) -> bool {
    let val = val.trim();
    if val.is_empty() {
        return false;
    }
    // Strip optional leading sign
    let digits = if val.starts_with('+') || val.starts_with('-') {
        &val[1..]
    } else {
        val
    };
    // A valid XMP Integer consists solely of ASCII digits after the sign
    digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit())
}

/// Check that all rdf:li items in a Seq body are valid integer values.
///
/// Returns an error message if any rdf:li contains a non-integer value
/// (decimal "8.0", rational "1/1", or arbitrary text). (#489)
fn check_seq_integer_items(body: &str, prop_name: &str) -> Option<String> {
    let mut search = 0;
    while let Some(li_pos) = body[search..].find("<rdf:li") {
        let abs = search + li_pos;
        let tag_end = body[abs..]
            .find('>')
            .map(|i| abs + i + 1)
            .unwrap_or(abs + 7);
        let li_close = "</rdf:li>";
        let val = if let Some(close) = body[tag_end..].find(li_close) {
            body[tag_end..tag_end + close].trim()
        } else {
            ""
        };
        if !val.is_empty() && !val.starts_with('<') && is_non_integer_value(val) {
            return Some(format!(
                "XMP property '{}' contains non-integer item '{}' in rdf:Seq (must be integer)",
                prop_name,
                &val[..val.len().min(40)]
            ));
        }
        search = tag_end;
    }
    None
}

/// §6.7.9.2 (PDF/A-1) / §6.6.2.3.1 (PDF/A-2/3/4) — Non-standard property in a closed XMP namespace.
///
/// For namespaces with a fixed set of defined properties (photoshop:, xmpRights:, pdfaid:),
/// any property not in the known set fires T2. Extension schema declarations in the current
/// PDF package override this check. (#489)
fn check_closed_namespace_properties(
    xmp: &str,
    schemas: &[ExtensionSchema],
    prefix: &str,
    valid_props: &[&str],
    level: PdfALevel,
    report: &mut ComplianceReport,
) {
    let rule = match level.part() {
        1 => "6.7.9.2",
        4 => "6.5.2",
        _ => "6.6.2.3.1",
    };

    let prefix_bytes = prefix.as_bytes();
    let prefix_len = prefix_bytes.len();

    // Collect property names declared in extension schemas for this prefix.
    let ext_local_names: HashSet<String> = schemas
        .iter()
        .filter(|s| format!("{}:", s.prefix) == prefix)
        .flat_map(|s| s.properties.iter().map(|p| p.name.clone()))
        .collect();

    let bytes = xmp.as_bytes();
    let mut reported: HashSet<String> = HashSet::new();
    let mut pos = 0;

    while pos + prefix_len < bytes.len() {
        if &bytes[pos..pos + prefix_len] == prefix_bytes {
            let preceded = pos == 0
                || bytes[pos - 1] == b'<'
                || bytes[pos - 1] == b' '
                || bytes[pos - 1] == b'\t'
                || bytes[pos - 1] == b'\n';
            if preceded {
                let name_start = pos;
                let name_end = xmp[pos + prefix_len..]
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                    .map(|i| pos + prefix_len + i)
                    .unwrap_or(xmp.len());
                let prop_name = &xmp[name_start..name_end];
                let local_name = &prop_name[prefix_len..];

                // Skip closing tags, empty names, and xmlns: declarations.
                if !local_name.is_empty()
                    && !local_name.starts_with('/')
                    && !prop_name.contains("xmlns")
                    && !reported.contains(prop_name)
                    && !valid_props.contains(&prop_name)
                    && !ext_local_names.contains(local_name)
                {
                    error(
                        report,
                        rule,
                        format!(
                            "XMP property '{}' is not defined in the predefined {} schema",
                            prop_name, prefix
                        ),
                    );
                    reported.insert(prop_name.to_string());
                }
            }
            pos += prefix_len;
        } else {
            pos += 1;
        }
    }
}

/// §6.7.9.2 (PDF/A-1) / §6.6.2.3.1 (PDF/A-2/3/4) — Properties not predefined in XMP 2004.
///
/// Scans for properties in `VERAPDF_NOT_PREDEFINED_PROPS`. Per veraPDF's strict XMP 2004
/// internal list, these properties are not predefined and fire T2 unless declared in a
/// pdfaExtension schema in the current PDF. (#489)
fn check_not_predefined_properties(
    xmp: &str,
    schemas: &[ExtensionSchema],
    level: PdfALevel,
    report: &mut ComplianceReport,
) {
    let rule = match level.part() {
        1 => "6.7.9.2",
        4 => "6.5.2",
        _ => "6.6.2.3.1",
    };

    // Build full qualified property names declared in extension schemas.
    let ext_props: HashSet<String> = schemas
        .iter()
        .flat_map(|s| {
            s.properties
                .iter()
                .map(move |p| format!("{}:{}", s.prefix, p.name))
        })
        .collect();

    for &prop in VERAPDF_NOT_PREDEFINED_PROPS {
        // Skip if declared in extension schemas of the current PDF.
        if ext_props.contains(prop) {
            continue;
        }
        // Scan for element-form or attribute-form usage of the property.
        let has_prop = xmp.contains(&format!("<{}>", prop))
            || xmp.contains(&format!("<{} ", prop))
            || xmp.contains(&format!("<{}/", prop))
            || xmp.contains(&format!(" {}=", prop))
            || xmp.contains(&format!("\t{}=", prop))
            || xmp.contains(&format!("\n{}=", prop));
        if has_prop {
            error(
                report,
                rule,
                format!(
                    "XMP property '{}' is not predefined in XMP 2004 and not declared in an extension schema",
                    prop
                ),
            );
        }
    }
}

/// Check whether a body containing rdf:Alt has at least one rdf:li without xml:lang.
fn has_rdf_li_without_lang(body: &str) -> bool {
    let mut search = 0;
    while let Some(li_pos) = body[search..].find("<rdf:li") {
        let abs = search + li_pos;
        // Find end of the opening tag
        let tag_end = match body[abs..].find('>') {
            Some(i) => abs + i,
            None => break,
        };
        let open_tag = &body[abs..=tag_end];
        if !open_tag.contains("xml:lang") {
            return true;
        }
        search = tag_end + 1;
    }
    false
}

/// §6.7.11 — pdfaid identification properties must use the 'pdfaid' prefix.
///
/// veraPDF tests 4 and 5: if the PDF/A Identification Schema namespace
/// (http://www.aiim.org/pdfa/ns/id/) is bound to any prefix other than
/// 'pdfaid', the part and conformance properties are not accessible and
/// the document is non-conformant. (#467)
fn check_pdfaid_prefix(xmp: &str, report: &mut ComplianceReport) {
    const PDFAID_NS: &str = "http://www.aiim.org/pdfa/ns/id/";

    // Find xmlns: declarations that bind the pdfaid namespace to a prefix
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
                    // Extract the namespace URI value
                    let after_eq = &xmp[abs + eq + 1..];
                    let uri = if let Some(s) = after_eq.strip_prefix('"') {
                        s.split('"').next()
                    } else if let Some(s) = after_eq.strip_prefix('\'') {
                        s.split('\'').next()
                    } else {
                        None
                    };
                    if let Some(uri) = uri {
                        if uri == PDFAID_NS && prefix_name != "pdfaid" {
                            error(
                                report,
                                "6.7.11",
                                format!(
                                    "PDF/A Identification Schema bound to prefix '{}' instead of required 'pdfaid'",
                                    prefix_name
                                ),
                            );
                            return; // report once
                        }
                    }
                }
            }
        }
        search = abs;
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

// ============================================================================
// Structural checks (PDF catalog / document structure, not XMP content)
// ============================================================================

/// §6.9 (PDF/A-2/3) / §6.10 (PDF/A-4): OCProperties config dicts must not have /AS.
///
/// Neither the default (/D) nor any alternate (/Configs) OCG configuration dict
/// may contain an /AS entry in PDF/A-2/3. veraPDF reports this as §6.9. (#FN-6.9)
fn check_oc_d_as_restriction(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    if level.part() < 2 {
        return;
    }
    let Some(cat) = check::catalog(pdf) else {
        return;
    };
    let Some(ocprops) = cat.get::<Dict<'_>>(keys::OCPROPERTIES) else {
        return;
    };
    let rule = if level.part() == 4 { "6.10" } else { "6.9" };
    // Check default config dict /D.
    if let Some(d_dict) = ocprops.get::<Dict<'_>>(b"D" as &[u8]) {
        if d_dict.contains_key(b"AS" as &[u8]) {
            error(
                report,
                rule,
                "OCProperties default config (/D) must not have /AS entry",
            );
        }
    }
    // Check each alternate config in /Configs array — same /AS restriction applies.
    if let Some(configs) = ocprops.get::<Array<'_>>(b"Configs" as &[u8]) {
        for (idx, cfg) in configs.iter::<Dict<'_>>().enumerate() {
            if cfg.contains_key(b"AS" as &[u8]) {
                error(
                    report,
                    rule,
                    format!(
                        "OCProperties alternate config {} must not have /AS entry",
                        idx
                    ),
                );
            }
        }
    }
}

/// §6.10 (PDF/A-2/3) / §6.10 (PDF/A-4): OCG Order array must include all OCGs.
///
/// If OCProperties/D/Order is present, every OCG must be referenced in it.
/// veraPDF reports missing OCGs in Order as §6.10 for PDF/A-2/3. Fixes #FN-6.10.
fn check_ocg_order_completeness(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    if level.part() < 2 {
        return;
    }
    let Some(cat) = check::catalog(pdf) else {
        return;
    };
    let Some(ocprops) = cat.get::<Dict<'_>>(keys::OCPROPERTIES) else {
        return;
    };
    let Some(d_dict) = ocprops.get::<Dict<'_>>(b"D" as &[u8]) else {
        return;
    };
    let Some(order_arr) = d_dict.get::<Array<'_>>(b"Order" as &[u8]) else {
        return; // No Order array — not a violation by itself
    };

    // Collect all OCG object IDs declared in OCProperties/OCGs.
    let mut all_ocgs: HashSet<ObjRef> = HashSet::new();
    if let Some(ocgs_arr) = ocprops.get::<Array<'_>>(b"OCGs" as &[u8]) {
        for item in ocgs_arr.raw_iter() {
            if let Some(r) = item.as_obj_ref() {
                all_ocgs.insert(r);
            }
        }
    }
    if all_ocgs.is_empty() {
        return;
    }

    // Collect all OCG refs referenced in Order (recursively, Order may contain arrays).
    let mut order_ocgs: HashSet<ObjRef> = HashSet::new();
    collect_order_refs(&order_arr, &mut order_ocgs);

    let rule = if level.part() == 4 { "6.10" } else { "6.6.4" };
    for ocg_ref in &all_ocgs {
        if !order_ocgs.contains(ocg_ref) {
            error(
                report,
                rule,
                format!(
                    "OCG {} not referenced in OCProperties/D/Order",
                    ocg_ref.obj_number
                ),
            );
        }
    }
}

/// Recursively collect ObjRef entries from a potentially nested Order array.
fn collect_order_refs(arr: &Array<'_>, out: &mut HashSet<ObjRef>) {
    for item in arr.raw_iter() {
        if let Some(r) = item.as_obj_ref() {
            out.insert(r);
        } else if let pdf_syntax::object::MaybeRef::NotRef(Object::Array(nested)) = item {
            collect_order_refs(&nested, out);
        }
    }
}

/// §6.10 (PDF/A-2/3) / §6.11 (PDF/A-4): Names/AlternatePresentations is forbidden.
///
/// The document Names dictionary must not contain an AlternatePresentations
/// entry. Also checks for /PresSteps in page dictionaries (§6.10 T2). (#FN-6.10)
fn check_alternate_presentations_absent(
    pdf: &Pdf,
    level: PdfALevel,
    report: &mut ComplianceReport,
) {
    if level.part() < 2 {
        return;
    }
    // In ISO 19005-2/3, §6.10 forbids AlternatePresentations; in PDF/A-4 it is §6.11.
    let rule = if level.part() <= 3 { "6.10" } else { "6.11" };
    let Some(cat) = check::catalog(pdf) else {
        return;
    };
    if let Some(names) = cat.get::<Dict<'_>>(keys::NAMES) {
        if names.contains_key(b"AlternatePresentations" as &[u8]) {
            error(
                report,
                rule,
                "Names dictionary must not contain AlternatePresentations",
            );
        }
    }
    // §6.10 T2 (PDF/A-2/3): page dictionaries must not contain /PresSteps.
    if level.part() <= 3 {
        for (page_idx, page) in pdf.pages().iter().enumerate() {
            if page.raw().contains_key(b"PresSteps" as &[u8]) {
                error(
                    report,
                    rule,
                    format!("Page {} has forbidden /PresSteps entry", page_idx + 1),
                );
            }
        }
    }
}

/// §6.12 (PDF/A-2/3/4): /Requirements key in catalog is forbidden.
///
/// The document catalog must not have a /Requirements entry. Fixes #FN-6.12.
fn check_requirements_absent(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    if level.part() < 2 {
        return;
    }
    let Some(cat) = check::catalog(pdf) else {
        return;
    };
    if cat.contains_key(b"Requirements" as &[u8]) {
        error(
            report,
            "6.12",
            "Document catalog must not contain /Requirements entry (§6.12)",
        );
    }
}

/// §6.6.2 (PDF/A-2/3): Widget annotations and AcroForm fields must not have /AA.
///
/// check.rs emits "6.4.1" for Widget /AA which veraPDF uses for PDF/A-1.
/// For PDF/A-2/3 veraPDF uses "6.6.2". Fixes #FN-6.6.2.
fn check_widget_aa_pdfa23(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    // Only for PDF/A-2/3: part 1 uses 6.4.1 (handled elsewhere), part 4 uses different rule.
    if level.part() != 2 && level.part() != 3 {
        return;
    }
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(annots) = page_dict.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>() {
            let subtype = annot.get::<Name>(keys::SUBTYPE);
            let is_widget = subtype.as_ref().is_some_and(|s| s.as_ref() == b"Widget");
            if !is_widget {
                continue;
            }
            if annot.contains_key(b"AA" as &[u8]) {
                error(
                    report,
                    "6.6.2",
                    format!(
                        "Widget annotation on page {} has forbidden /AA entry (§6.6.2)",
                        page_idx + 1
                    ),
                );
            }
        }
    }
    // Also check AcroForm field tree for /AA on field nodes.
    let Some(cat) = check::catalog(pdf) else {
        return;
    };
    let Some(acroform) = cat.get::<Dict<'_>>(keys::ACRO_FORM) else {
        return;
    };
    let Some(fields) = acroform.get::<Array<'_>>(keys::FIELDS) else {
        return;
    };
    check_field_aa_recursive(&fields, report);
}

/// Recursively check AcroForm field nodes for forbidden /AA entries (§6.6.2).
fn check_field_aa_recursive(fields: &Array<'_>, report: &mut ComplianceReport) {
    for field in fields.iter::<Dict<'_>>() {
        if field.contains_key(b"AA" as &[u8]) {
            // Only report on field nodes (those with /FT or /T), not widget-only annots.
            let has_ft = field.contains_key(b"FT" as &[u8]);
            let has_t = field.contains_key(b"T" as &[u8]);
            if has_ft || has_t {
                error(
                    report,
                    "6.6.2",
                    "AcroForm field node has forbidden /AA entry (§6.6.2)",
                );
            }
        }
        if let Some(kids) = field.get::<Array<'_>>(keys::KIDS) {
            check_field_aa_recursive(&kids, report);
        }
    }
}

/// §6.7.2.2 (PDF/A-2/3): MarkInfo/Marked must be true for conformance level A.
///
/// check_tagged_requirements in pdfa.rs emits "6.8" but veraPDF uses "6.7.2.2".
/// Fixes #FN-6.7.2.2.
fn check_mark_info_required(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    // Only applies for tagged (conformance A) PDF/A-2/3.
    if !level.requires_tagged() || (level.part() != 2 && level.part() != 3) {
        return;
    }
    let Some(cat) = check::catalog(pdf) else {
        return;
    };
    let mark_info = cat.get::<Dict<'_>>(keys::MARK_INFO);
    let marked = mark_info
        .as_ref()
        .and_then(|d| d.get::<bool>(b"Marked" as &[u8]))
        .unwrap_or(false);
    if !marked {
        error(
            report,
            "6.7.2.2",
            "MarkInfo/Marked must be true for PDF/A tagged conformance (§6.7.2.2)",
        );
    }
}

/// §6.7.3.3 (PDF/A-2/3/4-A): StructTreeRoot is required for tagged conformance.
///
/// check_tagged_requirements emits "6.8" but veraPDF uses "6.7.3.3".
/// Fixes #FN-6.7.3.3.
fn check_struct_tree_root_required(pdf: &Pdf, level: PdfALevel, report: &mut ComplianceReport) {
    if !level.requires_tagged() {
        return;
    }
    if check::struct_tree_root(pdf).is_none() {
        error(
            report,
            "6.7.3.3",
            "StructTreeRoot is required for PDF/A tagged conformance (§6.7.3.3)",
        );
    }
}

/// §6.7.3.4: RoleMap must not contain circular mappings.
///
/// Cycles in the RoleMap prevent role resolution and are a §6.7.3.4 violation.
/// Fixes #FN-6.7.3.4.
fn check_role_map_no_cycles(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(cat) = check::catalog(pdf) else {
        return;
    };
    let Some(str_root) = cat.get::<Dict<'_>>(keys::STRUCT_TREE_ROOT) else {
        return;
    };
    let Some(role_map) = str_root.get::<Dict<'_>>(keys::ROLE_MAP) else {
        return;
    };

    // Build a name→name mapping from the RoleMap.
    let mut map: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    for (key, val_ref) in role_map.entries() {
        if let pdf_syntax::object::MaybeRef::NotRef(Object::Name(target)) = val_ref {
            map.insert(key.as_ref().to_vec(), target.as_ref().to_vec());
        }
    }

    // DFS cycle detection: for each key, walk the chain and check for repetition.
    for start in map.keys() {
        let mut visited: HashSet<Vec<u8>> = HashSet::new();
        let mut current = start.clone();
        loop {
            if !visited.insert(current.clone()) {
                error(
                    report,
                    "6.7.3.4",
                    format!(
                        "RoleMap contains a cycle involving role '{}'",
                        String::from_utf8_lossy(&current)
                    ),
                );
                break;
            }
            match map.get(&current) {
                Some(next) => current = next.clone(),
                None => break,
            }
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
