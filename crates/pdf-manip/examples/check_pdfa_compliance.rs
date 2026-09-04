//! Quick compliance check for a converted PDF

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: check_pdfa_compliance <pdf>");
        std::process::exit(1);
    }
    let pdf_data = std::fs::read(&args[1]).expect("read failed");
    let pdf = match pdf_syntax::Pdf::new(pdf_data) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("parse failed: {:?}", e);
            std::process::exit(1);
        }
    };
    let report = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A2b);
    println!("compliant: {}", report.compliant);
    println!("issues: {}", report.issues.len());
    let limit = if args.len() >= 3 {
        args[2].parse::<usize>().unwrap_or(30)
    } else {
        30
    };
    for issue in report.issues.iter().take(limit) {
        println!("  [{:?}] {}: {}", issue.severity, issue.rule, issue.message);
    }
}
