#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz lopdf load → save → load roundtrip.
    if data.len() > 4 && data.len() < 4 * 1024 * 1024 {
        if let Ok(mut doc) = lopdf::Document::load_mem(data) {
            let mut saved = Vec::new();
            if doc.save_to(&mut saved).is_ok() {
                let _ = lopdf::Document::load_mem(&saved);
            }
        }
    }
});
