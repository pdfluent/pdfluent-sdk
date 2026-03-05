//! XFA PDF collector — download, detect, and classify XFA PDFs for the test corpus.

use pdfium_ffi_bridge::pdf_reader::PdfReader;
use pdfium_ffi_bridge::xfa_extract::XfaPackets;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Metadata sidecar for a collected XFA PDF.
#[derive(Debug, Serialize, Deserialize)]
pub struct PdfMetadata {
    pub source_url: String,
    pub filename: String,
    pub xfa_type: String,
    pub field_count: usize,
    pub page_count: usize,
    pub has_formcalc: bool,
    pub has_acroform_hybrid: bool,
    pub packets: Vec<String>,
    pub file_size_bytes: u64,
    pub collected_at: String,
}

/// Classification result for a PDF file.
#[derive(Debug)]
pub struct Classification {
    pub xfa_type: String,
    pub field_count: usize,
    pub page_count: usize,
    pub has_formcalc: bool,
    pub packets: Vec<String>,
}

/// Download PDFs from a URL list, filtering for XFA content.
pub fn collect(url_file: &Path, output_dir: &Path, limit: Option<usize>) -> anyhow::Result<()> {
    let urls = read_url_list(url_file)?;
    let urls = match limit {
        Some(n) => &urls[..n.min(urls.len())],
        None => &urls,
    };

    fs::create_dir_all(output_dir)?;

    let client = reqwest::blocking::ClientBuilder::new()
        .user_agent(
            "xfa-collector/0.1 (XFA research; https://github.com/jasperdew/xfa-native-rust)",
        )
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let mut collected = 0;
    let mut skipped = 0;
    let mut errors = 0;

    for (i, url) in urls.iter().enumerate() {
        let filename = url_to_filename(url, i);
        let pdf_path = output_dir.join(&filename);
        let meta_path = output_dir.join(format!("{}.json", filename.trim_end_matches(".pdf")));

        // Resume support: skip if already downloaded
        if pdf_path.exists() && meta_path.exists() {
            println!("  [{}/{}] SKIP (exists): {}", i + 1, urls.len(), filename);
            collected += 1;
            continue;
        }

        print!("  [{}/{}] GET {}... ", i + 1, urls.len(), url);

        match download_and_check(&client, url, &pdf_path, &meta_path, &filename) {
            Ok(true) => {
                println!("XFA ✓");
                collected += 1;
            }
            Ok(false) => {
                println!("not XFA, skipped");
                skipped += 1;
            }
            Err(e) => {
                println!("ERROR: {e}");
                errors += 1;
            }
        }
    }

    println!();
    println!("Done: {collected} XFA PDFs collected, {skipped} non-XFA skipped, {errors} errors");
    Ok(())
}

/// Download a single PDF, check for XFA, save if XFA.
fn download_and_check(
    client: &reqwest::blocking::Client,
    url: &str,
    pdf_path: &Path,
    meta_path: &Path,
    filename: &str,
) -> anyhow::Result<bool> {
    let response = client.get(url).send()?;
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("HTTP {}", response.status()));
    }

    let bytes = response.bytes()?;

    // Check if it's a PDF
    if bytes.len() < 5 || &bytes[..5] != b"%PDF-" {
        return Err(anyhow::anyhow!("not a PDF file"));
    }

    // Try to detect XFA
    let reader = PdfReader::from_bytes(&bytes)?;
    let xfa_result = reader.extract_xfa();

    match xfa_result {
        Ok(packets) if !packets.packets.is_empty() => {
            // It's XFA — save the PDF and metadata
            fs::write(pdf_path, &bytes)?;

            let classification = classify_xfa(&packets, &reader);
            let today = chrono_free_date();

            let metadata = PdfMetadata {
                source_url: url.to_string(),
                filename: filename.to_string(),
                xfa_type: classification.xfa_type,
                field_count: classification.field_count,
                page_count: classification.page_count,
                has_formcalc: classification.has_formcalc,
                has_acroform_hybrid: false,
                packets: classification.packets,
                file_size_bytes: bytes.len() as u64,
                collected_at: today,
            };

            let json = serde_json::to_string_pretty(&metadata)?;
            fs::write(meta_path, json)?;

            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Classify an XFA PDF based on its packets.
fn classify_xfa(packets: &XfaPackets, reader: &PdfReader) -> Classification {
    let packet_names: Vec<String> = packets.packets.iter().map(|(n, _)| n.clone()).collect();

    let template_xml = packets.template().unwrap_or("");

    // Count <field elements in template
    let field_count = template_xml.matches("<field").count();

    // Check for FormCalc scripts
    let has_formcalc = template_xml.contains("<script")
        || template_xml.contains("<calculate")
        || template_xml.contains("contentType=\"application/x-formcalc\"");

    // Determine static vs dynamic
    // Dynamic XFA typically has layout="tb" (top-to-bottom flow) on subforms
    let xfa_type = if template_xml.contains("layout=\"tb\"")
        || template_xml.contains("layout=\"lr-tb\"")
        || template_xml.contains("layout=\"rl-tb\"")
    {
        "dynamic".to_string()
    } else {
        "static".to_string()
    };

    let page_count = reader.page_count();

    Classification {
        xfa_type,
        field_count,
        page_count,
        has_formcalc,
        packets: packet_names,
    }
}

/// Scan a directory of PDFs and classify each one.
pub fn scan(dir: &Path, report_path: &Path) -> anyhow::Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        })
        .collect();
    entries.sort();

    println!(
        "Scanning {} PDF files in {}...",
        entries.len(),
        dir.display()
    );

    let mut results: Vec<PdfMetadata> = Vec::new();

    for (i, path) in entries.iter().enumerate() {
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        print!("  [{}/{}] {}... ", i + 1, entries.len(), filename);

        match scan_single_pdf(path) {
            Ok(Some(meta)) => {
                println!(
                    "{} XFA, {} fields, {} pages",
                    meta.xfa_type, meta.field_count, meta.page_count
                );
                results.push(meta);
            }
            Ok(None) => {
                println!("not XFA");
            }
            Err(e) => {
                println!("ERROR: {e}");
            }
        }
    }

    // Write report
    let json = serde_json::to_string_pretty(&results)?;
    fs::write(report_path, &json)?;

    println!();
    println!(
        "Report written to {} ({} XFA PDFs found)",
        report_path.display(),
        results.len()
    );
    print_stats_summary(&results);

    Ok(())
}

/// Scan a single PDF file and return metadata if it contains XFA.
fn scan_single_pdf(path: &Path) -> anyhow::Result<Option<PdfMetadata>> {
    let bytes = fs::read(path)?;

    if bytes.len() < 5 || &bytes[..5] != b"%PDF-" {
        return Ok(None);
    }

    let reader = PdfReader::from_bytes(&bytes)?;
    let xfa_result = reader.extract_xfa();

    match xfa_result {
        Ok(packets) if !packets.packets.is_empty() => {
            let classification = classify_xfa(&packets, &reader);
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            Ok(Some(PdfMetadata {
                source_url: String::new(),
                filename,
                xfa_type: classification.xfa_type,
                field_count: classification.field_count,
                page_count: classification.page_count,
                has_formcalc: classification.has_formcalc,
                has_acroform_hybrid: false,
                packets: classification.packets,
                file_size_bytes: bytes.len() as u64,
                collected_at: chrono_free_date(),
            }))
        }
        _ => Ok(None),
    }
}

/// Print corpus statistics from a list of metadata.
pub fn stats(dir: &Path) -> anyhow::Result<()> {
    // Look for report.json first, otherwise scan
    let report_path = dir.join("report.json");
    let results: Vec<PdfMetadata> = if report_path.exists() {
        let json = fs::read_to_string(&report_path)?;
        serde_json::from_str(&json)?
    } else {
        // Collect from .json sidecar files
        let mut metas = Vec::new();
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && path.file_name().is_some_and(|s| s != "report.json")
            {
                if let Ok(json) = fs::read_to_string(&path) {
                    if let Ok(meta) = serde_json::from_str::<PdfMetadata>(&json) {
                        metas.push(meta);
                    }
                }
            }
        }
        metas
    };

    if results.is_empty() {
        println!("No XFA PDFs found. Run 'scan' or 'collect' first.");
        return Ok(());
    }

    print_stats_summary(&results);
    Ok(())
}

fn print_stats_summary(results: &[PdfMetadata]) {
    let total = results.len();
    let dynamic = results.iter().filter(|m| m.xfa_type == "dynamic").count();
    let static_count = results.iter().filter(|m| m.xfa_type == "static").count();
    let with_formcalc = results.iter().filter(|m| m.has_formcalc).count();
    let total_fields: usize = results.iter().map(|m| m.field_count).sum();
    let total_pages: usize = results.iter().map(|m| m.page_count).sum();
    let total_bytes: u64 = results.iter().map(|m| m.file_size_bytes).sum();

    println!();
    println!("=== XFA Corpus Statistics ===");
    println!("  Total XFA PDFs:    {total}");
    println!("  Dynamic XFA:       {dynamic}");
    println!("  Static XFA:        {static_count}");
    println!("  With FormCalc:     {with_formcalc}");
    println!("  Total fields:      {total_fields}");
    println!("  Total pages:       {total_pages}");
    println!(
        "  Total size:        {:.1} MB",
        total_bytes as f64 / 1_048_576.0
    );
    if total > 0 {
        println!(
            "  Avg fields/form:   {:.1}",
            total_fields as f64 / total as f64
        );
        println!(
            "  Avg pages/form:    {:.1}",
            total_pages as f64 / total as f64
        );
    }
}

/// Read a URL list file (one URL per line, # comments, blank lines ignored).
fn read_url_list(path: &Path) -> anyhow::Result<Vec<String>> {
    let content = fs::read_to_string(path)?;
    let urls: Vec<String> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect();
    Ok(urls)
}

/// Convert a URL to a safe, unique filename.
///
/// Includes a hash prefix derived from the full URL to prevent collisions
/// when different hosts serve files with the same basename.
fn url_to_filename(url: &str, index: usize) -> String {
    // Hash the full URL to create a unique prefix
    let hash = simple_hash(url);
    let prefix = format!("{hash:08x}");

    if let Some(last_segment) = url.rsplit('/').next() {
        let clean = last_segment.split('?').next().unwrap_or(last_segment);
        if clean.ends_with(".pdf") && clean.len() < 100 {
            return format!("{prefix}_{clean}");
        }
    }
    format!("{prefix}_pdf_{index:04}.pdf")
}

/// Simple non-cryptographic hash for filename uniqueness.
fn simple_hash(s: &str) -> u32 {
    let mut h: u32 = 0;
    for b in s.bytes() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(b));
    }
    h
}

/// Get today's date as ISO 8601 string without pulling in chrono.
fn chrono_free_date() -> String {
    // Use std::process::Command to get the date, fallback to empty
    std::process::Command::new("date")
        .args(["+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_to_filename_extracts_pdf_name_with_hash() {
        let name = url_to_filename("https://example.com/forms/f1040.pdf", 0);
        assert!(name.ends_with("_f1040.pdf"));
        assert!(name.len() > "f1040.pdf".len()); // hash prefix present
    }

    #[test]
    fn url_to_filename_different_hosts_produce_unique_names() {
        let a = url_to_filename("https://host-a.com/form.pdf", 0);
        let b = url_to_filename("https://host-b.com/form.pdf", 0);
        assert_ne!(a, b); // different URLs → different filenames
        assert!(a.ends_with("_form.pdf"));
        assert!(b.ends_with("_form.pdf"));
    }

    #[test]
    fn url_to_filename_with_query_params() {
        let name = url_to_filename("https://example.com/get?file=form.pdf&v=2", 5);
        assert!(name.contains("pdf_0005.pdf"));
    }

    #[test]
    fn url_to_filename_fallback() {
        let name = url_to_filename("https://example.com/download/abc123", 3);
        assert!(name.contains("pdf_0003.pdf"));
    }

    #[test]
    fn read_url_list_ignores_comments_and_blanks() {
        let dir = std::env::temp_dir().join("xfa_test_urls");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        fs::write(
            &path,
            "# Comment\nhttps://a.com/1.pdf\n\nhttps://b.com/2.pdf\n# end\n",
        )
        .unwrap();

        let urls = read_url_list(&path).unwrap();
        assert_eq!(urls, vec!["https://a.com/1.pdf", "https://b.com/2.pdf"]);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn chrono_free_date_returns_valid_format() {
        let date = chrono_free_date();
        // Should be YYYY-MM-DD format
        assert!(date.len() == 10 || date == "unknown");
        if date != "unknown" {
            assert!(date.chars().nth(4) == Some('-'));
            assert!(date.chars().nth(7) == Some('-'));
        }
    }
}
