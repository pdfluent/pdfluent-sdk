use super::*;

fn make_test_doc() -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let content_data = b"BT /F1 12 Tf (Test) Tj ET".to_vec();
    let content_stream = Stream::new(dictionary! {}, content_data);
    let content_id = doc.add_object(Object::Stream(content_stream));

    let page_dict = dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(lopdf::Dictionary::new()),
    };
    let page_id = doc.add_object(Object::Dictionary(page_dict));

    let pages_dict = dictionary! {
        "Type" => "Pages",
        "Count" => Object::Integer(1),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

    let catalog = dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    };
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    doc
}

#[test]
fn build_square_annotation() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(100.0, 200.0, 300.0, 400.0);
    let annot_id = AnnotationBuilder::new(AnnotSubtype::Square, rect)
        .color(1.0, 0.0, 0.0)
        .border_width(2.0)
        .contents("Red square")
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"Square".to_vec())
        );
        assert!(d.get(b"AP").is_ok());
        assert!(d.get(b"C").is_ok());
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn build_circle_annotation() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(50.0, 50.0, 150.0, 150.0);
    let annot_id = AnnotationBuilder::new(AnnotSubtype::Circle, rect)
        .color(0.0, 0.0, 1.0)
        .interior_color(0.8, 0.8, 1.0)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"Circle".to_vec())
        );
        assert!(d.get(b"IC").is_ok());
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn build_with_opacity() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(0.0, 0.0, 100.0, 100.0);
    let annot_id = AnnotationBuilder::new(AnnotSubtype::Square, rect)
        .opacity(0.5)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        let ca = d.get(b"CA").unwrap();
        assert_eq!(ca, &Object::Real(0.5));
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn reject_zero_area_rect() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(100.0, 200.0, 100.0, 400.0); // zero width
    let result = AnnotationBuilder::new(AnnotSubtype::Square, rect).build(&mut doc);
    assert!(result.is_err());
}

#[test]
fn add_annotation_to_page_creates_annots() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(10.0, 10.0, 50.0, 50.0);
    let annot_id = AnnotationBuilder::new(AnnotSubtype::Square, rect)
        .build(&mut doc)
        .unwrap();

    add_annotation_to_page(&mut doc, 1, annot_id).unwrap();

    // Verify page now has /Annots.
    let pages = doc.get_pages();
    let page_id = pages[&1];
    if let Object::Dictionary(d) = doc.get_object(page_id).unwrap() {
        let annots = d.get(b"Annots").unwrap();
        if let Object::Array(arr) = annots {
            assert_eq!(arr.len(), 1);
            assert_eq!(arr[0], Object::Reference(annot_id));
        } else {
            panic!("Expected array");
        }
    }
}

#[test]
fn add_annotation_appends_to_existing_annots() {
    let mut doc = make_test_doc();
    let rect1 = AnnotRect::new(10.0, 10.0, 50.0, 50.0);
    let rect2 = AnnotRect::new(60.0, 60.0, 100.0, 100.0);

    let id1 = AnnotationBuilder::new(AnnotSubtype::Square, rect1)
        .build(&mut doc)
        .unwrap();
    let id2 = AnnotationBuilder::new(AnnotSubtype::Circle, rect2)
        .build(&mut doc)
        .unwrap();

    add_annotation_to_page(&mut doc, 1, id1).unwrap();
    add_annotation_to_page(&mut doc, 1, id2).unwrap();

    let pages = doc.get_pages();
    let page_id = pages[&1];
    if let Object::Dictionary(d) = doc.get_object(page_id).unwrap() {
        if let Object::Array(arr) = d.get(b"Annots").unwrap() {
            assert_eq!(arr.len(), 2);
        } else {
            panic!("Expected array");
        }
    }
}

#[test]
fn invalid_page_returns_error() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(10.0, 10.0, 50.0, 50.0);
    let annot_id = AnnotationBuilder::new(AnnotSubtype::Square, rect)
        .build(&mut doc)
        .unwrap();

    let result = add_annotation_to_page(&mut doc, 99, annot_id);
    assert!(result.is_err());
}

#[test]
fn highlight_annotation() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
    let annot_id = AnnotationBuilder::new(AnnotSubtype::Highlight, rect)
        .color(1.0, 1.0, 0.0)
        .opacity(0.4)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"Highlight".to_vec())
        );
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn custom_appearance() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(0.0, 0.0, 100.0, 100.0);
    let annot_id = AnnotationBuilder::new(AnnotSubtype::Square, rect)
        .appearance(|b| {
            let red = AppearanceColor::new(1.0, 0.0, 0.0);
            b.filled_rect(&red);
        })
        .build(&mut doc)
        .unwrap();

    assert!(doc.get_object(annot_id).is_ok());
}

// --- Issue #303 tests: Markup annotations ---

#[test]
fn highlight_with_quad_points() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
    let annot_id = AnnotationBuilder::highlight(rect)
        .quad_points_from_rect(&rect)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"Highlight".to_vec())
        );
        // Check QuadPoints present.
        let qp = d.get(b"QuadPoints").unwrap();
        if let Object::Array(arr) = qp {
            assert_eq!(arr.len(), 8); // Single quad = 8 values.
        } else {
            panic!("Expected QuadPoints array");
        }
        // Check opacity is set.
        assert!(d.get(b"CA").is_ok());
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn highlight_has_multiply_blend() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
    let annot_id = AnnotationBuilder::highlight(rect).build(&mut doc).unwrap();

    // Get the appearance stream and verify its resources contain BM /Multiply.
    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        let ap = d.get(b"AP").unwrap();
        if let Object::Dictionary(ap_dict) = ap {
            let n_ref = ap_dict.get(b"N").unwrap();
            if let Object::Reference(stream_id) = n_ref {
                let stream = doc.get_object(*stream_id).unwrap();
                if let Object::Stream(s) = stream {
                    let res = s.dict.get(b"Resources").unwrap();
                    if let Object::Dictionary(res_dict) = res {
                        let gs = res_dict.get(b"ExtGState").unwrap();
                        if let Object::Dictionary(gs_dict) = gs {
                            let gs0_ref = gs_dict.get(b"GS0").unwrap();
                            if let Object::Reference(gs0_id) = gs0_ref {
                                let gs0 = doc.get_object(*gs0_id).unwrap();
                                if let Object::Dictionary(gs0_dict) = gs0 {
                                    assert_eq!(
                                        gs0_dict.get(b"BM").unwrap(),
                                        &Object::Name(b"Multiply".to_vec())
                                    );
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    panic!("Could not find BM /Multiply in ExtGState");
}

#[test]
fn underline_convenience() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
    let annot_id = AnnotationBuilder::underline(rect)
        .quad_points_from_rect(&rect)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"Underline".to_vec())
        );
        assert!(d.get(b"QuadPoints").is_ok());
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn strikeout_convenience() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
    let annot_id = AnnotationBuilder::strikeout(rect).build(&mut doc).unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"StrikeOut".to_vec())
        );
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn squiggly_convenience() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
    let annot_id = AnnotationBuilder::squiggly(rect).build(&mut doc).unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"Squiggly".to_vec())
        );
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn multi_quad_points() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 688.0, 400.0, 712.0);
    // Two quads for two lines of text.
    let qp = vec![
        72.0, 712.0, 400.0, 712.0, 72.0, 700.0, 400.0, 700.0, // line 1
        72.0, 700.0, 300.0, 700.0, 72.0, 688.0, 300.0, 688.0, // line 2
    ];
    let annot_id = AnnotationBuilder::highlight(rect)
        .quad_points(qp)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        if let Object::Array(arr) = d.get(b"QuadPoints").unwrap() {
            assert_eq!(arr.len(), 16); // 2 quads × 8 values.
        } else {
            panic!("Expected QuadPoints array");
        }
    }
}

// --- Issue #304 tests: Geometric annotations ---

#[test]
fn square_convenience() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(100.0, 100.0, 200.0, 200.0);
    let annot_id = AnnotationBuilder::square(rect)
        .color(0.0, 0.0, 1.0)
        .interior_color(0.9, 0.9, 1.0)
        .border_width(2.0)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"Square".to_vec())
        );
        assert!(d.get(b"IC").is_ok());
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn circle_convenience() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(50.0, 50.0, 150.0, 150.0);
    let annot_id = AnnotationBuilder::circle(rect)
        .color(1.0, 0.0, 0.0)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"Circle".to_vec())
        );
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn line_annotation_with_endpoints() {
    let mut doc = make_test_doc();
    let annot_id = AnnotationBuilder::line(100.0, 200.0, 400.0, 600.0)
        .color(1.0, 0.0, 0.0)
        .border_width(2.0)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(d.get(b"Subtype").unwrap(), &Object::Name(b"Line".to_vec()));
        // Check /L array present.
        let l = d.get(b"L").unwrap();
        if let Object::Array(arr) = l {
            assert_eq!(arr.len(), 4);
        } else {
            panic!("Expected /L array");
        }
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn line_with_endings() {
    let mut doc = make_test_doc();
    let annot_id = AnnotationBuilder::line(100.0, 300.0, 500.0, 300.0)
        .line_endings(LineEnding::ClosedArrow, LineEnding::OpenArrow)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        let le = d.get(b"LE").unwrap();
        if let Object::Array(arr) = le {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0], Object::Name(b"ClosedArrow".to_vec()));
            assert_eq!(arr[1], Object::Name(b"OpenArrow".to_vec()));
        } else {
            panic!("Expected /LE array");
        }
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn ink_annotation() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(50.0, 50.0, 200.0, 200.0);
    let strokes = vec![
        vec![60.0, 60.0, 100.0, 150.0, 180.0, 80.0],
        vec![70.0, 70.0, 120.0, 160.0],
    ];
    let annot_id = AnnotationBuilder::ink(rect, strokes)
        .color(0.0, 0.5, 0.0)
        .border_width(3.0)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(d.get(b"Subtype").unwrap(), &Object::Name(b"Ink".to_vec()));
        let ink = d.get(b"InkList").unwrap();
        if let Object::Array(arr) = ink {
            assert_eq!(arr.len(), 2); // Two strokes.
        } else {
            panic!("Expected InkList array");
        }
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn polygon_annotation() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(100.0, 100.0, 300.0, 300.0);
    let verts = vec![100.0, 100.0, 300.0, 100.0, 200.0, 300.0];
    let annot_id = AnnotationBuilder::polygon(rect, verts)
        .color(0.0, 0.0, 1.0)
        .interior_color(0.8, 0.8, 1.0)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"Polygon".to_vec())
        );
        let v = d.get(b"Vertices").unwrap();
        if let Object::Array(arr) = v {
            assert_eq!(arr.len(), 6);
        } else {
            panic!("Expected Vertices array");
        }
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn polyline_annotation() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(50.0, 50.0, 400.0, 200.0);
    let verts = vec![50.0, 100.0, 200.0, 180.0, 350.0, 60.0, 400.0, 150.0];
    let annot_id = AnnotationBuilder::polyline(rect, verts)
        .color(1.0, 0.5, 0.0)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"PolyLine".to_vec())
        );
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn dashed_line_annotation() {
    let mut doc = make_test_doc();
    let annot_id = AnnotationBuilder::line(72.0, 400.0, 540.0, 400.0)
        .dash(vec![3.0, 2.0])
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        let bs = d.get(b"BS").unwrap();
        if let Object::Dictionary(bs_dict) = bs {
            assert_eq!(bs_dict.get(b"S").unwrap(), &Object::Name(b"D".to_vec()));
            assert!(bs_dict.get(b"D").is_ok());
        } else {
            panic!("Expected BS dictionary");
        }
    } else {
        panic!("Expected dictionary");
    }
}

// --- Issue #305 tests: Text annotations ---

#[test]
fn free_text_annotation() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 700.0, 300.0, 730.0);
    let annot_id = AnnotationBuilder::free_text(rect, "Hello World", 14.0)
        .alignment(1) // Center
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Subtype").unwrap(),
            &Object::Name(b"FreeText".to_vec())
        );
        assert!(d.get(b"DA").is_ok());
        assert_eq!(d.get(b"Q").unwrap(), &Object::Integer(1));
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn sticky_note_annotation() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(500.0, 700.0, 524.0, 724.0);
    let annot_id = AnnotationBuilder::sticky_note(rect, TextIcon::Comment)
        .contents("This is a comment")
        .color(1.0, 1.0, 0.0)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(d.get(b"Subtype").unwrap(), &Object::Name(b"Text".to_vec()));
        assert_eq!(d.get(b"Name").unwrap(), &Object::Name(b"Comment".to_vec()));
        assert!(d.get(b"Contents").is_ok());
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn stamp_annotation() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 600.0, 250.0, 650.0);
    let annot_id = AnnotationBuilder::stamp(rect, StampName::Approved)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(d.get(b"Subtype").unwrap(), &Object::Name(b"Stamp".to_vec()));
        assert_eq!(d.get(b"Name").unwrap(), &Object::Name(b"Approved".to_vec()));
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn stamp_custom_name() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 500.0, 250.0, 550.0);
    let annot_id = AnnotationBuilder::stamp_custom(rect, "ReviewNeeded")
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(
            d.get(b"Name").unwrap(),
            &Object::Name(b"ReviewNeeded".to_vec())
        );
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn link_uri_annotation() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 700.0, 200.0, 712.0);
    let annot_id = AnnotationBuilder::link_uri(rect, "https://example.com")
        .color(0.0, 0.0, 1.0)
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert_eq!(d.get(b"Subtype").unwrap(), &Object::Name(b"Link".to_vec()));
        let action = d.get(b"A").unwrap();
        if let Object::Dictionary(a) = action {
            assert_eq!(a.get(b"S").unwrap(), &Object::Name(b"URI".to_vec()));
        } else {
            panic!("Expected action dictionary");
        }
    } else {
        panic!("Expected dictionary");
    }
}

#[test]
fn link_destination_annotation() {
    let mut doc = make_test_doc();
    let rect = AnnotRect::new(72.0, 650.0, 200.0, 662.0);
    let annot_id = AnnotationBuilder::link_dest(rect, "chapter1")
        .build(&mut doc)
        .unwrap();

    let annot = doc.get_object(annot_id).unwrap();
    if let Object::Dictionary(d) = annot {
        assert!(d.get(b"Dest").is_ok());
    } else {
        panic!("Expected dictionary");
    }
}
