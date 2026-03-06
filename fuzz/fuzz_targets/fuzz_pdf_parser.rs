#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the main PDF parser entry point.
    // Exercises xref parsing, object resolution, page enumeration,
    // and stream decoding — all critical attack surface.
    if let Ok(pdf) = pdf_syntax::Pdf::new(data.to_vec()) {
        let pages = pdf.pages();
        for page in pages.iter() {
            for _op in page.typed_operations() {}
        }
    }
});
