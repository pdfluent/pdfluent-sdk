//! Quick compliance check for a converted PDF
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
