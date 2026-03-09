#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the PDF/A conversion pipeline.
    if data.len() > 16 && data.len() < 4 * 1024 * 1024 {
        if let Ok(mut doc) = lopdf::Document::load_mem(data) {
            let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
            let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(
                &mut doc,
                pdf_manip::pdfa_xmp::PdfAConformance::A2b,
                None,
            );
            let mut out = Vec::new();
            let _ = doc.save_to(&mut out);
        }
    }
});
