//! Debug: check specific code widths in converted gen-* PDFs.


fn check_codes(doc: &lopdf::Document, font_name_needle: &str, codes: &[u32]) {
    for (id, obj) in &doc.objects {
        let lopdf::Object::Dictionary(dict) = obj else { continue };
        let base = dict.get(b"BaseFont").ok()
            .and_then(|o| if let lopdf::Object::Name(n) = o { Some(String::from_utf8_lossy(n).to_string()) } else { None })
            .unwrap_or_default();
        if !base.contains(font_name_needle) { continue }
        let Ok(lopdf::Object::Name(sub)) = dict.get(b"Subtype") else { continue };
        if !matches!(sub.as_slice(), b"TrueType" | b"Type1" | b"MMType1") { continue }

        let fc = dict.get(b"FirstChar").ok()
            .and_then(|o| if let lopdf::Object::Integer(i) = o { Some(*i as u32) } else { None })
            .unwrap_or(0);
        let lc = dict.get(b"LastChar").ok()
            .and_then(|o| if let lopdf::Object::Integer(i) = o { Some(*i as u32) } else { None })
            .unwrap_or(0);

        let widths = match dict.get(b"Widths").ok() {
            Some(lopdf::Object::Array(arr)) => Some(arr.clone()),
            Some(lopdf::Object::Reference(r)) => doc.get_object(*r).ok()
                .and_then(|o| if let lopdf::Object::Array(a) = o { Some(a.clone()) } else { None }),
            _ => None,
        };

        // Get encoding
        let enc = match dict.get(b"Encoding").ok() {
            Some(lopdf::Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
            Some(lopdf::Object::Dictionary(d)) => format!("Dict(base={})", d.get(b"BaseEncoding").ok()
                .and_then(|o| if let lopdf::Object::Name(n) = o { Some(String::from_utf8_lossy(n).to_string()) } else { None })
                .unwrap_or_default()),
            None => "(none)".to_string(),
            _ => "(other)".to_string(),
        };

        print!("  {:?} {} sub={} fc={fc} lc={lc} enc={enc}",
            id, base, String::from_utf8_lossy(sub));
        for &code in codes {
            let w = widths.as_ref().and_then(|w| {
                if code >= fc && (code - fc) < w.len() as u32 {
                    let idx = (code - fc) as usize;
                    Some(match &w[idx] {
                        lopdf::Object::Integer(i) => format!("{i}"),
                        lopdf::Object::Real(r) => format!("{r:.2}"),
                        _ => "?".into(),
                    })
                } else { None }
            }).unwrap_or("N/A".into());
            print!(" code{code}={w}");
        }
        println!();
    }
}

fn main() {
    let tests: &[(&str, &str, &[u32])] = &[
        ("/tmp/gen-154_154779-retest.pdf", "TimesNewRoman", &[160]),
        ("/tmp/gen-319_319905-retest.pdf", "TimesNewRoman", &[146]),
        ("/tmp/gen-322_322073-retest.pdf", "NewCenturySchlbk", &[39]),
        ("/tmp/gen-490_490200-retest.pdf", "CMR12", &[222, 223]),
        ("/tmp/gen-571-retest.pdf", "Times-Roman", &[160]),
        ("/tmp/gen-131_131159-retest.pdf", "AvantGarde", &[129]),
    ];

    for (path, needle, codes) in tests {
        println!("=== {} ({}) ===", path, needle);
        let data = std::fs::read(path).unwrap();
        let doc = lopdf::Document::load_mem(&data).unwrap();
        check_codes(&doc, needle, codes);
    }
}
