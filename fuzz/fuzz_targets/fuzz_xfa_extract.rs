#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the text and XFA content extraction pipeline.
    // Exercises lopdf parsing + text extraction; malformed PDFs must not panic.
    if data.len() > 16 && data.len() < 4 * 1024 * 1024 {
        let Ok(doc) = lopdf::Document::load_mem(data) else {
            return;
        };

        // Extract all text blocks — exercises CMap, font, and encoding paths.
        let _ = pdf_extract::extract_text(&doc);

        // Try to find and parse XFA streams from AcroForm.
        // XFA is stored as an array [name, stream-ref, ...] under /AcroForm /XFA.
        let Ok(catalog) = doc.catalog() else { return };
        let Ok(acroform_ref) = catalog.get(b"AcroForm").and_then(|v| v.as_reference()) else {
            return;
        };
        let Ok(acroform) = doc.get_dictionary(acroform_ref) else {
            return;
        };
        let Ok(xfa_arr) = acroform.get(b"XFA").and_then(|v| v.as_array()) else {
            return;
        };

        // Elements alternate: [name-str, stream-ref, name-str, stream-ref, ...]
        let mut take_next = false;
        for item in xfa_arr {
            if take_next {
                // Odd elements are stream object references; decode and try XML parse.
                if let Ok(stream_id) = item.as_reference() {
                    if let Ok(stream_obj) = doc.get_object(stream_id) {
                        if let Ok(stream) = stream_obj.as_stream() {
                            if let Ok(content) = stream.decompressed_content() {
                                if let Ok(xml) = std::str::from_utf8(&content) {
                                    let _ = xfa_dom_resolver::data_dom::DataDom::from_xml(xml);
                                }
                            }
                        }
                    }
                }
                take_next = false;
            } else {
                take_next = true; // Even elements are name strings; skip.
            }
        }
    }
});
