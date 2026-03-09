#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz ContentEditor roundtrip: decode → encode → decode.
    if let Ok(editor) = pdf_manip::ContentEditor::from_stream(data) {
        if let Ok(encoded) = editor.encode() {
            let _ = pdf_manip::ContentEditor::from_stream(&encoded);
        }
    }
});
