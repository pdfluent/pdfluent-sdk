use pdf_syntax::object::dict::keys;
use pdf_syntax::object::{Dict, Name, Object, Stream};
use pdf_syntax::Pdf;
use std::env;

fn main() {
    let path = env::args().nth(1).expect("usage: debug_transparency <path>");
    let data = std::fs::read(&path).unwrap();
    let pdf = Pdf::new(data).unwrap();

    let level = pdf_compliance::detect_pdfa_level(&pdf);
    println!("Detected level: {level:?}");

    let has_oi = pdf_compliance::check::has_output_intent(&pdf);
    println!("Has OutputIntent: {has_oi}");

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        println!("\n=== Page {} ===", page_idx + 1);
        let page_dict = page.raw();

        // Check Group
        if let Some(group) = page_dict.get::<Dict<'_>>(b"Group" as &[u8]) {
            println!("  Has /Group dict");
            if let Some(s) = group.get::<Name>(keys::S) {
                println!("    S = {}", std::str::from_utf8(s.as_ref()).unwrap_or("?"));
            }
            if let Some(cs) = group.get::<Name>(keys::CS) {
                println!(
                    "    CS = {}",
                    std::str::from_utf8(cs.as_ref()).unwrap_or("?")
                );
            } else {
                println!("    No CS entry");
            }
        } else {
            println!("  No /Group");
        }

        // Check Resources via raw dict
        println!("  --- Raw dict Resources ---");
        if let Some(res) = page_dict.get::<Dict<'_>>(keys::RESOURCES) {
            println!("    Has /Resources (raw)");
            if let Some(gs) = res.get::<Dict<'_>>(keys::EXT_G_STATE) {
                println!("    Has /ExtGState with {} entries", gs.entries().count());
                for (name, _) in gs.entries() {
                    let nm = std::str::from_utf8(name.as_ref()).unwrap_or("?");
                    if let Some(gs_d) = gs.get::<Dict<'_>>(name.as_ref()) {
                        // Check CA/ca/BM/SMask
                        let has_ca = gs_d.get::<Object<'_>>(b"CA" as &[u8]);
                        let has_ca_lower = gs_d.get::<Object<'_>>(b"ca" as &[u8]);
                        let has_bm = gs_d.get::<Name>(keys::BM);
                        let has_smask = gs_d.get::<Object<'_>>(keys::SMASK);
                        println!("      {nm}: CA={has_ca:?} ca={has_ca_lower:?} BM={has_bm:?} SMask={}", has_smask.is_some());
                    } else {
                        println!("      {nm}: could not resolve to Dict");
                    }
                }
            } else {
                println!("    No /ExtGState (raw)");
            }
        } else {
            println!("    No /Resources in raw dict");
        }

        // Check Resources via page.resources()
        println!("  --- page.resources() ---");
        let res = page.resources();
        let gs_dict = &res.ext_g_states;
        println!(
            "    ext_g_states entries: {}",
            gs_dict.entries().count()
        );
        for (name, _) in gs_dict.entries() {
            let nm = std::str::from_utf8(name.as_ref()).unwrap_or("?");
            if let Some(gs_d) = gs_dict.get::<Dict<'_>>(name.as_ref()) {
                let has_ca = gs_d.get::<Object<'_>>(b"CA" as &[u8]);
                let has_ca_lower = gs_d.get::<Object<'_>>(b"ca" as &[u8]);
                let has_bm = gs_d.get::<Name>(keys::BM);
                let has_smask = gs_d.get::<Object<'_>>(keys::SMASK);
                println!("      {nm}: CA={has_ca:?} ca={has_ca_lower:?} BM={has_bm:?} SMask={}", has_smask.is_some());
            } else {
                println!("      {nm}: could not resolve to Dict");
            }
        }

        // Check XObjects
        let xobj_dict = &res.x_objects;
        for (name, _) in xobj_dict.entries() {
            let nm = std::str::from_utf8(name.as_ref()).unwrap_or("?");
            if let Some(stream) = xobj_dict.get::<Stream<'_>>(name.as_ref()) {
                let dict = stream.dict();
                let subtype = dict.get::<Name>(keys::SUBTYPE);
                let st = subtype
                    .as_ref()
                    .map(|s| std::str::from_utf8(s.as_ref()).unwrap_or("?"))
                    .unwrap_or("none");
                let has_smask = dict.contains_key(keys::SMASK);
                println!("    XObject {nm}: Subtype={st} SMask={has_smask}");
            }
        }
    }

    // Run actual validation
    if let Some(level) = level {
        let report = pdf_compliance::validate_pdfa(&pdf, level);
        println!("\n=== Compliance Report ===");
        for issue in &report.issues {
            if issue.severity == pdf_compliance::Severity::Error {
                println!(
                    "  [{}] {} ({})",
                    issue.rule,
                    issue.message,
                    issue.location.as_deref().unwrap_or("")
                );
            }
        }
    }
}
