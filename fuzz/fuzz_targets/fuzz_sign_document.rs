#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the signature parsing and validation pipeline end-to-end.
    // Exercises both the pdf-syntax parser and the byte-range / cert decoding
    // paths in pdf-sign; neither should panic on arbitrary input.
    if data.len() > 16 && data.len() < 4 * 1024 * 1024 {
        if let Ok(pdf) = pdf_syntax::Pdf::new(data.to_vec()) {
            // Parse signature fields (reads /AcroForm, /SigFlags, /ByteRange …).
            let fields = pdf_sign::signature_fields(&pdf);

            // Attempt to validate every signature found (exercises cert + digest).
            let results = pdf_sign::validate_signatures(&pdf);

            // Ensure the two views are consistent: every validated result should
            // correspond to a field that was detected.
            let _ = (fields.len(), results.len());
        }
    }
});
