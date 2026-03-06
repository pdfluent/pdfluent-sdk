//! PDF/A and PDF/UA compliance validation.

use anyhow::{Context, Result};
use std::path::Path;

use pdf_compliance::{ComplianceReport, PdfALevel, Severity};
use pdf_syntax::Pdf;

pub fn run(input: &Path, profile: &str, json: bool) -> Result<()> {
    let data = std::fs::read(input).context("failed to read input PDF")?;
    let pdf = Pdf::new(data).map_err(|e| anyhow::anyhow!("pdf-syntax parse error: {e:?}"))?;

    let report = match parse_profile(profile)? {
        Profile::PdfA(level) => pdf_compliance::validate_pdfa(&pdf, level),
        Profile::PdfUa => pdf_compliance::validate_pdfua(&pdf),
    };

    if json {
        print_json(&report, input)?;
    } else {
        print_text(&report, input);
    }

    if !report.is_compliant() {
        std::process::exit(1);
    }

    Ok(())
}

enum Profile {
    PdfA(PdfALevel),
    PdfUa,
}

fn parse_profile(s: &str) -> Result<Profile> {
    let normalized = s.to_lowercase().replace(['-', '/'], "");
    match normalized.as_str() {
        "pdfa1a" | "a1a" => Ok(Profile::PdfA(PdfALevel::A1a)),
        "pdfa1b" | "a1b" => Ok(Profile::PdfA(PdfALevel::A1b)),
        "pdfa2a" | "a2a" => Ok(Profile::PdfA(PdfALevel::A2a)),
        "pdfa2b" | "a2b" => Ok(Profile::PdfA(PdfALevel::A2b)),
        "pdfa2u" | "a2u" => Ok(Profile::PdfA(PdfALevel::A2u)),
        "pdfa3a" | "a3a" => Ok(Profile::PdfA(PdfALevel::A3a)),
        "pdfa3b" | "a3b" => Ok(Profile::PdfA(PdfALevel::A3b)),
        "pdfa3u" | "a3u" => Ok(Profile::PdfA(PdfALevel::A3u)),
        "pdfua" | "pdfua1" | "ua" | "ua1" => Ok(Profile::PdfUa),
        _ => anyhow::bail!(
            "unknown profile '{s}'. Supported: pdf-a1a, pdf-a1b, pdf-a2a, pdf-a2b, pdf-a2u, pdf-a3a, pdf-a3b, pdf-a3u, pdf-ua"
        ),
    }
}

fn print_json(report: &ComplianceReport, input: &Path) -> Result<()> {
    let issues: Vec<serde_json::Value> = report
        .issues
        .iter()
        .map(|i| {
            serde_json::json!({
                "rule": i.rule,
                "severity": format!("{:?}", i.severity),
                "message": i.message,
                "location": i.location,
            })
        })
        .collect();

    let result = serde_json::json!({
        "file": input.display().to_string(),
        "compliant": report.is_compliant(),
        "errors": report.error_count(),
        "warnings": report.warning_count(),
        "issues": issues,
    });
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn print_text(report: &ComplianceReport, input: &Path) {
    println!("File: {}", input.display());
    println!(
        "Compliant: {}",
        if report.is_compliant() { "YES" } else { "NO" }
    );
    println!(
        "Errors: {}, Warnings: {}",
        report.error_count(),
        report.warning_count()
    );

    if !report.issues.is_empty() {
        println!();
        for issue in &report.issues {
            let severity = match issue.severity {
                Severity::Error => "ERROR",
                Severity::Warning => "WARN",
                Severity::Info => "INFO",
            };
            let loc = issue
                .location
                .as_deref()
                .map(|l| format!(" [{l}]"))
                .unwrap_or_default();
            println!("  [{severity}] {}{loc}: {}", issue.rule, issue.message);
        }
    }
}
