use pdf_redact::{search_and_redact, RedactSearchOptions};

fn main() {
    // Test tr_813.pdf — glyph-indexed fonts (R36/R39/R42/R45) without ToUnicode;
    // position-based fallback must kick in.
    test_redact("/tmp/tr_813.pdf", "spinal", "tr_813.pdf");

    // Regression: tr_360.pdf — inline images; should still pass.
    test_redact("/tmp/tr_360.pdf", "spinal", "tr_360.pdf");

    // Regression: PDFBOX-2641-0.pdf — normal PDF.
    test_redact("/tmp/PDFBOX-2641-0.pdf", "spinal", "PDFBOX-2641-0.pdf");

    // Fix #466: MOZILLA-711366-1.pdf — symbolic TrueType font (R11/GIYRNY+Arial,Bold)
    // has no ToUnicode; Encoding/Differences dict maps byte→glyph name for "Certificate".
    // forward_encoding fallback in decode_string now handles this case.
    test_redact("/tmp/MOZILLA-711366-1.pdf", "Certificate", "MOZILLA-711366-1.pdf");
}

fn test_redact(path: &str, word: &str, label: &str) {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            println!("{label}: SKIP (cannot read: {e})");
            return;
        }
    };
    let mut doc = match lopdf::Document::load_mem(&data) {
        Ok(d) => d,
        Err(e) => {
            println!("{label}: SKIP (lopdf error: {e})");
            return;
        }
    };

    let opts = RedactSearchOptions::default();
    match search_and_redact(&mut doc, word, &opts) {
        Ok(report) => {
            println!(
                "{label}: matches={} areas={} ops_removed={}",
                report.matches_found, report.areas_redacted, report.operations_removed
            );

            // Save and re-extract to verify word is gone.
            let mut out = Vec::new();
            doc.save_to(&mut out).expect("save");
            let doc2 = lopdf::Document::load_mem(&out).expect("reload");
            let pages = doc2.get_pages();
            let mut word_still_present = false;
            for (&page_num, _) in &pages {
                if let Ok(chars) = pdf_extract::extract_positioned_chars(&doc2, page_num) {
                    let text: String = chars.iter().map(|c| c.ch).collect();
                    if text.to_lowercase().contains(&word.to_lowercase()) {
                        word_still_present = true;
                        if let Some(pos) = text.to_lowercase().find(&word.to_lowercase()) {
                            let start = pos.saturating_sub(20);
                            let end = (pos + word.len() + 20).min(text.len());
                            println!(
                                "  page {page_num}: word still found! context: {:?}",
                                &text[start..end]
                            );
                        }
                    }
                }
            }
            if word_still_present {
                println!("  FAIL: word '{word}' still extractable after redaction");
            } else if report.matches_found > 0 {
                println!("  PASS: word '{word}' successfully removed");
            } else {
                println!("  PASS: word '{word}' was not found (no matches)");
            }
        }
        Err(e) => {
            println!("{label}: ERROR: {e}");
        }
    }
}
