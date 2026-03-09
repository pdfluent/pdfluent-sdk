#![no_main]

use libfuzzer_sys::fuzz_target;
use pdf_annot::builder::{add_annotation_to_page, AnnotRect, AnnotationBuilder};

fuzz_target!(|data: &[u8]| {
    // Fuzz annotation creation on arbitrary PDF bytes.
    if data.len() > 16 && data.len() < 4 * 1024 * 1024 {
        if let Ok(mut doc) = lopdf::Document::load_mem(data) {
            if !doc.get_pages().is_empty() {
                let rect = AnnotRect {
                    x0: 72.0,
                    y0: 700.0,
                    x1: 200.0,
                    y1: 720.0,
                };
                if let Ok(annot_id) = AnnotationBuilder::highlight(rect)
                    .color(1.0, 1.0, 0.0)
                    .quad_points_from_rect(&rect)
                    .build(&mut doc)
                {
                    let _ = add_annotation_to_page(&mut doc, 1, annot_id);
                    let mut out = Vec::new();
                    let _ = doc.save_to(&mut out);
                }
            }
        }
    }
});
