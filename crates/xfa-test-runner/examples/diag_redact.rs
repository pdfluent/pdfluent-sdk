use pdf_redact::search_redact::{search_and_redact, RedactSearchOptions};

fn main() {
    println!("=== MOZILLA-666767-3 redact diagnostic ===\n");
    diag_mozilla_666767();
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

    // -----------------------------------------------------------------------
    // F. All Mozilla positions in original (extract_positioned_chars)
    // -----------------------------------------------------------------------
    println!("--- F. Mozilla positions in original (page 1) ---");
    {
        let doc = lopdf::Document::load_mem(&data).expect("lopdf");
        let chars = pdf_extract::extract_positioned_chars(&doc, 1).unwrap_or_default();
        let mut count = 0;
        let mut i = 0;
        while i < chars.len() {
            if chars[i].ch == 'M' && i + 7 <= chars.len() {
                let slice: String = chars[i..i + 7].iter().map(|c| c.ch).collect();
                if slice == word {
                    let bbox = [
                        chars[i].bbox[0],
                        chars[i].bbox[1],
                        chars[i + 6].bbox[2],
                        chars[i + 6].bbox[3],
                    ];
                    let ctx_start = i.saturating_sub(15);
                    let ctx_end = (i + 20).min(chars.len());
                    let ctx: String = chars[ctx_start..ctx_end].iter().map(|c| c.ch).collect();
                    println!(
                        "  #{count} x={:.1} y={:.1} bbox={:.1?} ctx={:?}",
                        chars[i].bbox[0],
                        chars[i].bbox[1],
                        bbox,
                        ctx
                    );
                    count += 1;
                }
            }
            i += 1;
        }
        println!("  Total Mozilla occurrences: {count}");
    }

    // -----------------------------------------------------------------------
    // G. Surviving Mozilla positions after redaction
    // -----------------------------------------------------------------------
    println!("\n--- G. Surviving Mozilla positions in redacted file ---");
    {
        let mut doc2 = lopdf::Document::load_mem(&data).expect("lopdf2");
        let opts = RedactSearchOptions::default();
        let report = search_and_redact(&mut doc2, word, &opts).expect("redact");
        println!("  ops_removed={}", report.operations_removed);

        let mut saved = Vec::new();
        doc2.save_to(&mut saved).expect("save");
        let doc3 = lopdf::Document::load_mem(&saved).expect("reload");
        let chars = pdf_extract::extract_positioned_chars(&doc3, 1).unwrap_or_default();

        let mut i = 0;
        let mut surviving = 0;
        while i < chars.len() {
            if chars[i].ch == 'M' && i + 7 <= chars.len() {
                let slice: String = chars[i..i + 7].iter().map(|c| c.ch).collect();
                if slice == word {
                    let ctx_start = i.saturating_sub(15);
                    let ctx_end = (i + 20).min(chars.len());
                    let ctx: String = chars[ctx_start..ctx_end].iter().map(|c| c.ch).collect();
                    println!(
                        "  SURVIVING #{surviving} x={:.1} y={:.1} ctx={:?}",
                        chars[i].bbox[0],
                        chars[i].bbox[1],
                        ctx
                    );
                    for j in 0..7 {
                        println!(
                            "    ch[{j}]='{}' x={:.2} y={:.2}",
                            chars[i + j].ch,
                            chars[i + j].bbox[0],
                            chars[i + j].bbox[1]
                        );
                    }
                    surviving += 1;
                }
            }
            i += 1;
        }
        if surviving == 0 {
            println!("  None — redaction complete!");
        }
    }

    // -----------------------------------------------------------------------
    // H. Show extract_text_runs positions — runs containing Mozilla-ish text
    // -----------------------------------------------------------------------
    println!("\n--- H. extract_text_runs containing Mozilla-ish text ---");
    {
        let doc = lopdf::Document::load_mem(&data).expect("lopdf");
        let editor = pdf_manip::content_editor::editor_for_page(&doc, 1).expect("editor");
        let fonts = pdf_manip::text_run::FontMap::from_page(&doc, 1).expect("fonts");
        let runs = pdf_manip::text_run::extract_text_runs(&editor, &fonts);
        println!("  Total text runs on page 1: {}", runs.len());
        // Show ALL runs near y=564 (surviving Mozilla y-position)
        println!("  --- All runs between y=560 and y=570 ---");
        for run in &runs {
            if run.y >= 560.0 && run.y <= 570.0 {
                let text = &run.text;
                println!(
                    "  run x={:.2} y={:.2} w={:.2} ops={:?} text={:?}",
                    run.x,
                    run.y,
                    run.width,
                    run.ops_range,
                    &text[..text.len().min(60)]
                );
            }
        }
    }
}
