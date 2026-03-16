#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Must never panic regardless of input. #463 / crash resistance audit.
    let _ = pdf_syntax::Pdf::new(data.to_vec());
});
