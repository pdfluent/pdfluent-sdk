//! Debug tool for PDF/A conversion: runs the full pipeline with detailed
//! per-step reporting and validates the output with pdf-compliance.
//!
//! Usage:
//!   cargo run -p pdf-manip --example debug_pdfa -- input.pdf [output.pdf]
//!
//! If no output path is given the converted PDF is written to a temporary file
//! whose path is printed so you can feed it to veraPDF.

use std::collections::BTreeMap;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <input.pdf> [output.pdf]", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = args.get(2).cloned();

    // --- Load input --------------------------------------------------------
    let data = std::fs::read(input_path).unwrap_or_else(|e| {
        eprintln!("Failed to read {input_path}: {e}");
        std::process::exit(1);
    });
    println!("Input: {input_path} ({} bytes)", data.len());

    let mut doc = lopdf::Document::load_mem(&data).unwrap_or_else(|e| {
        eprintln!("Failed to parse PDF: {e}");
        std::process::exit(1);
    });

    let total_start = Instant::now();
    println!();
    println!("=== PDF/A Conversion Pipeline ===");
    println!();

    // --- Step 1: cleanup_for_pdfa ------------------------------------------
    let step_start = Instant::now();
    print_step(1, "cleanup_for_pdfa");
    match pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false) {
        Ok(r) => {
            let mut details: Vec<String> = Vec::new();
            if r.js_actions_removed > 0 {
                details.push(format!("JS actions removed: {}", r.js_actions_removed));
            }
            if r.embedded_files_removed > 0 {
                details.push(format!(
                    "embedded files removed: {}",
                    r.embedded_files_removed
                ));
            }
            if r.transparency_groups_found > 0 {
                details.push(format!(
                    "transparency groups found: {}",
                    r.transparency_groups_found
                ));
            }
            if r.encryption_removed {
                details.push("encryption removed".into());
            }
            if r.aa_entries_removed > 0 {
                details.push(format!("AA entries removed: {}", r.aa_entries_removed));
            }
            if r.transfer_functions_removed > 0 {
                details.push(format!(
                    "transfer functions removed: {}",
                    r.transfer_functions_removed
                ));
            }
            if r.rendering_intents_fixed > 0 {
                details.push(format!(
                    "rendering intents fixed: {}",
                    r.rendering_intents_fixed
                ));
            }
            if r.trailer_id_added {
                details.push("trailer /ID added".into());
            }
            if r.annotation_flags_fixed > 0 {
                details.push(format!(
                    "annotation flags fixed: {}",
                    r.annotation_flags_fixed
                ));
            }
            if r.lzw_streams_reencoded > 0 {
                details.push(format!(
                    "LZW streams re-encoded: {}",
                    r.lzw_streams_reencoded
                ));
            }
            if r.ocg_fixes > 0 {
                details.push(format!("OCG fixes: {}", r.ocg_fixes));
            }
            if r.cidtogidmap_added > 0 {
                details.push(format!("CIDToGIDMap added: {}", r.cidtogidmap_added));
            }
            if r.ap_fixes > 0 {
                details.push(format!("AP fixes: {}", r.ap_fixes));
            }
            if details.is_empty() {
                details.push("no changes needed".into());
            }
            for d in &details {
                println!("       - {d}");
            }
        }
        Err(e) => println!("       ERROR: {e}"),
    }
    print_elapsed(step_start);

    // --- Step 2: embed_fonts -----------------------------------------------
    let step_start = Instant::now();
    print_step(2, "embed_fonts");
    match pdf_manip::pdfa_fonts::embed_fonts(&mut doc) {
        Ok(r) => {
            println!("       - fonts inspected: {}", r.fonts_inspected);
            println!("       - non-embedded found: {}", r.non_embedded_found);
            println!("       - fonts embedded: {}", r.fonts_embedded);
            if !r.failed.is_empty() {
                println!("       - failed ({}):", r.failed.len());
                for (name, reason) in &r.failed {
                    println!("           {name}: {reason}");
                }
            }
        }
        Err(e) => println!("       ERROR: {e}"),
    }
    print_elapsed(step_start);

    // --- Step 3: fix_cff_widths --------------------------------------------
    let step_start = Instant::now();
    print_step(3, "fix_cff_widths");
    let cff_widths = pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    println!("       - fonts fixed: {cff_widths}");
    print_elapsed(step_start);

    // --- Step 4: fix_truetype_cid_widths -----------------------------------
    let step_start = Instant::now();
    print_step(4, "fix_truetype_cid_widths");
    let tt_cid_widths = pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    println!("       - fonts fixed: {tt_cid_widths}");
    print_elapsed(step_start);

    // --- Step 5: fix_type1_charset -----------------------------------------
    let step_start = Instant::now();
    print_step(5, "fix_type1_charset");
    let charset_fixed = pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    println!("       - fonts fixed: {charset_fixed}");
    print_elapsed(step_start);

    // --- Step 6: fix_truetype_encoding -------------------------------------
    let step_start = Instant::now();
    print_step(6, "fix_truetype_encoding");
    let enc_fixed = pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    println!("       - fonts fixed: {enc_fixed}");
    print_elapsed(step_start);

    // --- Step 7: fix_font_width_mismatches ----------------------------------
    let step_start = Instant::now();
    print_step(7, "fix_font_width_mismatches");
    let width_fixed = pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    println!("       - fonts fixed: {width_fixed}");
    print_elapsed(step_start);

    // --- Step 7b: fix_symbolic_font_flags -----------------------------------
    let step_start = Instant::now();
    print_step(7, "fix_symbolic_font_flags");
    let sym_fixed = pdf_manip::pdfa_fonts::fix_symbolic_font_flags(&mut doc);
    println!("       - fonts fixed: {sym_fixed}");
    print_elapsed(step_start);

    // --- Step 8: fix_cidset ------------------------------------------------
    let step_start = Instant::now();
    print_step(8, "fix_cidset");
    let cidset_fixed = pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
    println!("       - fonts fixed: {cidset_fixed}");
    print_elapsed(step_start);

    // --- Step 9: normalize_colorspaces -------------------------------------
    let step_start = Instant::now();
    print_step(8, "normalize_colorspaces");
    match pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc) {
        Ok(r) => {
            println!("       - had OutputIntent: {}", r.had_output_intent);
            println!("       - OutputIntent added: {}", r.output_intent_added);
            println!("       - pages scanned: {}", r.pages_scanned);
            if !r.device_colorspaces_found.is_empty() {
                println!(
                    "       - device colorspaces: {}",
                    r.device_colorspaces_found.join(", ")
                );
            }
            if r.separations_unified > 0 {
                println!("       - separations unified: {}", r.separations_unified);
            }
            if r.overprint_mode_fixed > 0 {
                println!("       - overprint mode fixed: {}", r.overprint_mode_fixed);
            }
            if r.icc_n_fixed > 0 {
                println!("       - ICC /N fixed: {}", r.icc_n_fixed);
            }
        }
        Err(e) => println!("       ERROR: {e}"),
    }
    print_elapsed(step_start);

    // --- Step 9: repair_xmp_metadata ---------------------------------------
    let step_start = Instant::now();
    print_step(9, "repair_xmp_metadata (A2b)");
    match pdf_manip::pdfa_xmp::repair_xmp_metadata(
        &mut doc,
        pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        None,
    ) {
        Ok(r) => {
            println!(
                "       - XMP {}: info_synced={}, pdfa_id_set={}",
                if r.xmp_created { "created" } else { "updated" },
                r.info_synced,
                r.pdfa_id_set
            );
        }
        Err(e) => println!("       ERROR: {e}"),
    }
    print_elapsed(step_start);

    // --- Step 10: save + fix_pdf_header ------------------------------------
    let step_start = Instant::now();
    print_step(10, "save + fix_pdf_header");
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap_or_else(|e| {
        eprintln!("       ERROR saving: {e}");
        std::process::exit(1);
    });
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
    println!("       - output size: {} bytes", saved.len());
    print_elapsed(step_start);

    let pipeline_elapsed = total_start.elapsed();
    println!();
    println!(
        "Pipeline completed in {:.1}ms",
        pipeline_elapsed.as_secs_f64() * 1000.0
    );

    // --- Write output ------------------------------------------------------
    let out_path = if let Some(p) = output_path {
        std::fs::write(&p, &saved).unwrap_or_else(|e| {
            eprintln!("Failed to write output: {e}");
            std::process::exit(1);
        });
        p
    } else {
        let dir = std::env::temp_dir();
        let stem = std::path::Path::new(input_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let tmp_path = dir.join(format!("{stem}_pdfa.pdf"));
        std::fs::write(&tmp_path, &saved).unwrap_or_else(|e| {
            eprintln!("Failed to write temp output: {e}");
            std::process::exit(1);
        });
        tmp_path.to_string_lossy().into_owned()
    };
    println!("Output: {out_path}");

    // --- Compliance validation ---------------------------------------------
    println!();
    println!("=== PDF/A Compliance Validation (A2b) ===");
    println!();

    let val_start = Instant::now();
    let pdf = match pdf_syntax::Pdf::new(saved) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to re-parse output with pdf-syntax: {e:?}");
            std::process::exit(1);
        }
    };

    let report = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A2b);
    let val_elapsed = val_start.elapsed();

    if report.issues.is_empty() {
        println!("No issues found -- document is compliant.");
    } else {
        // Group issues by rule prefix (first two dotted segments, e.g. "6.1")
        let mut by_category: BTreeMap<String, Vec<&pdf_compliance::ComplianceIssue>> =
            BTreeMap::new();
        for issue in &report.issues {
            let cat = rule_category(&issue.rule);
            by_category.entry(cat).or_default().push(issue);
        }

        for (category, issues) in &by_category {
            println!("[{category}] -- {} issue(s)", issues.len());
            for issue in issues {
                let sev = match issue.severity {
                    pdf_compliance::Severity::Error => "ERROR",
                    pdf_compliance::Severity::Warning => "WARN ",
                    pdf_compliance::Severity::Info => "INFO ",
                };
                let loc = issue
                    .location
                    .as_deref()
                    .map(|l| format!(" @ {l}"))
                    .unwrap_or_default();
                println!("  [{sev}] {}: {}{loc}", issue.rule, issue.message);
            }
            println!();
        }
    }

    // --- Summary -----------------------------------------------------------
    println!("=== Summary ===");
    println!();
    println!("  Compliant: {}", report.compliant);
    println!("  Errors:    {}", report.error_count());
    println!("  Warnings:  {}", report.warning_count());
    println!(
        "  Info:      {}",
        report.issues.len() - report.error_count() - report.warning_count()
    );
    println!(
        "  Validation time: {:.1}ms",
        val_elapsed.as_secs_f64() * 1000.0
    );
    println!(
        "  Total time:      {:.1}ms",
        total_start.elapsed().as_secs_f64() * 1000.0
    );
    println!();
    println!("Output saved to: {out_path}");

    if !report.compliant {
        std::process::exit(2);
    }
}

fn print_step(n: usize, name: &str) {
    println!("  [{n:>2}] {name}");
}

fn print_elapsed(start: Instant) {
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    println!("       ({ms:.1}ms)");
}

/// Extract a category from a rule string by taking the first two dotted
/// segments (e.g. "6.1.2" -> "6.1", "6.2.4.3" -> "6.2").
fn rule_category(rule: &str) -> String {
    let mut parts = rule.splitn(3, '.');
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => format!("{a}.{b}"),
        _ => rule.to_string(),
    }
}
