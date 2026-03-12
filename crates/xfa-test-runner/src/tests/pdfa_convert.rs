use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::{PdfTest, TestResult, TestStatus};

/// PDF/A conversion roundtrip: convert to PDF/A-2b, validate with our checker
/// and optionally with veraPDF oracle.
pub struct PdfAConvertTest {
    verapdf: Option<Arc<crate::oracles::verapdf::VeraPdfOracle>>,
    progress: Arc<Mutex<String>>,
}

impl PdfAConvertTest {
    pub fn new() -> Self {
        Self {
            verapdf: None,
            progress: Arc::new(Mutex::new(String::new())),
        }
    }

    pub fn with_verapdf(mut self, oracle: Arc<crate::oracles::verapdf::VeraPdfOracle>) -> Self {
        self.verapdf = Some(oracle);
        self
    }
}

impl PdfTest for PdfAConvertTest {
    fn name(&self) -> &str {
        "pdfa_convert"
    }

    fn progress_tracker(&self) -> Option<Arc<Mutex<String>>> {
        Some(self.progress.clone())
    }

    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;

        let set_progress = |msg: &str| {
            if let Ok(mut p) = self.progress.lock() {
                *p = msg.to_string();
            }
        };

        set_progress("parsing");

        // 1. Check if already PDF/A-compliant — skip.
        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("pdf-syntax parse failed".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        if pdf_compliance::detect_pdfa_level(&pdf).is_some() {
            return TestResult {
                status: TestStatus::Pass,
                error_message: Some("already PDF/A".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // 2. Load via lopdf for mutation.
        set_progress("lopdf_load");
        let mut doc = match lopdf::Document::load_mem(pdf_data) {
            Ok(d) if !d.objects.is_empty() => d,
            Ok(_) | Err(_) => {
                // Fallback: try repair strategies (strip garbage, fix xref, rebuild from objects).
                match try_repair_for_lopdf(pdf_data) {
                    Some(d) => d,
                    None => {
                        return TestResult {
                            status: TestStatus::Skip,
                            error_message: Some("lopdf load failed (all repairs exhausted)".into()),
                            duration_ms: elapsed(),
                            oracle_score: None,
                            metadata: HashMap::new(),
                        };
                    }
                }
            }
        };

        // 2a. If lopdf lost pages, try sanitizing #-encoded names and reloading.
        // Only applied when normal load produces 0 pages, since # replacement can
        // break valid PDFs that use legitimate #XX hex encoding.
        if doc.get_pages().is_empty() && raw_has_hash_names(pdf_data) {
            let sanitized = sanitize_hash_names_raw(pdf_data);
            if let Ok(d2) = lopdf::Document::load_mem(&sanitized) {
                if !d2.get_pages().is_empty() {
                    doc = d2;
                }
            }
        }

        // Fix wrong Root reference (corrupt trailer may point to non-Catalog object).
        fix_wrong_root(&mut doc);

        if doc.get_pages().is_empty() {
            // Fallback: try adding missing /Type /Page entries to page-like objects.
            try_fix_missing_page_types(&mut doc);
        }

        // 2b. Handle encrypted documents: try empty password decryption.
        if doc.trailer.get(b"Encrypt").is_ok() {
            match doc.decrypt("") {
                Ok(()) => {
                    doc.trailer.remove(b"Encrypt");
                }
                Err(_) => {
                    return TestResult {
                        status: TestStatus::Skip,
                        error_message: Some("encrypted PDF (decryption failed)".into()),
                        duration_ms: elapsed(),
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            }
        }

        // 3. Run PDF/A conversion pipeline.
        // Run cleanup BEFORE the final page count check — cleanup removes encryption
        // which can prevent lopdf from seeing pages.
        set_progress("cleanup");
        let cleanup_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false)
        }));
        let cleanup_report = match cleanup_result {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("cleanup failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic in cleanup_for_pdfa".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        // After cleanup (which removes encryption), retry page detection.
        if doc.get_pages().is_empty() {
            try_fix_missing_page_types(&mut doc);
            // If still empty, proceed anyway — the pipeline handles empty page
            // trees gracefully, and veraPDF will catch genuinely broken PDFs.
        }

        // 3a. Embed non-embedded fonts.
        set_progress("font_embed");
        let font_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::embed_fonts(&mut doc)
        }));
        let font_report = match font_result {
            Ok(Ok(r)) => Some(r),
            _ => None,
        };

        // NOTE: fix_width_mismatches disabled — causes regression on simple TrueType fonts.
        // CFF-only and CIDFontType2 width fixing is safe.
        set_progress("cff_widths");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc)
        }));

        set_progress("tt_cid_widths");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc)
        }));

        // 3a1b. Fix Type1 CharSet from CFF program.
        set_progress("charset");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc)
        }));

        // 3a2. Fix TrueType encoding for non-symbolic fonts.
        set_progress("font_encoding");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc)
        }));

        // 3a2a. Add Unicode (3,1) cmap to TrueType fonts that only have Mac Roman (1,0).
        set_progress("unicode_cmap");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc)
        }));

        // 3a2b. Fix .notdef glyph references (6.2.11.8:1).
        set_progress("notdef_refs");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc)
        }));

        // 3a2c. Fix .notdef in CID fonts by modifying content streams (6.2.11.8:1).
        set_progress("cid_notdef");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc)
        }));

        // 3a2d. Fix .notdef in symbolic simple fonts via content stream modification.
        set_progress("symbolic_notdef");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc)
        }));

        // 3a2d2. Strip control characters from content streams (catches remaining
        // .notdef refs from PFB fonts where glyph availability is unknown).
        set_progress("strip_control");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc)
        }));

        // 3a2e. Ensure undefined WinAnsi codes have Differences entries.
        set_progress("undef_encoding");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc)
        }));

        // 3a2f. Fix incorrect Symbolic flags on non-symbolic CFF fonts.
        set_progress("symbolic_flags");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc)
        }));

        // 3a2g. Populate missing FirstChar/LastChar/Widths for embedded fonts.
        set_progress("missing_widths");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc)
        }));

        // 3a3. Conservative width mismatch fix for simple TrueType/Type1 fonts.
        // Only updates individual mismatched width entries; skips unreliable mappings.
        // Also handles subset fonts (ABCDEF+FontName).
        set_progress("width_mismatches");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc)
        }));

        // 3a3b. Fix symbolic font widths (ZapfDingbats, Symbol) with CFF/TT programs.
        set_progress("symbolic_widths");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc)
        }));

        // 3a5. Fix CIDSet for CID fonts.
        set_progress("cidset");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_cidset(&mut doc)
        }));

        // 3b. Normalize color spaces: add sRGB OutputIntent if missing.
        set_progress("colorspace");
        let colorspace_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc)
        }));
        match colorspace_result {
            Ok(Ok(cs_report)) => {
                if cs_report.output_intent_added {
                    // OutputIntent was added — good.
                    let _ = cs_report;
                }
            }
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("colorspace normalization failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic in normalize_colorspaces".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        }

        // 3c. Supplementary PDF/A fixups (small rule fixes).
        set_progress("fixups");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fixups::run_fixups(&mut doc)
        }));

        // 3c1. Re-normalize color spaces after fixups. Some fixups can introduce
        // or update DeviceN/Separation structures, so run 6.2.4.4 consistency
        // checks once more before final validation.
        set_progress("colorspace_post_fixups");
        let colorspace_post_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc)
        }));
        match colorspace_post_result {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("post-fixups colorspace normalization failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic in post-fixups normalize_colorspaces".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        }

        set_progress("xmp_repair");
        let xmp_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_xmp::repair_xmp_metadata(
                &mut doc,
                pdf_manip::pdfa_xmp::PdfAConformance::A2b,
                None,
            )
        }));
        match xmp_result {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("xmp repair failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic in repair_xmp_metadata".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        }

        // 4. Save.
        set_progress("save");
        let mut saved = Vec::new();
        if let Err(e) = doc.save_to(&mut saved) {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("save failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // 4b. Fix PDF header for PDF/A compliance (binary comment).
        pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
        // 4c. Fix startxref offset (lopdf can write wrong values).
        pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);



        // 5. Validate with our own checker.
        set_progress("validate_own");
        let pdf2 = match pdf_syntax::Pdf::new(saved.clone()) {
            Ok(p) => p,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("reparse failed: {e:?}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let report = pdf_compliance::validate_pdfa(&pdf2, pdf_compliance::PdfALevel::A2b);

        let mut metadata = HashMap::new();
        metadata.insert("own_errors".into(), report.issues.len().to_string());
        metadata.insert("own_compliant".into(), report.compliant.to_string());
        metadata.insert(
            "js_removed".into(),
            cleanup_report.js_actions_removed.to_string(),
        );
        metadata.insert(
            "cidtogidmap_added".into(),
            cleanup_report.cidtogidmap_added.to_string(),
        );
        metadata.insert("ap_fixes".into(), cleanup_report.ap_fixes.to_string());
        if let Some(ref fr) = font_report {
            metadata.insert("fonts_embedded".into(), fr.fonts_embedded.to_string());
            metadata.insert("fonts_failed".into(), fr.failed.len().to_string());
        }

        // 6. Validate with veraPDF oracle if available.
        if let Some(verapdf) = &self.verapdf {
            set_progress("validate_verapdf");

            // Write to temp file for veraPDF.
            let tmp = match write_temp_pdf(&saved, path) {
                Some(p) => p,
                None => {
                    metadata.insert("verapdf".into(), "temp_write_failed".into());
                    return TestResult {
                        status: if report.compliant {
                            TestStatus::Pass
                        } else {
                            TestStatus::Fail
                        },
                        error_message: if report.compliant {
                            None
                        } else {
                            Some(format!("{} compliance issues", report.issues.len()))
                        },
                        duration_ms: elapsed(),
                        oracle_score: None,
                        metadata,
                    };
                }
            };

            // Compute a hash for cache key.
            let hash = {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(&saved);
                format!("{:x}", hasher.finalize())
            };
            let oracle_result = verapdf.validate(&tmp, &hash);
            let _ = std::fs::remove_file(&tmp);

            match oracle_result {
                Ok(verapdf_report) => {
                    let oracle_errors = verapdf_report.failed_rules as usize;
                    metadata.insert("verapdf_errors".into(), oracle_errors.to_string());
                    if !verapdf_report.rule_failures.is_empty() {
                        let failed_rules: Vec<String> = verapdf_report
                            .rule_failures
                            .iter()
                            .map(|r| format!("{}:{}", r.clause, r.test_number))
                            .collect();
                        metadata.insert("verapdf_failed_rules".into(), failed_rules.join("|"));
                        if let Some(first) = failed_rules.first() {
                            metadata.insert("verapdf_first_rule".into(), first.clone());
                        }
                    }

                    if oracle_errors == 0 {
                        return TestResult {
                            status: TestStatus::Pass,
                            error_message: None,
                            duration_ms: elapsed(),
                            oracle_score: Some(1.0),
                            metadata,
                        };
                    } else {
                        return TestResult {
                            status: TestStatus::Fail,
                            error_message: Some(format!(
                                "{oracle_errors} veraPDF errors after conversion"
                            )),
                            duration_ms: elapsed(),
                            oracle_score: Some(0.0),
                            metadata,
                        };
                    }
                }
                Err(e) => {
                    metadata.insert("verapdf".into(), format!("error: {e}"));
                }
            }
        }

        // Fallback: use our own checker result.
        if report.compliant {
            TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        } else {
            let issue_details: Vec<String> = report
                .issues
                .iter()
                .take(5)
                .map(|i| format!("{}: {}", i.rule, i.message))
                .collect();
            TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!(
                    "{} compliance issues: {}",
                    report.issues.len(),
                    issue_details.join("; ")
                )),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        }
    }
}

/// Write bytes to a temp file next to the original PDF.
fn write_temp_pdf(data: &[u8], original: &Path) -> Option<std::path::PathBuf> {
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("tmp");
    let dir = std::env::temp_dir();
    let safe_stem: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    for attempt in 0..8u8 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        let pid = std::process::id();
        let path = dir.join(format!(
            "{safe_stem}_{pid}_{nanos}_{attempt}_pdfa_converted.pdf"
        ));

        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                if f.write_all(data).is_ok() {
                    return Some(path);
                }
                let _ = std::fs::remove_file(&path);
                return None;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }

    None
}

/// Accept a loaded Document only if it has at least 1 object.
/// lopdf sometimes "succeeds" loading corrupt data but finds 0 objects.
fn accept_doc(doc: lopdf::Document) -> Option<lopdf::Document> {
    if doc.objects.is_empty() {
        None
    } else {
        Some(doc)
    }
}

/// Try to load PDF data with lopdf, accepting only non-empty documents.
fn try_load(data: &[u8]) -> Option<lopdf::Document> {
    lopdf::Document::load_mem(data).ok().and_then(accept_doc)
}

/// Try to repair PDF data so that lopdf can load it.
///
/// Strategies:
/// Pre-process raw PDF bytes: replace `#` in PDF name tokens with `_`.
///
/// lopdf cannot parse names with `#XX` hex escaping (e.g. `/Im#22`), dropping
/// the entire containing object. We replace `#` with `_` in name contexts,
/// which is a 1-byte substitution that preserves xref offsets.
///
/// Only modifies bytes that are inside name tokens (after `/`), never inside
/// stream content, string literals, or comments. Since content streams also
/// reference these names (e.g. `/Im#22 Do`), the replacement is consistent.
/// Quick check: does the raw PDF data contain `#` in name-like positions?
fn raw_has_hash_names(data: &[u8]) -> bool {
    // Look for `/Name#` patterns — a `/` followed by alphanumeric chars and then `#`.
    let mut i = 0;
    while i + 3 < data.len() {
        if data[i] == b'/' {
            i += 1;
            while i < data.len() && data[i].is_ascii_alphanumeric() {
                i += 1;
            }
            if i < data.len() && data[i] == b'#' {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// Replace `#` with `_` in all PDF name tokens (after `/`), both in dict
/// keys and content stream operators. This is a 1-byte substitution that
/// preserves xref offsets and stream lengths.
///
/// Skips: string literals `(...)`, hex strings `<...>`, and `%` comments.
/// Processes content inside streams (since content stream operators like
/// `/Im#22 Do` need the same renaming as their Resource dict keys).
fn sanitize_hash_names_raw(data: &[u8]) -> Vec<u8> {
    let mut result = data.to_vec();
    let mut i = 0;
    let mut in_string = 0i32;
    let mut in_hex_string = false;

    while i < result.len() {
        // Track string literals (parentheses).
        if result[i] == b'(' && !in_hex_string {
            in_string += 1;
            i += 1;
            continue;
        }
        if result[i] == b')' && in_string > 0 {
            in_string -= 1;
            i += 1;
            continue;
        }
        if result[i] == b'\\' && in_string > 0 {
            i += 2;
            continue;
        }
        if in_string > 0 {
            i += 1;
            continue;
        }

        // Skip dict delimiters << and >>.
        if result[i] == b'<' && i + 1 < result.len() && result[i + 1] == b'<' {
            i += 2;
            continue;
        }
        if result[i] == b'>' && i + 1 < result.len() && result[i + 1] == b'>' {
            i += 2;
            continue;
        }

        // Track hex strings.
        if result[i] == b'<' {
            in_hex_string = true;
            i += 1;
            continue;
        }
        if result[i] == b'>' && in_hex_string {
            in_hex_string = false;
            i += 1;
            continue;
        }
        if in_hex_string {
            i += 1;
            continue;
        }

        // Skip comments (but not %PDF header or %%EOF).
        if result[i] == b'%' {
            while i < result.len() && result[i] != b'\n' && result[i] != b'\r' {
                i += 1;
            }
            continue;
        }

        // Name token: replace `#` with `_`.
        if result[i] == b'/' {
            i += 1;
            while i < result.len() && !is_name_delimiter(result[i]) {
                if result[i] == b'#' {
                    result[i] = b'_';
                }
                i += 1;
            }
            continue;
        }

        i += 1;
    }

    result
}

fn is_name_delimiter(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t' | b'\n' | b'\r' | b'\0' | b'/' | b'[' | b']' | b'(' | b')' | b'<' | b'>'
            | b'{' | b'}' | b'%'
    )
}

/// 1. Strip garbage bytes before the %PDF header (offset header).
/// 2. Try appending a minimal %%EOF + startxref if missing/broken.
/// 3. Try truncating trailing garbage after the last %%EOF.
fn try_repair_for_lopdf(data: &[u8]) -> Option<lopdf::Document> {
    // Strategy 1: Find %PDF- offset and strip leading garbage.
    let offset = data.windows(5).position(|w| w == b"%PDF-")?;

    if offset > 0 {
        let trimmed = &data[offset..];
        if let Some(doc) = try_load(trimmed) {
            return Some(doc);
        }
        // Continue with trimmed data for further strategies.
        return try_repair_xref(trimmed);
    }

    try_repair_xref(data)
}

/// Try to fix xref/trailer issues in PDF data.
///
/// Strategies:
/// - Find the last `startxref` and `%%EOF`, and verify the xref offset.
///   If the offset is wrong, try to fix it.
/// - If `%%EOF` is missing, append one.
/// - If `startxref` points to wrong location, try scanning for actual xref position.
fn try_repair_xref(data: &[u8]) -> Option<lopdf::Document> {
    // Look for "startxref" in the last part of the file.
    let search_start = data.len().saturating_sub(4096);
    let tail = &data[search_start..];

    // Find the last startxref.
    let startxref_pos = tail
        .windows(9)
        .rposition(|w| w == b"startxref")
        .map(|p| search_start + p);

    if let Some(sxr) = startxref_pos {
        // Read the xref offset value after "startxref".
        let after = &data[sxr + 9..];
        let offset_str: String = after
            .iter()
            .skip_while(|b| b.is_ascii_whitespace())
            .take_while(|b| b.is_ascii_digit())
            .map(|&b| b as char)
            .collect();

        if let Ok(xref_offset) = offset_str.parse::<usize>() {
            // Check if the xref offset actually points to "xref" or a valid xref stream.
            if xref_offset < data.len() {
                let at_offset = &data[xref_offset..];
                let has_valid_xref = if at_offset.starts_with(b"xref") {
                    // Check it's not an empty xref (immediately followed by "trailer").
                    let after_keyword = &at_offset[4..];
                    let trimmed = after_keyword
                        .iter()
                        .position(|b| !b.is_ascii_whitespace())
                        .map(|p| &after_keyword[p..])
                        .unwrap_or(b"");
                    !trimmed.starts_with(b"trailer")
                } else if at_offset.len() > 10 && at_offset[0].is_ascii_digit() {
                    // Could be a cross-reference stream: "N 0 obj".
                    // Distinguish from xref table entries ("0000NNNNNN 00000 n").
                    // xref entries have a 10-digit offset; xref streams have "N 0 obj".
                    at_offset
                        .windows(5)
                        .take(20)
                        .any(|w| w == b"0 obj" || w == b"0 OBJ")
                } else {
                    false
                };

                if !has_valid_xref {
                    // The offset is wrong. Try to find the actual xref position.
                    if let Some(real_xref) = find_last_xref_pos(data) {
                        let mut repaired = data.to_vec();
                        // Rebuild the startxref section.
                        repaired.truncate(sxr);
                        repaired.extend_from_slice(
                            format!("startxref\n{real_xref}\n%%EOF\n").as_bytes(),
                        );
                        if let Some(doc) = try_load(&repaired) {
                            return Some(doc);
                        }
                        // If it still fails, try fixing xref line endings too.
                        if let Some(double_repaired) = try_fix_xref_line_endings(&repaired) {
                            if let Some(doc) = try_load(&double_repaired) {
                                return Some(doc);
                            }
                        }
                    }
                }
            }
        }
    }

    // Strategy: if no %%EOF in last 512 bytes but startxref exists earlier in file,
    // try appending %%EOF.
    let last_512 = &data[data.len().saturating_sub(512)..];
    let has_eof = last_512.windows(5).any(|w| w == b"%%EOF");

    if !has_eof {
        if let Some(real_xref) = find_last_xref_pos(data) {
            let mut repaired = data.to_vec();
            repaired.extend_from_slice(format!("\nstartxref\n{real_xref}\n%%EOF\n").as_bytes());
            if let Some(doc) = try_load(&repaired) {
                return Some(doc);
            }
        }
    }

    // Strategy: corrupt trailer dict (e.g., filled with `<<<<...`).
    // Reconstruct trailer from xref table by scanning for /Root reference.
    if let Some(repaired) = try_rebuild_trailer(data) {
        if let Some(doc) = try_load(&repaired) {
            return Some(doc);
        }
    }

    // Strategy: corrupt /Size value (e.g., "/Size (h)" instead of "/Size 80").
    // Fix by computing the correct size from the xref table.
    if let Some(repaired) = try_fix_trailer_size(data) {
        if let Some(doc) = try_load(&repaired) {
            return Some(doc);
        }
    }

    // Strategy: comment after startxref value (e.g., "23291 %comment").
    // Strip non-digit chars after the offset number.
    if let Some(repaired) = try_fix_startxref_comment(data) {
        if let Some(doc) = try_load(&repaired) {
            return Some(doc);
        }
    }

    // Strategy: xref entries with wrong line endings (19 bytes with LF only
    // instead of 20 bytes with CR+LF). Fix by normalizing to CR+LF.
    if let Some(repaired) = try_fix_xref_line_endings(data) {
        if let Some(doc) = try_load(&repaired) {
            return Some(doc);
        }
    }

    // Final strategy: rebuild xref from scratch by scanning for "N G obj" markers.
    // This handles PDFs with completely corrupt xref tables (wrong offsets, etc.).
    if let Some(doc) = try_rebuild_xref_from_objects(data) {
        return Some(doc);
    }

    None
}

/// Rebuild a PDF's xref table from scratch by scanning for object definitions.
///
/// Scans the file body for "N G obj" patterns, collects all object offsets,
/// finds /Root (Catalog), and builds a new valid xref table + trailer.
fn try_rebuild_xref_from_objects(data: &[u8]) -> Option<lopdf::Document> {
    // Scan for "N 0 obj" patterns (generation 0, which is most common).
    let mut objects: Vec<(u32, usize)> = Vec::new(); // (obj_num, offset)
    let mut root_ref: Option<u32> = None;
    let mut info_ref: Option<u32> = None;

    let mut pos = 0;
    while pos + 10 < data.len() {
        // Look for a digit followed by " 0 obj".
        if data[pos].is_ascii_digit() {
            // Check if this position starts a line (or is at the start).
            let at_line_start = pos == 0 || data[pos - 1] == b'\n' || data[pos - 1] == b'\r';
            if at_line_start {
                // Try to parse "N 0 obj" or "N G obj".
                let end = std::cmp::min(pos + 20, data.len());
                let chunk = &data[pos..end];
                if let Some(obj_info) = parse_obj_marker(chunk) {
                    objects.push((obj_info.0, pos));
                    // Check if this object is /Type /Catalog.
                    let obj_end = std::cmp::min(pos + 4096, data.len());
                    let obj_data = &data[pos..obj_end];
                    if obj_data.windows(14).any(|w| w == b"/Type /Catalog" || w == b"/Type/Catalog") {
                        root_ref = Some(obj_info.0);
                    }
                    if obj_data.windows(8).any(|w| w == b"/Author " || w == b"/Creator")
                        && obj_data.windows(14).any(|w| w == b"/CreationDate " || w == b"/ModDate ")
                        && info_ref.is_none()
                    {
                        info_ref = Some(obj_info.0);
                    }
                    // Skip past "obj" to avoid re-matching.
                    pos += 5;
                    continue;
                }
            }
        }
        pos += 1;
    }

    if objects.is_empty() || root_ref.is_none() {
        return None;
    }

    // Sort by object number.
    objects.sort_by_key(|&(num, _)| num);
    objects.dedup_by_key(|o| o.0);

    let max_obj = objects.last().map(|o| o.0).unwrap_or(0);
    let size = max_obj + 1;

    // Build xref table.
    let mut xref_entries = Vec::new();
    // Entry 0: free list head. Exactly 20 bytes: 10+1+5+1+1+CR+LF.
    xref_entries.push(format!("{:010} {:05} f\r\n", 0, 65535));

    let obj_map: std::collections::HashMap<u32, usize> =
        objects.iter().cloned().collect();

    for num in 1..size {
        if let Some(&offset) = obj_map.get(&num) {
            xref_entries.push(format!("{:010} {:05} n\r\n", offset, 0));
        } else {
            xref_entries.push(format!("{:010} {:05} f\r\n", 0, 0));
        }
    }

    // Find %PDF header for clean data start.
    let header_start = data.windows(5).position(|w| w == b"%PDF-").unwrap_or(0);
    let body = &data[header_start..];

    // Find the end of the last object (before any existing xref/trailer).
    let body_end = if let Some(xp) = find_last_xref_pos(body) {
        xp
    } else {
        body.len()
    };

    // Build new PDF: original body + new xref + trailer.
    let mut repaired = body[..body_end].to_vec();
    // Ensure newline before xref.
    if !repaired.ends_with(b"\n") {
        repaired.push(b'\n');
    }
    let xref_pos = repaired.len();
    repaired.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
    for entry in &xref_entries {
        repaired.extend_from_slice(entry.as_bytes());
    }

    // Trailer.
    let root = root_ref.unwrap();
    repaired.extend_from_slice(b"trailer\n<<\n");
    repaired.extend_from_slice(format!("/Size {size}\n/Root {root} 0 R\n").as_bytes());
    if let Some(info) = info_ref {
        repaired.extend_from_slice(format!("/Info {info} 0 R\n").as_bytes());
    }
    repaired.extend_from_slice(b">>\n");
    repaired.extend_from_slice(format!("startxref\n{xref_pos}\n%%EOF\n").as_bytes());

    try_load(&repaired)
}

/// Parse "N G obj" at the start of a byte slice. Returns (obj_num, gen_num) if valid.
fn parse_obj_marker(data: &[u8]) -> Option<(u32, u16)> {
    let s = std::str::from_utf8(data).ok()?;
    let mut parts = s.split_whitespace();
    let num: u32 = parts.next()?.parse().ok()?;
    let gen: u16 = parts.next()?.parse().ok()?;
    let keyword = parts.next()?;
    if keyword == "obj" {
        Some((num, gen))
    } else {
        None
    }
}

/// Rebuild a corrupt trailer by scanning for /Root in the xref range.
fn try_rebuild_trailer(data: &[u8]) -> Option<Vec<u8>> {
    // Find the xref table.
    let xref_pos = find_last_xref_pos(data)?;
    let after_xref = &data[xref_pos..];

    // Parse the xref subsection header: "xref\nSTART COUNT\n"
    let header_end = after_xref
        .windows(1)
        .skip(5) // "xref\n"
        .position(|w| w[0] == b'\n')
        .map(|p| p + 5 + 1)?;

    let header_line = std::str::from_utf8(&after_xref[5..header_end]).ok()?;
    let parts: Vec<&str> = header_line.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let _start: usize = parts[0].parse().ok()?;
    let count: usize = parts[1].parse().ok()?;

    // Find the trailer keyword.
    let trailer_pos = data[xref_pos..].windows(7).position(|w| w == b"trailer")?;
    let abs_trailer = xref_pos + trailer_pos;

    // Check if trailer dict is corrupt (e.g., filled with `<` chars).
    let after_trailer = &data[abs_trailer + 7..];
    let dict_start = after_trailer
        .iter()
        .position(|&b| !b.is_ascii_whitespace())?;
    let dict_data = &after_trailer[dict_start..];

    // A valid trailer dict starts with "<<" followed by something other than "<".
    let is_corrupt = dict_data.starts_with(b"<<<") || !dict_data.starts_with(b"<<");

    if !is_corrupt {
        return None;
    }

    // Scan all xref entries to find /Root by checking each object.
    // Parse xref entries (20 bytes each): "OFFSET GENERATION STATUS\n"
    let entries_start = xref_pos + header_end;
    let mut root_ref = None;
    for i in 1..count {
        let entry_offset = entries_start + i * 20;
        if entry_offset + 20 > data.len() {
            break;
        }
        let entry = &data[entry_offset..entry_offset + 20];
        if entry[17] == b'f' {
            continue; // free entry
        }
        let offset_str = std::str::from_utf8(&entry[..10]).ok()?;
        let obj_offset: usize = offset_str.trim().parse().ok()?;
        if obj_offset + 20 >= data.len() {
            continue;
        }
        // Check if this object contains /Type /Catalog.
        let obj_data = &data[obj_offset..std::cmp::min(obj_offset + 4096, data.len())];
        if obj_data
            .windows(14)
            .any(|w| w == b"/Type /Catalog" || w == b"/Type/Catalog")
        {
            root_ref = Some(i);
            break;
        }
    }

    let root_ref = root_ref?;

    // Rebuild: everything up to trailer, then a valid trailer dict.
    let mut repaired = data[..abs_trailer].to_vec();
    repaired.extend_from_slice(
        format!(
            "trailer\n<< /Size {count} /Root {root_ref} 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n"
        )
        .as_bytes(),
    );
    Some(repaired)
}

/// Fix trailer with invalid /Size value (e.g., "/Size (h)" instead of "/Size 80").
fn try_fix_trailer_size(data: &[u8]) -> Option<Vec<u8>> {
    // Find "/Size" in the trailer.
    let trailer_pos = data.windows(7).rposition(|w| w == b"trailer")?;
    let after_trailer = &data[trailer_pos..];

    // Find /Size followed by non-integer value.
    let size_pos = after_trailer.windows(5).position(|w| w == b"/Size")?;
    let abs_size = trailer_pos + size_pos;
    let after_size = &data[abs_size + 5..];

    // Skip whitespace after /Size.
    let val_start = after_size.iter().position(|b| !b.is_ascii_whitespace())?;

    // If the value starts with a digit, /Size is already valid.
    if after_size[val_start].is_ascii_digit() {
        return None;
    }

    // Compute the correct size from the xref table.
    let xref_pos = find_last_xref_pos(data)?;
    let after_xref = &data[xref_pos..];
    let header_end = after_xref
        .windows(1)
        .skip(5)
        .position(|w| w[0] == b'\n')
        .map(|p| p + 5 + 1)?;
    let header_line = std::str::from_utf8(&after_xref[5..header_end]).ok()?;
    let parts: Vec<&str> = header_line.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let count: usize = parts[1].parse().ok()?;

    // Find the end of the bad value (next / or >>).
    let val_end = after_size[val_start..]
        .iter()
        .position(|&b| b == b'/' || b == b'>')
        .unwrap_or(after_size.len() - val_start);

    let mut repaired = data[..abs_size + 5].to_vec();
    repaired.push(b' ');
    repaired.extend_from_slice(count.to_string().as_bytes());
    repaired.push(b' ');
    repaired.extend_from_slice(&data[abs_size + 5 + val_start + val_end..]);
    Some(repaired)
}

/// Fix startxref with trailing comment (e.g., "23291 %Must be...").
fn try_fix_startxref_comment(data: &[u8]) -> Option<Vec<u8>> {
    let search_start = data.len().saturating_sub(4096);
    let sxr_pos = data[search_start..]
        .windows(9)
        .rposition(|w| w == b"startxref")
        .map(|p| search_start + p)?;

    let after = &data[sxr_pos + 9..];

    // Extract digits.
    let digits_start = after.iter().position(|b| b.is_ascii_digit())?;
    let digits_end = after[digits_start..]
        .iter()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(after.len() - digits_start);

    // Check if there's non-whitespace, non-EOF content after the digits.
    let remainder = &after[digits_start + digits_end..];
    let has_garbage = remainder
        .iter()
        .take_while(|&&b| b != b'%' || remainder.windows(5).any(|w| w != b"%%EOF"))
        .any(|b| !b.is_ascii_whitespace() && *b != b'%');

    // Also check: is there a '%' that is NOT '%%EOF'?
    let after_digits = &after[digits_start + digits_end..];
    let trimmed = after_digits
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .cloned()
        .collect::<Vec<u8>>();
    let needs_fix = !trimmed.is_empty()
        && !trimmed.starts_with(b"%%EOF")
        && (trimmed[0] == b'%' || has_garbage);

    if !needs_fix {
        return None;
    }

    let offset_str = std::str::from_utf8(&after[digits_start..digits_start + digits_end]).ok()?;

    let mut repaired = data[..sxr_pos].to_vec();
    repaired.extend_from_slice(format!("startxref\n{offset_str}\n%%EOF\n").as_bytes());
    Some(repaired)
}

/// Fix xref entries with wrong line endings (19 bytes with LF only).
/// lopdf requires exactly 20-byte entries. Normalizes to CR+LF.
fn try_fix_xref_line_endings(data: &[u8]) -> Option<Vec<u8>> {
    let xref_pos = find_last_xref_pos(data)?;

    // Find subsection header: "xref\nSTART COUNT\n"
    let after = &data[xref_pos + 4..]; // skip "xref"
    let header_start = after.iter().position(|b| !b.is_ascii_whitespace())?;
    let header_end = after[header_start..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|p| header_start + p + 1)?;

    let header_line = std::str::from_utf8(&after[header_start..header_end]).ok()?;
    let parts: Vec<&str> = header_line.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let count: usize = parts[1].parse().ok()?;

    // Find trailer position to delimit entries.
    let trailer_pos = data[xref_pos..]
        .windows(7)
        .position(|w| w == b"trailer")
        .map(|p| xref_pos + p)?;

    let entries_start = xref_pos + 4 + header_end;
    let entries_data = &data[entries_start..trailer_pos];

    // Split entries by newline.
    let entries: Vec<&[u8]> = entries_data
        .split(|&b| b == b'\n')
        .filter(|e| !e.is_empty())
        .collect();

    if entries.len() != count {
        return None;
    }

    // Check if entries need fixing. After split by \n, entries with CR+LF
    // end with \r (19 bytes), entries with LF-only don't (18 bytes).
    // lopdf requires exactly 20-byte entries (content + SP + CR + LF).
    let has_cr = entries.iter().any(|e| e.ends_with(b"\r"));
    if has_cr {
        // Already has CR+LF line endings — check for other issues.
        let all_correct = entries.iter().all(|e| e.ends_with(b"\r") && e.len() == 19);
        if all_correct {
            return None;
        }
    }

    // Rebuild with proper 20-byte entries (content + CR + LF).
    let mut repaired = data[..entries_start].to_vec();
    for entry in &entries {
        let stripped = if entry.ends_with(b"\r") {
            &entry[..entry.len() - 1]
        } else {
            entry
        };
        // Pad to 18 chars if needed (left-pad offset with zeros).
        if stripped.len() == 18 {
            repaired.extend_from_slice(stripped);
        } else {
            // Try to parse and reformat.
            let s = std::str::from_utf8(stripped).ok()?;
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() != 3 {
                return None;
            }
            repaired.extend_from_slice(
                format!("{:0>10} {:0>5} {}", parts[0], parts[1], parts[2]).as_bytes(),
            );
        }
        repaired.extend_from_slice(b"\r\n");
    }
    repaired.extend_from_slice(&data[trailer_pos..]);

    // Fix startxref to point to the xref in the repaired data.
    // The xref position hasn't changed since we only modified entry content.
    Some(repaired)
}

/// Find the byte offset of the last valid standalone "xref" keyword in the data.
/// Skips "xref" substrings inside "startxref" and empty xref tables.
fn find_last_xref_pos(data: &[u8]) -> Option<usize> {
    // Iterate backwards through all "xref" matches.
    let mut search_end = data.len();
    while search_end >= 4 {
        if let Some(pos) = data[..search_end].windows(4).rposition(|w| w == b"xref") {
            // Skip if this is part of "startxref".
            if pos >= 5 && &data[pos - 5..pos] == b"start" {
                search_end = pos;
                continue;
            }
            // Verify it's at the start of a line.
            if pos == 0 || data[pos - 1] == b'\n' || data[pos - 1] == b'\r' {
                // Skip empty xref tables ("xref\ntrailer" with no entries).
                let after = &data[pos + 4..];
                let non_ws = after
                    .iter()
                    .position(|b| !b.is_ascii_whitespace())
                    .map(|p| &after[p..])
                    .unwrap_or(b"");
                if non_ws.starts_with(b"trailer") {
                    search_end = pos;
                    continue;
                }
                return Some(pos);
            }
            search_end = pos;
        } else {
            break;
        }
    }
    None
}

/// Try to fix lopdf documents where get_pages() returns empty because page
/// dictionaries lack a /Type /Page entry.
///
/// Walks the page tree from the catalog and adds /Type /Page to leaf nodes
/// that have a /MediaBox (strong indicator of a page). Returns true if pages
/// were found after the fix.
/// Fix a wrong Root reference in the trailer.
///
/// Some corrupt PDFs have a trailer that points to a non-Catalog object.
/// We check if the Root object has `/Type /Catalog`; if not, scan all objects
/// to find the real catalog and update the trailer.
fn fix_wrong_root(doc: &mut lopdf::Document) {
    let root_id = match doc.trailer.get(b"Root").ok() {
        Some(lopdf::Object::Reference(id)) => *id,
        _ => return,
    };

    // Check if Root actually has /Type /Catalog.
    let is_catalog = match doc.objects.get(&root_id) {
        Some(lopdf::Object::Dictionary(d)) => {
            matches!(d.get(b"Type").ok(), Some(lopdf::Object::Name(n)) if n == b"Catalog")
        }
        _ => false,
    };

    if is_catalog {
        return; // Root is correct.
    }

    // Scan all objects to find the real Catalog.
    for (id, obj) in &doc.objects {
        if let lopdf::Object::Dictionary(d) = obj {
            if matches!(d.get(b"Type").ok(), Some(lopdf::Object::Name(n)) if n == b"Catalog")
                && d.has(b"Pages")
            {
                doc.trailer
                    .set("Root", lopdf::Object::Reference(*id));
                return;
            }
        }
    }
}

fn try_fix_missing_page_types(doc: &mut lopdf::Document) -> bool {
    // Collect page-like object IDs from the page tree.
    let page_ids = collect_page_tree_leaves(doc);

    if page_ids.is_empty() {
        return false;
    }

    let mut fixed = false;
    for page_id in &page_ids {
        if let Ok(dict) = doc.get_dictionary_mut(*page_id) {
            // Only fix if it doesn't already have /Type or has wrong type.
            let needs_fix = !matches!(dict.get_type(), Ok(t) if t == b"Page");

            if needs_fix {
                // Verify it looks like a page (has MediaBox or Contents).
                let has_media_box = dict.get(b"MediaBox").is_ok();
                let has_contents = dict.get(b"Contents").is_ok();
                let has_parent = dict.get(b"Parent").is_ok();

                if has_media_box || has_contents || has_parent {
                    dict.set("Type", lopdf::Object::Name(b"Page".to_vec()));
                    fixed = true;
                }
            }
        }
    }

    if fixed {
        // Re-check if pages are now found.
        !doc.get_pages().is_empty()
    } else {
        false
    }
}

/// Walk the page tree from the catalog's /Pages entry and collect leaf node IDs.
/// Unlike lopdf's get_pages(), this doesn't require /Type to be present.
fn collect_page_tree_leaves(doc: &lopdf::Document) -> Vec<lopdf::ObjectId> {
    let mut leaves = Vec::new();

    let pages_id = match doc
        .catalog()
        .and_then(|cat| cat.get(b"Pages"))
        .and_then(lopdf::Object::as_reference)
    {
        Ok(id) => id,
        Err(_) => return leaves,
    };

    let mut stack = vec![pages_id];
    let mut visited = std::collections::HashSet::new();
    let limit = doc.objects.len().min(10_000);

    while let Some(id) = stack.pop() {
        if !visited.insert(id) || visited.len() > limit {
            continue;
        }

        let dict = match doc.get_dictionary(id) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Check if this node has /Kids — if so, it's a Pages node.
        if let Ok(kids) = dict.get(b"Kids").and_then(lopdf::Object::as_array) {
            for kid in kids {
                if let Ok(kid_id) = kid.as_reference() {
                    stack.push(kid_id);
                }
            }
        } else {
            // No /Kids — this is a leaf (page) node.
            leaves.push(id);
        }
    }

    leaves
}
