#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the SOM (Scripting Object Model) path parser.
    // SOM paths like "form1.page1.field[2]" are parsed from user-supplied data.
    if let Ok(path) = std::str::from_utf8(data) {
        let _ = xfa_dom_resolver::som::parse_som(path);

        // Also test path resolution against a minimal DOM
        let xml = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
            <xfa:data><form><field>v</field></form></xfa:data>
        </xfa:datasets>"#;
        if let Ok(dom) = xfa_dom_resolver::data_dom::DataDom::from_xml(xml) {
            let _ = xfa_dom_resolver::som::resolve_data_path(&dom, path, dom.root());
        }
    }
});
