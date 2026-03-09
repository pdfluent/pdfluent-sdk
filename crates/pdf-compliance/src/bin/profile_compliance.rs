use std::time::Instant;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: profile_compliance <pdf>");
    let data = std::fs::read(&path).expect("cannot read file");
    println!("File: {} ({} bytes)", path, data.len());

    let t0 = Instant::now();
    let pdf = pdf_syntax::Pdf::new(data).expect("parse failed");
    println!("Parse: {:?}", t0.elapsed());

    let obj_count = pdf.objects().into_iter().count();
    println!("Objects: {}", obj_count);
    println!("Pages: {}", pdf.pages().iter().count());

    // Time the full PDF/A validation
    let level = pdf_compliance::detect_pdfa_level(&pdf).unwrap_or(pdf_compliance::PdfALevel::A1b);
    println!("Detected level: {:?}", level);

    let t1 = Instant::now();
    let report = pdf_compliance::validate_pdfa_timed(&pdf, level);
    let elapsed = t1.elapsed();
    println!("Validation: {:?}", elapsed);
    println!("Issues: {}", report.issues.len());
    println!("Compliant: {}", report.compliant);
}
