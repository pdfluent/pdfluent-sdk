//! Edge-Case Report — identify rare and complex XFA patterns in the corpus.
//!
//! Scans each PDF for edge cases that enterprise customers encounter:
//! deep nesting, 500+ fields, Unicode/CJK text, hybrid forms,
//! complex FormCalc scripts, dynamic pagination, etc.
//!
//! Usage:
//!   cargo run --bin edge-case-report -- --corpus corpus/

use clap::Parser;
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "edge-case-report", about = "Analyze XFA corpus for edge cases")]
struct Cli {
    /// Path to the PDF corpus directory.
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,

    /// Output path for the JSON report.
    #[arg(long, default_value = "reports/edge-cases.json")]
    output: PathBuf,
}

/// Flags for detected edge cases.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct EdgeCaseFlags {
    /// Subform nesting depth >= 10.
    pub deep_nesting: bool,
    pub max_nesting_depth: usize,

    /// 500+ fields in a single form.
    pub high_field_count: bool,
    pub field_count: usize,

    /// Unicode characters outside ASCII (CJK, Arabic, etc.).
    pub has_unicode: bool,
    pub has_cjk: bool,

    /// Mixed AcroForm + XFA hybrid form.
    pub is_hybrid: bool,

    /// Complex FormCalc scripts (multi-line, function definitions).
    pub has_complex_formcalc: bool,
    pub formcalc_script_count: usize,

    /// JavaScript event scripts.
    pub has_javascript: bool,
    pub javascript_script_count: usize,

    /// Dynamic pagination (layout="tb" with overflow).
    pub has_dynamic_pagination: bool,

    /// Repeating sections (occur min/max).
    pub has_repeating: bool,
    pub repeating_section_count: usize,

    /// Tables (layout="table").
    pub has_tables: bool,
    pub table_count: usize,

    /// Conditional visibility (relevant expressions).
    pub has_relevant: bool,

    /// Custom fonts referenced.
    pub has_custom_fonts: bool,

    /// Barcode fields.
    pub has_barcodes: bool,

    /// Image fields.
    pub has_images: bool,

    /// Signatures.
    pub has_signatures: bool,

    /// Multiple page areas.
    pub has_multiple_page_areas: bool,
    pub page_area_count: usize,
}

/// Per-form edge case analysis.
#[derive(Debug, Serialize, Deserialize)]
pub struct FormEdgeCases {
    pub filename: String,
    pub xfa_type: String,
    pub edge_case_count: usize,
    pub flags: EdgeCaseFlags,
}

/// Edge-case corpus report.
#[derive(Debug, Serialize, Deserialize)]
pub struct EdgeCaseReport {
    pub generated_at: String,
    pub total_forms: usize,
    pub forms: Vec<FormEdgeCases>,
    pub summary: EdgeCaseSummary,
}

/// Summary across the corpus.
#[derive(Debug, Serialize, Deserialize)]
pub struct EdgeCaseSummary {
    pub forms_with_deep_nesting: usize,
    pub forms_with_500_plus_fields: usize,
    pub forms_with_unicode: usize,
    pub forms_with_cjk: usize,
    pub forms_with_hybrid: usize,
    pub forms_with_complex_formcalc: usize,
    pub forms_with_javascript: usize,
    pub forms_with_dynamic_pagination: usize,
    pub forms_with_repeating: usize,
    pub forms_with_tables: usize,
    pub forms_with_relevant: usize,
    pub forms_with_barcodes: usize,
    pub forms_with_images: usize,
    pub forms_with_signatures: usize,
    pub forms_with_multiple_page_areas: usize,
    pub edge_case_distribution: BTreeMap<String, usize>,
    pub top_complex_forms: Vec<String>,
}

fn main() {
    let cli = Cli::parse();

    println!("XFA Edge-Case Analysis");
    println!("======================");
    println!("Corpus: {}", cli.corpus.display());
    println!();

    let forms = analyze_corpus(&cli.corpus);
    let summary = build_summary(&forms);

    let report = EdgeCaseReport {
        generated_at: chrono_free_date(),
        total_forms: forms.len(),
        forms,
        summary,
    };

    print_summary(&report);

    if let Some(parent) = cli.output.parent() {
        fs::create_dir_all(parent).ok();
    }
    let json = serde_json::to_string_pretty(&report).expect("JSON serialization failed");
    fs::write(&cli.output, json).expect("Failed to write report");
    println!("\nReport: {}", cli.output.display());
}

fn analyze_corpus(corpus_dir: &Path) -> Vec<FormEdgeCases> {
    let mut entries: Vec<PathBuf> = match fs::read_dir(corpus_dir) {
        Ok(dir) => dir
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")))
            .collect(),
        Err(e) => {
            eprintln!("Error: {e}");
            return Vec::new();
        }
    };
    entries.sort();

    println!("Analyzing {} PDFs...", entries.len());

    let mut results = Vec::with_capacity(entries.len());
    for (i, path) in entries.iter().enumerate() {
        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        print!("  [{}/{}] {}", i + 1, entries.len(), filename);

        match analyze_single_pdf(path) {
            Some(result) => {
                let tags: Vec<&str> = collect_tags(&result.flags);
                if tags.is_empty() {
                    println!(" — no edge cases");
                } else {
                    println!(" — {}", tags.join(", "));
                }
                results.push(result);
            }
            None => {
                println!(" — SKIP (not XFA)");
            }
        }
    }

    results
}

fn analyze_single_pdf(path: &Path) -> Option<FormEdgeCases> {
    let bytes = fs::read(path).ok()?;
    let reader = PdfReader::from_bytes(&bytes).ok()?;
    let packets = reader.extract_xfa().ok()?;

    if packets.packets.is_empty() {
        return None;
    }

    let template_xml = packets.template().unwrap_or("");
    let full_xml = packets.full_xml.as_deref().unwrap_or("");

    let xfa_type = if template_xml.contains("layout=\"tb\"")
        || template_xml.contains("layout=\"lr-tb\"")
        || template_xml.contains("layout=\"rl-tb\"")
    {
        "dynamic"
    } else {
        "static"
    };

    let flags = detect_edge_cases(template_xml, full_xml, &reader);
    let edge_case_count = count_edge_cases(&flags);

    Some(FormEdgeCases {
        filename: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
        xfa_type: xfa_type.to_string(),
        edge_case_count,
        flags,
    })
}

fn detect_edge_cases(template_xml: &str, full_xml: &str, reader: &PdfReader) -> EdgeCaseFlags {
    let mut flags = EdgeCaseFlags::default();

    // Deep nesting
    flags.max_nesting_depth = measure_nesting_depth(template_xml);
    flags.deep_nesting = flags.max_nesting_depth >= 10;

    // Field count
    flags.field_count = count_tag_occurrences(template_xml, "<field ");
    flags.high_field_count = flags.field_count >= 500;

    // Unicode / CJK
    for ch in full_xml.chars() {
        if ch as u32 > 127 {
            flags.has_unicode = true;
            if is_cjk(ch) {
                flags.has_cjk = true;
                break;
            }
        }
    }

    // Hybrid AcroForm + XFA
    flags.is_hybrid = detect_hybrid(reader);

    // FormCalc scripts
    let formcalc_count = count_formcalc_scripts(template_xml);
    flags.formcalc_script_count = formcalc_count;
    flags.has_complex_formcalc = has_complex_formcalc(template_xml);

    // JavaScript scripts
    flags.javascript_script_count = count_javascript_scripts(template_xml);
    flags.has_javascript = flags.javascript_script_count > 0;

    // Dynamic pagination
    flags.has_dynamic_pagination = template_xml.contains("layout=\"tb\"")
        && (template_xml.contains("overflow") || template_xml.contains("<break"));

    // Repeating sections
    flags.repeating_section_count = count_repeating_sections(template_xml);
    flags.has_repeating = flags.repeating_section_count > 0;

    // Tables
    flags.table_count = count_tag_occurrences(template_xml, "layout=\"table\"");
    flags.has_tables = flags.table_count > 0;

    // Conditional visibility (relevant)
    flags.has_relevant = template_xml.contains("relevant=\"");

    // Custom fonts
    flags.has_custom_fonts = template_xml.contains("<font typeface=\"")
        && !only_standard_fonts(template_xml);

    // Barcodes
    flags.has_barcodes = template_xml.contains("<barcode");

    // Images
    flags.has_images = template_xml.contains("<image");

    // Signatures
    flags.has_signatures = template_xml.contains("<signature")
        || template_xml.contains("type=\"signature\"");

    // Multiple page areas
    flags.page_area_count = count_tag_occurrences(template_xml, "<pageArea");
    flags.has_multiple_page_areas = flags.page_area_count > 1;

    flags
}

fn measure_nesting_depth(xml: &str) -> usize {
    let mut max_depth: usize = 0;
    let mut depth: usize = 0;

    let mut pos = 0;
    while pos < xml.len() {
        if let Some(lt) = xml[pos..].find('<') {
            let abs = pos + lt;
            if xml[abs..].starts_with("</") {
                depth = depth.saturating_sub(1);
            } else if !xml[abs..].starts_with("<?") && !xml[abs..].starts_with("<!--") {
                // Check for self-closing
                if let Some(gt) = xml[abs..].find('>') {
                    if xml.as_bytes()[abs + gt - 1] != b'/' {
                        depth += 1;
                        if depth > max_depth {
                            max_depth = depth;
                        }
                    }
                }
            }
            pos = abs + 1;
        } else {
            break;
        }
    }

    max_depth
}

fn count_tag_occurrences(xml: &str, pattern: &str) -> usize {
    xml.matches(pattern).count()
}

fn is_cjk(ch: char) -> bool {
    let code = ch as u32;
    // CJK Unified Ideographs, Katakana, Hiragana, Hangul
    (0x4E00..=0x9FFF).contains(&code)
        || (0x3040..=0x309F).contains(&code)
        || (0x30A0..=0x30FF).contains(&code)
        || (0xAC00..=0xD7AF).contains(&code)
        || (0x3400..=0x4DBF).contains(&code)
}

fn detect_hybrid(reader: &PdfReader) -> bool {
    // Check if AcroForm has both XFA and Fields entries
    let doc = reader.document();
    let catalog_ref = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .ok();

    if let Some(cat_ref) = catalog_ref {
        if let Ok(catalog) = doc.get_object(cat_ref).and_then(|o| o.as_dict()) {
            if let Ok(acroform_ref) = catalog.get(b"AcroForm").and_then(|o| o.as_reference()) {
                if let Ok(acroform) = doc.get_object(acroform_ref).and_then(|o| o.as_dict()) {
                    let has_xfa = acroform.has(b"XFA");
                    let has_fields = acroform.has(b"Fields");
                    return has_xfa && has_fields;
                }
            }
        }
    }
    false
}

fn count_formcalc_scripts(xml: &str) -> usize {
    // Count <calculate> and <script contentType="application/x-formcalc">
    let calc_count = xml.matches("<calculate").count();
    let formcalc_count = xml.matches("application/x-formcalc").count();
    calc_count + formcalc_count
}

fn has_complex_formcalc(xml: &str) -> bool {
    // Complex: multi-line scripts, function definitions, loops
    xml.contains("endfunc") || xml.contains("endfor") || xml.contains("endwhile")
}

fn count_javascript_scripts(xml: &str) -> usize {
    xml.matches("application/x-javascript").count()
}

fn count_repeating_sections(xml: &str) -> usize {
    let mut count = 0;
    let mut pos = 0;
    while let Some(idx) = xml[pos..].find("occur") {
        let abs = pos + idx;
        let snippet = &xml[abs..xml.len().min(abs + 100)];
        if snippet.contains("max=\"-1\"") || snippet.contains("max=\"") {
            // Check that max is > 1 or -1 (unlimited)
            if let Some(max_start) = snippet.find("max=\"") {
                let val_start = max_start + 5;
                if let Some(val_end) = snippet[val_start..].find('"') {
                    let max_val = &snippet[val_start..val_start + val_end];
                    if max_val == "-1" || max_val.parse::<i32>().is_ok_and(|v| v > 1) {
                        count += 1;
                    }
                }
            }
        }
        pos = abs + 5;
    }
    count
}

fn only_standard_fonts(xml: &str) -> bool {
    let standard_fonts = [
        "Courier", "Helvetica", "Times", "Symbol", "ZapfDingbats",
        "Arial", "Myriad Pro", "MyriadPro",
    ];

    let mut pos = 0;
    while let Some(idx) = xml[pos..].find("typeface=\"") {
        let abs = pos + idx + 10;
        if let Some(end) = xml[abs..].find('"') {
            let font_name = &xml[abs..abs + end];
            if !standard_fonts.iter().any(|sf| font_name.contains(sf)) {
                return false;
            }
        }
        pos = abs;
    }
    true
}

fn count_edge_cases(flags: &EdgeCaseFlags) -> usize {
    let mut count = 0;
    if flags.deep_nesting { count += 1; }
    if flags.high_field_count { count += 1; }
    if flags.has_unicode { count += 1; }
    if flags.has_cjk { count += 1; }
    if flags.is_hybrid { count += 1; }
    if flags.has_complex_formcalc { count += 1; }
    if flags.has_javascript { count += 1; }
    if flags.has_dynamic_pagination { count += 1; }
    if flags.has_repeating { count += 1; }
    if flags.has_tables { count += 1; }
    if flags.has_relevant { count += 1; }
    if flags.has_barcodes { count += 1; }
    if flags.has_images { count += 1; }
    if flags.has_signatures { count += 1; }
    if flags.has_multiple_page_areas { count += 1; }
    count
}

fn collect_tags(flags: &EdgeCaseFlags) -> Vec<&str> {
    let mut tags = Vec::new();
    if flags.deep_nesting { tags.push("deep-nesting"); }
    if flags.high_field_count { tags.push("500+fields"); }
    if flags.has_cjk { tags.push("CJK"); }
    else if flags.has_unicode { tags.push("unicode"); }
    if flags.is_hybrid { tags.push("hybrid"); }
    if flags.has_complex_formcalc { tags.push("complex-formcalc"); }
    if flags.has_javascript { tags.push("javascript"); }
    if flags.has_dynamic_pagination { tags.push("dynamic-pagination"); }
    if flags.has_repeating { tags.push("repeating"); }
    if flags.has_tables { tags.push("tables"); }
    if flags.has_barcodes { tags.push("barcodes"); }
    if flags.has_images { tags.push("images"); }
    if flags.has_signatures { tags.push("signatures"); }
    if flags.has_multiple_page_areas { tags.push("multi-pagearea"); }
    tags
}

fn build_summary(forms: &[FormEdgeCases]) -> EdgeCaseSummary {
    let mut dist: BTreeMap<String, usize> = BTreeMap::new();

    let mut summary = EdgeCaseSummary {
        forms_with_deep_nesting: 0,
        forms_with_500_plus_fields: 0,
        forms_with_unicode: 0,
        forms_with_cjk: 0,
        forms_with_hybrid: 0,
        forms_with_complex_formcalc: 0,
        forms_with_javascript: 0,
        forms_with_dynamic_pagination: 0,
        forms_with_repeating: 0,
        forms_with_tables: 0,
        forms_with_relevant: 0,
        forms_with_barcodes: 0,
        forms_with_images: 0,
        forms_with_signatures: 0,
        forms_with_multiple_page_areas: 0,
        edge_case_distribution: BTreeMap::new(),
        top_complex_forms: Vec::new(),
    };

    for f in forms {
        let flags = &f.flags;
        if flags.deep_nesting { summary.forms_with_deep_nesting += 1; *dist.entry("deep-nesting".into()).or_default() += 1; }
        if flags.high_field_count { summary.forms_with_500_plus_fields += 1; *dist.entry("500+fields".into()).or_default() += 1; }
        if flags.has_unicode { summary.forms_with_unicode += 1; *dist.entry("unicode".into()).or_default() += 1; }
        if flags.has_cjk { summary.forms_with_cjk += 1; *dist.entry("cjk".into()).or_default() += 1; }
        if flags.is_hybrid { summary.forms_with_hybrid += 1; *dist.entry("hybrid".into()).or_default() += 1; }
        if flags.has_complex_formcalc { summary.forms_with_complex_formcalc += 1; *dist.entry("complex-formcalc".into()).or_default() += 1; }
        if flags.has_javascript { summary.forms_with_javascript += 1; *dist.entry("javascript".into()).or_default() += 1; }
        if flags.has_dynamic_pagination { summary.forms_with_dynamic_pagination += 1; *dist.entry("dynamic-pagination".into()).or_default() += 1; }
        if flags.has_repeating { summary.forms_with_repeating += 1; *dist.entry("repeating".into()).or_default() += 1; }
        if flags.has_tables { summary.forms_with_tables += 1; *dist.entry("tables".into()).or_default() += 1; }
        if flags.has_relevant { summary.forms_with_relevant += 1; *dist.entry("relevant".into()).or_default() += 1; }
        if flags.has_barcodes { summary.forms_with_barcodes += 1; *dist.entry("barcodes".into()).or_default() += 1; }
        if flags.has_images { summary.forms_with_images += 1; *dist.entry("images".into()).or_default() += 1; }
        if flags.has_signatures { summary.forms_with_signatures += 1; *dist.entry("signatures".into()).or_default() += 1; }
        if flags.has_multiple_page_areas { summary.forms_with_multiple_page_areas += 1; *dist.entry("multi-pagearea".into()).or_default() += 1; }
    }

    summary.edge_case_distribution = dist;

    // Top complex forms (most edge cases)
    let mut sorted: Vec<&FormEdgeCases> = forms.iter().collect();
    sorted.sort_by(|a, b| b.edge_case_count.cmp(&a.edge_case_count));
    summary.top_complex_forms = sorted
        .iter()
        .take(10)
        .map(|f| format!("{} ({})", f.filename, f.edge_case_count))
        .collect();

    summary
}

fn print_summary(report: &EdgeCaseReport) {
    let s = &report.summary;
    println!();
    println!("=== Edge-Case Summary ({} forms) ===", report.total_forms);
    println!("  Deep nesting (10+):    {}", s.forms_with_deep_nesting);
    println!("  500+ fields:           {}", s.forms_with_500_plus_fields);
    println!("  Unicode:               {}", s.forms_with_unicode);
    println!("  CJK:                   {}", s.forms_with_cjk);
    println!("  Hybrid AcroForm+XFA:   {}", s.forms_with_hybrid);
    println!("  Complex FormCalc:      {}", s.forms_with_complex_formcalc);
    println!("  JavaScript:            {}", s.forms_with_javascript);
    println!("  Dynamic pagination:    {}", s.forms_with_dynamic_pagination);
    println!("  Repeating sections:    {}", s.forms_with_repeating);
    println!("  Tables:                {}", s.forms_with_tables);
    println!("  Conditional (relevant):{}", s.forms_with_relevant);
    println!("  Barcodes:              {}", s.forms_with_barcodes);
    println!("  Images:                {}", s.forms_with_images);
    println!("  Signatures:            {}", s.forms_with_signatures);
    println!("  Multiple page areas:   {}", s.forms_with_multiple_page_areas);

    println!();
    println!("  Top complex forms:");
    for f in &s.top_complex_forms {
        println!("    {f}");
    }
}

fn chrono_free_date() -> String {
    std::process::Command::new("date")
        .args(["+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
