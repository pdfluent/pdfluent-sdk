//! Signature validation and display.

use anyhow::{Context, Result};
use std::path::Path;

use pdf_sign::{DocumentSecurityStore, ValidationStatus};
use pdf_syntax::Pdf;

pub fn run(input: &Path, json: bool) -> Result<()> {
    let data = std::fs::read(input).context("failed to read input PDF")?;
    let pdf = Pdf::new(data).map_err(|e| anyhow::anyhow!("pdf-syntax parse error: {e:?}"))?;

    let results = pdf_sign::validate_signatures(&pdf);
    let docmdp = pdf_sign::get_docmdp_permission(&pdf);
    let dss = DocumentSecurityStore::from_pdf(&pdf);

    if json {
        let sigs: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                let status_str = match &r.status {
                    ValidationStatus::Valid => "valid".to_string(),
                    ValidationStatus::Invalid(msg) => format!("invalid: {msg}"),
                    ValidationStatus::Unknown(msg) => format!("unknown: {msg}"),
                };
                serde_json::json!({
                    "field": r.field_name,
                    "status": status_str,
                    "signer": r.signer,
                    "timestamp": r.timestamp,
                    "sub_filter": r.sub_filter.map(|sf| format!("{sf:?}")),
                })
            })
            .collect();

        let result = serde_json::json!({
            "file": input.display().to_string(),
            "signatures": sigs,
            "docmdp": docmdp.as_ref().map(|d| format!("{:?}", d.permission)),
            "dss": dss.as_ref().map(|d| serde_json::json!({
                "has_ltv": d.has_ltv_data(),
                "certificates": d.certificates.len(),
                "ocsp_responses": d.ocsp_responses.len(),
                "crls": d.crls.len(),
                "vri_entries": d.vri_entries.len(),
            })),
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("File: {}", input.display());

        if results.is_empty() {
            println!("No signatures found.");
        } else {
            println!("Signatures: {}", results.len());
            for r in &results {
                let status = match &r.status {
                    ValidationStatus::Valid => "VALID",
                    ValidationStatus::Invalid(_) => "INVALID",
                    ValidationStatus::Unknown(_) => "UNKNOWN",
                };
                println!("\n  Field: {}", r.field_name);
                println!("  Status: {status}");
                match &r.status {
                    ValidationStatus::Invalid(msg) | ValidationStatus::Unknown(msg) => {
                        println!("  Detail: {msg}");
                    }
                    _ => {}
                }
                if let Some(signer) = &r.signer {
                    println!("  Signer: {signer}");
                }
                if let Some(ts) = &r.timestamp {
                    println!("  Timestamp: {ts}");
                }
                if let Some(sf) = &r.sub_filter {
                    println!("  SubFilter: {sf:?}");
                }
            }
        }

        if let Some(d) = &docmdp {
            println!("\nDocMDP: {:?}", d.permission);
            if let Some(f) = &d.certifying_field {
                println!("  Certifying field: {f}");
            }
        }

        if let Some(d) = &dss {
            println!(
                "\nDSS: {} certs, {} OCSPs, {} CRLs",
                d.certificates.len(),
                d.ocsp_responses.len(),
                d.crls.len()
            );
            if d.has_ltv_data() {
                println!("  LTV data: present");
            }
            for vri in &d.vri_entries {
                println!(
                    "  VRI {}: {} certs, {} OCSPs, {} CRLs",
                    &vri.key[..8.min(vri.key.len())],
                    vri.certificates.len(),
                    vri.ocsp_responses.len(),
                    vri.crls.len()
                );
            }
        }
    }

    Ok(())
}
