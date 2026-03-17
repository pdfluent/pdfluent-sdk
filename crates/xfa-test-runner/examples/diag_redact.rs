use pdf_redact::search_redact::{search_and_redact, RedactSearchOptions};

fn main() {
    // Regression: existing tests
    test_redact("/tmp/MOZILLA-711366-1.pdf", "Certificate", "MOZILLA-711366-1.pdf");

    // Fix #466 open issue: MOZILLA-666767-3 — "Mozilla" survives page 1 after redaction.
    // Hypothesis: extract_positioned_chars misses one occurrence that pdf_engine finds.
    println!("\n--- MOZILLA-666767-3 deep diagnostic ---");
    diag_mozilla_666767();
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
                                "  page {page_num}: word still found (extract_positioned_chars)! context: {:?}",
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

fn diag_mozilla_666767() {
    let path = "/tmp/MOZILLA-666767-3.pdf";
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            println!("SKIP: {e}");
            return;
        }
    };
    let word = "Mozilla";

    // Step 1: Extract page 1 via pdf_engine (same as test verification).
    let engine_text = {
        let doc = pdf_engine::PdfDocument::open(data.clone()).expect("engine open");
        doc.extract_text(0).unwrap_or_default()
    };
    let engine_has = engine_text.to_lowercase().contains(&word.to_lowercase());
    println!("pdf_engine page 1: word present={engine_has}");
    if engine_has {
        if let Some(pos) = engine_text.to_lowercase().find(&word.to_lowercase()) {
            let start = pos.saturating_sub(30);
            let end = (pos + word.len() + 30).min(engine_text.len());
            println!("  context: {:?}", &engine_text[start..end]);
        }
    }

    // Step 2: Extract positioned chars from page 1 via pdf_extract.
    let doc = lopdf::Document::load_mem(&data).expect("lopdf open");
    let chars_p1 = pdf_extract::extract_positioned_chars(&doc, 1).unwrap_or_default();
    let extract_text: String = chars_p1.iter().map(|c| c.ch).collect();
    let extract_has = extract_text.to_lowercase().contains(&word.to_lowercase());
    println!(
        "pdf_extract page 1: {} chars, word present={}",
        chars_p1.len(),
        extract_has
    );
    if extract_has {
        if let Some(pos) = extract_text.to_lowercase().find(&word.to_lowercase()) {
            let start = pos.saturating_sub(30);
            let end = (pos + word.len() + 30).min(extract_text.len());
            println!("  context: {:?}", &extract_text[start..end]);
        }
    } else {
        // Show first 300 chars of what extract_positioned_chars found
        let preview: String = extract_text.chars().take(300).collect();
        println!("  extract_positioned_chars text preview: {:?}", preview);
    }

    // Step 3: Check pages 1..3 for total Mozilla count via pdf_extract.
    println!("\nAll-page Mozilla count via pdf_extract:");
    for page_num in 1..=3 {
        let chars = pdf_extract::extract_positioned_chars(&doc, page_num).unwrap_or_default();
        let t: String = chars.iter().map(|c| c.ch).collect();
        let count = t.to_lowercase().matches(&word.to_lowercase()).count();
        println!("  page {page_num}: {count} occurrences, {} chars total", chars.len());
    }

    // Step 4: Now redact and check.
    let mut doc2 = lopdf::Document::load_mem(&data).expect("lopdf open");
    let opts = RedactSearchOptions::default();
    let report = search_and_redact(&mut doc2, word, &opts).expect("redact");
    println!(
        "\nRedact: matches={} areas={} ops={}",
        report.matches_found, report.areas_redacted, report.operations_removed
    );

    let mut saved = Vec::new();
    doc2.save_to(&mut saved).expect("save");

    // Step 5: After redaction, check pdf_engine page 1.
    let engine_text2 = {
        let d = pdf_engine::PdfDocument::open(saved.clone()).expect("engine open2");
        d.extract_text(0).unwrap_or_default()
    };
    let engine_has2 = engine_text2.to_lowercase().contains(&word.to_lowercase());
    println!("After redact — pdf_engine page 1: word present={engine_has2}");
    if engine_has2 {
        if let Some(pos) = engine_text2.to_lowercase().find(&word.to_lowercase()) {
            let start = pos.saturating_sub(40);
            let end = (pos + word.len() + 40).min(engine_text2.len());
            println!("  context: {:?}", &engine_text2[start..end]);
        }
    }

    // Step 6: Check pdf_extract page 1 after redact.
    let doc3 = lopdf::Document::load_mem(&saved).expect("reload");
    let chars3 = pdf_extract::extract_positioned_chars(&doc3, 1).unwrap_or_default();
    let t3: String = chars3.iter().map(|c| c.ch).collect();
    let extract_has3 = t3.to_lowercase().contains(&word.to_lowercase());
    println!("After redact — pdf_extract page 1: word present={extract_has3}");
    if extract_has3 {
        if let Some(pos) = t3.to_lowercase().find(&word.to_lowercase()) {
            let start = pos.saturating_sub(40);
            let end = (pos + word.len() + 40).min(t3.len());
            println!("  context: {:?}", &t3[start..end]);
        }
    }
}
