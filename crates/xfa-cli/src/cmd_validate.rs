//! PDF/A and PDF/UA compliance validation.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use anyhow::{bail, Result};
use std::path::Path;

use crate::error::CliError;
use pdf_compliance::{ComplianceReport, PdfALevel, Severity};
use pdf_syntax::Pdf;

pub fn run(input: &Path, profile: &str, json: bool) -> Result<()> {
    let data = std::fs::read(input).map_err(|e| {
        anyhow::anyhow!(CliError {
            message: format!("Could not read input PDF: {}", input.display()),
            why: Some(e.to_string()),
            fix: Some("Check if the file exists and is readable.".to_string()),
            docs: Some("https://docs.pdfluent.com/errors/E001".to_string()),
        })
    })?;

    let pdf = Pdf::new(data).map_err(|e| {
        anyhow::anyhow!(CliError {
            message: format!("Could not parse PDF: {}", input.display()),
            why: Some(format!("pdf-syntax parse error: {e:?}")),
            fix: Some("Ensure the file is a valid PDF document.".to_string()),
            docs: Some("https://docs.pdfluent.com/errors/E002".to_string()),
        })
    })?;

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
        "pdfa4" | "a4" => Ok(Profile::PdfA(PdfALevel::A4)),
        "pdfa4f" | "a4f" => Ok(Profile::PdfA(PdfALevel::A4f)),
        "pdfa4e" | "a4e" => Ok(Profile::PdfA(PdfALevel::A4e)),
        "pdfua" | "pdfua1" | "ua" | "ua1" => Ok(Profile::PdfUa),
        _ => {
            bail!(CliError {
                message: format!("Unknown profile '{s}'"),
                why: Some("The specified compliance profile is not supported.".to_string()),
                fix: Some("Supported: pdf-a1a, pdf-a1b, pdf-a2a, pdf-a2b, pdf-a2u, pdf-a3a, pdf-a3b, pdf-a3u, pdf-ua".to_string()),
                docs: Some("https://docs.pdfluent.com/errors/E003".to_string()),
            })
        }
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
