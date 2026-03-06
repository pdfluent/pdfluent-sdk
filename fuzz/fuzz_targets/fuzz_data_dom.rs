#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the DataDom XML parser and tree operations.
    if let Ok(xml) = std::str::from_utf8(data) {
        if let Ok(dom) = xfa_dom_resolver::data_dom::DataDom::from_xml(xml) {
            // Exercise tree traversal
            let _ = dom.to_xml();
            let _ = dom.len();

            if let Some(root) = dom.root() {
                let _ = dom.get(root);
                let _ = dom.children(root);
            }
        }
    }
});
