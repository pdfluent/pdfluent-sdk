#![no_main]

use libfuzzer_sys::fuzz_target;
use pdf_redact::search_redact::{search_and_redact, RedactSearchOptions};

fuzz_target!(|data: &[u8]| {
    // Fuzz search-and-redact: load PDF, redact a fixed term.
    if data.len() > 16 && data.len() < 4 * 1024 * 1024 {
        if let Ok(mut doc) = lopdf::Document::load_mem(data) {
            let opts = RedactSearchOptions::exact("test");
            let _ = search_and_redact(&mut doc, "test", &opts);
        }
    }
});
