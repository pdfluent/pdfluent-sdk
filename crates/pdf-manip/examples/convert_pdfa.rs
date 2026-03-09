//! Convert a PDF to PDF/A and save the output.
//! Usage: cargo run -p pdf-manip --example convert_pdfa -- input.pdf output.pdf

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <input.pdf> <output.pdf>", args[0]);
        std::process::exit(1);
    }

    let data = std::fs::read(&args[1]).expect("read input");
    let mut doc = lopdf::Document::load_mem(&data).expect("load");

    // Run conversion pipeline
    match pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false) {
        Ok(r) => eprintln!(
            "Cleanup: js_removed={}, ap_fixes={}",
            r.js_actions_removed, r.ap_fixes
        ),
        Err(e) => eprintln!("Cleanup error: {e}"),
    }

    match pdf_manip::pdfa_fonts::embed_fonts(&mut doc) {
        Ok(r) => eprintln!(
            "Fonts: embedded={}, failed={}",
            r.fonts_embedded,
            r.failed.len()
        ),
        Err(e) => eprintln!("Font error: {e}"),
    }

    match pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc) {
        Ok(r) => eprintln!(
            "Colorspace: had_intent={}, added={}, device_cs={:?}",
            r.had_output_intent, r.output_intent_added, r.device_colorspaces_found
        ),
        Err(e) => eprintln!("Colorspace error: {e}"),
    }

    match pdf_manip::pdfa_xmp::repair_xmp_metadata(
        &mut doc,
        pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        None,
    ) {
        Ok(_) => eprintln!("XMP: repaired"),
        Err(e) => eprintln!("XMP error: {e}"),
    }

    // Save
    let mut saved = Vec::new();
    doc.save_to(&mut saved).expect("save");
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);

    std::fs::write(&args[2], &saved).expect("write output");
    eprintln!("Saved to {} ({} bytes)", args[2], saved.len());
}
