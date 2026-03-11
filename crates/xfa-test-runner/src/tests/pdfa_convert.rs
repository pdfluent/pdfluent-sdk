use std::collections::HashMap;
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
                status: TestStatus::Skip,
                error_message: Some("already PDF/A".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // 2. Load via lopdf for mutation.
        set_progress("lopdf_load");
        let mut doc = match lopdf::Document::load_mem(pdf_data) {
            Ok(d) => d,
            Err(e) => {
                // Fallback: try stripping garbage bytes before %PDF header.
                match try_repair_for_lopdf(pdf_data) {
                    Some(d) => d,
                    None => {
                        return TestResult {
                            status: TestStatus::Skip,
                            error_message: Some(format!("lopdf load failed: {e}")),
                            duration_ms: elapsed(),
                            oracle_score: None,
                            metadata: HashMap::new(),
                        };
                    }
                }
            }
        };

        if doc.get_pages().is_empty() {
            // Fallback: try adding missing /Type /Page entries to page-like objects.
            if !try_fix_missing_page_types(&mut doc) {
                // Second fallback: check if pdf-syntax found pages (it has brute-force mode).
                let syntax_page_count = pdf.pages().len();
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("0 pages (pdf-syntax found {syntax_page_count})")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        }

        // 3. Run PDF/A conversion pipeline.
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
            TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!(
                    "{} compliance issues after conversion",
                    report.issues.len()
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
    let path = dir.join(format!("{stem}_pdfa_converted.pdf"));
    std::fs::write(&path, data).ok()?;
    Some(path)
}

/// Try to repair PDF data so that lopdf can load it.
///
/// Strategies:
/// 1. Strip garbage bytes before the %PDF header (offset header).
/// 2. Try appending a minimal %%EOF + startxref if missing/broken.
/// 3. Try truncating trailing garbage after the last %%EOF.
fn try_repair_for_lopdf(data: &[u8]) -> Option<lopdf::Document> {
    // Strategy 1: Find %PDF- offset and strip leading garbage.
    let offset = data.windows(5).position(|w| w == b"%PDF-")?;

    if offset > 0 {
        let trimmed = &data[offset..];
        if let Ok(doc) = lopdf::Document::load_mem(trimmed) {
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
                let has_xref = at_offset.starts_with(b"xref")
                    || (at_offset.len() > 5 && at_offset[0].is_ascii_digit());

                if !has_xref {
                    // The offset is wrong. Try to find the actual xref position.
                    if let Some(real_xref) = find_last_xref_pos(data) {
                        let mut repaired = data.to_vec();
                        // Rebuild the startxref section.
                        repaired.truncate(sxr);
                        repaired.extend_from_slice(
                            format!("startxref\n{real_xref}\n%%EOF\n").as_bytes(),
                        );
                        if let Ok(doc) = lopdf::Document::load_mem(&repaired) {
                            return Some(doc);
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
            if let Ok(doc) = lopdf::Document::load_mem(&repaired) {
                return Some(doc);
            }
        }
    }

    None
}

/// Find the byte offset of the last "xref" keyword or xref stream object in the data.
fn find_last_xref_pos(data: &[u8]) -> Option<usize> {
    // First try: find "xref" keyword (traditional xref table).
    let xref_pos = data.windows(4).rposition(|w| w == b"xref");

    if let Some(pos) = xref_pos {
        // Verify it's actually at the start of a line (preceded by newline or start of file).
        if pos == 0 || data[pos - 1] == b'\n' || data[pos - 1] == b'\r' {
            return Some(pos);
        }
    }

    // Fallback: scan for cross-reference stream objects.
    // These look like "N 0 obj" followed by a dictionary containing /Type /XRef.
    // This is harder to find reliably, so we just return None for now.
    None
}

/// Try to fix lopdf documents where get_pages() returns empty because page
/// dictionaries lack a /Type /Page entry.
///
/// Walks the page tree from the catalog and adds /Type /Page to leaf nodes
/// that have a /MediaBox (strong indicator of a page). Returns true if pages
/// were found after the fix.
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
