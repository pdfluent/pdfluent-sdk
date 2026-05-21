//! `measure` — flatten one PDF under one rendering policy and emit stable JSON
//! metrics for the D13 FreshMerge corpus measurement harness.
//!
//! This command is a thin measurement wrapper around
//! [`pdf_xfa::flatten_xfa_to_pdf_with_policy_and_metadata`]. It does **not**
//! change any flatten or rendering behavior. The default policy is
//! `saved-state` ([`pdf_xfa::XfaRenderingPolicy::SavedStateFaithful`]);
//! `fresh-merge` selects the experimental
//! [`pdf_xfa::XfaRenderingPolicy::FreshMergeExperimental`] policy.
//!
//! See `benchmarks/runs/xfa_enterprise_plan/d13a_xfa_cli_measure_subcommand/PHASE1_MEASURE_CONTRACT.md`
//! for the interface contract.

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;

/// One measurement record emitted by `measure`. Field names overlapping the D13
/// measurement schema use the schema's exact names so the runner's metrics
/// parser can read them directly.
#[derive(Serialize)]
struct MeasureOutput {
    doc_id: String,
    /// Input filename basename only — never the full local path (privacy).
    input_label: String,
    /// `SavedStateFaithful` | `FreshMergeExperimental` (matches schema enum).
    policy: String,
    provider_oracle_scope: String,
    oracle_scope: String,
    success: bool,
    error_kind: Option<String>,
    page_count: Option<usize>,
    text_ops: Option<usize>,
    text_chars: Option<usize>,
    file_size_bytes: Option<usize>,
    structural_xfa_retained: Option<bool>,
    runtime_errors: usize,
    formdom_unmatched_count: Option<usize>,
    fresh_merge_admitted_nodes: usize,
    output_valid: bool,
    output_validity_error: Option<String>,
    version: String,
    binary_sha: Option<String>,
    measured_at: String,
}

/// Run the `measure` subcommand.
///
/// # Errors
/// Returns an error if the input cannot be read or the JSON cannot be written.
/// Flatten failures are **not** returned as errors — they are recorded in the
/// output JSON with `success=false`.
#[allow(clippy::too_many_arguments)]
pub fn run(
    input: &Path,
    policy_token: &str,
    output_json: Option<&Path>,
    doc_id: Option<&str>,
    oracle_scope: &str,
    provider: &str,
    output_pdf: Option<&Path>,
    no_write_output_pdf: bool,
    trace: bool,
) -> Result<()> {
    // Resolve policy. Unknown token fails loudly (exit 2), matching `flatten`.
    let policy = match pdf_xfa::XfaRenderingPolicy::from_token(policy_token) {
        Some(p) => p,
        None => {
            eprintln!(
                "unknown --policy '{policy_token}' (expected 'saved-state' or 'fresh-merge')"
            );
            std::process::exit(2);
        }
    };

    // Optional provenance trace for this run only (does not change output).
    if trace {
        std::env::set_var("XFA_PRESENCE_PROV", "1");
    }

    let input_label = input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input.pdf".to_string());
    let resolved_doc_id = doc_id.map(str::to_string).unwrap_or_else(|| {
        input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| input_label.clone())
    });

    let pdf_bytes = std::fs::read(input).context("failed to read input PDF")?;

    let measured_at = unix_to_iso8601(now_unix_secs());
    let version = env!("CARGO_PKG_VERSION").to_string();
    let binary_sha = option_env!("GIT_SHA").map(str::to_string);
    let policy_label = format!("{policy:?}"); // SavedStateFaithful | FreshMergeExperimental

    // Flatten under the requested policy. Failures are recorded, not thrown.
    let out = match pdf_xfa::flatten_xfa_to_pdf_with_policy_and_metadata(&pdf_bytes, policy) {
        Ok((flat_bytes, metadata)) => {
            let runtime_errors = metadata
                .dynamic_scripts
                .formcalc_errors
                .saturating_add(metadata.dynamic_scripts.js_runtime_errors);

            // Re-load the output to validate it and to measure pages / text ops.
            let (page_count, text_ops, structural_xfa_retained, output_valid, output_err) =
                match lopdf::Document::load_mem(&flat_bytes) {
                    Ok(doc) => (
                        Some(doc.get_pages().len()),
                        Some(count_text_ops(&doc)),
                        xfa_retained(&doc),
                        true,
                        None,
                    ),
                    Err(e) => (None, None, None, false, Some(format!("{e}"))),
                };

            // Write the flattened PDF only if explicitly requested and not suppressed.
            if let Some(pdf_path) = output_pdf {
                if no_write_output_pdf {
                    eprintln!(
                        "note: --no-write-output-pdf set; not writing PDF to {}",
                        pdf_path.display()
                    );
                } else {
                    std::fs::write(pdf_path, &flat_bytes)
                        .context("failed to write --output-pdf")?;
                }
            }

            MeasureOutput {
                doc_id: resolved_doc_id,
                input_label,
                policy: policy_label,
                provider_oracle_scope: provider.to_string(),
                oracle_scope: oracle_scope.to_string(),
                success: true,
                error_kind: None,
                page_count,
                text_ops,
                text_chars: None,
                file_size_bytes: Some(flat_bytes.len()),
                structural_xfa_retained,
                runtime_errors,
                formdom_unmatched_count: None,
                fresh_merge_admitted_nodes: metadata.fresh_merge_admitted_nodes,
                output_valid,
                output_validity_error: output_err,
                version,
                binary_sha,
                measured_at,
            }
        }
        Err(e) => {
            let error_kind = match &e {
                pdf_xfa::error::XfaError::Encrypted(_) => "encrypted",
                _ => "flatten_error",
            };
            MeasureOutput {
                doc_id: resolved_doc_id,
                input_label,
                policy: policy_label,
                provider_oracle_scope: provider.to_string(),
                oracle_scope: oracle_scope.to_string(),
                success: false,
                error_kind: Some(format!("{error_kind}: {e}")),
                page_count: None,
                text_ops: None,
                text_chars: None,
                file_size_bytes: None,
                structural_xfa_retained: None,
                runtime_errors: 0,
                formdom_unmatched_count: None,
                fresh_merge_admitted_nodes: 0,
                output_valid: false,
                output_validity_error: None,
                version,
                binary_sha,
                measured_at,
            }
        }
    };

    // Stable, pretty JSON.
    let json = serde_json::to_string_pretty(&out).context("failed to serialize measure output")?;

    match output_json {
        Some(json_path) => {
            if let Some(parent) = json_path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)
                    .context("failed to create --output-json directory")?;
            }
            std::fs::write(json_path, json.as_bytes()).context("failed to write --output-json")?;
            // Failure is recorded in the file; batch runners continue. Exit 0.
        }
        None => {
            println!("{json}");
            // Standalone failure should be visible via exit code.
            if !out.success {
                let code = if out
                    .error_kind
                    .as_deref()
                    .is_some_and(|k| k.starts_with("encrypted"))
                {
                    2
                } else {
                    1
                };
                std::process::exit(code);
            }
        }
    }

    Ok(())
}

/// Count `Tj`/`TJ` text-showing operators across all page content streams.
fn count_text_ops(doc: &lopdf::Document) -> usize {
    let mut count = 0usize;
    for (_, page_id) in doc.get_pages() {
        if let Ok(content_bytes) = doc.get_page_content(page_id) {
            if let Ok(content) = lopdf::content::Content::decode(&content_bytes) {
                for op in content.operations {
                    if op.operator == "Tj" || op.operator == "TJ" {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}

/// Best-effort check: does the flattened output still carry an XFA packet in its
/// catalog AcroForm? Returns `None` when the catalog cannot be read.
fn xfa_retained(doc: &lopdf::Document) -> Option<bool> {
    let root_id = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())?;
    let root = doc.get_dictionary(root_id).ok()?;
    let acroform = match root.get(b"AcroForm") {
        Ok(obj) => obj,
        // No AcroForm at all → definitely no XFA retained.
        Err(_) => return Some(false),
    };
    let acroform_dict = match acroform {
        lopdf::Object::Reference(r) => doc.get_dictionary(*r).ok()?,
        lopdf::Object::Dictionary(d) => d,
        _ => return Some(false),
    };
    Some(acroform_dict.get(b"XFA").is_ok())
}

/// Current time as whole seconds since the Unix epoch (0 if the clock is before
/// the epoch, which should not happen).
fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Convert a Unix timestamp (seconds, UTC) to an ISO-8601 string
/// `YYYY-MM-DDTHH:MM:SSZ`. Dependency-free; uses Howard Hinnant's
/// `civil_from_days` algorithm.
fn unix_to_iso8601(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (hh, mm, ss) = (tod / 3600, (tod % 3600) / 60, tod % 60);

    // Shift epoch to 0000-03-01 for the era-based civil-date computation.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };

    format!("{year:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_epoch_zero() {
        assert_eq!(unix_to_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn iso8601_known_datetime() {
        // 2021-01-01T00:00:00Z == 1609459200
        assert_eq!(unix_to_iso8601(1_609_459_200), "2021-01-01T00:00:00Z");
        // 2026-05-22T12:34:56Z == 1779453296 (verified against Python datetime)
        assert_eq!(unix_to_iso8601(1_779_453_296), "2026-05-22T12:34:56Z");
    }

    #[test]
    fn iso8601_leap_day() {
        // 2020-02-29T23:59:59Z == 1583020799
        assert_eq!(unix_to_iso8601(1_583_020_799), "2020-02-29T23:59:59Z");
    }
}
