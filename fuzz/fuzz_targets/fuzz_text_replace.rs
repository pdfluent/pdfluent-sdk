#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz text replacement: load PDF, replace a fixed term.
    if data.len() > 16 && data.len() < 4 * 1024 * 1024 {
        if let Ok(mut doc) = lopdf::Document::load_mem(data) {
            let _ = pdf_manip::text_replace::replace_text_all_pages(
                &mut doc,
                "the",
                "REPLACED",
            );
        }
    }
});
