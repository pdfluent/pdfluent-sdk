use pdf_manip::text_replace::replace_text;
use pdf_manip::text_run::FontMap;

fn extract_text(data: &[u8]) -> Option<String> {
    let doc = pdf_engine::PdfDocument::open(data.to_vec()).ok()?;
    doc.extract_text(0).ok()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: diag_text_replace <pdf>");
    let data = std::fs::read(path).expect("read");

    let text = extract_text(&data).unwrap_or_default();
    println!(
        "PDFium text (first 200): {:?}",
        &text[..text.len().min(200)]
    );

    let word = match text
        .split_whitespace()
        .find(|w| w.len() >= 3 && w.chars().all(|c| c.is_alphanumeric()))
    {
        Some(w) => w.to_string(),
        None => {
            println!("No word found");
            return;
        }
    };
    println!("Search word: {:?}", word);

    let mut doc = match lopdf::Document::load_mem(&data) {
        Ok(d) => d,
        Err(e) => {
            println!("lopdf failed: {e}");
            return;
        }
    };

    let fonts = match FontMap::from_page(&doc, 1) {
        Ok(f) => f,
        Err(e) => {
            println!("FontMap failed: {e}");
            return;
        }
    };
    println!("FontMap OK. Font names: {:?}", "-fonts-");

    let replacement = "__XFA_REPLACED__";
    match replace_text(&mut doc, 1, &word, replacement, &fonts) {
        Ok(0) => println!("0 replacements"),
        Ok(n) => {
            println!("Replaced {n}x");
            let mut saved = Vec::new();
            doc.save_to(&mut saved).expect("save");
            let new_text = extract_text(&saved).unwrap_or_default();
            println!(
                "After (first 300): {:?}",
                &new_text[..new_text.len().min(300)]
            );
            if new_text.contains(replacement) {
                println!("✓ PASS");
            } else {
                println!("✗ FAIL: '{}' not found", replacement);
            }
        }
        Err(e) => println!("replace_text → Skip: {e}"),
    }
}
