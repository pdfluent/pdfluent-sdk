//! Accuracy Report — measure field conversion accuracy across the test corpus.
//!
//! Scans the corpus directory, extracts XFA fields from each PDF, and
//! reports per-form and aggregate accuracy metrics.

use clap::Parser;
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "accuracy-report",
    about = "Measure XFA field conversion accuracy across the test corpus"
)]
struct Cli {
    /// Path to corpus directory containing XFA PDFs.
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,

    /// Output file for JSON report.
    #[arg(long, default_value = "reports/accuracy/report.json")]
    output: PathBuf,

    /// Also generate HTML report.
    #[arg(long)]
    html: bool,
}

#[derive(Debug, Serialize)]
struct FormReport {
    filename: String,
    has_xfa: bool,
    has_template: bool,
    has_datasets: bool,
    packet_names: Vec<String>,
    field_count: usize,
    subform_count: usize,
    draw_count: usize,
    page_count: usize,
    category: String,
    status: FormStatus,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum FormStatus {
    Parsed,
    PartialParse,
    XfaNotFound,
    LoadFailed,
}

#[derive(Debug, Serialize)]
struct AccuracyReport {
    total_forms: usize,
    xfa_forms: usize,
    parsed_forms: usize,
    partial_forms: usize,
    failed_forms: usize,
    total_fields: usize,
    total_subforms: usize,
    total_draws: usize,
    parse_rate_pct: f64,
    categories: Vec<CategorySummary>,
    forms: Vec<FormReport>,
}

#[derive(Debug, Serialize)]
struct CategorySummary {
    name: String,
    total: usize,
    parsed: usize,
    fields: usize,
    parse_rate_pct: f64,
}

fn main() {
    let cli = Cli::parse();

    if !cli.corpus.exists() {
        eprintln!(
            "ERROR: Corpus directory not found: {}",
            cli.corpus.display()
        );
        eprintln!("Run `cargo run --bin xfa-collector` first to build the corpus.");
        std::process::exit(1);
    }

    // Collect PDF files
    let mut pdfs: Vec<PathBuf> = std::fs::read_dir(&cli.corpus)
        .expect("Failed to read corpus directory")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "pdf"))
        .collect();
    pdfs.sort();

    if pdfs.is_empty() {
        eprintln!("No PDF files found in {}", cli.corpus.display());
        std::process::exit(1);
    }

    println!("Accuracy Report");
    println!("================");
    println!("Corpus: {} ({} PDFs)", cli.corpus.display(), pdfs.len());
    println!();

    let mut forms = Vec::new();

    for pdf_path in &pdfs {
        let filename = pdf_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let report = analyze_pdf(pdf_path, &filename);

        let status_str = match report.status {
            FormStatus::Parsed => "PARSED",
            FormStatus::PartialParse => "PARTIAL",
            FormStatus::XfaNotFound => "NO XFA",
            FormStatus::LoadFailed => "FAILED",
        };

        printf_status(&filename, status_str, report.field_count);
        forms.push(report);
    }

    // Compute aggregates
    let total_forms = forms.len();
    let xfa_forms = forms.iter().filter(|f| f.has_xfa).count();
    let parsed_forms = forms
        .iter()
        .filter(|f| matches!(f.status, FormStatus::Parsed))
        .count();
    let partial_forms = forms
        .iter()
        .filter(|f| matches!(f.status, FormStatus::PartialParse))
        .count();
    let failed_forms = forms
        .iter()
        .filter(|f| matches!(f.status, FormStatus::LoadFailed))
        .count();
    let total_fields: usize = forms.iter().map(|f| f.field_count).sum();
    let total_subforms: usize = forms.iter().map(|f| f.subform_count).sum();
    let total_draws: usize = forms.iter().map(|f| f.draw_count).sum();

    let parse_rate_pct = if total_forms > 0 {
        ((parsed_forms + partial_forms) as f64 / total_forms as f64) * 100.0
    } else {
        0.0
    };

    // Category breakdown
    let categories = compute_categories(&forms);

    let report = AccuracyReport {
        total_forms,
        xfa_forms,
        parsed_forms,
        partial_forms,
        failed_forms,
        total_fields,
        total_subforms,
        total_draws,
        parse_rate_pct,
        categories,
        forms,
    };

    // Print summary
    println!();
    println!("Summary");
    println!("-------");
    println!("Total forms:    {total_forms}");
    println!("XFA forms:      {xfa_forms}");
    println!("Parsed:         {parsed_forms}");
    println!("Partial parse:  {partial_forms}");
    println!("Failed:         {failed_forms}");
    println!("Total fields:   {total_fields}");
    println!("Total subforms: {total_subforms}");
    println!("Parse rate:     {parse_rate_pct:.1}%");

    println!();
    println!("Categories");
    println!("----------");
    for cat in &report.categories {
        println!(
            "  {:<20} {:>3} forms, {:>5} fields, {:.1}% parsed",
            cat.name, cat.total, cat.fields, cat.parse_rate_pct
        );
    }

    // Write JSON report
    if let Some(parent) = cli.output.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let json = serde_json::to_string_pretty(&report).expect("JSON serialization failed");
    std::fs::write(&cli.output, &json).expect("Failed to write report");
    println!();
    println!("JSON report: {}", cli.output.display());

    // HTML report
    if cli.html {
        let html_path = cli.output.with_extension("html");
        let html = generate_html_report(&report);
        std::fs::write(&html_path, html).expect("Failed to write HTML report");
        println!("HTML report: {}", html_path.display());
    }
}

fn printf_status(filename: &str, status: &str, field_count: usize) {
    println!(
        "  {:<40} {:>8}  {:>4} fields",
        filename, status, field_count
    );
}

fn analyze_pdf(path: &std::path::Path, filename: &str) -> FormReport {
    let category = categorize_filename(filename);
    let mut errors = Vec::new();

    // Load PDF
    let reader = match PdfReader::from_file(path) {
        Ok(r) => r,
        Err(e) => {
            return FormReport {
                filename: filename.to_string(),
                has_xfa: false,
                has_template: false,
                has_datasets: false,
                packet_names: vec![],
                field_count: 0,
                subform_count: 0,
                draw_count: 0,
                page_count: 0,
                category,
                status: FormStatus::LoadFailed,
                errors: vec![e.to_string()],
            };
        }
    };

    let page_count = reader.page_count();

    // Extract XFA
    let xfa = match reader.extract_xfa() {
        Ok(packets) => packets,
        Err(_) => {
            return FormReport {
                filename: filename.to_string(),
                has_xfa: false,
                has_template: false,
                has_datasets: false,
                packet_names: vec![],
                field_count: 0,
                subform_count: 0,
                draw_count: 0,
                page_count,
                category,
                status: FormStatus::XfaNotFound,
                errors: vec![],
            };
        }
    };

    let has_xfa = !xfa.packets.is_empty() || xfa.full_xml.is_some();
    let has_template = xfa.template().is_some();
    let has_datasets = xfa.datasets().is_some();
    let packet_names: Vec<String> = xfa.packets.iter().map(|(n, _)| n.clone()).collect();

    if !has_xfa {
        return FormReport {
            filename: filename.to_string(),
            has_xfa: false,
            has_template: false,
            has_datasets: false,
            packet_names,
            field_count: 0,
            subform_count: 0,
            draw_count: 0,
            page_count,
            category,
            status: FormStatus::XfaNotFound,
            errors: vec![],
        };
    }

    // Parse template to count fields
    let mut field_count = 0;
    let mut subform_count = 0;
    let mut draw_count = 0;

    if let Some(template_xml) = xfa.template() {
        match count_template_elements(template_xml) {
            Ok((fields, subforms, draws)) => {
                field_count = fields;
                subform_count = subforms;
                draw_count = draws;
            }
            Err(e) => {
                errors.push(format!("Template parse error: {e}"));
            }
        }
    }

    let status = if errors.is_empty() && has_template {
        FormStatus::Parsed
    } else if has_xfa {
        FormStatus::PartialParse
    } else {
        FormStatus::XfaNotFound
    };

    FormReport {
        filename: filename.to_string(),
        has_xfa,
        has_template,
        has_datasets,
        packet_names,
        field_count,
        subform_count,
        draw_count,
        page_count,
        category,
        status,
        errors,
    }
}

fn count_template_elements(xml: &str) -> std::result::Result<(usize, usize, usize), String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;

    let mut fields = 0;
    let mut subforms = 0;
    let mut draws = 0;

    for node in doc.descendants() {
        if node.is_element() {
            match node.tag_name().name() {
                "field" => fields += 1,
                "subform" => subforms += 1,
                "draw" => draws += 1,
                _ => {}
            }
        }
    }

    Ok((fields, subforms, draws))
}

fn categorize_filename(filename: &str) -> String {
    let lower = filename.to_lowercase();
    if lower.starts_with("f1")
        || lower.starts_with("f2")
        || lower.starts_with("f3")
        || lower.starts_with("f4")
        || lower.starts_with("f5")
        || lower.starts_with("f6")
        || lower.starts_with("f7")
        || lower.starts_with("f8")
        || lower.starts_with("f9")
        || lower.starts_with("fw")
        || lower.starts_with("fss")
    {
        "irs_tax".to_string()
    } else if lower.starts_with("i-") {
        "uscis_immigration".to_string()
    } else if lower.contains("-fill-") {
        "canadian_tax".to_string()
    } else if lower.starts_with("sf") {
        "us_standard".to_string()
    } else if lower.starts_with("n-") {
        "uscis_naturalization".to_string()
    } else {
        "other".to_string()
    }
}

fn compute_categories(forms: &[FormReport]) -> Vec<CategorySummary> {
    let mut cat_map: std::collections::BTreeMap<String, (usize, usize, usize)> =
        std::collections::BTreeMap::new();

    for form in forms {
        let entry = cat_map.entry(form.category.clone()).or_insert((0, 0, 0));
        entry.0 += 1;
        if matches!(form.status, FormStatus::Parsed | FormStatus::PartialParse) {
            entry.1 += 1;
        }
        entry.2 += form.field_count;
    }

    cat_map
        .into_iter()
        .map(|(name, (total, parsed, fields))| {
            let parse_rate_pct = if total > 0 {
                (parsed as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            CategorySummary {
                name,
                total,
                parsed,
                fields,
                parse_rate_pct,
            }
        })
        .collect()
}

fn generate_html_report(report: &AccuracyReport) -> String {
    let mut html = String::from(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Field Conversion Accuracy Report</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; max-width: 1000px; margin: 2rem auto; padding: 0 1rem; color: #333; }
    h1 { border-bottom: 2px solid #059669; padding-bottom: 0.5rem; }
    .summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(130px, 1fr)); gap: 1rem; margin: 1.5rem 0; }
    .card { background: #f8fafc; border: 1px solid #e2e8f0; border-radius: 8px; padding: 1rem; text-align: center; }
    .card .value { font-size: 2rem; font-weight: 700; }
    .card .label { font-size: 0.875rem; color: #64748b; margin-top: 0.25rem; }
    .pass { color: #16a34a; }
    .fail { color: #dc2626; }
    .warn { color: #d97706; }
    .bar { background: #e2e8f0; border-radius: 999px; height: 24px; overflow: hidden; margin: 1rem 0; }
    .bar .fill { height: 100%; border-radius: 999px; background: #059669; }
    table { width: 100%; border-collapse: collapse; margin-top: 1rem; font-size: 0.875rem; }
    th, td { padding: 0.4rem 0.6rem; text-align: left; border-bottom: 1px solid #e2e8f0; }
    th { background: #f1f5f9; font-weight: 600; }
    .badge { display: inline-block; padding: 2px 8px; border-radius: 4px; font-size: 0.7rem; font-weight: 600; }
    .badge.parsed { background: #dcfce7; color: #166534; }
    .badge.partial { background: #fef3c7; color: #92400e; }
    .badge.noxfa { background: #f1f5f9; color: #64748b; }
    .badge.failed { background: #fef2f2; color: #991b1b; }
    footer { margin-top: 2rem; padding-top: 1rem; border-top: 1px solid #e2e8f0; font-size: 0.875rem; color: #94a3b8; }
  </style>
</head>
<body>
"#,
    );

    html.push_str(&format!(
        r#"  <h1>Field Conversion Accuracy Report</h1>
  <div class="summary">
    <div class="card"><div class="value">{}</div><div class="label">Total Forms</div></div>
    <div class="card"><div class="value pass">{}</div><div class="label">XFA Forms</div></div>
    <div class="card"><div class="value pass">{}</div><div class="label">Parsed</div></div>
    <div class="card"><div class="value warn">{}</div><div class="label">Partial</div></div>
    <div class="card"><div class="value fail">{}</div><div class="label">Failed</div></div>
    <div class="card"><div class="value">{}</div><div class="label">Total Fields</div></div>
    <div class="card"><div class="value">{:.1}%</div><div class="label">Parse Rate</div></div>
  </div>
  <div class="bar"><div class="fill" style="width: {:.1}%"></div></div>
"#,
        report.total_forms,
        report.xfa_forms,
        report.parsed_forms,
        report.partial_forms,
        report.failed_forms,
        report.total_fields,
        report.parse_rate_pct,
        report.parse_rate_pct,
    ));

    // Category table
    html.push_str(
        r#"  <h2>By Category</h2>
  <table>
    <thead><tr><th>Category</th><th>Forms</th><th>Parsed</th><th>Fields</th><th>Rate</th></tr></thead>
    <tbody>
"#,
    );
    for cat in &report.categories {
        html.push_str(&format!(
            "      <tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.1}%</td></tr>\n",
            cat.name, cat.total, cat.parsed, cat.fields, cat.parse_rate_pct,
        ));
    }
    html.push_str("    </tbody>\n  </table>\n\n");

    // Per-form table
    html.push_str(
        r#"  <h2>Per-Form Results</h2>
  <table>
    <thead><tr><th>File</th><th>Status</th><th>Fields</th><th>Subforms</th><th>Pages</th><th>Category</th></tr></thead>
    <tbody>
"#,
    );
    for form in &report.forms {
        let (badge_class, label) = match form.status {
            FormStatus::Parsed => ("parsed", "PARSED"),
            FormStatus::PartialParse => ("partial", "PARTIAL"),
            FormStatus::XfaNotFound => ("noxfa", "NO XFA"),
            FormStatus::LoadFailed => ("failed", "FAILED"),
        };
        html.push_str(&format!(
            "      <tr><td>{}</td><td><span class=\"badge {}\">{}</span></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            form.filename, badge_class, label, form.field_count, form.subform_count, form.page_count, form.category,
        ));
    }
    html.push_str("    </tbody>\n  </table>\n\n");

    html.push_str(
        r#"  <footer>
    <p>Generated by XFA Engine accuracy-report tool.</p>
  </footer>
</body>
</html>
"#,
    );

    html
}
