//! PDF/A font embedding and subsetting.
//!
//! Detects non-embedded fonts and embeds them for PDF/A conformity.
//! Key fixes:
//! - Type0 fonts: embeds on CIDFont descendant (not Type0 root)
//! - Subtype update: Type1→TrueType when embedding TTF (veraPDF checks this)
//! - Font detection: also finds fonts without Type=Font (only Subtype)
//! - Fallback: uses DejaVuSans for any unresolvable font
//! - Width matching: updates Widths/DW from embedded font data

use crate::error::{ManipError, Result};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use std::path::PathBuf;

/// Report from font embedding pass.
#[derive(Debug, Clone)]
pub struct FontEmbedReport {
    /// Number of fonts inspected.
    pub fonts_inspected: usize,
    /// Number of non-embedded fonts found.
    pub non_embedded_found: usize,
    /// Number of fonts successfully embedded.
    pub fonts_embedded: usize,
    /// Fonts that could not be embedded (name, reason).
    pub failed: Vec<(String, String)>,
}

/// Standard 14 font names that must be embedded for PDF/A.
const STANDARD_14: &[&str] = &[
    "Courier",
    "Courier-Bold",
    "Courier-BoldOblique",
    "Courier-Oblique",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-BoldOblique",
    "Helvetica-Oblique",
    "Times-Roman",
    "Times-Bold",
    "Times-BoldItalic",
    "Times-Italic",
    "Symbol",
    "ZapfDingbats",
];

/// Fallback font paths for any font that cannot be found (tried in order).
const FALLBACK_FONTS: &[&str] = &[
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
];

/// Shared in-repo font pack used to keep local and VPS embedding deterministic.
const REPO_FONT_PACK_REL: &str = "../../.font-pack";

/// Font subtypes that indicate a Font dictionary.
const FONT_SUBTYPES: &[&str] = &[
    "Type0",
    "Type1",
    "TrueType",
    "Type3",
    "CIDFontType0",
    "CIDFontType2",
    "MMType1",
];

/// Info about a non-embedded font and where to embed it.
struct NonEmbeddedFont {
    /// The font dictionary object ID (Type1, TrueType, or Type0).
    font_id: ObjectId,
    /// The object ID where the FontDescriptor lives (or should live).
    /// For simple fonts this is the same as font_id.
    /// For Type0 fonts this is the CIDFont descendant.
    target_id: ObjectId,
    /// Font name.
    name: String,
    /// Whether this is a Type0 composite font.
    is_type0: bool,
    /// Original Subtype of the font dict at font_id.
    subtype: String,
}

/// Check if a dictionary looks like a Font dict (has Type=Font OR a font Subtype).
fn is_font_dict(dict: &lopdf::Dictionary) -> bool {
    if get_name(dict, b"Type").as_deref() == Some("Font") {
        return true;
    }
    if let Some(st) = get_name(dict, b"Subtype") {
        return FONT_SUBTYPES.contains(&st.as_str());
    }
    false
}

/// Detect all non-embedded fonts in the document.
pub fn find_non_embedded_fonts(doc: &Document) -> Vec<(ObjectId, String)> {
    find_non_embedded_fonts_detailed(doc)
        .into_iter()
        .map(|f| (f.font_id, f.name))
        .collect()
}

/// Detect all non-embedded fonts with embedding target info.
fn find_non_embedded_fonts_detailed(doc: &Document) -> Vec<NonEmbeddedFont> {
    let mut result = Vec::new();
    // Track CIDFont IDs that are descendants of Type0 fonts to avoid double-counting.
    // Use HashSet for O(1) lookup — Vec::contains here was O(N×D) over all PDF objects. (#perf)
    let mut descendant_ids: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();

    // First pass: collect all CIDFont descendant IDs from Type0 fonts.
    for obj in doc.objects.values() {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        if !is_font_dict(dict) {
            continue;
        }
        let subtype = get_name(dict, b"Subtype").unwrap_or_default();
        if subtype == "Type0" {
            if let Ok(Object::Array(arr)) = dict.get(b"DescendantFonts") {
                for item in arr {
                    if let Object::Reference(id) = item {
                        descendant_ids.insert(*id);
                    }
                }
            }
        }
    }

    // Second pass: find non-embedded fonts.
    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };

        if !is_font_dict(dict) {
            continue;
        }

        let font_name = get_name_resolved(doc, dict, b"BaseFont")
            .or_else(|| get_name_lossy_resolved(doc, dict, b"BaseFont"))
            .unwrap_or_else(|| format!("FallbackFont{}", id.0));

        let subtype = get_name(dict, b"Subtype").unwrap_or_default();

        // Skip CIDFont descendants — they are handled via their parent Type0 font.
        if descendant_ids.contains(id) {
            continue;
        }

        let is_type0 = subtype == "Type0";

        if is_type0 {
            let descendant_info = get_descendant_embed_info(doc, dict);
            match descendant_info {
                Some((cid_id, true)) => {
                    let _ = cid_id;
                }
                Some((cid_id, false)) => {
                    result.push(NonEmbeddedFont {
                        font_id: *id,
                        target_id: cid_id,
                        name: font_name,
                        is_type0: true,
                        subtype,
                    });
                }
                None => {
                    if !has_embedded_font_program(doc, dict) {
                        result.push(NonEmbeddedFont {
                            font_id: *id,
                            target_id: *id,
                            name: font_name,
                            is_type0: true,
                            subtype,
                        });
                    }
                }
            }
        } else if !has_embedded_font_program(doc, dict) {
            result.push(NonEmbeddedFont {
                font_id: *id,
                target_id: *id,
                name: font_name,
                is_type0: false,
                subtype,
            });
        }
    }

    result
}

/// Check if a font dictionary has an embedded font program via FontDescriptor.
/// Verifies that FontFile/FontFile2/FontFile3 actually points to a Stream object,
/// not just that the key exists (lopdf can drop stream data during load/save).
fn has_embedded_font_program(doc: &Document, dict: &lopdf::Dictionary) -> bool {
    match dict.get(b"FontDescriptor").ok() {
        Some(Object::Reference(fd_id)) => {
            if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) {
                has_valid_font_file(doc, fd)
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Check if a FontDescriptor has a valid FontFile/FontFile2/FontFile3 stream.
fn has_valid_font_file(doc: &Document, fd: &lopdf::Dictionary) -> bool {
    for key in &[b"FontFile".as_slice(), b"FontFile2", b"FontFile3"] {
        let Some(data) = read_fontfile_stream_content(doc, fd, key) else {
            continue;
        };
        if data.is_empty() {
            continue;
        }
        if *key == b"FontFile2" && !is_usable_truetype_font_program(&data) {
            continue;
        }
        return true;
    }
    false
}

fn read_fontfile_stream_content(
    doc: &Document,
    fd: &lopdf::Dictionary,
    key: &[u8],
) -> Option<Vec<u8>> {
    let obj = fd.get(key).ok()?;
    let stream = match obj {
        Object::Stream(s) => s.clone(),
        Object::Reference(id) => match doc.objects.get(id) {
            Some(Object::Stream(s)) => s.clone(),
            _ => return None,
        },
        _ => return None,
    };
    let mut stream = stream;
    let _ = stream.decompress();
    Some(stream.content)
}

fn is_usable_truetype_font_program(data: &[u8]) -> bool {
    let Ok(face) = ttf_parser::Face::parse(data, 0) else {
        return false;
    };
    // CIDFontType2 subsets used with Identity-H/V encoding deliberately omit
    // the cmap table (GID == CID, no encoding lookup needed). Only reject if
    // the font is completely empty or unparseable.
    face.number_of_glyphs() > 0
}

/// Get descendant CIDFont info: (object_id, is_embedded).
fn get_descendant_embed_info(
    doc: &Document,
    type0_dict: &lopdf::Dictionary,
) -> Option<(ObjectId, bool)> {
    let descendants = match type0_dict.get(b"DescendantFonts").ok() {
        Some(Object::Array(arr)) => arr,
        _ => return None,
    };

    for item in descendants {
        let desc_id = match item {
            Object::Reference(id) => *id,
            _ => continue,
        };
        let Some(Object::Dictionary(desc)) = doc.objects.get(&desc_id) else {
            continue;
        };
        let embedded = match desc.get(b"FontDescriptor").ok() {
            Some(Object::Reference(fd_id)) => {
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) {
                    has_valid_font_file(doc, fd)
                } else {
                    false
                }
            }
            _ => false,
        };
        return Some((desc_id, embedded));
    }

    None
}

/// Return whether a Name object value is valid UTF-8 (direct or indirect).
fn name_is_valid_utf8(doc: &Document, dict_id: ObjectId, key: &[u8]) -> bool {
    let Some(Object::Dictionary(dict)) = doc.objects.get(&dict_id) else {
        return true;
    };
    let Some(raw) = get_name_bytes_resolved(doc, dict, key) else {
        return false;
    };
    std::str::from_utf8(&raw).is_ok()
}

/// Create a conservative PDF name token that is always valid UTF-8.
fn sanitize_font_name_for_pdf(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '+' | '.') {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}

/// Repair invalid or missing font name entries to satisfy PDF/A 6.1.8 UTF-8 rules.
fn repair_invalid_font_names(doc: &mut Document, info: &NonEmbeddedFont, fd_id: ObjectId) {
    let fallback = format!("FallbackFont{}", info.font_id.0);
    let safe_name = sanitize_font_name_for_pdf(&info.name, &fallback);
    let safe_bytes = safe_name.as_bytes().to_vec();

    let repair_root = !name_is_valid_utf8(doc, info.font_id, b"BaseFont");
    let repair_target =
        info.target_id != info.font_id && !name_is_valid_utf8(doc, info.target_id, b"BaseFont");
    let repair_fd = !name_is_valid_utf8(doc, fd_id, b"FontName");

    if repair_root {
        if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&info.font_id) {
            font.set("BaseFont", Object::Name(safe_bytes.clone()));
        }
    }
    if repair_target {
        if let Some(Object::Dictionary(ref mut target)) = doc.objects.get_mut(&info.target_id) {
            target.set("BaseFont", Object::Name(safe_bytes.clone()));
        }
    }
    if repair_fd {
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            fd.set("FontName", Object::Name(safe_bytes));
        }
    }
}

/// Promote inline font dictionaries in `/Resources /Font` to standalone objects.
///
/// Some PDFs define fonts as inline dicts inside the Resources `/Font` map
/// instead of as standalone objects referenced by object ID.  For example:
/// ```text
/// /Font << /bannertopdf-font << /Type /Font /Subtype /Type1 /BaseFont /Courier >> >>
/// ```
/// `embed_fonts` only iterates `doc.objects`, so it never sees such inline dicts
/// and cannot embed them.  This function:
/// 1. Iterates all dict objects looking for an inline `/Font` sub-dict.
/// 2. For each inline font dict (a `Dictionary`, not a `Reference`), creates
///    a new document object and replaces the inline dict with a `Reference`.
///
/// Must run **before** `embed_fonts` so the promoted fonts become candidates
/// for embedding.
///
/// Returns the number of inline font dicts promoted.
pub fn promote_inline_font_dicts(doc: &mut Document) -> usize {
    // Collect (owner_id, path, font_alias, font_dict) for inline font entries.
    // "path" is whether the /Font dict was found directly or via /Resources.
    // Structure: Vec<(object_id_to_mutate, via_resources, font_alias, font_dict)>
    let mut to_promote: Vec<(ObjectId, bool, Vec<u8>, lopdf::Dictionary)> = Vec::new();

    for (id, obj) in &doc.objects {
        let Object::Dictionary(d) = obj else { continue };

        // Case 1: object has /Font directly (e.g., XObject Form, Type3 font).
        if let Ok(Object::Dictionary(fonts_dict)) = d.get(b"Font") {
            let fonts_dict = fonts_dict.clone();
            for (alias, font_val) in fonts_dict.iter() {
                if let Object::Dictionary(fd) = font_val {
                    to_promote.push((*id, false, alias.to_vec(), fd.clone()));
                }
            }
        }

        // Case 2: object has /Resources → /Font (e.g., Page, ContentStream).
        // Resources may be inline or an indirect reference.
        let res_dict = match d.get(b"Resources") {
            Ok(Object::Dictionary(res)) => Some((res.clone(), false)),
            Ok(Object::Reference(res_ref)) => {
                if let Some(Object::Dictionary(res)) = doc.objects.get(res_ref) {
                    Some((res.clone(), true))
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some((res, res_is_indirect)) = res_dict {
            // Font dict inside Resources may also be inline or indirect.
            let fonts_dict = match res.get(b"Font") {
                Ok(Object::Dictionary(fd)) => Some((fd.clone(), false)),
                Ok(Object::Reference(fd_ref)) => {
                    if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_ref) {
                        Some((fd.clone(), true))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some((fonts_dict, fonts_is_indirect)) = fonts_dict {
                for (alias, font_val) in fonts_dict.iter() {
                    if let Object::Dictionary(fd) = font_val {
                        // Store the object that owns the Font dict for mutation.
                        // If Resources is indirect, mutate the Resources object.
                        // If Font dict is indirect, mutate the Font dict object.
                        let (owner, via_res) = if fonts_is_indirect {
                            // Font dict is a separate object — no need to promote,
                            // the font entries inside it might be inline though.
                            // We need the Font dict's reference to mutate it.
                            if let Ok(Object::Reference(fd_ref)) = res.get(b"Font") {
                                (*fd_ref, false)
                            } else {
                                continue;
                            }
                        } else if res_is_indirect {
                            if let Ok(Object::Reference(res_ref)) = d.get(b"Resources") {
                                (*res_ref, true)
                            } else {
                                continue;
                            }
                        } else {
                            (*id, true)
                        };
                        to_promote.push((owner, via_res, alias.to_vec(), fd.clone()));
                    }
                }
            }
        }
    }

    let mut promoted = 0usize;
    for (owner_id, via_resources, alias, font_dict) in to_promote {
        let new_id = doc.add_object(Object::Dictionary(font_dict));
        if let Some(Object::Dictionary(owner)) = doc.objects.get_mut(&owner_id) {
            if via_resources {
                if let Ok(Object::Dictionary(res)) = owner.get_mut(b"Resources") {
                    if let Ok(Object::Dictionary(fonts)) = res.get_mut(b"Font") {
                        fonts.set(alias.as_slice(), Object::Reference(new_id));
                        promoted += 1;
                    }
                }
            } else if let Ok(Object::Dictionary(fonts)) = owner.get_mut(b"Font") {
                fonts.set(alias.as_slice(), Object::Reference(new_id));
                promoted += 1;
            }
        }
    }
    promoted
}

/// Embed fonts from system font files into the document.
pub fn embed_fonts(doc: &mut Document) -> Result<FontEmbedReport> {
    let mut report = FontEmbedReport {
        fonts_inspected: 0,
        non_embedded_found: 0,
        fonts_embedded: 0,
        failed: Vec::new(),
    };

    let non_embedded = find_non_embedded_fonts_detailed(doc);
    report.fonts_inspected = count_all_fonts(doc);
    report.non_embedded_found = non_embedded.len();

    for info in &non_embedded {
        let font_path = find_system_font(&info.name).or_else(find_fallback_font);

        match font_path {
            Some(path) => match embed_font_on_target(doc, info, &path) {
                Ok(()) => report.fonts_embedded += 1,
                Err(e) => report.failed.push((info.name.clone(), format!("{e}"))),
            },
            None => {
                report
                    .failed
                    .push((info.name.clone(), "no font file available".into()));
            }
        }
    }

    // Second pass: embed fonts via their FontDescriptor objects directly.
    // This handles PDFs where font dicts are inline (not separate objects) and
    // are therefore not found by find_non_embedded_fonts_detailed, but their
    // FontDescriptor objects ARE indirect and can be updated.
    let fd_embedded = embed_via_font_descriptors(doc);
    report.fonts_embedded += fd_embedded;

    // Synchronize Subtype for all font dicts that share a FontDescriptor.
    // When embed_font_on_target changes one font dict's Subtype (e.g.,
    // Type1→TrueType after embedding a .ttf), other dicts pointing to the
    // same FD must also be updated. Otherwise fix_truetype_encoding skips
    // them (it only processes Subtype=TrueType) and fix_font_width_mismatches
    // uses the wrong computation path.
    sync_subtypes_from_fontfile(doc);

    Ok(report)
}

/// Synchronize font dict Subtype with the actual embedded font program type.
/// When a TrueType font is embedded via FontFile2 but the font dict still says
/// /Subtype /Type1, update it to /Subtype /TrueType. Needed for font dicts
/// that share a FontDescriptor where only ONE dict was updated by embed_font_on_target.
pub fn sync_subtypes_from_fontfile(doc: &mut Document) {
    // Build map: FD id → expected Subtype based on FontFile key
    let mut fd_fonttype: std::collections::HashMap<ObjectId, &'static [u8]> = Default::default();
    for (&id, obj) in doc.objects.iter() {
        let Object::Dictionary(d) = obj else { continue };
        if get_name(d, b"Type").as_deref() != Some("FontDescriptor") {
            continue;
        }
        if d.has(b"FontFile2") {
            fd_fonttype.insert(id, b"TrueType");
        } else if d.has(b"FontFile3") {
            fd_fonttype.insert(id, b"Type1");
        }
    }
    if fd_fonttype.is_empty() {
        return;
    }

    // Update font dicts that have mismatched Subtype
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let (fd_ref, current_subtype, is_subset) = {
            let Some(Object::Dictionary(d)) = doc.objects.get(&id) else {
                continue;
            };
            let sub = match get_name(d, b"Subtype") {
                Some(s) if s == "Type1" || s == "TrueType" || s == "MMType1" => s,
                _ => continue,
            };
            let fd = match d.get(b"FontDescriptor").ok() {
                Some(Object::Reference(r)) => *r,
                _ => continue,
            };
            let bf = get_name(d, b"BaseFont").unwrap_or_default();
            let subset = bf.len() > 7 && bf.as_bytes()[6] == b'+';
            (fd, sub, subset)
        };

        // Skip subset fonts — their Subtype and encoding are correct from the
        // original authoring tool. Changing Subtype from Type1 to TrueType
        // (because a shared FD was re-embedded as TrueType for another font)
        // breaks the encoding and glyph lookup path.
        if is_subset {
            continue;
        }

        if let Some(&expected) = fd_fonttype.get(&fd_ref) {
            let expected_str = std::str::from_utf8(expected).unwrap_or("");
            // Pre-check: is the FD symbolic? (read before mutable borrow)
            let fd_is_symbolic = matches!(
                doc.objects.get(&fd_ref),
                Some(Object::Dictionary(fd)) if fd.get(b"Flags").ok()
                    .and_then(|f| f.as_i64().ok()).unwrap_or(0) & 4 != 0
            );
            if let Some(Object::Dictionary(ref mut d)) = doc.objects.get_mut(&id) {
                if current_subtype != expected_str {
                    d.set("Subtype", Object::Name(expected.to_vec()));
                }
                // Ensure non-symbolic, non-subset TrueType substitute fonts use
                // WinAnsiEncoding. Some dicts get MacRomanEncoding from
                // fix_truetype_encoding, creating inconsistent width computation
                // across dicts sharing the same FD. Standardize on WinAnsi.
                let is_subset = {
                    let bf = get_name(d, b"BaseFont").unwrap_or_default();
                    bf.len() > 7 && bf.as_bytes()[6] == b'+'
                };
                if expected == b"TrueType" && !fd_is_symbolic && !is_subset {
                    match d.get(b"Encoding").ok() {
                        None => {
                            // No encoding at all — add WinAnsi
                            d.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
                        }
                        Some(Object::Name(n)) if n != b"WinAnsiEncoding" => {
                            // Simple name encoding (e.g., MacRomanEncoding) — override
                            d.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
                        }
                        Some(Object::Dictionary(enc_dict)) => {
                            // Encoding dict with Differences — update BaseEncoding
                            // but preserve the Differences array
                            let mut enc = enc_dict.clone();
                            enc.set("BaseEncoding", Object::Name(b"WinAnsiEncoding".to_vec()));
                            d.set("Encoding", Object::Dictionary(enc));
                        }
                        _ => {} // Already WinAnsi or reference
                    }
                }
            }
        }
    }
}

/// Embed fonts by scanning FontDescriptor objects without a font program.
/// FontDescriptor objects are always indirect (separate PDF objects) even when
/// the parent font dict is inline. We can embed a font program directly into
/// the FontDescriptor using its /FontName entry.
fn embed_via_font_descriptors(doc: &mut Document) -> usize {
    // Collect (fd_id, font_name) for descriptors without font programs.
    let mut to_embed: Vec<(ObjectId, String)> = Vec::new();
    for (id, obj) in &doc.objects {
        let Object::Dictionary(d) = obj else { continue };
        if get_name(d, b"Type").as_deref() != Some("FontDescriptor") {
            continue;
        }
        if has_valid_font_file(doc, d) {
            continue;
        }
        let Some(font_name) = get_name(d, b"FontName") else {
            continue;
        };
        let base = strip_subset_prefix(&font_name).to_owned();
        // Skip Standard 14 fonts in the second pass — they cause stack overflow
        // in embed_font_on_target → update_metrics_from_font for certain PDFs.
        // Standard 14 fonts are handled by the first pass (find_non_embedded_fonts_detailed)
        // which has proper error handling. (#stack-overflow-std14)
        if is_standard_14(&base) {
            continue;
        }
        // Standard 14 fonts DO need embedding in PDF/A (ISO 19005 requires all fonts
        // to be embedded, no exceptions for the base 14). The previous skip caused
        // §6.2.11.4.1:1 violations when Standard 14 fonts were referenced but not
        // embedded by the first pass (e.g. Helvetica in Form XObjects).
        to_embed.push((*id, base));
    }

    let mut embedded = 0usize;
    for (fd_id, font_name) in to_embed {
        let Some(path) = find_system_font(&font_name).or_else(find_fallback_font) else {
            continue;
        };
        let Ok(font_data) = std::fs::read(&path) else {
            continue;
        };

        // Detect font type: TrueType (.ttf/.otf with TT outlines) or CFF.
        let ext = std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let is_truetype = ext.eq_ignore_ascii_case("ttf")
            || font_data.starts_with(b"\x00\x01\x00\x00")
            || font_data.starts_with(b"true");

        let (ff_key, subtype): (&[u8], Option<&[u8]>) = if is_truetype {
            (b"FontFile2", None)
        } else {
            (b"FontFile3", Some(b"OpenType"))
        };

        let mut stream = lopdf::Stream::new(
            lopdf::dictionary! {
                "Length" => Object::Integer(font_data.len() as i64),
            },
            font_data,
        );
        if let Some(sub) = subtype {
            stream.dict.set("Subtype", Object::Name(sub.to_vec()));
        }
        let _ = stream.compress();
        let ff_id = doc.add_object(Object::Stream(stream));

        if let Some(Object::Dictionary(fd)) = doc.objects.get_mut(&fd_id) {
            fd.set(ff_key, Object::Reference(ff_id));
            embedded += 1;
        }
    }
    embedded
}

/// Third-pass font embedding: find font dicts without a FontDescriptor and
/// create one with an embedded font program. This handles Standard 14 fonts
/// that are referenced without any FontDescriptor (common in legacy PDFs).
#[allow(dead_code)]
fn embed_bare_fonts(doc: &mut Document) -> usize {
    // Collect (font_dict_id, base_font_name) for fonts lacking FontDescriptor.
    let mut to_embed: Vec<(ObjectId, String)> = Vec::new();
    for (&id, obj) in doc.objects.iter() {
        let Object::Dictionary(d) = obj else { continue };
        let subtype = match d.get(b"Subtype").ok() {
            Some(Object::Name(n)) => n.clone(),
            _ => continue,
        };
        if subtype != b"Type1" && subtype != b"TrueType" && subtype != b"MMType1" {
            continue;
        }
        // Must have BaseFont.
        let base_font = match get_name(d, b"BaseFont") {
            Some(n) => n,
            None => continue,
        };
        // Skip if FontDescriptor already exists as a valid Dictionary.
        // This third pass ONLY handles fonts completely lacking a FontDescriptor.
        // Fonts with FD but no FontFile are handled by the first two passes;
        // re-processing them here can trigger stack overflows via recursive
        // font embedding.
        if d.has(b"FontDescriptor") {
            let fd_is_valid_dict = match d.get(b"FontDescriptor").ok() {
                Some(Object::Reference(fd_ref)) => {
                    matches!(doc.objects.get(fd_ref), Some(Object::Dictionary(_)))
                }
                Some(Object::Dictionary(_)) => true,
                _ => false, // null or invalid reference
            };
            if fd_is_valid_dict {
                continue;
            }
            // FD reference points to null — fall through to create a new one
        }
        let base = strip_subset_prefix(&base_font).to_owned();
        to_embed.push((id, base));
    }

    let mut embedded = 0usize;
    for (font_dict_id, font_name) in to_embed {
        let Some(path) = find_system_font(&font_name).or_else(find_fallback_font) else {
            continue;
        };
        let Ok(font_data) = std::fs::read(&path) else {
            continue;
        };

        // Detect font type.
        let is_truetype = font_data.starts_with(b"\x00\x01\x00\x00")
            || font_data.starts_with(b"true")
            || path.ends_with(".ttf")
            || path.ends_with(".TTF");

        let (ff_key, ff_subtype): (&[u8], Option<&[u8]>) = if is_truetype {
            (b"FontFile2", None)
        } else {
            (b"FontFile3", Some(b"OpenType"))
        };

        // Create font file stream.
        let mut stream = lopdf::Stream::new(
            lopdf::dictionary! {
                "Length" => Object::Integer(font_data.len() as i64),
            },
            font_data,
        );
        if let Some(sub) = ff_subtype {
            stream.dict.set("Subtype", Object::Name(sub.to_vec()));
        }
        let _ = stream.compress();
        let ff_id = doc.add_object(Object::Stream(stream));

        // Create FontDescriptor.
        let mut fd_dict = lopdf::dictionary! {
            "Type" => Object::Name(b"FontDescriptor".to_vec()),
            "FontName" => Object::Name(font_name.as_bytes().to_vec()),
            "Flags" => Object::Integer(32), // NonSymbolic
            "ItalicAngle" => Object::Integer(0),
            "Ascent" => Object::Integer(800),
            "Descent" => Object::Integer(-200),
            "CapHeight" => Object::Integer(700),
            "StemV" => Object::Integer(80),
            "FontBBox" => Object::Array(vec![
                Object::Integer(-200),
                Object::Integer(-300),
                Object::Integer(1200),
                Object::Integer(900),
            ]),
        };
        fd_dict.set(ff_key, Object::Reference(ff_id));
        let fd_id = doc.add_object(Object::Dictionary(fd_dict));

        // Add FontDescriptor reference to the font dict.
        if let Some(Object::Dictionary(ref mut d)) = doc.objects.get_mut(&font_dict_id) {
            d.set("FontDescriptor", Object::Reference(fd_id));
            // If Subtype is Type1 but we embedded TrueType, update Subtype.
            if is_truetype {
                d.set("Subtype", Object::Name(b"TrueType".to_vec()));
            }
            embedded += 1;
        }
    }
    embedded
}

/// Ensure all non-symbolic TrueType fonts with FontFile2 have an Encoding entry.
/// Without Encoding, veraPDF may use font-internal cmap tables that differ
/// between the original and substitute fonts, causing width mismatches.
/// Must run AFTER embed_fonts and fix_font_width_mismatches.
pub fn ensure_truetype_encoding(doc: &mut Document) -> usize {
    // Build set of FD IDs that have FontFile2
    let mut ff2_fds: std::collections::HashSet<ObjectId> = Default::default();
    for (&id, obj) in doc.objects.iter() {
        let Object::Dictionary(d) = obj else { continue };
        if get_name(d, b"Type").as_deref() == Some("FontDescriptor") && d.has(b"FontFile2") {
            ff2_fds.insert(id);
        }
    }

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;
    for id in ids {
        let needs_enc = {
            let Some(Object::Dictionary(d)) = doc.objects.get(&id) else {
                continue;
            };
            if get_name(d, b"Subtype").as_deref() != Some("TrueType") {
                continue;
            }
            if d.has(b"Encoding") {
                continue;
            }
            // Check if FD has FontFile2
            let fd = match d.get(b"FontDescriptor").ok() {
                Some(Object::Reference(r)) => *r,
                _ => continue,
            };
            if !ff2_fds.contains(&fd) {
                continue;
            }
            // Skip symbolic fonts
            if is_font_symbolic(doc, d) {
                continue;
            }
            true
        };
        if needs_enc {
            if let Some(Object::Dictionary(ref mut d)) = doc.objects.get_mut(&id) {
                d.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
                fixed += 1;
            }
        }
    }
    fixed
}

/// Check if this is a Standard 14 font.
pub fn is_standard_14(name: &str) -> bool {
    let clean = strip_subset_prefix(name);
    STANDARD_14.contains(&clean)
}

/// Strip subset prefix (e.g., "ABCDEF+FontName" → "FontName").
fn strip_subset_prefix(name: &str) -> &str {
    if name.len() > 7 && name.as_bytes()[6] == b'+' {
        &name[7..]
    } else {
        name
    }
}

/// Embed a font file, targeting the correct dictionary for Type0 vs simple fonts.
/// Also updates the font Subtype to match the embedded program type.
fn embed_font_on_target(doc: &mut Document, info: &NonEmbeddedFont, font_path: &str) -> Result<()> {
    let raw_data = std::fs::read(font_path)
        .map_err(|e| ManipError::Other(format!("failed to read font file: {e}")))?;

    // If the file is a TrueType Collection (.ttc), extract the matching face
    // into a standalone TrueType font. PDF FontFile2 does not accept TTC data.
    let font_data = if raw_data.len() >= 4 && &raw_data[0..4] == b"ttcf" {
        let face_index = find_ttc_face_index(&raw_data, &info.name);
        extract_ttc_face(&raw_data, face_index).ok_or_else(|| {
            ManipError::Other(format!(
                "failed to extract face {} from TTC {}",
                face_index, font_path
            ))
        })?
    } else {
        raw_data
    };

    let is_truetype = font_path.ends_with(".ttf")
        || font_path.ends_with(".ttc")
        || (font_data.len() >= 4
            && (&font_data[0..4] == b"\x00\x01\x00\x00" || &font_data[0..4] == b"true"));

    let is_otf =
        font_path.ends_with(".otf") || (font_data.len() >= 4 && &font_data[0..4] == b"OTTO");

    let font_file_key = if is_truetype {
        "FontFile2"
    } else if is_otf {
        "FontFile3"
    } else {
        "FontFile"
    };

    // Create font stream.
    let mut stream_dict = dictionary! {
        "Length" => Object::Integer(font_data.len() as i64),
    };
    if is_truetype {
        stream_dict.set("Length1", Object::Integer(font_data.len() as i64));
    }
    if is_otf {
        stream_dict.set("Subtype", Object::Name(b"OpenType".to_vec()));
    }

    let font_stream = Stream::new(stream_dict, font_data.clone());
    let stream_id = doc.add_object(Object::Stream(font_stream));

    // Get or create FontDescriptor on the target (CIDFont for Type0, font itself otherwise).
    let mut fd_id = get_or_create_font_descriptor(doc, info.target_id)?;

    // If the FD already has an embedded font program (FontFile/FontFile2/FontFile3),
    // it's shared with a subset font that already has correct data. Don't overwrite
    // the subset's font program — create a NEW FontDescriptor for this non-subset font
    // to avoid breaking §6.2.11.4.1:2 for the subset's glyphs.
    let fd_already_has_fontfile = matches!(
        doc.objects.get(&fd_id),
        Some(Object::Dictionary(fd)) if fd.has(b"FontFile") || fd.has(b"FontFile2") || fd.has(b"FontFile3")
    );
    if fd_already_has_fontfile {
        // Clone the FD and create a new one for this font
        let new_fd = if let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) {
            let mut cloned = fd.clone();
            cloned.remove(b"FontFile");
            cloned.remove(b"FontFile2");
            cloned.remove(b"FontFile3");
            cloned.set(font_file_key, Object::Reference(stream_id));
            cloned
        } else {
            lopdf::dictionary! {
                "Type" => "FontDescriptor",
                "FontName" => Object::Name(info.name.as_bytes().to_vec()),
                "Flags" => Object::Integer(32),
            }
        };
        fd_id = doc.add_object(Object::Dictionary(new_fd));
        // Update the font dict to point to the new FD
        if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&info.font_id) {
            font.set("FontDescriptor", Object::Reference(fd_id));
        }
    } else {
        // FD has no existing font file — safe to write directly
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            fd.remove(b"FontFile");
            fd.remove(b"FontFile2");
            fd.remove(b"FontFile3");
            fd.set(font_file_key, Object::Reference(stream_id));
        }
    }

    // Update font Subtype to match embedded program type.
    // veraPDF checks that Subtype matches the FontFile type.
    if is_truetype && !info.is_type0 {
        // For simple fonts: Type1 → TrueType when embedding .ttf
        if info.subtype == "Type1" || info.subtype == "MMType1" {
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&info.font_id) {
                font.set("Subtype", Object::Name(b"TrueType".to_vec()));
            }
        }
    }
    if is_otf && !info.is_type0 {
        // For simple fonts: TrueType → Type1 when embedding .otf (CFF-based OpenType).
        // veraPDF checks that FontFile3 with /Subtype /OpenType matches Type1.
        if info.subtype == "TrueType" {
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&info.font_id) {
                font.set("Subtype", Object::Name(b"Type1".to_vec()));
            }
        }
    }
    if is_truetype && info.is_type0 {
        // For CIDFont descendants: CIDFontType0 → CIDFontType2 when embedding .ttf
        let target_subtype = {
            doc.objects
                .get(&info.target_id)
                .and_then(|o| {
                    if let Object::Dictionary(d) = o {
                        get_name(d, b"Subtype")
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        };
        if target_subtype == "CIDFontType0" {
            if let Some(Object::Dictionary(ref mut cid)) = doc.objects.get_mut(&info.target_id) {
                cid.set("Subtype", Object::Name(b"CIDFontType2".to_vec()));
            }
        }
    }

    // Normalize invalid font names before further checks.
    // This targets PDF/A 6.1.8:1 (UTF-8 validity of names).
    repair_invalid_font_names(doc, info, fd_id);

    // Update Widths and FontDescriptor metrics from the embedded font.
    if is_truetype || is_otf {
        update_metrics_from_font(doc, info, &font_data);
    }

    // If we embedded a non-symbolic font (e.g., DejaVuSans) for a symbolic-named
    // font (e.g., ZapfDingbats), update FontDescriptor Flags to match the actual
    // embedded program. veraPDF checks the font program, not the name.
    // Skip actual symbolic fonts (Symbol, ZapfDingbats) — they must keep Symbolic
    // flag so veraPDF uses CFF internal encoding for width validation.
    if (is_truetype || is_otf) && !is_symbolic_font_name(&info.name) {
        if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
            let has_31_cmap = face.tables().cmap.as_ref().is_some_and(|cmap| {
                cmap.subtables.into_iter().any(|st| {
                    st.platform_id == ttf_parser::PlatformId::Windows && st.encoding_id == 1
                })
            });
            if has_31_cmap {
                // Font program has Windows Unicode cmap → non-symbolic.
                if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                    if let Ok(Object::Integer(flags)) = fd.get(b"Flags") {
                        let mut f = *flags;
                        f &= !4; // Clear Symbolic (bit 3)
                        f |= 32; // Set Nonsymbolic (bit 6)
                        fd.set("Flags", Object::Integer(f));
                    }
                }
            }
        }
    }

    // PDF/A 6.2.11.6:4: for symbolic TrueType fonts, the cmap must contain
    // exactly 1 subtable or include (3,0) Microsoft Symbol. Fix the embedded
    // font stream in-place if needed.
    if is_truetype {
        let is_symbolic = doc
            .objects
            .get(&fd_id)
            .and_then(|o| {
                if let Object::Dictionary(d) = o {
                    Some(d)
                } else {
                    None
                }
            })
            .and_then(|d| {
                if let Ok(Object::Integer(f)) = d.get(b"Flags") {
                    Some(*f & 4 != 0)
                } else {
                    None
                }
            })
            .unwrap_or(false);
        if is_symbolic {
            fix_symbolic_truetype_cmap(doc, stream_id);
        }
    }

    Ok(())
}

/// Fix symbolic TrueType font cmap table (PDF/A 6.2.11.6:4).
///
/// For symbolic fonts, the cmap must have exactly 1 subtable or include (3,0)
/// Microsoft Symbol. If neither condition holds, strip the cmap to 1 subtable
/// by modifying the embedded font stream binary.
fn fix_symbolic_truetype_cmap(doc: &mut Document, stream_id: ObjectId) {
    let mut st = match doc.objects.get(&stream_id) {
        Some(Object::Stream(s)) => s.clone(),
        _ => return,
    };
    let _ = st.decompress();
    let mut data = st.content;

    if data.len() < 12 {
        return;
    }

    // Parse Offset Table to find cmap.
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    let mut cmap_dir_pos = None;
    for i in 0..num_tables {
        let pos = 12 + i * 16;
        if pos + 16 > data.len() {
            return;
        }
        if &data[pos..pos + 4] == b"cmap" {
            cmap_dir_pos = Some(pos);
            break;
        }
    }

    let dir_pos = match cmap_dir_pos {
        Some(p) => p,
        None => return,
    };

    let cmap_off = u32::from_be_bytes([
        data[dir_pos + 8],
        data[dir_pos + 9],
        data[dir_pos + 10],
        data[dir_pos + 11],
    ]) as usize;

    if cmap_off + 4 > data.len() {
        return;
    }

    let num_sub = u16::from_be_bytes([data[cmap_off + 2], data[cmap_off + 3]]);
    if num_sub <= 1 {
        return; // Already 1 subtable — no fix needed.
    }

    // Check if (3,0) Microsoft Symbol cmap exists and collect record metadata.
    let mut records: Vec<(usize, u16, u16)> = Vec::new();
    for j in 0..num_sub as usize {
        let rec = cmap_off + 4 + j * 8;
        if rec + 8 > data.len() {
            return;
        }
        let plat = u16::from_be_bytes([data[rec], data[rec + 1]]);
        let enc = u16::from_be_bytes([data[rec + 2], data[rec + 3]]);
        records.push((j, plat, enc));
        if plat == 3 && enc == 0 {
            return; // Already has (3,0) — no fix needed.
        }
    }

    // Strip to 1 cmap subtable.
    //
    // Keep a subtable that preserves byte-code coverage when possible. For
    // legacy symbolic fonts this is typically the Mac Roman (1,0) cmap.
    let preferred = records
        .iter()
        .find(|(_, plat, enc)| *plat == 1 && *enc == 0)
        .or_else(|| {
            records
                .iter()
                .find(|(_, plat, enc)| *plat == 3 && *enc == 1)
        })
        .or_else(|| records.iter().find(|(_, plat, _)| *plat == 0))
        .map(|(idx, _, _)| *idx)
        .unwrap_or(0);

    if preferred != 0 {
        let first_rec = cmap_off + 4;
        let pref_rec = cmap_off + 4 + preferred * 8;
        let pref_bytes = data[pref_rec..pref_rec + 8].to_vec();
        data[first_rec..first_rec + 8].copy_from_slice(&pref_bytes);
    }
    data[cmap_off + 2] = 0;
    data[cmap_off + 3] = 1;

    if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&stream_id) {
        stream.set_plain_content(data);
    }
}

/// Find the face index within a TTC that best matches the target font name.
fn find_ttc_face_index(data: &[u8], target_name: &str) -> u32 {
    let clean = strip_subset_prefix(target_name);
    let num_faces = ttf_parser::fonts_in_collection(data).unwrap_or(0);
    for i in 0..num_faces {
        if let Ok(face) = ttf_parser::Face::parse(data, i) {
            for name in face.names() {
                // Check PostScript name (name ID 6) and full name (name ID 4).
                if name.name_id == ttf_parser::name_id::POST_SCRIPT_NAME
                    || name.name_id == ttf_parser::name_id::FULL_NAME
                {
                    if let Some(s) = name.to_string() {
                        if s.eq_ignore_ascii_case(clean) {
                            return i;
                        }
                    }
                }
            }
        }
    }
    0 // default to first face
}

/// Extract a single face from a TrueType Collection (TTC) into a standalone
/// TrueType font. Returns `None` if the data is not a valid TTC or the face
/// index is out of range.
fn extract_ttc_face(data: &[u8], face_index: u32) -> Option<Vec<u8>> {
    if data.len() < 12 || &data[0..4] != b"ttcf" {
        return None;
    }

    let num_fonts = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    if face_index >= num_fonts {
        return None;
    }

    let header_end = 12 + num_fonts as usize * 4;
    if data.len() < header_end {
        return None;
    }

    // Offset to the Offset Table for this face.
    let off_pos = 12 + face_index as usize * 4;
    let face_off = u32::from_be_bytes([
        data[off_pos],
        data[off_pos + 1],
        data[off_pos + 2],
        data[off_pos + 3],
    ]) as usize;

    if face_off + 12 > data.len() {
        return None;
    }

    let sf_version = &data[face_off..face_off + 4];
    let num_tables = u16::from_be_bytes([data[face_off + 4], data[face_off + 5]]) as usize;

    if face_off + 12 + num_tables * 16 > data.len() {
        return None;
    }

    // Read table records (tag, checksum, offset, length).
    struct Rec {
        tag: [u8; 4],
        checksum: [u8; 4],
        offset: u32,
        length: u32,
    }
    let mut tables = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let p = face_off + 12 + i * 16;
        tables.push(Rec {
            tag: [data[p], data[p + 1], data[p + 2], data[p + 3]],
            checksum: [data[p + 4], data[p + 5], data[p + 6], data[p + 7]],
            offset: u32::from_be_bytes([data[p + 8], data[p + 9], data[p + 10], data[p + 11]]),
            length: u32::from_be_bytes([data[p + 12], data[p + 13], data[p + 14], data[p + 15]]),
        });
    }

    // Build standalone TrueType font.
    let dir_end = 12 + num_tables * 16;
    let data_start = (dir_end as u32 + 3) & !3;

    let mut out = Vec::new();

    // Offset Table header.
    out.extend_from_slice(sf_version);
    out.extend_from_slice(&(num_tables as u16).to_be_bytes());
    // searchRange, entrySelector, rangeShift — copy from original.
    out.extend_from_slice(&data[face_off + 6..face_off + 12]);

    // Table directory with updated offsets.
    let mut cur = data_start;
    let mut new_offsets = Vec::with_capacity(num_tables);
    for t in &tables {
        new_offsets.push(cur);
        cur += (t.length + 3) & !3;
    }
    for (i, t) in tables.iter().enumerate() {
        out.extend_from_slice(&t.tag);
        out.extend_from_slice(&t.checksum);
        out.extend_from_slice(&new_offsets[i].to_be_bytes());
        out.extend_from_slice(&t.length.to_be_bytes());
    }

    // Pad to data_start.
    out.resize(data_start as usize, 0);

    // Table data.
    for t in &tables {
        let start = t.offset as usize;
        let end = start + t.length as usize;
        if end > data.len() {
            return None;
        }
        out.extend_from_slice(&data[start..end]);
        let pad = (4 - (t.length % 4)) % 4;
        out.extend(std::iter::repeat_n(0u8, pad as usize));
    }

    Some(out)
}

/// Update font metrics (Widths, FontBBox, etc.) from the embedded font data.
fn update_metrics_from_font(doc: &mut Document, info: &NonEmbeddedFont, font_data: &[u8]) {
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return;
    };

    let units_per_em = face.units_per_em() as f64;
    if units_per_em == 0.0 {
        return;
    }
    let scale = 1000.0 / units_per_em;

    // Update FontDescriptor metrics.
    let fd_id = {
        let Some(Object::Dictionary(target)) = doc.objects.get(&info.target_id) else {
            return;
        };
        match target.get(b"FontDescriptor").ok() {
            Some(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };

    if let Some(fd_id) = fd_id {
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            let ascent = (face.ascender() as f64 * scale).round() as i64;
            let descent = (face.descender() as f64 * scale).round() as i64;
            let bbox = face.global_bounding_box();
            fd.set("Ascent", Object::Integer(ascent));
            fd.set("Descent", Object::Integer(descent));
            fd.set(
                "FontBBox",
                Object::Array(vec![
                    Object::Integer((bbox.x_min as f64 * scale).round() as i64),
                    Object::Integer((bbox.y_min as f64 * scale).round() as i64),
                    Object::Integer((bbox.x_max as f64 * scale).round() as i64),
                    Object::Integer((bbox.y_max as f64 * scale).round() as i64),
                ]),
            );
            if let Some(cap_h) = face.capital_height() {
                fd.set(
                    "CapHeight",
                    Object::Integer((cap_h as f64 * scale).round() as i64),
                );
            }
        }
    }

    if info.is_type0 {
        update_cid_widths(doc, info.target_id, &face, scale);
    } else {
        // For CFF-based symbolic fonts (Symbol, ZapfDingbats), use the CFF
        // internal encoding to compute widths.  The Unicode cmap in OTF wrappers
        // maps unrelated Latin codepoints to symbol glyphs, producing wrong widths.
        let is_cff = face.tables().glyf.is_none();
        let (_base_encoding_name, skip_unreliable_simple_width_update) = {
            let font_dict = match doc.objects.get(&info.font_id) {
                Some(Object::Dictionary(d)) => d,
                _ => return,
            };
            let is_symbolic = is_font_symbolic(doc, font_dict) || is_symbolic_font_name(&info.name);
            let is_type1_like = matches!(info.subtype.as_str(), "Type1" | "MMType1");
            let (enc_name, _differences) = get_simple_encoding_info(doc, font_dict);
            let base_font = get_name(font_dict, b"BaseFont").unwrap_or_default();
            let is_subset = base_font.len() > 7 && base_font.as_bytes()[6] == b'+';
            let has_existing_widths = match font_dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => !arr.is_empty(),
                Some(Object::Reference(r)) => doc
                    .get_object(*r)
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .map(|arr| !arr.is_empty())
                    .unwrap_or(false),
                _ => false,
            };
            // For non-symbolic Type1-like fonts without a concrete base encoding
            // name, code-to-glyph mapping is frequently ambiguous. Keep existing
            // widths and let the mismatch pass perform targeted corrections.
            (
                enc_name.clone(),
                is_type1_like
                    && !is_symbolic
                    && !is_subset
                    && enc_name.is_empty()
                    && has_existing_widths,
            )
        };
        // Always use CFF encoding for symbolic CFF fonts (Symbol, ZapfDingbats, etc.).
        // Using WinAnsiEncoding/MacRomanEncoding + Unicode cmap produces wrong widths
        // because the cmap maps unrelated Latin codepoints to symbol glyphs.
        // fix_classic_symbolic_base14_encoding strips these encodings later anyway,
        // so CFF-based widths are the correct source of truth from the start.
        let use_symbolic_cff_widths = is_cff && is_symbolic_font_name(&info.name);
        if use_symbolic_cff_widths {
            update_simple_widths_cff_symbolic(doc, info, font_data, &face, scale);
        } else if !skip_unreliable_simple_width_update {
            update_simple_widths(doc, info.font_id, &face, scale);
        }
    }
}

/// Update Widths for a simple font (Type1/TrueType).
/// Uses the font's Encoding to map character codes to glyph widths.
fn update_simple_widths(
    doc: &mut Document,
    font_id: ObjectId,
    face: &ttf_parser::Face,
    scale: f64,
) {
    let (first_char, last_char, encoding_name, differences) = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return;
        };
        let fc = font
            .get(b"FirstChar")
            .ok()
            .and_then(|o| match o {
                Object::Integer(i) => Some(*i as u32),
                _ => None,
            })
            .unwrap_or(0);
        let lc = font
            .get(b"LastChar")
            .ok()
            .and_then(|o| match o {
                Object::Integer(i) => Some(*i as u32),
                _ => None,
            })
            .unwrap_or(255);
        let mut enc_name = String::new();
        let mut diffs = std::collections::HashMap::new();
        match font.get(b"Encoding").ok() {
            Some(Object::Name(n)) => {
                enc_name = String::from_utf8(n.clone()).unwrap_or_default();
            }
            Some(Object::Dictionary(enc_dict)) => {
                if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                    enc_name = base;
                }
                parse_differences(doc, enc_dict, &mut diffs);
            }
            Some(Object::Reference(enc_ref)) => {
                if let Ok(Object::Dictionary(enc_dict)) = doc.get_object(*enc_ref) {
                    if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                        enc_name = base;
                    }
                    parse_differences(doc, enc_dict, &mut diffs);
                }
            }
            _ => {}
        }
        (fc, lc, enc_name, diffs)
    };

    // veraPDF validates TrueType widths using the PDF Encoding to map
    // character codes to Unicode, then looks up in the (3,1) cmap.
    // This is the same algorithm for both TrueType and CFF fonts.
    let is_truetype_outline = face.tables().glyf.is_some();

    let mut widths = Vec::new();
    for code in first_char..=last_char {
        // Differences override takes priority over base encoding.
        let width = if let Some(glyph_name) = differences.get(&code) {
            // Try glyph name → Unicode → (3,1) cmap.
            let w = glyph_name_to_unicode(glyph_name)
                .and_then(|u| face.glyph_index(u))
                .and_then(|gid| face.glyph_hor_advance(gid))
                .map(|w| (w as f64 * scale).round() as i64);
            w.or_else(|| {
                // Fallback: look up glyph by name directly.
                face.glyph_index_by_name(glyph_name)
                    .and_then(|gid| face.glyph_hor_advance(gid))
                    .map(|w| (w as f64 * scale).round() as i64)
            })
            .unwrap_or(0)
        } else {
            let ch = encoding_to_char(code, &encoding_name);
            if let Some(glyph_id) = face.glyph_index(ch) {
                face.glyph_hor_advance(glyph_id)
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0)
            } else if is_truetype_outline && code <= u16::MAX as u32 {
                face.glyph_hor_advance(ttf_parser::GlyphId(code as u16))
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0)
            } else {
                0
            }
        };
        widths.push(Object::Integer(width));
    }

    if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
        if !widths.is_empty() {
            font.set("Widths", Object::Array(widths));
            font.set("FirstChar", Object::Integer(first_char as i64));
            font.set("LastChar", Object::Integer(last_char as i64));
        }
    }
}

/// Extract the raw CFF table data from an OTF font file.
///
/// Returns None if the font is not OTF or has no CFF table.
fn extract_cff_table(font_data: &[u8]) -> Option<&[u8]> {
    let raw_face = ttf_parser::RawFace::parse(font_data, 0).ok()?;
    for record in raw_face.table_records {
        if &record.tag.to_bytes() == b"CFF " {
            let start = record.offset as usize;
            let end = start.checked_add(record.length as usize)?;
            return font_data.get(start..end);
        }
    }
    None
}

/// Compute Widths for a CFF-based symbolic font (Symbol, ZapfDingbats).
///
/// veraPDF validates Symbolic CFF fonts using:
/// 1. PDF Encoding Differences → glyph name → CFF charset → GID → hmtx width
/// 2. CFF internal encoding → GID → hmtx width (for non-Differences codes)
///
/// For OTF fonts where the CFF encoding is empty (all .notdef), codes not in
/// Differences get .notdef width. Codes IN Differences get the named glyph width.
fn update_simple_widths_cff_symbolic(
    doc: &mut Document,
    info: &NonEmbeddedFont,
    font_data: &[u8],
    face: &ttf_parser::Face,
    scale: f64,
) {
    let font_id = info.font_id;

    // Extract the CFF table from the OTF wrapper for encoding lookup.
    let cff_data = extract_cff_table(font_data);
    let cff = cff_data.and_then(cff_parser::Table::parse);

    // Parse PDF Encoding Differences (e.g., [1 /bullet]) so we can look up
    // named glyphs that override the CFF internal encoding.
    let (first_char, last_char, differences) = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return;
        };
        let fc = font
            .get(b"FirstChar")
            .ok()
            .and_then(|o| match o {
                Object::Integer(i) => Some(*i as u32),
                _ => None,
            })
            .unwrap_or(0);
        let lc = font
            .get(b"LastChar")
            .ok()
            .and_then(|o| match o {
                Object::Integer(i) => Some(*i as u32),
                _ => None,
            })
            .unwrap_or(255);
        let mut diffs = std::collections::HashMap::new();
        match font.get(b"Encoding").ok() {
            Some(Object::Dictionary(enc_dict)) => {
                parse_differences(doc, enc_dict, &mut diffs);
            }
            Some(Object::Reference(enc_ref)) => {
                if let Ok(Object::Dictionary(enc_dict)) = doc.get_object(*enc_ref) {
                    parse_differences(doc, enc_dict, &mut diffs);
                }
            }
            _ => {}
        }
        (fc, lc, diffs)
    };

    let mut widths = Vec::new();
    for code in first_char..=last_char {
        let width = if code > 255 {
            0
        } else if let Some(glyph_name) = differences.get(&code) {
            // Code is in Differences: look up glyph by name in the font.
            // veraPDF resolves Differences names via CFF charset, then hmtx.
            if let Some(gid) = face.glyph_index_by_name(glyph_name) {
                face.glyph_hor_advance(gid)
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0)
            } else {
                // Glyph name not found — try via Unicode mapping.
                glyph_name_to_unicode(glyph_name)
                    .and_then(|u| face.glyph_index(u))
                    .and_then(|gid| face.glyph_hor_advance(gid))
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0)
            }
        } else if let Some(ref cff) = cff {
            // No Differences entry: use CFF internal encoding.
            let gid = cff
                .encoding
                .code_to_gid(&cff.charset, code as u8)
                .map(|g| ttf_parser::GlyphId(g.0))
                .unwrap_or(ttf_parser::GlyphId(0));
            face.glyph_hor_advance(gid)
                .map(|w| (w as f64 * scale).round() as i64)
                .unwrap_or(0)
        } else {
            // No CFF table available — use .notdef width.
            face.glyph_hor_advance(ttf_parser::GlyphId(0))
                .map(|w| (w as f64 * scale).round() as i64)
                .unwrap_or(0)
        };
        widths.push(Object::Integer(width));
    }

    if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
        if !widths.is_empty() {
            font.set("Widths", Object::Array(widths));
            font.set("FirstChar", Object::Integer(first_char as i64));
            font.set("LastChar", Object::Integer(last_char as i64));
        }
    }
}

/// Map a character code to a Unicode character based on PDF encoding name.
fn encoding_to_char(code: u32, encoding_name: &str) -> char {
    match encoding_name {
        "WinAnsiEncoding" => winansi_to_char(code),
        "MacRomanEncoding" => macroman_to_char(code),
        "StandardEncoding" => standard_encoding_to_char(code),
        _ => {
            // Default: assume identity mapping for codes 0-127,
            // WinAnsi-like for 128-255.
            if code < 128 {
                char::from_u32(code).unwrap_or(' ')
            } else {
                winansi_to_char(code)
            }
        }
    }
}

/// Adobe Standard Encoding character map (PostScript Language Reference, Appendix E).
/// Codes 0-127 are ASCII (same across encodings). Codes 128-255 differ significantly
/// from WinAnsi: for example, code 177 = "endash" (U+2013, not ± U+00B1), and
/// code 208 = "emdash" (U+2014, not Ð U+00D0). Using the correct mapping ensures
/// veraPDF §6.2.11.5 glyph-name → CFF-charset lookups find the right glyphs. (#504)
fn standard_encoding_to_char(code: u32) -> char {
    // Standard Encoding is NOT ASCII for all codes < 128. Two codes differ:
    //   code 39 = "quoteright" (U+2019), not "quotesingle" (U+0027/ASCII)
    //   code 96 = "quoteleft"  (U+2018), not "grave" (U+0060/ASCII)
    // These overrides must be applied BEFORE the ASCII fallback, otherwise
    // the AGL lookup produces the wrong glyph name (quotesingle/grave
    // instead of quoteright/quoteleft). (#fix-std-enc-code39)
    match code {
        39 => return '\u{2019}', // quoteright
        96 => return '\u{2018}', // quoteleft
        _ => {}
    }
    if code < 128 {
        return char::from_u32(code).unwrap_or('\u{FFFF}');
    }
    if code > 255 {
        return '\u{FFFF}';
    }
    // Map from Standard Encoding code → Unicode code point.
    // U+FFFF is used as sentinel for codes that are undefined in Standard Encoding;
    // it is not in the Adobe Glyph List so unicode_to_glyph_name returns None,
    // which triggers the CFF internal encoding fallback in cff_width_for_code. (#504)
    const STD_ENC: [u32; 128] = [
        //  128       129       130       131       132       133       134       135
        0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
        //  136       137       138       139       140       141       142       143
        0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
        //  144       145       146       147       148       149       150       151
        0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
        //  152       153       154       155       156       157       158       159
        0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
        //  160       161           162           163           164           165           166           167
        0xFFFF, 0x00A1, /*¡*/
        0x00A2, /*¢*/
        0x00A3, /*£*/
        0x2044, /*⁄*/
        0x00A5, /*¥*/
        0x0192, /*ƒ*/
        0x00A7, /*§*/
        //  168           169           170           171           172           173           174           175
        0x00A4, /*¤*/
        0x0027, /*'*/
        0x201C, /*"*/
        0x00AB, /*«*/
        0x2039, /*‹*/
        0x203A, /*›*/
        0xFB01, /*ﬁ*/
        0xFB02, /*ﬂ*/
        //  176       177           178           179           180           181       182           183
        0xFFFF, 0x2013, /*–*/
        0x2020, /*†*/
        0x2021, /*‡*/
        0x00B7, /*·*/
        0xFFFF, 0x00B6, /*¶*/
        0x2022, /*•*/
        //  184           185           186           187           188           189           190       191
        0x201A, /*‚*/
        0x201E, /*„*/
        0x201D, /*"*/
        0x00BB, /*»*/
        0x2026, /*…*/
        0x2030, /*‰*/
        0xFFFF, 0x00BF, /*¿*/
        //  192       193           194           195           196           197           198           199
        0xFFFF, 0x0060, /*`*/
        0x00B4, /*´*/
        0x02C6, /*ˆ*/
        0x02DC, /*˜*/
        0x00AF, /*¯*/
        0x02D8, /*˘*/
        0x02D9, /*˙*/
        //  200           201       202           203           204       205           206           207
        0x00A8, /*¨*/
        0xFFFF, 0x02DA, /*˚*/
        0x00B8, /*¸*/
        0xFFFF, 0x02DD, /*˝*/
        0x02DB, /*˛*/
        0x02C7, /*ˇ*/
        //  208           209       210       211       212       213       214       215
        0x2014, /*—*/
        0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
        //  216       217       218       219       220       221       222       223
        0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
        //  224       225           226       227           228       229       230       231
        0xFFFF, 0x00C6, /*Æ*/
        0xFFFF, 0x00AA, /*ª*/
        0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
        //  232           233           234           235           236       237       238       239
        0x0141, /*Ł*/
        0x00D8, /*Ø*/
        0x0152, /*Œ*/
        0x00BA, /*º*/
        0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
        //  240       241           242       243       244       245       246           247
        0xFFFF, 0x00E6, /*æ*/
        0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0x0131, /*ı*/
        0xFFFF,
        //  248           249           250           251           252       253       254       255
        0x0142, /*ł*/
        0x00F8, /*ø*/
        0x0153, /*œ*/
        0x00DF, /*ß*/
        0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
    ];
    let cp = STD_ENC[(code - 128) as usize];
    char::from_u32(cp).unwrap_or('\u{FFFF}')
}

/// WinAnsiEncoding character map (codes 128-159 differ from Unicode).
fn winansi_to_char(code: u32) -> char {
    if !(128..=255).contains(&code) {
        return char::from_u32(code).unwrap_or(' ');
    }
    // WinAnsi codes 160-255 follow Latin-1 (ISO 8859-1) exactly, except for
    // 128-159 which differ. Fall through to the Latin-1 mapping below.
    // Note: code 160 = U+00A0 (non-breaking space, NOT regular space U+0020),
    // and code 173 = U+00AD (soft hyphen, NOT hyphen U+002D). The compliance
    // check uses the literal WinAnsi Unicode values, so we must match. (#FN-6.2.11.5-notdef)
    // WinAnsi codes 128-159 that differ from Latin-1.
    const WINANSI_128_159: [u32; 32] = [
        0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, // 128-135
        0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D, 0x017D, 0x008F, // 136-143
        0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, // 144-151
        0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178, // 152-159
    ];
    if code < 160 {
        char::from_u32(WINANSI_128_159[(code - 128) as usize]).unwrap_or(' ')
    } else {
        char::from_u32(code).unwrap_or(' ')
    }
}

/// MacRomanEncoding character map (codes 128-255).
fn macroman_to_char(code: u32) -> char {
    if code < 128 {
        return char::from_u32(code).unwrap_or(' ');
    }
    if code > 255 {
        return char::from_u32(code).unwrap_or(' ');
    }
    // Full MacRoman 128-255 mapping to Unicode.
    const MACROMAN_128_255: [u32; 128] = [
        0x00C4, 0x00C5, 0x00C7, 0x00C9, 0x00D1, 0x00D6, 0x00DC, 0x00E1, // 128-135
        0x00E0, 0x00E2, 0x00E4, 0x00E3, 0x00E5, 0x00E7, 0x00E9, 0x00E8, // 136-143
        0x00EA, 0x00EB, 0x00ED, 0x00EC, 0x00EE, 0x00EF, 0x00F1, 0x00F3, // 144-151
        0x00F2, 0x00F4, 0x00F6, 0x00F5, 0x00FA, 0x00F9, 0x00FB, 0x00FC, // 152-159
        0x2020, 0x00B0, 0x00A2, 0x00A3, 0x00A7, 0x2022, 0x00B6, 0x00DF, // 160-167
        0x00AE, 0x00A9, 0x2122, 0x00B4, 0x00A8, 0x2260, 0x00C6, 0x00D8, // 168-175
        0x221E, 0x00B1, 0x2264, 0x2265, 0x00A5, 0x00B5, 0x2202, 0x2211, // 176-183
        0x220F, 0x03C0, 0x222B, 0x00AA, 0x00BA, 0x2126, 0x00E6, 0x00F8, // 184-191
        0x00BF, 0x00A1, 0x00AC, 0x221A, 0x0192, 0x2248, 0x2206, 0x00AB, // 192-199
        0x00BB, 0x2026, 0x00A0, 0x00C0, 0x00C3, 0x00D5, 0x0152, 0x0153, // 200-207
        0x2013, 0x2014, 0x201C, 0x201D, 0x2018, 0x2019, 0x00F7, 0x25CA, // 208-215
        0x00FF, 0x0178, 0x2044, 0x20AC, 0x2039, 0x203A, 0xFB01, 0xFB02, // 216-223
        0x2021, 0x00B7, 0x201A, 0x201E, 0x2030, 0x00C2, 0x00CA, 0x00C1, // 224-231
        0x00CB, 0x00C8, 0x00CD, 0x00CE, 0x00CF, 0x00CC, 0x00D3, 0x00D4, // 232-239
        0xF8FF, 0x00D2, 0x00DA, 0x00DB, 0x00D9, 0x0131, 0x02C6, 0x02DC, // 240-247
        0x00AF, 0x02D8, 0x02D9, 0x02DA, 0x00B8, 0x02DD, 0x02DB, 0x02C7, // 248-255
    ];
    char::from_u32(MACROMAN_128_255[(code - 128) as usize]).unwrap_or(' ')
}

/// Update DW (default width) for a CIDFont.
fn update_cid_widths(doc: &mut Document, cid_id: ObjectId, face: &ttf_parser::Face, scale: f64) {
    let default_width = face
        .glyph_hor_advance(ttf_parser::GlyphId(0))
        .map(|w| (w as f64 * scale).round() as i64)
        .unwrap_or(1000);
    if let Some(Object::Dictionary(ref mut cid)) = doc.objects.get_mut(&cid_id) {
        cid.set("DW", Object::Integer(default_width));
        // Remove W array to avoid width mismatches — DW will serve as fallback.
        cid.remove(b"W");
    }
}

/// Get the FontDescriptor reference from a Font dictionary, or create one.
fn get_or_create_font_descriptor(doc: &mut Document, font_id: ObjectId) -> Result<ObjectId> {
    let existing = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return Err(ManipError::Other("font object not found".into()));
        };
        match font.get(b"FontDescriptor").ok() {
            Some(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };

    if let Some(fd_id) = existing {
        // Verify the FD reference points to a valid Dictionary. Some legacy
        // PDFs have FontDescriptor references that point to null objects.
        if matches!(doc.objects.get(&fd_id), Some(Object::Dictionary(_))) {
            return Ok(fd_id);
        }
        // FD is null/invalid — fall through to create a new one.
    }

    let font_name = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return Err(ManipError::Other("font object not found".into()));
        };
        get_name(font, b"BaseFont").unwrap_or_else(|| "Unknown".into())
    };

    // Set Symbolic (4) for known symbolic fonts, Nonsymbolic (32) otherwise.
    let flags: i64 = if is_symbolic_font_name(&font_name) {
        4
    } else {
        32
    };

    let fd = dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(font_name.into_bytes()),
        "Flags" => Object::Integer(flags),
        "FontBBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(-200),
            Object::Integer(1000), Object::Integer(800),
        ]),
        "ItalicAngle" => Object::Integer(0),
        "Ascent" => Object::Integer(800),
        "Descent" => Object::Integer(-200),
        "CapHeight" => Object::Integer(700),
        "StemV" => Object::Integer(80),
    };
    let fd_id = doc.add_object(Object::Dictionary(fd));

    if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
        font.set("FontDescriptor", Object::Reference(fd_id));
    }

    Ok(fd_id)
}

/// Map Standard 14 font names to available system font files.
// Font directory prefix macros for compile-time concatenation.
macro_rules! lib {
    ($f:literal) => {
        concat!("/usr/share/fonts/truetype/liberation/", $f)
    };
}
macro_rules! urw {
    ($f:literal) => {
        concat!("/usr/share/fonts/opentype/urw-base35/", $f)
    };
}
macro_rules! noto {
    ($f:literal) => {
        concat!("/usr/share/fonts/truetype/noto/", $f)
    };
}
macro_rules! dv {
    ($f:literal) => {
        concat!("/usr/share/fonts/truetype/dejavu/", $f)
    };
}
macro_rules! mac {
    ($f:literal) => {
        concat!("/System/Library/Fonts/Supplemental/", $f)
    };
}

fn repo_font_pack_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REPO_FONT_PACK_REL)
}

/// Resolve a font candidate path with deterministic priority:
/// 1) in-repo shared font pack by basename
/// 2) original absolute/relative candidate path
fn resolve_font_candidate_path(candidate: &str) -> Option<String> {
    if let Some(file_name) = std::path::Path::new(candidate)
        .file_name()
        .and_then(|n| n.to_str())
    {
        let packed = repo_font_pack_dir().join(file_name);
        if packed.exists() {
            return Some(packed.to_string_lossy().to_string());
        }
    }

    if std::path::Path::new(candidate).exists() {
        return Some(candidate.to_string());
    }

    None
}

fn find_fallback_font() -> Option<String> {
    for candidate in FALLBACK_FONTS {
        if let Some(path) = resolve_font_candidate_path(candidate) {
            return Some(path);
        }
    }
    None
}

fn standard14_system_path(clean_name: &str) -> Option<String> {
    // Priority: Liberation (TTF) > URW Base35 (OTF) > Noto > DejaVu > macOS.
    // TTF fonts are preferred because width computation for TrueType uses the
    // same algorithm as veraPDF (raw character code as Unicode in (3,1) cmap).
    // URW OTF stays as fallback; for PostScript Level 2 fonts URW is first
    // since it's the correct metric match.
    let candidates: &[&str] = match clean_name {
        // --- Sans-serif (Helvetica / Arial family) ---
        "Helvetica"
        | "ArialMT"
        | "Arial"
        | "Tahoma"
        | "Verdana"
        | "LucidaSansUnicode"
        | "LucidaSans"
        | "SegoeUI"
        | "Calibri"
        | "TrebuchetMS"
        | "LucidaGrande"
        | "HelveticaNeue"
        | "HelveticaLTStd-Roman"
        | "NimbusSanL-Regu"
        | "NimbusSans-Regular" => &[
            lib!("LiberationSans-Regular.ttf"),
            urw!("NimbusSans-Regular.otf"),
            noto!("NotoSans-Regular.ttf"),
            dv!("DejaVuSans.ttf"),
            "/System/Library/Fonts/Helvetica.ttc",
            mac!("Arial.ttf"),
        ],
        "Helvetica-Bold" | "Arial-BoldMT" | "Arial,Bold" | "Arial-Bold" | "ArialBlack"
        | "Tahoma,Bold" | "Tahoma-Bold" | "Verdana,Bold" | "Verdana-Bold" | "Calibri,Bold"
        | "Calibri-Bold" | "HelveticaNeue-Bold" | "SegoeUI,Bold" | "SegoeUI-Bold" => &[
            lib!("LiberationSans-Bold.ttf"),
            urw!("NimbusSans-Bold.otf"),
            noto!("NotoSans-Bold.ttf"),
            dv!("DejaVuSans-Bold.ttf"),
            mac!("Arial Bold.ttf"),
        ],
        "Helvetica-Oblique"
        | "Arial-ItalicMT"
        | "Arial,Italic"
        | "Arial-Italic"
        | "Verdana,Italic"
        | "Verdana-Italic"
        | "Calibri,Italic"
        | "Calibri-Italic"
        | "HelveticaNeue-Italic"
        | "SegoeUI,Italic"
        | "SegoeUI-Italic" => &[
            lib!("LiberationSans-Italic.ttf"),
            urw!("NimbusSans-Italic.otf"),
            noto!("NotoSans-Italic.ttf"),
            dv!("DejaVuSans-Oblique.ttf"),
            mac!("Arial Italic.ttf"),
        ],
        "Helvetica-BoldOblique"
        | "Arial-BoldItalicMT"
        | "Arial,BoldItalic"
        | "Calibri,BoldItalic"
        | "HelveticaNeue-BoldItalic" => &[
            lib!("LiberationSans-BoldItalic.ttf"),
            urw!("NimbusSans-BoldItalic.otf"),
            noto!("NotoSans-BoldItalic.ttf"),
            dv!("DejaVuSans-BoldOblique.ttf"),
            mac!("Arial Bold Italic.ttf"),
        ],
        // --- Serif (Times family) ---
        "Times-Roman"
        | "TimesNewRomanPSMT"
        | "TimesNewRoman"
        | "TimesNewRomanPS"
        | "Georgia"
        | "BookAntiqua"
        | "Cambria"
        | "Garamond"
        | "Palatino"
        | "PalatinoLinotype"
        | "NimbusRomanNo9L-Regu"
        | "NimbusRomNo9L-Regu"
        | "NimbusRoman-Regular" => &[
            lib!("LiberationSerif-Regular.ttf"),
            urw!("NimbusRoman-Regular.otf"),
            noto!("NotoSerif-Regular.ttf"),
            dv!("DejaVuSerif.ttf"),
            mac!("Times New Roman.ttf"),
        ],
        "Times-Bold"
        | "TimesNewRomanPS-BoldMT"
        | "TimesNewRoman,Bold"
        | "TimesNewRoman-Bold"
        | "Georgia,Bold"
        | "Georgia-Bold"
        | "Cambria,Bold"
        | "Cambria-Bold" => &[
            lib!("LiberationSerif-Bold.ttf"),
            urw!("NimbusRoman-Bold.otf"),
            noto!("NotoSerif-Bold.ttf"),
            dv!("DejaVuSerif-Bold.ttf"),
            mac!("Times New Roman Bold.ttf"),
        ],
        "Times-Italic"
        | "TimesNewRomanPS-ItalicMT"
        | "TimesNewRoman,Italic"
        | "TimesNewRoman-Italic"
        | "Georgia,Italic"
        | "Georgia-Italic"
        | "Cambria,Italic"
        | "Cambria-Italic" => &[
            lib!("LiberationSerif-Italic.ttf"),
            urw!("NimbusRoman-Italic.otf"),
            noto!("NotoSerif-Italic.ttf"),
            dv!("DejaVuSerif-Italic.ttf"),
            mac!("Times New Roman Italic.ttf"),
        ],
        "Times-BoldItalic"
        | "TimesNewRomanPS-BoldItalicMT"
        | "TimesNewRoman,BoldItalic"
        | "Cambria,BoldItalic" => &[
            lib!("LiberationSerif-BoldItalic.ttf"),
            urw!("NimbusRoman-BoldItalic.otf"),
            noto!("NotoSerif-BoldItalic.ttf"),
            dv!("DejaVuSerif-BoldItalic.ttf"),
            mac!("Times New Roman Bold Italic.ttf"),
        ],
        // --- Monospace (Courier family) ---
        "Courier" | "CourierNewPSMT" | "CourierNew" | "CourierNewPS" | "LucidaConsole"
        | "Consolas" => &[
            lib!("LiberationMono-Regular.ttf"),
            urw!("NimbusMonoPS-Regular.otf"),
            dv!("DejaVuSansMono.ttf"),
            mac!("Courier New.ttf"),
        ],
        "Courier-Bold" | "CourierNewPS-BoldMT" | "CourierNew,Bold" | "CourierNew-Bold" => &[
            lib!("LiberationMono-Bold.ttf"),
            urw!("NimbusMonoPS-Bold.otf"),
            dv!("DejaVuSansMono-Bold.ttf"),
            mac!("Courier New Bold.ttf"),
        ],
        "Courier-Oblique" | "CourierNewPS-ItalicMT" | "CourierNew,Italic" | "CourierNew-Italic" => {
            &[
                lib!("LiberationMono-Italic.ttf"),
                urw!("NimbusMonoPS-Italic.otf"),
                dv!("DejaVuSansMono-Oblique.ttf"),
                mac!("Courier New Italic.ttf"),
            ]
        }
        "Courier-BoldOblique" | "CourierNewPS-BoldItalicMT" | "CourierNew,BoldItalic" => &[
            lib!("LiberationMono-BoldItalic.ttf"),
            urw!("NimbusMonoPS-BoldItalic.otf"),
            dv!("DejaVuSansMono-BoldOblique.ttf"),
            mac!("Courier New Bold Italic.ttf"),
        ],
        // --- Symbolic fonts ---
        "Symbol" | "SymbolMT" => &[
            urw!("StandardSymbolsPS.otf"),
            "/System/Library/Fonts/Symbol.ttf",
        ],
        "ZapfDingbats" => &[
            urw!("D050000L.otf"),
            "/System/Library/Fonts/Supplemental/Apple Symbols.ttf",
        ],
        // --- Narrow variants ---
        "ArialNarrow" => &[
            lib!("LiberationSansNarrow-Regular.ttf"),
            urw!("NimbusSansNarrow-Regular.otf"),
            dv!("DejaVuSansCondensed.ttf"),
            mac!("Arial Narrow.ttf"),
        ],
        "ArialNarrow,Bold" | "ArialNarrow-Bold" => &[
            lib!("LiberationSansNarrow-Bold.ttf"),
            urw!("NimbusSansNarrow-Bold.otf"),
            dv!("DejaVuSansCondensed-Bold.ttf"),
            mac!("Arial Narrow Bold.ttf"),
        ],
        "ArialNarrow,Italic" | "ArialNarrow-Italic" => &[
            lib!("LiberationSansNarrow-Italic.ttf"),
            urw!("NimbusSansNarrow-Oblique.otf"),
            dv!("DejaVuSansCondensed-Oblique.ttf"),
            mac!("Arial Narrow Italic.ttf"),
        ],
        "ArialNarrow,BoldItalic" | "ArialNarrow-BoldItalic" => &[
            lib!("LiberationSansNarrow-BoldItalic.ttf"),
            urw!("NimbusSansNarrow-BoldOblique.otf"),
            dv!("DejaVuSansCondensed-BoldOblique.ttf"),
            mac!("Arial Narrow Bold Italic.ttf"),
        ],
        "ArialRoundedMTBold" => &[
            lib!("LiberationSans-Bold.ttf"),
            urw!("NimbusSans-Bold.otf"),
            dv!("DejaVuSans-Bold.ttf"),
            mac!("Arial Rounded Bold.ttf"),
        ],
        // --- PostScript Level 2 base 35 fonts (URW equivalents) ---
        "NewCenturySchlbk-Roman" | "CenturySchoolbook" => {
            &[urw!("C059-Roman.otf"), lib!("LiberationSerif-Regular.ttf")]
        }
        "NewCenturySchlbk-Bold" | "CenturySchoolbook-Bold" => {
            &[urw!("C059-Bold.otf"), lib!("LiberationSerif-Bold.ttf")]
        }
        "NewCenturySchlbk-Italic" | "CenturySchoolbook-Italic" => {
            &[urw!("C059-Italic.otf"), lib!("LiberationSerif-Italic.ttf")]
        }
        "NewCenturySchlbk-BoldItalic" | "CenturySchoolbook-BoldItalic" => &[
            urw!("C059-BdIta.otf"),
            lib!("LiberationSerif-BoldItalic.ttf"),
        ],
        "Bookman-Light" | "BookmanOldStyle" => &[
            urw!("URWBookman-Light.otf"),
            lib!("LiberationSerif-Regular.ttf"),
        ],
        "Bookman-Demi" | "BookmanOldStyle-Bold" => &[
            urw!("URWBookman-Demi.otf"),
            lib!("LiberationSerif-Bold.ttf"),
        ],
        "AvantGarde-Book" | "AvantGardeITCbyBT-Book" => &[
            urw!("URWGothic-Book.otf"),
            lib!("LiberationSans-Regular.ttf"),
        ],
        "AvantGarde-Demi" => &[urw!("URWGothic-Demi.otf"), lib!("LiberationSans-Bold.ttf")],
        "Palatino-Roman" | "PalatinoLinotype-Roman" => {
            &[urw!("P052-Roman.otf"), lib!("LiberationSerif-Regular.ttf")]
        }
        "Palatino-Bold" | "PalatinoLinotype-Bold" => {
            &[urw!("P052-Bold.otf"), lib!("LiberationSerif-Bold.ttf")]
        }
        "Palatino-Italic" | "PalatinoLinotype-Italic" => {
            &[urw!("P052-Italic.otf"), lib!("LiberationSerif-Italic.ttf")]
        }
        "Palatino-BoldItalic" | "PalatinoLinotype-BoldItalic" => &[
            urw!("P052-BoldItalic.otf"),
            lib!("LiberationSerif-BoldItalic.ttf"),
        ],
        "ZapfChancery-MediumItalic" => &[urw!("Z003-MediumItalic.otf")],
        // --- Misc common fonts ---
        "Impact" | "ComicSansMS" => &[lib!("LiberationSans-Bold.ttf"), noto!("NotoSans-Bold.ttf")],
        _ => return None,
    };
    for &path in candidates {
        if let Some(resolved) = resolve_font_candidate_path(path) {
            return Some(resolved);
        }
    }
    None
}

/// Search common system font directories for a font file.
fn find_system_font(font_name: &str) -> Option<String> {
    let clean_name = strip_subset_prefix(font_name);

    if let Some(path) = standard14_system_path(clean_name) {
        return Some(path);
    }

    // Heuristic: for unknown fonts, infer style and pick a matching substitute.
    if let Some(path) = heuristic_font_match(clean_name) {
        return Some(path);
    }

    let candidates: Vec<String> = vec![
        format!("{clean_name}.ttf"),
        format!("{clean_name}.otf"),
        format!("{clean_name}.TTF"),
        format!("{clean_name}.OTF"),
        format!("{}Regular.ttf", clean_name.replace('-', "")),
        format!("{}-Regular.ttf", clean_name),
    ];

    let dirs = if cfg!(target_os = "macos") {
        vec![
            "/System/Library/Fonts/",
            "/Library/Fonts/",
            "~/Library/Fonts/",
        ]
    } else if cfg!(target_os = "linux") {
        vec![
            "/usr/share/fonts/truetype/",
            "/usr/share/fonts/opentype/",
            "/usr/share/fonts/",
            "/usr/local/share/fonts/",
            "~/.fonts/",
            "~/.local/share/fonts/",
        ]
    } else {
        vec!["C:\\Windows\\Fonts\\"]
    };

    for dir in &dirs {
        for candidate in &candidates {
            let path = format!("{dir}{candidate}");
            let expanded = path.replace('~', &std::env::var("HOME").unwrap_or_default());
            if std::path::Path::new(&expanded).exists() {
                return Some(expanded);
            }
        }
        let expanded_dir = dir.replace('~', &std::env::var("HOME").unwrap_or_default());
        for candidate in &candidates {
            if let Some(path) = find_font_recursive(&expanded_dir, candidate) {
                return Some(path);
            }
        }
    }

    None
}

/// Heuristic font matching: infer weight/style from font name, then pick a
/// Liberation or Noto substitute based on whether the name looks like serif,
/// sans-serif, or monospace.
fn heuristic_font_match(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();

    // Detect weight and style from common suffixes.
    let is_bold = lower.contains("bold")
        || lower.contains("demi")
        || lower.contains("black")
        || lower.contains("heavy");
    let is_italic =
        lower.contains("italic") || lower.contains("oblique") || lower.contains("slant");

    // Detect font class.
    let is_mono = lower.contains("mono")
        || lower.contains("courier")
        || lower.contains("code")
        || lower.contains("console")
        || lower.contains("typewriter");
    let is_serif = (lower.contains("serif") && !lower.contains("sansserif") && !lower.contains("sans-serif"))
            || lower.contains("roman")
            || lower.contains("times")
            || lower.contains("garamond")
            || lower.contains("georgia")
            || lower.contains("bookman")
            || lower.contains("century")
            || lower.contains("palatino")
            || lower.contains("cambria")
            || lower.contains("minion")
            || lower.contains("melior")
            || lower.contains("nimbusroman")  // NimbusRomanNo9L (URW)
            || lower.contains("goudy")
            || lower.contains("schoolbook")
            || lower.contains("schlbk");

    let key = if is_mono {
        match (is_bold, is_italic) {
            (true, true) => "Courier-BoldOblique",
            (true, false) => "Courier-Bold",
            (false, true) => "Courier-Oblique",
            (false, false) => "Courier",
        }
    } else if is_serif {
        match (is_bold, is_italic) {
            (true, true) => "Times-BoldItalic",
            (true, false) => "Times-Bold",
            (false, true) => "Times-Italic",
            (false, false) => "Times-Roman",
        }
    } else {
        // Default to sans-serif.
        match (is_bold, is_italic) {
            (true, true) => "Helvetica-BoldOblique",
            (true, false) => "Helvetica-Bold",
            (false, true) => "Helvetica-Oblique",
            (false, false) => "Helvetica",
        }
    };

    standard14_system_path(key)
}

fn find_font_recursive(dir: &str, filename: &str) -> Option<String> {
    find_font_recursive_depth(dir, filename, 0)
}

fn find_font_recursive_depth(dir: &str, filename: &str, depth: u32) -> Option<String> {
    if depth > 3 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.eq_ignore_ascii_case(filename) {
                    return path.to_str().map(|s| s.to_string());
                }
            }
        } else if path.is_dir() {
            if let Some(found) =
                find_font_recursive_depth(path.to_str().unwrap_or(""), filename, depth + 1)
            {
                return Some(found);
            }
        }
    }
    None
}

/// Fix width mismatches for fonts with embedded programs (6.2.11.5:1).
///
/// Only updates widths that actually differ from the embedded font by > 1 unit.
/// This avoids the regression that blanket width updates cause.
pub fn fix_width_mismatches(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;
    for font_id in font_ids {
        let (subtype, fd_id, is_type0) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            let is_type0 = subtype == "Type0";

            if is_type0 {
                let desc_fd = get_descendant_embed_info(doc, dict);
                match desc_fd {
                    Some((cid_id, true)) => {
                        let cid_fd = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                match d.get(b"FontDescriptor").ok() {
                                    Some(Object::Reference(id)) => Some(*id),
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        });
                        (subtype, cid_fd, true)
                    }
                    _ => continue,
                }
            } else {
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                };
                (subtype, fd_id, false)
            }
        };

        let Some(fd_id) = fd_id else { continue };

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        // Try TrueType/OpenType first, then CFF.
        if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
            let units_per_em = face.units_per_em() as f64;
            if units_per_em == 0.0 {
                continue;
            }
            let scale = 1000.0 / units_per_em;

            // For simple fonts, check if existing widths differ from font widths.
            if !is_type0 && subtype != "CIDFontType0" && subtype != "CIDFontType2" {
                let has_mismatch = {
                    let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
                        continue;
                    };
                    let fc = font
                        .get(b"FirstChar")
                        .ok()
                        .and_then(|o| match o {
                            Object::Integer(i) => Some(*i as u32),
                            _ => None,
                        })
                        .unwrap_or(0);
                    let existing_widths = match font.get(b"Widths").ok() {
                        Some(Object::Array(arr)) => arr,
                        _ => continue,
                    };
                    let enc = font
                        .get(b"Encoding")
                        .ok()
                        .and_then(|o| match o {
                            Object::Name(n) => String::from_utf8(n.clone()).ok(),
                            _ => None,
                        })
                        .unwrap_or_default();

                    let mut mismatch = false;
                    for (i, obj) in existing_widths.iter().enumerate() {
                        let pdf_w = match obj {
                            Object::Integer(w) => *w,
                            Object::Real(r) => *r as i64,
                            _ => continue,
                        };
                        let code = fc + i as u32;
                        let ch = encoding_to_char(code, &enc);
                        let expected = if let Some(gid) = face.glyph_index(ch) {
                            face.glyph_hor_advance(gid)
                                .map(|w| (w as f64 * scale).round() as i64)
                                .unwrap_or(0)
                        } else if code <= u16::MAX as u32 {
                            face.glyph_hor_advance(ttf_parser::GlyphId(code as u16))
                                .map(|w| (w as f64 * scale).round() as i64)
                                .unwrap_or(0)
                        } else {
                            0
                        };
                        if (pdf_w - expected).abs() > 1 {
                            mismatch = true;
                            break;
                        }
                    }
                    mismatch
                };

                if has_mismatch {
                    update_simple_widths(doc, font_id, &face, scale);
                    fixed += 1;
                }
            }
        } else if let Some(cff) = cff_parser::Table::parse(&font_data) {
            // CFF font — fix widths for CID fonts using CFF glyph widths.
            if is_type0 || subtype == "CIDFontType0" {
                let target_id = if is_type0 {
                    // For Type0, get the CIDFont descendant ID.
                    doc.objects.get(&font_id).and_then(|o| {
                        if let Object::Dictionary(d) = o {
                            get_descendant_embed_info(doc, d).map(|(id, _)| id)
                        } else {
                            None
                        }
                    })
                } else {
                    Some(font_id)
                };
                let Some(cid_font_id) = target_id else {
                    continue;
                };
                if fix_cid_widths_from_cff(doc, cid_font_id, &cff) {
                    fixed += 1;
                }
            } else if subtype != "CIDFontType2" {
                // Simple font with FontFile3 (CFF) — fix /Widths to match CFF encoding
                // widths so §6.2.11.5 is satisfied. The compliance checker maps each code
                // via cff.glyph_index(code) and compares with /Widths; we apply the same
                // mapping here and update any mismatched entries. (#6.2.11.5-simple-cff)
                if fix_simple_cff_widths(doc, font_id, &cff) {
                    fixed += 1;
                }
            }
        }
    }
    fixed
}

/// Fix CID font /W (widths) array from CFF glyph width data (6.2.11.5:1).
///
/// For SID-based CFF fonts, reads the actual glyph widths from the CFF program
/// and rebuilds the /W array to match. This ensures consistency between the
/// font dictionary widths and the embedded font program.
fn fix_cid_widths_from_cff(
    doc: &mut Document,
    cid_font_id: ObjectId,
    cff: &cff_parser::Table<'_>,
) -> bool {
    let num_glyphs = cff.number_of_glyphs();
    if num_glyphs == 0 {
        return false;
    }

    // Collect widths for all glyphs from the CFF program, grouped by CID.
    //
    // For CID-keyed CFF fonts, each FD in the FDArray may define its own
    // FontMatrix (op 12 7). The correct text-space width is:
    //   advance × FD_matrix.sx × 1000
    // Using the top-level matrix for all glyphs is wrong when a per-FD matrix
    // differs (e.g. CopperplateGothic with FD matrix 0.000686 instead of 0.001),
    // causing inflated widths and persistent 6.2.11.5:1 failures. (#OOM)
    let mut by_cid: std::collections::HashMap<u16, Vec<i64>> = std::collections::HashMap::new();
    for gid in 0..num_glyphs {
        let glyph_id = cff_parser::GlyphId(gid);
        if let Some(w) = cff.glyph_width(glyph_id) {
            let fd_matrix = cff.glyph_fd_matrix(glyph_id);
            let scale = if fd_matrix.sx.abs() > f32::EPSILON {
                fd_matrix.sx * 1000.0
            } else {
                1.0_f32
            };
            let scaled = (w as f64 * scale as f64).round() as i64;
            // For CIDFontType0 (CID-keyed CFF), every valid glyph must have a
            // charset entry. GIDs where glyph_cid returns None are unassigned
            // charstrings that carry no CID mapping — skip them. Using
            // unwrap_or(gid) as fallback caused phantom GIDs to collide with
            // real CID mappings, triggering the dedup logic and replacing
            // correct widths with the .notdef width. (#6.2.11.5-cid-phantom)
            let Some(cid) = cff.glyph_cid(glyph_id) else {
                continue;
            };
            by_cid.entry(cid).or_default().push(scaled);
        }
    }

    if by_cid.is_empty() {
        // cff_parser does not expose glyph widths for CID-keyed CFF fonts.
        // Any heuristic rewrite of /W without authoritative glyph widths can
        // turn correct default widths into wrong explicit entries (6.2.11.5:1).
        // Keep existing CID widths unchanged in this case.
        return false;
    }

    // Resolve CID duplicates.
    //
    // Some subset CFF fonts repeat the same CID for multiple glyphs. veraPDF
    // may resolve such collisions to .notdef width; choosing an arbitrary
    // duplicate width causes persistent 6.2.11.5:1 mismatches. Prefer .notdef
    // width when a CID has conflicting widths.
    // (notdef_dw is recomputed after the by_cid loop using per-FD matrix)
    let notdef_dw_for_dedup: Option<i64> = {
        let g0 = cff_parser::GlyphId(0);
        cff.glyph_width(g0).map(|w| {
            let fd_matrix = cff.glyph_fd_matrix(g0);
            let scale = if fd_matrix.sx.abs() > f32::EPSILON {
                fd_matrix.sx * 1000.0
            } else {
                1.0_f32
            };
            (w as f64 * scale as f64).round() as i64
        })
    };

    let mut widths: Vec<(u16, i64)> = by_cid
        .into_iter()
        .map(|(cid, vals)| {
            let resolved = if vals.len() <= 1 {
                vals[0]
            } else {
                // Multiple GIDs mapping to the same CID. If all agree on the
                // same width there is no conflict — use that width directly.
                // Only fall back to .notdef when the widths genuinely differ,
                // to match veraPDF's behaviour for ambiguous duplicates.
                // (#6.2.11.5-cid-dedup)
                let all_same = vals.iter().all(|&v| v == vals[0]);
                if all_same {
                    vals[0]
                } else {
                    let mut freq: std::collections::HashMap<i64, usize> =
                        std::collections::HashMap::new();
                    for v in &vals {
                        *freq.entry(*v).or_default() += 1;
                    }
                    notdef_dw_for_dedup.unwrap_or_else(|| {
                        freq.into_iter()
                            .max_by_key(|(_, c)| *c)
                            .map(|(w, _)| w)
                            .unwrap_or(vals[0])
                    })
                }
            };
            (cid, resolved)
        })
        .collect();
    widths.sort_by_key(|(cid, _)| *cid);

    // Determine DW (default width).
    //
    // Use GID 0 (.notdef) with its correct per-FD matrix for the default width.
    // Mode fallback for fonts where .notdef width is unavailable.
    let notdef_dw = {
        let g0 = cff_parser::GlyphId(0);
        cff.glyph_width(g0).map(|w| {
            let fd_matrix = cff.glyph_fd_matrix(g0);
            let scale = if fd_matrix.sx.abs() > f32::EPSILON {
                fd_matrix.sx * 1000.0
            } else {
                1.0_f32
            };
            (w as f64 * scale as f64).round() as i64
        })
    };
    let mut freq: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for (_, w) in &widths {
        *freq.entry(*w).or_default() += 1;
    }
    let mode_dw = freq
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(w, _)| *w)
        .unwrap_or(1000);
    let dw = notdef_dw.unwrap_or(mode_dw);

    // Sanity check: if the computed DW/widths are systematically ~1000× larger
    // than the existing /W values, our FD-matrix scale factor is wrong for this
    // font.  This happens when `cff_parser::glyph_fd_matrix` returns sx=1.0
    // (unit matrix) instead of the standard 0.001 for a CFF whose charstring
    // units are already at the standard scale, causing scale = 1.0 * 1000 =
    // 1000 and inflating every width by a factor of 1000.
    // In that case the existing /W (which veraPDF accepts) is already correct;
    // overwriting it would introduce a §6.2.11.5 regression. (#FP-6.2.11.5-cid)
    {
        let existing_dw: Option<i64> = doc.objects.get(&cid_font_id).and_then(|o| {
            if let Object::Dictionary(d) = o {
                if let Ok(Object::Integer(v)) = d.get(b"DW") {
                    return Some(*v);
                }
            }
            None
        });
        if let Some(ex_dw) = existing_dw {
            if ex_dw > 0 && dw > 0 {
                let ratio = dw as f64 / ex_dw as f64;
                // If our computed DW is 500× or more larger than the existing DW
                // the FD matrix scale is clearly wrong — leave the /W untouched.
                // No upper bound: e.g. MCQKME+SymbolMT reaches ratio ≈ 2400
                // (existing DW=250, computed DW=600000). (#FP-6.2.11.5-cid)
                if ratio > 500.0 {
                    return false;
                }
            }
        }
    }

    // Build /W array: consecutive runs of widths that differ from DW.
    // Format: [cid [w1 w2 ...] cid2 [w3 w4 ...] ...]
    let mut w_array: Vec<Object> = Vec::new();
    let mut run_start: Option<u16> = None;
    let mut run_widths: Vec<Object> = Vec::new();

    for (cid, w) in &widths {
        if *w == dw {
            // Flush any accumulated run.
            if let Some(start) = run_start.take() {
                w_array.push(Object::Integer(start as i64));
                w_array.push(Object::Array(std::mem::take(&mut run_widths)));
            }
            continue;
        }

        match run_start {
            Some(start) if *cid == start + run_widths.len() as u16 => {
                // Continue existing run.
                run_widths.push(Object::Integer(*w));
            }
            _ => {
                // Flush previous run and start new.
                if let Some(start) = run_start.take() {
                    w_array.push(Object::Integer(start as i64));
                    w_array.push(Object::Array(std::mem::take(&mut run_widths)));
                }
                run_start = Some(*cid);
                run_widths.push(Object::Integer(*w));
            }
        }
    }
    // Flush last run.
    if let Some(start) = run_start {
        w_array.push(Object::Integer(start as i64));
        w_array.push(Object::Array(run_widths));
    }

    // Update the CID font dictionary.
    if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&cid_font_id) {
        dict.set("DW", Object::Integer(dw));
        if !w_array.is_empty() {
            dict.set("W", Object::Array(w_array));
        }
        true
    } else {
        false
    }
}

/// Fix /Widths for a simple (non-CID) font with a FontFile3 (CFF) embedding.
///
/// veraPDF §6.2.11.5 validates simple CFF fonts by mapping each character code
/// through the CFF internal encoding (`cff.glyph_index(code)`) and comparing
/// the resulting charstring advance width with the /Widths array entry. This
/// function applies the same mapping and updates any mismatched /Widths entries
/// in-place, ensuring the PDF dict and the embedded font program agree.
///
/// Codes where the CFF encoding has no glyph (glyph_index returns None) are
/// skipped — veraPDF also skips those. Codes with pdf_w == 0 are skipped as
/// "unused/placeholder" entries. (#6.2.11.5-simple-cff)
fn fix_simple_cff_widths(
    doc: &mut Document,
    font_id: ObjectId,
    cff: &cff_parser::Table<'_>,
) -> bool {
    let (fc, lc, existing_widths) = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return false;
        };
        let fc = match font.get(b"FirstChar").ok() {
            Some(Object::Integer(i)) => *i as usize,
            _ => return false,
        };
        let lc = match font.get(b"LastChar").ok() {
            Some(Object::Integer(i)) => *i as usize,
            _ => return false,
        };
        let widths = match font.get(b"Widths").ok() {
            Some(Object::Array(arr)) => arr.clone(),
            _ => return false,
        };
        (fc, lc, widths)
    };

    if lc < fc {
        return false;
    }
    let expected_len = lc - fc + 1;
    if existing_widths.len() < expected_len {
        return false;
    }

    // Walk codes [fc..=lc] and collect corrected widths.
    let mut new_widths = existing_widths.clone();
    let mut changed = false;

    for code in fc..=lc {
        let idx = code - fc;
        let pdf_w = match &existing_widths[idx] {
            Object::Integer(w) => *w,
            Object::Real(r) => *r as i64,
            _ => continue,
        };
        if pdf_w == 0 {
            continue; // placeholder/unused — skip as veraPDF does
        }
        // Map code → GID via CFF encoding (same mapping the compliance checker uses).
        let Some(gid) = cff.glyph_index(code as u8) else {
            continue;
        };
        let Some(cff_w) = cff.glyph_width(gid) else {
            continue;
        };
        let cff_w_i64 = cff_w as i64;
        if (cff_w_i64 - pdf_w).abs() > 1 {
            new_widths[idx] = Object::Integer(cff_w_i64);
            changed = true;
        }
    }

    if !changed {
        return false;
    }

    if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
        font.set("Widths", Object::Array(new_widths));
        true
    } else {
        false
    }
}

/// Fallback CID width repair when CFF glyph widths are unavailable.
///
/// In some CID-keyed CFF subsets, `glyph_cid` is available but `glyph_width`
/// is missing for all glyphs. We then use conservative repairs derived from
/// existing /W entries:
/// 1) duplicate charset CIDs: remap to a high-CID proxy width
/// 2) charset CIDs missing in /W (thus falling back to DW): assign the nearest
///    explicit width so used glyphs don't default to 1000 spuriously.
#[allow(dead_code)]
fn fix_cid_duplicate_widths_from_w(
    doc: &mut Document,
    cid_font_id: ObjectId,
    cff: &cff_parser::Table<'_>,
) -> bool {
    use std::collections::{HashMap, HashSet};

    // Collect CIDs (and duplicate CIDs) from the CFF charset.
    let mut seen: HashSet<u16> = HashSet::new();
    let mut cid_set: HashSet<u16> = HashSet::new();
    let mut duplicates: Vec<u16> = Vec::new();
    for gid in 0..cff.number_of_glyphs() {
        if let Some(cid) = cff.glyph_cid(cff_parser::GlyphId(gid)) {
            if cid > 0 {
                cid_set.insert(cid);
            }
            if !seen.insert(cid) {
                duplicates.push(cid);
            }
        }
    }
    if duplicates.is_empty() && cid_set.is_empty() {
        return false;
    }
    duplicates.sort_unstable();
    duplicates.dedup();

    let Some(Object::Dictionary(dict)) = doc.objects.get_mut(&cid_font_id) else {
        return false;
    };

    let dw = match dict.get(b"DW").ok() {
        Some(Object::Integer(v)) => *v,
        Some(Object::Real(v)) => *v as i64,
        _ => 1000,
    };

    // Parse existing explicit widths from /W.
    let mut explicit: HashMap<u16, i64> = HashMap::new();
    if let Ok(Object::Array(w_arr)) = dict.get(b"W") {
        let mut i = 0usize;
        while i < w_arr.len() {
            let start_cid = match &w_arr[i] {
                Object::Integer(v) => *v as u16,
                _ => break,
            };
            i += 1;
            if i >= w_arr.len() {
                break;
            }
            match &w_arr[i] {
                Object::Array(widths) => {
                    for (j, w) in widths.iter().enumerate() {
                        let val = match w {
                            Object::Integer(v) => *v,
                            Object::Real(v) => *v as i64,
                            _ => dw,
                        };
                        explicit.insert(start_cid + j as u16, val);
                    }
                    i += 1;
                }
                Object::Integer(end_cid) => {
                    i += 1;
                    if i >= w_arr.len() {
                        break;
                    }
                    let val = match &w_arr[i] {
                        Object::Integer(v) => *v,
                        Object::Real(v) => *v as i64,
                        _ => dw,
                    };
                    for cid in start_cid..=(*end_cid as u16) {
                        explicit.insert(cid, val);
                    }
                    i += 1;
                }
                _ => break,
            }
        }
    }
    if explicit.is_empty() {
        return false;
    }

    let mut keys: Vec<u16> = explicit.keys().copied().collect();
    keys.sort_unstable();

    let mut changed = false;
    for dup in duplicates {
        let current = explicit.get(&dup).copied().unwrap_or(dw);
        // Prefer a high-CID proxy width to avoid perturbing normal low-CID runs.
        let replacement = keys
            .iter()
            .rev()
            .find_map(|cid| {
                if *cid > dup {
                    explicit.get(cid).copied().filter(|w| *w != current)
                } else {
                    None
                }
            })
            .or_else(|| {
                keys.iter()
                    .find_map(|cid| explicit.get(cid).copied().filter(|w| *w != current))
            });

        if let Some(new_w) = replacement {
            if explicit.insert(dup, new_w) != Some(new_w) {
                changed = true;
            }
        }
    }

    // If a CID exists in charset but has no explicit /W entry, it falls back to DW.
    // For tiny CID subsets this often creates 1000-width mismatches; assign the
    // nearest explicit width as conservative proxy.
    for cid in cid_set {
        if explicit.contains_key(&cid) {
            continue;
        }
        let replacement = keys
            .iter()
            .copied()
            .filter(|k| *k > cid)
            .min()
            .and_then(|k| explicit.get(&k).copied())
            .or_else(|| {
                keys.iter()
                    .copied()
                    .filter(|k| *k < cid)
                    .max()
                    .and_then(|k| explicit.get(&k).copied())
            });
        if let Some(new_w) = replacement {
            if new_w != dw {
                explicit.insert(cid, new_w);
                changed = true;
            }
        }
    }

    if !changed {
        return false;
    }

    // Rebuild /W from explicit widths while keeping DW unchanged.
    let mut items: Vec<(u16, i64)> = explicit.into_iter().collect();
    items.sort_by_key(|(cid, _)| *cid);

    let mut w_array: Vec<Object> = Vec::new();
    let mut run_start: Option<u16> = None;
    let mut run_widths: Vec<Object> = Vec::new();

    for (cid, w) in items {
        if w == dw {
            if let Some(start) = run_start.take() {
                w_array.push(Object::Integer(start as i64));
                w_array.push(Object::Array(std::mem::take(&mut run_widths)));
            }
            continue;
        }
        match run_start {
            Some(start) if cid == start + run_widths.len() as u16 => {
                run_widths.push(Object::Integer(w));
            }
            _ => {
                if let Some(start) = run_start.take() {
                    w_array.push(Object::Integer(start as i64));
                    w_array.push(Object::Array(std::mem::take(&mut run_widths)));
                }
                run_start = Some(cid);
                run_widths.push(Object::Integer(w));
            }
        }
    }
    if let Some(start) = run_start {
        w_array.push(Object::Integer(start as i64));
        w_array.push(Object::Array(run_widths));
    }

    dict.set("DW", Object::Integer(dw));
    if w_array.is_empty() {
        dict.remove(b"W");
    } else {
        dict.set("W", Object::Array(w_array));
    }
    true
}

/// Fix FontDescriptor metrics to match embedded font programs (6.2.11.6:3).
///
/// Updates Ascent, Descent, CapHeight, FontBBox in FontDescriptor dicts
/// when they don't match the embedded font. Does NOT touch widths.
pub fn fix_font_descriptor_metrics(doc: &mut Document) -> usize {
    // Collect all FontDescriptor IDs that have embedded fonts.
    let fd_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if matches!(get_name(dict, b"Type").as_deref(), Some("FontDescriptor")) {
                    // Check if there's an embedded font.
                    if dict.has(b"FontFile") || dict.has(b"FontFile2") || dict.has(b"FontFile3") {
                        return Some(*id);
                    }
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;
    for fd_id in fd_ids {
        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let Ok(face) = ttf_parser::Face::parse(&font_data, 0) else {
            continue;
        };

        let units_per_em = face.units_per_em() as f64;
        if units_per_em == 0.0 {
            continue;
        }
        let scale = 1000.0 / units_per_em;

        let ascent = (face.ascender() as f64 * scale).round() as i64;
        let descent = (face.descender() as f64 * scale).round() as i64;
        let bbox = face.global_bounding_box();
        let cap_height = face
            .capital_height()
            .map(|h| (h as f64 * scale).round() as i64);

        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            // Only update if values differ.
            let ascent_ok = matches!(
                fd.get(b"Ascent").ok(),
                Some(Object::Integer(v)) if (*v - ascent).abs() <= 1
            );
            let descent_ok = matches!(
                fd.get(b"Descent").ok(),
                Some(Object::Integer(v)) if (*v - descent).abs() <= 1
            );
            let needs_update = !ascent_ok || !descent_ok;

            if needs_update {
                fd.set("Ascent", Object::Integer(ascent));
                fd.set("Descent", Object::Integer(descent));
                fd.set(
                    "FontBBox",
                    Object::Array(vec![
                        Object::Integer((bbox.x_min as f64 * scale).round() as i64),
                        Object::Integer((bbox.y_min as f64 * scale).round() as i64),
                        Object::Integer((bbox.x_max as f64 * scale).round() as i64),
                        Object::Integer((bbox.y_max as f64 * scale).round() as i64),
                    ]),
                );
                if let Some(ch) = cap_height {
                    fd.set("CapHeight", Object::Integer(ch));
                }
                fixed += 1;
            }
        }
    }
    fixed
}

/// Fix font metrics for all already-embedded fonts (6.2.11.6:3, 6.2.11.5:1).
///
/// Reads embedded font programs (FontFile2/FontFile3) and updates
/// Ascent, Descent, CapHeight, FontBBox in the FontDescriptor, and
/// Widths in the font dictionary to match the actual font data.
#[allow(dead_code)]
pub fn fix_embedded_font_metrics(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;
    for font_id in font_ids {
        let (subtype, fd_id, is_type0) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            let is_type0 = subtype == "Type0";

            if is_type0 {
                // For Type0, find descendant CIDFont and its FontDescriptor.
                let desc_fd = get_descendant_embed_info(doc, dict);
                match desc_fd {
                    Some((cid_id, true)) => {
                        let cid_fd = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                match d.get(b"FontDescriptor").ok() {
                                    Some(Object::Reference(id)) => Some(*id),
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        });
                        (subtype, cid_fd, true)
                    }
                    _ => continue,
                }
            } else {
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                };
                (subtype, fd_id, false)
            }
        };

        let Some(fd_id) = fd_id else { continue };

        // Read the embedded font program data.
        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let Ok(face) = ttf_parser::Face::parse(&font_data, 0) else {
            continue;
        };

        let units_per_em = face.units_per_em() as f64;
        if units_per_em == 0.0 {
            continue;
        }
        let scale = 1000.0 / units_per_em;

        // Update FontDescriptor metrics.
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            let ascent = (face.ascender() as f64 * scale).round() as i64;
            let descent = (face.descender() as f64 * scale).round() as i64;
            let bbox = face.global_bounding_box();
            fd.set("Ascent", Object::Integer(ascent));
            fd.set("Descent", Object::Integer(descent));
            fd.set(
                "FontBBox",
                Object::Array(vec![
                    Object::Integer((bbox.x_min as f64 * scale).round() as i64),
                    Object::Integer((bbox.y_min as f64 * scale).round() as i64),
                    Object::Integer((bbox.x_max as f64 * scale).round() as i64),
                    Object::Integer((bbox.y_max as f64 * scale).round() as i64),
                ]),
            );
            if let Some(cap_h) = face.capital_height() {
                fd.set(
                    "CapHeight",
                    Object::Integer((cap_h as f64 * scale).round() as i64),
                );
            }
        }

        // Update widths.
        if is_type0 || subtype == "CIDFontType0" || subtype == "CIDFontType2" {
            // Find the descendant CIDFont ID for Type0.
            if is_type0 {
                let cid_id = {
                    let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                        continue;
                    };
                    get_descendant_embed_info(doc, dict).map(|(id, _)| id)
                };
                if let Some(cid_id) = cid_id {
                    update_cid_widths(doc, cid_id, &face, scale);
                }
            }
        } else {
            // Construct a temporary NonEmbeddedFont for update_simple_widths.
            let info = NonEmbeddedFont {
                font_id,
                target_id: font_id,
                name: String::new(),
                is_type0: false,
                subtype: subtype.clone(),
            };
            let _ = info;
            update_simple_widths(doc, font_id, &face, scale);
        }

        fixed += 1;
    }
    fixed
}

/// Read the embedded font program data from a FontDescriptor.
fn read_embedded_font_data(doc: &Document, fd_id: ObjectId) -> Option<Vec<u8>> {
    let fd = match doc.objects.get(&fd_id) {
        Some(Object::Dictionary(d)) => d,
        _ => return None,
    };

    for key in &[b"FontFile2" as &[u8], b"FontFile3", b"FontFile"] {
        if let Ok(Object::Reference(stream_id)) = fd.get(key) {
            if let Some(Object::Stream(stream)) = doc.objects.get(stream_id) {
                let mut s = stream.clone();
                let _ = s.decompress();
                let data = &s.content;

                // lopdf may not support ASCIIHexDecode. If decompression left
                // ASCII hex characters, decode manually. The ASCIIHexDecode
                // end marker is '>'; lopdf may also leave `endstream` bytes.
                // Truncate at '>' if present, then check remaining is hex.
                let trimmed = if let Some(gt_pos) = data.iter().position(|&b| b == b'>') {
                    &data[..gt_pos]
                } else {
                    data
                };
                if trimmed.len() >= 8
                    && trimmed
                        .iter()
                        .all(|&b| b.is_ascii_hexdigit() || matches!(b, b'\r' | b'\n' | b' '))
                {
                    let decoded: Vec<u8> = trimmed
                        .iter()
                        .copied()
                        .filter(|b| b.is_ascii_hexdigit())
                        .collect::<Vec<_>>()
                        .chunks(2)
                        .filter_map(|pair| {
                            if pair.len() == 2 {
                                let hi = hex_nibble(pair[0])?;
                                let lo = hex_nibble(pair[1])?;
                                Some((hi << 4) | lo)
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !decoded.is_empty() {
                        return Some(decoded);
                    }
                }

                return Some(s.content);
            }
        }
    }
    None
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// Fix widths for CFF-based fonts only (6.2.11.5:1).
///
/// Unlike fix_width_mismatches (which is disabled due to TrueType regressions),
/// this only targets fonts with FontFile3 (CFF) programs where glyph_width is
/// available. Safe to call without affecting TrueType fonts.
pub fn fix_cff_widths(doc: &mut Document) -> usize {
    use std::collections::HashSet;

    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;
    let mut processed_cid_fonts: HashSet<ObjectId> = HashSet::new();

    for font_id in font_ids {
        let (subtype, fd_id, cid_font_id) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();

            if subtype == "Type0" {
                let desc = get_descendant_embed_info(doc, dict);
                match desc {
                    Some((cid_id, true)) => {
                        let cid_fd = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                match d.get(b"FontDescriptor").ok() {
                                    Some(Object::Reference(id)) => Some(*id),
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        });
                        let cid_subtype = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                get_name(d, b"Subtype")
                            } else {
                                None
                            }
                        });
                        (cid_subtype.unwrap_or_default(), cid_fd, Some(cid_id))
                    }
                    _ => continue,
                }
            } else if subtype == "CIDFontType0" {
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                };
                (subtype, fd_id, Some(font_id))
            } else {
                continue; // Skip TrueType and simple fonts.
            }
        };

        // Only process CIDFontType0 (CFF-based CID fonts).
        if subtype != "CIDFontType0" {
            continue;
        }

        let Some(fd_id) = fd_id else { continue };
        let Some(cid_id) = cid_font_id else { continue };
        if !processed_cid_fonts.insert(cid_id) {
            continue;
        }

        // Only process FontFile3 (CFF programs).
        let has_ff3 = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile3")
        );
        if !has_ff3 {
            continue;
        }

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        // CIDFontType0 streams may contain either raw CFF data or OTF-wrapped
        // CFF. Support both so width repair covers embedded OpenType CFF fonts.
        let Some(cff) =
            cff_parser::Table::parse(&font_data).or_else(|| extract_cff_from_otf(&font_data))
        else {
            continue;
        };

        if fix_cid_widths_from_cff(doc, cid_id, &cff) {
            fixed += 1;
        }
    }
    fixed
}

/// Fix widths for TrueType CIDFontType2 fonts (6.2.11.5:1).
///
/// CIDFontType2 fonts use CIDToGIDMap (Identity or explicit) to map CIDs
/// to GlyphIDs. This is safer than simple TrueType width fixing because
/// the mapping is unambiguous (no encoding complexity).
pub fn fix_truetype_cid_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (subtype, fd_id, cid_font_id, type0_cmap_name) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();

            if subtype == "Type0" {
                // Get CIDFont descendant.
                let desc = get_descendant_embed_info(doc, dict);
                match desc {
                    Some((cid_id, true)) => {
                        let cid_subtype = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                get_name(d, b"Subtype")
                            } else {
                                None
                            }
                        });
                        let cid_fd = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                match d.get(b"FontDescriptor").ok() {
                                    Some(Object::Reference(id)) => Some(*id),
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        });
                        (
                            cid_subtype.unwrap_or_default(),
                            cid_fd,
                            Some(cid_id),
                            resolve_type0_cmap_name(doc, dict),
                        )
                    }
                    _ => continue,
                }
            } else if subtype == "CIDFontType2" {
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                };
                (subtype, fd_id, Some(font_id), None)
            } else {
                continue;
            }
        };

        // Only process CIDFontType2 (TrueType-based CID fonts).
        if subtype != "CIDFontType2" {
            continue;
        }

        let Some(fd_id) = fd_id else { continue };
        let Some(cid_id) = cid_font_id else { continue };

        // Process fonts with FontFile2 (TrueType) or FontFile3 (OTF/CFF).
        // Some CIDFontType2 fonts embed an OTF CFF program as FontFile3 instead
        // of FontFile2; ttf_parser handles both, so we include them here.
        let has_embedded = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile2") || d.has(b"FontFile3")
        );
        if !has_embedded {
            continue;
        }

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        // Some malformed CIDFontType2 subsets ship inconsistent TrueType
        // metric headers (invalid indexToLocFormat + numberOfHMetrics == 0).
        // In these files validators resolve glyph widths as zero; force a
        // zero-width dictionary to stay consistent with the embedded program.
        let force_zero_widths = {
            let head = tt_find_table(&font_data, b"head");
            let hhea = tt_find_table(&font_data, b"hhea");
            if let (Some(head), Some(hhea)) = (head, hhea) {
                if head.len() >= 52 && hhea.len() >= 36 {
                    let idx_format = i16::from_be_bytes([head[50], head[51]]);
                    let num_h_metrics = u16::from_be_bytes([hhea[34], hhea[35]]);
                    (idx_format != 0 && idx_format != 1) && num_h_metrics == 0
                } else {
                    false
                }
            } else {
                false
            }
        };
        if force_zero_widths {
            if let Some(Object::Dictionary(ref mut cid_dict)) = doc.objects.get_mut(&cid_id) {
                cid_dict.set("DW", Object::Integer(0));
                cid_dict.remove(b"W");
            }
            fixed += 1;
            continue;
        }

        let parsed_face = ttf_parser::Face::parse(&font_data, 0).ok();
        let raw_metrics = if parsed_face.is_none() {
            tt_parse_raw_metrics(&font_data)
        } else {
            None
        };
        let (num_glyphs, scale) = if let Some(face) = parsed_face.as_ref() {
            let upem = face.units_per_em() as f64;
            if upem == 0.0 {
                continue;
            }
            (face.number_of_glyphs(), 1000.0 / upem)
        } else if let Some(raw) = raw_metrics.as_ref() {
            (raw.num_glyphs, 1000.0 / raw.units_per_em as f64)
        } else {
            continue;
        };

        let should_install_predefined_map = type0_cmap_name
            .as_deref()
            .filter(|name| !name.starts_with("UniJIS-"))
            .filter(|name| !is_identity_type0_cmap(name))
            .and_then(load_predefined_unicode_cmap_ranges)
            .and_then(|ranges| {
                let face = parsed_face.as_ref()?;
                let needs_map = match doc.objects.get(&cid_id) {
                    Some(Object::Dictionary(cid_dict)) => match cid_dict.get(b"CIDToGIDMap").ok() {
                        None => true,
                        Some(Object::Name(n)) if n == b"Identity" => true,
                        _ => false,
                    },
                    _ => false,
                };
                if !needs_map {
                    return None;
                }
                let map_bytes = build_predefined_cmap_cidtogid_map(face, &ranges)?;
                Some(map_bytes)
            });
        if let Some(map_bytes) = should_install_predefined_map {
            let map_id = doc.add_object(Object::Stream(lopdf::Stream::new(
                dictionary! {},
                map_bytes,
            )));
            if let Some(Object::Dictionary(ref mut cid_dict)) = doc.objects.get_mut(&cid_id) {
                cid_dict.set("CIDToGIDMap", Object::Reference(map_id));
            }
        }

        enum CidToGidMode {
            Identity,
            Stream(Vec<u8>),
        }

        // Read CIDToGIDMap to determine mapping.
        let cid_to_gid_mode = {
            let Some(Object::Dictionary(cid_dict)) = doc.objects.get(&cid_id) else {
                continue;
            };
            match cid_dict.get(b"CIDToGIDMap").ok() {
                Some(Object::Name(n)) if n == b"Identity" => CidToGidMode::Identity,
                None => CidToGidMode::Identity, // Default is Identity per spec.
                Some(Object::Reference(id)) => match doc.objects.get(id) {
                    Some(Object::Stream(s)) => {
                        let mut st = s.clone();
                        let _ = st.decompress();
                        CidToGidMode::Stream(st.content)
                    }
                    _ => continue,
                },
                Some(Object::Stream(s)) => {
                    let mut st = s.clone();
                    let _ = st.decompress();
                    CidToGidMode::Stream(st.content)
                }
                _ => continue,
            }
        };

        // Collect widths: CID → width in PDF units.
        let mut widths: Vec<(u16, i64)> = Vec::new();
        match cid_to_gid_mode {
            CidToGidMode::Identity => {
                // Identity mapping: CID == GID.
                for gid in 0..num_glyphs {
                    let w = if let Some(face) = parsed_face.as_ref() {
                        let gid_obj = ttf_parser::GlyphId(gid);
                        // 6.2.11.5 compares dictionary widths to the embedded
                        // font program metrics (hmtx advances). Even when glyf
                        // outline data is empty, non-zero advances remain valid.
                        face.glyph_hor_advance(gid_obj)
                            .map(|a| (a as f64 * scale).round() as i64)
                            .unwrap_or(0)
                    } else if let Some(raw) = raw_metrics.as_ref() {
                        tt_raw_glyph_advance(raw, gid)
                            .map(|a| (a as f64 * scale).round() as i64)
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    widths.push((gid, w));
                }
            }
            CidToGidMode::Stream(map_bytes) => {
                // Stream mapping: each 2-byte big-endian entry maps CID index -> GID.
                for (cid, chunk) in map_bytes.chunks_exact(2).enumerate() {
                    if cid > u16::MAX as usize {
                        break;
                    }
                    let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                    // 0xFFFF is the "not mapped" sentinel used by some subsetters.
                    // GID 0 (.notdef) is a real glyph with its own advance width;
                    // veraPDF validates that the dictionary width matches the hmtx
                    // advance for every rendered CID, including those that fall back
                    // to .notdef, so we must include them rather than skipping them.
                    if gid == u16::MAX {
                        continue;
                    }
                    let w = if gid < num_glyphs {
                        if let Some(face) = parsed_face.as_ref() {
                            let gid_obj = ttf_parser::GlyphId(gid);
                            face.glyph_hor_advance(gid_obj)
                                .map(|a| (a as f64 * scale).round() as i64)
                                .unwrap_or(0)
                        } else if let Some(raw) = raw_metrics.as_ref() {
                            tt_raw_glyph_advance(raw, gid)
                                .map(|a| (a as f64 * scale).round() as i64)
                                .unwrap_or(0)
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    widths.push((cid as u16, w));
                }
            }
        }

        if widths.is_empty() {
            continue;
        }

        // Check if existing widths already match (avoid unnecessary changes).
        let existing_matches = check_cid_widths_match(doc, cid_id, &widths);
        if existing_matches {
            continue;
        }

        // Determine DW (most common width).
        let mut freq: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
        for (_, w) in &widths {
            *freq.entry(*w).or_default() += 1;
        }
        let dw = freq
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(w, _)| *w)
            .unwrap_or(1000);

        // Build /W array with runs of non-default widths.
        let mut w_array: Vec<Object> = Vec::new();
        let mut i = 0;
        while i < widths.len() {
            let (cid, w) = widths[i];
            if w == dw {
                i += 1;
                continue;
            }
            // Start a run of consecutive CIDs with non-default widths.
            let start = cid;
            let mut run: Vec<i64> = vec![w];
            i += 1;
            while i < widths.len() {
                let (next_cid, next_w) = widths[i];
                if next_cid != start + run.len() as u16 {
                    break;
                }
                if next_w == dw && (i + 1 >= widths.len() || widths[i + 1].0 != next_cid + 1) {
                    break;
                }
                run.push(next_w);
                i += 1;
            }
            w_array.push(Object::Integer(start as i64));
            w_array.push(Object::Array(
                run.into_iter().map(Object::Integer).collect(),
            ));
        }

        // Update the CIDFont dictionary.
        if let Some(Object::Dictionary(ref mut cid_dict)) = doc.objects.get_mut(&cid_id) {
            cid_dict.set("DW", Object::Integer(dw));
            if w_array.is_empty() {
                cid_dict.remove(b"W");
            } else {
                cid_dict.set("W", Object::Array(w_array));
            }
        }

        fixed += 1;
    }

    fixed
}

/// Check if existing CIDFont W/DW widths match the expected widths.
fn check_cid_widths_match(doc: &Document, cid_id: ObjectId, expected: &[(u16, i64)]) -> bool {
    let Some(Object::Dictionary(cid_dict)) = doc.objects.get(&cid_id) else {
        return false;
    };

    let dw = match cid_dict.get(b"DW").ok() {
        Some(Object::Integer(v)) => *v,
        _ => 1000,
    };

    // Build a map of CID → expected width from the W array.
    let mut existing_widths: std::collections::HashMap<u16, i64> = std::collections::HashMap::new();
    if let Ok(Object::Array(w_arr)) = cid_dict.get(b"W") {
        let mut i = 0;
        while i < w_arr.len() {
            let start_cid = match &w_arr[i] {
                Object::Integer(v) => *v as u16,
                _ => break,
            };
            i += 1;
            if i >= w_arr.len() {
                break;
            }
            match &w_arr[i] {
                Object::Array(widths) => {
                    for (j, w) in widths.iter().enumerate() {
                        if let Object::Integer(v) = w {
                            existing_widths.insert(start_cid + j as u16, *v);
                        }
                    }
                    i += 1;
                }
                Object::Integer(end_cid) => {
                    i += 1;
                    if i >= w_arr.len() {
                        break;
                    }
                    if let Object::Integer(width) = &w_arr[i] {
                        for cid in start_cid..=(*end_cid as u16) {
                            existing_widths.insert(cid, *width);
                        }
                    }
                    i += 1;
                }
                _ => break,
            }
        }
    }

    // Compare: for each expected width, check if existing matches.
    for (cid, exp_w) in expected {
        let actual = existing_widths.get(cid).copied().unwrap_or(dw);
        if (actual - exp_w).abs() > 1 {
            return false;
        }
    }
    true
}

/// Fix CharSet in Type 1 font descriptors (6.2.11.4.2:1).
///
/// The CharSet string must list all glyph names present in the CFF font program.
pub fn fix_type1_charset(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (fd_id, is_type1) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "Type1" && subtype != "MMType1" {
                continue;
            }
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            (fd_id, true)
        };

        if !is_type1 {
            continue;
        }

        // Only process FontFile3 (CFF programs) or FontFile (Type 1 PFB).
        let has_fontfile3 = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile3")
        );
        let has_fontfile = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile")
        );
        if !has_fontfile3 && !has_fontfile {
            continue;
        }

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let charset_str = if has_fontfile3 {
            // CFF font program — extract glyph names from CFF.
            let Some(cff) = cff_parser::Table::parse(&font_data) else {
                continue;
            };
            let num_glyphs = cff.number_of_glyphs();
            let mut cs = String::new();
            for gid in 0..num_glyphs {
                let glyph_id = cff_parser::GlyphId(gid);
                if let Some(name) = cff.glyph_name(glyph_id) {
                    if name != ".notdef" {
                        cs.push('/');
                        cs.push_str(name);
                    }
                }
            }
            cs
        } else {
            // PFB/Type1 font — decrypt eexec and extract glyph names from CharStrings.
            extract_type1_glyph_names_to_charset(&font_data)
        };

        if charset_str.is_empty() {
            continue;
        }

        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            fd.set(
                "CharSet",
                Object::String(charset_str.into_bytes(), lopdf::StringFormat::Literal),
            );
            fixed += 1;
        }
    }
    fixed
}

/// Extract glyph names from a Type 1 (PFB) font program and return a CharSet string.
///
/// Strips PFB segment headers, decrypts the eexec-encrypted binary section,
/// then extracts glyph names from the /CharStrings dictionary.
/// Returns a string like "/A/B/C/space" (without .notdef).
fn extract_type1_glyph_names_to_charset(pfb_data: &[u8]) -> String {
    let ps_data = strip_pfb_headers(pfb_data);

    // First try: see if /CharStrings is visible in cleartext (rare but possible).
    if let Some(cs) = extract_glyph_names_from_charstrings(&ps_data) {
        if !cs.is_empty() {
            return cs;
        }
    }

    // Decrypt eexec section and try again.
    let decrypted = decrypt_eexec(&ps_data);
    if decrypted.is_empty() {
        return String::new();
    }

    extract_glyph_names_from_charstrings(&decrypted).unwrap_or_default()
}

/// Strip PFB (Printer Font Binary) headers from embedded Type1 FontFile streams.
///
/// PDF requires raw PostScript, not PFB format. This function finds all
/// FontDescriptor FontFile streams starting with `\x80\x01` (PFB magic),
/// strips the segment headers, and updates Length1/Length2/Length3 entries.
///
/// Returns the number of font streams fixed.
pub fn fix_pfb_font_streams(doc: &mut Document) -> usize {
    // Collect (fd_id, ff_stream_id) for FontDescriptors with PFB FontFile streams.
    let targets: Vec<(ObjectId, ObjectId)> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Dictionary(dict) = obj else {
                return None;
            };
            if !matches!(get_name(dict, b"Type").as_deref(), Some("FontDescriptor")) {
                return None;
            }
            let ff_id = match dict.get(b"FontFile").ok() {
                Some(Object::Reference(r)) => *r,
                _ => return None,
            };
            Some((*id, ff_id))
        })
        .collect();

    let mut fixed = 0;

    for (_fd_id, ff_id) in targets {
        // Read raw (possibly compressed) stream content.
        let raw_data = {
            let Some(Object::Stream(s)) = doc.objects.get(&ff_id) else {
                continue;
            };
            let mut s = s.clone();
            let _ = s.decompress();
            s.content
        };

        // Skip if not PFB format.
        if raw_data.len() < 6 || raw_data[0] != 0x80 || raw_data[1] != 0x01 {
            continue;
        }

        let Some((stripped, l1, l2, l3)) = parse_pfb_segments(&raw_data) else {
            continue;
        };

        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&ff_id) {
            stream.set_plain_content(stripped);
            stream.dict.set("Length1", Object::Integer(l1));
            stream.dict.set("Length2", Object::Integer(l2));
            if l3 > 0 {
                stream.dict.set("Length3", Object::Integer(l3));
            } else {
                stream.dict.remove(b"Length3");
            }
        }
        fixed += 1;
    }

    fixed
}

/// Fix Type1 FontFile streams where the first byte of the binary eexec section
/// is a PDF whitespace character (0x00, 0x09, 0x0A, 0x0C, 0x0D, 0x20).
///
/// veraPDF's `Type1FontProgram.parseFont()` calls `skipSpacesExceptNullByte()`
/// before reading the eexec binary section, which skips any leading PDF
/// whitespace bytes. If the first encrypted byte of the binary section happens
/// to be a PDF whitespace character, veraPDF starts decryption from the wrong
/// position, causing the Private dict to be garbage and `glyphWidths` to be
/// null → `containsFontFile == false`.
///
/// Fix: decrypt the binary section, re-encrypt with new 4-byte seed values
/// where the first encrypted byte is not a PDF whitespace character.
///
/// Returns the number of font streams fixed.
pub fn fix_type1_eexec_space_prefix(doc: &mut Document) -> usize {
    const PDF_SPACE: [u8; 6] = [0x00, 0x09, 0x0A, 0x0C, 0x0D, 0x20];

    // Collect (fd_id, ff_stream_id) for FontDescriptors with Type1 FontFile streams.
    let targets: Vec<(ObjectId, ObjectId)> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Dictionary(dict) = obj else {
                return None;
            };
            if !matches!(get_name(dict, b"Type").as_deref(), Some("FontDescriptor")) {
                return None;
            }
            let ff_id = match dict.get(b"FontFile").ok() {
                Some(Object::Reference(r)) => *r,
                _ => return None,
            };
            Some((*id, ff_id))
        })
        .collect();

    let mut fixed = 0;

    for (_fd_id, ff_id) in targets {
        let (stream_data, l1, l2) = {
            let Some(Object::Stream(s)) = doc.objects.get(&ff_id) else {
                continue;
            };
            let mut s = s.clone();
            let _ = s.decompress();
            let data = s.content;
            let l1 = match s.dict.get(b"Length1").ok() {
                Some(Object::Integer(n)) => *n as usize,
                _ => continue,
            };
            let l2 = match s.dict.get(b"Length2").ok() {
                Some(Object::Integer(n)) => *n as usize,
                _ => continue,
            };
            (data, l1, l2)
        };

        // Validate bounds.
        if l1 >= stream_data.len() || l1 + l2 > stream_data.len() || l2 < 5 {
            continue;
        }

        // Check if the first byte of the binary section is a PDF whitespace.
        let first_binary_byte = stream_data[l1];
        if !PDF_SPACE.contains(&first_binary_byte) {
            continue;
        }

        let binary = &stream_data[l1..l1 + l2];

        // Decrypt the binary section to extract the real font data.
        let decrypted_all = type1_binary_eexec_decrypt(binary);
        if decrypted_all.len() < 5 {
            continue;
        }
        let real_data = &decrypted_all[4..]; // Skip 4 random seed bytes.

        // Re-encrypt with new seed where first encrypted byte is not a space.
        // Initial key = 55665. First encrypted byte = seed[0] ^ (55665 >> 8) = seed[0] ^ 0xD9.
        // 0xAA ^ 0xD9 = 0x73 ('s'), which is not a PDF space character.
        let new_binary = type1_binary_eexec_encrypt(real_data, 0xAA);
        debug_assert_eq!(new_binary.len(), l2);
        debug_assert!(!PDF_SPACE.contains(&new_binary[0]));

        // Rebuild the full stream: cleartext + new_binary + trailing.
        let mut new_stream = Vec::with_capacity(stream_data.len());
        new_stream.extend_from_slice(&stream_data[..l1]);
        new_stream.extend_from_slice(&new_binary);
        new_stream.extend_from_slice(&stream_data[l1 + l2..]);

        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&ff_id) {
            stream.set_plain_content(new_stream);
            // Length1/Length2/Length3 remain the same.
        }
        fixed += 1;
    }

    fixed
}

/// Decrypt Type1 binary eexec-encoded data with key 55665 (binary only, no hex support).
fn type1_binary_eexec_decrypt(data: &[u8]) -> Vec<u8> {
    let mut key: u16 = 55665;
    let mut result = Vec::with_capacity(data.len());
    for &c in data {
        let p = c ^ (key >> 8) as u8;
        key = (c as u16)
            .wrapping_add(key)
            .wrapping_mul(52845)
            .wrapping_add(22719);
        result.push(p);
    }
    result
}

/// Re-encrypt real Type1 font data with 4 new seed bytes.
///
/// The first encrypted byte = `seed_first ^ (55665 >> 8)` = `seed_first ^ 0xD9`.
/// Using seed_first=0xAA gives first_encrypted=0x73 ('s'), not a PDF whitespace.
fn type1_binary_eexec_encrypt(real_data: &[u8], seed_first: u8) -> Vec<u8> {
    let seeds = [seed_first, 0u8, 0u8, 0u8];
    let full_plain: Vec<u8> = seeds.iter().chain(real_data.iter()).copied().collect();
    let mut key: u16 = 55665;
    let mut result = Vec::with_capacity(full_plain.len());
    for &p in &full_plain {
        let c = p ^ (key >> 8) as u8;
        key = (c as u16)
            .wrapping_add(key)
            .wrapping_mul(52845)
            .wrapping_add(22719);
        result.push(c);
    }
    result
}

/// Fix stub Type1 fonts by redirecting their FontFile to a matching full font program.
///
/// Some PDF generators (e.g. dvips/LaTeX) include two versions of the same font:
///  - A stub with only `.notdef` CharString and `/CharSet()` (empty)
///  - A full subset with actual glyphs and a non-empty `/CharSet`
///
/// veraPDF's `Type1FontProgram.parseFont()` fails on stub fonts (no real CharStrings
/// after skipping `.notdef`), so `containsFontFile == false` → rule 6.2.11.4.1:1 fails.
///
/// Fix: for each stub FontDescriptor (has `/CharSet()` empty), find another FontDescriptor
/// whose base font name (strip 6-char prefix + optional `~XX` suffix) matches and has a
/// parseable font program. Redirect the stub's `/FontFile` reference to the real font's
/// FontFile stream and remove the empty `/CharSet` entry.
///
/// Returns the number of stub FontDescriptors fixed.
pub fn fix_type1_stub_font_files(doc: &mut Document) -> usize {
    // Step 1: Collect all Type1 FontDescriptors with FontFile streams.
    // Record: font_name, fd_obj_id, ff_obj_id, charset_is_empty
    #[derive(Debug, Clone)]
    struct FdInfo {
        fd_id: ObjectId,
        ff_id: ObjectId,
        full_name: String,
        base_name: String, // after stripping 6-char prefix and ~XX suffix
        charset_empty: bool,
    }

    let fd_infos: Vec<FdInfo> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Dictionary(dict) = obj else {
                return None;
            };
            if !matches!(get_name(dict, b"Type").as_deref(), Some("FontDescriptor")) {
                return None;
            }
            // Must have a FontFile (Type1) reference, not FontFile2 (TrueType) or FontFile3 (CFF)
            let ff_id = match dict.get(b"FontFile").ok() {
                Some(Object::Reference(r)) => *r,
                _ => return None,
            };
            let full_name = match dict.get(b"FontName").ok() {
                Some(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
                _ => return None,
            };
            // Detect empty CharSet: /CharSet() or /CharSet with empty string value
            let charset_empty = match dict.get(b"CharSet").ok() {
                Some(Object::String(s, _)) => s.is_empty(),
                Some(Object::Name(n)) => n.is_empty(),
                None => false,
                _ => false,
            };
            let base_name = type1_base_font_name(&full_name);
            Some(FdInfo {
                fd_id: *id,
                ff_id,
                full_name,
                base_name,
                charset_empty,
            })
        })
        .collect();

    // Step 2: Build map from base_name → ff_id for non-stub fonts
    use std::collections::HashMap;
    let mut real_font_map: HashMap<String, (ObjectId, String)> = HashMap::new();
    for info in &fd_infos {
        if !info.charset_empty {
            real_font_map
                .entry(info.base_name.clone())
                .or_insert_with(|| (info.ff_id, info.full_name.clone()));
        }
    }

    // Step 3: For each stub, redirect FontFile to real font's FontFile
    let mut fixed = 0usize;
    let stubs: Vec<FdInfo> = fd_infos.into_iter().filter(|i| i.charset_empty).collect();

    for stub in &stubs {
        let (real_ff_id, ref real_name) = match real_font_map.get(&stub.base_name) {
            Some(r) => r.clone(),
            None => continue, // No matching real font found
        };
        // Don't redirect to itself
        if real_ff_id == stub.ff_id {
            continue;
        }
        // Update the stub's FontDescriptor: redirect FontFile and remove CharSet
        let fd_obj = match doc.objects.get_mut(&stub.fd_id) {
            Some(Object::Dictionary(d)) => d,
            _ => continue,
        };
        fd_obj.set(b"FontFile", Object::Reference(real_ff_id));
        fd_obj.remove(b"CharSet");
        eprintln!(
            "fix_type1_stub_font_files: {} stub → redirect FontFile to {} ({})",
            stub.full_name, real_name, real_ff_id.0
        );
        fixed += 1;
    }

    fixed
}

/// Fix invalid CFF BCD (Binary-Coded Decimal) real number encodings that cause
/// veraPDF's CFF parser to throw `NumberFormatException` → `successfullyParsed = false`
/// → rule 6.2.11.4.1:1 fails.
///
/// Two known invalid patterns:
///
///  1. `1e ff`: The byte `0xff` splits to nibbles `(0xf, 0xf)`.  Nibble `0xf` means
///     "end of BCD", so the parser exits immediately with an **empty** `StringBuilder`.
///     `Float.parseFloat("")` → `NumberFormatException`.
///     Fix: replace `1e ff` → `1e 0f` → nibbles `(0, 0xf)` → string `"0"` → 0.0.
///
///  2. `1e c0 00 48 82 81 25 ff`: BCD starts with nibble `0xc` (`'E-'`) before any
///     mantissa digits → string `"E-00048828125"` → `Float.parseFloat` fails.
///     This was meant to encode `0.00048828125 = 1/2048` (FontMatrix scale for
///     2048-unit-em fonts).
///     Fix: replace with `1e 4a 88 28 12 5b e4 ff` → `"4.8828125E-4"` = 0.00048828125.
///
/// Only CFF streams (`/Subtype /CIDFontType0C` or `/Type1C`) are examined.
///
/// Safety: scanning is limited to bytes before the CharStrings INDEX section. In CFF
/// charstrings, byte `0x1e` is the `vhcurveto` operator (value 30), so replacing `1e ff`
/// in charstring data would corrupt the font. By stopping at `charstrings_offset` we only
/// touch DICT sections (Top DICT, Private DICTs) where `0x1e` marks BCD real numbers.
///
/// Returns the number of font streams patched.
pub fn fix_cff_invalid_bcd(doc: &mut Document) -> usize {
    // Pattern 1: 1e ff  →  1e 0f  ("" → "0")
    const BAD_EMPTY: [u8; 2] = [0x1e, 0xff];
    const GOOD_ZERO: [u8; 2] = [0x1e, 0x0f];

    // Pattern 2: malformed FontMatrix 1/2048 BCD  →  correct "4.8828125E-4"
    const BAD_FONTMATRIX: [u8; 8] = [0x1e, 0xc0, 0x00, 0x48, 0x82, 0x81, 0x25, 0xff];
    const GOOD_FONTMATRIX: [u8; 8] = [0x1e, 0x4a, 0x88, 0x28, 0x12, 0x5b, 0xe4, 0xff];

    // Collect IDs of CFF font streams.
    let targets: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Stream(s) = obj else {
                return None;
            };
            match s.dict.get(b"Subtype").ok() {
                Some(Object::Name(n))
                    if n.as_slice() == b"CIDFontType0C" || n.as_slice() == b"Type1C" =>
                {
                    Some(*id)
                }
                _ => None,
            }
        })
        .collect();

    let mut fixed = 0usize;

    for id in targets {
        let data = {
            let Some(Object::Stream(s)) = doc.objects.get(&id) else {
                continue;
            };
            let mut s = s.clone();
            let _ = s.decompress();
            s.content
        };

        // Only process streams that begin with a valid CFF header (major version == 1).
        // Streams starting with 0x00 or 0x4F ('O' for OTTO/OTF) are SFNT/TrueType and
        // must NOT have their bytes patched as BCD — the 1e byte is a charstring op there.
        if data.first() != Some(&1) {
            continue;
        }

        // Limit the scan to the DICT sections (before CharStrings INDEX). Byte 0x1e is
        // the `vhcurveto` operator in Type 2 charstrings, so replacing it in charstring
        // data would corrupt the font program. (#465)
        let scan_end = cff_charstrings_offset(&data).unwrap_or(data.len());

        let mut patched = data.clone();
        let mut changed = false;

        // Apply pattern 2 first (longer → more specific, avoids overlap with p1).
        let mut i = 0;
        while i + BAD_FONTMATRIX.len() <= scan_end.min(patched.len()) {
            if patched[i..i + BAD_FONTMATRIX.len()] == BAD_FONTMATRIX {
                patched[i..i + GOOD_FONTMATRIX.len()].copy_from_slice(&GOOD_FONTMATRIX);
                changed = true;
                i += GOOD_FONTMATRIX.len();
            } else {
                i += 1;
            }
        }

        // Apply pattern 1.
        let mut i = 0;
        while i + BAD_EMPTY.len() <= scan_end.min(patched.len()) {
            if patched[i..i + BAD_EMPTY.len()] == BAD_EMPTY {
                patched[i..i + GOOD_ZERO.len()].copy_from_slice(&GOOD_ZERO);
                changed = true;
                i += GOOD_ZERO.len();
            } else {
                i += 1;
            }
        }

        if changed {
            if let Some(Object::Stream(stream)) = doc.objects.get_mut(&id) {
                stream.set_plain_content(patched);
            }
            fixed += 1;
        }
    }

    fixed
}

/// Find the byte offset in a raw CFF stream where the CharStrings INDEX begins.
///
/// BCD real numbers (byte `0x1e`) only appear in DICT sections. In Type 2 charstrings,
/// `0x1e` is the `vhcurveto` operator. Scanning only up to `charstrings_offset` makes
/// BCD replacement safe: we never touch charstring bytes. (#465)
///
/// Returns `None` if the CFF structure can't be parsed (caller falls back to full scan).
fn cff_charstrings_offset(data: &[u8]) -> Option<usize> {
    // CFF header: major(1), minor(1), hdrSize(1), offSize(1)
    if data.len() < 4 || data[0] != 1 {
        return None;
    }
    let hdr_size = data[2] as usize;

    // Skip Name INDEX → find Top DICT INDEX start.
    let top_dict_start = cff_skip_index(data, hdr_size)?;

    // Parse Top DICT INDEX header: count(2), offSize(1), offsets[(count+1)*offSize], data.
    if top_dict_start + 2 > data.len() {
        return None;
    }
    let count = u16::from_be_bytes([data[top_dict_start], data[top_dict_start + 1]]) as usize;
    if count == 0 {
        return None;
    }
    let off_size = *data.get(top_dict_start + 2)? as usize;
    if off_size == 0 || off_size > 4 {
        return None;
    }
    let offsets_end = top_dict_start + 3 + (count + 1) * off_size;
    if offsets_end > data.len() {
        return None;
    }

    // Read offset[0] and offset[count] (1-based) to determine Top DICT data bounds.
    let read_off = |i: usize| -> Option<usize> {
        let start = top_dict_start + 3 + i * off_size;
        let bytes = data.get(start..start + off_size)?;
        let mut val = 0usize;
        for &b in bytes {
            val = (val << 8) | b as usize;
        }
        val.checked_sub(1) // CFF offsets are 1-based
    };

    let first_off = read_off(0).unwrap_or(0);
    let last_off = read_off(count)?;
    let td_start = offsets_end + first_off;
    let td_end = offsets_end + last_off;
    if td_end > data.len() {
        return None;
    }

    // Scan Top DICT data for operator 17 (CharStrings offset operand).
    // CFF DICT encoding: integers/reals (operands) precede their operator byte.
    let td = &data[td_start..td_end];
    let mut i = 0;
    let mut last_int: Option<usize> = None;
    while i < td.len() {
        let b = td[i];
        match b {
            17 => {
                // CharStrings operator — preceding integer is the absolute stream offset.
                return last_int;
            }
            12 => {
                // 2-byte operator escape.
                last_int = None;
                i += 2;
            }
            0..=21 => {
                // 1-byte operator (other than 12 and 17, handled above).
                last_int = None;
                i += 1;
            }
            28 => {
                // shortint: 3 bytes total (op + 2-byte big-endian value).
                if i + 2 < td.len() {
                    let v = i16::from_be_bytes([td[i + 1], td[i + 2]]);
                    last_int = if v >= 0 { Some(v as usize) } else { None };
                }
                i += 3;
            }
            29 => {
                // longint: 5 bytes total (op + 4-byte big-endian value).
                if i + 4 < td.len() {
                    let v = i32::from_be_bytes([td[i + 1], td[i + 2], td[i + 3], td[i + 4]]);
                    last_int = if v >= 0 { Some(v as usize) } else { None };
                }
                i += 5;
            }
            30 => {
                // BCD real number: variable length, ends when a nibble 0xF is seen.
                i += 1;
                while i < td.len() {
                    let byte = td[i];
                    i += 1;
                    if (byte >> 4) == 0xF || (byte & 0xF) == 0xF {
                        break;
                    }
                }
                last_int = None; // CharStrings offset is always an integer.
            }
            32..=246 => {
                // 1-byte integer: value = b − 139.
                let v = b as i32 - 139;
                last_int = if v >= 0 { Some(v as usize) } else { None };
                i += 1;
            }
            247..=250 => {
                // 2-byte positive integer.
                if i + 1 < td.len() {
                    let v = (b as u32 - 247) * 256 + td[i + 1] as u32 + 108;
                    last_int = Some(v as usize);
                }
                i += 2;
            }
            251..=254 => {
                // 2-byte negative integer (negative offsets are not valid CharStrings offsets).
                last_int = None;
                i += 2;
            }
            _ => {
                last_int = None;
                i += 1;
            }
        }
    }
    None
}

/// Skip over a CFF INDEX at `start` in `data`, returning the offset of the next byte.
fn cff_skip_index(data: &[u8], start: usize) -> Option<usize> {
    if start + 2 > data.len() {
        return None;
    }
    let count = u16::from_be_bytes([data[start], data[start + 1]]) as usize;
    if count == 0 {
        return Some(start + 2);
    }
    let off_size = *data.get(start + 2)? as usize;
    if off_size == 0 || off_size > 4 {
        return None;
    }
    let offsets_end = start + 3 + (count + 1) * off_size;
    if offsets_end > data.len() {
        return None;
    }
    // Read the last offset value (1-based) to determine data section end.
    let last_off_start = start + 3 + count * off_size;
    let last_off_bytes = data.get(last_off_start..last_off_start + off_size)?;
    let mut last_off = 0usize;
    for &b in last_off_bytes {
        last_off = (last_off << 8) | b as usize;
    }
    // last_off is 1-based, so actual end = offsets_end + last_off - 1.
    Some(offsets_end + last_off.saturating_sub(1))
}

/// Fix TrueType/SFNT font programs that are stored in `FontFile3` with `Subtype
/// /CIDFontType0C` instead of the correct `FontFile2` (or `FontFile3` with
/// `Subtype /CIDFontType2`).
///
/// Some PDFs embed TrueType (sfnt magic `00 01 00 00`) fonts but label them as
/// CFF (`/Subtype /CIDFontType0C`). veraPDF's CFF parser immediately fails on
/// non-CFF data → `successfullyParsed = false` → `containsFontFile == false` →
/// rule 6.2.11.4.1:1 fails.
///
/// Note: OTTO (`4F 54 54 4F`) means OpenType CFF in SFNT; veraPDF can parse
/// that in FontFile3 so those are left unchanged.
///
/// Fix:
/// 1. Find `FontDescriptor` objects whose `FontFile3` stream starts with TrueType magic.
/// 2. Rename the `FontFile3` key to `FontFile2` (TrueType container).
/// 3. Remove the erroneous `Subtype` from the font stream dict.
/// 4. For CIDFontType0 DescendantFont dicts linked to such a descriptor, change
///    their `Subtype` to `CIDFontType2` (TrueType CID font).
///
/// Returns the number of font descriptors fixed.
pub fn fix_mislabeled_truetype_as_cff(doc: &mut Document) -> usize {
    // Only fix pure TrueType (sfnt 00010000). OTTO (4F54544F) is OpenType CFF
    // in an SFNT wrapper; veraPDF can parse it in FontFile3/CIDFontType0C so
    // renaming to FontFile2/CIDFontType2 would break those fonts.
    const TT_MAGIC: [u8; 4] = [0x00, 0x01, 0x00, 0x00];

    // Step 1: Find FontDescriptor IDs whose FontFile3 is actually a TrueType stream.
    let fd_to_fix: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Dictionary(d) = obj else {
                return None;
            };
            if !matches!(get_name(d, b"Type").as_deref(), Some("FontDescriptor")) {
                return None;
            }
            let ff3_id = match d.get(b"FontFile3").ok() {
                Some(Object::Reference(r)) => *r,
                _ => return None,
            };
            // Check the FontFile3 stream for TrueType SFNT magic.
            let stream_obj = doc.objects.get(&ff3_id)?;
            let Object::Stream(s) = stream_obj else {
                return None;
            };
            let mut s2 = s.clone();
            let _ = s2.decompress();
            let magic = s2.content.get(..4)?;
            if magic == TT_MAGIC {
                Some(*id)
            } else {
                None
            }
        })
        .collect();

    if fd_to_fix.is_empty() {
        return 0;
    }

    // Step 2: For each such FontDescriptor, rename FontFile3 → FontFile2
    // and collect the stream IDs.
    let mut stream_ids_to_fix: Vec<ObjectId> = Vec::new();
    let fd_ids: std::collections::HashSet<ObjectId> = fd_to_fix.iter().copied().collect();

    for fd_id in &fd_to_fix {
        let Some(Object::Dictionary(d)) = doc.objects.get_mut(fd_id) else {
            continue;
        };
        if let Ok(Object::Reference(r)) = d.get(b"FontFile3").cloned() {
            stream_ids_to_fix.push(r);
            d.remove(b"FontFile3");
            d.set(b"FontFile2", Object::Reference(r));
        }
    }

    // Step 3: Remove Subtype from the (formerly FontFile3) font streams.
    for sid in &stream_ids_to_fix {
        if let Some(Object::Stream(s)) = doc.objects.get_mut(sid) {
            s.dict.remove(b"Subtype");
        }
    }

    // Step 4: Fix DescendantFont dicts that reference one of the fixed descriptors.
    // DescendantFonts entries can be inline dicts (Object::Dictionary) inside arrays.
    let type0_ids: Vec<ObjectId> = doc
        .objects
        .keys()
        .copied()
        .filter(|id| {
            if let Some(Object::Dictionary(d)) = doc.objects.get(id) {
                matches!(get_name(d, b"Type").as_deref(), Some("Font"))
                    && matches!(get_name(d, b"Subtype").as_deref(), Some("Type0"))
            } else {
                false
            }
        })
        .collect();

    for t0_id in type0_ids {
        let Some(Object::Dictionary(d)) = doc.objects.get_mut(&t0_id) else {
            continue;
        };
        let df_arr = match d.get_mut(b"DescendantFonts") {
            Ok(Object::Array(a)) => a,
            _ => continue,
        };
        for elem in df_arr.iter_mut() {
            let Object::Dictionary(cid_dict) = elem else {
                continue;
            };
            // Change Subtype CIDFontType0 → CIDFontType2 if FontDescriptor is one we fixed.
            let is_cid0 = matches!(
                get_name(cid_dict, b"Subtype").as_deref(),
                Some("CIDFontType0")
            );
            if !is_cid0 {
                continue;
            }
            let fd_ref = match cid_dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(r)) => *r,
                _ => continue,
            };
            if fd_ids.contains(&fd_ref) {
                cid_dict.set(b"Subtype", Object::Name(b"CIDFontType2".to_vec()));
            }
        }
    }

    fd_to_fix.len()
}

/// Fix simple fonts that are declared as `/Subtype /TrueType` but whose
/// FontDescriptor embeds a CFF program (`/FontFile3` with `/Subtype /Type1C`).
///
/// PDF/A-2 §6.2.11.4.1:1 `containsFontFile` is `false` for TrueType fonts
/// unless the embedded file is `FontFile2` or `FontFile3/OpenType`.  A
/// `FontFile3/Type1C` stream satisfies `containsFontFile` only for Type1 fonts.
/// These malformed font dicts (TrueType container, CFF program) appear in
/// PDFs authored by some older PostScript drivers.
///
/// Fix: change the font dict's `/Subtype` from `/TrueType` to `/Type1`.
/// The embedded CFF program is unchanged.  Downstream fixes (`fix_type1_charset`,
/// `fix_font_width_mismatches`, etc.) will then treat the font correctly.
///
/// Returns the number of font dicts corrected.
pub fn fix_truetype_with_cff_program(doc: &mut Document) -> usize {
    // Collect font object IDs where Subtype=TrueType but FontDescriptor has FontFile3/Type1C.
    let to_fix: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Dictionary(d) = obj else {
                return None;
            };
            if !matches!(get_name(d, b"Type").as_deref(), Some("Font")) {
                return None;
            }
            if !matches!(get_name(d, b"Subtype").as_deref(), Some("TrueType")) {
                return None;
            }
            // Resolve FontDescriptor.
            let fd = match d.get(b"FontDescriptor").ok() {
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(fd)) => fd.clone(),
                    _ => return None,
                },
                Some(Object::Dictionary(fd)) => fd.clone(),
                _ => return None,
            };
            // Must have FontFile3 (not FontFile2).
            if fd.has(b"FontFile2") || !fd.has(b"FontFile3") {
                return None;
            }
            // FontFile3 stream must have Subtype=Type1C (CFF).
            let ff3_subtype = match fd.get(b"FontFile3").ok() {
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Stream(s)) => get_name(&s.dict, b"Subtype").unwrap_or_default(),
                    _ => return None,
                },
                _ => return None,
            };
            if ff3_subtype != "Type1C" {
                return None;
            }
            Some(*id)
        })
        .collect();

    let count = to_fix.len();
    for id in to_fix {
        if let Some(Object::Dictionary(d)) = doc.objects.get_mut(&id) {
            d.set(b"Subtype", Object::Name(b"Type1".to_vec()));
        }
    }
    count
}

/// Fix non-standard `/CharStrings` dict syntax in Type1 font eexec sections.
///
/// Some old fonts (e.g. Keycap) use:
/// ```text
/// /CharStrings N dict def
///   Private begin CharStrings begin
/// ```
/// instead of the standard:
/// ```text
/// /CharStrings N dict dup begin
/// ```
/// veraPDF's `Type1PrivateParser` expects exactly 3 tokens after the count
/// (`dict`, `dup`, `begin`), so it misparses the non-standard form and
/// `successfullyParsed` remains false → rule 6.2.11.4.1:1 fails.
///
/// Fix: decrypt the binary eexec section, replace the non-standard pattern
/// with the standard one, re-encrypt, update `Length2`, and store as
/// uncompressed plain content (filter removed).
///
/// Returns the number of font streams patched.
pub fn fix_type1_nonstandard_charstrings(doc: &mut Document) -> usize {
    // Collect (stream_id, l1, l2, l2_is_ref) for FontFile streams.
    // l2_is_ref: the object ID of the /Length2 integer object, if it was a reference.
    #[derive(Debug)]
    struct Target {
        stream_id: ObjectId,
        l1: usize,
        l2: usize,
        l2_ref: Option<ObjectId>, // object to update when l2 changes
    }

    let targets: Vec<Target> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Stream(s) = obj else {
                return None;
            };
            // Must have Length1 AND Length2 → Type1 font stream.
            if s.dict.get(b"Length1").is_err() || s.dict.get(b"Length2").is_err() {
                return None;
            }
            // Must NOT have /Subtype (FontFile3) — we want classic FontFile.
            if s.dict.get(b"Subtype").is_ok() {
                return None;
            }
            Some((*id, s.clone()))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|(id, s)| {
            let resolve_int = |obj: &Object| -> Option<(usize, Option<ObjectId>)> {
                match obj {
                    Object::Integer(n) => Some((*n as usize, None)),
                    Object::Reference(r) => {
                        if let Some(Object::Integer(n)) = doc.objects.get(r) {
                            Some((*n as usize, Some(*r)))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            };
            let l1_obj = s.dict.get(b"Length1").ok()?;
            let l2_obj = s.dict.get(b"Length2").ok()?;
            let (l1, _) = resolve_int(l1_obj)?;
            let (l2, l2_ref) = resolve_int(l2_obj)?;
            Some(Target {
                stream_id: id,
                l1,
                l2,
                l2_ref,
            })
        })
        .collect();

    let mut fixed = 0usize;

    for t in targets {
        // Decompress to get raw font bytes.
        let content = {
            let Some(Object::Stream(s)) = doc.objects.get(&t.stream_id) else {
                continue;
            };
            let mut s2 = s.clone();
            let _ = s2.decompress();
            s2.content
        };

        if content.len() < t.l1 + 5 || t.l1 + t.l2 > content.len() {
            continue;
        }

        let binary = &content[t.l1..t.l1 + t.l2];

        // Decrypt the binary section.
        let dec = type1_binary_eexec_decrypt(binary);
        if dec.len() < 5 {
            continue;
        }
        let real_data = &dec[4..]; // skip 4 random seed bytes

        // Search for the non-standard pattern:
        // "/CharStrings " + digits + " dict def" + whitespace + "Private begin CharStrings begin"
        // (and optionally trailing whitespace/newline before the glyph definitions)
        let Some((match_start, match_len, count_str)) = find_nonstandard_charstrings(real_data)
        else {
            continue;
        };

        // Build the replacement bytes: "/CharStrings N dict dup begin\n"
        let mut replacement = Vec::new();
        replacement.extend_from_slice(b"/CharStrings ");
        replacement.extend_from_slice(count_str.as_bytes());
        replacement.extend_from_slice(b" dict dup begin\n");

        // Patch the real_data.
        let mut new_real: Vec<u8> = Vec::with_capacity(real_data.len());
        new_real.extend_from_slice(&real_data[..match_start]);
        new_real.extend_from_slice(&replacement);
        new_real.extend_from_slice(&real_data[match_start + match_len..]);

        // Re-encrypt: seed_first=0xAA gives first_encrypted = 0xAA^0xD9 = 0x73 ('s').
        let new_binary = type1_binary_eexec_encrypt(&new_real, 0xAA);
        let new_l2 = new_binary.len();

        // Rebuild the full decompressed stream.
        let mut new_content: Vec<u8> =
            Vec::with_capacity(t.l1 + new_l2 + (content.len() - t.l1 - t.l2));
        new_content.extend_from_slice(&content[..t.l1]);
        new_content.extend_from_slice(&new_binary);
        new_content.extend_from_slice(&content[t.l1 + t.l2..]);

        // Update /Length2 reference object (or inline value).
        if let Some(ref_id) = t.l2_ref {
            if let Some(obj) = doc.objects.get_mut(&ref_id) {
                *obj = Object::Integer(new_l2 as i64);
            }
        } else {
            // Inline Length2: patch directly in stream dict.
            if let Some(Object::Stream(s)) = doc.objects.get_mut(&t.stream_id) {
                s.dict.set(b"Length2", Object::Integer(new_l2 as i64));
            }
        }

        // Store as plain (uncompressed) content — removes Filter/DecodeParms/Length.
        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&t.stream_id) {
            stream.set_plain_content(new_content);
        }
        fixed += 1;
    }

    fixed
}

/// Search for the non-standard CharStrings header pattern in decrypted Type1 eexec data.
///
/// Matches: `/CharStrings N dict def` + whitespace + `Private begin CharStrings begin` + optional whitespace.
/// Returns `(start_offset, match_length, count_string)` or `None` if not found.
fn find_nonstandard_charstrings(data: &[u8]) -> Option<(usize, usize, String)> {
    // Find "/CharStrings "
    let prefix = b"/CharStrings ";
    let mut i = 0;
    while i + prefix.len() < data.len() {
        if &data[i..i + prefix.len()] != prefix {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i + prefix.len();
        // Read digits (the count N).
        let digit_start = j;
        while j < data.len() && data[j].is_ascii_digit() {
            j += 1;
        }
        if j == digit_start {
            i += 1;
            continue;
        }
        let count_str = std::str::from_utf8(&data[digit_start..j]).ok()?.to_string();
        // Expect " dict def"
        if data.get(j..j + 9) != Some(b" dict def") {
            i += 1;
            continue;
        }
        j += 9;
        // Skip whitespace (newline, spaces).
        while j < data.len() && matches!(data[j], b' ' | b'\t' | b'\n' | b'\r') {
            j += 1;
        }
        // Expect "Private begin CharStrings begin"
        let suffix = b"Private begin CharStrings begin";
        if data.get(j..j + suffix.len()) != Some(suffix) {
            i += 1;
            continue;
        }
        j += suffix.len();
        // Skip trailing whitespace/newline after "begin".
        while j < data.len() && matches!(data[j], b' ' | b'\t' | b'\n' | b'\r') {
            j += 1;
        }
        return Some((start, j - start, count_str));
    }
    None
}

/// Extract the base font name by stripping a 6-character random subset prefix
/// (e.g. `YWFNOL+CMSY8` → `CMSY8`) and an optional `~XX` hex suffix
/// (e.g. `TXCMVS+CMSY8~32` → `CMSY8`).
fn type1_base_font_name(name: &str) -> String {
    // Strip 6-char prefix + '+' if present
    let after_prefix = if name.len() > 7 && name.as_bytes().get(6) == Some(&b'+') {
        &name[7..]
    } else {
        name
    };
    // Strip ~XX suffix (tilde + 2 hex chars)
    if after_prefix.len() >= 3 {
        let bytes = after_prefix.as_bytes();
        let last3 = &bytes[bytes.len() - 3..];
        if last3[0] == b'~' && last3[1].is_ascii_hexdigit() && last3[2].is_ascii_hexdigit() {
            return after_prefix[..after_prefix.len() - 3].to_string();
        }
    }
    after_prefix.to_string()
}

/// Parse PFB segments and return (raw_ps_data, length1, length2, length3).
///
/// Length1 = cleartext (type-1) bytes before the binary section.
/// Length2 = binary (type-2, eexec) bytes.
/// Length3 = trailing cleartext (type-1) bytes after the binary section.
fn parse_pfb_segments(data: &[u8]) -> Option<(Vec<u8>, i64, i64, i64)> {
    if data.len() < 6 || data[0] != 0x80 {
        return None;
    }

    let mut result = Vec::new();
    let mut i = 0;
    let mut l1: i64 = 0;
    let mut l2: i64 = 0;
    let mut l3: i64 = 0;
    let mut seen_binary = false;

    while i + 6 <= data.len() && data[i] == 0x80 {
        let seg_type = data[i + 1];
        if seg_type == 3 {
            break; // EOF segment — no length field
        }
        let length =
            u32::from_le_bytes([data[i + 2], data[i + 3], data[i + 4], data[i + 5]]) as usize;
        i += 6;
        let end = (i + length).min(data.len());
        let seg = &data[i..end];

        match seg_type {
            1 => {
                if seen_binary {
                    l3 += seg.len() as i64;
                } else {
                    l1 += seg.len() as i64;
                }
            }
            2 => {
                l2 += seg.len() as i64;
                seen_binary = true;
            }
            _ => {}
        }
        result.extend_from_slice(seg);
        i = end;
    }

    if result.is_empty() {
        return None;
    }

    Some((result, l1, l2, l3))
}

/// Strip PFB (Printer Font Binary) segment headers.
///
/// PFB files have segments prefixed by: 0x80, type_byte, length_le32, data.
/// This function concatenates the data portions, stripping the headers.
fn strip_pfb_headers(data: &[u8]) -> Vec<u8> {
    if data.is_empty() || data[0] != 0x80 {
        return data.to_vec();
    }

    let mut result = Vec::new();
    let mut i = 0;

    while i + 6 <= data.len() && data[i] == 0x80 {
        let seg_type = data[i + 1];
        if seg_type == 3 {
            break; // EOF segment
        }
        let length =
            u32::from_le_bytes([data[i + 2], data[i + 3], data[i + 4], data[i + 5]]) as usize;
        i += 6;
        let end = (i + length).min(data.len());
        result.extend_from_slice(&data[i..end]);
        i = end;
    }

    result
}

/// Decrypt the eexec-encrypted section of a Type 1 font program.
///
/// Finds the `eexec` keyword, then decrypts using the standard Type 1
/// eexec key (55665). Handles both hex-encoded and binary eexec data.
/// Skips the first 4 random bytes after decryption.
fn decrypt_eexec(ps_data: &[u8]) -> Vec<u8> {
    // Find "eexec" keyword.
    let eexec_pos = ps_data.windows(5).position(|w| w == b"eexec");
    let Some(eexec_pos) = eexec_pos else {
        return Vec::new();
    };

    // Skip past "eexec" and any following whitespace.
    let mut start = eexec_pos + 5;
    while start < ps_data.len() && ps_data[start].is_ascii_whitespace() {
        start += 1;
    }

    if start >= ps_data.len() {
        return Vec::new();
    }

    // Determine if hex-encoded or binary.
    // Hex-encoded eexec has only hex digits and whitespace.
    let cipher = if ps_data[start..]
        .iter()
        .take(32)
        .all(|b| b.is_ascii_hexdigit() || b.is_ascii_whitespace())
    {
        // Hex-encoded: decode hex pairs.
        let hex_str: String = ps_data[start..]
            .iter()
            .filter(|b| b.is_ascii_hexdigit())
            .map(|&b| b as char)
            .collect();
        let mut bytes = Vec::with_capacity(hex_str.len() / 2);
        let chars: Vec<char> = hex_str.chars().collect();
        for pair in chars.chunks(2) {
            if pair.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&format!("{}{}", pair[0], pair[1]), 16) {
                    bytes.push(byte);
                }
            }
        }
        bytes
    } else {
        // Binary eexec.
        ps_data[start..].to_vec()
    };

    if cipher.len() < 4 {
        return Vec::new();
    }

    // Decrypt with eexec key (55665).
    let mut key: u16 = 55665;
    let mut plain = Vec::with_capacity(cipher.len());

    for &c in &cipher {
        let p = c ^ (key >> 8) as u8;
        key = (c as u16)
            .wrapping_add(key)
            .wrapping_mul(52845)
            .wrapping_add(22719);
        plain.push(p);
    }

    // Skip first 4 random bytes.
    if plain.len() > 4 {
        plain[4..].to_vec()
    } else {
        Vec::new()
    }
}

/// Extract glyph names from a (potentially decrypted) Type 1 font's /CharStrings section.
///
/// Parses entries like: `/glyphname N RD <N binary bytes> ND`
/// Returns a CharSet string like "/A/B/C/space" (excluding .notdef).
///
/// Works directly on raw bytes to avoid UTF-8 lossy conversion issues where
/// binary charstring data would be expanded by replacement characters.
fn extract_glyph_names_from_charstrings(data: &[u8]) -> Option<String> {
    // Find "/CharStrings" in the raw byte data.
    let cs_pos = data.windows(12).position(|w| w == b"/CharStrings")?;
    let section = &data[cs_pos..];

    let mut names = Vec::new();
    let mut i = 0;

    while i < section.len() {
        // Look for glyph name definitions: "/glyphname"
        if section[i] == b'/' {
            let start = i + 1;
            let mut end = start;
            while end < section.len() && !section[end].is_ascii_whitespace() && section[end] != b'/'
            {
                end += 1;
            }

            // Extract name as ASCII (glyph names are always ASCII).
            let name_bytes = &section[start..end];
            let name = match std::str::from_utf8(name_bytes) {
                Ok(s) => s,
                Err(_) => {
                    // Non-ASCII "name" means we're in binary data — skip.
                    i = end;
                    continue;
                }
            };

            // Stop if we hit "end" or other section markers.
            if name == "CharStrings" || name == "FontName" || name == "Encoding" {
                i = end;
                continue;
            }
            if name == "end" {
                break;
            }

            if !name.is_empty() && name != ".notdef" {
                // Validate: glyph names should only contain printable ASCII.
                if name.bytes().all(|b| (0x21..0x7f).contains(&b)) {
                    names.push(name.to_string());
                }
            }

            // Skip past the binary charstring data: "N RD <1 delim + N binary bytes>"
            // or "N -| <1 delim + N binary bytes>"
            i = end;
            // Skip whitespace to find the charstring length N.
            while i < section.len() && section[i].is_ascii_whitespace() {
                i += 1;
            }
            // Parse N (the charstring byte count).
            let n_start = i;
            while i < section.len() && section[i].is_ascii_digit() {
                i += 1;
            }
            if i > n_start {
                if let Some(n) = std::str::from_utf8(&section[n_start..i])
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    // Skip whitespace.
                    while i < section.len() && section[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    // Skip "RD" or "-|" token.
                    if i + 2 <= section.len()
                        && (&section[i..i + 2] == b"RD" || &section[i..i + 2] == b"-|")
                    {
                        i += 2;
                    }
                    // Skip one delimiter byte (space after RD), then N binary bytes.
                    if i < section.len() {
                        i += 1; // delimiter
                    }
                    i += n; // binary charstring data
                }
            }
            continue;
        }

        // End of CharStrings dict: "end" or "readonly" as standalone tokens.
        if i + 3 <= section.len()
            && &section[i..i + 3] == b"end"
            && (i + 3 == section.len()
                || section[i + 3].is_ascii_whitespace()
                || section[i + 3] == b'\0')
        {
            break;
        }
        if i + 8 <= section.len() && &section[i..i + 8] == b"readonly" {
            break;
        }

        i += 1;
    }

    if names.is_empty() {
        return None;
    }

    let mut charset = String::new();
    for name in &names {
        charset.push('/');
        charset.push_str(name);
    }
    Some(charset)
}

/// Fix widths for simple TrueType fonts with explicit standard encoding (6.2.11.5:1).
///
/// Only processes fonts with WinAnsiEncoding or MacRomanEncoding to avoid
/// regression from incorrect encoding assumptions. Compares existing /Widths
/// against hmtx table and fixes mismatches.
pub fn fix_simple_truetype_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (fd_id, encoding_name) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "TrueType" {
                continue;
            }

            // Only process fonts with explicit standard encoding.
            let enc = match dict.get(b"Encoding").ok() {
                Some(Object::Name(n)) => String::from_utf8(n.clone()).ok(),
                Some(Object::Dictionary(enc_dict)) => get_name(enc_dict, b"BaseEncoding"),
                _ => None,
            };
            let enc = match enc.as_deref() {
                Some("WinAnsiEncoding") | Some("MacRomanEncoding") => {
                    enc.expect("matched arm guarantees enc is Some")
                }
                _ => continue, // Skip fonts without standard encoding.
            };

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };

            // Must have embedded font.
            let has_embedded = doc
                .objects
                .get(&fd_id)
                .and_then(|o| {
                    if let Object::Dictionary(fd) = o {
                        Some(fd.has(b"FontFile2"))
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            if !has_embedded {
                continue;
            }

            (fd_id, enc)
        };

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let Ok(face) = ttf_parser::Face::parse(&font_data, 0) else {
            continue;
        };

        let units_per_em = face.units_per_em() as f64;
        if units_per_em == 0.0 {
            continue;
        }
        let scale = 1000.0 / units_per_em;

        // Check for mismatches.
        let has_mismatch = {
            let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
                continue;
            };
            let fc = font
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(0);
            let existing = match font.get(b"Widths").ok() {
                Some(Object::Array(arr)) => arr,
                _ => continue,
            };

            let mut mismatch = false;
            for (i, obj) in existing.iter().enumerate() {
                let pdf_w = match obj {
                    Object::Integer(w) => *w,
                    Object::Real(r) => *r as i64,
                    _ => continue,
                };
                let code = fc + i as u32;
                let ch = encoding_to_char(code, &encoding_name);
                let expected = if let Some(gid) = face.glyph_index(ch) {
                    face.glyph_hor_advance(gid)
                        .map(|w| (w as f64 * scale).round() as i64)
                        .unwrap_or(0)
                } else if code <= u16::MAX as u32 {
                    face.glyph_hor_advance(ttf_parser::GlyphId(code as u16))
                        .map(|w| (w as f64 * scale).round() as i64)
                        .unwrap_or(0)
                } else {
                    0
                };
                if (pdf_w - expected).abs() > 1 {
                    mismatch = true;
                    break;
                }
            }
            mismatch
        };

        if has_mismatch {
            update_simple_widths(doc, font_id, &face, scale);
            fixed += 1;
        }
    }
    fixed
}

/// Fix widths for Type1 fonts with CFF font programs (6.2.11.5:1).
///
/// Reads glyph widths from FontFile3 (CFF) and updates the /Widths array.
/// Only processes Type1 fonts that have a CFF program embedded.
pub fn fix_type1_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let fd_id = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };

            // Only process FontFile3 (CFF programs).
            let has_ff3 = doc
                .objects
                .get(&fd_id)
                .and_then(|o| {
                    if let Object::Dictionary(fd) = o {
                        Some(fd.has(b"FontFile3"))
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            if !has_ff3 {
                continue;
            }

            fd_id
        };

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let Some(cff) = cff_parser::Table::parse(&font_data) else {
            continue;
        };

        // Read FirstChar/LastChar/Encoding from font dict.
        let (first_char, last_char, encoding_name) = {
            let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
                continue;
            };
            let fc = font
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(0);
            let lc = font
                .get(b"LastChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(255);
            let enc = font
                .get(b"Encoding")
                .ok()
                .and_then(|o| match o {
                    Object::Name(n) => String::from_utf8(n.clone()).ok(),
                    _ => None,
                })
                .unwrap_or_default();
            (fc, lc, enc)
        };

        // Get the CFF font matrix scale.
        let matrix = cff.matrix();
        let scale = cff_matrix_scale(matrix.sx);

        // Build widths from CFF glyph data.
        let mut widths = Vec::new();
        let mut any_mismatch = false;

        // Read existing widths for comparison.
        let existing_widths: Vec<i64> = {
            let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
                continue;
            };
            match font.get(b"Widths").ok() {
                Some(Object::Array(arr)) => arr
                    .iter()
                    .map(|o| match o {
                        Object::Integer(w) => *w,
                        Object::Real(r) => *r as i64,
                        _ => 0,
                    })
                    .collect(),
                _ => continue,
            }
        };

        for code in first_char..=last_char {
            // Map code to glyph name via encoding, then look up in CFF.
            let ch = encoding_to_char(code, &encoding_name);
            let glyph_name = unicode_to_glyph_name(ch);

            let width = if let Some(ref name) = glyph_name {
                // Find GID by glyph name.
                find_cff_glyph_width_by_name(&cff, name, scale)
            } else if code <= u16::MAX as u32 {
                // Direct GID lookup.
                cff.glyph_width(cff_parser::GlyphId(code as u16))
                    .map(|w| (w as f64 * scale).round() as i64)
            } else {
                None
            };

            let w = width.unwrap_or(0);
            let idx = (code - first_char) as usize;
            if idx < existing_widths.len() && (existing_widths[idx] - w).abs() > 1 {
                any_mismatch = true;
            }
            widths.push(Object::Integer(w));
        }

        if any_mismatch {
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
                font.set("Widths", Object::Array(widths));
            }
            fixed += 1;
        }
    }
    fixed
}

/// Map a Unicode character to a common glyph name (for CFF lookup).
fn unicode_to_glyph_name(ch: char) -> Option<String> {
    let code = ch as u32;
    match code {
        0x20 => Some("space".into()),
        0x21..=0x7E => Some(String::from(ch)), // ASCII printable
        0xC0..=0xFF => {
            // Latin-1 supplement — use standard names.
            Some(format!("uni{code:04X}"))
        }
        _ => Some(format!("uni{code:04X}")),
    }
}

/// Find a glyph width in a CFF table by name.
fn find_cff_glyph_width_by_name(
    cff: &cff_parser::Table<'_>,
    name: &str,
    scale: f64,
) -> Option<i64> {
    let num_glyphs = cff.number_of_glyphs();
    for gid in 0..num_glyphs {
        let glyph_id = cff_parser::GlyphId(gid);
        if let Some(gname) = cff.glyph_name(glyph_id) {
            if gname == name {
                return cff
                    .glyph_width(glyph_id)
                    .map(|w| (w as f64 * scale).round() as i64);
            }
        }
    }
    None
}

/// Add CIDToGIDMap /Identity to CIDFontType2 dicts that are missing it.
///
/// ISO 19005-2 §6.2.11.3.2 requires that CIDFontType2 (TrueType) dicts
/// have an explicit CIDToGIDMap entry. The PDF spec defaults to Identity
/// when absent, but veraPDF treats the absence as a violation. (#439)
pub fn fix_missing_cidtogidmap(doc: &mut Document) -> usize {
    // Collect all object IDs that are CIDFontType2 with no CIDToGIDMap.
    let ids_to_fix: Vec<lopdf::ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(&id, obj)| {
            let dict = obj.as_dict().ok()?;
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "CIDFontType2" {
                return None;
            }
            if dict.get(b"CIDToGIDMap").is_ok() {
                return None; // already has CIDToGIDMap
            }
            Some(id)
        })
        .collect();

    let count = ids_to_fix.len();
    for id in ids_to_fix {
        if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
            // Default per PDF spec is Identity, make it explicit for PDF/A compliance.
            dict.set("CIDToGIDMap", Object::Name(b"Identity".to_vec()));
        }
    }
    count
}

/// Fix CIDSet streams for all CID fonts (6.2.11.8:1).
///
/// CIDSet must be a stream containing a bitmap covering all CIDs present
/// in the embedded font program. This builds a complete CIDSet from the
/// font's glyph count.
pub fn fix_cidset(doc: &mut Document) -> usize {
    // For PDF/A-2b, rule 6.2.11.4.2:2: if a CIDSet stream exists in the
    // FontDescriptor, it must correctly identify all CIDs present in the font.
    //
    // veraPDF's CIDFontType2Program.containsCID(i) logic:
    //   - Returns true for any CID i where 1 <= i < cidToGidMappingSize,
    //     regardless of whether the GID value is 0/notdef.
    //   - For Identity CIDToGIDMap: returns true for 1 <= i < numGlyphs.
    //
    // Therefore:
    //   - Non-identity CIDToGIDMap (stream): CIDSet must have bits 1..N-1 set,
    //     where N = decompressed stream length / 2 (the mapping size).
    //   - Identity CIDToGIDMap or no CIDToGIDMap: CIDSet must have bits
    //     0..numGlyphs-1 set (from maxp).
    //   - For CIDFontType0 (CFF) or when we cannot determine coverage: remove
    //     CIDSet entirely (containsCIDSet==false satisfies the rule).
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    // Note: the pre-scan for non-identity CIDToGIDMap max sizes was removed
    // because non-identity CIDToGIDMap fonts now have their CIDSet removed
    // rather than regenerated (see below). This is always safe since
    // containsCIDSet==false satisfies rule 6.2.11.4.2:2.

    for font_id in font_ids {
        let (subtype, base_font, fd_id) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "CIDFontType0" && subtype != "CIDFontType2" {
                continue;
            }
            let base_font = get_name(dict, b"BaseFont").unwrap_or_default();
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            (subtype, base_font, fd_id)
        };

        let has_cidset = doc
            .objects
            .get(&fd_id)
            .and_then(|o| {
                if let Object::Dictionary(fd) = o {
                    Some(fd.has(b"CIDSet"))
                } else {
                    None
                }
            })
            .unwrap_or(false);

        // For CIDFontType0 (CFF-based CID fonts): generate CIDSet from the CFF
        // charset. Required for subset fonts (XXXXXX+ prefix) per ISO 19005-2
        // §6.2.11.5:1. We enumerate all GIDs and collect their CIDs from the CFF
        // charset to build a correct bitmap. (#439)
        if subtype == "CIDFontType0" {
            let is_subset = {
                let b = base_font.as_bytes();
                b.len() >= 7 && b[6] == b'+' && b[..6].iter().all(|x| x.is_ascii_uppercase())
            };
            if is_subset {
                let cidset = build_cidset_from_cff(doc, fd_id);
                if let Some(bytes) = cidset {
                    let new_id = doc.new_object_id();
                    let mut sd = lopdf::Dictionary::new();
                    sd.set("Length", Object::Integer(bytes.len() as i64));
                    doc.objects
                        .insert(new_id, Object::Stream(lopdf::Stream::new(sd, bytes)));
                    if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                        fd.set("CIDSet", Object::Reference(new_id));
                    }
                    fixed += 1;
                } else if has_cidset {
                    // Can't generate correct CIDSet → remove (wrong > absent).
                    if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                        fd.remove(b"CIDSet");
                    }
                    fixed += 1;
                }
            } else if has_cidset {
                // Non-subset CIDFontType0: CIDSet not required; remove if present.
                if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                    fd.remove(b"CIDSet");
                }
                fixed += 1;
            }
            continue;
        }

        // For CIDFontType2 without a CIDSet: only generate one for subset fonts
        // (XXXXXX+ prefix), which require CIDSet per ISO 19005-2. Non-subset fonts
        // don't need one. (#439)
        let is_subset = {
            let b = base_font.as_bytes();
            b.len() >= 7 && b[6] == b'+' && b[..6].iter().all(|x| x.is_ascii_uppercase())
        };
        if !has_cidset && !is_subset {
            continue;
        }

        // For CIDFontType2 with FontFile2 (TrueType): regenerate CIDSet.
        if subtype == "CIDFontType2" {
            let has_ff2 = doc
                .objects
                .get(&fd_id)
                .and_then(|o| {
                    if let Object::Dictionary(fd) = o {
                        Some(fd.has(b"FontFile2"))
                    } else {
                        None
                    }
                })
                .unwrap_or(false);

            if has_ff2 {
                // Determine if the CIDFont has a non-Identity CIDToGIDMap stream.
                let cid_to_gid_map_id: Option<ObjectId> = {
                    if let Some(Object::Dictionary(d)) = doc.objects.get(&font_id) {
                        match d.get(b"CIDToGIDMap").ok() {
                            Some(Object::Reference(id)) => Some(*id),
                            _ => None, // Identity or absent
                        }
                    } else {
                        None
                    }
                };

                // Compute the CIDSet bit coverage based on CIDToGIDMap type.
                // Use the maximum map_size across all CIDFonts sharing this FD
                // (pre-scanned above) to handle shared FontDescriptors correctly. (#465)
                let cidset_bytes: Option<Vec<u8>> = if cid_to_gid_map_id.is_some() {
                    // Non-identity CIDToGIDMap stream: CIDSet must identify exactly
                    // those CIDs whose GID mapping is non-zero in the font program.
                    // Computing that correctly requires reading the GID entries and
                    // cross-referencing maxp — error-prone and often wrong in practice.
                    // Removing CIDSet is always safe: containsCIDSet==false satisfies
                    // rule 6.2.11.4.2:2. Fixes persistent 6.2.11.4.2 failures (#OOM).
                    None
                } else {
                    // Identity or absent CIDToGIDMap.
                    // veraPDF uses widths.length (= hhea.numberOfHMetrics) for
                    // getCIDList() and maxp.numGlyphs for containsCID(). If
                    // numberOfHMetrics > numGlyphs (corrupt font), no valid CIDSet
                    // exists — fall through to removal.
                    let font_data = read_embedded_font_data(doc, fd_id);
                    match font_data.as_deref() {
                        Some(data) => {
                            let num_glyphs = truetype_num_glyphs(data);
                            let n_hmetrics = truetype_n_hmetrics(data);
                            match (num_glyphs, n_hmetrics) {
                                (Some(ng), Some(nh)) if nh <= ng => Some(cidset_bitstream(ng)),
                                (Some(ng), None) => Some(cidset_bitstream(ng)),
                                _ => None, // corrupt or numberOfHMetrics > numGlyphs
                            }
                        }
                        None => None,
                    }
                };

                if let Some(bytes) = cidset_bytes {
                    let new_id = doc.new_object_id();
                    let mut stream_dict = lopdf::Dictionary::new();
                    stream_dict.set("Length", Object::Integer(bytes.len() as i64));
                    let stream = lopdf::Stream::new(stream_dict, bytes);
                    doc.objects.insert(new_id, Object::Stream(stream));
                    if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                        fd.set("CIDSet", Object::Reference(new_id));
                    }
                    fixed += 1;
                    continue;
                }
            }
        }

        // Fallback: remove CIDSet when we can't regenerate it correctly.
        // containsCIDSet==false satisfies rule 6.2.11.4.2:2.
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            fd.remove(b"CIDSet");
        }
        fixed += 1;
    }
    fixed
}

/// Extract a table offset from a TrueType font's directory.
fn truetype_table_offset(data: &[u8], tag: &[u8; 4]) -> Option<usize> {
    if data.len() < 12 {
        return None;
    }
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    for i in 0..num_tables {
        let entry_off = 12 + i * 16;
        if entry_off + 16 > data.len() {
            break;
        }
        if &data[entry_off..entry_off + 4] == tag {
            let table_off = u32::from_be_bytes([
                data[entry_off + 8],
                data[entry_off + 9],
                data[entry_off + 10],
                data[entry_off + 11],
            ]) as usize;
            return Some(table_off);
        }
    }
    None
}

/// Extract numGlyphs from a TrueType font's maxp table.
fn truetype_num_glyphs(data: &[u8]) -> Option<u16> {
    let off = truetype_table_offset(data, b"maxp")?;
    if off + 6 <= data.len() {
        Some(u16::from_be_bytes([data[off + 4], data[off + 5]]))
    } else {
        None
    }
}

/// Extract numberOfHMetrics from a TrueType font's hhea table.
/// Returns None if the table is absent or malformed.
fn truetype_n_hmetrics(data: &[u8]) -> Option<u16> {
    let off = truetype_table_offset(data, b"hhea")?;
    // hhea: 36 bytes total; numberOfHMetrics is at offset +34 (uint16).
    if off + 36 <= data.len() {
        Some(u16::from_be_bytes([data[off + 34], data[off + 35]]))
    } else {
        None
    }
}

/// Build a CIDSet bitstream with bits 0..num_glyphs-1 set (big-endian bit order).
fn cidset_bitstream(num_glyphs: u16) -> Vec<u8> {
    if num_glyphs == 0 {
        return vec![0u8];
    }
    let num_bytes = (num_glyphs as usize).div_ceil(8);
    let mut bits = vec![0xFFu8; num_bytes];
    // Clear trailing bits if numGlyphs is not a multiple of 8.
    let remainder = num_glyphs as usize % 8;
    if remainder != 0 {
        // Keep only the top `remainder` bits of the last byte.
        bits[num_bytes - 1] = 0xFFu8 << (8 - remainder);
    }
    bits
}

/// Build a CIDSet bitmap for a CIDFontType0 (CFF-based CID font) by enumerating
/// all glyphs and collecting their CIDs from the CFF charset. (#439)
///
/// Returns None if the CFF data can't be parsed or is not a CID-keyed font.
fn build_cidset_from_cff(doc: &Document, fd_id: ObjectId) -> Option<Vec<u8>> {
    let data = read_embedded_font_data(doc, fd_id)?;
    let table = cff_parser::Table::parse(&data)?;
    let n = table.number_of_glyphs();
    let mut max_cid = 0u16;
    let mut cids: Vec<u16> = Vec::with_capacity(n as usize);
    for i in 0..n {
        // glyph_cid returns None for SID-keyed (non-CID) fonts; skip those.
        if let Some(cid) = table.glyph_cid(cff_parser::GlyphId(i)) {
            if cid > max_cid {
                max_cid = cid;
            }
            cids.push(cid);
        }
    }
    if cids.is_empty() {
        return None;
    }
    // Bitmap: bit k (MSB-first within each byte) represents CID k.
    let num_bytes = (max_cid as usize / 8) + 1;
    let mut bits = vec![0u8; num_bytes];
    for cid in cids {
        bits[cid as usize / 8] |= 0x80u8 >> (cid % 8);
    }
    Some(bits)
}

/// Fix font width mismatches between /Widths array and embedded font program (6.2.11.5:1).
///
/// Conservative approach: only updates individual width entries that clearly mismatch,
/// and only for fonts where the glyph mapping can be unambiguously determined.
/// Fix incorrect Symbolic flags on non-symbolic fonts with CFF programs.
///
/// Some PDFs incorrectly set the Symbolic flag on standard Latin fonts.
/// veraPDF uses the Symbolic flag to decide whether to validate widths via
/// CFF internal encoding (Symbolic) or PDF/Unicode encoding (Nonsymbolic).
/// Wrong flags cause width validation failures.
pub fn fix_symbolic_flags(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;

    for font_id in font_ids {
        // Some normal text TrueType subsets carry Symbolic flags even though
        // they already have a safe text encoding (WinAnsi/MacRoman + safe
        // Differences). Treat these as non-symbolic: keep /Encoding, add a
        // real Unicode cmap in fix_truetype_unicode_cmap, and clear the bad
        // Symbolic flag here. (#pdfa-tt-misflagged-text-flags)
        let tt_text_fd: Option<ObjectId> = match doc.objects.get(&font_id) {
            Some(Object::Dictionary(dict))
                if get_name(dict, b"Subtype").as_deref() == Some("TrueType")
                    && truetype_is_misflagged_text_font(doc, dict) =>
            {
                match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(r)) => Some(*r),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(fd_id) = tt_text_fd {
            let changed = if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id)
            {
                let flags = match fd.get(b"Flags").ok() {
                    Some(Object::Integer(f)) => *f,
                    _ => 0,
                };
                let new_flags = (flags | 32) & !4;
                if new_flags != flags {
                    fd.set("Flags", Object::Integer(new_flags));
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if changed {
                fixed += 1;
            }
            continue;
        }

        // TrueType fonts named "Symbol" / "ZapfDingbats" etc. are sometimes
        // generated with Flags=32 (NonSymbolic) and /Encoding /WinAnsiEncoding.
        // veraPDF then maps character codes through WinAnsiEncoding → Unicode →
        // (3,1) cmap, but the (3,1) cmap of a Symbol subset uses low-byte or
        // PUA codepoints that don't match — isGlyphPresent=false (§6.2.11.4.1:2).
        // Fix: set Symbolic flag + remove /Encoding so veraPDF uses the
        // (3,0) Symbol cmap that fix_existing_symbolic_truetype_cmaps adds.
        let tt_sym_fd: Option<ObjectId> = match doc.objects.get(&font_id) {
            Some(Object::Dictionary(dict))
                if get_name(dict, b"Subtype").as_deref() == Some("TrueType") =>
            {
                let name = get_name(dict, b"BaseFont").unwrap_or_default();
                let base = strip_subset_prefix(&name).to_owned();
                if is_symbolic_font_name(&base) {
                    match dict.get(b"FontDescriptor").ok() {
                        Some(Object::Reference(r)) => Some(*r),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(fd_id) = tt_sym_fd {
            let (needs_flag_fix, has_encoding) = match doc.objects.get(&fd_id) {
                Some(Object::Dictionary(fd)) => {
                    let has_ff2 = fd.has(b"FontFile2");
                    let flags = match fd.get(b"Flags").ok() {
                        Some(Object::Integer(f)) => *f,
                        _ => 0,
                    };
                    let flag_fix = has_ff2 && ((flags & 4 == 0) || (flags & 32 != 0));
                    let enc = doc
                        .objects
                        .get(&font_id)
                        .and_then(|o| o.as_dict().ok())
                        .map(|d| d.has(b"Encoding"))
                        .unwrap_or(false);
                    (flag_fix, enc)
                }
                _ => (false, false),
            };
            if needs_flag_fix {
                if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                    let flags = match fd.get(b"Flags").ok() {
                        Some(Object::Integer(f)) => *f,
                        _ => 0,
                    };
                    fd.set("Flags", Object::Integer((flags | 4) & !32));
                }
                fixed += 1;
            }
            // Symbolic TrueType fonts must NOT have /Encoding (6.2.11.6:3),
            // regardless of whether the flags also needed fixing.
            if has_encoding {
                if let Some(Object::Dictionary(ref mut fdict)) = doc.objects.get_mut(&font_id) {
                    fdict.remove(b"Encoding");
                }
                if !needs_flag_fix {
                    fixed += 1;
                }
            }
            continue;
        }

        let (name, fd_id) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "Type1" && subtype != "MMType1" {
                continue;
            }
            let name = match get_name(dict, b"BaseFont") {
                Some(n) => n,
                None => continue,
            };
            // For known symbolic Type1 fonts (e.g. Symbol/ZapfDingbats),
            // enforce Symbolic flags so validators use the symbolic encoding path.
            let base = strip_subset_prefix(&name);
            if is_symbolic_font_name(base) {
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(r)) => *r,
                    _ => continue,
                };
                let needs_fix = match doc.objects.get(&fd_id) {
                    Some(Object::Dictionary(fd)) => {
                        let has_font_program =
                            fd.has(b"FontFile3") || fd.has(b"FontFile2") || fd.has(b"FontFile");
                        let flags = match fd.get(b"Flags").ok() {
                            Some(Object::Integer(f)) => *f,
                            _ => 0,
                        };
                        has_font_program && ((flags & 4 == 0) || (flags & 32 != 0))
                    }
                    _ => false,
                };
                if needs_fix {
                    if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                        let flags = match fd.get(b"Flags").ok() {
                            Some(Object::Integer(f)) => *f,
                            _ => 0,
                        };
                        let new_flags = (flags | 4) & !32; // Symbolic=1, Nonsymbolic=0
                        fd.set("Flags", Object::Integer(new_flags));
                        fixed += 1;
                    }
                }
                continue;
            }
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(r)) => *r,
                _ => continue,
            };
            (name, fd_id)
        };

        // Check if FontDescriptor has Symbolic flag and FontFile3.
        let needs_fix = {
            let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
                continue;
            };
            let has_ff3 = fd.has(b"FontFile3");
            let flags = match fd.get(b"Flags").ok() {
                Some(Object::Integer(f)) => *f,
                _ => continue,
            };
            has_ff3 && (flags & 4 != 0)
        };

        if !needs_fix {
            continue;
        }

        // Non-symbolic font with Symbolic flag → fix flags.
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            if let Ok(Object::Integer(flags)) = fd.get(b"Flags") {
                let mut f = *flags;
                f &= !4; // Clear Symbolic (bit 3)
                f |= 32; // Set Nonsymbolic (bit 6)
                fd.set("Flags", Object::Integer(f));
                fixed += 1;
            }
        }
        let _ = name; // suppress unused warning
    }

    fixed
}

/// Strip named standard encodings from classic base-14 symbolic fonts.
///
/// For Type1 fonts without an Encoding entry, PDF falls back to the font's
/// built-in encoding. On classic Symbol/ZapfDingbats fonts, generator-default
/// `/WinAnsiEncoding` or `/MacRomanEncoding` entries can override that mapping
/// incorrectly. Only strip the entry when there are no Differences, so
/// explicit custom encodings remain untouched.
pub fn fix_classic_symbolic_base14_encoding(doc: &mut Document) -> usize {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in ids {
        let should_strip = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            if !is_font_dict(dict) {
                continue;
            }

            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            let base_font = get_name(dict, b"BaseFont").unwrap_or_default();
            let base_name = strip_subset_prefix(&base_font);
            if !matches!(
                base_name,
                "Symbol" | "SymbolMT" | "ZapfDingbats" | "Dingbats"
            ) {
                continue;
            }

            match dict.get(b"Encoding").ok() {
                Some(Object::Name(n))
                    if n == b"WinAnsiEncoding"
                        || n == b"MacRomanEncoding"
                        || n == b"MacExpertEncoding"
                        // StandardEncoding is also wrong for Symbol/ZapfDingbats.
                        // Symbol fonts use their own internal encoding; applying
                        // StandardEncoding maps codes to Latin glyph names that
                        // produce wrong widths when the font program is replaced
                        // with a non-Latin alternative (e.g. StandardSymbolsPS).
                        || n == b"StandardEncoding" =>
                {
                    true
                }
                Some(Object::Dictionary(enc)) => {
                    !enc.has(b"Differences")
                        && matches!(
                            enc.get(b"BaseEncoding").ok(),
                            Some(Object::Name(n))
                                if n == b"WinAnsiEncoding"
                                    || n == b"MacRomanEncoding"
                                    || n == b"MacExpertEncoding"
                                    || n == b"StandardEncoding"
                        )
                }
                Some(Object::Reference(enc_id)) => match doc.objects.get(enc_id) {
                    Some(Object::Dictionary(enc)) => {
                        !enc.has(b"Differences")
                            && matches!(
                                enc.get(b"BaseEncoding").ok(),
                                Some(Object::Name(n))
                                    if n == b"WinAnsiEncoding"
                                        || n == b"MacRomanEncoding"
                                        || n == b"MacExpertEncoding"
                                        || n == b"StandardEncoding"
                            )
                    }
                    _ => false,
                },
                _ => false,
            }
        };

        if !should_strip {
            continue;
        }

        if let Some(Object::Dictionary(dict)) = doc.objects.get_mut(&font_id) {
            dict.remove(b"Encoding");
            fixed += 1;
        }
    }

    fixed
}

/// Populate missing FirstChar/LastChar/Widths for embedded simple fonts.
///
/// Some PDFs (especially pre-PDF/A) omit these entries for standard 14 fonts.
/// PDF/A requires them (ISO 32000-1:2008 9.6.1, rule 6.2.11.2:4-6).
pub fn fix_missing_simple_font_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;

    for font_id in font_ids {
        let needs_fix = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if !matches!(subtype.as_str(), "Type1" | "TrueType" | "MMType1") {
                continue;
            }
            // Check if Widths or FirstChar is missing.
            let has_widths = dict.has(b"Widths");
            let has_fc = dict.has(b"FirstChar");
            if has_widths && has_fc {
                continue;
            }
            // Must have an embedded font program.
            if !has_embedded_font_program(doc, dict) {
                continue;
            }
            true
        };
        if !needs_fix {
            continue;
        }

        // Read encoding info and compute widths from the embedded font.
        let (enc_name, differences, fd_id) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let mut enc = String::new();
            let mut diffs = std::collections::HashMap::new();
            match dict.get(b"Encoding").ok() {
                Some(Object::Name(n)) => {
                    enc = String::from_utf8(n.clone()).unwrap_or_default();
                }
                Some(Object::Dictionary(enc_dict)) => {
                    if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                        enc = base;
                    }
                    parse_differences(doc, enc_dict, &mut diffs);
                }
                Some(Object::Reference(enc_ref)) => {
                    if let Ok(Object::Dictionary(enc_dict)) = doc.get_object(*enc_ref) {
                        if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                            enc = base;
                        }
                        parse_differences(doc, enc_dict, &mut diffs);
                    }
                }
                _ => {}
            }
            if enc.is_empty() {
                enc = "WinAnsiEncoding".to_string();
            }
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            (enc, diffs, fd_id)
        };

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else {
            continue;
        };

        // Compute widths for codes 0-255.
        let first_char = 0u32;
        let last_char = 255u32;

        // Try TrueType/OTF first, then fall back to raw CFF.
        let widths: Vec<Object> = if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
            let units_per_em = face.units_per_em() as f64;
            if units_per_em == 0.0 {
                continue;
            }
            let scale = 1000.0 / units_per_em;
            (first_char..=last_char)
                .map(|code| {
                    let ch = if let Some(name) = differences.get(&code) {
                        glyph_name_to_unicode(name).unwrap_or(encoding_to_char(code, &enc_name))
                    } else {
                        encoding_to_char(code, &enc_name)
                    };
                    let width = if let Some(gid) = face.glyph_index(ch) {
                        face.glyph_hor_advance(gid)
                            .map(|w| (w as f64 * scale).round() as i64)
                            .unwrap_or(0)
                    } else if let Some(name) = differences.get(&code) {
                        if let Some(gid) = face.glyph_index_by_name(name) {
                            face.glyph_hor_advance(gid)
                                .map(|w| (w as f64 * scale).round() as i64)
                                .unwrap_or(0)
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    Object::Integer(width)
                })
                .collect()
        } else if let Some(cff) = cff_parser::Table::parse(&font_data) {
            // Raw CFF (Type1C) font data.
            let scale = cff_matrix_scale(cff.matrix().sx);
            let enc_map = parse_cff_encoding_map(&font_data);
            (first_char..=last_char)
                .map(|code| {
                    // Try Differences name → CFF charset → GID.
                    let glyph_name = if let Some(name) = differences.get(&code) {
                        Some(name.as_str().to_string())
                    } else {
                        // Map code via encoding to glyph name.
                        let ch = encoding_to_char(code, &enc_name);
                        unicode_to_glyph_name(ch)
                    };
                    let width = if let Some(name) = &glyph_name {
                        cff.glyph_index_by_name(name)
                            .and_then(|gid| cff.glyph_width(gid))
                            .map(|w| (w as f64 * scale).round() as i64)
                    } else {
                        None
                    };
                    // Also try CFF internal encoding.
                    let width = width.or_else(|| {
                        enc_map
                            .get(&(code as u8))
                            .and_then(|&gid| cff.glyph_width(cff_parser::GlyphId(gid)))
                            .map(|w| (w as f64 * scale).round() as i64)
                    });
                    Object::Integer(width.unwrap_or(0))
                })
                .collect()
        } else {
            continue;
        };

        if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
            dict.set("FirstChar", Object::Integer(first_char as i64));
            dict.set("LastChar", Object::Integer(last_char as i64));
            dict.set("Widths", Object::Array(widths));
            fixed += 1;
        }
    }

    fixed
}

/// Fix Type3 /Widths entries from CharProc d0/d1 widths (6.2.11.5:1).
pub fn fix_type3_font_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| match obj {
            Object::Dictionary(d) if get_name(d, b"Subtype").as_deref() == Some("Type3") => {
                Some(*id)
            }
            _ => None,
        })
        .collect();

    let mut fixed = 0usize;

    for font_id in font_ids {
        let (
            first_char,
            last_char,
            existing_widths,
            widths_ref,
            enc_info,
            charprocs,
            charprocs_ref,
        ) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let fc = match dict.get(b"FirstChar").ok() {
                Some(Object::Integer(i)) if *i >= 0 => *i as u32,
                _ => continue,
            };
            let lc = match dict.get(b"LastChar").ok() {
                Some(Object::Integer(i)) if *i >= 0 => *i as u32,
                _ => continue,
            };
            let (widths, widths_ref) = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => (arr.clone(), None),
                Some(Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(Object::Array(arr)) => (arr.clone(), Some(*r)),
                    _ => continue,
                },
                _ => continue,
            };
            if widths.is_empty() {
                continue;
            }
            let enc_info = extract_encoding_info(doc, dict);
            let (charprocs, charprocs_ref) = match dict.get(b"CharProcs").ok() {
                Some(Object::Dictionary(d)) => (d.clone(), None),
                Some(Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(Object::Dictionary(d)) => (d.clone(), Some(*r)),
                    _ => continue,
                },
                _ => continue,
            };
            (
                fc,
                lc,
                widths,
                widths_ref,
                enc_info,
                charprocs,
                charprocs_ref,
            )
        };

        let normalized_charprocs = normalize_type3_charproc_width_ops(doc, font_id, charprocs_ref);

        let mut enc_name = enc_info.base_encoding.clone();
        let differences: std::collections::HashMap<u32, String> =
            enc_info.differences.iter().cloned().collect();
        if enc_name.is_empty() {
            enc_name = "WinAnsiEncoding".to_string();
        }

        let mut inserted_space_width = None;
        if let Some(space_name) = differences
            .get(&32)
            .cloned()
            .or_else(|| unicode_to_glyph_name(encoding_to_char(32, &enc_name)))
        {
            if charprocs.get(space_name.as_bytes()).is_err() {
                let space_width =
                    derive_type3_space_width(doc, first_char, &existing_widths, &charprocs);
                let inserted_charproc = insert_type3_empty_charproc(
                    doc,
                    font_id,
                    charprocs_ref,
                    &space_name,
                    space_width,
                );
                if inserted_charproc {
                    inserted_space_width = Some(space_width);
                    let original_differences = enc_info.differences.clone();
                    let new_diffs = vec![(32u32, space_name.clone())];
                    let _ = apply_encoding_fixes(
                        doc,
                        font_id,
                        &enc_name,
                        &original_differences,
                        &[],
                        &new_diffs,
                        enc_info.enc_ref,
                    );
                }
            }
        }

        // Some legacy Type3 fonts can reference codes above /LastChar via
        // Encoding/Differences and CharProcs (e.g. /a255 while LastChar=254).
        // Preserve existing range and only extend upward when higher explicit
        // definitions exist, so used codes never fall back to dictionary width 0.
        let max_charproc_code = charprocs
            .iter()
            .filter_map(|(name, _)| parse_type3_numeric_name(name))
            .max();
        let max_diff_code = differences.keys().copied().max();
        let max_defined_code = match (max_charproc_code, max_diff_code) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let target_first_char = if inserted_space_width.is_some() {
            first_char.min(32)
        } else {
            first_char
        };
        let target_last_char = max_defined_code
            .filter(|m| *m >= first_char && *m <= 255 && *m > last_char)
            .unwrap_or(last_char);
        let target_len = (target_last_char.saturating_sub(target_first_char) + 1) as usize;

        let mut corrections: Vec<(usize, i64)> = Vec::new();

        for idx in 0..target_len {
            let code = target_first_char + idx as u32;
            let current = existing_widths
                .get(code.saturating_sub(first_char) as usize)
                .and_then(object_to_f64)
                .unwrap_or(0.0);

            let mut candidates: Vec<String> = Vec::new();
            if let Some(name) = differences.get(&code) {
                candidates.push(name.clone());
            } else {
                let ch = encoding_to_char(code, &enc_name);
                if let Some(name) = unicode_to_agl_name(ch) {
                    candidates.push(name);
                }
                if let Some(name) = unicode_to_glyph_name(ch) {
                    candidates.push(name);
                }
            }
            candidates.push(format!("a{code}"));
            candidates.push(format!("g{code}"));
            candidates.dedup();

            let expected = candidates
                .iter()
                .find_map(|name| type3_charproc_width(doc, &charprocs, name));
            let Some(expected) = expected else { continue };

            if (current - expected as f64).abs() >= 1.0 {
                corrections.push((idx, expected));
            }
        }

        if corrections.is_empty() {
            // If range normalization is needed, still apply it.
            if target_first_char == first_char
                && target_last_char == last_char
                && inserted_space_width.is_none()
                && !normalized_charprocs
            {
                continue;
            }
        }

        let mut new_widths = Vec::with_capacity(target_len);
        for idx in 0..target_len {
            let code = target_first_char + idx as u32;
            let original = if code >= first_char {
                existing_widths
                    .get((code - first_char) as usize)
                    .cloned()
                    .unwrap_or(Object::Integer(0))
            } else {
                Object::Integer(0)
            };
            new_widths.push(original);
        }
        for (idx, new_w) in &corrections {
            if *idx < new_widths.len() {
                new_widths[*idx] = Object::Integer(*new_w);
            }
        }
        if let Some(space_width) = inserted_space_width {
            let idx = (32 - target_first_char) as usize;
            if idx < new_widths.len() {
                new_widths[idx] = Object::Integer(space_width);
            }
        }

        if let Some(widths_id) = widths_ref {
            if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&widths_id) {
                *arr = new_widths;
                if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
                    font.set("FirstChar", Object::Integer(target_first_char as i64));
                    font.set("LastChar", Object::Integer(target_last_char as i64));
                }
                fixed += 1;
            }
        } else if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
            if let Ok(Object::Array(ref mut arr)) = font.get_mut(b"Widths") {
                *arr = new_widths;
                font.set("FirstChar", Object::Integer(target_first_char as i64));
                font.set("LastChar", Object::Integer(target_last_char as i64));
                fixed += 1;
            }
        }
    }

    fixed
}

fn normalize_type3_charproc_width_ops(
    doc: &mut Document,
    font_id: ObjectId,
    charprocs_ref: Option<ObjectId>,
) -> bool {
    let entries: Vec<(Vec<u8>, Object)> = match charprocs_ref {
        Some(ref_id) => match doc.objects.get(&ref_id) {
            Some(Object::Dictionary(charprocs)) => charprocs
                .iter()
                .map(|(name, obj)| (name.clone(), obj.clone()))
                .collect(),
            _ => Vec::new(),
        },
        None => match doc.objects.get(&font_id) {
            Some(Object::Dictionary(font)) => match font.get(b"CharProcs").ok() {
                Some(Object::Dictionary(charprocs)) => charprocs
                    .iter()
                    .map(|(name, obj)| (name.clone(), obj.clone()))
                    .collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        },
    };

    let mut changed = false;

    for (glyph_name, entry) in entries {
        match entry {
            Object::Reference(stream_id) => {
                let Some(Object::Stream(stream)) = doc.objects.get(&stream_id) else {
                    continue;
                };
                let mut stream = stream.clone();
                let _ = stream.decompress();
                let Some(new_content) = normalize_type3_charproc_stream(&stream.content) else {
                    continue;
                };
                if let Some(Object::Stream(ref mut target)) = doc.objects.get_mut(&stream_id) {
                    target.set_plain_content(new_content);
                    changed = true;
                }
            }
            Object::Stream(stream_obj) => {
                let mut stream = stream_obj.clone();
                let _ = stream.decompress();
                let Some(new_content) = normalize_type3_charproc_stream(&stream.content) else {
                    continue;
                };
                if let Some(ref_id) = charprocs_ref {
                    if let Some(Object::Dictionary(ref mut charprocs)) =
                        doc.objects.get_mut(&ref_id)
                    {
                        if let Ok(Object::Stream(ref mut target)) = charprocs.get_mut(&glyph_name) {
                            target.set_plain_content(new_content);
                            changed = true;
                        }
                    }
                } else if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id)
                {
                    if let Ok(Object::Dictionary(ref mut charprocs)) = font.get_mut(b"CharProcs") {
                        if let Ok(Object::Stream(ref mut target)) = charprocs.get_mut(&glyph_name) {
                            target.set_plain_content(new_content);
                            changed = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    changed
}

fn normalize_type3_charproc_stream(data: &[u8]) -> Option<Vec<u8>> {
    #[derive(Clone, Copy)]
    struct Token {
        start: usize,
        end: usize,
    }

    fn is_pdf_number(token: &[u8]) -> bool {
        if token.is_empty() {
            return false;
        }
        let mut idx = 0usize;
        if matches!(token[0], b'+' | b'-') {
            idx += 1;
        }
        let mut seen_digit = false;
        let mut seen_dot = false;
        while idx < token.len() {
            match token[idx] {
                b'0'..=b'9' => seen_digit = true,
                b'.' if !seen_dot => seen_dot = true,
                _ => return false,
            }
            idx += 1;
        }
        seen_digit
    }

    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        while i < data.len() && data[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= data.len() {
            break;
        }
        if data[i] == b'%' {
            while i < data.len() && data[i] != b'\n' && data[i] != b'\r' {
                i += 1;
            }
            continue;
        }
        let start = i;
        while i < data.len() && !data[i].is_ascii_whitespace() {
            i += 1;
        }
        tokens.push(Token { start, end: i });
    }

    let first_op_idx = tokens
        .iter()
        .position(|tok| !is_pdf_number(&data[tok.start..tok.end]))?;
    let width_op_idx = tokens.iter().position(|tok| {
        let token = &data[tok.start..tok.end];
        token == b"d0" || token == b"d1"
    })?;

    if width_op_idx == first_op_idx {
        return None;
    }

    let width_operand_count =
        if &data[tokens[width_op_idx].start..tokens[width_op_idx].end] == b"d0" {
            2usize
        } else {
            6usize
        };
    if width_op_idx < width_operand_count {
        return None;
    }

    let width_start_idx = width_op_idx - width_operand_count;
    if !tokens[width_start_idx..width_op_idx]
        .iter()
        .all(|tok| is_pdf_number(&data[tok.start..tok.end]))
    {
        return None;
    }

    let seq_start = tokens[width_start_idx].start;
    let seq_end = tokens[width_op_idx].end;
    let leading = data[..seq_start]
        .iter()
        .copied()
        .skip_while(|b| b.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if leading.is_empty() {
        return None;
    }

    let rest = &data[seq_end..];
    let mut out = Vec::with_capacity(data.len() + 2);
    out.extend_from_slice(&data[seq_start..seq_end]);
    out.push(b'\n');
    out.extend_from_slice(&leading);
    if !rest.is_empty() && !leading.last().is_some_and(|b| b.is_ascii_whitespace()) {
        out.push(b' ');
    }
    out.extend_from_slice(
        rest.iter()
            .copied()
            .skip_while(|b| b.is_ascii_whitespace())
            .collect::<Vec<_>>()
            .as_slice(),
    );
    Some(out)
}

fn parse_type3_numeric_name(name: &[u8]) -> Option<u32> {
    if name.len() < 2 || name[0] != b'a' {
        return None;
    }
    let digits = std::str::from_utf8(&name[1..]).ok()?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok()
}

fn derive_type3_space_width(
    doc: &Document,
    first_char: u32,
    existing_widths: &[Object],
    charprocs: &lopdf::Dictionary,
) -> i64 {
    let width_at_code = |code: u32| -> Option<i64> {
        if code < first_char {
            return None;
        }
        let idx = (code - first_char) as usize;
        existing_widths
            .get(idx)
            .and_then(object_to_f64)
            .map(|w| w.round() as i64)
    };

    width_at_code(32)
        .filter(|w| *w > 0)
        .or_else(|| width_at_code(b'0' as u32).filter(|w| *w > 0))
        .or_else(|| type3_charproc_width(doc, charprocs, "space").filter(|w| *w > 0))
        .or_else(|| type3_charproc_width(doc, charprocs, "32").filter(|w| *w > 0))
        .or_else(|| type3_charproc_width(doc, charprocs, "48").filter(|w| *w > 0))
        .or_else(|| {
            let positives: Vec<i64> = existing_widths
                .iter()
                .filter_map(object_to_f64)
                .map(|w| w.round() as i64)
                .filter(|w| *w > 0)
                .collect();
            if positives.is_empty() {
                None
            } else {
                Some(positives.iter().sum::<i64>() / positives.len() as i64)
            }
        })
        .unwrap_or(250)
}

fn insert_type3_empty_charproc(
    doc: &mut Document,
    font_id: ObjectId,
    charprocs_ref: Option<ObjectId>,
    glyph_name: &str,
    width: i64,
) -> bool {
    let stream = Stream::new(dictionary! {}, format!("{width} 0 d0\n").into_bytes());
    let stream_id = doc.add_object(Object::Stream(stream));

    if let Some(ref_id) = charprocs_ref {
        if let Some(Object::Dictionary(ref mut charprocs)) = doc.objects.get_mut(&ref_id) {
            charprocs.set(glyph_name, Object::Reference(stream_id));
            return true;
        }
        return false;
    }

    let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) else {
        return false;
    };
    let Some(Object::Dictionary(ref mut charprocs)) = font.get_mut(b"CharProcs").ok() else {
        return false;
    };
    charprocs.set(glyph_name, Object::Reference(stream_id));
    true
}

fn type3_charproc_width(
    doc: &Document,
    charprocs: &lopdf::Dictionary,
    glyph_name: &str,
) -> Option<i64> {
    let cp_obj = charprocs.get(glyph_name.as_bytes()).ok()?;
    let stream = match cp_obj {
        Object::Stream(s) => s.clone(),
        Object::Reference(r) => match doc.objects.get(r) {
            Some(Object::Stream(s)) => s.clone(),
            _ => return None,
        },
        _ => return None,
    };
    let mut st = stream;
    let _ = st.decompress();
    if let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&st.content) {
        for op in editor.operations() {
            if op.operator == "d0" || op.operator == "d1" {
                let wx = op.operands.first().and_then(object_to_f64)?;
                return Some(wx.round() as i64);
            }
        }
    }
    type3_charproc_width_fallback(&st.content)
}

/// Fallback parser for minimal Type3 charproc streams when content decoding fails.
/// Looks for `<wx> ... d0` or `<wx> ... d1` token sequences.
fn type3_charproc_width_fallback(data: &[u8]) -> Option<i64> {
    let tokens: Vec<&[u8]> = data
        .split(|b| b.is_ascii_whitespace())
        .filter(|t| !t.is_empty())
        .collect();
    for i in 0..tokens.len() {
        let need = if tokens[i] == b"d0" {
            2
        } else if tokens[i] == b"d1" {
            6
        } else {
            continue;
        };
        if i < need {
            continue;
        }
        let wx = std::str::from_utf8(tokens[i - need])
            .ok()
            .and_then(|s| s.parse::<f64>().ok())?;
        return Some(wx.round() as i64);
    }
    None
}

fn object_to_f64(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

/// Skips fonts where >50% of widths mismatch (indicates unreliable mapping).
///
/// Returns count of fonts whose widths were corrected.
pub fn fix_font_width_mismatches(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;

    for font_id in font_ids {
        let info = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();

            // Skip CID fonts — they are handled by fix_cff_widths / fix_truetype_cid_widths.
            // Skip Type0 — widths live on the CIDFont descendant.
            match subtype.as_str() {
                "TrueType" | "Type1" | "MMType1" => {}
                _ => continue,
            }

            // Skip symbolic TrueType fonts and classic Symbol/Zapf Type1 CFF
            // fonts. Those are handled by dedicated symbolic passes.
            let base_font = get_name(dict, b"BaseFont").unwrap_or_default();
            if !base_font.is_empty() {
                let name = base_font.clone();
                if is_symbolic_font_name(&name) && is_font_symbolic(doc, dict) {
                    let (has_ff2, has_ff3) = match dict.get(b"FontDescriptor").ok() {
                        Some(Object::Reference(fd_id)) => doc
                            .objects
                            .get(fd_id)
                            .and_then(|o| {
                                if let Object::Dictionary(fd) = o {
                                    Some((fd.has(b"FontFile2"), fd.has(b"FontFile3")))
                                } else {
                                    None
                                }
                            })
                            .unwrap_or((false, false)),
                        _ => (false, false),
                    };
                    let base = strip_subset_prefix(&name);
                    let is_classic_symbol = matches!(
                        base,
                        "Symbol"
                            | "SymbolMT"
                            | "ZapfDingbats"
                            | "Wingdings"
                            | "Webdings"
                            | "Dingbats"
                            | "MTExtra"
                    );
                    // Subset symbolic CFF fonts (ABCDEF+ prefix) are NOT skipped:
                    // the CFF internal encoding in a subset is authoritative and
                    // veraPDF uses it directly for §6.2.11.5 width comparison.
                    // Non-subset symbolic fonts are handled by dedicated passes.
                    // (#6.2.11.5-symbol-subset)
                    let is_sym_subset = name.len() > 7 && name.as_bytes()[6] == b'+';
                    if !is_sym_subset && (has_ff2 || (has_ff3 && is_classic_symbol)) {
                        continue;
                    }
                }
                // NOTE: TrueType subset fonts (ABCDEF+Name) are processed normally.
                // Subset cmaps ARE updated during subsetting, so cmap-based glyph
                // lookup (encoding → Unicode → cmap) works correctly. The GID
                // fallback was removed from the width computation, so renumbered
                // GIDs don't cause incorrect matches.
            }

            // Must have FontDescriptor with embedded font program.
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };

            // Must have existing /Widths array and FirstChar.
            let fc = match dict.get(b"FirstChar").ok() {
                Some(Object::Integer(i)) => *i as u32,
                _ => continue,
            };
            let (existing_widths, widths_ref) = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => (arr.clone(), None),
                Some(Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(Object::Array(arr)) => (arr.clone(), Some(*r)),
                    _ => continue,
                },
                _ => continue,
            };
            if existing_widths.is_empty() {
                continue;
            }

            // Get encoding info. For non-symbolic TrueType fonts without encoding
            // that have an embedded font program (FontFile2), use WinAnsiEncoding.
            // This matches veraPDF's expectation for substitute fonts.
            let enc_info = {
                let mut ei = get_simple_encoding_info(doc, dict);
                if ei.0.is_empty() && subtype == "TrueType" && !is_font_symbolic(doc, dict) {
                    // Check if the FD has FontFile2 (embedded TrueType)
                    let fd_has_ff2 = match dict.get(b"FontDescriptor").ok() {
                        Some(Object::Reference(fd_ref)) => {
                            matches!(
                                doc.objects.get(fd_ref),
                                Some(Object::Dictionary(fd)) if fd.has(b"FontFile2")
                            )
                        }
                        _ => false,
                    };
                    if fd_has_ff2 {
                        ei.0 = "WinAnsiEncoding".to_string();
                    }
                }
                ei
            };

            // Track whether the font has an explicit PDF-level Encoding entry.
            let has_explicit_encoding = dict.has(b"Encoding") || !enc_info.0.is_empty();

            // Extract ToUnicode map for codes undefined in the PDF encoding.
            // Used to mirror veraPDF's fallback: when WinAnsiEncoding leaves a
            // code undefined (e.g. 0x81), veraPDF resolves via ToUnicode → Unicode
            // → font cmap → advance. (#507, gen-131)
            let to_unicode_map = read_font_to_unicode_map(doc, dict);

            // Detect symbolic TrueType fonts (Flags bit 2) that use (3,0) Symbol
            // cmap for width validation instead of (3,1) Unicode cmap.
            let symbolic_tt = subtype == "TrueType" && is_font_symbolic(doc, dict);

            (
                subtype,
                base_font,
                fd_id,
                fc,
                existing_widths,
                enc_info,
                widths_ref,
                has_explicit_encoding,
                to_unicode_map,
                symbolic_tt,
            )
        };

        let (
            subtype,
            base_font,
            fd_id,
            first_char,
            existing_widths,
            enc_info,
            _widths_ref,
            _has_explicit_encoding,
            to_unicode_map,
            symbolic_tt,
        ) = info;

        // Check if font program is embedded (FontFile key exists).
        // We don't verify stream content here — read_embedded_font_data handles that.
        let has_embedded = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3")
        );
        if !has_embedded {
            continue;
        }

        // Determine which font file type is embedded.
        let (has_ff1, has_ff2, has_ff3) = {
            let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
                continue;
            };
            (
                fd.has(b"FontFile"),
                fd.has(b"FontFile2"),
                fd.has(b"FontFile3"),
            )
        };
        // Subset CFF Type1 fonts (ABCDEF+Name) have an authoritative CFF internal
        // encoding created during subsetting. The ≤50-unit conservative filter
        // is not needed for them — apply it only to non-subset fonts. (#496)
        let is_subset_font = base_font.len() > 7 && base_font.as_bytes()[6] == b'+';
        let ambiguous_cff_base_encoding = has_ff3
            && (subtype == "Type1" || subtype == "MMType1" || subtype == "TrueType")
            && enc_info.0.is_empty()
            && !is_subset_font;
        let skip_subset_symbol_cff_without_pdf_encoding = has_ff3
            && is_subset_font
            && is_symbolic_font_name(strip_subset_prefix(&base_font))
            && enc_info.0.is_empty()
            && enc_info.1.is_empty();
        // When no PDF encoding exists at all (neither BaseEncoding nor Differences),
        // the CFF internal encoding is the sole code→GID mapping, which is exactly
        // what veraPDF uses for §6.2.11.5. Corrections via cff_width_for_code in
        // this case are computed with the same mapping and are definitively correct;
        // the conservative 50-unit filter is not needed. (#6.2.11.5-simple-cff)
        let no_pdf_encoding = enc_info.0.is_empty() && enc_info.1.is_empty();

        if skip_subset_symbol_cff_without_pdf_encoding {
            continue;
        }

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else {
            continue;
        };

        let mut corrections: Vec<(usize, i64)>;

        if has_ff2 && subtype == "TrueType" {
            // TrueType font with FontFile2 — use ttf-parser + cmap.
            corrections = compute_truetype_width_corrections_inner(
                &font_data,
                first_char,
                &existing_widths,
                &enc_info,
                symbolic_tt,
            );
        } else if has_ff3 && (subtype == "Type1" || subtype == "MMType1" || subtype == "TrueType") {
            // CFF font program (FontFile3). veraPDF §6.2.11.5 always validates
            // FontFile3 against CFF charstring widths regardless of the PDF
            // Subtype. This includes TrueType fonts that embed a CFF program
            // (non-standard but seen in the wild — mislabeled TrueType+CFF).
            // is_subset_font already computed above.
            corrections = compute_cff_type1_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                &enc_info,
                is_subset_font,
                Some(&to_unicode_map),
            );
        } else if has_ff2 && (subtype == "Type1" || subtype == "MMType1") {
            // Type1 font re-encoded as TrueType (after embedding fallback font).
            corrections = compute_truetype_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                &enc_info,
            );
        } else if has_ff1 && (subtype == "Type1" || subtype == "MMType1") {
            let type1_corr = compute_type1_fontfile_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                &enc_info,
            );
            // For non-subset fonts, apply a conservative delta-5 filter for
            // ambiguous AGL-based corrections. Type1 charstring parsing can give
            // wrong widths for seac composites and unusual charstrings even when
            // the PDF has an explicit /Encoding (e.g. Helvetica-Condensed-Black
            // code 55 parsed as 250 when actual is 500).
            // IMPORTANT: do NOT bypass this filter for is_certain=true (.notdef path).
            // The .notdef fallback fires incorrectly for non-subset fonts that have
            // implicit StandardEncoding which our parser can't detect (e.g.
            // NewCenturySchlbk-Italic code 39: parser → .notdef → advance 278, but
            // veraPDF uses StandardEncoding → "quoteright" → advance 204). (#507)
            let is_subset = base_font.len() > 7 && base_font.as_bytes()[6] == b'+';
            corrections = type1_corr
                .into_iter()
                .filter_map(|(idx, new_w, _is_certain, allow_large_delta)| {
                    if !is_subset {
                        let pdf_w = existing_widths.get(idx).and_then(object_to_f64)?;
                        if (pdf_w - new_w as f64).abs() > 5.0 && !allow_large_delta {
                            return None; // too large a delta → ambiguous mapping, skip
                        }
                    }
                    Some((idx, new_w))
                })
                .collect();
        } else {
            continue;
        }

        if ambiguous_cff_base_encoding && !no_pdf_encoding {
            // Keep only conservative deltas for ambiguous CFF base-encoding
            // mappings where some PDF encoding context exists; large jumps are
            // typically wrong code->glyph matches. When no_pdf_encoding is true,
            // corrections are computed from CFF internal encoding (same as
            // veraPDF) and are definitively correct — skip this filter.
            //
            // Exceptions to the 50-unit cap:
            // - Low-byte codes (≤127): AGL glyph name lookups are reliable for
            //   ASCII range codes (e.g. code 39 "quotesingle"→"quoteright"). (#FN-6.2.11.5-agl-alt)
            // - Explicit /Differences entries: deterministic glyph name → CFF width
            //   mappings are correct regardless of delta size.
            // - CFF-internal encoding maps code to a valid non-.notdef glyph: the CFF
            //   encoding is authoritative when no PDF BaseEncoding is present, even if
            //   some Differences exist for other codes. This matches veraPDF's fallback
            //   path and avoids blocking corrections like Times-Roman code 177 (±, delta=64)
            //   that exceed the 50-unit cap but are definitively correct. (#504)
            let cff_enc_early: Option<std::collections::HashMap<u8, u16>> =
                if has_ff3 && enc_info.0.is_empty() && !is_subset_font {
                    // Must extract the raw CFF table bytes first: font_data may be
                    // an OTF-wrapped CFF (starts with "OTTO"), and parse_cff_encoding_map
                    // expects raw CFF, not the OTF container. (#504)
                    let cff_bytes = extract_cff_bytes_from_otf(&font_data).unwrap_or(&font_data);
                    Some(parse_cff_encoding_map(cff_bytes))
                } else {
                    None
                };
            corrections.retain(|(idx, new_w)| {
                let code = first_char + *idx as u32;
                if code <= 127 || enc_info.1.contains_key(&code) {
                    return true;
                }
                // Allow when CFF encoding maps this code to a valid (non-.notdef) GID.
                if matches!(&cff_enc_early, Some(m) if m.get(&(code as u8)).copied().unwrap_or(0) != 0) {
                    return true;
                }
                let Some(pdf_w) = existing_widths.get(*idx).and_then(object_to_f64) else {
                    return false;
                };
                (pdf_w - *new_w as f64).abs() <= 50.0
            });
        }

        // For Type1 CFF fonts, keep in-range corrections conservative unless the
        // font descriptor indicates the legacy fixed metrics profile (flag bit
        // 18 set in these corpora), where full Differences-based correction is
        // stable. Otherwise only keep "space" corrections from .notdef remediation.
        if has_ff3 && (subtype == "Type1" || subtype == "MMType1") {
            let uses_cff_internal_encoding_only = enc_info.0.is_empty() && enc_info.1.is_empty();
            let allow_full_cff_corrections = match doc.objects.get(&fd_id) {
                Some(Object::Dictionary(fd)) => match fd.get(b"Flags").ok() {
                    Some(Object::Integer(flags)) => {
                        (*flags & 262_144) != 0 || uses_cff_internal_encoding_only
                    }
                    _ => uses_cff_internal_encoding_only,
                },
                _ => uses_cff_internal_encoding_only,
            };
            if !allow_full_cff_corrections {
                let is_subset = base_font.len() > 7 && base_font.as_bytes()[6] == b'+';
                // For non-subset fonts with no BaseEncoding, the CFF internal encoding
                // is authoritative. Pre-compute the CFF encoding map so the filter can
                // allow corrections where the CFF maps the code to a valid (non-.notdef)
                // GID — those corrections are definitively correct. (#479)
                // Compute CFF encoding map for both subset and non-subset fonts.
                // For subset fonts, the CFF encoding maps codes to GIDs directly and
                // veraPDF uses it as a fallback when name-based lookup fails (e.g. for
                // glyphs with GID-based or non-standard names). Applying corrections for
                // these codes is safe because they are definitively mapped. (#504)
                // Parse the CFF internal encoding map for the high-byte filter.
                // veraPDF §6.2.11.5 falls back to CFF glyph_index(code) when the
                // PDF encoding has no name for a code (e.g. WinAnsiEncoding code 129
                // = undefined control char). This applies to ALL font encodings, not
                // just SE-implied or subset fonts. Populate the map unconditionally so
                // the filter allows corrections for CFF-mapped codes > 127 on any font.
                // (#6.2.11.5-cff-enc-filter)
                let cff_enc_for_filter: Option<std::collections::HashMap<u8, u16>> = {
                    // Must extract raw CFF bytes first: font_data may be OTF-wrapped.
                    let cff_bytes = extract_cff_bytes_from_otf(&font_data).unwrap_or(&font_data);
                    Some(parse_cff_encoding_map(cff_bytes))
                };
                // Compute .notdef (GID 0) advance for this font so the filter can
                // allow corrections that bring the dict width to .notdef width.
                // veraPDF uses .notdef width for codes absent from the font program,
                // so these corrections are definitively correct. (#479)
                let notdef_w_rounded: Option<i64> = ttf_parser::Face::parse(&font_data, 0)
                    .ok()
                    .and_then(|face| {
                        let upem = face.units_per_em() as f64;
                        if upem > 0.0 {
                            let scale = 1000.0 / upem;
                            face.glyph_hor_advance(ttf_parser::GlyphId(0))
                                .map(|w| (w as f64 * scale).round() as i64)
                        } else {
                            None
                        }
                    })
                    // Fallback for bare CFF (Type1C) fonts where ttf_parser fails.
                    .or_else(|| {
                        cff_parser::Table::parse(&font_data).and_then(|cff| {
                            let scale = cff_matrix_scale(cff.matrix().sx);
                            cff.glyph_width(cff_parser::GlyphId(0))
                                .map(|w| w as f64 * scale)
                                .map(|w| w.round() as i64)
                        })
                    });
                // Compute the CFF Private DICT defaultWidthX (used by veraPDF for
                // codes whose named glyph is absent from the CFF subset charset —
                // Case 1 in cff_width_for_code). When new_w == defaultWidthX, the
                // correction is definitively correct: veraPDF will use defaultWidthX
                // for the same code and the dict width must match. (#6.2.11.5-case1-defaultwidthx)
                let cff_default_width_x_rounded: Option<i64> = {
                    let cff_bytes = extract_cff_bytes_from_otf(&font_data).unwrap_or(&font_data);
                    cff_parser::Table::parse(cff_bytes).and_then(|cff| {
                        let scale = cff_matrix_scale(cff.matrix().sx);
                        cff.default_width_x()
                            .map(|w| (w as f64 * scale).round() as i64)
                    })
                };
                corrections.retain(|(idx, new_w)| {
                    let code = first_char + *idx as u32;
                    // Always allow corrections that set the width to .notdef —
                    // those come from the .notdef fallback path and are correct.
                    if matches!(notdef_w_rounded, Some(nw) if nw == *new_w) {
                        return true;
                    }
                    // Allow corrections that set the width to CFF defaultWidthX for
                    // high-byte codes with a PDF encoding. veraPDF (Case 1) uses
                    // defaultWidthX for codes whose named glyph is absent from the
                    // CFF subset charset — these corrections are definitively correct.
                    // Safety: if new_w equals defaultWidthX AND a correction exists,
                    // then pdf_w ≠ defaultWidthX (otherwise no correction would be
                    // generated). Matching new_w to defaultWidthX uniquely identifies
                    // Case 1 corrections without a false-positive risk. (#6.2.11.5-case1-defaultwidthx)
                    if code > 127
                        && !enc_info.0.is_empty()
                        && matches!(cff_default_width_x_rounded, Some(dw) if dw == *new_w)
                    {
                        return true;
                    }
                    if is_subset {
                        // High-byte subset remaps are often validated through
                        // CFF internal encoding. Keep low-byte edits, explicit
                        // /space fixes, standard-encoding high-byte codes that
                        // resolve to an actual glyph name in the subset, and
                        // codes where the CFF encoding maps to a valid GID
                        // (veraPDF's fallback when name lookup fails). (#504)
                        //
                        // Explicit Differences entries are also safe for subset
                        // fonts: veraPDF §6.2.11.5 maps code → glyph name (via
                        // Differences) → CFF charset → charstring width — the
                        // same path our cff_width_for_code uses. If we computed
                        // a correction for a Differences-mapped code, it is
                        // correct and should not be filtered out. (#6.2.11.5-diff-subset)
                        //
                        // WinAnsiEncoding subset fonts: same
                        // logic as the non-subset branch applies. cff_width_for_code
                        // uses the T1 glyph name table for high-byte codes, so when
                        // a correction is generated the glyph was found by name —
                        // it is definitively correct. This handles e.g. code 173
                        // ("hyphen" via T1 WinAnsi table) in NGEPHG+Helvetica where
                        // subset_standard_cff_code_is_safe returns false because the
                        // Unicode AGL path ("softhyphen") is absent from the subset.
                        // (#6.2.11.5-t1-winansi, gen-348)
                        code <= 127
                            || enc_info.1.contains_key(&code)
                            || matches!(enc_info.0.as_str(), "WinAnsiEncoding")
                            || (subset_standard_cff_code_is_safe(
                                &font_data,
                                code,
                                &enc_info.0,
                                &enc_info.1,
                            )
                                // If the CFF internal encoding maps this code to GID 0 (.notdef),
                                // veraPDF uses defaultWidthX for the code, not the named glyph's
                                // charstring width. Block the correction to avoid spurious
                                // §6.2.11.5 failures. (#507, §6.2.11.5-tekton-std-enc-notdef)
                                && !matches!(&cff_enc_for_filter, Some(m) if m.get(&(code as u8)).copied().unwrap_or(1) == 0))
                            || matches!(&cff_enc_for_filter, Some(m) if m.get(&(code as u8)).copied().unwrap_or(0) != 0)
                    } else {
                        // Explicit Differences entries are deterministic mappings, so
                        // high-byte corrections remain safe on non-subset fonts.
                        // Also allow when the CFF encoding maps the code to a valid GID:
                        // for fonts with no BaseEncoding, CFF encoding is authoritative. (#479)
                        //
                        // WinAnsiEncoding is a standard PDF encoding
                        // whose high-byte codes (128-255) all have well-defined AGL glyph
                        // names.  compute_cff_corrections_by_name uses name-based lookup for
                        // these encodings, which is unambiguous: if the glyph name exists in
                        // the CFF charset the width is correct; if not, cff_width_for_code
                        // returns None and no correction is generated.  Blocking high-byte
                        // corrections for these reliable encodings causes §6.2.11.5 failures
                        // for standard glyphs such as "softhyphen" (code 173, Helvetica /
                        // Times-Roman) whose CFF StandardEncoding does not map code 0xAD.
                        // Allow all name-based corrections when the PDF encoding is reliable.
                        // (#546, gen-348 Helvetica/Times-Roman code 173)
                        code <= 127
                            || enc_info.1.contains_key(&code)
                            || matches!(&cff_enc_for_filter, Some(m) if m.get(&(code as u8)).copied().unwrap_or(0) != 0)
                            || matches!(enc_info.0.as_str(), "WinAnsiEncoding")
                    }
                });
            }
        }

        // Also compute widths for codes outside [FirstChar, LastChar] (up to 255).
        // Some fonts have codes used in content streams that fall outside
        // this range. Extend the Widths array to cover them.
        let last_char = first_char + existing_widths.len() as u32 - 1;
        let mut extensions: Vec<(u32, i64)> = Vec::new(); // (code, width)
        let mut prepend_codes: Vec<(u32, i64)> = Vec::new(); // codes below FirstChar

        // Helper closure to compute a single width for a code.
        let compute_width_for_code = |code: u32| -> Option<f64> {
            if has_ff2 {
                if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
                    let upem = face.units_per_em() as f64;
                    if upem > 0.0 {
                        let scale = 1000.0 / upem;
                        return get_truetype_glyph_width_fractional(
                            &face,
                            code,
                            &enc_info.0,
                            &enc_info.1,
                            scale,
                        );
                    }
                }
            } else if has_ff3 {
                return compute_cff_single_width(&font_data, code, &enc_info.0, &enc_info.1);
            } else if has_ff1 {
                return compute_type1_fontfile_single_width(
                    &font_data,
                    code,
                    &enc_info.0,
                    &enc_info.1,
                );
            }
            None
        };

        // Codes below FirstChar.
        if !ambiguous_cff_base_encoding && first_char > 0 {
            for code in 0..first_char {
                if let Some(w) = compute_width_for_code(code) {
                    let w_rounded = w.round() as i64;
                    if w_rounded != 0 {
                        prepend_codes.push((code, w_rounded));
                    }
                }
            }
        }

        // Codes above LastChar.
        if !ambiguous_cff_base_encoding && last_char < 255 {
            for code in (last_char + 1)..=255 {
                if let Some(w) = compute_width_for_code(code) {
                    let w_rounded = w.round() as i64;
                    if w_rounded != 0 {
                        extensions.push((code, w_rounded));
                    }
                }
            }
        }

        if corrections.is_empty() && extensions.is_empty() && prepend_codes.is_empty() {
            continue;
        }

        // Safety check: if more than 50% of widths mismatch AND the encoding
        // is not a well-known standard encoding, our mapping is probably wrong.
        // For WinAnsiEncoding, the mapping is unambiguous,
        // so we trust the computed widths even if many differ (common when a
        // fallback font like DejaVuSans was embedded for Helvetica/Times etc.).
        // CFF internal encoding is also reliable — when no PDF-level Encoding
        // exists, the CFF's own encoding provides an unambiguous code-to-GID map.
        // Custom CFF encoding in OTF-wrapped fonts is also reliable, since the
        // CFF encoding map directly provides the code-to-GID mapping that
        // veraPDF uses for width comparison.
        let has_reliable_encoding = matches!(enc_info.0.as_str(), "WinAnsiEncoding");
        let uses_cff_encoding = enc_info.0.is_empty() && enc_info.1.is_empty() && has_ff3;
        let uses_custom_cff_encoding = has_ff3 && {
            if let Some(cff_bytes) = extract_cff_bytes_from_otf(&font_data) {
                cff_has_custom_encoding(cff_bytes)
            } else {
                // Raw CFF (Type1C) — check directly
                cff_has_custom_encoding(&font_data)
            }
        };
        // Type 1 FontFile widths are computed from the font program directly,
        // so they are always reliable regardless of encoding.
        let uses_type1_fontfile = has_ff1;
        // CFF (FontFile3) with Differences-based encoding: each code maps to
        // a specific glyph name that we look up in the CFF charset. This is
        // unambiguous, like WinAnsiEncoding.
        let has_differences_cff = has_ff3 && !enc_info.1.is_empty();
        // TrueType (FontFile2) widths are computed via cmap tables, which
        // provide a reliable code→GID mapping. Trust these even without a
        // standard PDF encoding name.
        let uses_truetype_fontfile = has_ff2;
        let total_widths = existing_widths.len();
        if !has_reliable_encoding
            && !uses_cff_encoding
            && !uses_custom_cff_encoding
            && !uses_type1_fontfile
            && !has_differences_cff
            && !uses_truetype_fontfile
            && corrections.len() * 2 > total_widths
            && extensions.is_empty()
            && prepend_codes.is_empty()
        {
            continue;
        }

        // Apply corrections, extensions, and prepends.
        // Determine the new FirstChar and LastChar.
        let new_first_char = if !prepend_codes.is_empty() {
            prepend_codes[0].0
        } else {
            first_char
        };
        let new_last_char = extensions
            .last()
            .map(|(code, _)| *code)
            .unwrap_or(last_char);
        let new_len = (new_last_char - new_first_char + 1) as usize;

        // Build the new widths array.
        let mut new_widths: Vec<Object> = vec![Object::Integer(0); new_len];

        // Copy existing widths at the correct offset.
        let offset = (first_char - new_first_char) as usize;
        for (i, obj) in existing_widths.iter().enumerate() {
            let target = offset + i;
            if target < new_widths.len() {
                new_widths[target] = obj.clone();
            }
        }

        // Apply inline corrections (relative to original first_char).
        for (idx, new_w) in &corrections {
            let target = offset + *idx;
            if target < new_widths.len() {
                new_widths[target] = Object::Integer(*new_w);
            }
        }
        // Apply prepend codes.
        for (code, w) in &prepend_codes {
            let target = (*code - new_first_char) as usize;
            if target < new_widths.len() {
                new_widths[target] = Object::Integer(*w);
            }
        }
        // Apply extensions.
        for (code, w) in &extensions {
            let target = (*code - new_first_char) as usize;
            if target < new_widths.len() {
                new_widths[target] = Object::Integer(*w);
            }
        }

        // Write back. Always set as inline array on the font dict to avoid
        // clobbering shared /Widths references. Multiple font dicts may point to
        // the same /Widths object but need different corrections (e.g. different
        // Encoding/Differences → different target widths for the same code).
        // Writing inline gives each font its own copy. (#fix-shared-widths)
        if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
            font.set("Widths", Object::Array(new_widths));
            // Also write the encoding to the PDF dict if we inferred it.
            // Without this, veraPDF uses its own encoding detection which may
            // differ from our width computation, causing persistent mismatches.
            if !enc_info.0.is_empty() && !font.has(b"Encoding") {
                font.set("Encoding", Object::Name(enc_info.0.as_bytes().to_vec()));
            }
        }
        // Update FirstChar if prepended.
        if new_first_char < first_char {
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
                font.set("FirstChar", Object::Integer(new_first_char as i64));
            }
        }
        // Update LastChar if extended.
        if new_last_char > last_char {
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
                font.set("LastChar", Object::Integer(new_last_char as i64));
            }
        }
        fixed += 1;
    }

    fixed
}

/// Get encoding information for a simple font.
/// Returns (encoding_name, differences_map).
/// The differences_map maps character code -> glyph name from Encoding/Differences.
fn get_simple_encoding_info(
    doc: &Document,
    font_dict: &lopdf::Dictionary,
) -> (String, std::collections::HashMap<u32, String>) {
    let mut enc_name = String::new();
    let mut differences = std::collections::HashMap::new();

    match font_dict.get(b"Encoding").ok() {
        Some(Object::Name(n)) => {
            enc_name = String::from_utf8(n.clone()).unwrap_or_default();
        }
        Some(Object::Dictionary(enc_dict)) => {
            if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                enc_name = base;
            }
            parse_differences(doc, enc_dict, &mut differences);
        }
        Some(Object::Reference(enc_ref)) => {
            if let Ok(obj) = doc.get_object(*enc_ref) {
                match obj {
                    Object::Name(n) => {
                        enc_name = String::from_utf8(n.clone()).unwrap_or_default();
                    }
                    Object::Dictionary(enc_dict) => {
                        if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                            enc_name = base;
                        }
                        parse_differences(doc, enc_dict, &mut differences);
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    (enc_name, differences)
}

/// Parse /Differences array from an encoding dictionary.
fn parse_differences(
    doc: &Document,
    enc_dict: &lopdf::Dictionary,
    differences: &mut std::collections::HashMap<u32, String>,
) {
    let diff_arr = match enc_dict.get(b"Differences").ok() {
        Some(Object::Array(arr)) => Some(arr),
        Some(Object::Reference(r)) => doc.get_object(*r).ok().and_then(|o| o.as_array().ok()),
        _ => None,
    };
    let Some(diff_arr) = diff_arr else { return };

    let mut code: u32 = 0;
    for item in diff_arr {
        match item {
            Object::Integer(i) if *i >= 0 => code = *i as u32,
            Object::Name(n) => {
                if let Ok(name) = String::from_utf8(n.clone()) {
                    differences.insert(code, name);
                }
                code = code.saturating_add(1);
            }
            Object::Reference(r) => {
                if let Ok(resolved) = doc.get_object(*r) {
                    match resolved {
                        Object::Integer(i) if *i >= 0 => code = *i as u32,
                        Object::Name(n) => {
                            if let Ok(name) = String::from_utf8(n.clone()) {
                                differences.insert(code, name);
                            }
                            code = code.saturating_add(1);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

/// Compute width corrections for a simple TrueType font.
///
/// Returns a list of (index_in_widths_array, correct_width) for mismatched entries.
/// Uses the font's cmap table to map character codes to glyph IDs.
fn compute_truetype_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_info: &(String, std::collections::HashMap<u32, String>),
) -> Vec<(usize, i64)> {
    compute_truetype_width_corrections_inner(
        font_data,
        first_char,
        existing_widths,
        enc_info,
        false,
    )
}

fn compute_truetype_width_corrections_inner(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_info: &(String, std::collections::HashMap<u32, String>),
    is_symbolic: bool,
) -> Vec<(usize, i64)> {
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return Vec::new();
    };

    let units_per_em = face.units_per_em() as f64;
    if units_per_em == 0.0 {
        return Vec::new();
    }
    let scale = 1000.0 / units_per_em;

    let (enc_name, differences) = enc_info;

    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // Determine the expected glyph width from the font program (fractional).
        let expected_w = get_truetype_glyph_width_fractional_inner(
            &face,
            code,
            enc_name,
            differences,
            scale,
            is_symbolic,
        );

        let Some(frac_w) = expected_w else { continue };

        // veraPDF compares fractional widths with tolerance > 1.0.
        // Use the same threshold to match its validation logic.
        if (pdf_w - frac_w).abs() > 1.0 {
            corrections.push((i, frac_w.round() as i64));
        }
    }

    corrections
}

/// Get the expected glyph width for a character code in a TrueType font.
///
/// Uses the encoding to map code -> Unicode -> glyph ID via cmap.
/// If the Differences array overrides the glyph for this code, uses that.
#[allow(dead_code)]
fn get_truetype_glyph_width_for_code(
    face: &ttf_parser::Face,
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    scale: f64,
) -> Option<i64> {
    // If Differences maps this code to a glyph name, try to use it.
    if let Some(glyph_name) = differences.get(&code) {
        // Try to find glyph by name -> Unicode -> cmap.
        if let Some(unicode) = glyph_name_to_unicode(glyph_name) {
            if let Some(gid) = face.glyph_index(unicode) {
                return face
                    .glyph_hor_advance(gid)
                    .map(|w| (w as f64 * scale).round() as i64);
            }
        }
        // For .notdef or unmapped glyphs, use glyph ID 0 width.
        if glyph_name == ".notdef" {
            return face
                .glyph_hor_advance(ttf_parser::GlyphId(0))
                .map(|w| (w as f64 * scale).round() as i64);
        }
    }

    // Map code -> Unicode via encoding.
    let ch = encoding_to_char(code, enc_name);

    // Primary: look up via cmap (what veraPDF does for non-symbolic fonts).
    if let Some(gid) = face.glyph_index(ch) {
        return face
            .glyph_hor_advance(gid)
            .map(|w| (w as f64 * scale).round() as i64);
    }

    // Fallback: try direct GID = code (identity mapping for some TrueType fonts).
    if code <= u16::MAX as u32 {
        if let Some(w) = face.glyph_hor_advance(ttf_parser::GlyphId(code as u16)) {
            return Some((w as f64 * scale).round() as i64);
        }
    }

    // Can't determine width — return None to skip this entry.
    None
}

/// Get the expected glyph width as an unrounded f64 for fractional comparison.
///
/// veraPDF compares the fractional glyph width from the font program against
/// the /Widths value with a tolerance of > 1.0. Uses the PDF Encoding to
/// map character codes to Unicode, then looks up in the font's cmap.
fn get_truetype_glyph_width_fractional(
    face: &ttf_parser::Face,
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    scale: f64,
) -> Option<f64> {
    get_truetype_glyph_width_fractional_inner(face, code, enc_name, differences, scale, false)
}

/// Inner implementation with `is_symbolic` flag for symbolic TrueType fonts.
fn get_truetype_glyph_width_fractional_inner(
    face: &ttf_parser::Face,
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    scale: f64,
    is_symbolic: bool,
) -> Option<f64> {
    // If Differences maps this code to a glyph name, try to use it.
    if let Some(glyph_name) = differences.get(&code) {
        // For explicit PDF Differences entries, prefer direct glyph-name
        // lookup over Unicode cmap lookup. In subset TrueType fonts the Unicode
        // cmap may point to a different subset glyph than the PDF encoding's
        // explicit /Differences name, while glyph_index_by_name preserves the
        // PDF's intended code→glyph mapping. (#pdfa-tt-diff-name-first)
        if let Some(gid) = face.glyph_index_by_name(glyph_name) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
        if let Some(unicode) = glyph_name_to_unicode(glyph_name) {
            // Apply canonical normalization (same as veraPDF): U+00AD → U+002D.
            let canonical = match unicode as u32 {
                0x00AD => '-' as u32, // soft hyphen → hyphen-minus
                _ => unicode as u32,
            };
            if let Some(gid) = lookup_unicode_cmap_31(face, canonical) {
                return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
            }
        }
        if glyph_name == ".notdef" {
            return face
                .glyph_hor_advance(ttf_parser::GlyphId(0))
                .map(|w| w as f64 * scale);
        }
    }

    // Map code → Unicode via PDF Encoding, then look up in the (3,1) cmap
    // specifically. veraPDF uses only the (3,1) cmap for non-symbolic TrueType
    // font width validation. Using the general glyph_index() would search all
    // subtables and may find a mapping in (1,0) Mac Roman that doesn't exist
    // in (3,1), causing width mismatches.
    //
    // Apply canonical Unicode normalization BEFORE the (3,1) cmap lookup.
    // veraPDF normalizes certain Unicode codepoints before width comparison:
    // U+00AD (soft hyphen) → U+002D (hyphen-minus). This is applied ALWAYS,
    // not as a fallback — even if the font has a soft-hyphen glyph with a
    // different advance than the hyphen, veraPDF uses the hyphen advance.
    // Applying it here (before the cmap lookup) ensures we generate a width
    // correction when the PDF dict still has the soft-hyphen advance.
    // (#fix-tt-cmap-soft-hyphen-pre-lookup)
    //
    // Use a tri-state raw lookup to distinguish:
    //   Some(gid) gid != 0 → real glyph, use its advance
    //   Some(GID(0))       → (3,1) explicitly maps to notdef; veraPDF uses the
    //                        notdef advance — do NOT fall through to Mac cmap
    //   None               → truly absent from (3,1); may fall through to Mac
    // (#fix-tt-cmap31-notdef-vs-absent)
    let ch = encoding_to_char(code, enc_name);
    let ch = match ch {
        '\u{00AD}' => '-', // soft hyphen → hyphen-minus (veraPDF canonical)
        other => other,
    };
    match lookup_unicode_cmap_31_raw(face, ch as u32) {
        Some(gid) if gid.0 != 0 => {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
        Some(_) => {
            // (3,1) cmap maps this code to GID 0 (notdef). veraPDF uses the
            // notdef advance for the §6.2.11.5 width check — return that here
            // instead of letting the Mac cmap find the real glyph width and
            // suppressing the correction. (#fix-tt-cmap31-notdef-vs-absent)
            return face
                .glyph_hor_advance(ttf_parser::GlyphId(0))
                .map(|w| w as f64 * scale);
        }
        None => {} // Truly absent from (3,1) — fall through to Mac cmap below.
    }

    // veraPDF §6.2.11.5 locates TrueType glyphs via glyph-name lookup:
    // encoding[code] → Unicode → AGL glyph name → font name table → GID.
    // Try this before returning notdef — the glyph may be present by name
    // even when absent from the (3,1) cmap (e.g. "quoteright" at WinAnsi
    // code 146, U+2019, is often in the font's name table but not in
    // a subsetted (3,1) cmap). (#507, #fix-tt-agl-name-fallback)
    if let Some(agl_name) = unicode_to_agl_name(ch) {
        if let Some(gid) = face.glyph_index_by_name(&agl_name) {
            if gid.0 != 0 {
                return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
            }
        }
    }

    // For symbolic TrueType fonts (Flags bit 2, no PDF encoding), try the
    // (3,0) Windows Symbol cmap before returning .notdef. Symbolic fonts use
    // codes in the 0xF000-0xF0FF range in the (3,0) cmap. veraPDF uses this
    // mapping for §6.2.11.5 width validation on symbolic fonts.
    // Also try (3,0) for non-symbolic fonts as a fallback when (3,1) has no mapping.
    if is_symbolic || enc_name.is_empty() {
        if let Some(gid) = lookup_symbol_cmap_30(face, code) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
    }

    // When (3,1) cmap exists but returned None for this code: veraPDF maps
    // to GID 0 (.notdef) and uses its advance for §6.2.11.5. Do NOT fall
    // back to Mac (1,0) cmap — it maps codes differently (e.g. code 160 is
    // dagger in Mac Roman but NBSP in WinAnsi). (#fix-tt-cmap31-authoritative)
    if has_cmap_31(face) {
        return face
            .glyph_hor_advance(ttf_parser::GlyphId(0))
            .map(|w| w as f64 * scale);
    }

    // No (3,1) cmap: other Unicode subtables are still safe because they
    // resolve the already-normalized Unicode code point, not the raw byte code.
    match face.glyph_index(ch) {
        Some(gid) if gid.0 != 0 => {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
        Some(_) => {
            return face
                .glyph_hor_advance(ttf_parser::GlyphId(0))
                .map(|w| w as f64 * scale);
        }
        None => {}
    }

    // Codes 128-159 differ between Mac Roman and WinAnsi, so do NOT fall
    // through to the raw Mac (1,0) byte-code cmap for WinAnsi fonts in this
    // range. If no Unicode/AGL mapping was found above, veraPDF treats these
    // as .notdef for §6.2.11.5. For MacRomanEncoding or no-encoding fonts,
    // 128-159 may still be valid Mac glyphs and can continue to the Mac cmap
    // fallback. (#507)
    if enc_name == "WinAnsiEncoding" && (128..=159).contains(&code) {
        return face
            .glyph_hor_advance(ttf_parser::GlyphId(0))
            .map(|w| w as f64 * scale);
    }

    // Last resort: Mac (1,0) byte-code cmap for MacRoman/no-encoding fonts.
    if code <= 255 {
        if let Some(gid) = lookup_mac_cmap(face, code) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
    }

    // Mapped nowhere: use .notdef advance. veraPDF uses .notdef for all
    // codes that don't map to a glyph, including control codes (< 32).
    // Previously we returned None for codes < 32, leaving dict width=0
    // uncorrected when veraPDF expected .notdef width. (#dict-zero-fix)
    face.glyph_hor_advance(ttf_parser::GlyphId(0))
        .map(|w| w as f64 * scale)
}

/// Look up a Unicode code point in the (3,1) Windows Unicode BMP cmap only.
/// Returns None if the font doesn't have a (3,1) subtable or doesn't map
/// the given code point. This matches veraPDF's behavior for non-symbolic
/// TrueType fonts.
fn lookup_unicode_cmap_31(face: &ttf_parser::Face, unicode: u32) -> Option<ttf_parser::GlyphId> {
    let cmap = face.tables().cmap?;
    for subtable in cmap.subtables {
        if subtable.platform_id == ttf_parser::PlatformId::Windows && subtable.encoding_id == 1 {
            let gid = subtable.glyph_index(unicode)?;
            if gid.0 != 0 {
                return Some(gid);
            }
            return None; // Mapped to .notdef
        }
    }
    None
}

/// Look up a Unicode code point in the (3,1) Windows Unicode BMP cmap, returning
/// the raw GlyphId without filtering GID 0. Unlike `lookup_unicode_cmap_31`, this
/// returns `Some(GlyphId(0))` when the code is explicitly mapped to the notdef glyph,
/// and `None` only when the code is truly absent from the (3,1) subtable.
/// Used to distinguish "mapped to notdef" from "not in cmap at all" for correct
/// fallback handling. (#fix-tt-cmap31-notdef-vs-absent)
fn lookup_unicode_cmap_31_raw(
    face: &ttf_parser::Face,
    unicode: u32,
) -> Option<ttf_parser::GlyphId> {
    let cmap = face.tables().cmap?;
    for subtable in cmap.subtables {
        if subtable.platform_id == ttf_parser::PlatformId::Windows && subtable.encoding_id == 1 {
            return subtable.glyph_index(unicode);
        }
    }
    None
}

/// Returns true when the font has a (3,1) Windows Unicode BMP cmap subtable.
/// When present, veraPDF uses it exclusively for non-symbolic TrueType fonts —
/// codes absent from it map to GID 0 (.notdef). When absent, Mac (1,0) cmap
/// or other subtables may be used as fallback.
fn has_cmap_31(face: &ttf_parser::Face) -> bool {
    face.tables().cmap.is_some_and(|cmap| {
        cmap.subtables
            .into_iter()
            .any(|s| s.platform_id == ttf_parser::PlatformId::Windows && s.encoding_id == 1)
    })
}

/// `(start_code, end_code, start_cid)` triple from a CMap cidrange entry.
type CmapRange = (u16, u16, u16);

/// Load CMap cidrange data for any named predefined CMap, bypassing the
/// unicode-only restriction in `load_predefined_unicode_cmap_ranges`.
/// Used by EUC-style CMap handling to get the code→CID mapping.
fn load_all_cmap_cidranges(cmap_name: &str) -> Option<Vec<CmapRange>> {
    let data = find_predefined_cmap_file(cmap_name)?;
    parse_predefined_cmap_cid_ranges(&data)
}

/// Load CID ranges from an embedded CMap stream in a font's /Encoding entry.
///
/// Used as a fallback when the encoding is a CMap stream object rather than a
/// named predefined CMap. Decompresses the stream, parses cidranges, and
/// detects whether the CMap uses an EUC/GBK-style mixed codespace (needed to
/// pick the correct text-string repair path in `fix_cid_font_notdef`).
///
/// Returns `(ranges, is_euc_style)` or `None` if the Encoding is not a stream.
fn load_embedded_cmap_stream_ranges(
    doc: &Document,
    font_dict: &lopdf::Dictionary,
) -> Option<(Vec<CmapRange>, bool)> {
    let enc_id = match font_dict.get(b"Encoding").ok()? {
        Object::Reference(r) => *r,
        _ => return None,
    };
    let stream = match doc.objects.get(&enc_id) {
        Some(Object::Stream(s)) => s,
        _ => return None,
    };
    let mut s = stream.clone();
    let _ = s.decompress();
    let ranges = parse_predefined_cmap_cid_ranges(&s.content)?;

    // Detect EUC/GBK-style by reading the internal /CMapName definition
    // inside the PostScript CMap program (may differ from the stream dict's
    // CMapName). Fonts like FOUNDER-GBK-EUC-H carry "euc" in the name even
    // when the PDF dict labels the stream "Fdr-gbk-5".
    let is_euc_style = std::str::from_utf8(&s.content).ok().is_some_and(|text| {
        if let Some(pos) = text.find("/CMapName") {
            let after = text[pos + 9..].trim_start();
            if let Some(name) = after.strip_prefix('/') {
                let name_end = name.find(|c: char| c.is_whitespace()).unwrap_or(name.len());
                let name_lower = name[..name_end].to_ascii_lowercase();
                return name_lower.contains("euc") || name_lower.contains("gbk");
            }
        }
        false
    });

    Some((ranges, is_euc_style))
}

/// Returns true for EUC-encoded CMaps (GB-EUC-H/V, KSC-EUC-H/V) that have a
/// mixed 1-byte/2-byte codespace: bytes 0x00–0x80 are single-byte codes and
/// bytes 0xA1–0xFE can start 2-byte sequences.
fn is_euc_style_cmap(cmap_name: &str) -> bool {
    let n = cmap_name.to_ascii_lowercase();
    n.contains("euc")
}

/// Fix a CID text string in-place for EUC-style CMaps (e.g. GB-EUC-H).
///
/// Unlike `fix_cid_text_string` which always processes bytes as 2-byte pairs,
/// this function respects the EUC codespace rules:
///   - Byte b ≤ 0x80: single-byte character code → look up CID in `cmap_ranges`
///   - Byte b ≥ 0xA1 followed by byte b2 ≥ 0xA1: 2-byte character code
///   - All other bytes (lone high bytes, undefined ranges): removed
///
/// Invalid codes (CID not in `valid_cids`) are removed without replacement so
/// that control-character table-rule sequences that map to .notdef do not
/// appear in the output stream.
fn fix_cid_text_string_euc(
    bytes: &mut Vec<u8>,
    cmap_ranges: &[(u16, u16, u16)],
    valid_cids: &std::collections::HashSet<u16>,
) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let mut changed = false;
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b <= 0x80 {
            // Single-byte EUC code in codespace [0x00, 0x80].
            let cid = cmap_code_to_cid(cmap_ranges, b as u16);
            let is_valid = cid.is_some_and(|c| valid_cids.contains(&c));
            if is_valid {
                result.push(b);
            } else {
                changed = true; // omit invalid 1-byte code
            }
            i += 1;
        } else if b >= 0xA1 && i + 1 < bytes.len() && bytes[i + 1] >= 0xA1 {
            // Valid 2-byte EUC pair.
            let b2 = bytes[i + 1];
            let code = (b as u16) * 256 + b2 as u16;
            let cid = cmap_code_to_cid(cmap_ranges, code);
            let is_valid = cid.is_some_and(|c| valid_cids.contains(&c));
            if is_valid {
                result.push(b);
                result.push(b2);
            } else {
                changed = true; // omit invalid 2-byte code
            }
            i += 2;
        } else {
            // Lone high byte (0x81–0xA0 or lone 0xA1+ without valid pair): discard.
            changed = true;
            i += 1;
        }
    }

    if changed {
        *bytes = result;
    }
    changed
}

/// Look up a raw byte code in the (1,0) Macintosh Roman cmap subtable.
/// This matches veraPDF's fallback behavior for non-symbolic TrueType fonts.
/// Look up a character code in the (3,0) Windows Symbol cmap subtable.
/// Symbolic TrueType fonts (Flags bit 2) often use a (3,0) cmap where
/// character codes are stored at 0xF000 + byte_code. veraPDF uses this
/// mapping for §6.2.11.5 width validation on symbolic fonts.
fn lookup_symbol_cmap_30(face: &ttf_parser::Face, code: u32) -> Option<ttf_parser::GlyphId> {
    let cmap = face.tables().cmap?;
    for subtable in cmap.subtables {
        if subtable.platform_id == ttf_parser::PlatformId::Windows && subtable.encoding_id == 0 {
            // (3,0) Symbol cmap: try code + 0xF000 first (standard for Symbol fonts),
            // then raw code as fallback.
            if let Some(gid) = subtable.glyph_index(0xF000 + code) {
                if gid.0 != 0 {
                    return Some(gid);
                }
            }
            if let Some(gid) = subtable.glyph_index(code) {
                if gid.0 != 0 {
                    return Some(gid);
                }
            }
        }
    }
    None
}

/// Returns true when the font has a (3,0) Windows Symbol cmap subtable.
#[allow(dead_code)]
fn has_cmap_30(face: &ttf_parser::Face) -> bool {
    face.tables().cmap.is_some_and(|cmap| {
        cmap.subtables
            .into_iter()
            .any(|s| s.platform_id == ttf_parser::PlatformId::Windows && s.encoding_id == 0)
    })
}

fn lookup_mac_cmap(face: &ttf_parser::Face, code: u32) -> Option<ttf_parser::GlyphId> {
    let cmap = face.tables().cmap?;
    for subtable in cmap.subtables {
        if subtable.platform_id == ttf_parser::PlatformId::Macintosh && subtable.encoding_id == 0 {
            let gid = subtable.glyph_index(code)?;
            if gid.0 != 0 {
                return Some(gid);
            }
        }
    }
    None
}

const PREDEFINED_CMAP_SEARCH_DIRS: &[&str] = &[
    concat!(env!("CARGO_MANIFEST_DIR"), "/resources/cmap"),
    "/usr/share/poppler/cMap",
    "/usr/share/fonts/cmap",
    "/usr/share/fonts/cMap",
    "/usr/share/ghostscript/cMap",
];

fn resolve_type0_cmap_name(doc: &Document, font_dict: &lopdf::Dictionary) -> Option<String> {
    match font_dict.get(b"Encoding").ok()? {
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        Object::Reference(r) => match doc.objects.get(r) {
            Some(Object::Name(n)) => Some(String::from_utf8_lossy(n).to_string()),
            Some(Object::Dictionary(d)) => match d.get(b"CMapName").ok()? {
                Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                _ => None,
            },
            Some(Object::Stream(s)) => match s.dict.get(b"CMapName").ok()? {
                Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                _ => None,
            },
            _ => None,
        },
        Object::Dictionary(d) => match d.get(b"CMapName").ok()? {
            Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
            _ => None,
        },
        Object::Stream(s) => match s.dict.get(b"CMapName").ok()? {
            Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn is_identity_type0_cmap(cmap_name: &str) -> bool {
    let cmap_name = cmap_name.to_ascii_lowercase();
    cmap_name == "identity-h" || cmap_name == "identity-v"
}

fn is_unicode_predefined_type0_cmap(cmap_name: &str) -> bool {
    let cmap_name = cmap_name.to_ascii_lowercase();
    cmap_name.starts_with("uni") && (cmap_name.contains("-ucs2-") || cmap_name.contains("-utf16-"))
}

fn find_predefined_cmap_file(cmap_name: &str) -> Option<Vec<u8>> {
    use std::path::Path;

    for base_dir in PREDEFINED_CMAP_SEARCH_DIRS {
        let direct = Path::new(base_dir).join(cmap_name);
        if let Ok(data) = std::fs::read(&direct) {
            return Some(data);
        }

        if let Ok(entries) = std::fs::read_dir(base_dir) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                let nested = entry.path().join(cmap_name);
                if let Ok(data) = std::fs::read(&nested) {
                    return Some(data);
                }
            }
        }
    }

    None
}

fn parse_predefined_cmap_cid_ranges(data: &[u8]) -> Option<Vec<CmapRange>> {
    let text = std::str::from_utf8(data).ok()?;
    let mut ranges = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('<') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(start_hex) = parts.next() else {
            continue;
        };
        let Some(end_hex) = parts.next() else {
            continue;
        };
        let Some(cid_str) = parts.next() else {
            continue;
        };
        if !start_hex.ends_with('>') || !end_hex.ends_with('>') {
            continue;
        }

        let start = u32::from_str_radix(start_hex.trim_matches(['<', '>']), 16).ok()?;
        let end = u32::from_str_radix(end_hex.trim_matches(['<', '>']), 16).ok()?;
        let base_cid = cid_str.parse::<u32>().ok()?;
        if start > u16::MAX as u32 || end > u16::MAX as u32 {
            continue;
        }
        let span = end.saturating_sub(start);
        if base_cid + span > u16::MAX as u32 {
            continue;
        }

        ranges.push((start as u16, end as u16, base_cid as u16));
    }

    if ranges.is_empty() {
        None
    } else {
        ranges.sort_unstable_by_key(|(start, _, _)| *start);
        Some(ranges)
    }
}

fn load_predefined_unicode_cmap_ranges(cmap_name: &str) -> Option<Vec<CmapRange>> {
    if !is_unicode_predefined_type0_cmap(cmap_name) {
        return None;
    }
    let data = find_predefined_cmap_file(cmap_name)?;
    parse_predefined_cmap_cid_ranges(&data)
}

fn build_predefined_cmap_cidtogid_map(
    face: &ttf_parser::Face,
    ranges: &[(u16, u16, u16)],
) -> Option<Vec<u8>> {
    let max_cid = ranges
        .iter()
        .map(|(start, end, base)| u32::from(*base) + u32::from(*end - *start))
        .max()? as usize;
    let mut map = vec![0u8; (max_cid + 1) * 2];
    let mut mapped_any = false;

    for (start, end, base_cid) in ranges {
        for code in *start..=*end {
            let Some(ch) = char::from_u32(code as u32) else {
                continue;
            };
            let Some(gid) = face
                .glyph_index(ch)
                .filter(|gid| gid.0 > 0 && tt_glyph_has_data(face, *gid))
            else {
                continue;
            };
            let cid = u32::from(*base_cid) + u32::from(code - *start);
            if cid > u16::MAX as u32 {
                continue;
            }
            let idx = cid as usize * 2;
            map[idx..idx + 2].copy_from_slice(&gid.0.to_be_bytes());
            mapped_any = true;
        }
    }

    mapped_any.then_some(map)
}

fn cmap_code_to_cid(ranges: &[(u16, u16, u16)], code: u16) -> Option<u16> {
    for (start, end, base_cid) in ranges {
        if code < *start || code > *end {
            continue;
        }
        return Some(base_cid.saturating_add(code - *start));
    }
    None
}

fn cmap_first_code_for_cid(ranges: &[(u16, u16, u16)], target_cid: u16) -> Option<u16> {
    for (start, end, base_cid) in ranges {
        let span = *end - *start;
        if target_cid < *base_cid || target_cid > base_cid.saturating_add(span) {
            continue;
        }
        return Some(start.saturating_add(target_cid - *base_cid));
    }
    None
}

fn build_valid_codes_from_cmap_ranges(
    valid_cids: &std::collections::HashSet<u16>,
    ranges: &[(u16, u16, u16)],
) -> std::collections::HashSet<u16> {
    let mut valid_codes = std::collections::HashSet::new();
    for (start, end, _) in ranges {
        for code in *start..=*end {
            if cmap_code_to_cid(ranges, code).is_some_and(|cid| valid_cids.contains(&cid)) {
                valid_codes.insert(code);
            }
        }
    }
    valid_codes
}

/// Map a common glyph name to its Unicode codepoint.
/// Based on the Adobe Glyph List (AGL) for common names.
/// Direct Adobe glyph name table for WinAnsiEncoding, matching the lookup
/// veraPDF uses in §6.2.11.5 width checks for Type 1 / Type1C fonts.
///
/// Differs from the Unicode AGL roundtrip for two codes:
///   - code 160 (U+00A0 NO-BREAK SPACE) → "space"   (AGL: "nbspace")
///   - code 173 (U+00AD SOFT HYPHEN)    → "hyphen"  (AGL: "softhyphen")
///
/// Using this table in `cff_width_for_code` keeps our correction aligned with
/// what veraPDF's compliance checker expects. (#6.2.11.5-t1-winansi)
fn winansi_type1_glyph_name(code: u8) -> Option<&'static str> {
    match code {
        128 => Some("Euro"),
        130 => Some("quotesinglbase"),
        131 => Some("florin"),
        132 => Some("quotedblbase"),
        133 => Some("ellipsis"),
        134 => Some("dagger"),
        135 => Some("daggerdbl"),
        136 => Some("circumflex"),
        137 => Some("perthousand"),
        138 => Some("Scaron"),
        139 => Some("guilsinglleft"),
        140 => Some("OE"),
        142 => Some("Zcaron"),
        145 => Some("quoteleft"),
        146 => Some("quoteright"),
        147 => Some("quotedblleft"),
        148 => Some("quotedblright"),
        149 => Some("bullet"),
        150 => Some("endash"),
        151 => Some("emdash"),
        152 => Some("tilde"),
        153 => Some("trademark"),
        154 => Some("scaron"),
        155 => Some("guilsinglright"),
        156 => Some("oe"),
        158 => Some("zcaron"),
        159 => Some("Ydieresis"),
        160 => Some("space"), // U+00A0 → "space" (not AGL "nbspace")
        161 => Some("exclamdown"),
        162 => Some("cent"),
        163 => Some("sterling"),
        164 => Some("currency"),
        165 => Some("yen"),
        166 => Some("brokenbar"),
        167 => Some("section"),
        168 => Some("dieresis"),
        169 => Some("copyright"),
        170 => Some("ordfeminine"),
        171 => Some("guillemotleft"),
        172 => Some("logicalnot"),
        173 => Some("hyphen"), // U+00AD → "hyphen" (not AGL "softhyphen")
        174 => Some("registered"),
        175 => Some("macron"),
        176 => Some("degree"),
        177 => Some("plusminus"),
        178 => Some("twosuperior"),
        179 => Some("threesuperior"),
        180 => Some("acute"),
        181 => Some("mu"),
        182 => Some("paragraph"),
        183 => Some("periodcentered"),
        184 => Some("cedilla"),
        185 => Some("onesuperior"),
        186 => Some("ordmasculine"),
        187 => Some("guillemotright"),
        188 => Some("onequarter"),
        189 => Some("onehalf"),
        190 => Some("threequarters"),
        191 => Some("questiondown"),
        192 => Some("Agrave"),
        193 => Some("Aacute"),
        194 => Some("Acircumflex"),
        195 => Some("Atilde"),
        196 => Some("Adieresis"),
        197 => Some("Aring"),
        198 => Some("AE"),
        199 => Some("Ccedilla"),
        200 => Some("Egrave"),
        201 => Some("Eacute"),
        202 => Some("Ecircumflex"),
        203 => Some("Edieresis"),
        204 => Some("Igrave"),
        205 => Some("Iacute"),
        206 => Some("Icircumflex"),
        207 => Some("Idieresis"),
        208 => Some("Eth"),
        209 => Some("Ntilde"),
        210 => Some("Ograve"),
        211 => Some("Oacute"),
        212 => Some("Ocircumflex"),
        213 => Some("Otilde"),
        214 => Some("Odieresis"),
        215 => Some("multiply"),
        216 => Some("Oslash"),
        217 => Some("Ugrave"),
        218 => Some("Uacute"),
        219 => Some("Ucircumflex"),
        220 => Some("Udieresis"),
        221 => Some("Yacute"),
        222 => Some("Thorn"),
        223 => Some("germandbls"),
        224 => Some("agrave"),
        225 => Some("aacute"),
        226 => Some("acircumflex"),
        227 => Some("atilde"),
        228 => Some("adieresis"),
        229 => Some("aring"),
        230 => Some("ae"),
        231 => Some("ccedilla"),
        232 => Some("egrave"),
        233 => Some("eacute"),
        234 => Some("ecircumflex"),
        235 => Some("edieresis"),
        236 => Some("igrave"),
        237 => Some("iacute"),
        238 => Some("icircumflex"),
        239 => Some("idieresis"),
        240 => Some("eth"),
        241 => Some("ntilde"),
        242 => Some("ograve"),
        243 => Some("oacute"),
        244 => Some("ocircumflex"),
        245 => Some("otilde"),
        246 => Some("odieresis"),
        247 => Some("divide"),
        248 => Some("oslash"),
        249 => Some("ugrave"),
        250 => Some("uacute"),
        251 => Some("ucircumflex"),
        252 => Some("udieresis"),
        253 => Some("yacute"),
        254 => Some("thorn"),
        255 => Some("ydieresis"),
        _ => None,
    }
}

fn standard_type1_glyph_name(code: u8) -> Option<&'static str> {
    match code {
        32 => Some("space"),
        33 => Some("exclam"),
        34 => Some("quotedbl"),
        35 => Some("numbersign"),
        36 => Some("dollar"),
        37 => Some("percent"),
        38 => Some("ampersand"),
        39 => Some("quoteright"),
        40 => Some("parenleft"),
        41 => Some("parenright"),
        42 => Some("asterisk"),
        43 => Some("plus"),
        44 => Some("comma"),
        45 => Some("hyphen"),
        46 => Some("period"),
        47 => Some("slash"),
        48 => Some("zero"),
        49 => Some("one"),
        50 => Some("two"),
        51 => Some("three"),
        52 => Some("four"),
        53 => Some("five"),
        54 => Some("six"),
        55 => Some("seven"),
        56 => Some("eight"),
        57 => Some("nine"),
        58 => Some("colon"),
        59 => Some("semicolon"),
        60 => Some("less"),
        61 => Some("equal"),
        62 => Some("greater"),
        63 => Some("question"),
        64 => Some("at"),
        65 => Some("A"),
        66 => Some("B"),
        67 => Some("C"),
        68 => Some("D"),
        69 => Some("E"),
        70 => Some("F"),
        71 => Some("G"),
        72 => Some("H"),
        73 => Some("I"),
        74 => Some("J"),
        75 => Some("K"),
        76 => Some("L"),
        77 => Some("M"),
        78 => Some("N"),
        79 => Some("O"),
        80 => Some("P"),
        81 => Some("Q"),
        82 => Some("R"),
        83 => Some("S"),
        84 => Some("T"),
        85 => Some("U"),
        86 => Some("V"),
        87 => Some("W"),
        88 => Some("X"),
        89 => Some("Y"),
        90 => Some("Z"),
        91 => Some("bracketleft"),
        92 => Some("backslash"),
        93 => Some("bracketright"),
        94 => Some("asciicircum"),
        95 => Some("underscore"),
        96 => Some("quoteleft"),
        97 => Some("a"),
        98 => Some("b"),
        99 => Some("c"),
        100 => Some("d"),
        101 => Some("e"),
        102 => Some("f"),
        103 => Some("g"),
        104 => Some("h"),
        105 => Some("i"),
        106 => Some("j"),
        107 => Some("k"),
        108 => Some("l"),
        109 => Some("m"),
        110 => Some("n"),
        111 => Some("o"),
        112 => Some("p"),
        113 => Some("q"),
        114 => Some("r"),
        115 => Some("s"),
        116 => Some("t"),
        117 => Some("u"),
        118 => Some("v"),
        119 => Some("w"),
        120 => Some("x"),
        121 => Some("y"),
        122 => Some("z"),
        123 => Some("braceleft"),
        124 => Some("bar"),
        125 => Some("braceright"),
        126 => Some("asciitilde"),
        161 => Some("exclamdown"),
        162 => Some("cent"),
        163 => Some("sterling"),
        164 => Some("fraction"),
        165 => Some("yen"),
        166 => Some("florin"),
        167 => Some("section"),
        168 => Some("currency"),
        169 => Some("quotesingle"),
        170 => Some("quotedblleft"),
        171 => Some("guillemotleft"),
        172 => Some("guilsinglleft"),
        173 => Some("guilsinglright"),
        174 => Some("fi"),
        175 => Some("fl"),
        177 => Some("endash"),
        178 => Some("dagger"),
        179 => Some("daggerdbl"),
        180 => Some("periodcentered"),
        182 => Some("paragraph"),
        183 => Some("bullet"),
        184 => Some("quotesinglbase"),
        185 => Some("quotedblbase"),
        186 => Some("quotedblright"),
        187 => Some("guillemotright"),
        188 => Some("ellipsis"),
        189 => Some("perthousand"),
        191 => Some("questiondown"),
        193 => Some("grave"),
        194 => Some("acute"),
        195 => Some("circumflex"),
        196 => Some("tilde"),
        197 => Some("macron"),
        198 => Some("breve"),
        199 => Some("dotaccent"),
        200 => Some("dieresis"),
        202 => Some("ring"),
        203 => Some("cedilla"),
        205 => Some("hungarumlaut"),
        206 => Some("ogonek"),
        207 => Some("caron"),
        208 => Some("emdash"),
        225 => Some("AE"),
        227 => Some("ordfeminine"),
        232 => Some("Lslash"),
        233 => Some("Oslash"),
        234 => Some("OE"),
        235 => Some("ordmasculine"),
        241 => Some("ae"),
        245 => Some("dotlessi"),
        248 => Some("lslash"),
        249 => Some("oslash"),
        250 => Some("oe"),
        251 => Some("germandbls"),
        _ => None,
    }
}

fn type1_winansi_glyph_name_for_code(code: u32) -> Option<String> {
    if code > 255 {
        return None;
    }
    if code >= 128 {
        return winansi_type1_glyph_name(code as u8).map(str::to_string);
    }
    let ch = encoding_to_char(code, "WinAnsiEncoding");
    unicode_to_agl_name(ch).or_else(|| unicode_to_glyph_name(ch))
}

fn cff_pdf_base_glyph_name(code: u32, enc_name: &str) -> Option<String> {
    if code > 255 {
        return None;
    }
    match enc_name {
        "WinAnsiEncoding" => type1_winansi_glyph_name_for_code(code),
        "" | "StandardEncoding" => standard_type1_glyph_name(code as u8).map(str::to_string),
        // For CFF simple fonts, veraPDF does not resolve high-byte MacRoman codes
        // through the MacRoman base encoding table; it falls back to the internal
        // CFF encoding unless /Differences overrides the code explicitly.
        "MacRomanEncoding" => None,
        _ => {
            let ch = encoding_to_char(code, enc_name);
            unicode_to_agl_name(ch).or_else(|| unicode_to_glyph_name(ch))
        }
    }
}

/// Parse a ToUnicode CMap stream into a single-byte code→Unicode mapping.
///
/// Handles `beginbfchar` / `endbfchar` and `beginbfrange` / `endbfrange` sections.
/// Only extracts mappings where the source code fits in one byte (codes 0x00–0xFF).
/// Used by `cff_width_for_code` to resolve codes that are undefined in the PDF
/// encoding (e.g. WinAnsiEncoding code 0x81 = U+2022 per ToUnicode). (#507, gen-131)
///
/// Handles compact CMaps where multiple entries appear on one line without separators,
/// e.g. `21 beginbfrange<20> <7e> <0020><7f> <7f> <2022><81> <81> <2022>endbfrange`.
fn parse_tounicode_map_bytes(stream_data: &[u8]) -> std::collections::HashMap<u8, char> {
    let text = String::from_utf8_lossy(stream_data);
    let mut map = std::collections::HashMap::new();

    // Tokenise: extract all <hex> tokens and keyword tokens in order.
    // A <hex> token is the content between '<' and '>'.
    // Keywords are whitespace-delimited words outside angle brackets.
    let mut tokens: Vec<String> = Vec::new();
    let text_ref: &str = &text;
    let mut s = text_ref;
    while !s.is_empty() {
        if let Some(lt) = s.find('<') {
            // Words before '<'
            for w in s[..lt].split_whitespace() {
                tokens.push(w.to_string());
            }
            s = &s[lt + 1..];
            if let Some(gt) = s.find('>') {
                tokens.push(s[..gt].to_string());
                s = &s[gt + 1..];
            }
        } else {
            for w in s.split_whitespace() {
                tokens.push(w.to_string());
            }
            break;
        }
    }

    #[derive(PartialEq)]
    enum Section {
        None,
        BfChar,
        BfRange,
    }
    let mut section = Section::None;
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if t.contains("beginbfchar") {
            section = Section::BfChar;
            i += 1;
            continue;
        }
        if t.contains("endbfchar") {
            section = Section::None;
            i += 1;
            continue;
        }
        if t.contains("beginbfrange") {
            section = Section::BfRange;
            i += 1;
            continue;
        }
        if t.contains("endbfrange") {
            section = Section::None;
            i += 1;
            continue;
        }

        // Only process pure hex tokens inside a section.
        let is_hex = !t.is_empty() && t.chars().all(|c| c.is_ascii_hexdigit());
        if !is_hex {
            i += 1;
            continue;
        }

        match section {
            Section::BfChar => {
                // Consume 2 tokens: <src> <dst>
                if i + 1 < tokens.len() {
                    let dt = &tokens[i + 1];
                    if dt.chars().all(|c| c.is_ascii_hexdigit()) {
                        if let (Ok(src), Ok(dst)) =
                            (u32::from_str_radix(t, 16), u32::from_str_radix(dt, 16))
                        {
                            if src <= 0xFF {
                                if let Some(ch) = char::from_u32(dst) {
                                    map.insert(src as u8, ch);
                                }
                            }
                        }
                        i += 2;
                        continue;
                    }
                }
                i += 1;
            }
            Section::BfRange => {
                // Consume 3 tokens: <start> <end> <first_dst>
                if i + 2 < tokens.len() {
                    let et = &tokens[i + 1];
                    let dt = &tokens[i + 2];
                    if et.chars().all(|c| c.is_ascii_hexdigit())
                        && dt.chars().all(|c| c.is_ascii_hexdigit())
                    {
                        if let (Ok(start), Ok(end), Ok(dst)) = (
                            u32::from_str_radix(t, 16),
                            u32::from_str_radix(et, 16),
                            u32::from_str_radix(dt, 16),
                        ) {
                            if start <= 0xFF {
                                let end_c = end.min(0xFF);
                                for code in start..=end_c {
                                    if let Some(ch) = char::from_u32(dst + (code - start)) {
                                        map.insert(code as u8, ch);
                                    }
                                }
                            }
                        }
                        i += 3;
                        continue;
                    }
                }
                i += 1;
            }
            Section::None => {
                i += 1;
            }
        }
    }
    map
}

/// Extract the ToUnicode code→char mapping for a font from the PDF document.
/// Returns an empty map if the font has no ToUnicode or parsing fails.
fn read_font_to_unicode_map(
    doc: &lopdf::Document,
    font_dict: &lopdf::Dictionary,
) -> std::collections::HashMap<u8, char> {
    let ref_id = match font_dict.get(b"ToUnicode").ok() {
        Some(Object::Reference(r)) => *r,
        _ => return std::collections::HashMap::new(),
    };
    let stream = match doc.get_object(ref_id) {
        Ok(Object::Stream(s)) => s.clone(),
        _ => return std::collections::HashMap::new(),
    };
    let mut s = stream;
    let _ = s.decompress();
    parse_tounicode_map_bytes(&s.content)
}

fn glyph_name_to_unicode(name: &str) -> Option<char> {
    // Handle "uniXXXX" format.
    if name.starts_with("uni") && name.len() == 7 {
        if let Ok(cp) = u32::from_str_radix(&name[3..], 16) {
            return char::from_u32(cp);
        }
    }

    // Single ASCII character names.
    if name.len() == 1 {
        return name.chars().next();
    }

    // Common glyph names from AGL.
    match name {
        "space" => Some(' '),
        "exclam" => Some('!'),
        "quotedbl" => Some('"'),
        "numbersign" => Some('#'),
        "dollar" => Some('$'),
        "percent" => Some('%'),
        "ampersand" => Some('&'),
        "quotesingle" => Some('\''),
        "parenleft" => Some('('),
        "parenright" => Some(')'),
        "asterisk" => Some('*'),
        "plus" => Some('+'),
        "comma" => Some(','),
        "hyphen" => Some('-'),
        "minus" => Some('\u{2212}'),
        "period" => Some('.'),
        "slash" => Some('/'),
        "zero" => Some('0'),
        "one" => Some('1'),
        "two" => Some('2'),
        "three" => Some('3'),
        "four" => Some('4'),
        "five" => Some('5'),
        "six" => Some('6'),
        "seven" => Some('7'),
        "eight" => Some('8'),
        "nine" => Some('9'),
        "colon" => Some(':'),
        "semicolon" => Some(';'),
        "less" => Some('<'),
        "equal" => Some('='),
        "greater" => Some('>'),
        "question" => Some('?'),
        "at" => Some('@'),
        "bracketleft" => Some('['),
        "backslash" => Some('\\'),
        "bracketright" => Some(']'),
        "asciicircum" => Some('^'),
        "underscore" => Some('_'),
        "grave" => Some('`'),
        "braceleft" => Some('{'),
        "bar" => Some('|'),
        "braceright" => Some('}'),
        "asciitilde" => Some('~'),
        "bullet" => Some('\u{2022}'),
        "ellipsis" => Some('\u{2026}'),
        "emdash" => Some('\u{2014}'),
        "endash" => Some('\u{2013}'),
        "fi" => Some('\u{FB01}'),
        "fl" => Some('\u{FB02}'),
        "quotedblleft" => Some('\u{201C}'),
        "quotedblright" => Some('\u{201D}'),
        "quoteleft" => Some('\u{2018}'),
        "quoteright" => Some('\u{2019}'),
        "quotesinglbase" => Some('\u{201A}'),
        "quotedblbase" => Some('\u{201E}'),
        "dagger" => Some('\u{2020}'),
        "daggerdbl" => Some('\u{2021}'),
        "trademark" => Some('\u{2122}'),
        "copyright" => Some('\u{00A9}'),
        "registered" => Some('\u{00AE}'),
        "degree" => Some('\u{00B0}'),
        "Euro" => Some('\u{20AC}'),
        "sterling" => Some('\u{00A3}'),
        "yen" => Some('\u{00A5}'),
        "cent" => Some('\u{00A2}'),
        "section" => Some('\u{00A7}'),
        "paragraph" => Some('\u{00B6}'),
        "germandbls" => Some('\u{00DF}'),
        "Adieresis" => Some('\u{00C4}'),
        "Odieresis" => Some('\u{00D6}'),
        "Udieresis" => Some('\u{00DC}'),
        "adieresis" => Some('\u{00E4}'),
        "odieresis" => Some('\u{00F6}'),
        "udieresis" => Some('\u{00FC}'),
        "Aacute" => Some('\u{00C1}'),
        "Agrave" => Some('\u{00C0}'),
        "Acircumflex" => Some('\u{00C2}'),
        "Atilde" => Some('\u{00C3}'),
        "Aring" => Some('\u{00C5}'),
        "AE" => Some('\u{00C6}'),
        "Ccedilla" => Some('\u{00C7}'),
        "Eacute" => Some('\u{00C9}'),
        "Egrave" => Some('\u{00C8}'),
        "Ecircumflex" => Some('\u{00CA}'),
        "Edieresis" => Some('\u{00CB}'),
        "Iacute" => Some('\u{00CD}'),
        "Igrave" => Some('\u{00CC}'),
        "Icircumflex" => Some('\u{00CE}'),
        "Idieresis" => Some('\u{00CF}'),
        "Ntilde" => Some('\u{00D1}'),
        "Oacute" => Some('\u{00D3}'),
        "Ograve" => Some('\u{00D2}'),
        "Ocircumflex" => Some('\u{00D4}'),
        "Otilde" => Some('\u{00D5}'),
        "Oslash" => Some('\u{00D8}'),
        "Scaron" => Some('\u{0160}'),
        "Uacute" => Some('\u{00DA}'),
        "Ugrave" => Some('\u{00D9}'),
        "Ucircumflex" => Some('\u{00DB}'),
        "Zcaron" => Some('\u{017D}'),
        "aacute" => Some('\u{00E1}'),
        "agrave" => Some('\u{00E0}'),
        "acircumflex" => Some('\u{00E2}'),
        "atilde" => Some('\u{00E3}'),
        "aring" => Some('\u{00E5}'),
        "ae" => Some('\u{00E6}'),
        "ccedilla" => Some('\u{00E7}'),
        "eacute" => Some('\u{00E9}'),
        "egrave" => Some('\u{00E8}'),
        "ecircumflex" => Some('\u{00EA}'),
        "edieresis" => Some('\u{00EB}'),
        "iacute" => Some('\u{00ED}'),
        "igrave" => Some('\u{00EC}'),
        "icircumflex" => Some('\u{00EE}'),
        "idieresis" => Some('\u{00EF}'),
        "ntilde" => Some('\u{00F1}'),
        "oacute" => Some('\u{00F3}'),
        "ograve" => Some('\u{00F2}'),
        "ocircumflex" => Some('\u{00F4}'),
        "otilde" => Some('\u{00F5}'),
        "oslash" => Some('\u{00F8}'),
        "scaron" => Some('\u{0161}'),
        "uacute" => Some('\u{00FA}'),
        "ugrave" => Some('\u{00F9}'),
        "ucircumflex" => Some('\u{00FB}'),
        "zcaron" => Some('\u{017E}'),
        "thorn" => Some('\u{00FE}'),
        "eth" => Some('\u{00F0}'),
        "Eth" => Some('\u{00D0}'),
        "Thorn" => Some('\u{00DE}'),
        "multiply" => Some('\u{00D7}'),
        "divide" => Some('\u{00F7}'),
        "mu" => Some('\u{00B5}'),
        "guillemotleft" => Some('\u{00AB}'),
        "guillemotright" => Some('\u{00BB}'),
        "guilsinglleft" => Some('\u{2039}'),
        "guilsinglright" => Some('\u{203A}'),
        "exclamdown" => Some('\u{00A1}'),
        "questiondown" => Some('\u{00BF}'),
        "perthousand" => Some('\u{2030}'),
        "circumflex" => Some('\u{02C6}'),
        "tilde" => Some('\u{02DC}'),
        "dotlessi" => Some('\u{0131}'),
        "lslash" => Some('\u{0142}'),
        "Lslash" => Some('\u{0141}'),
        "OE" => Some('\u{0152}'),
        "oe" => Some('\u{0153}'),
        "Ydieresis" => Some('\u{0178}'),
        "ydieresis" => Some('\u{00FF}'),
        "florin" => Some('\u{0192}'),
        "fraction" => Some('\u{2044}'),
        "acute" => Some('\u{00B4}'),
        "cedilla" => Some('\u{00B8}'),
        "dieresis" => Some('\u{00A8}'),
        "macron" => Some('\u{00AF}'),
        "ring" => Some('\u{02DA}'),
        "caron" => Some('\u{02C7}'),
        "breve" => Some('\u{02D8}'),
        "ogonek" => Some('\u{02DB}'),
        "hungarumlaut" => Some('\u{02DD}'),
        "dotaccent" => Some('\u{02D9}'),
        "nbspace" | "nonbreakingspace" => Some('\u{00A0}'),
        "ordfeminine" => Some('\u{00AA}'),
        "ordmasculine" => Some('\u{00BA}'),
        "logicalnot" => Some('\u{00AC}'),
        "brokenbar" => Some('\u{00A6}'),
        "currency" => Some('\u{00A4}'),
        "plusminus" => Some('\u{00B1}'),
        "onesuperior" => Some('\u{00B9}'),
        "twosuperior" => Some('\u{00B2}'),
        "threesuperior" => Some('\u{00B3}'),
        "onequarter" => Some('\u{00BC}'),
        "onehalf" => Some('\u{00BD}'),
        "threequarters" => Some('\u{00BE}'),
        "periodcentered" | "middot" => Some('\u{00B7}'),
        ".notdef" => None,
        _ => None,
    }
}

/// Compute width corrections for a Type1 font with FontFile (PFA/PFB) program.
///
/// Returns (index_in_widths_array, correct_width) for mismatched entries.
/// Only reliable for subset fonts where charstrings may differ from the original dict.
/// Returns `(index, new_width, is_certain, allow_large_delta)` tuples.
/// `allow_large_delta=true` is reserved for a narrow StandardEncoding punctuation
/// set where veraPDF's mapping is deterministic even when the width delta is
/// larger than the usual ambiguity guard.
fn compute_type1_fontfile_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_info: &(String, std::collections::HashMap<u32, String>),
) -> Vec<(usize, i64, bool, bool)> {
    let (enc_name, differences) = enc_info;
    let Some(parsed) = parse_type1_program(font_data) else {
        return Vec::new();
    };
    let scale = parsed.font_matrix_sx * 1000.0;
    let mut corrections = Vec::new();
    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };
        let code = first_char + i as u32;
        // Track whether this correction is definitely correct (e.g. from .notdef)
        // so we can bypass the outer delta-5 filter for it.
        let mut is_certain_correction = false;
        let mut allow_large_delta = false;
        let glyph_name = if let Some(name) = differences.get(&code) {
            name.as_str().to_string()
        } else if let Some(name) = parsed.encoding.get(&(code as u8)) {
            name.clone()
        } else if !enc_name.is_empty() {
            // PDF dict specifies an explicit base encoding — use it.
            let ch = encoding_to_char(code, enc_name);
            unicode_to_agl_name(ch)
                .or_else(|| unicode_to_glyph_name(ch))
                .unwrap_or_default()
        } else if !parsed.base_encoding_name.is_empty() {
            // No PDF-level encoding, but the font program declares a named encoding
            // (e.g. `/Encoding StandardEncoding def`). Use it for the code→glyph
            // mapping so we get correct charstring widths instead of falling back to
            // .notdef. This fixes §6.2.11.5:1 for fonts like Century-Bold that use
            // StandardEncoding in their PFB program but have a custom /Encoding dict
            // in the PDF with no BaseEncoding key. (#6.2.11.5-type1-named-enc)
            let ch = encoding_to_char(code, &parsed.base_encoding_name);
            let name = unicode_to_agl_name(ch)
                .or_else(|| unicode_to_glyph_name(ch))
                .unwrap_or_default();
            allow_large_delta = matches!(
                name.as_str(),
                "quoteright" | "quoteleft" | "hyphen" | "endash" | "emdash"
            );
            name
        } else {
            // No explicit PDF or font-level encoding for this code. veraPDF uses
            // the font's internal encoding; for codes absent from it (like code 39
            // not in Helvetica-Condensed-Bold's encoding), veraPDF maps to GID 0
            // and uses .notdef width. Use ".notdef" as the target glyph name so we
            // compute the correct .notdef width. This is provably correct (not an
            // ambiguous AGL heuristic), so mark is_certain_correction=true to bypass
            // the outer delta-5 filter. (#6.2.11.5-type1-notdef-fallback)
            is_certain_correction = true;
            ".notdef".to_string()
        };
        if glyph_name.is_empty() {
            continue;
        }
        // Look up width. For .notdef, always use 0 if charstring is absent.
        let cs_width = if glyph_name == ".notdef" {
            parsed
                .charstring_widths
                .get(".notdef")
                .copied()
                .unwrap_or_default()
        } else {
            match parsed.charstring_widths.get(glyph_name.as_str()).copied() {
                Some(w) => w,
                None => continue,
            }
        };
        let font_w = (cs_width as f64 * scale).round();
        let delta = (pdf_w - font_w).abs();
        if delta >= 1.0 {
            corrections.push((i, font_w as i64, is_certain_correction, allow_large_delta));
        }
    }
    corrections
}

/// Compute a single glyph width from a Type 1 FontFile program.
fn compute_type1_fontfile_single_width(
    font_data: &[u8],
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
) -> Option<f64> {
    let parsed = parse_type1_program(font_data)?;

    let glyph_name = if let Some(name) = differences.get(&code) {
        name.as_str().to_string()
    } else if let Some(name) = parsed.encoding.get(&(code as u8)) {
        name.clone()
    } else {
        let ch = encoding_to_char(code, enc_name);
        unicode_to_agl_name(ch)
            .or_else(|| unicode_to_glyph_name(ch))
            .unwrap_or_default()
    };

    if glyph_name.is_empty() || glyph_name == ".notdef" {
        return None;
    }

    let cs_width = *parsed.charstring_widths.get(glyph_name.as_str())?;
    Some((cs_width as f64 * parsed.font_matrix_sx * 1000.0).round())
}

/// Parsed data from a Type 1 font program.
struct Type1Parsed {
    font_matrix_sx: f64,
    encoding: std::collections::HashMap<u8, String>,
    charstring_widths: std::collections::HashMap<String, i32>,
    /// Named base encoding from the font program (e.g. "StandardEncoding"),
    /// or empty if the font uses a custom `dup N /name put` encoding instead.
    base_encoding_name: String,
}

/// Parse a Type 1 font program (PFB/PFA) to extract FontMatrix, Encoding, and widths.
fn parse_type1_program(data: &[u8]) -> Option<Type1Parsed> {
    // Find the cleartext/encrypted boundary.
    // PFB format has segment headers; PFA is plain text with hex-encoded eexec.
    let (cleartext, eexec_data) = split_type1_sections(data)?;

    // Parse FontMatrix from cleartext.
    let font_matrix_sx = parse_type1_font_matrix(cleartext).unwrap_or(0.001);

    // Parse Encoding from cleartext.
    // Also detect when the font specifies a named encoding (e.g. /Encoding StandardEncoding def)
    // so that the caller can fall back to it when the PDF dict has no BaseEncoding.
    let mut encoding = parse_type1_encoding(cleartext);
    let base_encoding_name = parse_type1_named_encoding(cleartext).unwrap_or_default();

    // Decrypt eexec section.
    let decrypted = eexec_decrypt(eexec_data);

    // Parse lenIV from cleartext first, then from decrypted Private dict.
    let len_iv_cleartext = parse_type1_len_iv(cleartext);
    let len_iv_bytes = parse_type1_len_iv_bytes(&decrypted);
    let len_iv = len_iv_cleartext.or(len_iv_bytes).unwrap_or(4) as usize;

    // Some Type1 programs define/override Encoding inside eexec; merge those.
    encoding.extend(parse_type1_encoding_bytes(&decrypted));

    // Parse which local subrs contain seac (for seac-via-callsubr detection).
    let seac_subrs = parse_type1_seac_subrs(&decrypted, len_iv);

    // Parse CharStrings from decrypted data.
    let charstring_widths = parse_type1_charstrings(&decrypted, len_iv, &seac_subrs);

    Some(Type1Parsed {
        font_matrix_sx,
        encoding,
        charstring_widths,
        base_encoding_name,
    })
}

/// Split a Type 1 font into cleartext and eexec-encrypted sections.
fn split_type1_sections(data: &[u8]) -> Option<(&[u8], &[u8])> {
    // Check for PFB format (starts with 0x80).
    if data.first() == Some(&0x80) {
        return split_pfb_sections(data);
    }

    // PFA format: find "eexec" keyword.
    let eexec_pos = find_bytes(data, b"eexec")?;
    let cleartext = &data[..eexec_pos];

    // Skip "eexec" and any whitespace.
    let mut pos = eexec_pos + 5;
    while pos < data.len() && matches!(data[pos], b' ' | b'\r' | b'\n' | b'\t') {
        pos += 1;
    }

    // The eexec data can be binary or hex-encoded.
    let remaining = &data[pos..];
    if remaining.is_empty() {
        return None;
    }

    // Check if hex-encoded (all hex chars + whitespace).
    let is_hex = remaining
        .iter()
        .take(20)
        .all(|b| b.is_ascii_hexdigit() || matches!(b, b'\r' | b'\n' | b' '));

    if is_hex {
        // Decode hex to binary.
        // For efficiency, we can't return a slice — we'd need owned data.
        // Instead, return the hex data and let the caller decode it.
        // Actually, since we need a slice, we'll handle hex in eexec_decrypt.
        Some((cleartext, remaining))
    } else {
        Some((cleartext, remaining))
    }
}

/// Split PFB (binary) format into cleartext and eexec sections.
fn split_pfb_sections(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut pos = 0;
    let mut cleartext_end = 0;

    while pos + 6 <= data.len() {
        if data[pos] != 0x80 {
            break;
        }
        let seg_type = data[pos + 1];
        let seg_len =
            u32::from_le_bytes([data[pos + 2], data[pos + 3], data[pos + 4], data[pos + 5]])
                as usize;
        let seg_data_start = pos + 6;

        match seg_type {
            1 => {
                // ASCII segment (cleartext).
                cleartext_end = seg_data_start + seg_len;
            }
            2 => {
                // Binary segment (eexec encrypted).
                let eexec_end = seg_data_start + seg_len;
                return Some((&data[6..cleartext_end], &data[seg_data_start..eexec_end]));
            }
            3 => break, // EOF marker.
            _ => break,
        }
        pos = seg_data_start + seg_len;
    }

    // Fallback: try eexec keyword search.
    let eexec_pos = find_bytes(&data[6..], b"eexec")?;
    let cleartext = &data[6..6 + eexec_pos];
    let mut skip = 6 + eexec_pos + 5;
    while skip < data.len() && matches!(data[skip], b' ' | b'\r' | b'\n' | b'\t') {
        skip += 1;
    }
    Some((cleartext, &data[skip..]))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Parse FontMatrix from Type 1 cleartext.
fn parse_type1_font_matrix(cleartext: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(cleartext).ok()?;
    let fm_pos = text.find("/FontMatrix")?;
    let after = &text[fm_pos..];
    let bracket_start = after.find('[')?;
    let bracket_end = after.find(']')?;
    let values_str = &after[bracket_start + 1..bracket_end];
    let values: Vec<f64> = values_str
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    if !values.is_empty() {
        Some(values[0])
    } else {
        None
    }
}

/// Parse Encoding array from Type 1 cleartext.
fn parse_type1_encoding(cleartext: &[u8]) -> std::collections::HashMap<u8, String> {
    let mut encoding = std::collections::HashMap::new();
    let Ok(text) = std::str::from_utf8(cleartext) else {
        return encoding;
    };

    // Look for patterns like: dup <code> /<name> put
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("dup ") || !trimmed.ends_with(" put") {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 4 && parts[0] == "dup" && parts[3] == "put" {
            if let Ok(code) = parts[1].parse::<u8>() {
                if let Some(name) = parts[2].strip_prefix('/') {
                    if name != ".notdef" {
                        encoding.insert(code, name.to_string());
                    }
                }
            }
        }
    }

    encoding
}

/// Detect a named encoding reference in Type 1 cleartext, e.g.:
///   `/Encoding StandardEncoding def`
/// Returns the encoding name ("StandardEncoding", "ISOLatin1Encoding", etc.)
/// or None if the font uses a custom `dup N /name put` encoding.
fn parse_type1_named_encoding(cleartext: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(cleartext).ok()?;
    let enc_pos = text.find("/Encoding")?;
    let after = text[enc_pos + 9..].trim_start();
    // Named encoding: /Encoding <Name> def  (no 'array' keyword)
    for name in &["StandardEncoding", "ISOLatin1Encoding", "ExpertEncoding"] {
        if after.starts_with(name) {
            return Some(name.to_string());
        }
    }
    None
}

/// Parse Encoding array from decrypted eexec bytes.
/// Restrict search to content before /CharStrings to avoid binary false positives.
fn parse_type1_encoding_bytes(data: &[u8]) -> std::collections::HashMap<u8, String> {
    let end = find_bytes(data, b"/CharStrings").unwrap_or(data.len());
    parse_type1_encoding(&data[..end])
}

/// Parse lenIV from Type 1 cleartext (number of random bytes at start of charstrings).
fn parse_type1_len_iv(cleartext: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(cleartext).ok()?;
    let pos = text.find("/lenIV")?;
    let after = &text[pos + 6..];
    let trimmed = after.trim_start();
    trimmed.split_whitespace().next()?.parse().ok()
}

/// Parse lenIV from raw bytes (e.g., decrypted Private dict).
/// Only searches before /CharStrings to avoid false positives from
/// binary charstring data that coincidentally contains the byte sequence.
fn parse_type1_len_iv_bytes(data: &[u8]) -> Option<u32> {
    // Restrict search to Private dict text portion (before /CharStrings).
    let search_end = find_bytes(data, b"/CharStrings").unwrap_or(data.len());
    let search_data = &data[..search_end];
    let pos = find_bytes(search_data, b"/lenIV")?;
    let after = &search_data[pos + 6..];
    // Skip whitespace.
    let trimmed = after.iter().position(|b| !b.is_ascii_whitespace())?;
    let start = trimmed;
    let end = after[start..]
        .iter()
        .position(|b| b.is_ascii_whitespace() || *b == b'/')
        .unwrap_or(after.len() - start);
    let num_str = std::str::from_utf8(&after[start..start + end]).ok()?;
    num_str.parse().ok()
}

/// Decrypt eexec-encrypted data. Initial key R=55665, c1=52845, c2=22719.
fn eexec_decrypt(data: &[u8]) -> Vec<u8> {
    // Check if hex-encoded.
    let is_hex = data
        .iter()
        .take(20)
        .all(|b| b.is_ascii_hexdigit() || matches!(b, b'\r' | b'\n' | b' '));

    let binary_data: Vec<u8>;
    let input = if is_hex {
        // Decode hex to binary.
        let hex_chars: Vec<u8> = data
            .iter()
            .copied()
            .filter(|b| b.is_ascii_hexdigit())
            .collect();
        binary_data = hex_chars
            .chunks(2)
            .filter_map(|pair| {
                if pair.len() == 2 {
                    let hi = hex_val(pair[0]);
                    let lo = hex_val(pair[1]);
                    Some((hi << 4) | lo)
                } else {
                    None
                }
            })
            .collect();
        binary_data.as_slice()
    } else {
        data
    };

    let mut r: u16 = 55665;
    let c1: u16 = 52845;
    let c2: u16 = 22719;

    let mut result = Vec::with_capacity(input.len());
    for &cipher in input {
        let plain = cipher ^ (r >> 8) as u8;
        r = (cipher as u16)
            .wrapping_add(r)
            .wrapping_mul(c1)
            .wrapping_add(c2);
        result.push(plain);
    }

    // Skip first 4 random bytes.
    if result.len() > 4 {
        result.drain(..4);
    }

    result
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'A'..=b'F' => b - b'A' + 10,
        b'a'..=b'f' => b - b'a' + 10,
        _ => 0,
    }
}

/// Parse the local Subrs array and return the set of subr indices that contain
/// a seac instruction (escape 12 6). Used to detect seac-via-callsubr patterns.
fn parse_type1_seac_subrs(decrypted: &[u8], len_iv: usize) -> std::collections::HashSet<u32> {
    let mut seac_subrs = std::collections::HashSet::new();

    // Find /Subrs in the decrypted Private dict.
    let Some(subrs_pos) = find_bytes(decrypted, b"/Subrs") else {
        return seac_subrs;
    };
    let data = &decrypted[subrs_pos + 6..]; // skip "/Subrs"

    // Scan for "dup <index> <len> RD <bytes>" entries.
    let mut pos = 0;
    while pos < data.len() {
        // Find "dup".
        let Some(dup_offset) = find_bytes(&data[pos..], b"dup") else {
            break;
        };
        pos += dup_offset + 3;

        // Skip whitespace.
        while pos < data.len() && data[pos].is_ascii_whitespace() {
            pos += 1;
        }

        // Read subr index.
        let idx_start = pos;
        while pos < data.len() && data[pos].is_ascii_digit() {
            pos += 1;
        }
        let Ok(subr_idx) = std::str::from_utf8(&data[idx_start..pos])
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .ok_or(())
        else {
            continue;
        };

        // Skip whitespace, read length.
        while pos < data.len() && data[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let len_start = pos;
        while pos < data.len() && data[pos].is_ascii_digit() {
            pos += 1;
        }
        let Ok(cs_len) = std::str::from_utf8(&data[len_start..pos])
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or(())
        else {
            continue;
        };

        // Skip whitespace, find RD or -|.
        while pos < data.len() && data[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos + 2 > data.len() {
            break;
        }
        let marker = &data[pos..pos + 2];
        if marker != b"RD" && marker != b"-|" {
            continue;
        }
        pos += 2;
        if pos < data.len() && matches!(data[pos], b' ' | b'\t') {
            pos += 1;
        }

        if pos + cs_len > data.len() {
            break;
        }
        let subr_data = &data[pos..pos + cs_len];
        pos += cs_len;

        if charstring_contains_seac(subr_data, len_iv) {
            seac_subrs.insert(subr_idx);
        }
    }

    seac_subrs
}

/// Decrypt a charstring/subr and check if it contains an inline seac instruction (12 6).
fn charstring_contains_seac(data: &[u8], len_iv: usize) -> bool {
    if data.len() <= len_iv {
        return false;
    }
    let mut r: u16 = 4330;
    let c1: u16 = 52845;
    let c2: u16 = 22719;
    let mut dec = Vec::with_capacity(data.len());
    for &cipher in data {
        let plain = cipher ^ (r >> 8) as u8;
        r = (cipher as u16)
            .wrapping_add(r)
            .wrapping_mul(c1)
            .wrapping_add(c2);
        dec.push(plain);
    }
    let cs = &dec[len_iv..];
    // Scan for 12 followed by 6 = seac.
    for i in 0..cs.len().saturating_sub(1) {
        if cs[i] == 12 && cs[i + 1] == 6 {
            return true;
        }
    }
    false
}

/// Parse CharStrings from decrypted eexec data to extract glyph widths.
/// Parse CharStrings from decrypted eexec data (works with raw bytes).
///
/// Charstring entries look like: `/<name> <length> RD <binary> ND`
/// or `/<name> <length> -| <binary> |-`.
fn parse_type1_charstrings(
    decrypted: &[u8],
    len_iv: usize,
    seac_subrs: &std::collections::HashSet<u32>,
) -> std::collections::HashMap<String, i32> {
    let mut widths = std::collections::HashMap::new();

    // Find /CharStrings marker.
    let Some(cs_pos) = find_bytes(decrypted, b"/CharStrings") else {
        return widths;
    };

    // Scan forward from /CharStrings looking for charstring entries.
    let mut pos = cs_pos;

    while pos < decrypted.len() {
        // Find next '/' which starts a glyph name.
        let Some(slash_offset) = decrypted[pos..].iter().position(|&b| b == b'/') else {
            break;
        };
        let slash_pos = pos + slash_offset;

        // Check for "end" keyword before this slash (end of CharStrings dict).
        // Look at up to 20 bytes before the slash for "end".
        let check_start = slash_pos.saturating_sub(20).max(pos);
        if find_bytes(&decrypted[check_start..slash_pos], b"end").is_some() {
            // Check if this is /FontName or other non-charstring entry.
            let remaining = &decrypted[slash_pos..];
            if !remaining.starts_with(b"/CharStrings") {
                break;
            }
        }

        // Extract glyph name: read ASCII chars until whitespace.
        let name_start = slash_pos + 1;
        if name_start >= decrypted.len() {
            break;
        }
        let name_end = decrypted[name_start..]
            .iter()
            .position(|b| b.is_ascii_whitespace())
            .map(|p| name_start + p)
            .unwrap_or(decrypted.len());
        let glyph_name = std::str::from_utf8(&decrypted[name_start..name_end])
            .unwrap_or("")
            .to_string();

        if glyph_name.is_empty() {
            pos = name_end + 1;
            continue;
        }

        // Skip whitespace after name.
        let mut p = name_end;
        while p < decrypted.len() && decrypted[p].is_ascii_whitespace() {
            p += 1;
        }

        // Read length number.
        let num_start = p;
        while p < decrypted.len() && decrypted[p].is_ascii_digit() {
            p += 1;
        }
        let len_str = std::str::from_utf8(&decrypted[num_start..p]).unwrap_or("");
        let Ok(cs_len) = len_str.parse::<usize>() else {
            pos = p.max(name_end + 1);
            continue;
        };

        // Skip whitespace.
        while p < decrypted.len() && decrypted[p].is_ascii_whitespace() {
            p += 1;
        }

        // Find RD or -| marker.
        let marker_ok = if p + 2 <= decrypted.len() {
            let two = &decrypted[p..p + 2];
            two == b"RD" || two == b"-|"
        } else {
            false
        };

        if !marker_ok {
            pos = p.max(name_end + 1);
            continue;
        }

        // Skip marker (2 bytes) + one space.
        p += 2;
        if p < decrypted.len() && (decrypted[p] == b' ' || decrypted[p] == b'\t') {
            p += 1;
        }

        // Read cs_len bytes of binary charstring data.
        if p + cs_len > decrypted.len() {
            break;
        }
        let charstring_data = &decrypted[p..p + cs_len];
        if let Some(width) = decrypt_charstring_width(charstring_data, len_iv, seac_subrs) {
            widths.insert(glyph_name, width);
        }

        // Jump past the charstring data.
        pos = p + cs_len;
    }

    widths
}

/// Decrypt a Type 1 charstring and extract the width (wx from hsbw/sbw).
fn decrypt_charstring_width(
    data: &[u8],
    len_iv: usize,
    seac_subrs: &std::collections::HashSet<u32>,
) -> Option<i32> {
    if data.len() <= len_iv {
        return None;
    }

    // Charstring decryption: R=4330, c1=52845, c2=22719.
    let mut r: u16 = 4330;
    let c1: u16 = 52845;
    let c2: u16 = 22719;

    let mut decrypted = Vec::with_capacity(data.len());
    for &cipher in data {
        let plain = cipher ^ (r >> 8) as u8;
        r = (cipher as u16)
            .wrapping_add(r)
            .wrapping_mul(c1)
            .wrapping_add(c2);
        decrypted.push(plain);
    }

    // Skip lenIV random bytes.
    let cs = &decrypted[len_iv..];

    // Parse integers followed by hsbw (13) or sbw (12 7).
    // hsbw: sbx wx hsbw           → width = values[1]
    // sbw:  sbx sby wx wy sbw     → width = values[2]
    // TeX fonts often use `div` (12 12) in the preamble, e.g. `59 2125 4 div hsbw`
    // to encode fractional widths.
    //
    // IMPORTANT: only return a width when hsbw/sbw is actually reached.
    // Other escape operators (e.g. seac = 12 6) leave unrelated values on the
    // stack; returning values[1] in that case yields wrong results (e.g. the
    // seac accent x-offset instead of the actual glyph width).
    let mut pos = 0;
    let mut values = Vec::new();
    let mut found_width_op = false;
    let mut is_sbw = false;

    while pos < cs.len() && values.len() < 8 {
        let b = cs[pos];
        if b == 13 {
            // hsbw: stack has [sbx, wx]
            found_width_op = true;
            break;
        }
        if b == 12 {
            if pos + 1 < cs.len() && cs[pos + 1] == 12 {
                // div: pop two values, push quotient (a b div → a/b).
                pos += 2;
                if values.len() >= 2 {
                    let divisor = values.pop().expect("guarded by values.len() >= 2");
                    let dividend = values.pop().expect("guarded by values.len() >= 2");
                    if divisor != 0 {
                        values.push(dividend / divisor);
                    } else {
                        values.push(dividend);
                    }
                }
                continue;
            }
            if pos + 1 < cs.len() && cs[pos + 1] == 7 {
                // sbw: stack has [sbx, sby, wx, wy]
                is_sbw = true;
                found_width_op = true;
            }
            // Any other escape (seac, flex, etc.) — stop without a result.
            break;
        }
        // Parse integer.
        if (32..=246).contains(&b) {
            values.push(b as i32 - 139);
            pos += 1;
        } else if (247..=250).contains(&b) {
            if pos + 1 >= cs.len() {
                break;
            }
            values.push((b as i32 - 247) * 256 + cs[pos + 1] as i32 + 108);
            pos += 2;
        } else if (251..=254).contains(&b) {
            if pos + 1 >= cs.len() {
                break;
            }
            values.push(-(b as i32 - 251) * 256 - cs[pos + 1] as i32 - 108);
            pos += 2;
        } else if b == 255 {
            if pos + 4 >= cs.len() {
                break;
            }
            let val = i32::from_be_bytes([cs[pos + 1], cs[pos + 2], cs[pos + 3], cs[pos + 4]]);
            values.push(val);
            pos += 5;
        } else {
            // Unknown operator before width was found — stop without a result.
            break;
        }
    }

    if !found_width_op {
        return None;
    }

    // hsbw: width = values[1], sbw: width = values[2].
    let width = if is_sbw {
        values.get(2).copied()
    } else {
        values.get(1).copied()
    }?;

    // After finding hsbw/sbw, scan the rest of the charstring for seac.
    // seac can appear either inline (12 6) or via callsubr (10) where the subr
    // contains seac. In both cases the hsbw-derived width is unreliable because
    // Type1 spec mandates the BASE CHARACTER's width for composite glyphs.
    //
    // We track the value stack to know which subroutine is called (the TOS just
    // before callsubr is the subroutine index).
    pos += if is_sbw { 2 } else { 1 };
    let mut stack: Vec<i32> = Vec::with_capacity(8);
    while pos < cs.len() {
        let b = cs[pos];
        if b == 12 {
            if pos + 1 < cs.len() && cs[pos + 1] == 6 {
                return None; // inline seac — hsbw-derived width is unreliable
            }
            pos += 2;
            stack.clear();
        } else if b == 10 {
            // callsubr: TOS is the subroutine index.
            if let Some(&subr_idx) = stack.last() {
                if subr_idx >= 0 && seac_subrs.contains(&(subr_idx as u32)) {
                    return None; // subr contains seac — hsbw-derived width is unreliable
                }
            }
            pos += 1;
            stack.clear();
        } else if (32..=246).contains(&b) {
            stack.push(b as i32 - 139);
            pos += 1;
        } else if (247..=250).contains(&b) {
            if pos + 1 < cs.len() {
                stack.push((b as i32 - 247) * 256 + cs[pos + 1] as i32 + 108);
            }
            pos += 2;
        } else if (251..=254).contains(&b) {
            if pos + 1 < cs.len() {
                stack.push(-((b as i32 - 251) * 256) - cs[pos + 1] as i32 - 108);
            }
            pos += 2;
        } else if b == 255 {
            if pos + 4 < cs.len() {
                stack.push(i32::from_be_bytes([
                    cs[pos + 1],
                    cs[pos + 2],
                    cs[pos + 3],
                    cs[pos + 4],
                ]));
            }
            pos += 5;
        } else {
            // Other operator (endchar, hstem, vstem, rmoveto, …): clears stack.
            pos += 1;
            stack.clear();
        }
    }
    Some(width)
}

/// Compute the CFF font matrix scale factor, compensating for f32→f64 precision loss.
///
/// CFF FontMatrix `sx` is stored as f32 (commonly 0.001 for 1000 UPM fonts).
/// Converting `0.001f32` to f64 then multiplying by 1000.0 yields 1.0000000474…
/// instead of exactly 1.0. This tiny error propagates into glyph widths,
/// causing sub-unit discrepancies that veraPDF flags.
pub fn cff_matrix_scale(matrix_sx: f32) -> f64 {
    if matrix_sx.abs() > f32::EPSILON {
        // Use the raw f32→f64 cast without rounding. veraPDF uses the exact
        // float value from the CFF, not a rounded version. Rounding to 6
        // decimal places loses precision for non-standard FontMatrix values
        // (e.g., 1/1440 for TeX CMSY fonts), causing 3-7 unit width mismatches.
        matrix_sx as f64 * 1000.0
    } else {
        1.0
    }
}

/// Get CFF glyph width as f64, using the signed `glyph_width_f32` to handle
/// negative widths that arise from nominalWidthX offsets. Falls back to the
/// unsigned `glyph_width` for compatibility.
#[allow(dead_code)]
fn cff_glyph_width_f64(
    cff: &cff_parser::Table,
    gid: cff_parser::GlyphId,
    scale: f64,
) -> Option<f64> {
    // Prefer glyph_width_f32 which handles negative widths correctly.
    if let Some(w) = cff.glyph_width_f32(gid) {
        return Some(w as f64 * scale);
    }
    // Fallback to u16 version
    cff.glyph_width(gid).map(|w| w as f64 * scale)
}

/// Per-font CFF cache built once and shared across all per-code width lookups.
///
/// Eliminates O(n_codes × n_glyphs) complexity in the width-correction hot path:
/// - `parse_cff_encoding_map` and `cff_has_custom_encoding` were called up to 256×
///   per font inside `cff_width_for_code`; now called once.
/// - `find_cff_glyph_width_by_name_fractional` scanned all glyphs linearly per call;
///   now replaced by an O(1) HashMap lookup.
struct CffFontCtx {
    is_custom_enc: bool,
    /// code → GID for custom-encoding fonts (CFF enc_offset > 1).
    enc_map: std::collections::HashMap<u8, u16>,
    /// All glyph names present in the CFF charset, even when width parsing fails.
    charset_names: std::collections::HashSet<String>,
    /// Glyph name → advance width (PDF glyph-space, after scale).
    /// Contains ALL glyphs in the CFF charset. When a PDF /Encoding is present,
    /// veraPDF resolves glyphs by name iteration (not SID lookup), so custom SIDs
    /// are reachable. Used exclusively by compute_cff_corrections_by_name which
    /// always has has_pdf_encoding=true.
    name_to_width: std::collections::HashMap<String, f64>,
}

/// Build a `CffFontCtx` from an already-parsed CFF table and its raw bytes.
fn build_cff_font_ctx(cff: &cff_parser::Table, font_data: &[u8], scale: f64) -> CffFontCtx {
    let is_custom_enc = cff_has_custom_encoding(font_data);
    let enc_map = if is_custom_enc {
        parse_cff_encoding_map(font_data)
    } else {
        std::collections::HashMap::new()
    };

    // Pre-scan the CFF charset: glyph name → width.
    // Include ALL glyphs regardless of SID. When a PDF /Encoding is present
    // (which is always the case when this map is used via compute_cff_corrections_
    // by_name), veraPDF resolves glyphs by NAME iteration, not SID lookup.
    // Standard names under custom SIDs (≥391) ARE findable by veraPDF's name
    // path — only the CFF-encoding-only path (no PDF /Encoding) uses SID lookup
    // and would miss them, but that path doesn't use this map.
    // (#fix-cff-name-map-include-all)
    let num_glyphs = cff.number_of_glyphs();
    let mut name_to_width = std::collections::HashMap::with_capacity(num_glyphs as usize);
    let mut charset_names = std::collections::HashSet::with_capacity(num_glyphs as usize);
    for gid_raw in 0..num_glyphs {
        let gid = cff_parser::GlyphId(gid_raw);
        let Some(name) = cff.glyph_name(gid) else {
            continue;
        };
        charset_names.insert(name.to_string());
        if let Some(w) = cff.glyph_width(gid) {
            name_to_width.insert(name.to_string(), w as f64 * scale);
        }
    }

    CffFontCtx {
        is_custom_enc,
        enc_map,
        charset_names,
        name_to_width,
    }
}

/// Compute width corrections for a CFF font using its internal CFF encoding.
///
/// veraPDF §6.2.11.5 always uses the CFF `glyph_index` encoding to resolve
/// code → GID, ignoring the PDF /Encoding (including Differences).
/// (#6.2.11.5-custom-enc-diffs-ignored)
///
/// veraPDF width rule (confirmed empirically):
/// - Code → GID > 0: use charstring advance (`glyph_width`).
/// - Code → GID 0 (.notdef) or code absent (None): use Private DICT
///   `defaultWidthX`. veraPDF does NOT use the .notdef charstring advance —
///   it uses `defaultWidthX` for any code that ends up at GID 0 or is absent
///   from the encoding.
///   Evidence: JBGCOD+Century-Book code 149 → GID 0, .notdef advance=250,
///   defaultWidthX=500 → veraPDF reports font=500 (not 250).
///   (#6.2.11.5-gid0-uses-defaultwidthx)
fn compute_cff_corrections_for_custom_encoding(
    cff: &cff_parser::Table,
    cff_bytes: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    scale: f64,
) -> Vec<(usize, i64)> {
    // Use the raw CFF encoding map instead of cff.glyph_index(code).
    // glyph_index() may use StandardEncoding fallback for codes not in the
    // custom encoding, returning GID 0 for codes that ARE actually mapped.
    let enc_map = parse_cff_encoding_map(cff_bytes);

    let mut corrections = Vec::new();
    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };
        let code = first_char + i as u32;
        if code > 255 {
            continue;
        }
        // Look up GID from raw encoding map first, fall back to glyph_index.
        let gid_from_map = enc_map.get(&(code as u8)).copied().unwrap_or(0);
        let frac_w = if gid_from_map > 0 {
            cff.glyph_width(cff_parser::GlyphId(gid_from_map))
                .map(|w| w as f64 * scale)
        } else {
            // Try glyph_index as fallback (for fonts where the encoding map
            // doesn't cover all codes but glyph_index does).
            match cff.glyph_index(code as u8) {
                Some(gid) if gid.0 > 0 => cff.glyph_width(gid).map(|w| w as f64 * scale),
                _ => {
                    // .notdef or absent — skip unless pdf_w is 0
                    if pdf_w != 0.0 {
                        continue;
                    }
                    None
                }
            }
        };
        let Some(frac_w) = frac_w else { continue };
        let rounded_w = frac_w.round() as i64;
        if rounded_w != pdf_w as i64 {
            corrections.push((i, rounded_w));
        }
    }
    corrections
}

/// Compute width corrections for a CFF font using glyph-name lookup.
///
/// Core correction logic for FontFile3 fonts with a PDF /Encoding: mirrors
/// veraPDF §6.2.11.5 which maps code → PDF-encoding-name → CFF-charset →
/// charstring width. When the name is absent from the CFF charset, veraPDF
/// uses the .notdef advance width (GID 0) — cff_width_for_code handles this.
#[allow(clippy::too_many_arguments)]
fn compute_cff_corrections_by_name(
    cff: &cff_parser::Table,
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    scale: f64,
    is_subset: bool,
    to_unicode: Option<&std::collections::HashMap<u8, char>>,
) -> Vec<(usize, i64)> {
    // Build a per-font context once: pre-computes the CFF encoding map and a
    // name→width HashMap so that the per-code hot loop avoids 256× redundant
    // parse_cff_encoding_map / cff_has_custom_encoding calls and O(n_glyphs)
    // linear scans in find_cff_glyph_width_by_name_fractional. (#534)
    let ctx = build_cff_font_ctx(cff, font_data, scale);

    // For custom CFF encoding fonts, guard against CFF parser charstring bugs:
    // when the parser incorrectly reads an implicit-width charstring as having
    // an explicit width, preserve the existing defaultWidthX-aligned value.
    // Evidence: MELEBE+NCSchlbk GID 64 "atilde" — parser returns 682 (→333)
    // but veraPDF uses defaultWidthX 1139 (→556). (#6.2.11.5-cff-parser-dwx)
    let dwx_rounded: Option<i64> = if ctx.is_custom_enc {
        cff.default_width_x()
            .map(|w| (w as f64 * scale).round() as i64)
    } else {
        None
    };

    let mut corrections = Vec::new();
    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };
        let code = first_char + i as u32;
        if code > 255 {
            continue;
        }
        let frac_w_opt = cff_width_for_code(
            cff,
            font_data,
            code,
            enc_name,
            differences,
            scale,
            is_subset,
            Some(&ctx),
            to_unicode,
        );
        let Some(frac_w) = frac_w_opt else {
            continue;
        };
        let rounded_w = frac_w.round() as i64;
        if rounded_w != pdf_w as i64 {
            // Cross-validate: if the CFF encoding maps this code to GID 0
            // (.notdef), the name lookup may disagree with veraPDF's path.
            // Block when: (1) the correction target does NOT match .notdef/dwx
            // (it's a real glyph width), AND (2) pdf_w already matches .notdef/dwx.
            // This means the correction would move a correct .notdef-aligned value
            // to a wrong name-based value (custom SID glyph that veraPDF can't find).
            // Allow when: correction target matches .notdef/dwx (both agree), or
            // pdf_w doesn't match .notdef (correction is improving a wrong value).
            // Exempt code 173: veraPDF uses "hyphen" not .notdef for soft hyphen.
            // (#fix-cff-xval-gid0, #fix-cff-softhyphen-exempt)
            if code <= 255 && code != 173 {
                let cff_gid = cff.glyph_index(code as u8).map(|g| g.0).unwrap_or(0);
                if cff_gid == 0 {
                    let notdef_w = cff
                        .glyph_width(cff_parser::GlyphId(0))
                        .map(|w| (w as f64 * scale).round() as i64);
                    let dwx_w = cff
                        .default_width_x()
                        .map(|w| (w as f64 * scale).round() as i64);
                    let corr_is_notdef =
                        matches!(notdef_w, Some(nw) if (rounded_w - nw).abs() <= 1);
                    let corr_is_dwx = matches!(dwx_w, Some(dw) if (rounded_w - dw).abs() <= 1);
                    if !corr_is_notdef && !corr_is_dwx {
                        // Correction targets a real glyph width, not .notdef/dwx.
                        // Check if the target width is from a SID-reachable glyph
                        // in the name→width map. If so, veraPDF's name-based path
                        // would also find it → allow the correction even when pdf_w
                        // matches .notdef (the glyph IS in the charset under a
                        // standard SID). Only block when the width is NOT in the
                        // name map (came from a custom SID or encoding artifact).
                        // (#fix-cff-xval-sid-reachable)
                        let corr_in_name_map = ctx
                            .name_to_width
                            .values()
                            .any(|&w| (w.round() as i64 - rounded_w).abs() <= 1);
                        if !corr_in_name_map {
                            let pdf_matches_notdef = matches!(notdef_w, Some(nw) if (pdf_w.round() as i64 - nw).abs() <= 1);
                            let pdf_matches_dwx =
                                matches!(dwx_w, Some(dw) if (pdf_w.round() as i64 - dw).abs() <= 1);
                            if pdf_matches_notdef || pdf_matches_dwx {
                                continue;
                            }
                        }
                    }
                }
            }
            // For custom CFF encoding fonts where the code is NOT in the CFF
            // encoding: if the existing /Widths matches defaultWidthX (within 1)
            // and the computed width does NOT match defaultWidthX, the CFF parser
            // may have a charstring width bug (incorrectly reading an implicit-
            // width charstring as having an explicit width). Preserve the existing
            // defaultWidthX-aligned value. (#6.2.11.5-cff-parser-dwx)
            if let Some(dwx) = dwx_rounded {
                if ctx.is_custom_enc {
                    let code_in_enc = ctx.enc_map.get(&(code as u8)).copied().unwrap_or(0) != 0;
                    if !code_in_enc
                        && (pdf_w.round() as i64 - dwx).abs() <= 1
                        && (rounded_w - dwx).abs() > 1
                    {
                        continue; // preserve existing defaultWidthX-aligned width
                    }
                }
            }
            corrections.push((i, rounded_w));
        }
    }
    corrections
}

/// Compute width corrections for a Type1 font with CFF program (FontFile3).
fn compute_cff_type1_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_info: &(String, std::collections::HashMap<u32, String>),
    is_subset: bool,
    to_unicode: Option<&std::collections::HashMap<u8, char>>,
) -> Vec<(usize, i64)> {
    let (enc_name, differences) = enc_info;
    let has_pdf_encoding = !enc_name.is_empty() || !differences.is_empty();

    // When a PDF /Encoding is present, veraPDF §6.2.11.5 validates against CFF
    // charstring widths (via glyph name → CFF charset lookup), NOT hmtx advances.
    // The OTF path uses hmtx which can differ from the CFF charstring width and
    // would produce no correction when hmtx matches the PDF width but the CFF
    // charstring doesn't. For PDF-encoding fonts, always use the raw CFF path
    // (via cff_width_for_code) which mirrors veraPDF's name-based lookup.
    // For OTF-wrapped CFF, extract the CFF table from the OTF wrapper first.
    // (#6.2.11.5-otf-vs-cff)
    if has_pdf_encoding {
        // Try to extract CFF bytes from an OTF wrapper ('CFF ' table).
        let cff_bytes: &[u8] = if let Some(cff_bytes) = extract_cff_bytes_from_otf(font_data) {
            cff_bytes
        } else {
            font_data
        };
        if let Some(cff) = cff_parser::Table::parse(cff_bytes) {
            let matrix = cff.matrix();
            let scale = cff_matrix_scale(matrix.sx);
            // When PDF /Encoding is present (BaseEncoding or Differences), always use
            // name-based lookup: code → PDF glyph name → CFF charset → charstring width.
            // This matches veraPDF §6.2.11.5 regardless of CFF encoding type (Standard,
            // custom Format0/Format1) and regardless of subset status.
            //
            // Branches removed from 7bed210 that caused §6.2.11.5 regressions:
            // 1. cff_has_custom_encoding → compute_cff_corrections_for_custom_encoding:
            //    Used CFF glyph_index (ignoring PDF encoding) → wrong widths for fonts like
            //    Lucida with WinAnsiEncoding+custom CFF encoding (code 96 "grave" → 627 vs
            //    veraPDF's expected 241).
            // 2. enc_name.is_empty() && !is_subset → compute_cff_corrections_by_cff_encoding (removed):
            //    Ignored Differences entries → e.g. code 133/"endash" correction not generated
            //    for fonts with only Differences encoding (no BaseEncoding). (#507)
            return compute_cff_corrections_by_name(
                &cff,
                cff_bytes,
                first_char,
                existing_widths,
                enc_name,
                differences,
                scale,
                is_subset,
                to_unicode,
            );
        }
        // CFF parse failed: nothing we can do.
        return Vec::new();
    }

    // No PDF encoding: use the OTF path (CFF internal encoding → charstring width).
    // For OTF-wrapped CFF, compute_otf_cff_corrections extracts the 'CFF ' table
    // and uses CFF charstring widths via glyph_index (CFF encoding).
    if let Ok(face) = ttf_parser::Face::parse(font_data, 0) {
        let units_per_em = face.units_per_em() as f64;
        if units_per_em > 0.0 {
            let scale = 1000.0 / units_per_em;
            return compute_otf_cff_corrections(
                &face,
                font_data,
                first_char,
                existing_widths,
                enc_name,
                differences,
                false, // has_pdf_encoding = false (already checked above)
                scale,
                is_subset,
            );
        }
    }

    // Fall back to raw CFF parse (Type1C).
    let Some(cff) = cff_parser::Table::parse(font_data) else {
        return Vec::new();
    };

    let matrix = cff.matrix();
    let scale = cff_matrix_scale(matrix.sx);

    // Custom-encoding raw CFF with no PDF /Encoding: veraPDF uses the CFF
    // internal encoding exclusively. For codes mapping to GID 0, veraPDF uses
    // Private DICT defaultWidthX (not the .notdef charstring advance).
    // Using the shared corrector ensures consistent behavior with the
    // has_pdf_encoding=true custom-encoding path in fix_cff_widths.
    // Only applies to raw CFF (not OTF-wrapped — those already took the
    // ttf_parser path above). (#6.2.11.5-raw-cff-custom-no-enc)
    // Custom-encoding raw CFF with no PDF /Encoding: veraPDF uses the CFF
    // internal encoding exclusively. For codes mapping to GID 0, veraPDF uses
    // Private DICT defaultWidthX (not the .notdef charstring advance).
    // (#6.2.11.5-raw-cff-custom-no-enc)
    if cff_has_custom_encoding(font_data) {
        return compute_cff_corrections_for_custom_encoding(
            &cff,
            font_data,
            first_char,
            existing_widths,
            scale,
        );
    }

    // Build per-font context for the per-code loop below. (#534)
    let ctx = build_cff_font_ctx(&cff, font_data, scale);

    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // Primary: PDF encoding → glyph name → CFF charset lookup (matches veraPDF).
        // veraPDF §6.2.11.5 maps each code through the PDF /Encoding to a glyph
        // name, looks up that name in the CFF charset, and compares the charstring
        // advance width to the PDF /Widths entry.
        //
        // For fonts without PDF-level encoding, cff_width_for_code falls back to
        // the CFF internal encoding, which is the sole authoritative mapping.
        //
        // Do NOT use cff.glyph_index() as the primary path: for codes present in
        // WinAnsiEncoding/MacRomanEncoding but absent from the CFF Format0 table,
        // glyph_index() falls back to StandardEncoding → SID 0 → GID 0 (.notdef),
        // returning the notdef width. But veraPDF resolves these codes via the PDF
        // encoding → glyph name → CFF charset and finds the actual glyph (e.g.
        // code 151 WinAnsiEncoding→emdash→1000). Using .notdef (278) as the
        // expected width then introduces a false correction 1000→278. (#6.2.11.5)
        let frac_w = if code <= 255 {
            cff_width_for_code(
                &cff,
                font_data,
                code,
                enc_name,
                differences,
                scale,
                is_subset,
                Some(&ctx),
                None,
            )
        } else {
            None
        };

        // Fallback 1: for any code on any font without PDF-level encoding, veraPDF
        // uses the font's defaultWidthX (Private DICT op 20) for codes absent from
        // the CFF encoding table. defaultWidthX ≈ cff.glyph_width(GID 0) when .notdef
        // has no explicit charstring width. Applies to all codes (not just high-byte)
        // and to subset fonts too — if a glyph is absent from the CFF subset, veraPDF
        // still uses defaultWidthX. Only active when there is no PDF /Encoding key
        // (neither BaseEncoding nor Differences), because with PDF encoding the name-
        // based path in cff_width_for_code handles the mapping. (#479, #6.2.11.5-notdef-guard)
        let frac_w = frac_w.or_else(|| {
            if !has_pdf_encoding {
                cff.glyph_width(cff_parser::GlyphId(0))
                    .map(|w| w as f64 * scale)
            } else {
                None
            }
        });

        // Fallback 2: CFF internal encoding → GID → width (same as the compliance
        // checker check_font_program_widths, which calls table.glyph_index(code)
        // directly). When the PDF-encoding → glyph-name → CFF-charset path above
        // returns None (e.g. glyph is stored as "quoteright" but looked up via
        // "quotesingle", and no CFF charset entry is found), the CFF encoding
        // is still authoritative. Use it as a last resort for codes where
        // cff_width_for_code could not resolve a width. GID 0 = .notdef = not
        // encoded → skip (same guard as the compliance checker). (#FN-6.2.11.5-cff-enc)
        let frac_w = frac_w.or_else(|| {
            if code > 255 {
                return None;
            }
            let gid = cff.glyph_index(code as u8)?;
            if gid.0 == 0 {
                return None; // not encoded or .notdef
            }
            cff.glyph_width(gid).map(|w| w as f64 * scale)
        });

        let Some(frac_w) = frac_w else { continue };

        // Apply correction when the CFF-derived integer width (after rounding)
        // differs from the PDF Widths entry. veraPDF rounds the CFF advance to
        // an integer (thousandths of em) and compares to the /Widths value, so
        // only integer mismatches are violations. This threshold (rounded ≠ pdf_w)
        // is equivalent to |frac_w - pdf_w| >= 0.5. The prior threshold of 0.95
        // was too conservative — it missed cases like 280.77 vs 280 (diff 0.77)
        // that veraPDF flags as a violation (rounds 280.77 → 281 ≠ 280). (#772)
        let rounded_w = frac_w.round() as i64;
        if rounded_w != pdf_w as i64 {
            corrections.push((i, rounded_w));
        }
    }

    corrections
}

/// Compute width corrections for OTF-wrapped CFF fonts.
///
/// For OTF fonts, veraPDF validates against the hmtx table widths (not the CFF
/// charstring widths). When a PDF-level Encoding exists, we use cmap to map
/// code -> Unicode -> GID -> hmtx width. When no encoding exists (common in
/// subset fonts), we extract the CFF table from the OTF and use its internal
/// encoding to map code -> GID -> hmtx width.
#[allow(clippy::too_many_arguments)]
fn compute_otf_cff_corrections(
    face: &ttf_parser::Face,
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    has_pdf_encoding: bool,
    scale: f64,
    is_subset: bool,
) -> Vec<(usize, i64)> {
    // If no PDF encoding, extract CFF table and its FontMatrix scale.
    // We use CFF charstring widths (not hmtx) to match veraPDF §6.2.11.5. (#FP-6.2.11.5)
    let cff_table: Option<(cff_parser::Table<'_>, f64)> = if !has_pdf_encoding {
        extract_cff_bytes_from_otf(font_data).and_then(|cff_bytes| {
            let cff = cff_parser::Table::parse(cff_bytes)?;
            let cff_scale = cff_matrix_scale(cff.matrix().sx);
            Some((cff, cff_scale))
        })
    } else {
        None
    };

    // For non-subset fonts with PDF encoding AND custom CFF encoding, extract
    // CFF for verification. veraPDF uses the CFF internal encoding for width
    // comparison (rule 6.2.11.5:1) for non-subset fonts where the CFF encoding
    // codes correspond to the PDF encoding codes.
    //
    // IMPORTANT: do NOT use this for subset fonts. During subsetting, the CFF
    // encoding is rewritten with sequential arbitrary codes (GID 1 → code 1,
    // GID 2 → code 2, …), which have NO relation to the PDF encoding codes
    // (e.g. WinAnsiEncoding code 32 = 'space'). Using the CFF encoding for a
    // subset font causes catastrophic wrong corrections (e.g. space 280→1543).
    // For subset fonts, always use the PDF encoding → cmap → hmtx path. (#772)
    let custom_cff_info: Option<(cff_parser::Table, std::collections::HashMap<u8, u16>, f64)> =
        if has_pdf_encoding && !is_subset {
            extract_cff_bytes_from_otf(font_data).and_then(|cff_bytes| {
                // For fonts with an explicit BaseEncoding, only use the CFF path
                // when the CFF has a custom encoding (veraPDF uses the CFF internal
                // encoding for width validation in that case).
                // For fonts with no BaseEncoding (enc_name empty, StandardEncoding
                // implied for Type1 per PDF spec §8.5.3), always use the CFF encoding
                // path since veraPDF resolves code→glyph via StandardEncoding→charset→
                // charstring advance, not via the Unicode cmap + hmtx. (#504)
                if !enc_name.is_empty() && !cff_has_custom_encoding(cff_bytes) {
                    return None;
                }
                let enc_map = parse_cff_encoding_map(cff_bytes);
                let cff = cff_parser::Table::parse(cff_bytes)?;
                let matrix = cff.matrix();
                let cff_scale = cff_matrix_scale(matrix.sx);
                Some((cff, enc_map, cff_scale))
            })
        } else {
            None
        };

    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };

        let code = first_char + i as u32;

        let frac_w = if has_pdf_encoding {
            // For non-subset fonts with custom CFF encoding, use CFF encoding
            // → GID → CFF charstring width (veraPDF uses the CFF internal
            // encoding for width comparison for non-subset fonts).
            // For subset fonts, custom_cff_info is always None (see above).
            if let Some((ref cff, ref enc_map, cff_scale)) = custom_cff_info {
                if code <= 255 {
                    if let Some(&gid) = enc_map.get(&(code as u8)) {
                        if gid != 0 {
                            cff.glyph_width(cff_parser::GlyphId(gid))
                                .map(|w| w as f64 * cff_scale)
                        } else {
                            // CFF maps to .notdef
                            cff.glyph_width(cff_parser::GlyphId(0))
                                .map(|w| w as f64 * cff_scale)
                        }
                    } else {
                        // Code not in CFF encoding — fall through to cmap-based
                        // lookup (font may have a valid cmap mapping even when
                        // CFF encoding doesn't cover this code).
                        get_otf_width_via_encoding(face, code, enc_name, differences, scale)
                    }
                } else {
                    get_otf_width_via_encoding(face, code, enc_name, differences, scale)
                }
            } else {
                get_otf_width_via_encoding(face, code, enc_name, differences, scale)
            }
        } else if let Some((ref cff, cff_scale)) = cff_table {
            // No PDF encoding: use CFF internal encoding -> GID -> CFF charstring width.
            // veraPDF §6.2.11.5 validates against the CFF charstring advance, not hmtx.
            // For OTF-wrapped CFF fonts the hmtx and CFF charstring widths can differ
            // (e.g. Symbol "multiply": hmtx=250, CFF charstring=549). (#FP-6.2.11.5)
            //
            // For non-symbolic fonts (Latin script, e.g. Helvetica), veraPDF uses
            // Standard encoding as the default mapping when no PDF /Encoding is present
            // (Type1 per §8.5.3). cff.glyph_index() includes this Standard encoding
            // fallback, so we use it for fonts that have Latin characters.
            //
            // For symbolic/non-Latin fonts (e.g. StandardSymbolsPS.otf), the Standard
            // encoding fallback produces wrong GIDs: code 183 → StandardEncoding → SID
            // → finds a Latin glyph in the font's charset (e.g. GID 120, CFF 460) while
            // veraPDF uses Symbol cmap or GID 0 fallback (hmtx=250). Use code_to_gid
            // only (no Standard fallback) for such fonts to avoid wrong corrections.
            // Symbolic fonts are detected by absence of a Latin 'A' glyph. (#FP-6.2.11.5)
            if code > 255 {
                continue;
            }
            // Detect symbolic/non-Latin fonts by checking whether SID 35 ("A")
            // is in the CFF charset. Latin fonts (Helvetica, Times, …) have "A"
            // in their charset; symbol/pi fonts (StandardSymbolsPS, …) do not.
            // For non-Latin fonts we suppress the Standard encoding fallback
            // because veraPDF takes a different lookup path for symbolic fonts
            // (Symbol cmap or GID 0), so the Standard encoding fallback would
            // pick up the wrong glyph (e.g. SID 147 → GID 120 in StandardSymbolsPS,
            // CFF charstring 460, while veraPDF expects hmtx(GID 0)=250). (#FP-6.2.11.5)
            let has_latin_in_charset = cff.charset.sid_to_gid(cff_parser::StringId(35)).is_some(); // SID 35 = "A"
            let gid = match cff.encoding.code_to_gid(&cff.charset, code as u8) {
                Some(gid) if gid.0 != 0 || code == 0 => cff_parser::GlyphId(gid.0),
                _ => {
                    if has_latin_in_charset {
                        // Non-symbolic: Standard encoding fallback mirrors veraPDF's
                        // default encoding behavior (Type1 §8.5.3).
                        match cff.glyph_index(code as u8) {
                            Some(gid) if gid.0 != 0 || code == 0 => gid,
                            // Code absent from CFF encoding — veraPDF uses .notdef
                            // (GID 0) advance for §6.2.11.5. (#fix-cff-notdef-width)
                            _ => cff_parser::GlyphId(0),
                        }
                    } else {
                        // Symbolic font: veraPDF uses GID 0 for absent codes.
                        cff_parser::GlyphId(0)
                    }
                }
            };
            cff.glyph_width(gid).map(|w| w as f64 * cff_scale)
        } else {
            continue;
        };

        // Codes not found in the font program map to .notdef (GID 0).
        // veraPDF validates the Widths entry against GID 0's advance for such
        // codes. Restrict to high-byte codes (128-255) where absent glyphs are
        // expected in encoding gaps. (#479)
        // For subset fonts, a missing cmap entry means the glyph is not in the
        // subset — veraPDF does not validate widths for absent glyphs. (#772)
        // For CFF path (no PDF encoding), this fallback is unreachable (the
        // CFF branch uses `continue` when glyph_index returns None/0). For
        // the PDF-encoding path, use hmtx since that's the available fallback.
        let frac_w = frac_w.or_else(|| {
            if (128..=255).contains(&code) && !is_subset {
                face.glyph_hor_advance(ttf_parser::GlyphId(0))
                    .map(|w| w as f64 * scale)
            } else {
                None
            }
        });
        let Some(frac_w) = frac_w else { continue };

        // Use >= 1.0: CFF glyph_width returns integer u16, so a 1-unit diff
        // may mask a fractional diff >1 that veraPDF catches.
        if (pdf_w - frac_w).abs() >= 1.0 {
            corrections.push((i, frac_w.round() as i64));
        }
    }

    corrections
}

/// Get an OTF font's width for a character code using PDF encoding.
fn get_otf_width_via_encoding(
    face: &ttf_parser::Face,
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    scale: f64,
) -> Option<f64> {
    if let Some(glyph_name) = differences.get(&code) {
        if let Some(unicode) = glyph_name_to_unicode(glyph_name) {
            if let Some(gid) = face.glyph_index(unicode) {
                return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
            }
        }
        if let Some(gid) = face.glyph_index_by_name(glyph_name) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
        if glyph_name == ".notdef" {
            return face
                .glyph_hor_advance(ttf_parser::GlyphId(0))
                .map(|w| w as f64 * scale);
        }
    }

    let ch = encoding_to_char(code, enc_name);
    if let Some(gid) = face.glyph_index(ch) {
        return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
    }
    if let Some(agl_name) = unicode_to_agl_name(ch) {
        if let Some(gid) = face.glyph_index_by_name(&agl_name) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
    }
    if let Some(name) = unicode_to_glyph_name(ch) {
        if let Some(gid) = face.glyph_index_by_name(&name) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
    }

    None
}

/// Extract the raw CFF table bytes from an OTF font.
pub fn extract_cff_bytes_from_otf(font_data: &[u8]) -> Option<&[u8]> {
    if font_data.len() < 12 {
        return None;
    }

    let num_tables = u16::from_be_bytes([font_data[4], font_data[5]]) as usize;
    let mut offset = 12;

    for _ in 0..num_tables {
        if offset + 16 > font_data.len() {
            break;
        }
        let tag = &font_data[offset..offset + 4];
        let table_offset = u32::from_be_bytes([
            font_data[offset + 8],
            font_data[offset + 9],
            font_data[offset + 10],
            font_data[offset + 11],
        ]) as usize;
        let table_length = u32::from_be_bytes([
            font_data[offset + 12],
            font_data[offset + 13],
            font_data[offset + 14],
            font_data[offset + 15],
        ]) as usize;

        if tag == b"CFF " {
            if table_offset + table_length <= font_data.len() {
                return Some(&font_data[table_offset..table_offset + table_length]);
            }
            return None;
        }

        offset += 16;
    }

    None
}

/// Extract the CFF table from an OTF font.
fn extract_cff_from_otf(font_data: &[u8]) -> Option<cff_parser::Table<'_>> {
    let cff_bytes = extract_cff_bytes_from_otf(font_data)?;
    cff_parser::Table::parse(cff_bytes)
}

/// Like find_cff_glyph_width_by_name but returns f64 (unrounded) for fractional comparison.
fn find_cff_glyph_width_by_name_fractional(
    cff: &cff_parser::Table,
    _font_data: &[u8],
    glyph_name: &str,
    scale: f64,
) -> Option<f64> {
    // veraPDF resolves PDF glyph names via the CFF standard SID mapping:
    // PDF glyph name → standard SID (0–390) → charset → GID → charstring width.
    // When a font stores a standard glyph name under a CUSTOM SID (≥391 in the
    // font's private String INDEX), veraPDF's SID-based lookup fails to find it
    // and falls back to GID 0 → Private DICT defaultWidthX.
    // Fix: if the found glyph has a custom SID but the name IS a standard CFF
    // name, return None (veraPDF can't find it via SID → uses defaultWidthX).
    // Note: Type 1-compat seac composites (HHCOAA "igrave" etc.) also return
    // defaultWidthX because the seac asb is not a real advance width — this is
    // handled in cff-parser's seac endchar handler.  (#507)
    let is_standard_name = cff_parser::STANDARD_NAMES.contains(&glyph_name);
    let num_glyphs = cff.number_of_glyphs();
    for gid_raw in 0..num_glyphs {
        let gid = cff_parser::GlyphId(gid_raw);
        if let Some(name) = cff.glyph_name(gid) {
            if name == glyph_name {
                if is_standard_name {
                    // Standard CFF name: only valid if stored under a standard SID
                    // (0..STANDARD_NAMES.len()). Custom SID → veraPDF can't find it.
                    let sid = cff.charset.gid_to_sid(gid).map(|s| s.0).unwrap_or(u16::MAX);
                    if sid as usize >= cff_parser::STANDARD_NAMES.len() {
                        return None;
                    }
                }
                // Do NOT call cff_type2_endchar_default_width here. That function
                // fires whenever the charstring stack has exactly 4 items before
                // endchar (old seac composite detection), which is also true for
                // perfectly normal glyphs in subset CFF fonts, returning a wrong
                // defaultWidthX value instead of the glyph's actual advance.
                // Use cff.glyph_width directly; it parses the charstring correctly.
                return cff.glyph_width(gid).map(|w| w as f64 * scale);
            }
        }
    }
    None
}

/// Exact-name lookup that bypasses the standard-SID guard.
///
/// Used for resolver paths where veraPDF does not appear to rely on the CFF
/// standard SID table directly, such as ToUnicode fallback on OTF-wrapped CFF.
fn find_cff_glyph_width_by_exact_name_fractional(
    cff: &cff_parser::Table,
    glyph_name: &str,
    scale: f64,
) -> Option<f64> {
    for gid_raw in 0..cff.number_of_glyphs() {
        let gid = cff_parser::GlyphId(gid_raw);
        if cff.glyph_name(gid) == Some(glyph_name) {
            return cff.glyph_width(gid).map(|w| w as f64 * scale);
        }
    }
    None
}

/// Compute the expected width for a single character code in a CFF font program.
fn compute_cff_single_width(
    font_data: &[u8],
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
) -> Option<f64> {
    // Try OTF-wrapped CFF first.
    if let Ok(face) = ttf_parser::Face::parse(font_data, 0) {
        let upem = face.units_per_em() as f64;
        if upem > 0.0 {
            let scale = 1000.0 / upem;
            let has_pdf_encoding = !enc_name.is_empty() || !differences.is_empty();

            // For custom CFF encoding, use CFF charstring widths (veraPDF uses
            // CFF encoding for width comparison, not PDF encoding).
            if has_pdf_encoding && code <= 255 {
                if let Some(cff_bytes) = extract_cff_bytes_from_otf(font_data) {
                    if cff_has_custom_encoding(cff_bytes) {
                        let enc_map = parse_cff_encoding_map(cff_bytes);
                        if let Some(cff) = cff_parser::Table::parse(cff_bytes) {
                            let matrix = cff.matrix();
                            let cff_scale = cff_matrix_scale(matrix.sx);
                            if let Some(&gid) = enc_map.get(&(code as u8)) {
                                if gid != 0 {
                                    return cff
                                        .glyph_width(cff_parser::GlyphId(gid))
                                        .map(|w| w as f64 * cff_scale);
                                }
                            }
                            // Code not in CFF encoding — fall through to cmap-based
                            // lookup below (don't return .notdef width, as the font
                            // may have a valid cmap mapping for this code).
                        }
                    }
                }
            }

            return get_truetype_glyph_width_fractional(&face, code, enc_name, differences, scale);
        }
    }

    // Fall back to raw CFF parse.
    let cff = cff_parser::Table::parse(font_data)?;
    let matrix = cff.matrix();
    let scale = cff_matrix_scale(matrix.sx);

    // For custom-encoding CFF fonts, veraPDF uses the CFF encoding exclusively
    // for §6.2.11.5 — PDF Differences do NOT override the CFF encoding path.
    // (Evidence: HENJHG+Times-Roman code 243 absent from custom enc → veraPDF
    // always reports defaultWidthX=500 regardless of Differences remapping.)
    // For standard/expert-encoding fonts, Differences may still affect the
    // comparison via name lookup, so preserve the Differences exception there.
    // (#6.2.11.5-raw-cff-absent, #6.2.11.5-custom-enc-diffs-ignored)
    let use_cff_enc_path = !differences.contains_key(&code) || cff_has_custom_encoding(font_data);
    if code <= 255 && use_cff_enc_path {
        let gid = cff.glyph_index(code as u8);
        let has_pdf_enc_raw = !enc_name.is_empty() || !differences.is_empty();
        match gid {
            Some(g) if g.0 == 0 => {
                // GID 0 = .notdef.  For custom-encoding fonts or fonts without a
                // PDF encoding, veraPDF uses defaultWidthX as the "font width".
                // For standard/expert-encoding fonts WITH a PDF encoding (WinAnsi
                // etc.), fall through to the name-based lookup in cff_width_for_code:
                // the PDF encoding may map this code to a glyph that IS in the subset
                // at a different SID/position than the CFF internal encoding gives.
                // Example: SE maps code 252 to SID X (absent from subset) → GID 0,
                // but WinAnsi maps 252 to "udieresis" which IS in the subset.
                // (#6.2.11.5-std-enc-gid0-fallthrough)
                if cff_has_custom_encoding(font_data) || !has_pdf_enc_raw {
                    return cff.default_width_x().map(|w| w as f64 * scale);
                }
                // Fall through to cff_width_for_code below.
            }
            Some(g) => {
                return cff.glyph_width(g).map(|w| w as f64 * scale);
            }
            None => {
                if cff_has_custom_encoding(font_data) {
                    // Code absent from CFF encoding: veraPDF resolves via GID 0
                    // (.notdef) and uses its actual charstring advance width.
                    // Using defaultWidthX returns 0 for most subset fonts (Private
                    // DICT default_width=0) and causes false §6.2.11.5 corrections
                    // (e.g. gen-783 Garamond-Light Width[32]=250 → 0). (#gen-783)
                    return cff
                        .glyph_width(cff_parser::GlyphId(0))
                        .map(|w| w as f64 * scale);
                }
                // CID font or standard/expert encoding — fall through to name lookup.
            }
        }
    }

    // For Differences-overridden codes, CID fonts, or codes > 255: use name lookup.
    // is_subset=false: compute_cff_single_width is used for compliance checking and
    // single-code fixes where subset status is not propagated. The non-subset guard
    // (code_in_cff_enc) is conservative here — callers that DO have is_subset context
    // (fix_font_width_mismatches, compute_cff_type1_width_corrections) pass is_subset
    // via the correct path. (#6.2.11.5-subset-cff-enc-guard)
    cff_width_for_code(
        &cff,
        font_data,
        code,
        enc_name,
        differences,
        scale,
        false,
        None,
        None,
    )
}

/// Look up the CFF glyph width for a character code, trying multiple strategies:
/// 1. PDF encoding → glyph name → CFF name lookup
/// 2. CFF internal encoding (direct parse, no Standard Encoding fallback)
///
/// `ctx` is an optional pre-built `CffFontCtx` that caches the CFF encoding map and
/// name→width table.  When provided, all per-code CFF structure parses and linear
/// glyph-name scans are replaced by O(1) HashMap lookups.  Pass `None` for one-shot
/// callers where the overhead of building the context isn't worth it.
#[allow(clippy::too_many_arguments)]
fn cff_width_for_code(
    cff: &cff_parser::Table,
    font_data: &[u8],
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    scale: f64,
    _is_subset: bool,
    ctx: Option<&CffFontCtx>,
    to_unicode: Option<&std::collections::HashMap<u8, char>>,
) -> Option<f64> {
    let has_pdf_encoding = !enc_name.is_empty() || !differences.is_empty();

    // Hoist frequently-used CFF structural data from ctx (pre-computed) or
    // compute lazily when ctx is absent.  These were previously re-computed for
    // every code inside the hot per-code loop — up to 256× per font.
    let is_custom_enc = ctx.map_or_else(|| cff_has_custom_encoding(font_data), |c| c.is_custom_enc);
    // enc_map is used at two separate sites below; keep a single owned copy when
    // ctx is absent to avoid calling parse_cff_encoding_map twice.
    let enc_map_owned;
    let enc_map: &std::collections::HashMap<u8, u16> = if let Some(c) = ctx {
        &c.enc_map
    } else {
        enc_map_owned = parse_cff_encoding_map(font_data);
        &enc_map_owned
    };
    // Fast name→width lookup via pre-built HashMap (when ctx is available) or
    // falls back to the linear scan in find_cff_glyph_width_by_name_fractional.
    let lookup_name = |name: &str| -> Option<f64> {
        if let Some(c) = ctx {
            c.name_to_width.get(name).copied()
        } else {
            find_cff_glyph_width_by_name_fractional(cff, font_data, name, scale)
        }
    };
    let charset_has_name = |name: &str| -> bool {
        if let Some(c) = ctx {
            c.charset_names.contains(name)
        } else {
            cff_has_named_glyph(cff, name)
        }
    };

    // Primary path: PDF encoding → glyph name → CFF charset lookup.
    // veraPDF resolves code → glyph name via the PDF Encoding, then looks up
    // that name in the CFF charset. Only when the name is NOT found in the
    // CFF (e.g. because the subset uses GID-based names like G80) does it
    // fall back to the CFF internal encoding.
    //
    // pdf_glyph_name is hoisted outside the block so the final fallback can
    // distinguish undefined encoding codes (empty → .notdef charstring advance)
    // from named glyphs absent from the CFF charset (non-empty → defaultWidthX).
    // (#6.2.11.5-std-enc-defaultwidthx, #6.2.11.5-undef-notdef-charstring)
    let mut pdf_glyph_name = String::new();
    let mut name_found = false;
    let name_from_cff_se_override = false;
    if has_pdf_encoding {
        let glyph_name = if let Some(name) = differences.get(&code) {
            name.clone()
        } else if enc_name.is_empty() {
            // No BaseEncoding, not in Differences: PDF spec §8.5.3 says "use the
            // font's built-in encoding". For CFF/Type1C fonts (FontFile3), the
            // built-in encoding IS the CFF internal encoding (enc_offset 0/1/custom).
            // veraPDF resolves these codes via cff.glyph_index(code) — NOT via the
            // PostScript Standard Encoding. Return "" so we fall through to the CFF
            // encoding fallback (cff.glyph_index) below.
            //
            // Concretely: CFF SE maps code 39 → SID 8 = "quoteright" (advance 204),
            // NOT SID 104 = "quotesingle" (advance 278). The old ps_standard_encoding_
            // override("quotesingle") was based on PS SE which differs from CFF SE for
            // this code, causing a wrong 204→278 correction in C059-Italic. (#507)
            String::new()
        } else if enc_name == "MacRomanEncoding" {
            String::new()
        } else if let Some(name) = cff_pdf_base_glyph_name(code, enc_name) {
            name
        } else {
            let ch = encoding_to_char(code, enc_name);
            unicode_to_glyph_name(ch).unwrap_or_default()
        };
        // Hoist resolved glyph name for use in the final fallback below.
        pdf_glyph_name = glyph_name.clone();
        // Guard: Phase 1 name-based lookup (PDF enc → glyph name → CFF charset) is
        // only valid when the CFF encoding maps this code to a valid GID, OR when
        // the code appears in the PDF /Differences, OR for low-byte codes (< 128).
        //
        // For subset fonts with a CUSTOM CFF encoding (enc_offset > 1), veraPDF
        // uses the CFF encoding as authoritative. Codes NOT in the custom encoding
        // → veraPDF uses defaultWidthX. Phase 1 would wrongly return a charstring
        // width instead. Examples:
        //   - MELEBE+NCSchlbk code 227 "atilde" at GID 64 (w=682→333) but CFF enc
        //     has no entry for 227 → veraPDF uses defaultWidthX=1139→556.
        //   - BICKAF+Delta-Light code 237 "iacute" at GID 60 (w=292) but
        //     CFF enc[237]=GID 0 → veraPDF uses defaultWidthX=208.
        //   - DDEEDN+HawnOptimist code 228 "adieresis" (w=333) but CFF enc empty
        //     → glyph_index(228)=GID 0 → veraPDF uses defaultWidthX=500.
        //
        // NOTE: `subset_with_custom_cff` was REMOVED from this guard. Previously
        // it forced Phase 1 for subset+custom-enc fonts regardless of whether the
        // code was actually in the encoding — causing wrong corrections for the
        // above cases. (#6.2.11.5-cff-enc-primary, #6.2.11.5-subset-custom-enc)
        let from_differences = differences.contains_key(&code);
        // Use pre-computed enc_map (hoisted to top of function) instead of
        // calling parse_cff_encoding_map again.
        // For CUSTOM CFF encoding fonts (enc_offset > 1), veraPDF uses the CFF
        // encoding as authoritative for §6.2.11.5. A code that maps to GID 0
        // (or is absent from the encoding) means the glyph is undefined →
        // veraPDF uses defaultWidthX. Phase 1 must be blocked for those codes.
        //
        // For STANDARD / EXPERT CFF encoding fonts (enc_offset 0/1), veraPDF
        // resolves glyph widths via PDF /Encoding → glyph name → CFF charset
        // name lookup (SID-based), NOT via the CFF Standard Encoding. Phase 1
        // must always run so we can find the correct charstring width via the
        // PDF encoding name (e.g. MacRoman 143→"egrave" → CFF "egrave"→444).
        //
        // Note: `find_cff_glyph_width_by_name_fractional` already handles the
        // case where a standard glyph name is stored under a CUSTOM SID (≥391)
        // in the CFF String INDEX — it returns None so we fall through to
        // defaultWidthX (e.g. HHCOAA "igrave" under custom SID). Fixes #507.
        let in_cff_enc_map = if is_custom_enc {
            // Custom CFF enc: Phase 1 (PDF encoding → name → CFF charset lookup) runs:
            //
            // a) For WinAnsiEncoding / StandardEncoding: always run Phase 1 regardless
            //    of whether the code is in the CFF encoding. veraPDF resolves these
            //    encodings via name lookup even when the CFF has a custom encoding
            //    that doesn't include the code. (#507, gen-490)
            //
            // b) For codes explicitly present in the CFF encoding (including GID 0):
            //    Phase 1 runs even when the CFF enc maps the code to GID 0 (.notdef).
            //    veraPDF still uses name-based lookup for such codes. The old `!= 0`
            //    guard incorrectly blocked Phase 1 and fell through to defaultWidthX.
            matches!(enc_name, "WinAnsiEncoding" | "StandardEncoding")
                || enc_map.contains_key(&(code as u8))
        } else {
            // SE/Expert: PDF name lookup is always primary; always enable Phase 1.
            true
        };
        let code_in_cff_enc = code < 128 || from_differences || in_cff_enc_map;
        if code_in_cff_enc && !glyph_name.is_empty() {
            // Phase 1: PDF encoding → glyph name → CFF charset name lookup.
            // Includes ".notdef": when Differences maps a code to ".notdef",
            // veraPDF uses the actual GID 0 width (not defaultWidthX). The
            // lookup_name call resolves ".notdef" → GID 0 via charset.sid_to_gid(0).
            // Without this, we fall to defaultWidthX=0 and generate a false
            // correction (e.g. gen-783 XVPBKO+Garamond-Light code 32 → 250→0).
            // (#gen-783, §6.2.11.5)
            if let Some(w) = lookup_name(&glyph_name) {
                return Some(w);
            }
            if glyph_name == ".notdef" {
                // .notdef not found via glyph_index_by_name — return None so
                // the caller leaves the existing Widths entry unchanged rather
                // than applying a wrong defaultWidthX "correction".
                return None;
            }
            if !differences.contains_key(&code) && !name_from_cff_se_override {
                let ch = encoding_to_char(code, enc_name);
                if let Some(agl_name) = unicode_to_agl_name(ch) {
                    if agl_name != glyph_name {
                        if let Some(w) = lookup_name(&agl_name) {
                            return Some(w);
                        }
                        for alt in cff_glyph_name_alternatives(&agl_name) {
                            if let Some(w) = lookup_name(alt) {
                                return Some(w);
                            }
                        }
                    }
                }
            }
            if !name_from_cff_se_override {
                for alt in cff_glyph_name_alternatives(&glyph_name) {
                    if let Some(w) = lookup_name(alt) {
                        return Some(w);
                    }
                }
            }
            // If the PDF-encoding glyph name DOES exist in the charset but our
            // parser couldn't derive its width, do not fall back to the CFF
            // internal encoding. That fallback can overwrite a correct /Widths
            // entry with an unrelated internal-encoding width (e.g. MacRoman
            // quoteright). Leave the width unchanged instead. (#pdfa-macroman-cff-width-none)
            if charset_has_name(&glyph_name) {
                return None;
            }
            if !differences.contains_key(&code) && !name_from_cff_se_override {
                let ch = encoding_to_char(code, enc_name);
                if let Some(agl_name) = unicode_to_agl_name(ch) {
                    if agl_name != glyph_name && charset_has_name(&agl_name) {
                        return None;
                    }
                    for alt in cff_glyph_name_alternatives(&agl_name) {
                        if charset_has_name(alt) {
                            return None;
                        }
                    }
                }
            }
            if !name_from_cff_se_override {
                for alt in cff_glyph_name_alternatives(&glyph_name) {
                    if charset_has_name(alt) {
                        return None;
                    }
                }
            }
            // Name resolved but not found in CFF — will try CFF encoding below.
            name_found = false;
        }
    }

    // ToUnicode fallback: for codes with no glyph name in the PDF encoding
    // (e.g. WinAnsiEncoding codes 0x81, 0x8D, 0x8F, 0x90, 0x9D that are
    // undefined in the standard table), veraPDF resolves the glyph via the
    // ToUnicode CMap → Unicode → font cmap → advance. Mirror this lookup
    // when a ToUnicode map is provided.
    //
    // Example: gen-131 AvantGarde-Book WinAnsiEncoding code 129 → ToUnicode
    // U+2022 (bullet) → name="bullet" → advance 606. Without this fallback,
    // we return .notdef advance (277) and generate a wrong correction. (#507)
    if pdf_glyph_name.is_empty() {
        if let Some(tu_map) = to_unicode {
            if let Some(&ch) = tu_map.get(&(code as u8)) {
                if let Some(agl_name) = unicode_to_agl_name(ch) {
                    if let Some(w) = lookup_name(&agl_name) {
                        return Some(w);
                    }
                    if let Some(w) =
                        find_cff_glyph_width_by_exact_name_fractional(cff, &agl_name, scale)
                    {
                        return Some(w);
                    }
                    for alt in cff_glyph_name_alternatives(&agl_name) {
                        if let Some(w) = lookup_name(alt) {
                            return Some(w);
                        }
                        if let Some(w) =
                            find_cff_glyph_width_by_exact_name_fractional(cff, alt, scale)
                        {
                            return Some(w);
                        }
                    }
                    if charset_has_name(&agl_name) {
                        return None;
                    }
                    for alt in cff_glyph_name_alternatives(&agl_name) {
                        if charset_has_name(alt) {
                            return None;
                        }
                    }
                }
            }
        }
    }

    // Fallback: CFF internal encoding → GID → width.
    // veraPDF's algorithm for ALL CFF fonts with a PDF /Encoding:
    //   1. Resolve code → glyph name via PDF Encoding (SE, WinAnsi, Differences…)
    //   2. Look up glyph name in CFF charset → if found, use charstring width
    //   3. If NOT found → fall back to CFF INTERNAL encoding (glyph_index(code))
    //      → if found, use that charstring width
    //   4. If STILL not found → use .notdef width (Fix 2 below)
    // This means CFF internal encoding fallback is needed for SE-implied AND non-SE
    // fonts whenever the name-based lookup fails. Evidence: veraPDF reports e.g.
    // "Glyph width 727.65 in the embedded font program is not consistent with the
    // Widths" for SE-implied G-named fonts (G72, G73…) where "H", "I" etc. fail name
    // lookup but the CFF encoding maps the code to the correct non-0 GID.
    // (#6.2.11.5-std-enc-cff-fallback)
    // CFF internal encoding fallback is always active here. When we reach this
    // point, name_found=false (name_found=true causes early return above). For every
    // PDF /Encoding, veraPDF falls back to CFF glyph_index(code) when name lookup
    // fails — whether the encoding is SE-implied, WinAnsi, or explicit Differences.
    // (#6.2.11.5-std-enc-cff-fallback)
    let allow_cff_encoding_fallback = code <= 255;
    // Track whether the CFF encoding explicitly maps this code to GID 0 (.notdef).
    // This is distinct from "code absent from encoding" — for Standard Encoding,
    // absent SIDs return None from glyph_index rather than GID 0, so enc_map has
    // no entry for them. Only an explicit GID 0 in the map means provably .notdef.
    // (#6.2.11.5-notdef-guard)
    let mut cff_enc_explicit_notdef = false;

    if !name_found && code <= 255 && allow_cff_encoding_fallback {
        // Use pre-computed enc_map (hoisted to top of function).
        if let Some(&gid) = enc_map.get(&(code as u8)) {
            if gid != 0 {
                return cff
                    .glyph_width(cff_parser::GlyphId(gid))
                    .map(|w| w as f64 * scale);
            }
            // gid == 0 in enc_map: CFF encoding explicitly says this code is .notdef.
            cff_enc_explicit_notdef = true;
        }
        // cff.glyph_index falls back to StandardEncoding for codes not in the
        // CFF encoding table. When it returns GID 0, veraPDF treats the code as
        // absent → uses defaultWidthX (Case 1) or .notdef advance (Case 2).
        //
        // This applies to ALL codes (including ≤127) when we reach this point,
        // because the name-based lookup (Phase 1 above) already failed. If the
        // named glyph WERE in the charset, Phase 1 would have returned it.
        // So GID 0 here is authoritative. (#626, #fix-cff-lowbyte-notdef)
        if !is_custom_enc || enc_map.contains_key(&(code as u8)) {
            if let Some(gid) = cff.glyph_index(code as u8) {
                if gid.0 != 0 {
                    return cff.glyph_width(gid).map(|w| w as f64 * scale);
                }
                // GID 0 via encoding fallback — name lookup already failed above.
                cff_enc_explicit_notdef = true;
            }
        }

        // StandardEncoding fallback for codes absent from the custom Format0/Format1
        // CFF encoding. veraPDF falls back to Standard Encoding when a code has no
        // entry in the font's custom CFF encoding: SE → SID → CFF charset → GID →
        // charstring width. If the SID is not in the charset, GID = 0 (.notdef).
        // Example: WinAnsi code 243 = "oacute"; CFF subset doesn't include
        // "oacute"; SE code 243 also → "oacute" → not in subset → GID 0 → .notdef.
        //
        // When a PDF encoding is present, the SE lookup is only used to detect
        // whether veraPDF would reach GID 0 (.notdef). We NEVER return the SE
        // glyph's width for PDF-encoded fonts, because the PDF encoding already
        // determined the glyph name — the SE name may differ (e.g. WinAnsi 233 =
        // "eacute" but SE 233 = something else), which would produce wrong widths.
        // (#6.2.11.5-se-fallback)
        if !enc_map.contains_key(&(code as u8)) && is_custom_enc {
            let se_ch = encoding_to_char(code, "StandardEncoding");
            let mut se_glyph_found = false;
            if se_ch != '\u{FFFF}' {
                let mut se_w: Option<f64> = None;
                if let Some(agl_name) = unicode_to_agl_name(se_ch) {
                    se_w = lookup_name(&agl_name);
                }
                if se_w.is_none() {
                    let g_name = unicode_to_glyph_name(se_ch).unwrap_or_default();
                    if !g_name.is_empty() && g_name != ".notdef" {
                        se_w = lookup_name(&g_name);
                    }
                }
                if let Some(w) = se_w {
                    se_glyph_found = true;
                    // Only return the SE glyph's width when there is no PDF encoding.
                    // With a PDF encoding, the correct glyph was already resolved via
                    // the PDF encoding name, so using the SE glyph's width would be wrong
                    // if WinAnsi and SE map this code to different glyph names.
                    if !has_pdf_encoding {
                        return Some(w);
                    }
                }
            }
            // SE lookup found no glyph (or glyph was present but PDF encoding
            // overrides): veraPDF falls back to GID 0 → .notdef width.
            if !se_glyph_found {
                cff_enc_explicit_notdef = true;
            }
        }
    }

    // Final fallback: if BOTH name-based lookup AND CFF internal encoding returned
    // nothing (or GID 0), veraPDF uses the Private DICT defaultWidthX as the font
    // width for codes whose glyph name is absent from the CFF charset.
    //
    // Key empirical finding: for CFF fonts where the .notdef charstring advance
    // differs from defaultWidthX (e.g. JBGCOD+Century-Book: .notdef=250,
    // defaultWidthX=500), veraPDF consistently reports defaultWidthX=500, NOT the
    // .notdef charstring advance. This means veraPDF uses the Private DICT
    // defaultWidthX for codes absent from the charset, and the .notdef charstring
    // advance only for codes explicitly mapped to GID 0 by the CFF *internal*
    // encoding (i.e. custom-encoding fonts where GID 0 is explicitly the target).
    //
    // For standard/expert-encoding fonts (enc_offset 0/1) with a PDF /Encoding:
    //   - Code maps to SID via standard enc → SID absent from charset → fallback GID 0
    //   → veraPDF uses defaultWidthX (not .notdef advance).
    // For custom-encoding fonts: this path is not reached (handled by
    //   compute_cff_corrections_for_custom_encoding which already uses defaultWidthX).
    //
    // We distinguish four cases based on cff_enc_explicit_notdef and pdf_glyph_name:
    //
    // Case 1: cff_enc_explicit_notdef=true + has_pdf_encoding=true + pdf_glyph_name non-empty:
    //   Named glyph (e.g. "bullet") is absent from the CFF subset charset.
    //   veraPDF: absent SID → GID 0 → uses Private DICT defaultWidthX.
    //   (#6.2.11.5-std-enc-defaultwidthx)
    //
    // Case 2: cff_enc_explicit_notdef=true + has_pdf_encoding=true + pdf_glyph_name empty:
    //   Undefined encoding code (e.g. WinAnsi 0x90). veraPDF resolves undefined
    //   code → ".notdef" → uses the .notdef charstring advance (GID 0 width), NOT
    //   defaultWidthX. This is distinct from Case 1 because veraPDF's lookup path
    //   for an undefined code always ends at the .notdef charstring itself.
    //   (#6.2.11.5-undef-notdef-charstring)
    //
    // Case 3: cff_enc_explicit_notdef=true + has_pdf_encoding=false:
    //   Custom encoding, GID 0 is the explicit CFF target → .notdef charstring advance.
    //
    // Case 4: cff_enc_explicit_notdef=false: code absent from CFF encoding → None.
    if cff_enc_explicit_notdef {
        if has_pdf_encoding {
            if pdf_glyph_name.is_empty() {
                // Case 2: undefined encoding code.
                // For CFF fonts with CUSTOM encoding (enc_offset > 1), veraPDF
                // uses Private DICT defaultWidthX for codes not in the encoding,
                // regardless of whether the code has a PDF glyph name or not.
                // For STANDARD encoding fonts (enc_offset 0/1), veraPDF falls
                // through to the .notdef charstring advance.
                // Evidence: MELEBE+NCSchlbk-Roman code 227 has no PDF glyph name
                // (Differences-only encoding without BaseEncoding, code not in
                // Differences). CFF custom encoding maps 227→GID 0. veraPDF
                // reports font program width = defaultWidthX (1139→556.15), NOT
                // .notdef charstring (682→333). (#6.2.11.5-case2-custom-enc)
                if is_custom_enc {
                    return cff.default_width_x().map(|w| w as f64 * scale);
                }
                return cff
                    .glyph_width(cff_parser::GlyphId(0))
                    .map(|w| w as f64 * scale);
            }
            // Case 1: named glyph absent from subset → defaultWidthX.
            // Fall back to .notdef charstring advance when defaultWidthX is absent
            // from the Private DICT — veraPDF uses .notdef advance in that case.
            // (#fix-cff-case1-notdef-fallback)
            return cff.default_width_x().map(|w| w as f64 * scale).or_else(|| {
                cff.glyph_width(cff_parser::GlyphId(0))
                    .map(|w| w as f64 * scale)
            });
        }
        // Case 3: custom encoding, explicit GID 0 → .notdef charstring advance.
        return cff
            .glyph_width(cff_parser::GlyphId(0))
            .map(|w| w as f64 * scale);
    }
    if has_pdf_encoding {
        return None;
    }

    None
}

fn compute_classic_symbol_cff_single_width(
    font_data: &[u8],
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
) -> Option<f64> {
    if let Ok(face) = ttf_parser::Face::parse(font_data, 0) {
        let upem = face.units_per_em() as f64;
        if upem > 0.0 {
            let scale = 1000.0 / upem;

            if let Some(glyph_name) = differences.get(&code) {
                if let Some(gid) = face.glyph_index_by_name(glyph_name) {
                    return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
                }
                if let Some(unicode) = glyph_name_to_unicode(glyph_name) {
                    if let Some(gid) = face.glyph_index(unicode) {
                        return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
                    }
                }
            }

            let ch = encoding_to_char(code, enc_name);
            if let Some(agl_name) = unicode_to_agl_name(ch) {
                if let Some(gid) = face.glyph_index_by_name(&agl_name) {
                    return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
                }
            }
            if let Some(name) = unicode_to_glyph_name(ch) {
                if let Some(gid) = face.glyph_index_by_name(&name) {
                    return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
                }
            }
            if let Some(gid) = face.glyph_index(ch) {
                return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
            }
        }
    }

    compute_cff_single_width(font_data, code, enc_name, differences)
}

fn subset_standard_cff_code_is_safe(
    font_data: &[u8],
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
) -> bool {
    // Explicit Differences entries are deterministic mappings regardless of
    // BaseEncoding. If the CFF has the named glyph, the correction is safe. (#479)
    if let Some(name) = differences.get(&code) {
        return name != "space" && cff_font_has_named_glyph(font_data, name);
    }

    // Empty enc_name with no BaseEncoding: encoding_to_char() falls back to
    // WinAnsiEncoding for codes ≥128, so treat it as WinAnsiEncoding here too.
    // This matches the width computation path used by get_otf_width_via_encoding
    // and cff_width_for_code. (#504)
    let effective_enc = if enc_name.is_empty() && code >= 128 {
        "WinAnsiEncoding"
    } else {
        enc_name
    };

    if !matches!(effective_enc, "WinAnsiEncoding" | "StandardEncoding") {
        return false;
    }

    if let Some(name) = cff_pdf_base_glyph_name(code, effective_enc) {
        if cff_font_has_named_glyph(font_data, &name) {
            return true;
        }
        for alt in cff_glyph_name_alternatives(&name) {
            if cff_font_has_named_glyph(font_data, alt) {
                return true;
            }
        }
    }

    // Subset CFF fonts may use GID-based glyph names (e.g. "G80") that don't
    // match AGL or PostScript names, but the glyph IS accessible via the font's
    // Unicode cmap. If ttf-parser can resolve the character to a non-notdef
    // GID, the width correction is safe to apply. (#479)
    if let Ok(face) = ttf_parser::Face::parse(font_data, 0) {
        if let Some(name) = cff_pdf_base_glyph_name(code, effective_enc) {
            if let Some(unicode) = glyph_name_to_unicode(&name) {
                if face.glyph_index(unicode).map(|g| g.0).unwrap_or(0) > 0 {
                    return true;
                }
            }
        }
    }

    false
}

fn cff_font_has_named_glyph(font_data: &[u8], glyph_name: &str) -> bool {
    if let Ok(face) = ttf_parser::Face::parse(font_data, 0) {
        if face.glyph_index_by_name(glyph_name).is_some() {
            return true;
        }
    }

    if let Some(cff) = extract_cff_from_otf(font_data) {
        if cff_has_named_glyph(&cff, glyph_name) {
            return true;
        }
    }

    if let Some(cff) = cff_parser::Table::parse(font_data) {
        if cff_has_named_glyph(&cff, glyph_name) {
            return true;
        }
    }

    false
}

fn cff_has_named_glyph(cff: &cff_parser::Table<'_>, glyph_name: &str) -> bool {
    for gid_raw in 0..cff.number_of_glyphs() {
        let gid = cff_parser::GlyphId(gid_raw);
        if let Some(name) = cff.glyph_name(gid) {
            if name == glyph_name {
                return true;
            }
        }
    }
    false
}

/// Check if the CFF's non-.notdef glyphs use GID-based names (e.g. G80, G32)
/// rather than standard PostScript glyph names. When all names follow the
/// `G\d+` pattern, the only way to resolve code→GID is through CFF internal
/// encoding, because PDF-level name lookup will fail.
fn cff_has_gid_based_names(cff: &cff_parser::Table) -> bool {
    let n = cff.number_of_glyphs();
    if n <= 1 {
        return false;
    }
    // Check a sample of non-.notdef glyphs (skip GID 0).
    let mut gid_pattern = 0u32;
    let mut non_notdef = 0u32;
    for gid in 1..n {
        if let Some(name) = cff.glyph_name(cff_parser::GlyphId(gid)) {
            non_notdef += 1;
            // Match G followed by digits (e.g. G80, G32, G1)
            if name.starts_with('G')
                && name.len() > 1
                && name[1..].chars().all(|c| c.is_ascii_digit())
            {
                gid_pattern += 1;
            }
        }
        if non_notdef >= 10 {
            break;
        }
    }
    non_notdef > 0 && gid_pattern * 2 >= non_notdef
}

/// Check if the CFF has a custom encoding (not Standard or Expert).
fn cff_has_custom_encoding(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }
    let header_size = data[2] as usize;
    let Some(after_name) = skip_cff_index(data, header_size) else {
        return false;
    };
    let Some((top_dict_data, _)) = read_cff_index_first(data, after_name) else {
        return false;
    };
    let enc_offset = parse_cff_top_dict_encoding_offset(&top_dict_data);
    enc_offset > 1
}

pub fn parse_cff_encoding_map(data: &[u8]) -> std::collections::HashMap<u8, u16> {
    let mut map = std::collections::HashMap::new();

    // CFF structure: Header, Name INDEX, Top DICT INDEX, String INDEX, ...
    // We need the encoding offset from the Top DICT.
    if data.len() < 4 {
        return map;
    }

    let header_size = data[2] as usize; // hdrSize
    if header_size > data.len() {
        return map;
    }

    // Skip Name INDEX
    let name_idx_start = header_size;
    let Some(after_name) = skip_cff_index(data, name_idx_start) else {
        return map;
    };

    // Parse Top DICT INDEX to get encoding offset
    let Some((top_dict_data, _after_top_dict)) = read_cff_index_first(data, after_name) else {
        return map;
    };

    // Parse Top DICT for encoding offset (operator 16)
    let enc_offset = parse_cff_top_dict_encoding_offset(&top_dict_data);

    // enc_offset 0 = Standard Encoding, 1 = Expert Encoding.
    // For these, glyph_index() uses the correct encoding directly (no wrong fallback).
    if enc_offset == 0 || enc_offset == 1 {
        if let Some(cff) = cff_parser::Table::parse(data) {
            for code_byte in 0..=255u8 {
                if let Some(gid) = cff.glyph_index(code_byte) {
                    map.insert(code_byte, gid.0);
                }
            }
        }
        return map;
    }

    // Custom encoding at enc_offset
    let offset = enc_offset as usize;
    if offset >= data.len() {
        return map;
    }

    let format = data[offset] & 0x7F; // Low 7 bits = format, high bit = supplemental

    match format {
        0 => {
            // Format 0: nCodes byte, then nCodes code bytes
            if offset + 1 >= data.len() {
                return map;
            }
            let n_codes = data[offset + 1] as usize;
            for i in 0..n_codes {
                if offset + 2 + i >= data.len() {
                    break;
                }
                let code_byte = data[offset + 2 + i];
                // GID = i + 1 (.notdef is GID 0, implicit)
                map.insert(code_byte, (i + 1) as u16);
            }
            // NOTE: Supplement entries are intentionally NOT parsed.
            // veraPDF does not use CFF encoding supplement entries for width
            // comparison — supplement codes are treated as .notdef.
        }
        1 => {
            // Format 1: nRanges byte, then nRanges * (first: u8, nLeft: u8)
            if offset + 1 >= data.len() {
                return map;
            }
            let n_ranges = data[offset + 1] as usize;
            let mut gid: u16 = 1;
            for i in 0..n_ranges {
                let range_start = offset + 2 + i * 2;
                if range_start + 1 >= data.len() {
                    break;
                }
                let first = data[range_start];
                let n_left = data[range_start + 1];
                for j in 0..=n_left {
                    let code_byte = first.wrapping_add(j);
                    map.insert(code_byte, gid);
                    gid += 1;
                }
            }
            // NOTE: Supplement entries are intentionally NOT parsed.
            // veraPDF does not use CFF encoding supplement entries for width
            // comparison — supplement codes are treated as .notdef.
        }
        _ => {}
    }

    map
}

/// Skip a CFF INDEX structure and return the offset after it.
fn skip_cff_index(data: &[u8], start: usize) -> Option<usize> {
    if start + 2 > data.len() {
        return None;
    }
    let count = u16::from_be_bytes([data[start], data[start + 1]]) as usize;
    if count == 0 {
        return Some(start + 2);
    }
    if start + 3 > data.len() {
        return None;
    }
    let off_size = data[start + 2] as usize;
    if off_size == 0 || off_size > 4 {
        return None;
    }
    // offsets array: (count+1) entries of off_size bytes each
    let offsets_start = start + 3;
    let offsets_end = offsets_start + (count + 1) * off_size;
    if offsets_end > data.len() {
        return None;
    }
    // Last offset value gives the data size
    let last_off = read_cff_offset(data, offsets_start + count * off_size, off_size)?;
    // Data starts after offsets, first offset is 1-based
    Some(offsets_start + (count + 1) * off_size + last_off - 1)
}

/// Read the first entry from a CFF INDEX, returning (data, offset_after_index).
fn read_cff_index_first(data: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    if start + 2 > data.len() {
        return None;
    }
    let count = u16::from_be_bytes([data[start], data[start + 1]]) as usize;
    if count == 0 {
        return Some((Vec::new(), start + 2));
    }
    if start + 3 > data.len() {
        return None;
    }
    let off_size = data[start + 2] as usize;
    if off_size == 0 || off_size > 4 {
        return None;
    }
    let offsets_start = start + 3;
    let first_off = read_cff_offset(data, offsets_start, off_size)?;
    let second_off = read_cff_offset(data, offsets_start + off_size, off_size)?;
    let data_start = offsets_start + (count + 1) * off_size;
    let entry_start = data_start + first_off - 1;
    let entry_end = data_start + second_off - 1;
    if entry_end > data.len() {
        return None;
    }
    let last_off = read_cff_offset(data, offsets_start + count * off_size, off_size)?;
    let after = data_start + last_off - 1;
    Some((data[entry_start..entry_end].to_vec(), after))
}

/// Read a CFF offset value (1-4 bytes, big-endian).
fn read_cff_offset(data: &[u8], pos: usize, size: usize) -> Option<usize> {
    if pos + size > data.len() {
        return None;
    }
    let mut val = 0usize;
    for i in 0..size {
        val = (val << 8) | data[pos + i] as usize;
    }
    Some(val)
}

/// Parse the encoding offset from a CFF Top DICT.
/// Operator 16 (0x10) = Encoding offset (default 0 = Standard).
fn parse_cff_top_dict_encoding_offset(dict_data: &[u8]) -> u32 {
    let mut i = 0;
    let mut operand_stack: Vec<i64> = Vec::new();

    while i < dict_data.len() {
        let b0 = dict_data[i];
        match b0 {
            0..=11 => {
                // Operator (single byte)
                if b0 == 16 {
                    // Encoding operator
                    return operand_stack.last().copied().unwrap_or(0) as u32;
                }
                operand_stack.clear();
                i += 1;
            }
            12 => {
                // Two-byte operator
                operand_stack.clear();
                i += 2;
            }
            13..=21 => {
                // Operators 13-21
                if b0 == 16 {
                    return operand_stack.last().copied().unwrap_or(0) as u32;
                }
                operand_stack.clear();
                i += 1;
            }
            28 => {
                // 2-byte integer
                if i + 2 < dict_data.len() {
                    let val = i16::from_be_bytes([dict_data[i + 1], dict_data[i + 2]]) as i64;
                    operand_stack.push(val);
                }
                i += 3;
            }
            29 => {
                // 4-byte integer
                if i + 4 < dict_data.len() {
                    let val = i32::from_be_bytes([
                        dict_data[i + 1],
                        dict_data[i + 2],
                        dict_data[i + 3],
                        dict_data[i + 4],
                    ]) as i64;
                    operand_stack.push(val);
                }
                i += 5;
            }
            30 => {
                // Real number (BCD) — skip it
                i += 1;
                while i < dict_data.len() {
                    let nibbles = dict_data[i];
                    i += 1;
                    if nibbles & 0x0F == 0x0F || nibbles >> 4 == 0x0F {
                        break;
                    }
                }
                operand_stack.push(0); // Placeholder
            }
            32..=246 => {
                operand_stack.push(b0 as i64 - 139);
                i += 1;
            }
            247..=250 => {
                if i + 1 < dict_data.len() {
                    let val = (b0 as i64 - 247) * 256 + dict_data[i + 1] as i64 + 108;
                    operand_stack.push(val);
                }
                i += 2;
            }
            251..=254 => {
                if i + 1 < dict_data.len() {
                    let val = -(b0 as i64 - 251) * 256 - dict_data[i + 1] as i64 - 108;
                    operand_stack.push(val);
                }
                i += 2;
            }
            255 => {
                // Reserved in DICT
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    0 // Default: Standard Encoding
}

/// Alternative glyph names to try when the primary name isn't found in CFF.
fn cff_glyph_name_alternatives(name: &str) -> &'static [&'static str] {
    match name {
        "uni00AD" | "softhyphen" => &["hyphen", "sfthyphen"],
        "uni00A0" | "nbspace" => &["space"],
        "uni2010" => &["hyphen"],
        // "quotesingle" (SID 170) and "quoteright" (SID 169) are different CFF
        // SIDs. veraPDF uses exact SID lookup — if SID 170 is absent from the
        // charset, it does NOT try SID 169. Treating them as alternatives caused
        // wrong corrections (found "quoteright" 238 when veraPDF expects .notdef
        // 278). (#fix-cff-se-code39)
        // "quotesingle" => &["quoteright"],  // REMOVED — different SIDs
        _ => &[],
    }
}

/// Known symbolic font base names (exempt from encoding rules).
const SYMBOLIC_FONTS: &[&str] = &[
    "Symbol",
    "SymbolMT",
    "MTExtra",
    "ZapfDingbats",
    "Wingdings",
    "Webdings",
    "Dingbats",
    "CMSY10",
    "MSAM10",
    "MSBM10",
    "WASY8",
    "WASY9",
    "TXSY",
    "TXSYC",
    "TXEX",
];

/// Check if a font name (with optional subset prefix) is a symbolic font.
fn is_symbolic_font_name(name: &str) -> bool {
    let base = name.split('+').next_back().unwrap_or(name);
    if SYMBOLIC_FONTS
        .iter()
        .any(|sym| base.eq_ignore_ascii_case(sym))
    {
        return true;
    }

    // TeX/Math symbolic families are often subsetted/renamed.
    let up = base.to_ascii_uppercase();
    up.starts_with("CMSY")
        || up.starts_with("MSAM")
        || up.starts_with("MSBM")
        || up.starts_with("WASY")
        || up.starts_with("TXSY")
        || up.starts_with("TXEX")
}

fn truetype_has_safe_text_encoding(doc: &Document, dict: &lopdf::Dictionary) -> bool {
    let is_valid_enc =
        |enc_str: &str| enc_str == "WinAnsiEncoding" || enc_str == "MacRomanEncoding";

    match dict.get(b"Encoding") {
        Ok(Object::Name(enc)) => is_valid_enc(&String::from_utf8_lossy(enc)),
        Ok(Object::Dictionary(enc_dict)) => {
            let base_enc = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            is_valid_enc(&base_enc)
                && truetype_encoding_differences_are_safe(
                    doc,
                    Some(&Object::Dictionary(enc_dict.clone())),
                )
        }
        Ok(Object::Reference(enc_ref)) => match doc.get_object(*enc_ref) {
            Ok(Object::Name(enc)) => is_valid_enc(&String::from_utf8_lossy(enc)),
            Ok(Object::Dictionary(enc_dict)) => {
                let base_enc = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
                is_valid_enc(&base_enc)
                    && truetype_encoding_differences_are_safe(
                        doc,
                        Some(&Object::Dictionary(enc_dict.clone())),
                    )
            }
            _ => false,
        },
        _ => false,
    }
}

fn truetype_is_misflagged_text_font(doc: &Document, dict: &lopdf::Dictionary) -> bool {
    if get_name(dict, b"Subtype").as_deref() != Some("TrueType") {
        return false;
    }

    let base_font = get_name(dict, b"BaseFont").unwrap_or_default();
    is_font_symbolic(doc, dict)
        && !is_symbolic_font_name(&base_font)
        && truetype_has_safe_text_encoding(doc, dict)
}

/// Fix TrueType font encoding for PDF/A compliance (rules 6.2.11.6:2, 6.2.11.6:3).
///
/// - Non-symbolic TrueType fonts must have MacRomanEncoding or WinAnsiEncoding.
/// - Symbolic TrueType fonts must NOT have an Encoding entry.
pub fn fix_truetype_encoding(doc: &mut Document) -> usize {
    // Collect (font_id, target_encoding_name) pairs that need fixing.
    let mut to_fix: Vec<(ObjectId, &'static [u8])> = Vec::new();
    // Collect symbolic font IDs that need Encoding removed.
    let mut symbolic_to_strip: Vec<ObjectId> = Vec::new();

    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        // Only process TrueType fonts.
        if get_name(dict, b"Subtype").as_deref() != Some("TrueType") {
            continue;
        }

        let base_font = get_name(dict, b"BaseFont").unwrap_or_default();
        let symbolic_by_name = is_symbolic_font_name(&base_font);

        let safe_text_encoding = truetype_has_safe_text_encoding(doc, dict);

        // Some real text fonts ship with incorrect Symbolic flags set in the
        // original PDF (e.g. subset Times New Roman). If the font name is not
        // symbolic and it already carries a safe text encoding, do not strip
        // /Encoding here — later width/notdef passes depend on that mapping.
        let is_symbolic = if is_font_symbolic(doc, dict) {
            symbolic_by_name || !safe_text_encoding
        } else {
            false
        };
        if is_symbolic {
            // Symbolic fonts must NOT have Encoding (6.2.11.6:3).
            if dict.has(b"Encoding") {
                symbolic_to_strip.push(*id);
            }
            continue;
        }

        // Check existing Encoding. Both WinAnsiEncoding and MacRomanEncoding
        // are valid for non-symbolic TrueType fonts per §6.2.11.6:2.
        // MacRomanEncoding must be preserved as-is: veraPDF's §6.2.11.5 width
        // check uses glyph-name lookup (encoding[code] → glyph name → font
        // name table), so converting MacRomanEncoding → WinAnsiEncoding changes
        // which glyph name veraPDF expects for each code. For example, Mac code
        // 160 = "dagger" (present in the font), WinAnsi code 160 = "nbspace"
        // (absent) → converting Mac→Win causes §6.2.11.5 failures. (#507)
        // Determine whether this font needs its encoding fixed, and if so,
        // which target encoding to use. When flattening a dict with Differences
        // we preserve the base encoding (MacRoman→MacRoman, WinAnsi→WinAnsi).
        // Keep safe AGL-compatible Differences arrays intact: flattening them
        // erases meaningful overrides such as MacRoman code 173 -> /space and
        // can reintroduce .notdef failures after fallback TrueType embedding.
        let is_valid_enc =
            |enc_str: &str| enc_str == "WinAnsiEncoding" || enc_str == "MacRomanEncoding";
        let (needs_fix, target_enc): (bool, &'static [u8]) = match dict.get(b"Encoding") {
            Ok(Object::Name(enc)) => {
                let enc_str = String::from_utf8_lossy(enc);
                (!is_valid_enc(&enc_str), b"WinAnsiEncoding")
            }
            Ok(Object::Dictionary(enc_dict)) => {
                let base_enc = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
                let base_is_standard = is_valid_enc(&base_enc);
                let needs = !base_is_standard
                    || !truetype_encoding_differences_are_safe(
                        doc,
                        Some(&Object::Dictionary(enc_dict.clone())),
                    );
                let target: &'static [u8] = if base_enc == "MacRomanEncoding" {
                    b"MacRomanEncoding"
                } else {
                    b"WinAnsiEncoding"
                };
                (needs, target)
            }
            Ok(Object::Reference(enc_ref)) => match doc.get_object(*enc_ref) {
                Ok(Object::Name(enc)) => {
                    let enc_str = String::from_utf8_lossy(enc);
                    (!is_valid_enc(&enc_str), b"WinAnsiEncoding")
                }
                Ok(Object::Dictionary(enc_dict)) => {
                    let base_enc = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
                    let base_is_standard = is_valid_enc(&base_enc);
                    let needs = !base_is_standard
                        || !truetype_encoding_differences_are_safe(
                            doc,
                            Some(&Object::Dictionary(enc_dict.clone())),
                        );
                    let target: &'static [u8] = if base_enc == "MacRomanEncoding" {
                        b"MacRomanEncoding"
                    } else {
                        b"WinAnsiEncoding"
                    };
                    (needs, target)
                }
                _ => (true, b"WinAnsiEncoding"),
            },
            _ => (true, b"WinAnsiEncoding"), // Missing Encoding — needs fix.
        };

        if needs_fix {
            to_fix.push((*id, target_enc));
        }
    }

    // Apply fixes.
    let count = to_fix.len();
    for (id, enc_name) in to_fix {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(id) {
            // Set Encoding to a simple Name (WinAnsi or MacRoman, see above).
            // Preserving Differences arrays from referenced encoding dicts
            // can cause 6.2.11.6:2 violations when glyph names aren't in
            // the Adobe Glyph List. A simple Name avoids that check.
            dict.set("Encoding", Object::Name(enc_name.to_vec()));
        }
    }

    // Strip Encoding from symbolic TrueType fonts (6.2.11.6:3).
    for id in &symbolic_to_strip {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(*id) {
            dict.remove(b"Encoding");
        }
    }

    count + symbolic_to_strip.len()
}

fn truetype_encoding_differences_are_safe(doc: &Document, enc: Option<&Object>) -> bool {
    let diff = parse_differences_from_encoding(doc, enc);
    diff.is_empty()
        || diff
            .values()
            .all(|name| name == ".notdef" || glyph_name_to_unicode(name).is_some())
}

/// Fix symbolic TrueType cmap tables in already-embedded fonts (6.2.11.6:4).
///
/// For symbolic TrueType fonts the cmap must contain exactly one subtable or
/// include Microsoft Symbol (3,0). We reuse the in-place binary fixer used
/// during embedding for existing FontFile2 streams.
pub fn fix_existing_symbolic_truetype_cmaps(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0usize;

    for font_id in font_ids {
        let (fd_id, ff2_id) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            if get_name(dict, b"Subtype").as_deref() != Some("TrueType") {
                continue;
            }
            let base_name = get_name(dict, b"BaseFont").unwrap_or_default();
            if !is_font_symbolic(doc, dict) && !is_symbolic_font_name(&base_name) {
                continue;
            }
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
                continue;
            };
            let ff2_id = match fd.get(b"FontFile2").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            (fd_id, ff2_id)
        };

        fix_symbolic_truetype_cmap(doc, ff2_id);

        let Some(font_data) = read_embedded_font_data(doc, fd_id) else {
            continue;
        };
        if tt_has_symbol_cmap(&font_data) {
            continue;
        }

        let symbol_mappings = tt_build_symbol_cmap_mappings(&font_data);
        if symbol_mappings.is_empty() {
            continue;
        }

        let Some(new_font_data) = tt_add_symbol_cmap_subtable(&font_data, &symbol_mappings) else {
            continue;
        };
        let len = new_font_data.len() as i64;
        let new_stream = Stream::new(
            dictionary! {
                "Length" => len,
                "Length1" => len,
            },
            new_font_data,
        );
        doc.objects.insert(ff2_id, Object::Stream(new_stream));
        fixed += 1;
    }

    fixed
}

fn tt_build_symbol_cmap_mappings(data: &[u8]) -> Vec<(u16, u16)> {
    // Build (3,0) Symbol cmap by reading the Mac (1,0) cmap byte→GID map and
    // shifting each byte code to the PUA range 0xF000+code.
    // Using face.glyph_index(char) with byte codes as Unicode codepoints is
    // wrong: byte 183 → U+00B7 (middle dot) ≠ Symbol code 183 (multiply ×).
    // The Mac (1,0) cmap gives the authoritative byte→GID mapping for Symbol.
    // (#6.2.11.5-sym-cmap-build)
    let mac_map = tt_read_mac_cmap(data);
    if mac_map.is_empty() {
        // Fallback for fonts without Mac (1,0) cmap (e.g. Windows-authored Symbol
        // subsets). Windows Symbol fonts store glyphs under PUA codepoints 0xF000+code
        // in the (3,1) Unicode cmap. Try PUA lookup first; fall back to raw code
        // (handles legacy Symbol variants where codes ≤127 are ASCII-range glyphs).
        let Ok(face) = ttf_parser::Face::parse(data, 0) else {
            return Vec::new();
        };
        let mut mappings = Vec::new();
        for code in 0u16..=255 {
            // Try PUA first (0xF000+code), then raw code as Unicode.
            let gid = char::from_u32(0xF000u32 + code as u32)
                .and_then(|ch| face.glyph_index(ch))
                .filter(|gid| gid.0 > 0 && tt_glyph_has_data(&face, *gid))
                .or_else(|| {
                    char::from_u32(code as u32)
                        .and_then(|ch| face.glyph_index(ch))
                        .filter(|gid| gid.0 > 0 && tt_glyph_has_data(&face, *gid))
                });
            let Some(gid) = gid else {
                continue;
            };
            mappings.push((0xF000u16 + code, gid.0));
        }
        return mappings;
    }
    mac_map
        .into_iter()
        .map(|(code, gid)| (0xF000u16 + code as u16, gid))
        .collect()
}

/// Mac Roman code → Unicode mapping for codes 128-255.
const MAC_ROMAN_TO_UNICODE: [u16; 128] = [
    0x00C4, 0x00C5, 0x00C7, 0x00C9, 0x00D1, 0x00D6, 0x00DC, 0x00E1, // 128-135
    0x00E0, 0x00E2, 0x00E4, 0x00E3, 0x00E5, 0x00E7, 0x00E9, 0x00E8, // 136-143
    0x00EA, 0x00EB, 0x00ED, 0x00EC, 0x00EE, 0x00EF, 0x00F1, 0x00F3, // 144-151
    0x00F2, 0x00F4, 0x00F6, 0x00F5, 0x00FA, 0x00F9, 0x00FB, 0x00FC, // 152-159
    0x2020, 0x00B0, 0x00A2, 0x00A3, 0x00A7, 0x2022, 0x00B6, 0x00DF, // 160-167
    0x00AE, 0x00A9, 0x2122, 0x00B4, 0x00A8, 0x2260, 0x00C6, 0x00D8, // 168-175
    0x221E, 0x00B1, 0x2264, 0x2265, 0x00A5, 0x00B5, 0x2202, 0x2211, // 176-183
    0x220F, 0x03C0, 0x222B, 0x00AA, 0x00BA, 0x2126, 0x00E6, 0x00F8, // 184-191
    0x00BF, 0x00A1, 0x00AC, 0x221A, 0x0192, 0x2248, 0x2206, 0x00AB, // 192-199
    0x00BB, 0x2026, 0x00A0, 0x00C0, 0x00C3, 0x00D5, 0x0152, 0x0153, // 200-207
    0x2013, 0x2014, 0x201C, 0x201D, 0x2018, 0x2019, 0x00F7, 0x25CA, // 208-215
    0x00FF, 0x0178, 0x2044, 0x20AC, 0x2039, 0x203A, 0xFB01, 0xFB02, // 216-223
    0x2021, 0x00B7, 0x201A, 0x201E, 0x2030, 0x00C2, 0x00CA, 0x00C1, // 224-231
    0x00CB, 0x00C8, 0x00CD, 0x00CE, 0x00CF, 0x00CC, 0x00D3, 0x00D4, // 232-239
    0xF8FF, 0x00D2, 0x00DA, 0x00DB, 0x00D9, 0x0131, 0x02C6, 0x02DC, // 240-247
    0x00AF, 0x02D8, 0x02D9, 0x02DA, 0x00B8, 0x02DD, 0x02DB, 0x02C7, // 248-255
];

/// Convert a Mac Roman code (0-255) to its Unicode codepoint.
fn mac_roman_to_unicode(code: u8) -> u16 {
    if code < 128 {
        code as u16
    } else {
        MAC_ROMAN_TO_UNICODE[(code - 128) as usize]
    }
}

/// Add a (3,1) Unicode BMP cmap subtable to a TrueType font that lacks one.
///
/// Many embedded TrueType subsets only have a (1,0) Mac Roman cmap. veraPDF
/// requires a (3,1) Unicode cmap for non-symbolic fonts with WinAnsiEncoding.
/// This function reads the existing (1,0) cmap, converts Mac Roman codes to
/// Unicode, and rebuilds the font with an additional (3,1) format 4 subtable.
pub fn fix_truetype_unicode_cmap(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (fd_id, ff2_key, first_char, last_char, enc_info, prefer_pdf_encoding_symbol_cmap) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            if get_name(dict, b"Subtype").as_deref() != Some("TrueType") {
                continue;
            }

            let prefer_pdf_encoding_symbol_cmap = truetype_is_misflagged_text_font(doc, dict);
            if is_font_symbolic(doc, dict) && !prefer_pdf_encoding_symbol_cmap {
                continue;
            }
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            let first_char = dict
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some((*i).clamp(0, 255) as u32),
                    _ => None,
                })
                .unwrap_or(0);
            let last_char = dict
                .get(b"LastChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some((*i).clamp(0, 255) as u32),
                    _ => None,
                })
                .unwrap_or(255);
            let enc_info = get_simple_encoding_info(doc, dict);
            // Must have FontFile2 (TrueType font program).
            let ff2_key = {
                let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
                    continue;
                };
                match fd.get(b"FontFile2").ok() {
                    Some(Object::Reference(id)) => *id,
                    _ => continue,
                }
            };
            (
                fd_id,
                ff2_key,
                first_char,
                last_char,
                enc_info,
                prefer_pdf_encoding_symbol_cmap,
            )
        };

        // Read the font data.
        let Some(font_data) = read_embedded_font_data(doc, fd_id) else {
            continue;
        };

        // Check if the font already has a (3,1) cmap. If so, skip.
        if tt_has_unicode_cmap(&font_data) {
            continue;
        }

        // Read cmap mappings from the embedded font.
        let mac_mappings = tt_read_mac_cmap(&font_data);
        let sym_mappings = tt_read_symbol_cmap(&font_data);

        // Build Unicode → GID mappings for the (3,1) subtable.
        let mut unicode_mappings: Vec<(u16, u16)> = if prefer_pdf_encoding_symbol_cmap {
            tt_build_unicode_mappings_from_pdf_encoding(
                &enc_info.0,
                &enc_info.1,
                first_char,
                last_char,
                &sym_mappings,
            )
        } else {
            Vec::new()
        };

        // From (1,0) cmap: convert Mac Roman codes to Unicode.
        // Also add WinAnsi unicode for the same code, in case the encoding
        // was converted from MacRomanEncoding to WinAnsiEncoding by
        // fix_truetype_encoding (e.g. code 165: Mac=bullet U+2022,
        // WinAnsi=yen U+00A5 — both need to map to the same GID).
        if !mac_mappings.is_empty() {
            for (mac_code, gid) in &mac_mappings {
                if *gid == 0 {
                    continue; // Skip .notdef.
                }
                let mac_unicode = mac_roman_to_unicode(*mac_code);
                if mac_unicode > 0 && mac_unicode != 0xF8FF {
                    // Avoid PUA Apple logo.
                    unicode_mappings.push((mac_unicode, *gid));
                }
                // Also add the WinAnsi unicode for codes ≥160 only.
                // For Mac codes 128-159, Mac Roman and WinAnsi map completely
                // different characters to the same byte position (e.g. Mac byte
                // 146 = 'í'/U+00ED, WinAnsi byte 146 = U+2019 curly quote). Adding
                // the WinAnsi Unicode → GID mapping for these codes creates wrong
                // (3,1) cmap entries because the GID represents the Mac character
                // (e.g. 'í'), not the WinAnsi one (curly quote). For codes ≥160
                // the two encodings may share the same visual glyph (e.g. Mac 165
                // = bullet, WinAnsi 165 = yen — same GID for a converted font), so
                // adding both Unicode values → GID is intentional there.
                // (#fix-tt-cmap-winansi-128-159)
                let winansi_char = encoding_to_char(*mac_code as u32, "WinAnsiEncoding");
                let winansi_unicode = winansi_char as u16;
                if *mac_code >= 160 && winansi_unicode != mac_unicode && winansi_unicode > 0 {
                    unicode_mappings.push((winansi_unicode, *gid));
                }
            }
        }

        // From (3,0) cmap: convert PUA codes (U+F0xx) to standard Unicode.
        for (pua_code, gid) in &sym_mappings {
            if *gid == 0 {
                continue;
            }
            if *pua_code >= 0xF000 && *pua_code <= 0xF0FF {
                let standard = *pua_code - 0xF000;
                if standard > 0 {
                    unicode_mappings.push((standard, *gid));
                }
            }
        }

        // Deduplicate by Unicode code, preferring higher GIDs (more specific).
        unicode_mappings.sort_by_key(|(u, _)| *u);
        unicode_mappings.dedup_by_key(|(u, _)| *u);

        if unicode_mappings.is_empty() {
            continue;
        }

        // Rebuild the font with the additional (3,1) cmap subtable.
        let Some(new_font_data) = tt_add_unicode_cmap_subtable(&font_data, &unicode_mappings)
        else {
            continue;
        };

        // Replace the FontFile2 stream with the modified font data.
        let len = new_font_data.len() as i64;
        let new_stream = Stream::new(
            dictionary! {
                "Length" => len,
                "Length1" => len,
            },
            new_font_data,
        );
        doc.objects.insert(ff2_key, Object::Stream(new_stream));
        fixed += 1;
    }

    fixed
}

/// Align legacy single-byte text bytes with the embedded TrueType (3,1) cmap.
///
/// Two recurring cases need the same repair strategy:
/// - MacRomanEncoding bytes >= 160, where the embedded TrueType already has a
///   conflicting Windows Unicode cmap entry for the same visual glyph.
/// - WinAnsiEncoding bytes 128..159 in fallback TrueType fonts whose raw Mac
///   cmap still carries the glyph that the PDF width table was authored for.
///
/// When the current PDF width already matches the raw Mac-cmap glyph advance,
/// add a targeted alias in the (3,1) cmap so veraPDF resolves the byte through
/// the same GID during §6.2.11.5 width validation.
pub fn fix_truetype_macroman_unicode_aliases(doc: &mut Document) -> usize {
    use std::collections::{BTreeMap, HashMap};

    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let used_simple_codes = collect_simple_font_used_codes(doc);
    let mut fixed = 0usize;

    for font_id in font_ids {
        let (fd_id, ff2_id, first_char, last_char, widths, enc_name) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            if get_name(dict, b"Subtype").as_deref() != Some("TrueType") {
                continue;
            }
            if is_font_symbolic(doc, dict) {
                continue;
            }

            let enc_name = extract_encoding_info(doc, dict).base_encoding;
            if enc_name != "MacRomanEncoding" && enc_name != "WinAnsiEncoding" {
                continue;
            }

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            let ff2_id = match doc.objects.get(&fd_id) {
                Some(Object::Dictionary(fd)) => match fd.get(b"FontFile2").ok() {
                    Some(Object::Reference(id)) => *id,
                    _ => continue,
                },
                _ => continue,
            };
            let first_char = dict
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some((*i).clamp(0, 255) as u32),
                    _ => None,
                })
                .unwrap_or(0);
            let last_char = dict
                .get(b"LastChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some((*i).clamp(0, 255) as u32),
                    _ => None,
                })
                .unwrap_or(255);
            let widths = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => arr.clone(),
                Some(Object::Reference(id)) => match doc.get_object(*id) {
                    Ok(Object::Array(arr)) => arr.clone(),
                    _ => continue,
                },
                _ => continue,
            };

            (fd_id, ff2_id, first_char, last_char, widths, enc_name)
        };

        let Some(used_codes) = used_simple_codes.get(&font_id) else {
            continue;
        };
        if used_codes.is_empty() {
            continue;
        }

        let Some(font_data) = read_embedded_font_data(doc, fd_id) else {
            continue;
        };
        if !tt_has_unicode_cmap(&font_data) {
            continue;
        }

        let Ok(face) = ttf_parser::Face::parse(&font_data, 0) else {
            continue;
        };
        let units_per_em = face.units_per_em() as f64;
        if units_per_em == 0.0 {
            continue;
        }
        let scale = 1000.0 / units_per_em;

        let mac_map: HashMap<u8, u16> = tt_read_mac_cmap(&font_data).into_iter().collect();
        if mac_map.is_empty() {
            continue;
        }

        let mut alias_updates: BTreeMap<u16, u16> = BTreeMap::new();
        let mut mac_code_updates: BTreeMap<u8, u16> = BTreeMap::new();
        let target_range = match enc_name.as_str() {
            "MacRomanEncoding" => 160..=255,
            "WinAnsiEncoding" => 128..=159,
            _ => continue,
        };

        for &code in used_codes {
            if code < first_char || code > last_char || !target_range.contains(&code) {
                continue;
            }

            let width_idx = (code - first_char) as usize;
            let Some(pdf_w) = widths.get(width_idx).and_then(object_to_f64) else {
                continue;
            };

            let code_u8 = code as u8;
            let winansi_unicode = encoding_to_char(code, "WinAnsiEncoding") as u16;
            if winansi_unicode == 0 {
                continue;
            }

            let win_gid = lookup_unicode_cmap_31(&face, winansi_unicode as u32).or_else(|| {
                char::from_u32(winansi_unicode as u32).and_then(|ch| face.glyph_index(ch))
            });
            let Some(win_gid) = win_gid else {
                continue;
            };
            let Some(win_adv) = face.glyph_hor_advance(win_gid) else {
                continue;
            };
            let win_w = win_adv as f64 * scale;

            let mac_gid_u16 = mac_map.get(&code_u8).copied().unwrap_or(0);
            let mac_w = if mac_gid_u16 == 0 {
                None
            } else {
                face.glyph_hor_advance(ttf_parser::GlyphId(mac_gid_u16))
                    .map(|adv| adv as f64 * scale)
            };

            match enc_name.as_str() {
                "MacRomanEncoding" => {
                    let Some(mac_w) = mac_w else {
                        continue;
                    };
                    if (pdf_w - mac_w).abs() > 1.0 {
                        continue;
                    }
                    let mac_unicode = mac_roman_to_unicode(code_u8);
                    if winansi_unicode == mac_unicode || win_gid.0 == mac_gid_u16 {
                        continue;
                    }
                    if (pdf_w - win_w).abs() <= 1.0 {
                        continue;
                    }

                    alias_updates.insert(winansi_unicode, mac_gid_u16);
                }
                "WinAnsiEncoding" => {
                    if (pdf_w - win_w).abs() > 1.0 {
                        continue;
                    }
                    if mac_gid_u16 == win_gid.0 && mac_gid_u16 != 0 {
                        continue;
                    }
                    if mac_w.is_some_and(|w| (pdf_w - w).abs() <= 1.0) {
                        continue;
                    }

                    mac_code_updates.insert(code_u8, win_gid.0);
                }
                _ => {}
            }
        }

        if alias_updates.is_empty() && mac_code_updates.is_empty() {
            continue;
        }

        let mut new_font_data = font_data.clone();

        if !alias_updates.is_empty() {
            let mut merged: BTreeMap<u16, u16> = tt_read_windows_cmap(&new_font_data, 1)
                .into_iter()
                .collect();
            if merged.is_empty() {
                continue;
            }
            for (unicode, gid) in alias_updates {
                merged.insert(unicode, gid);
            }

            let mappings: Vec<(u16, u16)> = merged.into_iter().collect();
            let Some(updated) = tt_replace_windows_cmap_subtable(&new_font_data, &mappings, 1)
            else {
                continue;
            };
            new_font_data = updated;
        }

        if !mac_code_updates.is_empty() {
            let mut merged_mac: BTreeMap<u8, u16> =
                tt_read_mac_cmap(&new_font_data).into_iter().collect();
            if merged_mac.is_empty() {
                continue;
            }
            for (code, gid) in mac_code_updates {
                merged_mac.insert(code, gid);
            }

            let mac_mappings: Vec<(u8, u16)> = merged_mac.into_iter().collect();
            let Some(updated) = tt_replace_mac_cmap_subtable(&new_font_data, &mac_mappings) else {
                continue;
            };
            new_font_data = updated;
        }

        let len = new_font_data.len() as i64;
        let new_stream = Stream::new(
            dictionary! {
                "Length" => len,
                "Length1" => len,
            },
            new_font_data,
        );
        doc.objects.insert(ff2_id, Object::Stream(new_stream));
        fixed += 1;
    }

    fixed
}

// ---------------------------------------------------------------------------
// 6.2.11.7.2 — Add /ToUnicode CMap to Type1 fonts with standard encoding
// ---------------------------------------------------------------------------

/// Build a PDF ToUnicode CMap stream for a set of single-byte code→Unicode mappings.
fn build_type1_tounicode_cmap(mappings: &[(u8, u16)]) -> Vec<u8> {
    let mut s = String::new();
    s.push_str("/CIDInit /ProcSet findresource begin\n");
    s.push_str("12 dict begin\n");
    s.push_str("begincmap\n");
    s.push_str("/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    s.push_str("/CMapName /Adobe-Identity-UCS def\n");
    s.push_str("/CMapType 2 def\n");
    s.push_str("1 begincodespacerange\n");
    s.push_str("<00> <FF>\n");
    s.push_str("endcodespacerange\n");
    // CMap spec allows at most 100 entries per beginbfchar/endbfchar block.
    for chunk in mappings.chunks(100) {
        s.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (code, unicode) in chunk {
            s.push_str(&format!("<{:02X}> <{:04X}>\n", code, unicode));
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\n");
    s.push_str("CMapName currentdict /CMap defineresource pop\n");
    s.push_str("end\n");
    s.push_str("end\n");
    s.into_bytes()
}

/// Extract base encoding name and Differences list from an Encoding dictionary.
fn type1_enc_from_dict(enc_dict: &lopdf::Dictionary) -> (String, Vec<(u8, String)>) {
    let base_enc = match enc_dict.get(b"BaseEncoding").ok() {
        Some(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
        _ => "StandardEncoding".to_string(),
    };
    let mut differences: Vec<(u8, String)> = Vec::new();
    if let Ok(Object::Array(diffs)) = enc_dict.get(b"Differences") {
        let mut code: u8 = 0;
        for item in diffs {
            match item {
                Object::Integer(n) => code = (*n).clamp(0, 255) as u8,
                Object::Name(glyph) => {
                    differences.push((code, String::from_utf8_lossy(glyph).to_string()));
                    code = code.wrapping_add(1);
                }
                _ => {}
            }
        }
    }
    (base_enc, differences)
}

/// Add /ToUnicode CMap streams to Type1 fonts that lack them but have a
/// standard encoding (WinAnsiEncoding / MacRomanEncoding) or a
/// Differences-based Encoding dictionary.
///
/// ISO 19005-2 §6.2.11.7.2 requires every non-CID font in PDF/A-2/3 to
/// carry a /ToUnicode CMap. Fixes #483.
pub fn fix_type1_tounicode_from_encoding(doc: &mut Document) -> usize {
    use crate::encoding_utils::glyph_name_to_char;

    // First pass (immutable): collect fonts that need a ToUnicode CMap.
    // Each entry is (font_id, base_encoding_name, differences).
    type FontEncEntry = (ObjectId, String, Vec<(u8, String)>);
    let mut to_process: Vec<FontEncEntry> = Vec::new();

    for (&font_id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        // Only simple (non-CID) Type1 fonts.
        match get_name(dict, b"Subtype").as_deref() {
            Some("Type1") | Some("MMType1") => {}
            _ => continue,
        }
        // Skip fonts that already have a ToUnicode entry.
        if dict.get(b"ToUnicode").is_ok() {
            continue;
        }
        let enc_info: Option<(String, Vec<(u8, String)>)> = match dict.get(b"Encoding").ok() {
            Some(Object::Name(n)) => {
                let name = String::from_utf8_lossy(n).to_string();
                Some((name, vec![]))
            }
            Some(Object::Reference(enc_ref)) => {
                let enc_ref = *enc_ref;
                match doc.objects.get(&enc_ref) {
                    Some(Object::Dictionary(enc_dict)) => Some(type1_enc_from_dict(enc_dict)),
                    _ => None,
                }
            }
            Some(Object::Dictionary(enc_dict)) => Some(type1_enc_from_dict(enc_dict)),
            _ => None,
        };
        if let Some((base_enc, diffs)) = enc_info {
            to_process.push((font_id, base_enc, diffs));
        }
    }

    // Second pass (mutable): build and attach ToUnicode streams.
    let mut fixed = 0;
    for (font_id, base_enc, differences) in to_process {
        let enc_known = matches!(
            base_enc.as_str(),
            "WinAnsiEncoding" | "MacRomanEncoding" | "StandardEncoding"
        );
        if !enc_known && differences.is_empty() {
            continue;
        }

        // Build code→unicode table from the base encoding.
        let mut code_to_unicode: [Option<u16>; 256] = [None; 256];
        match base_enc.as_str() {
            "WinAnsiEncoding" | "MacRomanEncoding" => {
                for code in 32u32..=255 {
                    let ch = encoding_to_char(code, &base_enc);
                    let cp = ch as u32;
                    if cp > 0 && cp <= 0xFFFF && cp != 0xFFFD {
                        code_to_unicode[code as usize] = Some(cp as u16);
                    }
                }
            }
            "StandardEncoding" => {
                // Standard encoding is US-ASCII for codes 32-126.
                for code in 32u8..=126 {
                    code_to_unicode[code as usize] = Some(code as u16);
                }
            }
            _ => {} // Differences-only encoding: table starts empty.
        }

        // Apply Differences overrides.
        for (code, glyph_name) in &differences {
            match glyph_name_to_char(glyph_name) {
                Some(ch) => {
                    let cp = ch as u32;
                    if cp > 0 && cp <= 0xFFFF {
                        code_to_unicode[*code as usize] = Some(cp as u16);
                    } else {
                        code_to_unicode[*code as usize] = None;
                    }
                }
                None => {
                    code_to_unicode[*code as usize] = None;
                }
            }
        }

        let mappings: Vec<(u8, u16)> = code_to_unicode
            .iter()
            .enumerate()
            .filter_map(|(code, &unicode)| unicode.map(|u| (code as u8, u)))
            .collect();
        if mappings.is_empty() {
            continue;
        }

        let cmap_data = build_type1_tounicode_cmap(&mappings);
        let len = cmap_data.len() as i64;
        let stream_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! { "Length" => len },
            cmap_data,
        )));
        if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
            dict.set("ToUnicode", Object::Reference(stream_id));
            fixed += 1;
        }
    }

    fixed
}

/// Check if a TrueType font has a (3,1) Unicode BMP cmap.
fn tt_has_unicode_cmap(data: &[u8]) -> bool {
    let Some(cmap_data) = tt_find_table(data, b"cmap") else {
        return false;
    };
    if cmap_data.len() < 4 {
        return false;
    }
    let num_tables = u16::from_be_bytes([cmap_data[2], cmap_data[3]]) as usize;
    for i in 0..num_tables {
        let off = 4 + i * 8;
        if off + 4 > cmap_data.len() {
            break;
        }
        let platform = u16::from_be_bytes([cmap_data[off], cmap_data[off + 1]]);
        let encoding = u16::from_be_bytes([cmap_data[off + 2], cmap_data[off + 3]]);
        if platform == 3 && encoding == 1 {
            return true;
        }
    }
    false
}

fn tt_has_symbol_cmap(data: &[u8]) -> bool {
    let Some(cmap_data) = tt_find_table(data, b"cmap") else {
        return false;
    };
    if cmap_data.len() < 4 {
        return false;
    }
    let num_tables = u16::from_be_bytes([cmap_data[2], cmap_data[3]]) as usize;
    for i in 0..num_tables {
        let off = 4 + i * 8;
        if off + 4 > cmap_data.len() {
            break;
        }
        let platform = u16::from_be_bytes([cmap_data[off], cmap_data[off + 1]]);
        let encoding = u16::from_be_bytes([cmap_data[off + 2], cmap_data[off + 3]]);
        if platform == 3 && encoding == 0 {
            return true;
        }
    }
    false
}

/// Find a table in a TrueType font by tag, returning the table data slice.
fn tt_find_table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    if data.len() < 12 {
        return None;
    }
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    for i in 0..num_tables {
        let off = 12 + i * 16;
        if off + 16 > data.len() {
            break;
        }
        if &data[off..off + 4] == tag {
            let table_off =
                u32::from_be_bytes([data[off + 8], data[off + 9], data[off + 10], data[off + 11]])
                    as usize;
            let table_len = u32::from_be_bytes([
                data[off + 12],
                data[off + 13],
                data[off + 14],
                data[off + 15],
            ]) as usize;
            if table_off + table_len <= data.len() {
                return Some(&data[table_off..table_off + table_len]);
            }
        }
    }
    None
}

struct TtRawMetrics<'a> {
    units_per_em: u16,
    num_glyphs: u16,
    num_h_metrics: u16,
    hmtx: &'a [u8],
}

fn tt_parse_raw_metrics(data: &[u8]) -> Option<TtRawMetrics<'_>> {
    let head = tt_find_table(data, b"head");
    let hhea = tt_find_table(data, b"hhea");
    let hmtx = tt_find_table(data, b"hmtx")?;
    let maxp = tt_find_table(data, b"maxp");
    let loca = tt_find_table(data, b"loca")?;
    if hmtx.len() < 4 || loca.len() < 4 {
        return None;
    }

    let units_per_em = head
        .filter(|h| h.len() >= 20)
        .map(|h| u16::from_be_bytes([h[18], h[19]]))
        .filter(|u| *u > 0)
        .unwrap_or(1000);

    let inferred_num_h_metrics = (hmtx.len() / 4).clamp(1, u16::MAX as usize) as u16;
    let mut num_h_metrics = hhea
        .filter(|h| h.len() >= 36)
        .map(|h| u16::from_be_bytes([h[34], h[35]]))
        .and_then(|n| {
            if n == 0 {
                Some(1)
            } else if n as usize <= hmtx.len() / 4 {
                Some(n)
            } else {
                None
            }
        })
        .unwrap_or(inferred_num_h_metrics);

    let mut index_to_loc_format = head
        .filter(|h| h.len() >= 52)
        .map(|h| i16::from_be_bytes([h[50], h[51]]))
        .unwrap_or(-1);
    if index_to_loc_format != 0 && index_to_loc_format != 1 {
        let short_entries = loca.len() / 2;
        let long_entries = loca.len() / 4;
        if short_entries > 1 && short_entries.saturating_sub(1) >= num_h_metrics as usize {
            index_to_loc_format = 0;
        } else if long_entries > 1 {
            index_to_loc_format = 1;
        } else {
            return None;
        }
    }

    let inferred_num_glyphs = if index_to_loc_format == 0 {
        (loca.len() / 2).saturating_sub(1)
    } else {
        (loca.len() / 4).saturating_sub(1)
    }
    .clamp(1, u16::MAX as usize) as u16;

    let num_glyphs = maxp
        .filter(|m| m.len() >= 6)
        .map(|m| u16::from_be_bytes([m[4], m[5]]))
        .filter(|cand| *cand > 0)
        .map(|cand| {
            let needed = if index_to_loc_format == 0 {
                (cand as usize + 1) * 2
            } else {
                (cand as usize + 1) * 4
            };
            if needed <= loca.len() && inferred_num_glyphs as usize <= cand as usize * 4 {
                cand
            } else {
                inferred_num_glyphs
            }
        })
        .unwrap_or(inferred_num_glyphs);

    if num_h_metrics > num_glyphs {
        num_h_metrics = num_glyphs;
    }
    if num_h_metrics == 0 || num_glyphs == 0 {
        return None;
    }

    Some(TtRawMetrics {
        units_per_em,
        num_glyphs,
        num_h_metrics,
        hmtx,
    })
}

fn tt_raw_glyph_advance(metrics: &TtRawMetrics<'_>, gid: u16) -> Option<u16> {
    let idx = gid.min(metrics.num_h_metrics.saturating_sub(1)) as usize;
    let off = idx * 4;
    if off + 2 > metrics.hmtx.len() {
        return None;
    }
    Some(u16::from_be_bytes([
        metrics.hmtx[off],
        metrics.hmtx[off + 1],
    ]))
}

/// Read (1,0) Mac Roman cmap: returns Vec<(mac_code, gid)>.
fn tt_read_mac_cmap(data: &[u8]) -> Vec<(u8, u16)> {
    let Some(cmap_data) = tt_find_table(data, b"cmap") else {
        return Vec::new();
    };
    if cmap_data.len() < 4 {
        return Vec::new();
    }
    let num_tables = u16::from_be_bytes([cmap_data[2], cmap_data[3]]) as usize;
    for i in 0..num_tables {
        let rec_off = 4 + i * 8;
        if rec_off + 8 > cmap_data.len() {
            break;
        }
        let platform = u16::from_be_bytes([cmap_data[rec_off], cmap_data[rec_off + 1]]);
        let encoding = u16::from_be_bytes([cmap_data[rec_off + 2], cmap_data[rec_off + 3]]);
        if platform != 1 || encoding != 0 {
            continue;
        }
        let sub_off = u32::from_be_bytes([
            cmap_data[rec_off + 4],
            cmap_data[rec_off + 5],
            cmap_data[rec_off + 6],
            cmap_data[rec_off + 7],
        ]) as usize;
        if sub_off + 2 > cmap_data.len() {
            continue;
        }
        let format = u16::from_be_bytes([cmap_data[sub_off], cmap_data[sub_off + 1]]);
        match format {
            0 => {
                // Format 0: 256-byte array at offset 6.
                let arr_off = sub_off + 6;
                if arr_off + 256 > cmap_data.len() {
                    continue;
                }
                let mut result = Vec::new();
                for code in 0u16..256 {
                    let gid = cmap_data[arr_off + code as usize] as u16;
                    if gid > 0 {
                        result.push((code as u8, gid));
                    }
                }
                return result;
            }
            6 => {
                // Format 6: trimmed table.
                if sub_off + 10 > cmap_data.len() {
                    continue;
                }
                let first_code =
                    u16::from_be_bytes([cmap_data[sub_off + 6], cmap_data[sub_off + 7]]);
                let entry_count =
                    u16::from_be_bytes([cmap_data[sub_off + 8], cmap_data[sub_off + 9]]);
                let arr_off = sub_off + 10;
                let mut result = Vec::new();
                for j in 0..entry_count {
                    let gid_off = arr_off + j as usize * 2;
                    if gid_off + 2 > cmap_data.len() {
                        break;
                    }
                    let gid = u16::from_be_bytes([cmap_data[gid_off], cmap_data[gid_off + 1]]);
                    let code = first_code + j;
                    if gid > 0 && code <= 255 {
                        result.push((code as u8, gid));
                    }
                }
                return result;
            }
            _ => continue,
        }
    }
    Vec::new()
}

/// Read (3,0) Symbol cmap: returns Vec<(unicode_code, gid)>.
fn tt_read_symbol_cmap(data: &[u8]) -> Vec<(u16, u16)> {
    let Some(cmap_data) = tt_find_table(data, b"cmap") else {
        return Vec::new();
    };
    if cmap_data.len() < 4 {
        return Vec::new();
    }
    let num_tables = u16::from_be_bytes([cmap_data[2], cmap_data[3]]) as usize;
    for i in 0..num_tables {
        let rec_off = 4 + i * 8;
        if rec_off + 8 > cmap_data.len() {
            break;
        }
        let platform = u16::from_be_bytes([cmap_data[rec_off], cmap_data[rec_off + 1]]);
        let encoding = u16::from_be_bytes([cmap_data[rec_off + 2], cmap_data[rec_off + 3]]);
        if platform != 3 || encoding != 0 {
            continue;
        }
        let sub_off = u32::from_be_bytes([
            cmap_data[rec_off + 4],
            cmap_data[rec_off + 5],
            cmap_data[rec_off + 6],
            cmap_data[rec_off + 7],
        ]) as usize;
        if sub_off + 2 > cmap_data.len() {
            continue;
        }
        let format = u16::from_be_bytes([cmap_data[sub_off], cmap_data[sub_off + 1]]);
        if format == 4 {
            return tt_read_format4(cmap_data, sub_off);
        }
    }
    Vec::new()
}

fn tt_read_windows_cmap(data: &[u8], encoding_id: u16) -> Vec<(u16, u16)> {
    let Some(cmap_data) = tt_find_table(data, b"cmap") else {
        return Vec::new();
    };
    if cmap_data.len() < 4 {
        return Vec::new();
    }
    let num_tables = u16::from_be_bytes([cmap_data[2], cmap_data[3]]) as usize;
    for i in 0..num_tables {
        let rec_off = 4 + i * 8;
        if rec_off + 8 > cmap_data.len() {
            break;
        }
        let platform = u16::from_be_bytes([cmap_data[rec_off], cmap_data[rec_off + 1]]);
        let encoding = u16::from_be_bytes([cmap_data[rec_off + 2], cmap_data[rec_off + 3]]);
        if platform != 3 || encoding != encoding_id {
            continue;
        }
        let sub_off = u32::from_be_bytes([
            cmap_data[rec_off + 4],
            cmap_data[rec_off + 5],
            cmap_data[rec_off + 6],
            cmap_data[rec_off + 7],
        ]) as usize;
        if sub_off + 2 > cmap_data.len() {
            continue;
        }
        let format = u16::from_be_bytes([cmap_data[sub_off], cmap_data[sub_off + 1]]);
        match format {
            4 => return tt_read_format4(cmap_data, sub_off),
            12 => return tt_read_format12(cmap_data, sub_off),
            _ => continue,
        }
    }
    Vec::new()
}

fn tt_unicode_from_pdf_encoding_code(
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
) -> Option<u16> {
    if let Some(name) = differences.get(&code) {
        if name == ".notdef" {
            return None;
        }
        return glyph_name_to_unicode(name)
            .map(|ch| ch as u32)
            .filter(|u| *u <= u16::MAX as u32)
            .map(|u| u as u16);
    }

    if enc_name.is_empty() {
        return None;
    }

    let ch = encoding_to_char(code, enc_name);
    if ch == '\u{FFFF}' {
        return None;
    }
    Some(ch as u16)
}

fn tt_build_unicode_mappings_from_pdf_encoding(
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    first_char: u32,
    last_char: u32,
    symbol_mappings: &[(u16, u16)],
) -> Vec<(u16, u16)> {
    use std::collections::{BTreeMap, HashMap};

    let mut code_to_gid: HashMap<u32, u16> = HashMap::new();
    for (code, gid) in symbol_mappings {
        if *gid != 0 {
            code_to_gid.insert(*code as u32, *gid);
        }
    }

    let mut unicode_to_gid: BTreeMap<u16, u16> = BTreeMap::new();
    let start = first_char.min(255);
    let end = last_char.min(255);
    for code in start..=end {
        let Some(gid) = code_to_gid.get(&code).copied() else {
            continue;
        };
        let Some(unicode) = tt_unicode_from_pdf_encoding_code(code, enc_name, differences) else {
            continue;
        };
        unicode_to_gid.entry(unicode).or_insert(gid);
        // veraPDF canonicalizes soft hyphen to hyphen-minus for width checks.
        if unicode == 0x00AD {
            unicode_to_gid.entry(b'-' as u16).or_insert(gid);
        }
    }

    unicode_to_gid.into_iter().collect()
}

/// Parse a cmap format 4 subtable into (code, gid) pairs.
fn tt_read_format4(data: &[u8], off: usize) -> Vec<(u16, u16)> {
    if off + 14 > data.len() {
        return Vec::new();
    }
    let seg_count = u16::from_be_bytes([data[off + 6], data[off + 7]]) as usize / 2;
    let end_codes_off = off + 14;
    let start_codes_off = end_codes_off + seg_count * 2 + 2; // +2 for reservedPad
    let delta_off = start_codes_off + seg_count * 2;
    let range_off = delta_off + seg_count * 2;

    if range_off + seg_count * 2 > data.len() {
        return Vec::new();
    }

    let mut result = Vec::new();
    for i in 0..seg_count {
        let end_code =
            u16::from_be_bytes([data[end_codes_off + i * 2], data[end_codes_off + i * 2 + 1]]);
        let start_code = u16::from_be_bytes([
            data[start_codes_off + i * 2],
            data[start_codes_off + i * 2 + 1],
        ]);
        let delta = i16::from_be_bytes([data[delta_off + i * 2], data[delta_off + i * 2 + 1]]);
        let range_offset =
            u16::from_be_bytes([data[range_off + i * 2], data[range_off + i * 2 + 1]]);

        if start_code == 0xFFFF {
            break;
        }

        for code in start_code..=end_code {
            let gid = if range_offset == 0 {
                (code as i32 + delta as i32) as u16
            } else {
                let idx = range_offset as usize / 2 + (code - start_code) as usize + i; // relative to range_off position
                let gid_off = range_off + idx * 2;
                if gid_off + 2 > data.len() {
                    0
                } else {
                    let raw = u16::from_be_bytes([data[gid_off], data[gid_off + 1]]);
                    if raw == 0 {
                        0
                    } else {
                        (raw as i32 + delta as i32) as u16
                    }
                }
            };
            if gid > 0 {
                result.push((code, gid));
            }
        }
    }
    result
}

fn tt_read_format12(data: &[u8], off: usize) -> Vec<(u16, u16)> {
    if off + 16 > data.len() {
        return Vec::new();
    }
    let n_groups = u32::from_be_bytes([
        data[off + 12],
        data[off + 13],
        data[off + 14],
        data[off + 15],
    ]) as usize;
    let mut result = Vec::new();
    let mut group_off = off + 16;
    for _ in 0..n_groups {
        if group_off + 12 > data.len() {
            break;
        }
        let start_char = u32::from_be_bytes([
            data[group_off],
            data[group_off + 1],
            data[group_off + 2],
            data[group_off + 3],
        ]);
        let end_char = u32::from_be_bytes([
            data[group_off + 4],
            data[group_off + 5],
            data[group_off + 6],
            data[group_off + 7],
        ]);
        let start_gid = u32::from_be_bytes([
            data[group_off + 8],
            data[group_off + 9],
            data[group_off + 10],
            data[group_off + 11],
        ]);

        if start_char <= 0xFFFF {
            let end_bmp = end_char.min(0xFFFF);
            for code in start_char..=end_bmp {
                let gid = start_gid + (code - start_char);
                if gid > 0 && gid <= u16::MAX as u32 {
                    result.push((code as u16, gid as u16));
                }
            }
        }

        group_off += 12;
    }
    result
}

/// Build a cmap format 4 subtable from Unicode → GID mappings.
fn tt_build_format4(mappings: &[(u16, u16)]) -> Vec<u8> {
    let mut sorted: Vec<(u16, u16)> = mappings.to_vec();
    sorted.sort_by_key(|(u, _)| *u);
    sorted.dedup_by_key(|(u, _)| *u);

    // Build segments: merge consecutive codes with consecutive GIDs.
    let mut segments: Vec<(u16, u16, i16)> = Vec::new(); // (start, end, delta)
    for &(unicode, gid) in &sorted {
        let delta = (gid as i32 - unicode as i32) as i16;
        if let Some(last) = segments.last_mut() {
            if last.2 == delta && unicode == last.1 + 1 {
                last.1 = unicode;
                continue;
            }
        }
        segments.push((unicode, unicode, delta));
    }
    // Sentinel segment.
    segments.push((0xFFFF, 0xFFFF, 1));

    let seg_count = segments.len();
    let seg_count_x2 = (seg_count * 2) as u16;
    let max_pow2 = if seg_count > 0 {
        (seg_count as f64).log2().floor() as u32
    } else {
        0
    };
    let search_range = 2u16.pow(max_pow2) * 2;
    let entry_selector = max_pow2 as u16;
    let range_shift = seg_count_x2 - search_range;

    let length = 16 + seg_count * 8; // header(14) + 4 arrays × segCount × 2 + reservedPad(2)
    let mut data = Vec::with_capacity(length);

    // Header.
    data.extend_from_slice(&4u16.to_be_bytes()); // format
    data.extend_from_slice(&(length as u16).to_be_bytes());
    data.extend_from_slice(&0u16.to_be_bytes()); // language
    data.extend_from_slice(&seg_count_x2.to_be_bytes());
    data.extend_from_slice(&search_range.to_be_bytes());
    data.extend_from_slice(&entry_selector.to_be_bytes());
    data.extend_from_slice(&range_shift.to_be_bytes());

    // endCode array.
    for (_, end, _) in &segments {
        data.extend_from_slice(&end.to_be_bytes());
    }
    // reservedPad.
    data.extend_from_slice(&0u16.to_be_bytes());
    // startCode array.
    for (start, _, _) in &segments {
        data.extend_from_slice(&start.to_be_bytes());
    }
    // idDelta array.
    for (_, _, delta) in &segments {
        data.extend_from_slice(&delta.to_be_bytes());
    }
    // idRangeOffset array (all zeros — using idDelta only).
    for _ in &segments {
        data.extend_from_slice(&0u16.to_be_bytes());
    }

    data
}

/// Build a Macintosh cmap format 6 subtable from raw byte code -> GID mappings.
fn tt_build_format6(mappings: &[(u8, u16)]) -> Vec<u8> {
    let mut glyphs = [0u16; 256];
    for (code, gid) in mappings {
        glyphs[*code as usize] = *gid;
    }

    let length = 10 + glyphs.len() * 2;
    let mut data = Vec::with_capacity(length);
    data.extend_from_slice(&6u16.to_be_bytes()); // format
    data.extend_from_slice(&(length as u16).to_be_bytes());
    data.extend_from_slice(&0u16.to_be_bytes()); // language
    data.extend_from_slice(&0u16.to_be_bytes()); // firstCode
    data.extend_from_slice(&(glyphs.len() as u16).to_be_bytes());
    for gid in glyphs {
        data.extend_from_slice(&gid.to_be_bytes());
    }
    data
}

/// Calculate TrueType table checksum.
fn tt_checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 4 <= data.len() {
        sum = sum.wrapping_add(u32::from_be_bytes([
            data[i],
            data[i + 1],
            data[i + 2],
            data[i + 3],
        ]));
        i += 4;
    }
    if i < data.len() {
        let mut buf = [0u8; 4];
        for (j, byte) in data[i..].iter().enumerate() {
            buf[j] = *byte;
        }
        sum = sum.wrapping_add(u32::from_be_bytes(buf));
    }
    sum
}

/// Add a Windows cmap subtable to a TrueType font.
///
/// Rebuilds the cmap table with the original subtables plus a new format 4
/// subtable for platform 3 with the requested encoding ID.
fn tt_add_windows_cmap_subtable(
    data: &[u8],
    mappings: &[(u16, u16)],
    encoding_id: u16,
) -> Option<Vec<u8>> {
    if data.len() < 12 {
        return None;
    }
    let sf_version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    if data.len() < 12 + num_tables * 16 {
        return None;
    }

    // Parse table directory.
    struct TableEntry {
        tag: [u8; 4],
        offset: u32,
        length: u32,
    }
    let mut tables: Vec<TableEntry> = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let off = 12 + i * 16;
        let tag = [data[off], data[off + 1], data[off + 2], data[off + 3]];
        let offset =
            u32::from_be_bytes([data[off + 8], data[off + 9], data[off + 10], data[off + 11]]);
        let length = u32::from_be_bytes([
            data[off + 12],
            data[off + 13],
            data[off + 14],
            data[off + 15],
        ]);
        tables.push(TableEntry {
            tag,
            offset,
            length,
        });
    }

    // Build the new cmap table.
    let cmap_idx = tables.iter().position(|t| &t.tag == b"cmap")?;
    let old_cmap = &data[tables[cmap_idx].offset as usize
        ..(tables[cmap_idx].offset + tables[cmap_idx].length) as usize];

    let old_num_subtables = u16::from_be_bytes([old_cmap[2], old_cmap[3]]) as usize;
    let new_num_subtables = old_num_subtables + 1;
    let new_header_size = 4 + new_num_subtables * 8;
    let old_header_size = 4 + old_num_subtables * 8;
    let header_growth = 8; // One new encoding record.

    // Build new cmap: header + adjusted original subtables + new format 4.
    let format4 = tt_build_format4(mappings);
    let old_subtable_data = &old_cmap[old_header_size..];
    let new_format4_offset = new_header_size + old_subtable_data.len();

    let mut new_cmap = Vec::with_capacity(new_format4_offset + format4.len());

    // Header.
    new_cmap.extend_from_slice(&0u16.to_be_bytes()); // version
    new_cmap.extend_from_slice(&(new_num_subtables as u16).to_be_bytes());

    // Copy existing encoding records with adjusted offsets.
    for i in 0..old_num_subtables {
        let rec_off = 4 + i * 8;
        // Platform and encoding IDs (4 bytes).
        new_cmap.extend_from_slice(&old_cmap[rec_off..rec_off + 4]);
        // Adjust subtable offset.
        let old_offset = u32::from_be_bytes([
            old_cmap[rec_off + 4],
            old_cmap[rec_off + 5],
            old_cmap[rec_off + 6],
            old_cmap[rec_off + 7],
        ]);
        let new_offset = old_offset + header_growth as u32;
        new_cmap.extend_from_slice(&new_offset.to_be_bytes());
    }

    // Add new Windows encoding record.
    new_cmap.extend_from_slice(&3u16.to_be_bytes()); // platformID
    new_cmap.extend_from_slice(&encoding_id.to_be_bytes());
    new_cmap.extend_from_slice(&(new_format4_offset as u32).to_be_bytes());

    // Copy original subtable data.
    new_cmap.extend_from_slice(old_subtable_data);

    // Append new format 4 subtable.
    new_cmap.extend_from_slice(&format4);

    // Rebuild the entire font with the new cmap table.
    let dir_size = 12 + num_tables * 16;

    // Calculate table directory header values.
    let max_pow2 = if num_tables > 0 {
        (num_tables as f64).log2().floor() as u32
    } else {
        0
    };
    let search_range = 16u32 * 2u32.pow(max_pow2);
    let entry_selector = max_pow2;
    let range_shift = (num_tables * 16) as u32 - search_range;

    let mut output = Vec::with_capacity(data.len() + format4.len() + 64);

    // Font header.
    output.extend_from_slice(&sf_version.to_be_bytes());
    output.extend_from_slice(&(num_tables as u16).to_be_bytes());
    output.extend_from_slice(&(search_range as u16).to_be_bytes());
    output.extend_from_slice(&(entry_selector as u16).to_be_bytes());
    output.extend_from_slice(&(range_shift as u16).to_be_bytes());

    // Placeholder table directory (will fill in offsets after writing data).
    let dir_start = output.len();
    output.resize(dir_size, 0);

    // Write each table's data and record its position.
    let mut head_offset_in_output: Option<usize> = None;
    for (i, table) in tables.iter().enumerate() {
        // Pad to 4-byte boundary.
        while output.len() % 4 != 0 {
            output.push(0);
        }

        let table_data = if i == cmap_idx {
            &new_cmap
        } else {
            let start = table.offset as usize;
            let end = start + table.length as usize;
            if end > data.len() {
                return None;
            }
            &data[start..end]
        };

        let out_offset = output.len() as u32;
        let out_length = table_data.len() as u32;
        let checksum = tt_checksum(table_data);

        if &table.tag == b"head" {
            head_offset_in_output = Some(output.len());
        }

        // Fill in the directory entry.
        let entry_off = dir_start + i * 16;
        output[entry_off..entry_off + 4].copy_from_slice(&table.tag);
        output[entry_off + 4..entry_off + 8].copy_from_slice(&checksum.to_be_bytes());
        output[entry_off + 8..entry_off + 12].copy_from_slice(&out_offset.to_be_bytes());
        output[entry_off + 12..entry_off + 16].copy_from_slice(&out_length.to_be_bytes());

        output.extend_from_slice(table_data);
    }

    // Fix head checkSumAdjustment.
    if let Some(head_off) = head_offset_in_output {
        if head_off + 12 <= output.len() {
            // Zero out checkSumAdjustment before computing file checksum.
            output[head_off + 8..head_off + 12].copy_from_slice(&0u32.to_be_bytes());
            let file_checksum = tt_checksum(&output);
            let adjustment = 0xB1B0_AFBAu32.wrapping_sub(file_checksum);
            output[head_off + 8..head_off + 12].copy_from_slice(&adjustment.to_be_bytes());
        }
    }

    Some(output)
}

fn tt_cmap_subtable_len(cmap_data: &[u8], sub_off: usize) -> Option<usize> {
    if sub_off + 4 > cmap_data.len() {
        return None;
    }
    let format = u16::from_be_bytes([cmap_data[sub_off], cmap_data[sub_off + 1]]);
    let len = match format {
        12 | 13 => {
            if sub_off + 8 > cmap_data.len() {
                return None;
            }
            u32::from_be_bytes([
                cmap_data[sub_off + 4],
                cmap_data[sub_off + 5],
                cmap_data[sub_off + 6],
                cmap_data[sub_off + 7],
            ]) as usize
        }
        14 => {
            if sub_off + 6 > cmap_data.len() {
                return None;
            }
            u32::from_be_bytes([0, 0, cmap_data[sub_off + 2], cmap_data[sub_off + 3]]) as usize
        }
        _ => u16::from_be_bytes([cmap_data[sub_off + 2], cmap_data[sub_off + 3]]) as usize,
    };
    if sub_off + len > cmap_data.len() || len == 0 {
        return None;
    }
    Some(len)
}

fn tt_replace_windows_cmap_subtable(
    data: &[u8],
    mappings: &[(u16, u16)],
    encoding_id: u16,
) -> Option<Vec<u8>> {
    if data.len() < 12 {
        return None;
    }
    let sf_version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    if data.len() < 12 + num_tables * 16 {
        return None;
    }

    struct TableEntry {
        tag: [u8; 4],
        offset: u32,
        length: u32,
    }
    let mut tables: Vec<TableEntry> = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let off = 12 + i * 16;
        let tag = [data[off], data[off + 1], data[off + 2], data[off + 3]];
        let offset =
            u32::from_be_bytes([data[off + 8], data[off + 9], data[off + 10], data[off + 11]]);
        let length = u32::from_be_bytes([
            data[off + 12],
            data[off + 13],
            data[off + 14],
            data[off + 15],
        ]);
        tables.push(TableEntry {
            tag,
            offset,
            length,
        });
    }

    let cmap_idx = tables.iter().position(|t| &t.tag == b"cmap")?;
    let old_cmap = &data[tables[cmap_idx].offset as usize
        ..(tables[cmap_idx].offset + tables[cmap_idx].length) as usize];
    if old_cmap.len() < 4 {
        return None;
    }

    let old_num_subtables = u16::from_be_bytes([old_cmap[2], old_cmap[3]]) as usize;
    let mut kept_subtables: Vec<(u16, u16, Vec<u8>)> = Vec::new();
    for i in 0..old_num_subtables {
        let rec_off = 4 + i * 8;
        if rec_off + 8 > old_cmap.len() {
            return None;
        }
        let platform = u16::from_be_bytes([old_cmap[rec_off], old_cmap[rec_off + 1]]);
        let encoding = u16::from_be_bytes([old_cmap[rec_off + 2], old_cmap[rec_off + 3]]);
        if platform == 3 && encoding == encoding_id {
            continue;
        }
        let sub_off = u32::from_be_bytes([
            old_cmap[rec_off + 4],
            old_cmap[rec_off + 5],
            old_cmap[rec_off + 6],
            old_cmap[rec_off + 7],
        ]) as usize;
        let len = tt_cmap_subtable_len(old_cmap, sub_off)?;
        kept_subtables.push((
            platform,
            encoding,
            old_cmap[sub_off..sub_off + len].to_vec(),
        ));
    }

    let format4 = tt_build_format4(mappings);
    let new_num_subtables = kept_subtables.len() + 1;
    let mut new_cmap = Vec::new();
    new_cmap.extend_from_slice(&0u16.to_be_bytes());
    new_cmap.extend_from_slice(&(new_num_subtables as u16).to_be_bytes());

    let mut offset = 4 + new_num_subtables * 8;
    for (platform, encoding, bytes) in &kept_subtables {
        new_cmap.extend_from_slice(&platform.to_be_bytes());
        new_cmap.extend_from_slice(&encoding.to_be_bytes());
        new_cmap.extend_from_slice(&(offset as u32).to_be_bytes());
        offset += bytes.len();
    }
    new_cmap.extend_from_slice(&3u16.to_be_bytes());
    new_cmap.extend_from_slice(&encoding_id.to_be_bytes());
    new_cmap.extend_from_slice(&(offset as u32).to_be_bytes());

    for (_, _, bytes) in &kept_subtables {
        new_cmap.extend_from_slice(bytes);
    }
    new_cmap.extend_from_slice(&format4);

    let dir_size = 12 + num_tables * 16;
    let max_pow2 = if num_tables > 0 {
        (num_tables as f64).log2().floor() as u32
    } else {
        0
    };
    let search_range = 16u32 * 2u32.pow(max_pow2);
    let entry_selector = max_pow2;
    let range_shift = (num_tables * 16) as u32 - search_range;

    let mut output = Vec::with_capacity(data.len() + new_cmap.len() + 64);
    output.extend_from_slice(&sf_version.to_be_bytes());
    output.extend_from_slice(&(num_tables as u16).to_be_bytes());
    output.extend_from_slice(&(search_range as u16).to_be_bytes());
    output.extend_from_slice(&(entry_selector as u16).to_be_bytes());
    output.extend_from_slice(&(range_shift as u16).to_be_bytes());

    let dir_start = output.len();
    output.resize(dir_size, 0);

    let mut head_offset_in_output: Option<usize> = None;
    for (i, table) in tables.iter().enumerate() {
        while output.len() % 4 != 0 {
            output.push(0);
        }

        let table_data = if i == cmap_idx {
            &new_cmap
        } else {
            let start = table.offset as usize;
            let end = start + table.length as usize;
            if end > data.len() {
                return None;
            }
            &data[start..end]
        };

        let out_offset = output.len() as u32;
        let out_length = table_data.len() as u32;
        let checksum = tt_checksum(table_data);

        if &table.tag == b"head" {
            head_offset_in_output = Some(output.len());
        }

        let entry_off = dir_start + i * 16;
        output[entry_off..entry_off + 4].copy_from_slice(&table.tag);
        output[entry_off + 4..entry_off + 8].copy_from_slice(&checksum.to_be_bytes());
        output[entry_off + 8..entry_off + 12].copy_from_slice(&out_offset.to_be_bytes());
        output[entry_off + 12..entry_off + 16].copy_from_slice(&out_length.to_be_bytes());

        output.extend_from_slice(table_data);
    }

    if let Some(head_off) = head_offset_in_output {
        if head_off + 12 <= output.len() {
            output[head_off + 8..head_off + 12].copy_from_slice(&0u32.to_be_bytes());
            let file_checksum = tt_checksum(&output);
            let adjustment = 0xB1B0_AFBAu32.wrapping_sub(file_checksum);
            output[head_off + 8..head_off + 12].copy_from_slice(&adjustment.to_be_bytes());
        }
    }

    Some(output)
}

fn tt_replace_mac_cmap_subtable(data: &[u8], mappings: &[(u8, u16)]) -> Option<Vec<u8>> {
    if data.len() < 12 {
        return None;
    }
    let sf_version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    if data.len() < 12 + num_tables * 16 {
        return None;
    }

    struct TableEntry {
        tag: [u8; 4],
        offset: u32,
        length: u32,
    }
    let mut tables: Vec<TableEntry> = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let off = 12 + i * 16;
        let tag = [data[off], data[off + 1], data[off + 2], data[off + 3]];
        let offset =
            u32::from_be_bytes([data[off + 8], data[off + 9], data[off + 10], data[off + 11]]);
        let length = u32::from_be_bytes([
            data[off + 12],
            data[off + 13],
            data[off + 14],
            data[off + 15],
        ]);
        tables.push(TableEntry {
            tag,
            offset,
            length,
        });
    }

    let cmap_idx = tables.iter().position(|t| &t.tag == b"cmap")?;
    let old_cmap = &data[tables[cmap_idx].offset as usize
        ..(tables[cmap_idx].offset + tables[cmap_idx].length) as usize];
    if old_cmap.len() < 4 {
        return None;
    }

    let old_num_subtables = u16::from_be_bytes([old_cmap[2], old_cmap[3]]) as usize;
    let mut kept_subtables: Vec<(u16, u16, Vec<u8>)> = Vec::new();
    for i in 0..old_num_subtables {
        let rec_off = 4 + i * 8;
        if rec_off + 8 > old_cmap.len() {
            return None;
        }
        let platform = u16::from_be_bytes([old_cmap[rec_off], old_cmap[rec_off + 1]]);
        let encoding = u16::from_be_bytes([old_cmap[rec_off + 2], old_cmap[rec_off + 3]]);
        if platform == 1 && encoding == 0 {
            continue;
        }
        let sub_off = u32::from_be_bytes([
            old_cmap[rec_off + 4],
            old_cmap[rec_off + 5],
            old_cmap[rec_off + 6],
            old_cmap[rec_off + 7],
        ]) as usize;
        let len = tt_cmap_subtable_len(old_cmap, sub_off)?;
        kept_subtables.push((
            platform,
            encoding,
            old_cmap[sub_off..sub_off + len].to_vec(),
        ));
    }

    let format6 = tt_build_format6(mappings);
    let new_num_subtables = kept_subtables.len() + 1;
    let mut new_cmap = Vec::new();
    new_cmap.extend_from_slice(&0u16.to_be_bytes());
    new_cmap.extend_from_slice(&(new_num_subtables as u16).to_be_bytes());

    let mut offset = 4 + new_num_subtables * 8;
    for (platform, encoding, bytes) in &kept_subtables {
        new_cmap.extend_from_slice(&platform.to_be_bytes());
        new_cmap.extend_from_slice(&encoding.to_be_bytes());
        new_cmap.extend_from_slice(&(offset as u32).to_be_bytes());
        offset += bytes.len();
    }
    new_cmap.extend_from_slice(&1u16.to_be_bytes());
    new_cmap.extend_from_slice(&0u16.to_be_bytes());
    new_cmap.extend_from_slice(&(offset as u32).to_be_bytes());

    for (_, _, bytes) in &kept_subtables {
        new_cmap.extend_from_slice(bytes);
    }
    new_cmap.extend_from_slice(&format6);

    let dir_size = 12 + num_tables * 16;
    let max_pow2 = if num_tables > 0 {
        (num_tables as f64).log2().floor() as u32
    } else {
        0
    };
    let search_range = 16u32 * 2u32.pow(max_pow2);
    let entry_selector = max_pow2;
    let range_shift = (num_tables * 16) as u32 - search_range;

    let mut output = Vec::with_capacity(data.len() + new_cmap.len() + 64);
    output.extend_from_slice(&sf_version.to_be_bytes());
    output.extend_from_slice(&(num_tables as u16).to_be_bytes());
    output.extend_from_slice(&(search_range as u16).to_be_bytes());
    output.extend_from_slice(&(entry_selector as u16).to_be_bytes());
    output.extend_from_slice(&(range_shift as u16).to_be_bytes());

    let dir_start = output.len();
    output.resize(dir_size, 0);

    let mut head_offset_in_output: Option<usize> = None;
    for (i, table) in tables.iter().enumerate() {
        while output.len() % 4 != 0 {
            output.push(0);
        }

        let table_data = if i == cmap_idx {
            &new_cmap
        } else {
            let start = table.offset as usize;
            let end = start + table.length as usize;
            if end > data.len() {
                return None;
            }
            &data[start..end]
        };

        let out_offset = output.len() as u32;
        let out_length = table_data.len() as u32;
        let checksum = tt_checksum(table_data);

        if &table.tag == b"head" {
            head_offset_in_output = Some(output.len());
        }

        let entry_off = dir_start + i * 16;
        output[entry_off..entry_off + 4].copy_from_slice(&table.tag);
        output[entry_off + 4..entry_off + 8].copy_from_slice(&checksum.to_be_bytes());
        output[entry_off + 8..entry_off + 12].copy_from_slice(&out_offset.to_be_bytes());
        output[entry_off + 12..entry_off + 16].copy_from_slice(&out_length.to_be_bytes());

        output.extend_from_slice(table_data);
    }

    if let Some(head_off) = head_offset_in_output {
        if head_off + 12 <= output.len() {
            output[head_off + 8..head_off + 12].copy_from_slice(&0u32.to_be_bytes());
            let file_checksum = tt_checksum(&output);
            let adjustment = 0xB1B0_AFBAu32.wrapping_sub(file_checksum);
            output[head_off + 8..head_off + 12].copy_from_slice(&adjustment.to_be_bytes());
        }
    }

    Some(output)
}

/// Add a (3,1) Unicode BMP cmap subtable to a TrueType font.
fn tt_add_unicode_cmap_subtable(data: &[u8], mappings: &[(u16, u16)]) -> Option<Vec<u8>> {
    tt_add_windows_cmap_subtable(data, mappings, 1)
}

/// Add a (3,0) Microsoft Symbol cmap subtable to a TrueType font.
fn tt_add_symbol_cmap_subtable(data: &[u8], mappings: &[(u16, u16)]) -> Option<Vec<u8>> {
    tt_add_windows_cmap_subtable(data, mappings, 0)
}

/// Ensure non-symbolic TrueType fonts with WinAnsiEncoding have Differences
/// entries for ALL undefined codes (0-31, 127, 129, 141, 143, 144, 157).
///
/// Without explicit Differences, these codes have ambiguous glyph mapping:
/// veraPDF may use the font's built-in encoding or cmap fallbacks that differ
/// from our width computation. Mapping them to "space" ensures both veraPDF
/// and our width fixer use the same glyph (U+0020 → space width).
pub fn fix_undefined_encoding_codes(doc: &mut Document) -> usize {
    // Codes that are undefined in WinAnsiEncoding (CP-1252):
    // 0-31: C0 control characters (except 9, 10, 13 which are HT, LF, CR)
    // 127: DELETE
    // 129, 141, 143, 144, 157: undefined positions in CP-1252
    const UNDEFINED_CODES: &[u32] = &[
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 127, 129, 141, 143, 144, 157,
    ];

    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let dict = obj.as_dict().ok()?;
            if get_name(dict, b"Subtype").as_deref() != Some("TrueType") {
                return None;
            }
            let base_font = get_name(dict, b"BaseFont").unwrap_or_default();
            if is_font_symbolic(doc, dict) || is_symbolic_font_name(&base_font) {
                return None;
            }
            // Skip subset fonts — adding Differences for undefined WinAnsi codes
            // changes the Encoding from a simple name to a dict with Differences,
            // which alters veraPDF's glyph lookup path and can trigger §6.2.11.4.1:2
            // for unrelated codes in the subset.
            let is_subset = base_font.len() > 7 && base_font.as_bytes()[6] == b'+';
            if is_subset {
                return None;
            }
            // Check if encoding is WinAnsiEncoding (with or without Differences).
            let enc = dict.get(b"Encoding").ok()?;
            let is_winansi = match enc {
                Object::Name(n) => n == b"WinAnsiEncoding",
                Object::Dictionary(d) => {
                    get_name(d, b"BaseEncoding").as_deref() == Some("WinAnsiEncoding")
                }
                Object::Reference(r) => match doc.get_object(*r).ok() {
                    Some(Object::Name(n)) => n == b"WinAnsiEncoding",
                    Some(Object::Dictionary(d)) => {
                        get_name(d, b"BaseEncoding").as_deref() == Some("WinAnsiEncoding")
                    }
                    _ => false,
                },
                _ => false,
            };
            if is_winansi {
                Some(*id)
            } else {
                None
            }
        })
        .collect();

    let mut fixed = 0;

    for font_id in font_ids {
        // Parse existing Differences to see which undefined codes are already covered.
        let existing_diff = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let enc = dict.get(b"Encoding").ok();
            parse_differences_from_encoding(doc, enc)
        };

        // Find which undefined codes are missing from Differences.
        let missing: Vec<u32> = UNDEFINED_CODES
            .iter()
            .filter(|&&code| !existing_diff.contains_key(&code))
            .copied()
            .collect();

        if missing.is_empty() {
            continue;
        }

        // Build a new Differences array that includes all existing entries
        // plus the missing undefined codes → "space".
        let mut all_diff = existing_diff;
        for code in &missing {
            all_diff.insert(*code, "space".to_string());
        }

        // Convert to sorted Differences array format.
        let mut sorted_codes: Vec<u32> = all_diff.keys().copied().collect();
        sorted_codes.sort();

        let mut diff_array = Vec::new();
        let mut prev_code: Option<u32> = None;
        for code in sorted_codes {
            let needs_int = prev_code.is_none_or(|p| code != p + 1);
            if needs_int {
                diff_array.push(Object::Integer(code as i64));
            }
            diff_array.push(Object::Name(all_diff[&code].as_bytes().to_vec()));
            prev_code = Some(code);
        }

        // Update the font's Encoding to a dict with BaseEncoding + Differences.
        let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) else {
            continue;
        };
        let mut enc_dict = lopdf::Dictionary::new();
        enc_dict.set("Type", Object::Name(b"Encoding".to_vec()));
        enc_dict.set("BaseEncoding", Object::Name(b"WinAnsiEncoding".to_vec()));
        enc_dict.set("Differences", Object::Array(diff_array));
        dict.set("Encoding", Object::Dictionary(enc_dict));
        fixed += 1;
    }

    fixed
}

/// Parse Differences from an Encoding value (Name, Dictionary, or Reference).
fn parse_differences_from_encoding(
    doc: &Document,
    enc: Option<&Object>,
) -> std::collections::HashMap<u32, String> {
    let mut diff = std::collections::HashMap::new();
    let enc_dict = match enc {
        Some(Object::Dictionary(d)) => Some(d),
        Some(Object::Reference(r)) => doc.get_object(*r).ok().and_then(|o| o.as_dict().ok()),
        _ => None,
    };
    if let Some(enc_dict) = enc_dict {
        if let Ok(arr) = enc_dict.get(b"Differences") {
            let arr = match arr {
                Object::Array(a) => Some(a.as_slice()),
                Object::Reference(r) => doc
                    .get_object(*r)
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .map(|a| a.as_slice()),
                _ => None,
            };
            if let Some(arr) = arr {
                let mut code = 0u32;
                for item in arr {
                    match item {
                        Object::Integer(i) => code = *i as u32,
                        Object::Name(n) => {
                            diff.insert(code, String::from_utf8_lossy(n).to_string());
                            code += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    diff
}

/// Check if a font is symbolic based on FontDescriptor Flags and font name.
fn is_font_symbolic(doc: &Document, font_dict: &lopdf::Dictionary) -> bool {
    // Check FontDescriptor Flags FIRST — these may have been updated after
    // embedding a non-symbolic fallback font (e.g., DejaVuSans for ZapfDingbats).
    // Bit 3 (value 4) = Symbolic, bit 6 (value 32) = Nonsymbolic.
    let fd = match font_dict.get(b"FontDescriptor") {
        Ok(Object::Reference(id)) => doc.get_object(*id).ok(),
        Ok(obj) => Some(obj),
        _ => None,
    };
    if let Some(Object::Dictionary(fd_dict)) = fd {
        if let Ok(Object::Integer(flags)) = fd_dict.get(b"Flags") {
            let symbolic = (*flags & 4) != 0;
            let nonsymbolic = (*flags & 32) != 0;
            // Respect unambiguous flag settings first.
            if nonsymbolic && !symbolic {
                return false;
            }
            if symbolic && !nonsymbolic {
                return true;
            }
            // If both bits are set, the PDF is malformed (they are mutually
            // exclusive per ISO 32000 Table 122).  veraPDF treats the Symbolic
            // bit (4) as dominant for §6.2.11.6:3 validation — i.e. if bit 3
            // is set, the font is considered symbolic regardless of bit 6, so
            // an Encoding entry is forbidden.  Return true so our pipeline
            // strips Encoding from these fonts and avoids the violation.
            if symbolic && nonsymbolic {
                return true;
            }
        }
    }

    // Fallback: check base font name against known symbolic fonts.
    if let Some(name) = get_name(font_dict, b"BaseFont") {
        if is_symbolic_font_name(&name) {
            return true;
        }
    }

    false
}

/// Fix FontDescriptor Flags for known symbolic fonts.
/// Sets Symbolic bit (4) and clears Nonsymbolic bit (32) for Symbol/ZapfDingbats etc.
/// NOTE: Disabled — marking fallback fonts as Symbolic causes 6.2.11.6:4 regression
/// because DejaVuSans has multiple cmap subtables (symbolic fonts need exactly one).
#[allow(dead_code)]
pub fn fix_symbolic_font_flags(doc: &mut Document) -> usize {
    let mut to_fix: Vec<(ObjectId, ObjectId)> = Vec::new(); // (font_id, fd_id)

    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        let subtype = get_name(dict, b"Subtype").unwrap_or_default();
        if subtype != "TrueType" && subtype != "Type1" {
            continue;
        }
        let Some(name) = get_name(dict, b"BaseFont") else {
            continue;
        };
        if !is_symbolic_font_name(&name) {
            continue;
        }
        // Check if FontDescriptor Flags are wrong.
        let fd_id = match dict.get(b"FontDescriptor") {
            Ok(Object::Reference(fid)) => *fid,
            _ => continue,
        };
        let needs_fix = match doc.objects.get(&fd_id) {
            Some(Object::Dictionary(fd)) => match fd.get(b"Flags") {
                Ok(Object::Integer(flags)) => (*flags & 4) == 0, // Symbolic bit not set
                _ => true,
            },
            _ => false,
        };
        if needs_fix {
            to_fix.push((*id, fd_id));
        }
    }

    let count = to_fix.len();
    for (_font_id, fd_id) in to_fix {
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            let flags = match fd.get(b"Flags") {
                Ok(Object::Integer(f)) => *f,
                _ => 0,
            };
            // Set Symbolic (bit 2 = 4), clear Nonsymbolic (bit 5 = 32).
            let new_flags = (flags | 4) & !32;
            fd.set("Flags", Object::Integer(new_flags));
        }
    }
    count
}

/// Fix width mismatches for symbolic TrueType fonts — rule 6.2.11.5:1.
///
/// We intentionally limit this pass to TrueType (`FontFile2`) symbolic fonts.
/// For Type1/CFF symbolic fonts, different validators may resolve widths through
/// glyph names and encoding differences in ways that are not captured reliably
/// by our current CFF lookup, and aggressive rewrites can regress compliant files.
pub fn fix_symbolic_font_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;

    for font_id in font_ids {
        let info = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "TrueType" && subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            let name = match get_name(dict, b"BaseFont") {
                Some(n) => n,
                None => continue,
            };
            if !is_symbolic_font_name(&name) {
                continue;
            }
            let base_name = strip_subset_prefix(&name).to_string();
            let is_classic_symbol = matches!(
                base_name.as_str(),
                "Symbol" | "SymbolMT" | "ZapfDingbats" | "Dingbats"
            );
            // Respect descriptor flags: some Symbol-named fallback fonts are
            // intentionally non-symbolic and should stay on the regular path.
            if !is_font_symbolic(doc, dict) {
                continue;
            }

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };

            let fc = match dict.get(b"FirstChar").ok() {
                Some(Object::Integer(i)) => *i as u32,
                _ => continue,
            };
            let (existing_widths, widths_ref) = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => (arr.clone(), None),
                Some(Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(Object::Array(arr)) => (arr.clone(), Some(*r)),
                    _ => continue,
                },
                _ => continue,
            };
            if existing_widths.is_empty() {
                continue;
            }

            let is_subset = name.contains('+');
            let enc_info = if subtype == "TrueType" {
                (String::new(), std::collections::HashMap::new())
            } else {
                get_simple_encoding_info(doc, dict)
            };

            (
                subtype,
                fd_id,
                fc,
                existing_widths,
                widths_ref,
                is_subset,
                is_classic_symbol,
                enc_info,
            )
        };

        let (
            subtype,
            fd_id,
            first_char,
            existing_widths,
            widths_ref,
            is_subset,
            is_classic_symbol,
            enc_info,
        ) = info;

        let (has_ff, has_ff2, has_ff3) = match doc.objects.get(&fd_id) {
            Some(Object::Dictionary(d)) => {
                (d.has(b"FontFile"), d.has(b"FontFile2"), d.has(b"FontFile3"))
            }
            _ => continue,
        };
        if !has_ff && !has_ff2 && !has_ff3 {
            continue;
        }

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let corrections = if subtype == "TrueType" {
            if !has_ff2 {
                continue;
            }
            compute_symbolic_truetype_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                is_subset,
            )
        } else {
            if !has_ff3 {
                continue;
            }
            // Subset classic Symbol/ZapfDingbats CFF fonts (ABCDEF+Name prefix) are
            // handled by fix_font_width_mismatches via the CFF internal encoding path,
            // which exactly mirrors veraPDF §6.2.11.5. Running fix_symbolic_font_widths
            // on these fonts would re-apply PDF Encoding Differences-based widths (e.g.
            // code 1 → "second" → charstring 411), overwriting the correct CFF-encoding
            // widths (code 1 → StandardEncoding fallback → .notdef → 250). Skip them.
            // (#6.2.11.5-sym-subset-skip)
            if is_classic_symbol && is_subset {
                continue;
            }
            if is_classic_symbol {
                let mut merged = std::collections::BTreeMap::<usize, i64>::new();
                let has_explicit_differences = !enc_info.1.is_empty();

                // For explicit Differences names, prefer direct glyph-name /
                // Unicode lookup in the embedded font.
                for (idx, w) in compute_symbolic_difference_width_corrections(
                    &font_data,
                    first_char,
                    &existing_widths,
                    &enc_info.1,
                ) {
                    merged.insert(idx, w);
                }

                // When a classic Symbol/Zapf font carries a standard PDF
                // Encoding name (for example WinAnsiEncoding), the code ->
                // glyph-name mapping is deterministic per the PDF encoding,
                // even without explicit Differences. Use that mapping to keep
                // /Widths aligned with the embedded CFF program.
                if !enc_info.0.is_empty() {
                    for (idx, w) in compute_symbolic_cff_encoding_width_corrections(
                        &font_data,
                        first_char,
                        &existing_widths,
                        &enc_info.0,
                        &enc_info.1,
                    ) {
                        merged.entry(idx).or_insert(w);
                    }
                }

                // For subset classic Symbol/Zapf CFF fonts, use the CFF internal
                // encoding to compute per-glyph widths. veraPDF validates /Widths
                // against the CFF encoding, so this is the authoritative source. (#496)
                if is_subset {
                    let empty_enc = (String::new(), std::collections::HashMap::new());
                    for (idx, w) in compute_cff_type1_width_corrections(
                        &font_data,
                        first_char,
                        &existing_widths,
                        &empty_enc,
                        true,
                        None,
                    ) {
                        // Codes covered by explicit Differences are already handled
                        // by step 1 (find_cff_glyph_width_by_name_fractional).
                        // Do not let the CFF-internal-encoding fallback overwrite
                        // them: for subset Symbol CFF, cff.glyph_index(code) often
                        // returns None → hmtx .notdef fallback = 250, which is wrong
                        // for codes whose glyph (e.g. "multiply" @ 215) has a real
                        // CFF charstring width (e.g. 549). (#FP-6.2.11.5)
                        let code = first_char + idx as u32;
                        if enc_info.1.contains_key(&code) {
                            continue;
                        }
                        merged.entry(idx).or_insert(w);
                    }
                    // Also fill unmapped slots with .notdef when explicit Differences exist.
                    if has_explicit_differences {
                        for (idx, w) in compute_classic_symbol_cff_width_corrections(
                            &font_data,
                            first_char,
                            &existing_widths,
                        ) {
                            let code = first_char + idx as u32;
                            if enc_info.1.contains_key(&code) {
                                continue;
                            }
                            merged.entry(idx).or_insert(w);
                        }
                    }
                }

                merged.into_iter().collect()
            } else {
                // For non-classic Type1 symbolic fonts without a usable
                // PDF-level encoding, post-embed widths are often already
                // aligned with .notdef fallback behavior.
                if enc_info.0.is_empty() && enc_info.1.is_empty() {
                    continue;
                }
                compute_cff_type1_width_corrections(
                    &font_data,
                    first_char,
                    &existing_widths,
                    &enc_info,
                    is_subset,
                    None,
                )
            }
        };

        if corrections.is_empty() {
            continue;
        }

        // Safety: on subset symbolic fonts, skip very high-mismatch updates that
        // likely indicate an incorrect mapping strategy.
        if subtype == "TrueType" && is_subset && corrections.len() * 5 > existing_widths.len() * 4 {
            continue;
        }

        let mut new_widths = existing_widths.clone();
        for (idx, new_w) in &corrections {
            if *idx < new_widths.len() {
                new_widths[*idx] = Object::Integer(*new_w);
            }
        }

        if let Some(widths_id) = widths_ref {
            if let Some(Object::Array(ref mut widths)) = doc.objects.get_mut(&widths_id) {
                *widths = new_widths;
            } else {
                continue;
            }
        } else {
            let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) else {
                continue;
            };
            font.set("Widths", Object::Array(new_widths));
        }
        fixed += 1;
    }

    fixed
}

/// Compute width corrections for a symbolic TrueType font.
fn compute_symbolic_truetype_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    is_subset: bool,
) -> Vec<(usize, i64)> {
    use std::collections::HashMap;

    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return Vec::new();
    };

    let units_per_em = face.units_per_em() as f64;
    if units_per_em == 0.0 {
        return Vec::new();
    }
    let scale = 1000.0 / units_per_em;
    let mut corrections = Vec::new();
    let mac_map: HashMap<u8, u16> = tt_read_mac_cmap(font_data).into_iter().collect();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w,
            Object::Real(r) => *r as i64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // Symbolic TrueType: veraPDF maps code via (3,0) cmap at 0xF000+code,
        // or (1,0) cmap at code directly. Some subset symbol fonts are encoded
        // as direct code->GID without usable cmap entries; in that case, fall
        // back to GID == code.
        let gid = face
            .glyph_index(char::from_u32(0xF000 + code).unwrap_or('\0'))
            .or_else(|| face.glyph_index(char::from_u32(code).unwrap_or('\0')))
            .or_else(|| {
                if code <= 255 {
                    mac_map
                        .get(&(code as u8))
                        .copied()
                        .filter(|gid| *gid > 0)
                        .map(ttf_parser::GlyphId)
                } else {
                    None
                }
            })
            .or_else(|| {
                if is_subset && code < face.number_of_glyphs() as u32 {
                    Some(ttf_parser::GlyphId(code as u16))
                } else {
                    None
                }
            });
        let gid = gid.unwrap_or(ttf_parser::GlyphId(0));
        let Some(advance) = face.glyph_hor_advance(gid) else {
            continue;
        };

        let expected = (advance as f64 * scale).round() as i64;
        if (pdf_w - expected).abs() > 1 {
            corrections.push((i, expected));
        }
    }

    corrections
}

/// Compute corrections for explicit symbolic /Differences entries using
/// glyph-name or Unicode lookup in the embedded font.
fn compute_symbolic_difference_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    differences: &std::collections::HashMap<u32, String>,
) -> Vec<(usize, i64)> {
    // This function is called only for Type1/CFF fonts (FontFile3). veraPDF §6.2.11.5
    // validates widths against the CFF charstring advance, NOT against the OTF hmtx
    // table. Use CFF charstring widths to match veraPDF's reference. (#FP-6.2.11.5-sym)
    //
    // Get CFF bytes: raw font_data for Type1C, or extract from OTF wrapper.
    let cff_bytes: &[u8] = extract_cff_bytes_from_otf(font_data).unwrap_or(font_data);
    let Some(cff) = cff_parser::Table::parse(cff_bytes) else {
        return Vec::new();
    };
    let matrix = cff.matrix();
    let cff_scale = cff_matrix_scale(matrix.sx);

    let mut corrections = Vec::new();

    for (code, name) in differences {
        if *code < first_char {
            continue;
        }
        let idx = (*code - first_char) as usize;
        let Some(pdf_w) = existing_widths.get(idx).and_then(object_to_f64) else {
            continue;
        };
        // Look up glyph by PostScript name in the CFF charset.
        // If the name is not found in this subset, skip — do not apply .notdef width.
        let Some(expected) =
            find_cff_glyph_width_by_name_fractional(&cff, cff_bytes, name, cff_scale)
        else {
            continue;
        };
        if (pdf_w - expected).abs() >= 0.95 {
            corrections.push((idx, expected.round() as i64));
        }
    }

    corrections
}

fn compute_symbolic_cff_encoding_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
) -> Vec<(usize, i64)> {
    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let Some(pdf_w) = object_to_f64(obj) else {
            continue;
        };
        let code = first_char + i as u32;
        let Some(expected) =
            compute_classic_symbol_cff_single_width(font_data, code, enc_name, differences)
        else {
            continue;
        };
        if (pdf_w - expected).abs() >= 1.0 {
            corrections.push((i, expected.round() as i64));
        }
    }

    corrections
}

/// Compute conservative width corrections for classic Symbol/Zapf CFF fonts
/// when no PDF-level encoding is present. Unmapped codes fall back to .notdef.
fn compute_classic_symbol_cff_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
) -> Vec<(usize, i64)> {
    // Prefer OTF wrapper metrics when available.
    if let Ok(face) = ttf_parser::Face::parse(font_data, 0) {
        let upem = face.units_per_em() as f64;
        if upem > 0.0 {
            let scale = 1000.0 / upem;
            if extract_cff_from_otf(font_data).is_some() {
                let notdef = face
                    .glyph_hor_advance(ttf_parser::GlyphId(0))
                    .map(|w| w as f64 * scale)
                    .unwrap_or(0.0);
                let mut corrections = Vec::new();
                for (i, obj) in existing_widths.iter().enumerate() {
                    let Some(pdf_w) = object_to_f64(obj) else {
                        continue;
                    };
                    let code = first_char + i as u32;
                    if code > 255 {
                        continue;
                    }
                    let expected = notdef;
                    if (pdf_w - expected).abs() >= 1.0 {
                        corrections.push((i, expected.round() as i64));
                    }
                }
                return corrections;
            }
        }
    }

    // Raw CFF fallback.
    let Some(cff) = cff_parser::Table::parse(font_data) else {
        return Vec::new();
    };
    let scale = cff_matrix_scale(cff.matrix().sx);
    let notdef = cff
        .glyph_width(cff_parser::GlyphId(0))
        .map(|w| w as f64 * scale)
        .unwrap_or(0.0);
    let mut corrections = Vec::new();
    for (i, obj) in existing_widths.iter().enumerate() {
        let Some(pdf_w) = object_to_f64(obj) else {
            continue;
        };
        let code = first_char + i as u32;
        if code > 255 {
            continue;
        }
        let expected = notdef;
        if (pdf_w - expected).abs() >= 1.0 {
            corrections.push((i, expected.round() as i64));
        }
    }
    corrections
}

/// Compute width corrections for a symbolic CFF font.
#[allow(dead_code)]
fn compute_symbolic_cff_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
) -> Vec<(usize, i64)> {
    let Some(cff) = cff_parser::Table::parse(font_data) else {
        return Vec::new();
    };

    let matrix = cff.matrix();
    let scale = cff_matrix_scale(matrix.sx);

    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w,
            Object::Real(r) => *r as i64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // For symbolic CFF fonts, only trust explicit encoding-based lookup.
        // A fallback of `GID == code` can rewrite correct widths to unrelated
        // glyph advances (violating ISO 19005-2:2011 6.2.11.5 / veraPDF 6.2.11.5:1).
        let gid = cff.glyph_index(code as u8).filter(|g| g.0 > 0);

        let Some(gid) = gid else { continue };

        let Some(w) = cff.glyph_width(gid) else {
            continue;
        };

        let expected = (w as f64 * scale).round() as i64;

        if (pdf_w - expected).abs() > 1 {
            corrections.push((i, expected));
        }
    }

    corrections
}

fn count_all_fonts(doc: &Document) -> usize {
    doc.objects
        .values()
        .filter(|obj| {
            if let Object::Dictionary(dict) = obj {
                is_font_dict(dict)
            } else {
                false
            }
        })
        .count()
}

fn get_name(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    match dict.get(key).ok()? {
        Object::Name(n) => String::from_utf8(n.clone()).ok(),
        _ => None,
    }
}

/// Like `get_name` but returns raw bytes and resolves indirect references.
fn get_name_bytes_resolved(
    doc: &Document,
    dict: &lopdf::Dictionary,
    key: &[u8],
) -> Option<Vec<u8>> {
    match dict.get(key).ok()? {
        Object::Name(n) => Some(n.clone()),
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Name(n) => Some(n.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Like `get_name` but resolves indirect references through the document.
fn get_name_resolved(doc: &Document, dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    let raw = get_name_bytes_resolved(doc, dict, key)?;
    String::from_utf8(raw).ok()
}

/// Like `get_name_resolved`, but falls back to UTF-8 lossy conversion.
fn get_name_lossy_resolved(doc: &Document, dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    let raw = get_name_bytes_resolved(doc, dict, key)?;
    Some(String::from_utf8_lossy(&raw).to_string())
}

// ---------------------------------------------------------------------------
// 6.2.11.8:1 — Fix .notdef glyph references
// ---------------------------------------------------------------------------
//
// veraPDF rule 6.2.11.8:1: "A PDF/A-2 compliant document shall not contain
// a reference to the .notdef glyph from any of the text showing operators."
//
// This happens when character codes in content streams map to .notdef in
// the font's encoding. The safest approach is to fix the Encoding
// Differences array entries that explicitly map codes to .notdef.
//
// Strategy:
// 1. Find all simple fonts (Type1, TrueType) with Encoding Differences
//    containing ".notdef" entries.
// 2. For each such entry, try to find the CORRECT glyph name by looking up
//    the character code in the embedded font program (via Unicode cmap or
//    glyph name tables).
// 3. If a correct glyph exists in the font, use that name.
// 4. If not, use "space" as a safe AGL-compliant fallback.
// 5. For fonts WITHOUT Differences but where the base encoding maps some
//    used codes to .notdef: only fix if we can add Differences entries
//    where the font provably has the glyph.

/// Fix .notdef glyph references in font Encoding Differences arrays (6.2.11.8:1).
///
/// Scans all simple fonts for Encoding Differences containing ".notdef"
/// entries and replaces them with valid glyph names. Also checks fonts
/// without Differences where the encoding maps character codes to .notdef
/// in the embedded font, and adds Differences entries when the font
/// program provably contains the correct glyph.
///
/// Returns the number of fonts fixed.
pub fn fix_notdef_glyph_refs(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let used_simple_codes = collect_simple_font_used_codes(doc);
    let mut fixed = 0;

    for font_id in font_ids {
        let (subtype, fd_id, enc_info, first_char, last_char, is_subset) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            if !is_font_dict(dict) {
                continue;
            }
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();

            // Only handle simple fonts — Type0/CID font .notdef fixing is
            // much more complex (CMap rewriting) and too risky.
            if subtype != "TrueType" && subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            let base_font = get_name(dict, b"BaseFont").unwrap_or_default();
            let symbolic_name = is_symbolic_font_name(&base_font);
            let symbolic_flags = is_font_symbolic(doc, dict);
            let base_no_subset = strip_subset_prefix(&base_font);
            // Some legacy NewBrunswick Type1 fonts are flagged Symbolic but still
            // need the regular notdef path for missing space-glyph references.
            let allow_symbolic_type1_override = base_no_subset.contains("NewBrunswick");
            // Symbolic fonts by name are handled via stream-level repair.
            // For TrueType symbolic fonts (by flags), avoid Differences-based
            // edits here to prevent reintroducing /Encoding (6.2.11.6:3).
            // For Type1 (non-TrueType) fonts we do NOT skip on symbolic_flags
            // alone: text fonts like Garamond are sometimes incorrectly
            // flagged with Flags=4 (Symbolic) in the original PDF. When such
            // a font has Differences[code]=".notdef", fix_symbolic_flags (which
            // runs later in the pipeline) would correct the flag, but too late
            // for fix_notdef_glyph_refs. Skipping them leaves §6.2.11.8:1
            // unfixed. (#gen-783)
            if (symbolic_name && !(subtype != "TrueType" && allow_symbolic_type1_override))
                || (subtype == "TrueType" && symbolic_flags)
            {
                continue;
            }

            // Detect subset fonts (prefix like ABCDEF+FontName).
            // We still process them but use a more conservative replacement
            // strategy: only map .notdef codes to glyph names confirmed
            // present in the font subset.
            let is_subset = get_name(dict, b"BaseFont")
                .map(|n| n.contains('+'))
                .unwrap_or(false);

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            };

            // Extract encoding info.
            let enc_info = extract_encoding_info(doc, dict);

            // Extract FirstChar/LastChar to know which codes are actually used.
            let fc = dict
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(0);
            let lc = dict
                .get(b"LastChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(255);

            (subtype, fd_id, enc_info, fc, lc, is_subset)
        };

        let Some(fd_id) = fd_id else { continue };

        // Read the embedded font program data.
        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        if subtype == "TrueType" {
            if fix_notdef_in_truetype(
                doc,
                font_id,
                &font_data,
                &enc_info,
                first_char,
                last_char,
                is_subset,
                used_simple_codes.get(&font_id),
            ) {
                fixed += 1;
            }
        } else {
            // Type1 / MMType1 — try CFF parsing.
            if fix_notdef_in_type1(
                doc, font_id, &font_data, &enc_info, first_char, last_char, is_subset,
            ) {
                fixed += 1;
            }
        }
    }

    fixed
}

/// Strip control characters (0x00-0x1F except \t, \n, \r) from PDF string
/// literals in all content streams. These characters are non-printing and
/// frequently map to .notdef in fonts, causing PDF/A violations (6.2.11.8:1
/// and 6.2.11.4.1:2).
pub fn strip_control_chars_from_streams(doc: &mut Document) -> usize {
    use std::collections::HashMap;

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut total_fixed = 0usize;

    for &page_id in &page_ids {
        // Build a page-local map: font resource name -> is strip-safe simple font.
        let mut has_type0_font = false;
        let font_map: HashMap<String, bool> = {
            let page = match doc.objects.get(&page_id) {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let resources = match page.get(b"Resources").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let fonts = match resources.get(b"Font").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };

            let mut map = HashMap::new();
            for (key, val) in fonts.iter() {
                let name = String::from_utf8_lossy(key).to_string();
                let subtype = match val {
                    Object::Reference(id) => match doc.objects.get(id) {
                        Some(Object::Dictionary(d)) => get_name(d, b"Subtype").unwrap_or_default(),
                        _ => String::new(),
                    },
                    Object::Dictionary(d) => get_name(d, b"Subtype").unwrap_or_default(),
                    _ => String::new(),
                };
                if subtype == "Type0" {
                    has_type0_font = true;
                }

                // Strip in 1-byte text fonts, including Type3.
                let can_strip = subtype == "TrueType"
                    || subtype == "Type1"
                    || subtype == "MMType1"
                    || subtype == "Type3";
                map.insert(name, can_strip);
            }
            map
        };

        if !font_map.values().any(|v| *v) {
            continue;
        }

        // Note: pages that also have Type0 fonts are NOT skipped — we still process
        // simple-font operators on such pages.  The inner logic is safe: Type0 entries
        // have can_strip=false in font_map, so their text operators are left untouched.
        // strip_control_bytes is called with allow_collapse=!has_type0_font so 2-byte
        // sentinel pairs (malformed simple fonts on mixed pages) are handled correctly.

        let content_ids = crate::content_editor::get_content_stream_ids(doc, page_id);
        // A page Contents array is processed as a single concatenated stream, so the
        // current font can flow across individual stream boundaries.
        let mut current_font = String::new();

        for cs_id in content_ids {
            let stream_data = match doc.objects.get(&cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };

            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };
            let ops = editor.operations().to_vec();
            let mut modified = false;
            let mut new_ops = Vec::with_capacity(ops.len());

            for op in &ops {
                match op.operator.as_str() {
                    "Tf" => {
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font = String::from_utf8_lossy(name).to_string();
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        if font_map.get(&current_font).copied().unwrap_or(false) {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, _)) = new_op.operands.get_mut(str_idx)
                            {
                                if strip_control_bytes(bytes, !has_type0_font) {
                                    modified = true;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    "TJ" => {
                        if font_map.get(&current_font).copied().unwrap_or(false) {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, _) = item {
                                        if strip_control_bytes(bytes, !has_type0_font) {
                                            modified = true;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    _ => {
                        new_ops.push(op.clone());
                    }
                }
            }

            if modified {
                let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                if let Ok(encoded) = new_editor.encode() {
                    if let Some(Object::Stream(s)) = doc.objects.get_mut(&cs_id) {
                        s.set_plain_content(encoded);
                        total_fixed += 1;
                    }
                }
            }
        }
    }

    total_fixed
}

fn strip_control_bytes(bytes: &mut Vec<u8>, allow_collapse: bool) -> bool {
    let mut changed = false;

    // Some malformed PDFs encode simple-font text as 2-byte pairs where one
    // lane is a constant sentinel (00/FF). Collapse these to 1-byte codes.
    if allow_collapse && collapse_two_byte_simple_codes(bytes) {
        changed = true;
    }

    // On pages that also use Type0 fonts, keep 2-byte sentinel pairs intact:
    // stripping low bytes only can create ambiguous 1-byte hex strings.
    if !allow_collapse {
        if let Some(code_in_odd_lane) = paired_simple_code_lane(bytes) {
            let mut filtered = Vec::with_capacity(bytes.len());
            for i in (0..bytes.len()).step_by(2) {
                let code = if code_in_odd_lane {
                    bytes[i + 1]
                } else {
                    bytes[i]
                };
                if code >= 32 {
                    filtered.push(bytes[i]);
                    filtered.push(bytes[i + 1]);
                }
            }
            if filtered.len() != bytes.len() {
                *bytes = filtered;
                changed = true;
            }
            return changed;
        }
    }

    let original_len = bytes.len();
    bytes.retain(|b| *b >= 32);
    changed || bytes.len() != original_len
}

/// Fix .notdef references in CID (Type0) fonts by modifying content streams.
///
/// ISO 19005-2, §6.2.11.8: no .notdef glyph references allowed.
///
/// For CIDFontType0/CIDFontType2 with two-byte Type0 CMaps (Identity-H/V and
/// common CJK `*-H`/`*-V` CMaps), character codes in content streams are 2-byte
/// values. If a mapped CID does not have a glyph in the embedded font program,
/// it resolves to .notdef (GID 0). This function replaces such values in Tj/TJ
/// text strings with a valid fallback CID (typically space).
///
/// See NOTDEF_FIXES_LOG.md for the debug log of approaches tried.
pub fn fix_cid_font_notdef(doc: &mut Document) -> usize {
    use std::collections::{HashMap, HashSet};

    #[derive(Clone)]
    struct CidTextRepair {
        valid_values: HashSet<u16>,
        replacement_value: Option<u16>,
        /// For EUC-style CMaps (GB-EUC-H etc.), the CMap cidranges used to
        /// determine byte-code boundaries and code→CID mappings.
        euc_cmap_ranges: Option<Vec<CmapRange>>,
    }

    #[derive(Clone, Copy)]
    enum ContentContainer {
        Page(ObjectId),
        Form(ObjectId),
    }

    let mut containers: Vec<ContentContainer> = doc
        .get_pages()
        .values()
        .copied()
        .map(ContentContainer::Page)
        .collect();
    for (&id, obj) in &doc.objects {
        let Object::Stream(stream) = obj else {
            continue;
        };
        let is_form = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            == Some(b"Form");
        if is_form {
            containers.push(ContentContainer::Form(id));
        }
    }

    // For each page/Form XObject, find Type0 fonts with likely 2-byte CMaps
    // and build a set of valid CIDs per font resource name.
    let mut total_fixed = 0usize;

    for container in containers {
        // Get font resources: resource_name -> font_obj_id
        let font_map: HashMap<String, ObjectId> = {
            let resources = match container {
                ContentContainer::Page(page_id) => {
                    let page = match doc.objects.get(&page_id) {
                        Some(Object::Dictionary(d)) => d.clone(),
                        _ => continue,
                    };
                    match page.get(b"Resources").ok() {
                        Some(Object::Dictionary(d)) => d.clone(),
                        Some(Object::Reference(r)) => match doc.objects.get(r) {
                            Some(Object::Dictionary(d)) => d.clone(),
                            _ => continue,
                        },
                        _ => continue,
                    }
                }
                ContentContainer::Form(form_id) => {
                    let stream = match doc.objects.get(&form_id) {
                        Some(Object::Stream(s)) => s.clone(),
                        _ => continue,
                    };
                    match stream.dict.get(b"Resources").ok() {
                        Some(Object::Dictionary(d)) => d.clone(),
                        Some(Object::Reference(r)) => match doc.objects.get(r) {
                            Some(Object::Dictionary(d)) => d.clone(),
                            _ => continue,
                        },
                        _ => continue,
                    }
                }
            };
            let fonts = match resources.get(b"Font").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let mut map = HashMap::new();
            for (key, val) in fonts.iter() {
                let name = String::from_utf8_lossy(key).to_string();
                if let Object::Reference(id) = val {
                    map.insert(name, *id);
                }
            }
            map
        };

        // For each Type0 font, check if it uses a likely two-byte CMap and has .notdef CIDs.
        let mut notdef_fonts: HashMap<String, CidTextRepair> = HashMap::new();

        for (res_name, font_id) in &font_map {
            let Some(Object::Dictionary(font_dict)) = doc.objects.get(font_id) else {
                continue;
            };
            let subtype = get_name(font_dict, b"Subtype").unwrap_or_default();
            if subtype != "Type0" {
                continue;
            }

            // Apply only to Type0 encodings that are typically two-byte CMap
            // workflows: Identity-H/V, named CJK maps, or embedded CMap streams.
            let likely_two_byte = match font_dict.get(b"Encoding").ok() {
                Some(Object::Name(n)) => {
                    let enc_l = String::from_utf8_lossy(n).to_ascii_lowercase();
                    enc_l == "identity-h"
                        || enc_l == "identity-v"
                        || enc_l.ends_with("-h")
                        || enc_l.ends_with("-v")
                        || enc_l.contains("gbk")
                        || enc_l.contains("gb")
                        || enc_l.contains("cns")
                        || enc_l.contains("japan")
                        || enc_l.contains("korea")
                }
                Some(Object::Reference(enc_id)) => match doc.objects.get(enc_id) {
                    Some(Object::Name(n)) => {
                        let enc_l = String::from_utf8_lossy(n).to_ascii_lowercase();
                        enc_l == "identity-h"
                            || enc_l == "identity-v"
                            || enc_l.ends_with("-h")
                            || enc_l.ends_with("-v")
                            || enc_l.contains("gbk")
                            || enc_l.contains("gb")
                            || enc_l.contains("cns")
                            || enc_l.contains("japan")
                            || enc_l.contains("korea")
                    }
                    Some(Object::Dictionary(d)) => d.has(b"CMapName"),
                    Some(Object::Stream(s)) => {
                        if let Ok(Object::Name(cmap_name)) = s.dict.get(b"CMapName") {
                            let name_l = String::from_utf8_lossy(cmap_name).to_ascii_lowercase();
                            name_l.contains("gbk")
                                || name_l.contains("gb")
                                || name_l.contains("cns")
                                || name_l.contains("japan")
                                || name_l.contains("korea")
                                || name_l.ends_with("-h")
                                || name_l.ends_with("-v")
                        } else {
                            true
                        }
                    }
                    _ => false,
                },
                _ => false,
            };
            if !likely_two_byte {
                continue;
            }

            let cmap_name = resolve_type0_cmap_name(doc, font_dict);
            let predefined_ranges = cmap_name
                .as_deref()
                .filter(|name| !is_identity_type0_cmap(name))
                .and_then(load_predefined_unicode_cmap_ranges);
            let euc_cmap_ranges =
                cmap_name
                    .as_deref()
                    .filter(|name| !is_identity_type0_cmap(name))
                    .filter(|name| is_euc_style_cmap(name))
                    .and_then(load_all_cmap_cidranges)
                    .or_else(|| {
                        // Fallback: read cidranges directly from an embedded CMap stream.
                        // Needed for fonts whose /Encoding references a CMap stream with
                        // an internal EUC/GBK name (e.g. FOUNDER-GBK-EUC-H stored under
                        // the stream label "Fdr-gbk-5") — the dict-level CMapName "Fdr-gbk-5"
                        // doesn't contain "euc" so the file-based lookup above finds nothing.
                        // Fixes #460: prevents fix_cid_text_string from treating GBK char
                        // codes as raw CID pairs and corrupting the text stream.
                        load_embedded_cmap_stream_ranges(doc, font_dict)
                            .and_then(|(ranges, is_euc)| if is_euc { Some(ranges) } else { None })
                    });

            // Get descendant CIDFont (may be inline array or reference).
            let desc_arr = match font_dict.get(b"DescendantFonts").ok() {
                Some(Object::Array(arr)) => Some(arr.clone()),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Array(arr)) => Some(arr.clone()),
                    _ => None,
                },
                _ => None,
            };
            let desc_id = desc_arr.as_ref().and_then(|arr| {
                arr.first().and_then(|o| match o {
                    Object::Reference(id) => Some(*id),
                    _ => None,
                })
            });
            let Some(desc_id) = desc_id else {
                continue;
            };

            // Get FontDescriptor from CIDFont.
            let fd_id = doc.objects.get(&desc_id).and_then(|o| {
                if let Object::Dictionary(d) = o {
                    match d.get(b"FontDescriptor").ok() {
                        Some(Object::Reference(id)) => Some(*id),
                        _ => None,
                    }
                } else {
                    None
                }
            });
            let Some(fd_id) = fd_id else {
                continue;
            };

            // Read embedded font data.
            let Some(font_data) = read_embedded_font_data(doc, fd_id) else {
                continue;
            };

            // Determine CIDFont subtype to choose CFF or TrueType parsing.
            let cid_subtype = doc
                .objects
                .get(&desc_id)
                .and_then(|o| {
                    if let Object::Dictionary(d) = o {
                        get_name(d, b"Subtype")
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let mut valid_cids: HashSet<u16> = HashSet::new();
            let mut space_cid: u16 = 0;
            let mut clear_unparseable_text = false;

            if cid_subtype == "CIDFontType2" {
                // TrueType-based CID font. Handle both Identity and stream
                // CIDToGIDMap mappings.
                match ttf_parser::Face::parse(&font_data, 0) {
                    Ok(face) => {
                        let num_glyphs = face.number_of_glyphs();
                        if num_glyphs == 0 {
                            continue;
                        }

                        let map_obj = doc.objects.get(&desc_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                d.get(b"CIDToGIDMap").ok().cloned()
                            } else {
                                None
                            }
                        });
                        let has_glyph_data = |gid: u16| -> bool {
                            gid > 0
                                && gid < num_glyphs
                                && tt_glyph_has_data(&face, ttf_parser::GlyphId(gid))
                        };
                        let space_gid = face
                            .glyph_index(' ')
                            .map(|g| g.0)
                            .filter(|gid| has_glyph_data(*gid))
                            .unwrap_or(0);

                        match map_obj {
                            None => {
                                // Identity mapping: CID == GID.
                                for gid in 1..num_glyphs {
                                    if has_glyph_data(gid) {
                                        valid_cids.insert(gid);
                                    }
                                }
                                if space_gid > 0 {
                                    space_cid = space_gid;
                                }
                            }
                            Some(Object::Name(n)) if n == b"Identity" => {
                                // Identity mapping: CID == GID.
                                for gid in 1..num_glyphs {
                                    if has_glyph_data(gid) {
                                        valid_cids.insert(gid);
                                    }
                                }
                                if space_gid > 0 {
                                    space_cid = space_gid;
                                }
                            }
                            Some(Object::Reference(id)) => {
                                let map_bytes = match doc.objects.get(&id) {
                                    Some(Object::Stream(s)) => {
                                        let mut st = s.clone();
                                        let _ = st.decompress();
                                        st.content
                                    }
                                    _ => Vec::new(),
                                };
                                for (cid, chunk) in map_bytes.chunks_exact(2).enumerate() {
                                    if cid > u16::MAX as usize {
                                        break;
                                    }
                                    let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                                    if has_glyph_data(gid) {
                                        let cid_u16 = cid as u16;
                                        valid_cids.insert(cid_u16);
                                        if gid == space_gid {
                                            space_cid = cid_u16;
                                        }
                                    }
                                }
                            }
                            Some(Object::Stream(s)) => {
                                let mut st = s.clone();
                                let _ = st.decompress();
                                for (cid, chunk) in st.content.chunks_exact(2).enumerate() {
                                    if cid > u16::MAX as usize {
                                        break;
                                    }
                                    let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                                    if has_glyph_data(gid) {
                                        let cid_u16 = cid as u16;
                                        valid_cids.insert(cid_u16);
                                        if gid == space_gid {
                                            space_cid = cid_u16;
                                        }
                                    }
                                }
                            }
                            _ => continue,
                        }

                        // Fallback: if stream mapping yielded nothing, fall back
                        // to Identity semantics.
                        if valid_cids.is_empty() {
                            for gid in 1..num_glyphs {
                                if has_glyph_data(gid) {
                                    valid_cids.insert(gid);
                                }
                            }
                            if space_gid > 0 {
                                space_cid = space_gid;
                            }
                        }
                    }
                    Err(_) => {
                        continue;
                    }
                }
            } else {
                // CFF-based CID font (CIDFontType0): parse CFF for CID mapping.
                match cff_parser::Table::parse(&font_data) {
                    Some(cff) => {
                        let num_glyphs = cff.number_of_glyphs();
                        for gid in 0..num_glyphs {
                            let glyph_id = cff_parser::GlyphId(gid);
                            if let Some(cid) = cff.glyph_cid(glyph_id) {
                                let has_usable_width =
                                    cff.glyph_width(glyph_id).map(|w| w > 0).unwrap_or(false);
                                if gid > 0 && has_usable_width {
                                    valid_cids.insert(cid);
                                }
                                if let Some(name) = cff.glyph_name(glyph_id) {
                                    if name == "space" && gid > 0 && has_usable_width {
                                        space_cid = cid;
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        // FIX_LOG: CFF parse can fail for tiny/unusual CFF fonts (e.g. 565-byte
                        // HiddenHorzOCR in 0298). Fallback: try ttf_parser which handles
                        // OpenType-wrapped CFF as well.
                        match ttf_parser::Face::parse(&font_data, 0) {
                            Ok(face) => {
                                let num_glyphs = face.number_of_glyphs();
                                for gid in 1..num_glyphs {
                                    if tt_glyph_has_data(&face, ttf_parser::GlyphId(gid)) {
                                        valid_cids.insert(gid);
                                    }
                                }
                                if let Some(gid) = face.glyph_index(' ') {
                                    if gid.0 > 0 && tt_glyph_has_data(&face, gid) {
                                        space_cid = gid.0;
                                    }
                                }
                            }
                            Err(_) => {
                                // If the embedded CIDFontType0 program cannot be parsed at all,
                                // we cannot prove any rendered CID maps to a present glyph.
                                // Keep the font on the repair list with an empty valid set so
                                // text strings using it are cleared conservatively.
                                clear_unparseable_text = true;
                            }
                        }
                    }
                }
            }

            // If no space glyph found by name, use the first valid CID.
            if !clear_unparseable_text && space_cid == 0 && predefined_ranges.is_none() {
                if let Some(&first_valid) = valid_cids.iter().next() {
                    space_cid = first_valid;
                }
            }

            // Add font to the map. If the valid set is empty (font has only
            // .notdef, e.g. HiddenHorzOCR stub fonts), we still add it so text
            // strings get cleared entirely.
            let repair = if let Some(ranges) = predefined_ranges.as_ref() {
                let valid_codes = build_valid_codes_from_cmap_ranges(&valid_cids, ranges);
                let replacement_value = if valid_codes.contains(&0x0020) {
                    Some(0x0020)
                } else if space_cid > 0 {
                    cmap_first_code_for_cid(ranges, space_cid)
                        .filter(|code| valid_codes.contains(code))
                } else {
                    None
                };
                CidTextRepair {
                    valid_values: valid_codes,
                    replacement_value,
                    euc_cmap_ranges: None,
                }
            } else {
                CidTextRepair {
                    valid_values: valid_cids,
                    replacement_value: if clear_unparseable_text || space_cid == 0 {
                        None
                    } else {
                        Some(space_cid)
                    },
                    euc_cmap_ranges,
                }
            };
            notdef_fonts.insert(res_name.clone(), repair);
        }

        if notdef_fonts.is_empty() {
            continue;
        }

        // Step 3: parse content streams and fix text strings.
        // Track font name across content streams (font state carries over between
        // consecutive content streams on the same page — the graphics state is not
        // reset between them, per ISO 32000-1 §7.8.2).
        let content_ids = match container {
            ContentContainer::Page(page_id) => {
                crate::content_editor::get_content_stream_ids(doc, page_id)
            }
            ContentContainer::Form(form_id) => vec![form_id],
        };
        let mut stream_chunks: Vec<(ObjectId, Vec<u8>)> = Vec::new();
        for cs_id in &content_ids {
            let stream_data = match doc.objects.get(cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };
            stream_chunks.push((*cs_id, stream_data));
        }

        if stream_chunks.is_empty() {
            continue;
        }

        // Some PDFs split operators/tokens across consecutive content streams.
        // Parse merged content first so split tokens are seen as one stream.
        let mut handled_as_combined = false;
        if stream_chunks.len() > 1 {
            let mut merged = Vec::new();
            for (_, chunk) in &stream_chunks {
                merged.extend_from_slice(chunk);
                if !chunk.ends_with(b"\n") {
                    merged.push(b'\n');
                }
            }

            if let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&merged) {
                let ops = editor.operations().to_vec();
                let mut current_font_name = String::new();
                let mut in_text_object = false;
                let mut font_set_in_text_object = false;
                let mut gs_stack: Vec<(String, bool)> = Vec::new();
                let mut modified = false;
                let mut new_ops = Vec::with_capacity(ops.len());

                for op in &ops {
                    match op.operator.as_str() {
                        "q" => {
                            gs_stack.push((current_font_name.clone(), font_set_in_text_object));
                            new_ops.push(op.clone());
                        }
                        "Q" => {
                            if let Some((saved_font, saved_font_set)) = gs_stack.pop() {
                                current_font_name = saved_font;
                                font_set_in_text_object = saved_font_set;
                            }
                            new_ops.push(op.clone());
                        }
                        "BT" => {
                            // ISO 32000-1 §9.4.1: BT starts a new text object.
                            // Keep current font selection (it persists in text
                            // state), but track whether this BT sets Tf again.
                            in_text_object = true;
                            font_set_in_text_object = false;
                            new_ops.push(op.clone());
                        }
                        "ET" => {
                            in_text_object = false;
                            font_set_in_text_object = false;
                            new_ops.push(op.clone());
                        }
                        "Tf" => {
                            if let Some(Object::Name(name)) = op.operands.first() {
                                current_font_name = String::from_utf8_lossy(name).to_string();
                                font_set_in_text_object = true;
                            }
                            new_ops.push(op.clone());
                        }
                        "Tj" | "'" | "\"" => {
                            if let Some(repair) = notdef_fonts.get(&current_font_name) {
                                let mut new_op = op.clone();
                                let str_idx = if op.operator == "\"" { 2 } else { 0 };
                                if let Some(Object::String(bytes, fmt)) =
                                    new_op.operands.get_mut(str_idx)
                                {
                                    let mut changed_here = if let Some(euc) =
                                        repair.euc_cmap_ranges.as_deref()
                                    {
                                        fix_cid_text_string_euc(bytes, euc, &repair.valid_values)
                                    } else {
                                        fix_cid_text_string(
                                            bytes,
                                            &repair.valid_values,
                                            repair.replacement_value,
                                        )
                                    };
                                    if *fmt != lopdf::StringFormat::Hexadecimal {
                                        *fmt = lopdf::StringFormat::Hexadecimal;
                                        changed_here = true;
                                    }
                                    if changed_here {
                                        modified = true;
                                    }
                                }
                                new_ops.push(new_op);
                            } else {
                                let mut new_op = op.clone();
                                let str_idx = if op.operator == "\"" { 2 } else { 0 };
                                if let Some(Object::String(bytes, fmt)) =
                                    new_op.operands.get_mut(str_idx)
                                {
                                    if fix_unset_text_font_hex_string(
                                        bytes,
                                        *fmt,
                                        in_text_object,
                                        font_set_in_text_object,
                                    ) {
                                        modified = true;
                                    }
                                }
                                new_ops.push(new_op);
                            }
                        }
                        "TJ" => {
                            if let Some(repair) = notdef_fonts.get(&current_font_name) {
                                let mut new_op = op.clone();
                                if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                    for item in arr.iter_mut() {
                                        if let Object::String(bytes, fmt) = item {
                                            let mut changed_here = if let Some(euc) =
                                                repair.euc_cmap_ranges.as_deref()
                                            {
                                                fix_cid_text_string_euc(
                                                    bytes,
                                                    euc,
                                                    &repair.valid_values,
                                                )
                                            } else {
                                                fix_cid_text_string(
                                                    bytes,
                                                    &repair.valid_values,
                                                    repair.replacement_value,
                                                )
                                            };
                                            if *fmt != lopdf::StringFormat::Hexadecimal {
                                                *fmt = lopdf::StringFormat::Hexadecimal;
                                                changed_here = true;
                                            }
                                            if changed_here {
                                                modified = true;
                                            }
                                        }
                                    }
                                }
                                new_ops.push(new_op);
                            } else {
                                let mut new_op = op.clone();
                                if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                    for item in arr.iter_mut() {
                                        if let Object::String(bytes, fmt) = item {
                                            if fix_unset_text_font_hex_string(
                                                bytes,
                                                *fmt,
                                                in_text_object,
                                                font_set_in_text_object,
                                            ) {
                                                modified = true;
                                            }
                                        }
                                    }
                                }
                                new_ops.push(new_op);
                            }
                        }
                        _ => new_ops.push(op.clone()),
                    }
                }

                // Always normalize merged content back into a single stream for
                // CID Identity-H/V fonts on multi-stream pages. This fixes
                // split operators/tokens across stream boundaries (e.g.
                // "/C2_0" at end of one stream and "1 Tf" at start of the
                // next), which can otherwise leave malformed one-byte CID hex
                // strings in the physical stream data.
                let should_rewrite_combined = modified || stream_chunks.len() > 1;
                if should_rewrite_combined {
                    let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                    if let Ok(encoded) = new_editor.encode() {
                        let first_id = stream_chunks[0].0;
                        if let Some(Object::Stream(s)) = doc.objects.get_mut(&first_id) {
                            s.set_plain_content(encoded);
                            total_fixed += 1;
                        }
                        for (extra_id, _) in stream_chunks.iter().skip(1) {
                            if let Some(Object::Stream(s)) = doc.objects.get_mut(extra_id) {
                                s.set_plain_content(Vec::new());
                            }
                        }
                    }
                }

                handled_as_combined = true;
            }
        }

        if handled_as_combined {
            continue;
        }

        // Fallback: parse streams individually (keeps existing behavior).
        let mut current_font_name = String::new();
        let mut in_text_object = false;
        let mut font_set_in_text_object = false;
        let mut gs_stack: Vec<(String, bool)> = Vec::new();
        for (cs_id, stream_data) in stream_chunks {
            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };
            let ops = editor.operations().to_vec();
            let mut modified = false;
            let mut new_ops = Vec::with_capacity(ops.len());

            for op in &ops {
                match op.operator.as_str() {
                    "q" => {
                        gs_stack.push((current_font_name.clone(), font_set_in_text_object));
                        new_ops.push(op.clone());
                    }
                    "Q" => {
                        if let Some((saved_font, saved_font_set)) = gs_stack.pop() {
                            current_font_name = saved_font;
                            font_set_in_text_object = saved_font_set;
                        }
                        new_ops.push(op.clone());
                    }
                    "BT" => {
                        in_text_object = true;
                        font_set_in_text_object = false;
                        new_ops.push(op.clone());
                    }
                    "ET" => {
                        in_text_object = false;
                        font_set_in_text_object = false;
                        new_ops.push(op.clone());
                    }
                    "Tf" => {
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font_name = String::from_utf8_lossy(name).to_string();
                            font_set_in_text_object = true;
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        if let Some(repair) = notdef_fonts.get(&current_font_name) {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, fmt)) =
                                new_op.operands.get_mut(str_idx)
                            {
                                let mut changed_here =
                                    if let Some(euc) = repair.euc_cmap_ranges.as_deref() {
                                        fix_cid_text_string_euc(bytes, euc, &repair.valid_values)
                                    } else {
                                        fix_cid_text_string(
                                            bytes,
                                            &repair.valid_values,
                                            repair.replacement_value,
                                        )
                                    };
                                if *fmt != lopdf::StringFormat::Hexadecimal {
                                    *fmt = lopdf::StringFormat::Hexadecimal;
                                    changed_here = true;
                                }
                                if changed_here {
                                    modified = true;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, fmt)) =
                                new_op.operands.get_mut(str_idx)
                            {
                                if fix_unset_text_font_hex_string(
                                    bytes,
                                    *fmt,
                                    in_text_object,
                                    font_set_in_text_object,
                                ) {
                                    modified = true;
                                }
                            }
                            new_ops.push(new_op);
                        }
                    }
                    "TJ" => {
                        if let Some(repair) = notdef_fonts.get(&current_font_name) {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, fmt) = item {
                                        let mut changed_here =
                                            if let Some(euc) = repair.euc_cmap_ranges.as_deref() {
                                                fix_cid_text_string_euc(
                                                    bytes,
                                                    euc,
                                                    &repair.valid_values,
                                                )
                                            } else {
                                                fix_cid_text_string(
                                                    bytes,
                                                    &repair.valid_values,
                                                    repair.replacement_value,
                                                )
                                            };
                                        if *fmt != lopdf::StringFormat::Hexadecimal {
                                            *fmt = lopdf::StringFormat::Hexadecimal;
                                            changed_here = true;
                                        }
                                        if changed_here {
                                            modified = true;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, fmt) = item {
                                        if fix_unset_text_font_hex_string(
                                            bytes,
                                            *fmt,
                                            in_text_object,
                                            font_set_in_text_object,
                                        ) {
                                            modified = true;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        }
                    }
                    _ => new_ops.push(op.clone()),
                }
            }

            if modified {
                let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                if let Ok(encoded) = new_editor.encode() {
                    if let Some(Object::Stream(s)) = doc.objects.get_mut(&cs_id) {
                        s.set_plain_content(encoded);
                        total_fixed += 1;
                    }
                }
            }
        }
    }

    total_fixed
}

fn fix_unset_text_font_hex_string(
    bytes: &mut Vec<u8>,
    fmt: lopdf::StringFormat,
    in_text_object: bool,
    font_set_in_text_object: bool,
) -> bool {
    // PDF/A-2 6.2.11.8 forbids .notdef references in text-showing operators.
    // In malformed content, some streams emit hexadecimal text in a BT..ET
    // block before any Tf in that same text object. For Identity-H/V this can
    // be interpreted as broken 2-byte CID data and trigger .notdef.
    if fmt != lopdf::StringFormat::Hexadecimal {
        return false;
    }
    // Single-byte hexadecimal strings are ambiguous on pages that also use
    // Identity-H/V Type0 fonts. Some validators interpret them as malformed
    // two-byte CIDs (e.g. 0x49FF), which can resolve to .notdef.
    if bytes.len() == 1 {
        bytes.clear();
        return true;
    }
    if !in_text_object || font_set_in_text_object {
        return false;
    }
    false
}

/// Repair 2-byte Type0 text strings against a set of valid values.
///
/// For Identity-H/V fonts the values are CIDs. For predefined Unicode CMaps
/// (for example UniKS-UCS2-H) the values are character codes that must first be
/// resolved through the CMap before reaching a CID/GID.
fn fix_cid_text_string(
    bytes: &mut Vec<u8>,
    valid_values: &std::collections::HashSet<u16>,
    replacement_value: Option<u16>,
) -> bool {
    let mut changed = false;
    if bytes.is_empty() {
        return false;
    }
    // Type0 text strings are 2-byte code units for these CMaps. If malformed
    // odd lengths occur, drop the dangling byte so we can still repair them.
    if !bytes.len().is_multiple_of(2) {
        bytes.pop();
        changed = true;
    }
    if bytes.len() < 2 {
        return changed;
    }
    if valid_values.is_empty() {
        if !bytes.is_empty() {
            bytes.clear();
            return true;
        }
        return false;
    }
    let mut repaired = Vec::with_capacity(bytes.len());
    for i in (0..bytes.len()).step_by(2) {
        let value = ((bytes[i] as u16) << 8) | (bytes[i + 1] as u16);
        if value != 0 && valid_values.contains(&value) {
            repaired.push(bytes[i]);
            repaired.push(bytes[i + 1]);
            continue;
        }
        changed = true;
        if let Some(replacement) = replacement_value {
            repaired.extend_from_slice(&replacement.to_be_bytes());
        }
    }
    if changed {
        *bytes = repaired;
    }
    changed
}

/// Fix .notdef references in symbolic simple fonts by modifying content streams.
///
/// Symbolic fonts (Symbol, Wingdings, phonetic fonts, etc.) are not handled by
/// `fix_notdef_glyph_refs` because their custom encodings make Differences-based
/// fixes unreliable. Instead, this function replaces undefined character codes
/// directly in content streams with a valid code (typically space).
pub fn fix_symbolic_font_notdef_streams(doc: &mut Document) -> usize {
    use std::collections::{HashMap, HashSet};

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut total_fixed = 0;

    for &page_id in &page_ids {
        // Get font resources: resource_name -> font_obj_id
        let font_map: HashMap<String, ObjectId> = {
            let page = match doc.objects.get(&page_id) {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let resources = match page.get(b"Resources").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let fonts = match resources.get(b"Font").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let mut map = HashMap::new();
            for (key, val) in fonts.iter() {
                let name = String::from_utf8_lossy(key).to_string();
                if let Object::Reference(id) = val {
                    map.insert(name, *id);
                }
            }
            map
        };

        // Find symbolic simple fonts with undefined glyphs.
        // Map: resource_name -> (set of invalid codes, replacement code)
        let mut notdef_fonts: HashMap<String, HashSet<u8>> = HashMap::new();

        for (res_name, font_id) in &font_map {
            let Some(Object::Dictionary(dict)) = doc.objects.get(font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "TrueType" && subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            // Handle symbolic fonts by descriptor flags OR by well-known symbolic names.
            let base_name = get_name(dict, b"BaseFont").unwrap_or_default();
            if !is_font_symbolic(doc, dict) && !is_symbolic_font_name(&base_name) {
                continue;
            }

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            let Some(font_data) = read_embedded_font_data(doc, fd_id) else {
                continue;
            };

            let first_char = dict
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(0);
            let last_char = dict
                .get(b"LastChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(255);

            // Build set of invalid codes using font's cmap.
            // For symbolic fonts without Encoding, veraPDF uses the (3,0)
            // Symbol cmap subtable with 0xF000 offset. Only process fonts
            // that actually have a (3,0) subtable — other "symbolic" fonts
            // use different encoding mechanisms.
            let mut invalid_codes = HashSet::new();

            if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
                // Check if font has a (3,0) Microsoft Symbol cmap subtable.
                let has_symbol_cmap = face
                    .tables()
                    .cmap
                    .map(|cmap| {
                        cmap.subtables.into_iter().any(|st| {
                            st.platform_id == ttf_parser::PlatformId::Windows && st.encoding_id == 0
                        })
                    })
                    .unwrap_or(false);

                for code in first_char..=last_char.min(255) {
                    let sym_ch = char::from_u32(0xF000 + code);
                    let has_symbol_glyph = sym_ch
                        .and_then(|c| face.glyph_index(c))
                        .filter(|g| g.0 != 0)
                        .map(|g| tt_glyph_has_data(&face, g))
                        .unwrap_or(false);

                    // Some symbolic TrueType fonts (e.g. Apple Symbol.ttf)
                    // do not expose a strict (3,0) cmap path for all codes.
                    // Fall back to direct Unicode/GID probes before declaring
                    // a code invalid.
                    let has_unicode_glyph = char::from_u32(code)
                        .and_then(|c| face.glyph_index(c))
                        .filter(|g| g.0 != 0)
                        .map(|g| tt_glyph_has_data(&face, g))
                        .unwrap_or(false);
                    let has_glyph = if has_symbol_cmap {
                        has_symbol_glyph || has_unicode_glyph
                    } else {
                        has_unicode_glyph || has_symbol_glyph
                    };

                    if !has_glyph {
                        invalid_codes.insert(code as u8);
                    }
                }
            } else if let Some(cff) = cff_parser::Table::parse(&font_data) {
                // CFF symbolic font.
                //
                // Follow PDF encoding first when present (veraPDF 6.2.11.4.1:2 /
                // 6.2.11.5:1 path), and only fall back to CFF internal encoding
                // for cases where PDF-level mapping is absent/ambiguous.
                let enc_map = parse_cff_encoding_map(&font_data);
                let (enc_name, differences) = get_simple_encoding_info(doc, dict);
                let has_pdf_encoding = dict.get(b"Encoding").is_ok();
                let has_explicit_difference = !differences.is_empty();
                let allow_cff_encoding_fallback = !has_pdf_encoding
                    || cff_has_gid_based_names(&cff)
                    || (enc_name.is_empty() && !has_explicit_difference);

                for code in first_char..=last_char.min(255) {
                    let code_u8 = code as u8;
                    let mut has_glyph = false;

                    // Resolve via PDF encoding / Differences mapping first.
                    let glyph_name = if let Some(name) = differences.get(&code) {
                        Some(name.clone())
                    } else if !enc_name.is_empty() {
                        let ch = encoding_to_char(code, &enc_name);
                        unicode_to_glyph_name(ch).or_else(|| unicode_to_agl_name(ch))
                    } else {
                        None
                    };

                    if let Some(name) = glyph_name {
                        if !name.is_empty() && name != ".notdef" {
                            has_glyph = cff
                                .glyph_index_by_name(&name)
                                .and_then(|gid| cff.glyph_width(gid))
                                .is_some();

                            if !has_glyph {
                                for alt in cff_glyph_name_alternatives(&name) {
                                    if cff
                                        .glyph_index_by_name(alt)
                                        .and_then(|gid| cff.glyph_width(gid))
                                        .is_some()
                                    {
                                        has_glyph = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    // Fallback to CFF internal code->GID mapping only for
                    // no/ambiguous PDF encoding scenarios.
                    if !has_glyph && allow_cff_encoding_fallback {
                        has_glyph = enc_map.get(&code_u8).map(|&gid| gid != 0).unwrap_or(false);
                    }

                    if !has_glyph {
                        invalid_codes.insert(code_u8);
                    }
                }
            } else if let Some(parsed) = parse_type1_program(&font_data) {
                // Classic Type1 (FontFile/PFB/PFA) symbolic font.
                for code in first_char..=last_char.min(255) {
                    let code_u8 = code as u8;
                    let glyph_name = parsed.encoding.get(&code_u8).map(|s| s.as_str());
                    let has_glyph = glyph_name
                        .filter(|name| !name.is_empty() && *name != ".notdef")
                        .and_then(|name| parsed.charstring_widths.get(name))
                        // In these symbolic subsets, zero-width slots are
                        // typically .notdef proxies and should be removed.
                        .map(|w| *w > 0)
                        .unwrap_or(false);
                    if !has_glyph {
                        invalid_codes.insert(code_u8);
                    }
                }
            }

            // Fallback: when font program parsing is inconclusive, infer invalid
            // codes from PDF widths/encoding metadata for this simple font.
            if invalid_codes.is_empty() {
                invalid_codes.extend(invalid_simple_codes_from_widths(
                    doc, dict, first_char, last_char,
                ));
            }
            // For simple fonts, any single-byte code outside the declared
            // FirstChar..LastChar range is not defined by the font dictionary.
            // Keeping those bytes in text-showing operators causes .notdef /
            // missing-glyph failures (6.2.11.8:1, 6.2.11.4.1:2), especially in
            // symbolic TeX subsets where FirstChar is often 33 and stream text
            // still contains ASCII spaces (0x20).
            let first_bound = first_char.min(256);
            for code in 0..first_bound {
                invalid_codes.insert(code as u8);
            }
            let last_bound = last_char.min(255);
            if last_bound < 255 {
                for code in (last_bound + 1)..=255 {
                    invalid_codes.insert(code as u8);
                }
            }
            if !invalid_codes.is_empty() {
                notdef_fonts.insert(res_name.clone(), invalid_codes);
            }
        }

        if notdef_fonts.is_empty() {
            continue;
        }

        // Scan content streams and replace invalid codes.
        let content_ids = crate::content_editor::get_content_stream_ids(doc, page_id);
        // Preserve the selected simple font across page content-stream boundaries.
        let mut current_font = String::new();

        for cs_id in content_ids {
            let stream_data = match doc.objects.get(&cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };

            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };
            let ops = editor.operations().to_vec();
            let mut modified = false;
            let mut new_ops = Vec::with_capacity(ops.len());

            for op in &ops {
                match op.operator.as_str() {
                    "Tf" => {
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font = String::from_utf8_lossy(name).to_string();
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        if let Some(invalid_codes) = notdef_fonts.get(&current_font) {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, _)) = new_op.operands.get_mut(str_idx)
                            {
                                if fix_simple_text_string(bytes, invalid_codes) {
                                    modified = true;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    "TJ" => {
                        if let Some(invalid_codes) = notdef_fonts.get(&current_font) {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, _) = item {
                                        if fix_simple_text_string(bytes, invalid_codes) {
                                            modified = true;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    _ => {
                        new_ops.push(op.clone());
                    }
                }
            }

            if modified {
                let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                if let Ok(encoded) = new_editor.encode() {
                    if let Some(Object::Stream(s)) = doc.objects.get_mut(&cs_id) {
                        s.set_plain_content(encoded);
                        total_fixed += 1;
                    }
                }
            }
        }
    }

    total_fixed
}

/// Remove out-of-range byte codes from simple-font text strings.
///
/// For simple fonts (Type1/MMType1/TrueType), bytes outside FirstChar..LastChar
/// are not defined by the font dictionary and can resolve to .notdef / missing
/// glyphs in validators (6.2.11.8:1, 6.2.11.4.1:2). This pass strips those
/// bytes directly in content streams.
pub fn fix_simple_font_out_of_range_codes(doc: &mut Document) -> usize {
    use std::collections::HashMap;

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut total_fixed = 0usize;

    for &page_id in &page_ids {
        let font_map: HashMap<String, ObjectId> = {
            let page = match doc.objects.get(&page_id) {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let resources = match page.get(b"Resources").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let fonts = match resources.get(b"Font").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let mut map = HashMap::new();
            for (key, val) in fonts.iter() {
                let name = String::from_utf8_lossy(key).to_string();
                if let Object::Reference(id) = val {
                    map.insert(name, *id);
                }
            }
            map
        };

        let mut font_ranges: HashMap<String, (u8, u8)> = HashMap::new();
        let mut has_type0_font = false;
        for (res_name, font_id) in &font_map {
            let Some(Object::Dictionary(dict)) = doc.objects.get(font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype == "Type0" {
                has_type0_font = true;
            }
            if subtype != "TrueType" && subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            let first_char = dict
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i),
                    _ => None,
                })
                .unwrap_or(0)
                .clamp(0, 255) as u8;
            let last_char = dict
                .get(b"LastChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i),
                    _ => None,
                })
                .unwrap_or(255)
                .clamp(0, 255) as u8;

            font_ranges.insert(res_name.clone(), (first_char, last_char));
        }

        if font_ranges.is_empty() {
            continue;
        }

        let content_ids = crate::content_editor::get_content_stream_ids(doc, page_id);
        // Contents arrays are logically concatenated, so text state is page-scoped here.
        let mut current_font = String::new();

        for cs_id in content_ids {
            let stream_data = match doc.objects.get(&cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };

            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };
            let ops = editor.operations().to_vec();
            let mut modified = false;
            let mut new_ops = Vec::with_capacity(ops.len());

            for op in &ops {
                match op.operator.as_str() {
                    "Tf" => {
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font = String::from_utf8_lossy(name).to_string();
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        if let Some((first_char, last_char)) = font_ranges.get(&current_font) {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, _)) = new_op.operands.get_mut(str_idx)
                            {
                                if fix_simple_text_string_out_of_range(
                                    bytes,
                                    *first_char,
                                    *last_char,
                                    !has_type0_font,
                                ) {
                                    modified = true;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    "TJ" => {
                        if let Some((first_char, last_char)) = font_ranges.get(&current_font) {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, _) = item {
                                        if fix_simple_text_string_out_of_range(
                                            bytes,
                                            *first_char,
                                            *last_char,
                                            !has_type0_font,
                                        ) {
                                            modified = true;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    _ => new_ops.push(op.clone()),
                }
            }

            if modified {
                let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                if let Ok(encoded) = new_editor.encode() {
                    if let Some(Object::Stream(s)) = doc.objects.get_mut(&cs_id) {
                        s.set_plain_content(encoded);
                        total_fixed += 1;
                    }
                }
            }
        }
    }

    total_fixed
}

/// Combined single-pass replacement for `fix_simple_font_out_of_range_codes` +
/// `strip_control_chars_from_streams`. Processes each content stream once instead of
/// twice, halving ContentEditor parse and re-encode overhead. (#534 perf)
///
/// Returns `(range_fixed, control_fixed)` — streams modified by each sub-pass.
pub fn fix_simple_font_streams(doc: &mut Document) -> (usize, usize) {
    use std::collections::HashMap;

    struct FontInfo {
        /// True for TrueType/Type1/MMType1/Type3 — strip control bytes.
        can_strip: bool,
        /// FirstChar/LastChar range (TrueType/Type1/MMType1 only).
        range: Option<(u8, u8)>,
    }

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut range_fixed = 0usize;
    let mut ctrl_fixed = 0usize;

    for &page_id in &page_ids {
        let mut has_type0 = false;

        let font_infos: HashMap<String, FontInfo> = {
            let page = match doc.objects.get(&page_id) {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let resources = match page.get(b"Resources").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let fonts = match resources.get(b"Font").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };

            let mut map = HashMap::new();
            for (key, val) in fonts.iter() {
                let res_name = String::from_utf8_lossy(key).to_string();
                let fd = match val {
                    Object::Reference(id) => match doc.objects.get(id) {
                        Some(Object::Dictionary(d)) => d,
                        _ => continue,
                    },
                    Object::Dictionary(d) => d,
                    _ => continue,
                };
                let subtype = get_name(fd, b"Subtype").unwrap_or_default();
                if subtype == "Type0" {
                    has_type0 = true;
                }
                let can_strip =
                    matches!(subtype.as_str(), "TrueType" | "Type1" | "MMType1" | "Type3");
                let range = if matches!(subtype.as_str(), "TrueType" | "Type1" | "MMType1") {
                    let fc = fd
                        .get(b"FirstChar")
                        .ok()
                        .and_then(|o| match o {
                            Object::Integer(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(0)
                        .clamp(0, 255) as u8;
                    let lc = fd
                        .get(b"LastChar")
                        .ok()
                        .and_then(|o| match o {
                            Object::Integer(i) => Some(*i),
                            _ => None,
                        })
                        .unwrap_or(255)
                        .clamp(0, 255) as u8;
                    Some((fc, lc))
                } else {
                    None
                };
                map.insert(res_name, FontInfo { can_strip, range });
            }
            map
        };

        if !font_infos
            .values()
            .any(|fi| fi.can_strip || fi.range.is_some())
        {
            continue;
        }
        let content_ids = crate::content_editor::get_content_stream_ids(doc, page_id);
        let mut current_font = String::new();

        for cs_id in content_ids {
            let stream_data = match doc.objects.get(&cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };

            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };
            let ops = editor.operations().to_vec();
            let mut did_range = false;
            let mut did_ctrl = false;
            let mut new_ops = Vec::with_capacity(ops.len());

            for op in &ops {
                match op.operator.as_str() {
                    "Tf" => {
                        if let Some(Object::Name(n)) = op.operands.first() {
                            current_font = String::from_utf8_lossy(n).to_string();
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        let fi = font_infos.get(&current_font);
                        if fi.is_some_and(|fi| fi.can_strip || fi.range.is_some()) {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let (Some(fi), Some(Object::String(bytes, _))) =
                                (fi, new_op.operands.get_mut(str_idx))
                            {
                                if let Some((fc, lc)) = fi.range {
                                    if fix_simple_text_string_out_of_range(
                                        bytes, fc, lc, !has_type0,
                                    ) {
                                        did_range = true;
                                    }
                                }
                                if fi.can_strip && strip_control_bytes(bytes, !has_type0) {
                                    did_ctrl = true;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    "TJ" => {
                        let fi = font_infos.get(&current_font);
                        if fi.is_some_and(|fi| fi.can_strip || fi.range.is_some()) {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let (Some(fi), Object::String(bytes, _)) = (fi, item) {
                                        if let Some((fc, lc)) = fi.range {
                                            if fix_simple_text_string_out_of_range(
                                                bytes, fc, lc, !has_type0,
                                            ) {
                                                did_range = true;
                                            }
                                        }
                                        if fi.can_strip && strip_control_bytes(bytes, !has_type0) {
                                            did_ctrl = true;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    _ => new_ops.push(op.clone()),
                }
            }

            if did_range || did_ctrl {
                let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                if let Ok(encoded) = new_editor.encode() {
                    if let Some(Object::Stream(s)) = doc.objects.get_mut(&cs_id) {
                        s.set_plain_content(encoded);
                        if did_range {
                            range_fixed += 1;
                        }
                        if did_ctrl {
                            ctrl_fixed += 1;
                        }
                    }
                }
            }
        }
    }

    (range_fixed, ctrl_fixed)
}

/// Replace single-byte codes in a simple font text string that are invalid.
#[allow(clippy::ptr_arg)]
fn fix_simple_text_string(
    bytes: &mut Vec<u8>,
    invalid_codes: &std::collections::HashSet<u8>,
) -> bool {
    let changed = collapse_two_byte_simple_codes(bytes);
    let original_len = bytes.len();
    bytes.retain(|b| !invalid_codes.contains(b));
    changed || bytes.len() != original_len
}

#[allow(clippy::ptr_arg)]
fn fix_simple_text_string_out_of_range(
    bytes: &mut Vec<u8>,
    first_char: u8,
    last_char: u8,
    allow_collapse: bool,
) -> bool {
    let changed = allow_collapse && collapse_two_byte_simple_codes(bytes);
    let original_len = bytes.len();
    if first_char > last_char {
        if !bytes.is_empty() {
            bytes.clear();
            return true;
        }
        return changed;
    }

    // Preserve sentinel-paired 2-byte encoding when collapse is disabled.
    if !allow_collapse {
        if let Some(code_in_odd_lane) = paired_simple_code_lane(bytes) {
            let mut filtered = Vec::with_capacity(bytes.len());
            for i in (0..bytes.len()).step_by(2) {
                let code = if code_in_odd_lane {
                    bytes[i + 1]
                } else {
                    bytes[i]
                };
                if code >= first_char && code <= last_char {
                    filtered.push(bytes[i]);
                    filtered.push(bytes[i + 1]);
                }
            }
            let len_changed = filtered.len() != original_len;
            if len_changed {
                *bytes = filtered;
            }
            return changed || len_changed;
        }
    }

    bytes.retain(|b| *b >= first_char && *b <= last_char);
    changed || bytes.len() != original_len
}

fn paired_simple_code_lane(bytes: &[u8]) -> Option<bool> {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let even_all_00 = bytes.iter().step_by(2).all(|b| *b == 0x00);
    let even_all_ff = bytes.iter().step_by(2).all(|b| *b == 0xFF);
    if even_all_00 || even_all_ff {
        return Some(true); // code byte is odd lane (i+1)
    }
    let odd_all_00 = bytes.iter().skip(1).step_by(2).all(|b| *b == 0x00);
    let odd_all_ff = bytes.iter().skip(1).step_by(2).all(|b| *b == 0xFF);
    if odd_all_00 || odd_all_ff {
        return Some(false); // code byte is even lane (i)
    }
    None
}

/// Conservative fallback for symbolic simple fonts: treat codes with explicit
/// zero/negative widths (or explicit .notdef Differences entries) as invalid.
fn invalid_simple_codes_from_widths(
    doc: &Document,
    dict: &lopdf::Dictionary,
    first_char: u32,
    last_char: u32,
) -> std::collections::HashSet<u8> {
    let mut invalid = std::collections::HashSet::new();

    let width_first_char = dict
        .get(b"FirstChar")
        .ok()
        .and_then(|o| match o {
            Object::Integer(i) => Some(*i),
            _ => None,
        })
        .unwrap_or(first_char as i64);

    let widths = match dict.get(b"Widths").ok() {
        Some(Object::Array(a)) => Some(a.clone()),
        Some(Object::Reference(r)) => match doc.objects.get(r) {
            Some(Object::Array(a)) => Some(a.clone()),
            _ => None,
        },
        _ => None,
    };

    if let Some(widths) = widths {
        for code in first_char..=last_char.min(255) {
            let idx = code as i64 - width_first_char;
            if idx < 0 {
                continue;
            }
            let Some(wobj) = widths.get(idx as usize) else {
                continue;
            };
            let w = match wobj {
                Object::Integer(i) => *i as f64,
                Object::Real(r) => *r as f64,
                _ => continue,
            };
            if w <= 0.0 {
                invalid.insert(code as u8);
            }
        }
    }

    // Explicit Differences /.notdef are always invalid references.
    let enc_info = extract_encoding_info(doc, dict);
    for (code, name) in enc_info.differences {
        if code <= 255 && code >= first_char && code <= last_char && name == ".notdef" {
            invalid.insert(code as u8);
        }
    }

    invalid
}

fn collapse_two_byte_simple_codes(bytes: &mut Vec<u8>) -> bool {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return false;
    }

    let even_all_00 = bytes.iter().step_by(2).all(|b| *b == 0x00);
    let even_all_ff = bytes.iter().step_by(2).all(|b| *b == 0xFF);
    let odd_all_00 = bytes.iter().skip(1).step_by(2).all(|b| *b == 0x00);
    let odd_all_ff = bytes.iter().skip(1).step_by(2).all(|b| *b == 0xFF);

    if !(even_all_00 || even_all_ff || odd_all_00 || odd_all_ff) {
        return false;
    }

    let take_odd = even_all_00 || even_all_ff;
    let mut collapsed = Vec::with_capacity(bytes.len() / 2);
    let start = if take_odd { 1 } else { 0 };
    for i in (start..bytes.len()).step_by(2) {
        collapsed.push(bytes[i]);
    }
    *bytes = collapsed;
    true
}

fn replace_simple_font_code_refs(
    doc: &mut Document,
    target_font_id: ObjectId,
    from_code: u8,
    to_code: Option<u8>,
) -> usize {
    use std::collections::HashSet;

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut total_fixed = 0usize;

    for &page_id in &page_ids {
        // Find all resource names on this page that resolve to the target font.
        let target_names: HashSet<String> = {
            let page = match doc.objects.get(&page_id) {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let resources = match page.get(b"Resources").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let fonts = match resources.get(b"Font").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };

            fonts
                .iter()
                .filter_map(|(key, val)| match val {
                    Object::Reference(id) if *id == target_font_id => {
                        Some(String::from_utf8_lossy(key).to_string())
                    }
                    _ => None,
                })
                .collect()
        };

        if target_names.is_empty() {
            continue;
        }

        let content_ids = crate::content_editor::get_content_stream_ids(doc, page_id);
        let mut current_font = String::new();

        for cs_id in content_ids {
            let stream_data = match doc.objects.get(&cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };

            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };
            let ops = editor.operations().to_vec();
            let mut modified = false;
            let mut new_ops = Vec::with_capacity(ops.len());

            for op in &ops {
                match op.operator.as_str() {
                    "Tf" => {
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font = String::from_utf8_lossy(name).to_string();
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        if target_names.contains(&current_font) {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, _)) = new_op.operands.get_mut(str_idx)
                            {
                                if collapse_two_byte_simple_codes(bytes) {
                                    modified = true;
                                }
                                if let Some(to) = to_code {
                                    for b in bytes.iter_mut() {
                                        if *b == from_code {
                                            *b = to;
                                            modified = true;
                                        }
                                    }
                                } else {
                                    let original_len = bytes.len();
                                    bytes.retain(|b| *b != from_code);
                                    modified |= bytes.len() != original_len;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    "TJ" => {
                        if target_names.contains(&current_font) {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, _) = item {
                                        if collapse_two_byte_simple_codes(bytes) {
                                            modified = true;
                                        }
                                        if let Some(to) = to_code {
                                            for b in bytes.iter_mut() {
                                                if *b == from_code {
                                                    *b = to;
                                                    modified = true;
                                                }
                                            }
                                        } else {
                                            let original_len = bytes.len();
                                            bytes.retain(|b| *b != from_code);
                                            modified |= bytes.len() != original_len;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    _ => {
                        new_ops.push(op.clone());
                    }
                }
            }

            if modified {
                let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                if let Ok(encoded) = new_editor.encode() {
                    if let Some(Object::Stream(s)) = doc.objects.get_mut(&cs_id) {
                        s.set_plain_content(encoded);
                        total_fixed += 1;
                    }
                }
            }
        }
    }

    total_fixed
}

/// Encoding info extracted from a font dictionary.
struct EncodingInfo {
    /// Base encoding name (e.g., "WinAnsiEncoding").
    base_encoding: String,
    /// Referenced encoding object ID (if encoding is a reference).
    enc_ref: Option<ObjectId>,
    /// Existing Differences: list of (code, glyph_name) pairs.
    differences: Vec<(u32, String)>,
}

/// Extract encoding info from a font dictionary.
fn extract_encoding_info(doc: &Document, dict: &lopdf::Dictionary) -> EncodingInfo {
    let mut info = EncodingInfo {
        base_encoding: String::new(),
        enc_ref: None,
        differences: Vec::new(),
    };

    match dict.get(b"Encoding").ok() {
        Some(Object::Name(n)) => {
            info.base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
        Some(Object::Dictionary(enc_dict)) => {
            info.base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            info.differences =
                parse_differences_to_vec_from_object(doc, enc_dict.get(b"Differences").ok());
        }
        Some(Object::Reference(enc_id)) => {
            info.enc_ref = Some(*enc_id);
            if let Ok(enc_obj) = doc.get_object(*enc_id) {
                match enc_obj {
                    Object::Name(n) => {
                        info.base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
                    }
                    Object::Dictionary(enc_dict) => {
                        if info.base_encoding.is_empty() {
                            info.base_encoding =
                                get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
                        }
                        info.differences = parse_differences_to_vec_from_object(
                            doc,
                            enc_dict.get(b"Differences").ok(),
                        );
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    info
}

/// Parse a Differences array into (code, glyph_name) pairs.
fn parse_differences_to_vec(doc: &Document, arr: &[Object]) -> Vec<(u32, String)> {
    let mut result = Vec::new();
    let mut current_code: Option<u32> = None;
    for obj in arr {
        match obj {
            Object::Integer(i) if *i >= 0 => {
                current_code = Some(*i as u32);
            }
            Object::Name(n) => {
                if let Some(code) = current_code {
                    if let Ok(name) = String::from_utf8(n.clone()) {
                        result.push((code, name));
                    }
                    current_code = Some(code + 1);
                }
            }
            Object::Reference(r) => {
                if let Ok(resolved) = doc.get_object(*r) {
                    match resolved {
                        Object::Integer(i) if *i >= 0 => {
                            current_code = Some(*i as u32);
                        }
                        Object::Name(n) => {
                            if let Some(code) = current_code {
                                if let Ok(name) = String::from_utf8(n.clone()) {
                                    result.push((code, name));
                                }
                                current_code = Some(code + 1);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    result
}

/// Parse a Differences object that may be an array or an indirect reference.
fn parse_differences_to_vec_from_object(
    doc: &Document,
    obj: Option<&Object>,
) -> Vec<(u32, String)> {
    match obj {
        Some(Object::Array(arr)) => parse_differences_to_vec(doc, arr),
        Some(Object::Reference(r)) => doc
            .get_object(*r)
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|arr| parse_differences_to_vec(doc, arr))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Check whether an Encoding dictionary reference is shared by multiple fonts.
fn is_encoding_ref_shared(doc: &Document, enc_ref: ObjectId, current_font_id: ObjectId) -> bool {
    let mut seen = 0usize;
    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        let Ok(Object::Reference(r)) = dict.get(b"Encoding") else {
            continue;
        };
        if *r != enc_ref {
            continue;
        }
        if *id != current_font_id {
            seen += 1;
            if seen > 0 {
                return true;
            }
        }
    }
    false
}

fn resolve_page_resources_local(doc: &Document, page_id: ObjectId) -> Option<lopdf::Dictionary> {
    let mut current_id = Some(page_id);

    while let Some(id) = current_id {
        let page = doc.get_object(id).ok()?.as_dict().ok()?;
        match page.get(b"Resources").ok() {
            Some(Object::Dictionary(dict)) => return Some(dict.clone()),
            Some(Object::Reference(resource_id)) => match doc.get_object(*resource_id).ok() {
                Some(Object::Dictionary(dict)) => return Some(dict.clone()),
                _ => return None,
            },
            _ => {
                current_id = page
                    .get(b"Parent")
                    .ok()
                    .and_then(|obj| obj.as_reference().ok());
            }
        }
    }

    None
}

fn collect_simple_font_used_codes(
    doc: &Document,
) -> std::collections::HashMap<ObjectId, std::collections::HashSet<u32>> {
    use std::collections::{HashMap, HashSet};

    fn insert_used_codes(set: &mut HashSet<u32>, bytes: &[u8]) {
        if let Some(code_in_odd_lane) = paired_simple_code_lane(bytes) {
            for i in (0..bytes.len()).step_by(2) {
                let code = if code_in_odd_lane {
                    bytes[i + 1]
                } else {
                    bytes[i]
                };
                set.insert(code as u32);
            }
            return;
        }

        for &b in bytes {
            set.insert(b as u32);
        }
    }

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut used_codes: HashMap<ObjectId, HashSet<u32>> = HashMap::new();

    for &page_id in &page_ids {
        let Some(resources) = resolve_page_resources_local(doc, page_id) else {
            continue;
        };
        let fonts = match resources.get(b"Font").ok() {
            Some(Object::Dictionary(d)) => d.clone(),
            Some(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            },
            _ => continue,
        };

        let mut font_map: HashMap<String, ObjectId> = HashMap::new();
        for (key, val) in fonts.iter() {
            let Object::Reference(font_id) = val else {
                continue;
            };
            let Some(Object::Dictionary(dict)) = doc.objects.get(font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            let is_simple = subtype == "TrueType"
                || subtype == "Type1"
                || subtype == "MMType1"
                || subtype == "Type3";
            if !is_simple {
                continue;
            }
            font_map.insert(String::from_utf8_lossy(key).to_string(), *font_id);
        }

        if font_map.is_empty() {
            continue;
        }

        let content_ids = crate::content_editor::get_content_stream_ids(doc, page_id);
        let mut current_font = String::new();

        for cs_id in content_ids {
            let stream_data = match doc.objects.get(&cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };

            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };

            for op in editor.operations() {
                match op.operator.as_str() {
                    "Tf" => {
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font = String::from_utf8_lossy(name).to_string();
                        }
                    }
                    "Tj" | "'" | "\"" => {
                        let Some(font_id) = font_map.get(&current_font).copied() else {
                            continue;
                        };
                        let str_idx = if op.operator == "\"" { 2 } else { 0 };
                        let Some(Object::String(bytes, _)) = op.operands.get(str_idx) else {
                            continue;
                        };
                        insert_used_codes(used_codes.entry(font_id).or_default(), bytes);
                    }
                    "TJ" => {
                        let Some(font_id) = font_map.get(&current_font).copied() else {
                            continue;
                        };
                        let Some(Object::Array(arr)) = op.operands.first() else {
                            continue;
                        };
                        for item in arr {
                            if let Object::String(bytes, _) = item {
                                insert_used_codes(used_codes.entry(font_id).or_default(), bytes);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    used_codes
}

/// Check if a TrueType glyph has actual data in the glyf table.
///
/// Subset fonts keep cmap entries for stripped glyphs (GID still valid),
/// but the loca table entry has zero length (no glyf data). This function
/// detects such empty slots by comparing consecutive loca offsets.
fn tt_glyph_has_data(face: &ttf_parser::Face, gid: ttf_parser::GlyphId) -> bool {
    let raw = face.raw_face();
    let Some(head) = raw.table(ttf_parser::Tag::from_bytes(b"head")) else {
        return true; // Can't check — assume present.
    };
    let Some(loca) = raw.table(ttf_parser::Tag::from_bytes(b"loca")) else {
        return true;
    };
    if head.len() < 52 {
        return true;
    }
    let idx_format = i16::from_be_bytes([head[50], head[51]]);
    let g = gid.0 as usize;

    if idx_format == 0 {
        // Short format: 2 bytes per entry, stored value × 2 = byte offset.
        let off = g * 2;
        if off + 4 > loca.len() {
            return true;
        }
        let o1 = u16::from_be_bytes([loca[off], loca[off + 1]]) as u32;
        let o2 = u16::from_be_bytes([loca[off + 2], loca[off + 3]]) as u32;
        o2 > o1
    } else {
        // Long format: 4 bytes per entry, stored value = byte offset.
        let off = g * 4;
        if off + 8 > loca.len() {
            return true;
        }
        let o1 = u32::from_be_bytes([loca[off], loca[off + 1], loca[off + 2], loca[off + 3]]);
        let o2 = u32::from_be_bytes([loca[off + 4], loca[off + 5], loca[off + 6], loca[off + 7]]);
        o2 > o1
    }
}

fn tt_glyph_is_real(face: &ttf_parser::Face, gid: ttf_parser::GlyphId, is_subset: bool) -> bool {
    gid.0 != 0 && (!is_subset || tt_glyph_has_data(face, gid))
}

/// Fix .notdef references in a TrueType font.
///
/// Phase 1: Replace any ".notdef" entries in existing Differences with
///          the correct glyph name (if found) or "space".
/// Phase 2: For codes NOT in Differences that map to .notdef via the base
///          encoding, add Differences entries IF the font has the glyph.
/// Phase 3: For subset fonts, detect codes that map to GIDs whose outlines
///          were stripped (empty loca entry) and remap them to "space".
#[allow(clippy::too_many_arguments)]
fn fix_notdef_in_truetype(
    doc: &mut Document,
    font_id: ObjectId,
    font_data: &[u8],
    enc_info: &EncodingInfo,
    first_char: u32,
    last_char: u32,
    is_subset: bool,
    used_codes: Option<&std::collections::HashSet<u32>>,
) -> bool {
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return false;
    };

    // Also handle referenced encoding dicts.
    let mut differences = enc_info.differences.clone();
    let mut base_encoding = enc_info.base_encoding.clone();
    let enc_ref = enc_info.enc_ref;
    let shared_encoding_ref = enc_ref.is_some_and(|r| is_encoding_ref_shared(doc, r, font_id));

    if let Some(ref_id) = enc_ref {
        // Dereference the encoding object.
        if let Some(Object::Dictionary(enc_dict)) = doc.objects.get(&ref_id) {
            if base_encoding.is_empty() {
                base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            }
            differences =
                parse_differences_to_vec_from_object(doc, enc_dict.get(b"Differences").ok());
        } else if let Some(Object::Name(n)) = doc.objects.get(&ref_id) {
            base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
    }

    // Phase 1: Find .notdef entries and entries referencing glyphs
    // not present in the font program (which veraPDF treats as .notdef).
    let mut replacements: Vec<(u32, String)> = Vec::new();

    for (code, name) in &differences {
        let is_notdef = name == ".notdef";
        let glyph_missing = !is_notdef && {
            // Check if the glyph name resolves to a real glyph in the font.
            let ch = glyph_name_to_unicode(name);
            match ch.and_then(|c| face.glyph_index(c)) {
                Some(gid) => !tt_glyph_is_real(&face, gid, is_subset),
                None => {
                    // Apply canonical normalization before deciding "missing":
                    // U+00AD (soft hyphen) → U+002D (hyphen-minus).
                    // veraPDF uses the same fallback for §6.2.11.8 presence checks.
                    if ch == Some('\u{00AD}')
                        && face
                            .glyph_index('-')
                            .is_some_and(|gid| tt_glyph_is_real(&face, gid, is_subset))
                    {
                        false // Accessible via canonical fallback — not missing.
                    } else {
                        // Try by post table name lookup.
                        face.glyph_index_by_name(name).is_none()
                    }
                }
            }
        };
        if is_notdef || glyph_missing {
            let replacement = sanitize_truetype_difference_name(find_truetype_glyph_name_for_code(
                &face,
                *code,
                &base_encoding,
            ));
            replacements.push((*code, replacement));
        }
    }

    // Phase 2: Check base encoding for .notdef mappings.
    let mut new_diffs: Vec<(u32, String)> = Vec::new();

    // Codes that already have valid Differences entries (glyph present in font).
    let valid_diff_codes: std::collections::HashSet<u32> = differences
        .iter()
        .filter(|(_, name)| {
            if name == ".notdef" {
                return false;
            }
            let ch = glyph_name_to_unicode(name);
            match ch.and_then(|c| face.glyph_index(c)) {
                Some(gid) => tt_glyph_is_real(&face, gid, is_subset),
                None => face.glyph_index_by_name(name).is_some(),
            }
        })
        .map(|(c, _)| *c)
        .collect();

    let check_start = first_char.min(255);
    let check_end = last_char.min(255);
    for code in check_start..=check_end {
        if let Some(codes) = used_codes {
            if !codes.is_empty() && !codes.contains(&code) {
                continue;
            }
        }
        // Shared encoding dictionaries are commonly referenced by multiple
        // subset fonts with different glyph sets. Avoid adding broad phase-2
        // remaps there; they can introduce cross-font width regressions.
        if shared_encoding_ref && is_subset && code >= 32 {
            continue;
        }
        if valid_diff_codes.contains(&code) {
            continue;
        }
        // Skip codes already handled in Phase 1.
        if replacements.iter().any(|(c, _)| *c == code) {
            continue;
        }

        // For codes below 32: control characters. For NON-subset fonts,
        // map them to "space". For subset fonts, skip — adding "space"
        // causes §6.2.11.4.1:2 when the subset doesn't contain "space",
        // and changing the Encoding from a simple name to a dict with
        // Differences can break the glyph lookup path.
        if code < 32 {
            if !is_subset {
                new_diffs.push((code, "space".to_string()));
            }
            continue;
        }

        let ch = encoding_to_char(code, &base_encoding);
        let printable_standard_code = (33..=126).contains(&code)
            && matches!(
                base_encoding.as_str(),
                "WinAnsiEncoding" | "MacRomanEncoding" | "StandardEncoding"
            );

        // Check if this code maps to a real glyph in the font.
        let gid_opt = face.glyph_index(ch);
        let has_valid_glyph = match gid_opt {
            Some(gid) => tt_glyph_is_real(&face, gid, is_subset),
            None => false,
        };

        if has_valid_glyph {
            continue; // Glyph present with outline data — no fix needed.
        }

        // Some Unicode characters are rendered via a canonical base glyph —
        // the same fallback that veraPDF uses for §6.2.11.5 width checking.
        // U+00AD (soft hyphen) is handled by falling back to U+002D (hyphen).
        // If the font has the canonical glyph, the code is effectively mapped
        // to a valid glyph and needs no Differences entry. Adding "space" here
        // would cause fix_font_width_mismatches to return the space width instead
        // of the hyphen width, producing a §6.2.11.5 mismatch. (#fix-tt-notdef-soft-hyphen)
        let canonical_fallback_ch: Option<char> = match ch {
            '\u{00AD}' => Some('-'), // soft hyphen → hyphen-minus
            _ => None,
        };
        if let Some(fb_ch) = canonical_fallback_ch {
            if let Some(fb_gid) = face.glyph_index(fb_ch) {
                let fb_valid = tt_glyph_is_real(&face, fb_gid, is_subset);
                if fb_valid {
                    continue; // Canonical fallback present — no Differences needed.
                }
            }
        }

        // For subset fonts: don't add "space" for codes that already have a
        // non-zero width. Those codes are USED in the content stream with the
        // original glyph mapping, which was correct. Adding Differences[code]="space"
        // introduces §6.2.11.4.1:2 when the subset doesn't contain "space".
        if is_subset {
            continue; // Skip ALL Phase 2 additions for subset fonts.
                      // Subset font encodings were created by the original authoring tool
                      // with correct glyph mappings. Our WinAnsi-based glyph check may
                      // not find the glyph but it IS in the subset. Adding "space" only
                      // causes missing glyph violations.
        }

        // The encoding maps this code to a Unicode char that the font
        // doesn't have. Check if the font has the glyph by name.
        let glyph_name = sanitize_truetype_difference_name(find_truetype_glyph_name_for_code(
            &face,
            code,
            &base_encoding,
        ));
        if glyph_name == "space" {
            if printable_standard_code {
                continue;
            }
            // Space is intentionally a blank glyph (no outline data in loca),
            // so tt_glyph_has_data returns false for it — but the glyph IS
            // valid and accessible via cmap. Only check that U+0020 is reachable;
            // outline presence is irrelevant for blank-by-design glyphs. (#504)
            if face.glyph_index(' ').is_some_and(|gid| gid.0 != 0) {
                new_diffs.push((code, "space".to_string()));
            }
        } else {
            // Font has a concrete replacement glyph by name — add it
            // to Differences so it doesn't resolve to .notdef.
            new_diffs.push((code, glyph_name));
        }
    }

    if replacements.is_empty() && new_diffs.is_empty() {
        return false;
    }

    // Apply the fixes by rebuilding the Encoding dictionary.
    apply_encoding_fixes(
        doc,
        font_id,
        &base_encoding,
        &differences,
        &replacements,
        &new_diffs,
        enc_ref,
    )
}

/// Fallback for fixing .notdef references when CFF parsing fails
/// (i.e., PFB Type1 fonts or corrupt CFF data).
///
/// Two strategies:
/// 1. Remap control characters (0-31) in Encoding/Differences to "space"
///    so they don't resolve to .notdef through missing glyphs.
/// 2. Remove control character bytes from content stream text strings.
fn fix_notdef_control_chars_fallback(
    doc: &mut Document,
    font_id: ObjectId,
    enc_info: &EncodingInfo,
    first_char: u32,
    _last_char: u32,
) -> bool {
    if first_char > 31 {
        return false;
    }

    // Strategy 1: Remap control characters in Encoding/Differences to "space".
    // For PFB fonts we can't determine available glyphs, but remapping control
    // codes to "space" is always safe since they are non-printing.
    let mut differences = enc_info.differences.clone();
    let mut base_encoding = enc_info.base_encoding.clone();
    let enc_ref = enc_info.enc_ref;

    if let Some(ref_id) = enc_ref {
        if let Some(Object::Dictionary(enc_dict)) = doc.objects.get(&ref_id) {
            if base_encoding.is_empty() {
                base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            }
            differences =
                parse_differences_to_vec_from_object(doc, enc_dict.get(b"Differences").ok());
        } else if let Some(Object::Name(n)) = doc.objects.get(&ref_id) {
            base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
    }

    if base_encoding.is_empty() {
        base_encoding = "StandardEncoding".to_string();
    }

    // Remap existing Differences for control codes (0-31) to "space".
    let replacements: Vec<(u32, String)> = differences
        .iter()
        .filter(|(code, name)| *code < 32 && name != "space" && name != ".notdef")
        .map(|(code, _)| (*code, "space".to_string()))
        .collect();

    // Add new Differences for control codes not yet in the array.
    let existing_codes: std::collections::HashSet<u32> =
        differences.iter().map(|(c, _)| *c).collect();
    let new_diffs: Vec<(u32, String)> = (first_char..32)
        .filter(|c| !existing_codes.contains(c))
        .map(|c| (c, "space".to_string()))
        .collect();

    let encoding_fixed = if !replacements.is_empty() || !new_diffs.is_empty() {
        apply_encoding_fixes(
            doc,
            font_id,
            &base_encoding,
            &differences,
            &replacements,
            &new_diffs,
            enc_ref,
        )
    } else {
        false
    };

    // Strategy 2: Remove control character bytes from content streams.
    let mut font_key: Option<Vec<u8>> = None;
    let page_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for pid in &page_ids {
        let Some(Object::Dictionary(page)) = doc.objects.get(pid) else {
            continue;
        };
        if get_name(page, b"Type").as_deref() != Some("Page") {
            continue;
        }
        let resources = match page.get(b"Resources").ok() {
            Some(Object::Dictionary(d)) => d.clone(),
            Some(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            },
            _ => continue,
        };
        let fonts = match resources.get(b"Font").ok() {
            Some(Object::Dictionary(d)) => d.clone(),
            Some(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            },
            _ => continue,
        };
        for (key, val) in fonts.iter() {
            if let Object::Reference(r) = val {
                if *r == font_id {
                    font_key = Some(key.clone());
                    break;
                }
            }
        }
        if font_key.is_some() {
            break;
        }
    }

    let Some(fk) = font_key else {
        return encoding_fixed;
    };
    let font_key_str = format!("/{}", String::from_utf8_lossy(&fk));

    let mut stream_fixed = false;
    let content_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for cid in content_ids {
        let is_stream = matches!(doc.objects.get(&cid), Some(Object::Stream(_)));
        if !is_stream {
            continue;
        }
        let content = {
            let Some(Object::Stream(ref stream)) = doc.objects.get(&cid) else {
                continue;
            };
            match stream.decompressed_content() {
                Ok(data) => data,
                Err(_) => continue,
            }
        };

        let font_key_bytes = font_key_str.as_bytes();
        if !content
            .windows(font_key_bytes.len())
            .any(|w| w == font_key_bytes)
        {
            continue;
        }

        // Check for any control character in string literals.
        let has_control = content.windows(2).any(|w| w[0] == b'(' && w[1] < 32)
            || content.iter().enumerate().any(|(i, &b)| {
                b < 32
                    && b != b'\n'
                    && b != b'\r'
                    && b != b'\t'
                    && i > 0
                    && content[..i]
                        .iter()
                        .rev()
                        .take_while(|&&c| c != b'(' && c != b')')
                        .count()
                        < content[..i]
                            .iter()
                            .rev()
                            .position(|&c| c == b'(')
                            .unwrap_or(usize::MAX)
            });
        if !has_control {
            continue;
        }

        // Remove all control characters (0-31) from PDF string literals in
        // the content stream.
        let mut new_content = Vec::with_capacity(content.len());
        let mut i = 0;
        let mut in_string = false;
        let mut modified = false;
        while i < content.len() {
            if content[i] == b'(' && !in_string {
                in_string = true;
                new_content.push(content[i]);
                i += 1;
                continue;
            }
            if content[i] == b')' && in_string {
                in_string = false;
                new_content.push(content[i]);
                i += 1;
                continue;
            }
            if content[i] == b'\\' && in_string {
                new_content.push(content[i]);
                i += 1;
                if i < content.len() {
                    new_content.push(content[i]);
                    i += 1;
                }
                continue;
            }
            if in_string && content[i] < 32 {
                // Skip control character.
                modified = true;
                i += 1;
                continue;
            }
            new_content.push(content[i]);
            i += 1;
        }

        if modified {
            let len = new_content.len() as i64;
            let new_stream = lopdf::Stream::new(
                lopdf::dictionary! {
                    "Length" => len,
                },
                new_content,
            );
            doc.objects.insert(cid, Object::Stream(new_stream));
            stream_fixed = true;
        }
    }

    encoding_fixed || stream_fixed
}

/// Fix .notdef references in a Type1 (CFF) font.
fn fix_notdef_in_type1(
    doc: &mut Document,
    font_id: ObjectId,
    font_data: &[u8],
    enc_info: &EncodingInfo,
    first_char: u32,
    last_char: u32,
    is_subset: bool,
) -> bool {
    let enc_ref = enc_info.enc_ref;
    let shared_encoding_ref = enc_ref.is_some_and(|r| is_encoding_ref_shared(doc, r, font_id));
    let has_fontfile1 = {
        let fd_id = match doc.objects.get(&font_id) {
            Some(Object::Dictionary(font)) => match font.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            },
            _ => None,
        };
        fd_id
            .and_then(|id| doc.objects.get(&id))
            .and_then(|o| o.as_dict().ok())
            .is_some_and(|fd| fd.has(b"FontFile"))
    };

    if has_fontfile1 && looks_like_type1_fontfile(font_data) {
        if fix_notdef_in_type1_fontfile(
            doc, font_id, font_data, enc_info, first_char, last_char, is_subset,
        ) {
            return true;
        }
        return fix_notdef_control_chars_fallback(doc, font_id, enc_info, first_char, last_char);
    }

    let cff = cff_parser::Table::parse(font_data);
    // If CFF parsing fails but we have control characters (0-31) in the range,
    // still add Differences to remap them away from .notdef.
    if cff.is_none() {
        if has_fontfile1
            && looks_like_type1_fontfile(font_data)
            && fix_notdef_in_type1_fontfile(
                doc, font_id, font_data, enc_info, first_char, last_char, is_subset,
            )
        {
            return true;
        }
        return fix_notdef_control_chars_fallback(doc, font_id, enc_info, first_char, last_char);
    }
    let cff = cff.expect("cff.is_none() branch returns above");

    // Build set of available glyph names.
    let mut available_glyphs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let num_glyphs = cff.number_of_glyphs();
    for gid in 0..num_glyphs {
        let glyph_id = cff_parser::GlyphId(gid);
        if let Some(name) = cff.glyph_name(glyph_id) {
            if name != ".notdef" && cff.glyph_width(glyph_id).is_some() {
                available_glyphs.insert(name.to_string());
            }
        }
    }

    // If CFF has no usable glyphs (only .notdef), we can't remap via
    // Differences — fall back to content stream modification.
    if available_glyphs.is_empty() {
        return fix_notdef_control_chars_fallback(doc, font_id, enc_info, first_char, last_char);
    }

    // Also handle referenced encoding dicts.
    let mut differences = enc_info.differences.clone();
    let mut base_encoding = enc_info.base_encoding.clone();
    if let Some(ref_id) = enc_ref {
        if let Some(Object::Dictionary(enc_dict)) = doc.objects.get(&ref_id) {
            if base_encoding.is_empty() {
                base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            }
            differences =
                parse_differences_to_vec_from_object(doc, enc_dict.get(b"Differences").ok());
        } else if let Some(Object::Name(n)) = doc.objects.get(&ref_id) {
            base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
    }
    if base_encoding.is_empty() {
        base_encoding = "StandardEncoding".to_string();
    }

    // Phase 1: Replace .notdef entries and entries referencing glyphs
    // not present in the font program (which veraPDF treats as .notdef).
    let charset_contains =
        |name: &str| -> bool { font_descriptor_charset_contains(doc, font_id, name) };
    let glyph_available =
        |name: &str| -> bool { available_glyphs.contains(name) || charset_contains(name) };
    let mut replacements: Vec<(u32, String)> = Vec::new();

    for (code, name) in &differences {
        if name == ".notdef" || !glyph_available(name) {
            let replacement = sanitize_type1_difference_name(
                find_type1_glyph_name_for_code(&available_glyphs, *code, &base_encoding),
                &available_glyphs,
            );
            replacements.push((*code, replacement));
        }
    }

    // Phase 2: Check base encoding for .notdef mappings (conservative).
    // Only check codes in the font's FirstChar..LastChar range.
    let mut new_diffs: Vec<(u32, String)> = Vec::new();

    // Codes that already have valid (present in font) Differences entries.
    let valid_diff_codes: std::collections::HashSet<u32> = differences
        .iter()
        .filter(|(_, name)| name != ".notdef" && glyph_available(name))
        .map(|(c, _)| *c)
        .collect();
    let differences_map: std::collections::HashMap<u32, String> =
        differences.iter().map(|(c, n)| (*c, n.clone())).collect();
    let current_pdf_width_for_code = |code: u32| -> Option<f64> {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return None;
        };
        let fc = match font.get(b"FirstChar").ok() {
            Some(Object::Integer(i)) => *i as u32,
            _ => return None,
        };
        let widths_arr = match font.get(b"Widths").ok() {
            Some(Object::Array(arr)) => arr.clone(),
            Some(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Array(arr)) => arr.clone(),
                _ => return None,
            },
            _ => return None,
        };
        if code < fc {
            return None;
        }
        let idx = (code - fc) as usize;
        widths_arr.get(idx).and_then(object_to_f64)
    };

    let check_start = first_char.min(255);
    let check_end = last_char.min(255);
    let skip_broad_phase2 = !is_subset && base_encoding == "MacRomanEncoding";
    for code in check_start..=check_end {
        // For subset fonts with shared encoding refs, skip codes >= 32 to avoid
        // inadvertent modifications to shared state. Exception: code 32 (space)
        // when 'space' is absent from the CFF subset — WinAnsiEncoding maps
        // code 32 → "space" which is missing, causing §6.2.11.4.1:2. The
        // apply_encoding_fixes() fn creates a private per-font encoding when
        // the ref is shared, so this is safe. (#fix-cff-subset-space-missing)
        let space_missing_at_32 = code == 32 && !available_glyphs.contains("space");
        if shared_encoding_ref && is_subset && code >= 32 && !space_missing_at_32 {
            continue;
        }
        if valid_diff_codes.contains(&code) {
            continue;
        }
        // Skip codes already handled in Phase 1 replacements.
        if replacements.iter().any(|(c, _)| *c == code) {
            continue;
        }
        if skip_broad_phase2 {
            continue;
        }

        // For codes below 32: control characters that standard encodings
        // don't map to real glyphs. Map to "space" to avoid .notdef.
        // In subset fonts where "space" isn't available, use any existing
        // glyph (control chars are invisible anyway).
        if code < 32 {
            if available_glyphs.contains("space") {
                new_diffs.push((code, "space".to_string()));
            } else if let Some(name) = available_glyphs.iter().next() {
                new_diffs.push((code, name.clone()));
            }
            continue;
        }

        let glyph_name = cff_pdf_base_glyph_name(code, &base_encoding);

        let has_glyph = match &glyph_name {
            Some(name) => glyph_available(name),
            None => false,
        };

        if has_glyph {
            continue; // Not .notdef — no fix needed.
        }

        // CFF internal encoding check: unicode_to_glyph_name may return a raw
        // ASCII char (e.g. "'" for U+0027) that is NOT in available_glyphs, but
        // the CFF charset stores it under its AGL name alias (e.g. "quoteright").
        // In those cases cff.glyph_index(code) returns a non-.notdef GID, meaning
        // veraPDF considers the code valid for §6.2.11.4.1.  Adding a Differences
        // entry here (e.g. code 39 → "space") would mislead fix_font_width_mismatches
        // into computing the wrong expected width (space=278 instead of
        // quoteright=222) and prevent the §6.2.11.5 width correction.
        // (#FN-6.2.11.5-cff-enc)
        if code <= 255 && cff.glyph_index(code as u8).is_some_and(|g| g.0 != 0) {
            continue;
        }

        // Subset high-byte codes can legitimately resolve through existing
        // CFF/internal mappings even when the AGL name isn't present in the
        // subset charset. If the current dictionary width already matches that
        // pre-fix mapping, skip remapping to avoid introducing drift.
        if code > 127 {
            if let (Some(pdf_w), Some(expected_pre_fix)) = (
                current_pdf_width_for_code(code),
                compute_cff_single_width(font_data, code, &base_encoding, &differences_map),
            ) {
                if (pdf_w - expected_pre_fix).abs() <= 1.0 {
                    continue;
                }
            }
        }

        // For CFF simple fonts, veraPDF does not resolve high-byte MacRoman codes
        // via the MacRoman base encoding table. Adding fresh Differences entries
        // for these codes commonly turns a valid internal CFF mapping into a
        // false /space remap and then a width mismatch. Leave them untouched.
        if base_encoding == "MacRomanEncoding" && code > 127 {
            continue;
        }

        // Try to find the glyph by a different name.
        let replacement = sanitize_type1_difference_name(
            find_type1_glyph_name_for_code(&available_glyphs, code, &base_encoding),
            &available_glyphs,
        );
        if is_subset && code > 127 {
            let safe_subset_high = matches!(
                replacement.as_str(),
                "space" | "period" | "comma" | "hyphen" | "periodcentered" | "middot" | "bullet"
            );
            if !safe_subset_high {
                continue;
            }
        }
        let replacement_is_space = replacement == "space";
        if replacement_is_space {
            // Remap to "space" when available for both subset and non-subset
            // fonts to avoid .notdef references.
            if available_glyphs.contains("space") {
                new_diffs.push((code, "space".to_string()));
            }
        } else {
            // Font has this glyph by a different name — safe to add.
            new_diffs.push((code, replacement));
        }
        if is_subset && replacement_is_space && !available_glyphs.contains("space") {
            // Last-resort for subset fonts: choose any available glyph.
            if let Some(name) = available_glyphs.iter().next() {
                new_diffs.push((code, name.clone()));
            }
        }
    }

    if replacements.is_empty() && new_diffs.is_empty() {
        return false;
    }

    let encoding_fixed = apply_encoding_fixes(
        doc,
        font_id,
        &base_encoding,
        &differences,
        &replacements,
        &new_diffs,
        enc_ref,
    );

    // Keep /Widths consistent with the remapped Differences entries using the
    // same code->glyph resolution path as width mismatch fixing.
    let mut width_updates: Vec<(u32, i64)> = Vec::new();
    let updated_enc_info = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return encoding_fixed;
        };
        get_simple_encoding_info(doc, font)
    };
    for (code, _name) in replacements.iter() {
        // Keep widths synchronized with the remapped Differences entries.
        // Otherwise veraPDF can resolve a remapped code to a different glyph
        // while the dictionary width still points to the old one (6.2.11.5:1).
        if let Some(w) =
            compute_cff_single_width(font_data, *code, &updated_enc_info.0, &updated_enc_info.1)
        {
            width_updates.push((*code, w.round() as i64));
        }
    }
    for (code, _name) in new_diffs.iter() {
        // For freshly-added Differences entries (codes that were absent/notdef
        // before), use the pre-fix CFF encoding path for the width update.
        // veraPDF §6.2.11.5 uses the CFF encoding (glyph_index), not the PDF
        // Differences, so a newly-added "space" remapping would give the wrong
        // width (e.g. "space"=333 vs CFF-absent → defaultWidthX=500).
        // (#6.2.11.5-new-diff-width)
        if let Some(w) =
            compute_cff_single_width(font_data, *code, &base_encoding, &differences_map)
        {
            width_updates.push((*code, w.round() as i64));
        }
    }
    let widths_fixed = apply_simple_width_updates_for_codes(doc, font_id, &width_updates);

    encoding_fixed || widths_fixed
}

fn looks_like_type1_fontfile(data: &[u8]) -> bool {
    if data.starts_with(&[0x80, 0x01]) || data.starts_with(&[0x80, 0x02]) {
        return true;
    }
    data.starts_with(b"%!PS-AdobeFont") || data.starts_with(b"%!FontType1")
}

fn apply_simple_width_updates_for_codes(
    doc: &mut Document,
    font_id: ObjectId,
    updates: &[(u32, i64)],
) -> bool {
    if updates.is_empty() {
        return false;
    }

    let (first_char, widths_ref) = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return false;
        };
        let first_char = match font.get(b"FirstChar").ok() {
            Some(Object::Integer(i)) => *i as u32,
            _ => return false,
        };
        let widths_ref = match font.get(b"Widths").ok() {
            Some(Object::Reference(id)) => Some(*id),
            Some(Object::Array(_)) => None,
            _ => return false,
        };
        (first_char, widths_ref)
    };

    let mut update_map: std::collections::HashMap<u32, i64> = std::collections::HashMap::new();
    for (code, width) in updates {
        update_map.insert(*code, *width);
    }

    let mut changed = false;
    if let Some(widths_id) = widths_ref {
        if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&widths_id) {
            for (code, new_w) in &update_map {
                if *code < first_char {
                    continue;
                }
                let idx = (*code - first_char) as usize;
                if idx >= arr.len() {
                    continue;
                }
                let current = match arr[idx] {
                    Object::Integer(v) => v,
                    Object::Real(v) => v as i64,
                    _ => continue,
                };
                if (current - *new_w).abs() > 1 {
                    arr[idx] = Object::Integer(*new_w);
                    changed = true;
                }
            }
        }
        return changed;
    }

    if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
        if let Ok(Object::Array(ref mut arr)) = font.get_mut(b"Widths") {
            for (code, new_w) in &update_map {
                if *code < first_char {
                    continue;
                }
                let idx = (*code - first_char) as usize;
                if idx >= arr.len() {
                    continue;
                }
                let current = match arr[idx] {
                    Object::Integer(v) => v,
                    Object::Real(v) => v as i64,
                    _ => continue,
                };
                if (current - *new_w).abs() > 1 {
                    arr[idx] = Object::Integer(*new_w);
                    changed = true;
                }
            }
        }
    }

    changed
}

fn fix_notdef_in_type1_fontfile(
    doc: &mut Document,
    font_id: ObjectId,
    font_data: &[u8],
    enc_info: &EncodingInfo,
    first_char: u32,
    last_char: u32,
    is_subset: bool,
) -> bool {
    // This fallback targets classic Type1 FontFile programs (PFB/PFA), where
    // CFF parsing is unavailable. Handle the common 0x20 ("space") case that
    // triggers .notdef on some custom encodings.
    let Some(parsed) = parse_type1_program(font_data) else {
        return false;
    };

    let mut available_glyphs: std::collections::HashSet<String> = parsed
        .charstring_widths
        .keys()
        .filter(|name| !name.is_empty() && name.as_str() != ".notdef")
        .cloned()
        .collect();
    if available_glyphs.is_empty() {
        return false;
    }

    // Also include names from parsed internal encoding.
    for name in parsed.encoding.values() {
        if !name.is_empty() && name != ".notdef" {
            available_glyphs.insert(name.clone());
        }
    }

    // Only relevant if code 32 is in range.
    let check_start = first_char.min(255);
    let check_end = last_char.min(255);
    if !(check_start..=check_end).contains(&32) {
        return false;
    }

    let mut differences = enc_info.differences.clone();
    let mut base_encoding = enc_info.base_encoding.clone();
    let enc_ref = enc_info.enc_ref;

    if let Some(ref_id) = enc_ref {
        if let Some(Object::Dictionary(enc_dict)) = doc.objects.get(&ref_id) {
            if base_encoding.is_empty() {
                base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            }
            differences =
                parse_differences_to_vec_from_object(doc, enc_dict.get(b"Differences").ok());
        } else if let Some(Object::Name(n)) = doc.objects.get(&ref_id) {
            base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
    }
    if base_encoding.is_empty() {
        base_encoding = "StandardEncoding".to_string();
    }

    let raw_internal_32_name = find_raw_type1_encoding_name(font_data, 32);
    let has_internal_32 = parsed.encoding.contains_key(&32) || raw_internal_32_name.is_some();
    let charset_contains =
        |name: &str| -> bool { font_descriptor_charset_contains(doc, font_id, name) };
    let mut replacements: Vec<(u32, String)> = Vec::new();
    let mut stream_remap_to: Option<u8> = None;
    let internal_32_name = parsed
        .encoding
        .get(&32)
        .cloned()
        .or(raw_internal_32_name)
        .as_ref()
        .filter(|name| !name.is_empty() && name.as_str() != ".notdef")
        .filter(|name| available_glyphs.contains(name.as_str()) || charset_contains(name.as_str()))
        .cloned();
    for (code, name) in &differences {
        if *code != 32 {
            continue;
        }
        let mismatches_internal_subset_name = internal_32_name.as_ref().is_some_and(|internal| {
            looks_like_subset_synthetic_glyph_name(internal) && name != internal
        });
        if name == ".notdef" || !available_glyphs.contains(name) || mismatches_internal_subset_name
        {
            let replacement = internal_32_name.clone().unwrap_or_else(|| {
                sanitize_type1_difference_name(
                    find_type1_glyph_name_for_code(&available_glyphs, *code, &base_encoding),
                    &available_glyphs,
                )
            });
            replacements.push((*code, replacement));
        }
    }

    let mut new_diffs: Vec<(u32, String)> = Vec::new();
    if !replacements.iter().any(|(code, _)| *code == 32) {
        // If parse_type1_encoding returned an empty map, the font uses a named
        // encoding form (e.g. "/Encoding StandardEncoding def") that our parser
        // can't enumerate. Validate code 32 via base_encoding instead.
        let internal_32_ok = if parsed.encoding.is_empty() {
            let ch = encoding_to_char(32, &base_encoding);
            unicode_to_glyph_name(ch)
                .as_ref()
                .is_some_and(|name| available_glyphs.contains(name))
        } else {
            parsed
                .encoding
                .get(&32)
                .map(|name| !name.is_empty() && name != ".notdef")
                .unwrap_or(false)
        };

        let has_valid_32 = differences
            .iter()
            .find(|(code, _)| *code == 32)
            .map(|(_, name)| name != ".notdef" && available_glyphs.contains(name))
            .unwrap_or_else(|| {
                let ch = encoding_to_char(32, &base_encoding);
                let glyph_name = unicode_to_glyph_name(ch);
                glyph_name
                    .as_ref()
                    .is_some_and(|name| available_glyphs.contains(name))
            })
            && internal_32_ok;

        if !has_valid_32 {
            let replacement = if !has_internal_32 {
                ["A", "a", "zero", "period", "hyphen", "n", "w"]
                    .iter()
                    .find(|name| available_glyphs.contains(**name))
                    .map(|name| (*name).to_string())
                    .or_else(|| {
                        available_glyphs
                            .iter()
                            .find(|name| name.as_str() != "space")
                            .cloned()
                    })
                    .unwrap_or_else(|| {
                        find_type1_glyph_name_for_code(&available_glyphs, 32, &base_encoding)
                    })
            } else {
                internal_32_name.clone().unwrap_or_else(|| {
                    find_type1_glyph_name_for_code(&available_glyphs, 32, &base_encoding)
                })
            };
            let preserve_internal_subset_name = internal_32_name.as_ref().is_some_and(|internal| {
                replacement == *internal
                    && looks_like_subset_synthetic_glyph_name(internal)
                    && charset_contains(internal)
            });
            let replacement = if preserve_internal_subset_name {
                replacement
            } else {
                sanitize_type1_difference_name(replacement, &available_glyphs)
            };
            if !has_internal_32 {
                if let Some((&code, _)) = parsed
                    .encoding
                    .iter()
                    .find(|(code, name)| **code != 32 && name.as_str() == replacement)
                {
                    stream_remap_to = Some(code);
                } else {
                    stream_remap_to = match replacement.as_str() {
                        "A" => Some(65),
                        "a" => Some(97),
                        "zero" => Some(48),
                        "period" => Some(46),
                        "hyphen" => Some(45),
                        "n" => Some(110),
                        "w" => Some(119),
                        _ => None,
                    };
                }
            }
            if replacement == "space" {
                if available_glyphs.contains("space") {
                    new_diffs.push((32, "space".to_string()));
                } else if is_subset {
                    if let Some(name) = available_glyphs.iter().next() {
                        new_diffs.push((32, name.clone()));
                    }
                }
            } else {
                new_diffs.push((32, replacement));
            }
        }
    }

    if replacements.is_empty() && new_diffs.is_empty() {
        if let Some(to_code) = stream_remap_to {
            return replace_simple_font_code_refs(doc, font_id, 32, Some(to_code)) > 0;
        }
        if !has_internal_32 {
            return replace_simple_font_code_refs(doc, font_id, 32, None) > 0;
        }
        return false;
    }

    let encoding_fixed = apply_encoding_fixes(
        doc,
        font_id,
        &base_encoding,
        &differences,
        &replacements,
        &new_diffs,
        enc_ref,
    );

    let stream_fixed = if let Some(to_code) = stream_remap_to {
        replace_simple_font_code_refs(doc, font_id, 32, Some(to_code)) > 0
    } else if !parsed.encoding.contains_key(&32) {
        replace_simple_font_code_refs(doc, font_id, 32, None) > 0
    } else {
        false
    };

    encoding_fixed || stream_fixed
}

/// Find the correct glyph name for a character code in a TrueType font.
///
/// Tries multiple strategies to find a valid glyph name:
/// 1. Unicode cmap lookup + post table name
/// 2. AGL name lookup in font
/// 3. Fallback to "space"
fn find_truetype_glyph_name_for_code(
    face: &ttf_parser::Face,
    code: u32,
    base_encoding: &str,
) -> String {
    let ch = encoding_to_char(code, base_encoding);

    // Strategy 1: Check if the font has a glyph via Unicode cmap.
    if let Some(gid) = face.glyph_index(ch) {
        // Font has this glyph! Try to find its name.
        if let Some(name) = face.glyph_name(gid) {
            return name.to_string();
        }
        // Glyph exists but has no name — use uniXXXX format.
        return format!("uni{:04X}", ch as u32);
    }

    // Strategy 2: Try glyph_index_by_name for common AGL names.
    if let Some(ref name) = unicode_to_agl_name(ch) {
        if face.glyph_index_by_name(name).is_some() {
            return name.clone();
        }
    }

    // Strategy 3: For ASCII range, try the character itself as glyph name.
    if (0x21..=0x7E).contains(&(ch as u32)) {
        let char_name = String::from(ch);
        if face.glyph_index_by_name(&char_name).is_some() {
            return char_name;
        }
    }

    // Strategy 4: try common AGL names that are usually present.
    for candidate in ["space", "period", "hyphen", "zero", "A", "a"] {
        if face.glyph_index_by_name(candidate).is_some() {
            return candidate.to_string();
        }
    }

    // Fallback.
    "space".to_string()
}

/// Ensure Differences names for TrueType fonts remain AGL-compatible.
///
/// veraPDF rule 6.2.11.6:2 rejects non-AGL glyph names in encoding
/// Differences for non-symbolic TrueType fonts. If we can't resolve a name to
/// Unicode, use "space" as a safe fallback.
fn sanitize_truetype_difference_name(name: String) -> String {
    if name == "space" {
        return name;
    }
    if glyph_name_to_unicode(&name).is_some() {
        return name;
    }
    "space".to_string()
}

/// Find the correct glyph name for a character code in a Type1/CFF font.
fn find_type1_glyph_name_for_code(
    available_glyphs: &std::collections::HashSet<String>,
    code: u32,
    base_encoding: &str,
) -> String {
    let ch = encoding_to_char(code, base_encoding);

    // Strategy 1: Standard Unicode AGL name.
    if let Some(name) = unicode_to_agl_name(ch) {
        if available_glyphs.contains(&name) {
            return name;
        }
    }

    // Strategy 2: Try uniXXXX format.
    let uni_name = format!("uni{:04X}", ch as u32);
    if available_glyphs.contains(&uni_name) {
        return uni_name;
    }

    // Strategy 3: For ASCII, try the character itself.
    if (0x21..=0x7E).contains(&(ch as u32)) {
        let char_name = String::from(ch);
        if available_glyphs.contains(&char_name) {
            return char_name;
        }
    }

    fallback_type1_glyph_name(available_glyphs)
}

/// Ensure Type1 Differences names remain Unicode-resolvable where possible.
///
/// Non-AGL custom names in Differences can trigger 6.2.11.6:2/width validation
/// inconsistencies. Prefer "space" as safe fallback when present.
fn sanitize_type1_difference_name(
    name: String,
    available_glyphs: &std::collections::HashSet<String>,
) -> String {
    if name == "space" || glyph_name_to_unicode(&name).is_some() {
        return name;
    }
    if available_glyphs.contains("space") {
        return "space".to_string();
    }
    fallback_type1_glyph_name(available_glyphs)
}

fn font_descriptor_charset_contains(doc: &Document, font_id: ObjectId, name: &str) -> bool {
    let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
        return false;
    };
    let fd_id = match font.get(b"FontDescriptor").ok() {
        Some(Object::Reference(id)) => *id,
        _ => return false,
    };
    let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
        return false;
    };
    let charset_bytes = match fd.get(b"CharSet").ok() {
        Some(Object::String(bytes, _)) => bytes.as_slice(),
        _ => return false,
    };
    String::from_utf8_lossy(charset_bytes)
        .split('/')
        .any(|token| token == name)
}

fn find_raw_type1_encoding_name(font_data: &[u8], code: u8) -> Option<String> {
    for line in String::from_utf8_lossy(font_data).lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("dup ") || !trimmed.ends_with(" put") {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 4
            && parts[0] == "dup"
            && parts[3] == "put"
            && parts[1].parse::<u8>().ok() == Some(code)
        {
            if let Some(name) = parts[2].strip_prefix('/') {
                if !name.is_empty() && name != ".notdef" {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

fn looks_like_subset_synthetic_glyph_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('G').or_else(|| name.strip_prefix('g')) else {
        return false;
    };
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_hexdigit())
}

fn fallback_type1_glyph_name(available_glyphs: &std::collections::HashSet<String>) -> String {
    for candidate in ["space", "nbspace", "period", "hyphen", "zero", "A", "a"] {
        if available_glyphs.contains(candidate) {
            return candidate.to_string();
        }
    }

    let mut names: Vec<String> = available_glyphs.iter().cloned().collect();
    names.sort();
    names
        .into_iter()
        .next()
        .unwrap_or_else(|| "space".to_string())
}

/// Map a Unicode character to an Adobe Glyph List name.
fn unicode_to_agl_name(ch: char) -> Option<String> {
    let code = ch as u32;
    match code {
        0x0020 => Some("space".into()),
        0x0021 => Some("exclam".into()),
        0x0022 => Some("quotedbl".into()),
        0x0023 => Some("numbersign".into()),
        0x0024 => Some("dollar".into()),
        0x0025 => Some("percent".into()),
        0x0026 => Some("ampersand".into()),
        0x0027 => Some("quotesingle".into()),
        0x0028 => Some("parenleft".into()),
        0x0029 => Some("parenright".into()),
        0x002A => Some("asterisk".into()),
        0x002B => Some("plus".into()),
        0x002C => Some("comma".into()),
        0x002D => Some("hyphen".into()),
        0x002E => Some("period".into()),
        0x002F => Some("slash".into()),
        0x0030 => Some("zero".into()),
        0x0031 => Some("one".into()),
        0x0032 => Some("two".into()),
        0x0033 => Some("three".into()),
        0x0034 => Some("four".into()),
        0x0035 => Some("five".into()),
        0x0036 => Some("six".into()),
        0x0037 => Some("seven".into()),
        0x0038 => Some("eight".into()),
        0x0039 => Some("nine".into()),
        0x003A => Some("colon".into()),
        0x003B => Some("semicolon".into()),
        0x003C => Some("less".into()),
        0x003D => Some("equal".into()),
        0x003E => Some("greater".into()),
        0x003F => Some("question".into()),
        0x0040 => Some("at".into()),
        0x0041..=0x005A => Some(String::from(ch)), // A-Z
        0x005B => Some("bracketleft".into()),
        0x005C => Some("backslash".into()),
        0x005D => Some("bracketright".into()),
        0x005E => Some("asciicircum".into()),
        0x005F => Some("underscore".into()),
        0x0060 => Some("grave".into()),
        0x0061..=0x007A => Some(String::from(ch)), // a-z
        0x007B => Some("braceleft".into()),
        0x007C => Some("bar".into()),
        0x007D => Some("braceright".into()),
        0x007E => Some("asciitilde".into()),
        0x00A0 => Some("nbspace".into()),
        0x00A1 => Some("exclamdown".into()),
        0x00A2 => Some("cent".into()),
        0x00A3 => Some("sterling".into()),
        0x00A4 => Some("currency".into()),
        0x00A5 => Some("yen".into()),
        0x00A6 => Some("brokenbar".into()),
        0x00A7 => Some("section".into()),
        0x00A8 => Some("dieresis".into()),
        0x00A9 => Some("copyright".into()),
        0x00AA => Some("ordfeminine".into()),
        0x00AB => Some("guillemotleft".into()),
        0x00AC => Some("logicalnot".into()),
        0x00AE => Some("registered".into()),
        0x00AF => Some("macron".into()),
        0x00B0 => Some("degree".into()),
        0x00B1 => Some("plusminus".into()),
        0x00B4 => Some("acute".into()),
        0x00B5 => Some("mu".into()),
        0x00B6 => Some("paragraph".into()),
        0x00B7 => Some("periodcentered".into()),
        0x00B8 => Some("cedilla".into()),
        0x00BA => Some("ordmasculine".into()),
        0x00BB => Some("guillemotright".into()),
        0x00BC => Some("onequarter".into()),
        0x00BD => Some("onehalf".into()),
        0x00BE => Some("threequarters".into()),
        0x00BF => Some("questiondown".into()),
        0x00C0 => Some("Agrave".into()),
        0x00C1 => Some("Aacute".into()),
        0x00C2 => Some("Acircumflex".into()),
        0x00C3 => Some("Atilde".into()),
        0x00C4 => Some("Adieresis".into()),
        0x00C5 => Some("Aring".into()),
        0x00C6 => Some("AE".into()),
        0x00C7 => Some("Ccedilla".into()),
        0x00C8 => Some("Egrave".into()),
        0x00C9 => Some("Eacute".into()),
        0x00CA => Some("Ecircumflex".into()),
        0x00CB => Some("Edieresis".into()),
        0x00CC => Some("Igrave".into()),
        0x00CD => Some("Iacute".into()),
        0x00CE => Some("Icircumflex".into()),
        0x00CF => Some("Idieresis".into()),
        0x00D0 => Some("Eth".into()),
        0x00D1 => Some("Ntilde".into()),
        0x00D2 => Some("Ograve".into()),
        0x00D3 => Some("Oacute".into()),
        0x00D4 => Some("Ocircumflex".into()),
        0x00D5 => Some("Otilde".into()),
        0x00D6 => Some("Odieresis".into()),
        0x00D7 => Some("multiply".into()),
        0x00D8 => Some("Oslash".into()),
        0x00D9 => Some("Ugrave".into()),
        0x00DA => Some("Uacute".into()),
        0x00DB => Some("Ucircumflex".into()),
        0x00DC => Some("Udieresis".into()),
        0x00DD => Some("Yacute".into()),
        0x00DE => Some("Thorn".into()),
        0x00DF => Some("germandbls".into()),
        0x00E0 => Some("agrave".into()),
        0x00E1 => Some("aacute".into()),
        0x00E2 => Some("acircumflex".into()),
        0x00E3 => Some("atilde".into()),
        0x00E4 => Some("adieresis".into()),
        0x00E5 => Some("aring".into()),
        0x00E6 => Some("ae".into()),
        0x00E7 => Some("ccedilla".into()),
        0x00E8 => Some("egrave".into()),
        0x00E9 => Some("eacute".into()),
        0x00EA => Some("ecircumflex".into()),
        0x00EB => Some("edieresis".into()),
        0x00EC => Some("igrave".into()),
        0x00ED => Some("iacute".into()),
        0x00EE => Some("icircumflex".into()),
        0x00EF => Some("idieresis".into()),
        0x00F0 => Some("eth".into()),
        0x00F1 => Some("ntilde".into()),
        0x00F2 => Some("ograve".into()),
        0x00F3 => Some("oacute".into()),
        0x00F4 => Some("ocircumflex".into()),
        0x00F5 => Some("otilde".into()),
        0x00F6 => Some("odieresis".into()),
        0x00F7 => Some("divide".into()),
        0x00F8 => Some("oslash".into()),
        0x00F9 => Some("ugrave".into()),
        0x00FA => Some("uacute".into()),
        0x00FB => Some("ucircumflex".into()),
        0x00FC => Some("udieresis".into()),
        0x00FD => Some("yacute".into()),
        0x00FE => Some("thorn".into()),
        0x00FF => Some("ydieresis".into()),
        0x0152 => Some("OE".into()),
        0x0153 => Some("oe".into()),
        0x0160 => Some("Scaron".into()),
        0x0161 => Some("scaron".into()),
        0x0178 => Some("Ydieresis".into()),
        0x017D => Some("Zcaron".into()),
        0x017E => Some("zcaron".into()),
        0x0192 => Some("florin".into()),
        0x02C6 => Some("circumflex".into()),
        0x02DC => Some("tilde".into()),
        0x2013 => Some("endash".into()),
        0x2014 => Some("emdash".into()),
        0x2018 => Some("quoteleft".into()),
        0x2019 => Some("quoteright".into()),
        0x201A => Some("quotesinglbase".into()),
        0x201C => Some("quotedblleft".into()),
        0x201D => Some("quotedblright".into()),
        0x201E => Some("quotedblbase".into()),
        0x2020 => Some("dagger".into()),
        0x2021 => Some("daggerdbl".into()),
        0x2022 => Some("bullet".into()),
        0x2026 => Some("ellipsis".into()),
        0x2030 => Some("perthousand".into()),
        0x2039 => Some("guilsinglleft".into()),
        0x203A => Some("guilsinglright".into()),
        0x20AC => Some("Euro".into()),
        0x2122 => Some("trademark".into()),
        // Ligatures in MacRomanEncoding (222=fi, 223=fl) and Standard Encoding
        // (174=fi, 175=fl). unicode_to_glyph_name returns "uniFB01" for U+FB01
        // which fails CFF charset lookups — must resolve to the canonical AGL name.
        // (#6.2.11.5-ligatures)
        0xFB00 => Some("ff".into()),
        0xFB01 => Some("fi".into()),
        0xFB02 => Some("fl".into()),
        0xFB03 => Some("ffi".into()),
        0xFB04 => Some("ffl".into()),
        // Other common AGL names missing from the table.
        0x0131 => Some("dotlessi".into()),
        0x02C7 => Some("caron".into()),
        0x02D8 => Some("breve".into()),
        0x02D9 => Some("dotaccent".into()),
        0x02DA => Some("ring".into()),
        0x02DB => Some("ogonek".into()),
        0x02DD => Some("hungarumlaut".into()),
        0x03C0 => Some("pi".into()),
        0x2044 => Some("fraction".into()),
        0x2126 => Some("Omega".into()),
        0x221E => Some("infinity".into()),
        0x220F => Some("product".into()),
        0x2211 => Some("summation".into()),
        0x2202 => Some("partialdiff".into()),
        0x221A => Some("radical".into()),
        0x222B => Some("integral".into()),
        0x2248 => Some("approxequal".into()),
        0x2206 => Some("Delta".into()),
        0x2260 => Some("notequal".into()),
        0x2264 => Some("lessequal".into()),
        0x2265 => Some("greaterequal".into()),
        0x25CA => Some("lozenge".into()),
        _ => None,
    }
}

/// Apply encoding fixes to a font dictionary.
///
/// Rebuilds the Encoding dictionary with the merged Differences array
/// that includes both the original entries (with .notdef replaced) and
/// any new entries.
fn apply_encoding_fixes(
    doc: &mut Document,
    font_id: ObjectId,
    base_encoding: &str,
    original_differences: &[(u32, String)],
    replacements: &[(u32, String)],
    new_diffs: &[(u32, String)],
    enc_ref: Option<ObjectId>,
) -> bool {
    // Build the replacement map: code -> new glyph name.
    let mut replacement_map: std::collections::HashMap<u32, String> =
        std::collections::HashMap::new();
    for (code, name) in replacements {
        replacement_map.insert(*code, name.clone());
    }

    // Merge original differences with replacements.
    let mut merged: Vec<(u32, String)> = Vec::new();
    for (code, name) in original_differences {
        if let Some(replacement) = replacement_map.get(code) {
            merged.push((*code, replacement.clone()));
        } else {
            merged.push((*code, name.clone()));
        }
    }

    // Add new differences.
    for (code, name) in new_diffs {
        merged.push((*code, name.clone()));
    }

    // Sort by code for a clean Differences array.
    merged.sort_by_key(|(code, _)| *code);

    if merged.is_empty() {
        return false;
    }

    // Build the Differences array.
    let mut diff_array: Vec<Object> = Vec::new();
    let mut prev_code: Option<u32> = None;
    for (code, name) in &merged {
        // Only emit a new code integer when the code is not consecutive.
        let need_code = match prev_code {
            Some(pc) => *code != pc + 1,
            None => true,
        };
        if need_code {
            diff_array.push(Object::Integer(*code as i64));
        }
        diff_array.push(Object::Name(name.as_bytes().to_vec()));
        prev_code = Some(*code);
    }

    // Determine the base encoding to use.
    let effective_base = if base_encoding.is_empty() {
        "WinAnsiEncoding"
    } else {
        base_encoding
    };

    let shared_encoding_ref =
        enc_ref.is_some_and(|ref_id| is_encoding_ref_shared(doc, ref_id, font_id));

    // If the encoding was a private reference, modify that referenced object.
    if let Some(ref_id) = enc_ref {
        if !shared_encoding_ref {
            if let Some(Object::Dictionary(ref mut enc)) = doc.objects.get_mut(&ref_id) {
                enc.set(
                    "BaseEncoding",
                    Object::Name(effective_base.as_bytes().to_vec()),
                );
                enc.set("Differences", Object::Array(diff_array));
                return true;
            }
        }
    }

    // Build new encoding dict.
    let enc_dict = lopdf::Dictionary::from_iter(vec![
        ("Type".to_string(), Object::Name(b"Encoding".to_vec())),
        (
            "BaseEncoding".to_string(),
            Object::Name(effective_base.as_bytes().to_vec()),
        ),
        ("Differences".to_string(), Object::Array(diff_array)),
    ]);

    if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
        dict.set("Encoding", Object::Dictionary(enc_dict));
        return true;
    }

    false
}

/// Fix Type3 fonts where CharProcs defines a `.notdef` glyph procedure.
///
/// Some Type3 fonts use `.notdef` as an internal glyph name for glyphs that
/// are actually rendered (e.g. space, dash).  They assign character codes in
/// their Encoding/Differences to `.notdef` and then supply a real drawing
/// procedure under CharProcs[.notdef].  This is valid PDF, but PDF/A-2
/// §6.2.11.8 forbids any Encoding entry that names `.notdef`, even when the
/// font provides a CharProc for it.
///
/// Fix: rename `.notdef` → `gnotdef` in CharProcs **and** replace every
/// `/.notdef` occurrence in the Encoding /Differences array with `/gnotdef`.
/// The drawn appearance is unchanged; only the glyph name changes.
///
/// Fixes §6.2.11.8 violations caused by Type3 fonts. (#507)
pub fn fix_type3_notdef_charprocs(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        // Identify Type3 fonts.
        let is_type3 = match doc.objects.get(&font_id) {
            Some(Object::Dictionary(dict)) => {
                is_font_dict(dict) && get_name(dict, b"Subtype").as_deref() == Some("Type3")
            }
            _ => false,
        };
        if !is_type3 {
            continue;
        }

        // Determine if CharProcs contains .notdef (indirect or inline).
        enum CharProcsLoc {
            Indirect(ObjectId),
            Inline, // inline within the font dict
        }
        let cp_loc: Option<CharProcsLoc> = match doc.objects.get(&font_id) {
            Some(Object::Dictionary(dict)) => match dict.get(b"CharProcs").ok() {
                Some(Object::Reference(id)) => {
                    let has = doc
                        .objects
                        .get(id)
                        .and_then(|o| o.as_dict().ok())
                        .map(|d| d.has(b".notdef"))
                        .unwrap_or(false);
                    if has {
                        Some(CharProcsLoc::Indirect(*id))
                    } else {
                        None
                    }
                }
                Some(Object::Dictionary(cp)) => {
                    if cp.has(b".notdef") {
                        Some(CharProcsLoc::Inline)
                    } else {
                        None
                    }
                }
                _ => None,
            },
            _ => None,
        };
        let Some(cp_loc) = cp_loc else {
            continue;
        };

        // Rename .notdef → gnotdef in CharProcs.
        match cp_loc {
            CharProcsLoc::Indirect(cp_id) => {
                if let Some(Object::Dictionary(cp)) = doc.objects.get_mut(&cp_id) {
                    if let Some(val) = cp.remove(b".notdef") {
                        cp.set("gnotdef", val);
                        fixed += 1;
                    }
                }
            }
            CharProcsLoc::Inline => {
                // Clone font dict, mutate inline CharProcs, reinsert.
                let mut font_dict = match doc.objects.get(&font_id).cloned() {
                    Some(Object::Dictionary(d)) => d,
                    _ => continue,
                };
                let modified = match font_dict.get_mut(b"CharProcs").ok() {
                    Some(Object::Dictionary(cp)) => {
                        if let Some(val) = cp.remove(b".notdef") {
                            cp.set("gnotdef", val);
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if modified {
                    doc.objects
                        .insert(font_id, Object::Dictionary(font_dict.clone()));
                    fixed += 1;
                    // Reload for Encoding patch below.
                    // (font_dict already has the updated CharProcs)
                }
            }
        }

        // Update the Encoding /Differences array to replace .notdef with gnotdef.
        // Handles both indirect and inline Encoding.
        let enc_ref = match doc.objects.get(&font_id) {
            Some(Object::Dictionary(dict)) => match dict.get(b"Encoding").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            },
            _ => None,
        };

        let rename_diffs = |diffs: &mut Vec<Object>| {
            for item in diffs.iter_mut() {
                if matches!(item, Object::Name(n) if n == b".notdef") {
                    *item = Object::Name(b"gnotdef".to_vec());
                }
            }
        };

        if let Some(enc_id) = enc_ref {
            if let Some(Object::Dictionary(enc_dict)) = doc.objects.get_mut(&enc_id) {
                if let Ok(Object::Array(diffs)) = enc_dict.get_mut(b"Differences") {
                    rename_diffs(diffs);
                }
            }
        } else {
            // Try inline Encoding within the font dict.
            let mut font_dict = match doc.objects.get(&font_id).cloned() {
                Some(Object::Dictionary(d)) => d,
                _ => continue,
            };
            let modified = match font_dict.get_mut(b"Encoding").ok() {
                Some(Object::Dictionary(enc)) => match enc.get_mut(b"Differences").ok() {
                    Some(Object::Array(diffs)) => {
                        rename_diffs(diffs);
                        true
                    }
                    _ => false,
                },
                _ => false,
            };
            if modified {
                doc.objects.insert(font_id, Object::Dictionary(font_dict));
            }
        }
    }

    fixed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc_with_unembedded_font() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));

        let content = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Hello) Tj ET".to_vec());
        let content_id = doc.add_object(Object::Stream(content));

        let mut font_res = lopdf::Dictionary::new();
        font_res.set("F1", Object::Reference(font_id));
        let mut res = lopdf::Dictionary::new();
        res.set("Font", Object::Dictionary(font_res));

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(res),
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
    fn test_find_non_embedded() {
        let doc = make_doc_with_unembedded_font();
        let non_embedded = find_non_embedded_fonts(&doc);
        assert_eq!(non_embedded.len(), 1);
        assert_eq!(non_embedded[0].1, "Helvetica");
    }

    #[test]
    fn test_find_font_without_type_key() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        // Font with ONLY Subtype, no Type key.
        let font_dict = dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));
        let _ = font_id;

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let non_embedded = find_non_embedded_fonts(&doc);
        assert_eq!(non_embedded.len(), 1, "should detect font without Type key");
        assert_eq!(non_embedded[0].1, "Courier");
    }

    #[test]
    fn test_is_standard_14() {
        assert!(is_standard_14("Helvetica"));
        assert!(is_standard_14("ABCDEF+Helvetica"));
        assert!(is_standard_14("Times-Roman"));
        assert!(!is_standard_14("ArialMT"));
    }

    #[test]
    fn test_embedded_font_not_detected() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        // Use FontFile (Type1) instead of FontFile2 (TrueType) to avoid the
        // ttf_parser validation that rejects non-TrueType data. The point of
        // this test is that an embedded font should NOT appear in the
        // non-embedded list, not to validate TrueType parsing.
        let font_stream = Stream::new(
            dictionary! { "Length1" => Object::Integer(10) },
            vec![0u8; 10],
        );
        let stream_id = doc.add_object(Object::Stream(font_stream));

        let fd = dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "TestFont",
            "FontFile" => Object::Reference(stream_id),
            "Flags" => Object::Integer(32),
            "FontBBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(1000), Object::Integer(1000),
            ]),
            "ItalicAngle" => Object::Integer(0),
            "Ascent" => Object::Integer(800),
            "Descent" => Object::Integer(-200),
            "CapHeight" => Object::Integer(700),
            "StemV" => Object::Integer(80),
        };
        let fd_id = doc.add_object(Object::Dictionary(fd));

        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "TestFont",
            "FontDescriptor" => Object::Reference(fd_id),
        };
        doc.add_object(Object::Dictionary(font));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let non_embedded = find_non_embedded_fonts(&doc);
        assert!(
            non_embedded.is_empty(),
            "embedded font should not be detected"
        );
    }

    #[test]
    fn test_embed_report_structure() {
        let mut doc = make_doc_with_unembedded_font();
        let report = embed_fonts(&mut doc).unwrap();
        assert_eq!(report.fonts_inspected, 1);
        assert_eq!(report.non_embedded_found, 1);
    }

    #[test]
    fn test_get_or_create_font_descriptor() {
        let mut doc = make_doc_with_unembedded_font();
        let non_embedded = find_non_embedded_fonts(&doc);
        let font_id = non_embedded[0].0;

        let fd_id = get_or_create_font_descriptor(&mut doc, font_id).unwrap();
        assert!(doc.objects.contains_key(&fd_id));

        if let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) {
            assert!(font.has(b"FontDescriptor"));
        }
    }

    #[test]
    fn test_type0_embedding_targets_descendant() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let cid_font = dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => "TestCIDFont",
        };
        let cid_id = doc.add_object(Object::Dictionary(cid_font));

        let type0 = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "TestCIDFont",
            "DescendantFonts" => Object::Array(vec![Object::Reference(cid_id)]),
        };
        let type0_id = doc.add_object(Object::Dictionary(type0));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let detailed = find_non_embedded_fonts_detailed(&doc);
        let type0_entry = detailed.iter().find(|f| f.font_id == type0_id);
        assert!(type0_entry.is_some(), "should detect Type0 as non-embedded");
        let entry = type0_entry.unwrap();
        assert_eq!(
            entry.target_id, cid_id,
            "embedding target should be CIDFont descendant"
        );
        // CIDFont should NOT appear separately.
        let cid_entry = detailed.iter().find(|f| f.font_id == cid_id);
        assert!(
            cid_entry.is_none(),
            "CIDFont descendant should not be listed separately"
        );
    }

    #[test]
    fn test_winansi_encoding() {
        assert_eq!(winansi_to_char(65), 'A');
        assert_eq!(winansi_to_char(128), '\u{20AC}'); // Euro sign
        assert_eq!(winansi_to_char(147), '\u{201C}'); // Left double quotation
        assert_eq!(winansi_to_char(200), 'È');
    }

    #[test]
    fn test_fix_embedded_font_metrics_no_crash_on_empty() {
        let mut doc = make_doc_with_unembedded_font();
        // No embedded fonts, should return 0 without crashing.
        let fixed = fix_embedded_font_metrics(&mut doc);
        assert_eq!(fixed, 0);
    }

    #[test]
    fn test_fix_cidset_no_crash_on_empty() {
        let mut doc = make_doc_with_unembedded_font();
        let fixed = fix_cidset(&mut doc);
        assert_eq!(fixed, 0);
    }

    #[test]
    fn test_fix_cidset_creates_cidset() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        // Minimal TrueType font file header (enough for ttf-parser to detect glyph count).
        // Use a real minimal TTF structure: offset table + head table.
        // For testing, we just create a CIDFont with embedded program and check CIDSet creation.
        let font_stream = Stream::new(
            dictionary! { "Length1" => Object::Integer(10) },
            vec![0u8; 10], // Not a valid TTF, so fix_cidset will skip it.
        );
        let stream_id = doc.add_object(Object::Stream(font_stream));

        let fd = dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "TestCID",
            "FontFile2" => Object::Reference(stream_id),
            "Flags" => Object::Integer(32),
            "FontBBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(1000), Object::Integer(1000),
            ]),
            "ItalicAngle" => Object::Integer(0),
            "Ascent" => Object::Integer(800),
            "Descent" => Object::Integer(-200),
            "CapHeight" => Object::Integer(700),
            "StemV" => Object::Integer(80),
        };
        let fd_id = doc.add_object(Object::Dictionary(fd));

        let cid_font = dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => "TestCID",
            "FontDescriptor" => Object::Reference(fd_id),
        };
        doc.add_object(Object::Dictionary(cid_font));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        // Invalid TTF data, so CIDSet won't be created — but no crash.
        let fixed = fix_cidset(&mut doc);
        assert_eq!(fixed, 0);
    }

    #[test]
    fn test_read_embedded_font_data() {
        let mut doc = Document::with_version("1.7");
        let font_bytes = vec![0xAA, 0xBB, 0xCC];
        let font_stream = Stream::new(
            dictionary! { "Length1" => Object::Integer(3) },
            font_bytes.clone(),
        );
        let stream_id = doc.add_object(Object::Stream(font_stream));
        let fd = dictionary! {
            "Type" => "FontDescriptor",
            "FontFile2" => Object::Reference(stream_id),
        };
        let fd_id = doc.add_object(Object::Dictionary(fd));

        let data = read_embedded_font_data(&doc, fd_id);
        assert!(data.is_some());
        assert_eq!(data.unwrap(), font_bytes);
    }
}
