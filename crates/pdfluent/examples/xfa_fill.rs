//! Fill an XFA form: enumerate fields, set values, save.
//!
//! ```bash
//! cargo run -p pdfluent --example xfa_fill -- form.pdf
//! cargo run -p pdfluent --example xfa_fill -- form.pdf \
//!     --set "form1.applicant.name=Alice" --out filled.pdf
//! ```

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;
use pdfluent::OpenOptions;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: xfa_fill <pdf> [--set name=value]... [--out out.pdf]");
        std::process::exit(2);
    };
    let mut sets: Vec<(String, String)> = Vec::new();
    let mut out_path: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--set" => {
                let kv = args.next().expect("--set needs name=value");
                let (k, v) = kv.split_once('=').expect("--set needs name=value");
                sets.push((k.to_string(), v.to_string()));
            }
            "--out" => out_path = Some(args.next().expect("--out needs a path")),
            other => panic!("unknown arg: {other}"),
        }
    }

    let mut doc = PdfDocument::open_with(&path, OpenOptions::new())?;

    if !doc.has_xfa_form() {
        eprintln!("{path}: not an XFA form");
        std::process::exit(1);
    }

    let model = doc.xfa_form_model()?;
    println!("XFA layout pages: {}", model.page_count);
    println!("fields: {}", model.fields.len());
    for f in model.fields.iter().take(20) {
        println!(
            "  {} ({:?}{}{}) = {:?}",
            f.name,
            f.field_type,
            if f.read_only { ", read-only" } else { "" },
            if f.multiline { ", multiline" } else { "" },
            f.value,
        );
    }
    if model.fields.len() > 20 {
        println!("  … {} more", model.fields.len() - 20);
    }

    for (name, value) in &sets {
        let outcome = doc.set_xfa_field_value(name, XfaFieldValue::Text(value))?;
        println!(
            "set {name} = {:?} (persisted to datasets: {})",
            outcome.raw_value, outcome.persisted_to_datasets
        );
    }

    if let Some(out) = out_path {
        doc.save(&out)?;
        println!("saved {out}");
    }
    Ok(())
}
