#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the XFA data DOM XML parser.
    // XFA forms contain XML datasets — malformed XML should never panic.
    if let Ok(xml) = std::str::from_utf8(data) {
        let _ = xfa_dom_resolver::xfa_dom::XfaDom::from_data_xml(xml);
    }
});
