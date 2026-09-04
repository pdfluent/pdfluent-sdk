// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_invoice::validate_en16931;
use pdf_invoice::zugferd::ZugferdInvoice;
use std::fs;
use std::path::PathBuf;

fn default_paths() -> Vec<PathBuf> {
    [
        "/tmp/xrechnung-examples/xrechnung-testsuite-master/src/test/business-cases/standard/01.01a-INVOICE_uncefact.xml",
        "/tmp/xrechnung-examples/xrechnung-testsuite-master/src/test/business-cases/standard/02.01a-INVOICE_uncefact.xml",
        "/tmp/xrechnung-examples/xrechnung-testsuite-master/src/test/business-cases/standard/03.01a-INVOICE_uncefact.xml",
    ]
    .into_iter()
    .map(PathBuf::from)
    .filter(|path| path.exists())
    .collect()
}

fn collect_uncefact_xml(path: &PathBuf, output: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    if path.is_file() {
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_uncefact.xml") || name.ends_with(".xml"))
        {
            output.push(path.clone());
        }
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            collect_uncefact_xml(&child, output)?;
        } else if child
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_uncefact.xml"))
        {
            output.push(child);
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<PathBuf> = {
        let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
        if args.is_empty() {
            default_paths()
        } else {
            let mut expanded = Vec::new();
            for path in args {
                collect_uncefact_xml(&path, &mut expanded)?;
            }
            expanded.sort();
            expanded
        }
    };

    if paths.is_empty() {
        eprintln!("No XRechnung XML files provided and no default sample paths exist.");
        std::process::exit(1);
    }

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut parse_failed = 0usize;

    for path in &paths {
        let xml = std::fs::read_to_string(path)?;
        match ZugferdInvoice::from_xml(&xml) {
            Ok(invoice) => {
                let result = validate_en16931(&invoice);
                if result.failed.is_empty() {
                    passed += 1;
                    println!(
                        "PASS {} profile={:?} warnings={}",
                        path.display(),
                        invoice.profile,
                        result.warning_count()
                    );
                } else {
                    failed += 1;
                    let summary = result
                        .failed
                        .iter()
                        .take(3)
                        .map(|(rule, message)| format!("{rule}: {message}"))
                        .collect::<Vec<_>>()
                        .join(" | ");
                    println!(
                        "FAIL {} profile={:?} failed={} {}",
                        path.display(),
                        invoice.profile,
                        result.error_count(),
                        summary
                    );
                }
            }
            Err(error) => {
                parse_failed += 1;
                println!("PARSE_FAIL {} {error}", path.display());
            }
        }
    }

    println!(
        "Summary: total={} passed={} failed={} parse_failed={}",
        paths.len(),
        passed,
        failed,
        parse_failed
    );

    Ok(())
}
