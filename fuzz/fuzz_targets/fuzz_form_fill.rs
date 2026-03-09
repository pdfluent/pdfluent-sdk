#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz form field reading and value setting via pdf-forms.
    if data.len() > 16 && data.len() < 4 * 1024 * 1024 {
        if let Ok(pdf) = pdf_syntax::Pdf::new(data.to_vec()) {
            if let Some(mut tree) = pdf_forms::parse_acroform(&pdf) {
                use pdf_forms::FormAccess;
                let names = tree.field_names();
                for name in names.iter().take(5) {
                    let _ = tree.get_value(name);
                    let _ = tree.set_value(name, "fuzz_value");
                }
            }
        }
    }
});
