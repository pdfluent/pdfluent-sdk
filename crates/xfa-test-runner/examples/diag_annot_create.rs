fn main() {
    let pdf = std::fs::read("/tmp/MOZILLA-666767-3.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&pdf).unwrap();

    let editor = pdf_manip::content_editor::editor_for_page(&doc, 1).unwrap();
    let fonts = pdf_manip::text_run::FontMap::from_page(&doc, 1).unwrap();
    let runs = pdf_manip::text_run::extract_text_runs(&editor, &fonts);
    let ops = editor.operations();

    // Find run #66 "Community Members" and show ops around it
    let comm_run = runs
        .iter()
        .find(|r| r.text.starts_with("Community Members"))
        .unwrap();
    println!(
        "'Community Members' run: ops={:?} x={:.2} y={:.2}",
        comm_run.ops_range, comm_run.x, comm_run.y
    );

    let start = comm_run.ops_range.start.saturating_sub(8);
    let end = (comm_run.ops_range.end + 5).min(ops.len());
    println!("Ops [{start}..{end}]:");
    for i in start..end {
        let op = &ops[i];
        // Show first 80 chars of operand debug
        let ops_str = format!("{:?}", op.operands.iter().take(2).collect::<Vec<_>>());
        println!(
            "  op[{}]: {} {}",
            i,
            op.operator,
            &ops_str[..ops_str.len().min(80)]
        );
    }

    // Also show: which ops are removed by text-matching (without my fix)
    // Simulate: what text-matched ops are there near y=569?
    let text_matched_runs: Vec<_> = runs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.text.contains("Mozilla") && (r.y - 569.0).abs() < 5.0)
        .collect();
    println!("\nText-matched runs near y=569:");
    for (i, r) in &text_matched_runs {
        println!(
            "  run[{}] ops={:?} x={:.2} y={:.2} text={:?}",
            i,
            r.ops_range,
            r.x,
            r.y,
            &r.text[..r.text.len().min(60)]
        );
    }

    // Show ALL runs between y=566 and y=572
    println!("\nAll runs y=[566,572]:");
    for (i, run) in runs.iter().enumerate() {
        if run.y >= 566.0 && run.y <= 572.0 {
            println!(
                "  run[{}] ops={:?} x={:.2} y={:.2} text={:?}",
                i,
                run.ops_range,
                run.x,
                run.y,
                &run.text[..run.text.len().min(60)]
            );
        }
    }
}
