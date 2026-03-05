//! Edge Case Analyzer — identify XFA forms with challenging characteristics.
//!
//! Scans the corpus for edge cases that may cause issues:
//! - Deep nesting (10+ levels)
//! - Large forms (500+ fields)
//! - Unicode/CJK text
//! - Mixed AcroForm+XFA hybrid forms
//! - Complex FormCalc scripts
//! - Dynamic subforms (repeatable sections)

use clap::Parser;
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "edge-case-analyzer",
    about = "Identify XFA forms with challenging edge-case characteristics"
)]
struct Cli {
    /// Path to corpus directory.
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,

    /// Output file for JSON report.
    #[arg(long, default_value = "reports/edge-cases/report.json")]
    output: PathBuf,

    /// Also generate HTML report.
    #[arg(long)]
    html: bool,
}

#[derive(Debug, Serialize)]
struct EdgeCaseReport {
    total_forms: usize,
    forms: Vec<FormAnalysis>,
    summary: EdgeCaseSummary,
}

#[derive(Debug, Serialize)]
struct EdgeCaseSummary {
    deep_nesting: Vec<String>,
    large_forms: Vec<String>,
    unicode_cjk: Vec<String>,
    hybrid_forms: Vec<String>,
    formcalc_scripts: Vec<String>,
    dynamic_subforms: Vec<String>,
    multi_page: Vec<String>,
}

#[derive(Debug, Serialize)]
struct FormAnalysis {
    filename: String,
    field_count: usize,
    subform_count: usize,
    max_nesting_depth: usize,
    has_formcalc: bool,
    formcalc_count: usize,
    has_dynamic_subforms: bool,
    dynamic_subform_count: usize,
    has_unicode_cjk: bool,
    has_acroform: bool,
    has_xfa: bool,
    page_count: usize,
    edge_case_tags: Vec<String>,
}

fn main() {
    let cli = Cli::parse();

    if !cli.corpus.exists() {
        eprintln!(
            "ERROR: Corpus directory not found: {}",
            cli.corpus.display()
        );
        std::process::exit(1);
    }

    let mut pdfs: Vec<PathBuf> = std::fs::read_dir(&cli.corpus)
        .expect("Failed to read corpus directory")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "pdf"))
        .collect();
    pdfs.sort();

    println!("Edge Case Analyzer");
    println!("===================");
    println!("Corpus: {} ({} PDFs)", cli.corpus.display(), pdfs.len());
    println!();

    let mut forms = Vec::new();

    for pdf_path in &pdfs {
        let filename = pdf_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let analysis = analyze_form(pdf_path, &filename);

        if !analysis.edge_case_tags.is_empty() {
            println!(
                "  {:<40} [{}]",
                filename,
                analysis.edge_case_tags.join(", ")
            );
        }

        forms.push(analysis);
    }

    // Build summary
    let summary = EdgeCaseSummary {
        deep_nesting: forms
            .iter()
            .filter(|f| f.max_nesting_depth >= 10)
            .map(|f| format!("{} (depth: {})", f.filename, f.max_nesting_depth))
            .collect(),
        large_forms: forms
            .iter()
            .filter(|f| f.field_count >= 500)
            .map(|f| format!("{} ({} fields)", f.filename, f.field_count))
            .collect(),
        unicode_cjk: forms
            .iter()
            .filter(|f| f.has_unicode_cjk)
            .map(|f| f.filename.clone())
            .collect(),
        hybrid_forms: forms
            .iter()
            .filter(|f| f.has_acroform && f.has_xfa)
            .map(|f| f.filename.clone())
            .collect(),
        formcalc_scripts: forms
            .iter()
            .filter(|f| f.formcalc_count >= 5)
            .map(|f| format!("{} ({} scripts)", f.filename, f.formcalc_count))
            .collect(),
        dynamic_subforms: forms
            .iter()
            .filter(|f| f.has_dynamic_subforms)
            .map(|f| format!("{} ({} dynamic)", f.filename, f.dynamic_subform_count))
            .collect(),
        multi_page: forms
            .iter()
            .filter(|f| f.page_count >= 5)
            .map(|f| format!("{} ({} pages)", f.filename, f.page_count))
            .collect(),
    };

    let total_forms = forms.len();
    let report = EdgeCaseReport {
        total_forms,
        forms,
        summary,
    };

    // Print summary
    println!();
    println!("Summary");
    println!("-------");
    println!(
        "Deep nesting (10+ levels):  {} forms",
        report.summary.deep_nesting.len()
    );
    println!(
        "Large forms (500+ fields):  {} forms",
        report.summary.large_forms.len()
    );
    println!(
        "Unicode/CJK content:        {} forms",
        report.summary.unicode_cjk.len()
    );
    println!(
        "Hybrid AcroForm+XFA:        {} forms",
        report.summary.hybrid_forms.len()
    );
    println!(
        "Complex FormCalc (5+):      {} forms",
        report.summary.formcalc_scripts.len()
    );
    println!(
        "Dynamic subforms:           {} forms",
        report.summary.dynamic_subforms.len()
    );
    println!(
        "Multi-page (5+):            {} forms",
        report.summary.multi_page.len()
    );

    // Write report
    if let Some(parent) = cli.output.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let json = serde_json::to_string_pretty(&report).expect("JSON serialization failed");
    std::fs::write(&cli.output, &json).expect("Failed to write report");
    println!();
    println!("JSON report: {}", cli.output.display());

    if cli.html {
        let html_path = cli.output.with_extension("html");
        let html = generate_html(&report);
        std::fs::write(&html_path, html).expect("Failed to write HTML report");
        println!("HTML report: {}", html_path.display());
    }
}

fn analyze_form(path: &Path, filename: &str) -> FormAnalysis {
    let reader = match PdfReader::from_file(path) {
        Ok(r) => r,
        Err(_) => {
            return FormAnalysis {
                filename: filename.to_string(),
                field_count: 0,
                subform_count: 0,
                max_nesting_depth: 0,
                has_formcalc: false,
                formcalc_count: 0,
                has_dynamic_subforms: false,
                dynamic_subform_count: 0,
                has_unicode_cjk: false,
                has_acroform: false,
                has_xfa: false,
                page_count: 0,
                edge_case_tags: vec!["load_failed".to_string()],
            };
        }
    };

    let page_count = reader.page_count();
    let has_acroform = check_has_acroform(path);

    let xfa = match reader.extract_xfa() {
        Ok(packets) => packets,
        Err(_) => {
            return FormAnalysis {
                filename: filename.to_string(),
                field_count: 0,
                subform_count: 0,
                max_nesting_depth: 0,
                has_formcalc: false,
                formcalc_count: 0,
                has_dynamic_subforms: false,
                dynamic_subform_count: 0,
                has_unicode_cjk: false,
                has_acroform,
                has_xfa: false,
                page_count,
                edge_case_tags: vec![],
            };
        }
    };

    let has_xfa = !xfa.packets.is_empty() || xfa.full_xml.is_some();
    let mut field_count = 0;
    let mut subform_count = 0;
    let mut max_depth = 0;
    let mut formcalc_count = 0;
    let mut dynamic_subform_count = 0;
    let mut has_unicode_cjk = false;

    if let Some(template_xml) = xfa.template() {
        if let Ok(doc) = roxmltree::Document::parse(template_xml) {
            for node in doc.descendants() {
                if !node.is_element() {
                    continue;
                }

                match node.tag_name().name() {
                    "field" => field_count += 1,
                    "subform" => {
                        subform_count += 1;
                        // Check for repeatable (dynamic) subforms
                        if let Some(occur) = node
                            .children()
                            .find(|c| c.is_element() && c.tag_name().name() == "occur")
                        {
                            let max = occur
                                .attribute("max")
                                .and_then(|v| v.parse::<i32>().ok())
                                .unwrap_or(1);
                            if max > 1 || max == -1 {
                                dynamic_subform_count += 1;
                            }
                        }
                    }
                    "calculate" | "validate" | "event" => {
                        // Check for FormCalc scripts
                        for child in node.children() {
                            if child.is_element() && child.tag_name().name() == "script" {
                                formcalc_count += 1;
                            }
                        }
                    }
                    _ => {}
                }

                // Check nesting depth
                let depth = ancestors_count(&node);
                if depth > max_depth {
                    max_depth = depth;
                }
            }

            // Check for Unicode/CJK in template text
            has_unicode_cjk = template_xml.chars().any(is_cjk_char);
        }
    }

    // Also check datasets for Unicode/CJK
    if !has_unicode_cjk {
        if let Some(datasets_xml) = xfa.datasets() {
            has_unicode_cjk = datasets_xml.chars().any(is_cjk_char);
        }
    }

    let mut tags = Vec::new();
    if max_depth >= 10 {
        tags.push("deep_nesting".to_string());
    }
    if field_count >= 500 {
        tags.push("large_form".to_string());
    }
    if has_unicode_cjk {
        tags.push("unicode_cjk".to_string());
    }
    if has_acroform && has_xfa {
        tags.push("hybrid".to_string());
    }
    if formcalc_count >= 5 {
        tags.push("complex_formcalc".to_string());
    }
    if dynamic_subform_count > 0 {
        tags.push("dynamic_subforms".to_string());
    }
    if page_count >= 5 {
        tags.push("multi_page".to_string());
    }

    FormAnalysis {
        filename: filename.to_string(),
        field_count,
        subform_count,
        max_nesting_depth: max_depth,
        has_formcalc: formcalc_count > 0,
        formcalc_count,
        has_dynamic_subforms: dynamic_subform_count > 0,
        dynamic_subform_count,
        has_unicode_cjk,
        has_acroform,
        has_xfa,
        page_count,
        edge_case_tags: tags,
    }
}

fn ancestors_count(node: &roxmltree::Node) -> usize {
    let mut count = 0;
    let mut current = node.parent();
    while let Some(parent) = current {
        count += 1;
        current = parent.parent();
    }
    count
}

fn is_cjk_char(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF     // CJK Unified Ideographs
        | 0x3400..=0x4DBF   // CJK Extension A
        | 0x3040..=0x309F   // Hiragana
        | 0x30A0..=0x30FF   // Katakana
        | 0xAC00..=0xD7AF   // Hangul Syllables
        | 0x0600..=0x06FF   // Arabic
        | 0x0900..=0x097F   // Devanagari
    )
}

fn check_has_acroform(path: &Path) -> bool {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let doc = match lopdf::Document::load_mem(&bytes) {
        Ok(d) => d,
        Err(_) => return false,
    };

    doc.trailer
        .get_deref(b"Root", &doc)
        .and_then(|o| o.as_dict())
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .is_some()
}

fn generate_html(report: &EdgeCaseReport) -> String {
    let mut html = String::from(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Edge Case Analysis Report</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; max-width: 1000px; margin: 2rem auto; padding: 0 1rem; color: #333; }
    h1 { border-bottom: 2px solid #dc2626; padding-bottom: 0.5rem; }
    .summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: 1rem; margin: 1.5rem 0; }
    .card { background: #f8fafc; border: 1px solid #e2e8f0; border-radius: 8px; padding: 1rem; text-align: center; }
    .card .value { font-size: 2rem; font-weight: 700; color: #dc2626; }
    .card .label { font-size: 0.8rem; color: #64748b; margin-top: 0.25rem; }
    table { width: 100%; border-collapse: collapse; margin-top: 1rem; font-size: 0.8rem; }
    th, td { padding: 0.4rem 0.5rem; text-align: left; border-bottom: 1px solid #e2e8f0; }
    th { background: #f1f5f9; font-weight: 600; }
    .tag { display: inline-block; padding: 1px 6px; border-radius: 3px; font-size: 0.7rem; font-weight: 600; margin: 1px; background: #fef2f2; color: #991b1b; }
    details { margin: 0.5rem 0; }
    summary { cursor: pointer; font-weight: 500; }
    ul { margin: 0.5rem 0; padding-left: 1.5rem; }
    footer { margin-top: 2rem; padding-top: 1rem; border-top: 1px solid #e2e8f0; font-size: 0.875rem; color: #94a3b8; }
  </style>
</head>
<body>
  <h1>Edge Case Analysis Report</h1>
"#,
    );

    html.push_str(&format!(
        r#"  <div class="summary">
    <div class="card"><div class="value">{}</div><div class="label">Deep Nesting</div></div>
    <div class="card"><div class="value">{}</div><div class="label">Large Forms</div></div>
    <div class="card"><div class="value">{}</div><div class="label">Unicode/CJK</div></div>
    <div class="card"><div class="value">{}</div><div class="label">Hybrid Forms</div></div>
    <div class="card"><div class="value">{}</div><div class="label">Complex FormCalc</div></div>
    <div class="card"><div class="value">{}</div><div class="label">Dynamic Subforms</div></div>
    <div class="card"><div class="value">{}</div><div class="label">Multi-Page</div></div>
  </div>
"#,
        report.summary.deep_nesting.len(),
        report.summary.large_forms.len(),
        report.summary.unicode_cjk.len(),
        report.summary.hybrid_forms.len(),
        report.summary.formcalc_scripts.len(),
        report.summary.dynamic_subforms.len(),
        report.summary.multi_page.len(),
    ));

    // Detail sections
    let sections = [
        ("Deep Nesting (10+ levels)", &report.summary.deep_nesting),
        ("Large Forms (500+ fields)", &report.summary.large_forms),
        ("Unicode/CJK Content", &report.summary.unicode_cjk),
        ("Hybrid AcroForm+XFA", &report.summary.hybrid_forms),
        (
            "Complex FormCalc (5+ scripts)",
            &report.summary.formcalc_scripts,
        ),
        ("Dynamic Subforms", &report.summary.dynamic_subforms),
        ("Multi-Page (5+ pages)", &report.summary.multi_page),
    ];

    for (title, items) in &sections {
        if !items.is_empty() {
            html.push_str(&format!(
                "  <details><summary>{} ({} forms)</summary>\n  <ul>\n",
                title,
                items.len()
            ));
            for item in *items {
                html.push_str(&format!("    <li>{item}</li>\n"));
            }
            html.push_str("  </ul>\n  </details>\n");
        }
    }

    // Full table
    html.push_str(
        r#"
  <h2>All Forms with Edge Cases</h2>
  <table>
    <thead><tr><th>File</th><th>Fields</th><th>Depth</th><th>Scripts</th><th>Pages</th><th>Tags</th></tr></thead>
    <tbody>
"#,
    );

    for form in &report.forms {
        if form.edge_case_tags.is_empty() {
            continue;
        }
        let tags: String = form
            .edge_case_tags
            .iter()
            .map(|t| format!("<span class=\"tag\">{t}</span>"))
            .collect::<Vec<_>>()
            .join(" ");
        html.push_str(&format!(
            "      <tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            form.filename,
            form.field_count,
            form.max_nesting_depth,
            form.formcalc_count,
            form.page_count,
            tags,
        ));
    }

    html.push_str(
        r#"    </tbody>
  </table>
  <footer><p>Generated by XFA Engine edge-case-analyzer.</p></footer>
</body>
</html>
"#,
    );

    html
}
