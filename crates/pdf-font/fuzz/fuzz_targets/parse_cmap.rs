#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Must never panic regardless of input. #463 / crash resistance audit.
    // Parse as CMap with no predefined-CMap resolver (None = standalone only).
    let _ = pdf_font::cmap::CMap::parse(data, |_name| None);
});
