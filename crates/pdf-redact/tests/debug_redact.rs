use pdf_redact::search_redact::{search_and_redact, RedactSearchOptions};

#[test]
#[ignore]
fn debug_redact_137208_united() {
    let data = std::fs::read("/tmp/redact-test-137.pdf").unwrap();
    let word = "UNITED";

    // Step 1: what does extract_positioned_chars return for page 1?
    let doc_lopdf = lopdf::Document::load_mem(&data).unwrap();
    let chars = pdf_extract::extract_positioned_chars(&doc_lopdf, 1).unwrap();
    let pos_text: String = chars.iter().map(|c| c.ch).collect();
    println!(
        "Positioned text (first 200): {:?}",
        &pos_text[..200.min(pos_text.len())]
    );
    let pos_count = pos_text.matches(word).count();
    println!(
        "'{}' in positioned text (page 1): {} occurrences",
        word, pos_count
    );

    // Step 2: what pages have this word via positioned_chars?
    let page_count = doc_lopdf.get_pages().len();
    println!("Total pages: {}", page_count);
    for p in 1..=page_count.min(3) as u32 {
        if let Ok(chars) = pdf_extract::extract_positioned_chars(&doc_lopdf, p) {
            let t: String = chars.iter().map(|c| c.ch).collect();
            let n = t.matches(word).count();
            if n > 0 {
                println!("  Page {}: {} occurrences via positioned_chars", p, n);
            }
        }
    }

    // Step 3: run search_and_redact (all pages)
    let mut doc2 = lopdf::Document::load_mem(&data).unwrap();
    let opts = RedactSearchOptions::default();
    let report = search_and_redact(&mut doc2, word, &opts).unwrap();
    println!(
        "search_and_redact: areas_redacted={} ops_removed={}",
        report.areas_redacted, report.operations_removed
    );

    let mut saved = Vec::new();
    doc2.save_to(&mut saved).unwrap();
    let doc3 = lopdf::Document::load_mem(&saved).unwrap();
    let chars_after = pdf_extract::extract_positioned_chars(&doc3, 1).unwrap();
    let text_after: String = chars_after.iter().map(|c| c.ch).collect();
    let remaining = text_after.matches(word).count();
    println!(
        "After redact: '{}' on page 1 via positioned_chars: {} occurrences",
        word, remaining
    );
}

#[test]
#[ignore]
fn debug_redact_gen419_hydrate() {
    let data = std::fs::read("/tmp/gen-419.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    let chars_before = pdf_extract::extract_positioned_chars(&doc, 1).unwrap();
    let text_before: String = chars_before.iter().map(|c| c.ch).collect();
    println!(
        "Before: {} 'Hydrate' occurrences",
        text_before.matches("Hydrate").count()
    );

    let opts = RedactSearchOptions::exact("Hydrate");
    let report = search_and_redact(&mut doc, "Hydrate", &opts).unwrap();
    println!(
        "areas_redacted={} ops_removed={}",
        report.areas_redacted, report.operations_removed
    );

    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    let doc2 = lopdf::Document::load_mem(&saved).unwrap();
    let chars_after = pdf_extract::extract_positioned_chars(&doc2, 1).unwrap();
    let text_after: String = chars_after.iter().map(|c| c.ch).collect();
    let remaining = text_after.matches("Hydrate").count();
    println!("After: {} 'Hydrate' occurrences remain", remaining);
    assert_eq!(remaining, 0, "Hydrate should be fully redacted");
}

#[test]
#[ignore]
fn debug_redact_gen881_are() {
    let data = std::fs::read("/tmp/gen-881.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    // Check initial "are" count
    let chars_before = pdf_extract::extract_positioned_chars(&doc, 1).unwrap();
    let text_before: String = chars_before.iter().map(|c| c.ch).collect();
    println!(
        "Before: {} 'are' occurrences",
        text_before.matches("are").count()
    );
    // Show first 5 contexts
    for (i, (pos, _)) in text_before.match_indices("are").enumerate().take(5) {
        let s = pos.saturating_sub(8);
        let e = (pos + 12).min(text_before.len());
        println!("  before[{i}]: {:?}", &text_before[s..e]);
    }

    let opts = RedactSearchOptions::exact("are");
    let report = search_and_redact(&mut doc, "are", &opts).unwrap();
    println!(
        "areas_redacted={} ops_removed={}",
        report.areas_redacted, report.operations_removed
    );

    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();

    let doc2 = lopdf::Document::load_mem(&saved).unwrap();
    let chars_after = pdf_extract::extract_positioned_chars(&doc2, 1).unwrap();
    let text_after: String = chars_after.iter().map(|c| c.ch).collect();
    let remaining: Vec<_> = text_after.match_indices("are").collect();
    println!("After: {} 'are' occurrences remain", remaining.len());
    for (i, (pos, _)) in remaining.iter().enumerate().take(5) {
        let s = pos.saturating_sub(8);
        let e = (pos + 12).min(text_after.len());
        println!("  after[{i}]: {:?}", &text_after[s..e]);
    }
}

#[test]
#[ignore]
fn debug_redact_r3_501_are() {
    let data = std::fs::read("/tmp/r3-501.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();
    let chars_before = pdf_extract::extract_positioned_chars(&doc, 1).unwrap();
    let text_before: String = chars_before.iter().map(|c| c.ch).collect();
    println!(
        "Page 1 text (first 200): {:?}",
        &text_before[..200.min(text_before.len())]
    );
    println!(
        "Before: {} 'Are' occurrences",
        text_before.matches("Are").count()
    );
}

#[test]
#[ignore]
fn debug_redact_r3_501_full() {
    use pdf_redact::search_redact::{search_and_redact, RedactSearchOptions};

    let data = std::fs::read("/tmp/r3-501.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    let chars_before = pdf_extract::extract_positioned_chars(&doc, 1).unwrap();
    let text_before: String = chars_before.iter().map(|c| c.ch).collect();
    let are_before: Vec<usize> = text_before.match_indices("Are").map(|(i, _)| i).collect();
    println!("Before: {} 'Are' occurrences", are_before.len());
    for pos in &are_before {
        let s = pos.saturating_sub(10);
        let e = (pos + 20).min(text_before.len());
        println!("  {:?}", &text_before[s..e]);
    }

    let opts = RedactSearchOptions::exact("Are");
    let report = search_and_redact(&mut doc, "Are", &opts).unwrap();
    println!(
        "areas_redacted={} ops_removed={}",
        report.areas_redacted, report.operations_removed
    );

    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    let doc2 = lopdf::Document::load_mem(&saved).unwrap();
    let chars_after = pdf_extract::extract_positioned_chars(&doc2, 1).unwrap();
    let text_after: String = chars_after.iter().map(|c| c.ch).collect();
    let are_after: Vec<usize> = text_after.match_indices("Are").map(|(i, _)| i).collect();
    println!("After: {} 'Are' occurrences remain", are_after.len());
    for pos in &are_after {
        let s = pos.saturating_sub(10);
        let e = (pos + 25).min(text_after.len());
        println!("  {:?}", &text_after[s..e]);
    }
    // Check page count to see if "Are" might be on a different page
    let pages = doc2.get_pages();
    println!("Total pages in redacted PDF: {}", pages.len());
    for page_num in pages.keys() {
        if let Ok(chars) = pdf_extract::extract_positioned_chars(&doc2, *page_num) {
            let t: String = chars.iter().map(|c| c.ch).collect();
            let count = t.matches("Are").count();
            if count > 0 {
                println!("  Page {}: {} 'Are' occurrences", page_num, count);
            }
        }
    }
}

#[test]
#[ignore]
fn debug_r3_501_page_structure() {
    use lopdf::Object;
    let data = std::fs::read("/tmp/r3-501.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();
    let pages = doc.get_pages();
    let (_, page_id) = pages.iter().find(|(n, _)| **n == 1).unwrap();
    let page_obj = doc.get_object(*page_id).unwrap();
    println!("Page 1 obj: {:?}", page_id);
    if let Object::Dictionary(d) = page_obj {
        // Check Contents
        if let Ok(contents) = d.get(b"Contents") {
            println!("Contents: {:?}", contents);
        }
        // Check Resources
        if let Ok(Object::Dictionary(res)) = d.get(b"Resources") {
            if let Ok(Object::Dictionary(xobjs)) = res.get(b"XObject") {
                println!("XObjects: {} entries", xobjs.len());
                for (k, v) in xobjs.iter() {
                    println!("  XObject {:?}: {:?}", String::from_utf8_lossy(k), v);
                }
            }
        }
    }

    // Check if "Are" appears in the raw content stream bytes
    let page_stream = doc.get_page_content(*page_id).unwrap();
    let text_preview: Vec<u8> = page_stream
        .iter()
        .copied()
        .filter(|b| *b >= 32 && *b < 127)
        .take(500)
        .collect();
    let s = String::from_utf8_lossy(&text_preview);
    // Find "Are" in the raw stream
    if s.contains("Are") {
        println!("'Are' found in raw content stream");
        // Find context
        for (i, _) in s.match_indices("Are") {
            let start = i.saturating_sub(20);
            let end = (i + 30).min(s.len());
            println!("  raw context: {:?}", &s[start..end]);
        }
    } else {
        println!("'Are' NOT in raw content stream — must be encoded");
    }
}

#[test]
#[ignore]
fn debug_r3_501_text_runs_vs_positioned() {
    use pdf_manip::content_editor::editor_for_page;
    use pdf_manip::text_run::{extract_text_runs, FontMap};

    let data = std::fs::read("/tmp/r3-501.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    // Get positioned chars with "Are" positions
    let chars = pdf_extract::extract_positioned_chars(&doc, 1).unwrap();
    let text: String = chars.iter().map(|c| c.ch).collect();
    println!("Text length: {}", text.len());
    for (pos, _) in text.match_indices("Are") {
        let _ch_start = pos; // byte offset in text
                             // Find which char_index this corresponds to
        let char_idx = text[..pos].chars().count();
        if char_idx + 3 <= chars.len() {
            let a = &chars[char_idx];
            let r = &chars[char_idx + 1];
            let e = &chars[char_idx + 2];
            println!(
                "'Are' positioned chars: A=({:.1},{:.1}) R=({:.1},{:.1}) E=({:.1},{:.1})",
                a.bbox[0], a.bbox[1], r.bbox[0], r.bbox[1], e.bbox[0], e.bbox[1]
            );
        }
    }

    // Get text runs
    let editor = editor_for_page(&doc, 1).unwrap();
    let fonts = FontMap::from_page(&doc, 1).unwrap();
    let runs = extract_text_runs(&editor, &fonts);
    println!("Total text runs: {}", runs.len());
    // Print runs that contain "Are", are short, OR are near y≈59.2 (second occurrence)
    for (i, run) in runs.iter().enumerate() {
        if run.text.contains("Are") || run.text.len() < 20 || (run.y - 59.2).abs() < 10.0 {
            println!(
                "  run[{i}] y={:.1} x={:.1} w={:.1} text={:?} ops={:?}",
                run.y,
                run.x,
                run.width,
                &run.text[..30.min(run.text.len())],
                run.ops_range
            );
        }
    }
    println!("--- CTM at second Are position ---");
    println!("positioned_chars 2nd Are y=59.2 → after CTM fix, what is run.y?");
}
