//! Diagnostic: simulate the text_replace test to understand failures.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
use pdf_manip::text_replace::replace_text;
use pdf_manip::text_run::FontMap;

fn extract_via_pdfengine(data: &[u8]) -> Option<String> {
    let doc = pdf_engine::PdfDocument::open(data.to_vec()).ok()?;
    doc.extract_text(0).ok()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: {} <pdf>", args[0]);
        std::process::exit(1);
    }
    let path = &args[1];
    let data = std::fs::read(path).expect("read");

    // Step 1: extract text via PDFium
    let text = extract_via_pdfengine(&data).unwrap_or_default();
    println!(
        "PDFium text (first 200): {:?}",
        &text[..text.len().min(200)]
    );

    let search_word = text
        .split_whitespace()
        .find(|w| w.len() >= 3 && w.chars().all(|c| c.is_alphanumeric()))
        .map(|w| w.to_string());
    let Some(word) = search_word else {
        println!("No suitable word found");
        return;
    };
    println!("Search word: {:?}", word);

    // Step 2: load via lopdf
    let mut doc = match lopdf::Document::load_mem(&data) {
        Ok(d) => d,
        Err(e) => {
            println!("lopdf load failed: {e}");
            return;
        }
    };

    // Step 3: build font map
    let fonts = match FontMap::from_page(&doc, 1) {
        Ok(f) => f,
        Err(e) => {
            println!("FontMap failed: {e}");
            return;
        }
    };
    println!("FontMap built OK");

    // Step 4: replace
    let replacement = "__XFA_REPLACED__";
    match replace_text(&mut doc, 1, &word, replacement, &fonts) {
        Ok(0) => println!("0 replacements (word not found in content stream)"),
        Ok(n) => {
            println!("Replaced {n}x");
            // Step 5: save and re-extract
            let mut saved = Vec::new();
            doc.save_to(&mut saved).expect("save");
            let new_text = extract_via_pdfengine(&saved).unwrap_or_default();
            println!(
                "After replacement (first 300): {:?}",
                &new_text[..new_text.len().min(300)]
            );
            if new_text.contains(replacement) {
                println!("✓ PASS: replacement found");
            } else {
                println!("✗ FAIL: replacement NOT found");
                // Show what's there instead of the word
                let _idx = new_text.find(&word[..3.min(word.len())]);
                println!(
                    "  Original word still present: {}",
                    new_text.contains(word.as_str())
                );
            }
        }
        Err(e) => println!("replace_text error (→ Skip): {e}"),
    }
}
