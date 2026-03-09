#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the signing pipeline — primarily signature detection and parsing.
    // We don't perform actual signing (requires a cert), but we exercise
    // the code paths that parse existing signatures from fuzzed input.
    if data.len() > 16 && data.len() < 4 * 1024 * 1024 {
        if let Ok(pdf) = pdf_syntax::Pdf::new(data.to_vec()) {
            let _ = pdf_sign::signature_fields(&pdf);
            let _ = pdf_sign::validate_signatures(&pdf);
        }
    }
});
