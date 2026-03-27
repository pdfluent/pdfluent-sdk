fn main() {
    for path in [
        "/tmp/mg_741.pdf",
        "/tmp/mg_875.pdf",
        "/tmp/mg_pdfbox686.pdf",
    ] {
        println!("\n=== {} ===", path);
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                println!("read error: {e}");
                continue;
            }
        };
        let doc = match lopdf::Document::load_mem(&data) {
            Ok(d) => d,
            Err(e) => {
                println!("lopdf load failed: {e}");
                continue;
            }
        };
        let original_pages = doc.get_pages().len();
        println!("Original pages: {original_pages}");

        let doc_clone = doc.clone();
        match pdf_manip::pages::merge_documents(&[doc_clone, doc]) {
            Ok(merged) => {
                let merged_pages = merged.get_pages().len();
                let expected = original_pages * 2;
                if merged_pages != expected {
                    println!("FAIL: merged={merged_pages}, expected={expected}");
                } else {
                    println!("OK: merged={merged_pages}");
                }
            }
            Err(e) => println!("merge failed: {e}"),
        }
    }
}
