#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the PDF/A compliance validator.
    // Arbitrary bytes should never cause a panic — only return Ok/Err.
    if data.len() > 16 && data.len() < 4 * 1024 * 1024 {
        if let Ok(pdf) = pdf_syntax::Pdf::new(data.to_vec()) {
            let level = pdf_compliance::detect_pdfa_level(&pdf)
                .unwrap_or(pdf_compliance::PdfALevel::A2b);
            let _ = pdf_compliance::validate_pdfa(&pdf, level);
        }
    }
});
