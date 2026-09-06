//! Integration tests for Sprint A/B/C SDK hardening: the image cache,
//! incremental save, and annotation flattening (including idempotency and
//! reference-stripping).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;
use pdfluent::{Error, ImageFormat, OpenOptions, SaveOptions};
use std::path::PathBuf;

fn mini(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus-mini")
        .join(name)
}

fn open_doc(name: &str) -> PdfDocument {
    let path = mini(name);
    PdfDocument::open_with(&path, OpenOptions::new()).expect("open")
}

fn read_and_fix_simple_pdf() -> Vec<u8> {
    let path = mini("simple.pdf");
    let mut bytes = std::fs::read(&path).expect("read original bytes");
    if let Some(pos) = bytes.windows(8).position(|w| w == b"//Length") {
        bytes[pos] = b'/';
        bytes[pos + 1] = b'L';
        bytes[pos + 2] = b'e';
        bytes[pos + 3] = b'n';
        bytes[pos + 4] = b'g';
        bytes[pos + 5] = b't';
        bytes[pos + 6] = b'h';
        bytes[pos + 7] = b' ';
    }
    bytes
}

#[test]
fn test_image_cache_equivalence() {
    let doc = open_doc("multi-page.pdf");

    // Render first page. This automatically creates/uses the document-level shared cache.
    let render1 = doc
        .render_page(1, 150, ImageFormat::Png)
        .expect("render first page");

    // Render again — this will hit the shared image cache. The output PNG bytes must match exactly.
    let render2 = doc
        .render_page(1, 150, ImageFormat::Png)
        .expect("render again");

    assert_eq!(
        render1, render2,
        "Rendered PNG bytes with cache must match exactly"
    );
}

#[test]
fn test_incremental_save_preserves_prefix() {
    let path = mini("simple.pdf");
    let original_bytes = std::fs::read(&path).expect("read original bytes");

    let mut doc = open_doc("simple.pdf");

    // Mutate the document metadata so there is a change to write
    doc.metadata_mut()
        .set_title("Sprint A Incremental Update")
        .commit()
        .expect("commit metadata");

    // Save incrementally
    let opts = SaveOptions::new().with_incremental(true);
    let tmp_path = std::env::temp_dir().join("pdfluent_incremental_preservation.pdf");
    doc.save_with(&tmp_path, opts).expect("save incrementally");

    let saved_bytes = std::fs::read(&tmp_path).expect("read saved bytes");
    let _ = std::fs::remove_file(&tmp_path);

    let n = original_bytes.len();
    assert!(
        saved_bytes.len() > n,
        "incremental save must append data, length should be greater than original"
    );
    assert_eq!(
        &saved_bytes[..n],
        &original_bytes[..],
        "incremental update must preserve original prefix byte-for-byte"
    );
}

#[test]
fn test_incremental_save_preserves_signatures() {
    let original_bytes = read_and_fix_simple_pdf();

    // Load with lopdf and add a dummy signature object
    let mut lopdf_doc = lopdf::Document::load_mem(&original_bytes).expect("load lopdf");
    lopdf_doc.max_id = lopdf_doc
        .objects
        .keys()
        .map(|&(id, _)| id)
        .max()
        .unwrap_or(0);
    use lopdf::{dictionary, Object};

    let sig_dict = dictionary! {
        "Type" => Object::Name(b"Sig".to_vec()),
        "Filter" => Object::Name(b"Adobe.PPKLite".to_vec()),
        "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
        "ByteRange" => Object::Array(vec![
            Object::Integer(0),
            Object::Integer(100),
            Object::Integer(200),
            Object::Integer(300),
        ]),
        "Contents" => Object::String(vec![0u8; 100], lopdf::StringFormat::Hexadecimal),
    };
    let _sig_id = lopdf_doc.add_object(sig_dict);

    // Save to bytes
    let mut modified_bytes = Vec::new();
    lopdf_doc
        .save_to(&mut modified_bytes)
        .expect("save to bytes");

    // Open in pdfluent
    let mut doc =
        PdfDocument::from_bytes_with(&modified_bytes, OpenOptions::new()).expect("open from bytes");

    // Perform a mutation: change metadata
    doc.metadata_mut()
        .set_title("Sprint A Incremental Signature Preservation")
        .commit()
        .expect("commit metadata");

    // Save incrementally
    let opts = SaveOptions::new().with_incremental(true);
    let tmp_path = std::env::temp_dir().join("pdfluent_incremental_signature.pdf");
    doc.save_with(&tmp_path, opts).expect("save incrementally");

    let saved_bytes = std::fs::read(&tmp_path).expect("read saved bytes");
    let _ = std::fs::remove_file(&tmp_path);

    // Verify that the prefix (which contains our dummy signature object) remains byte-for-byte identical
    let n = modified_bytes.len();
    assert!(saved_bytes.len() > n, "incremental save must append data");
    assert_eq!(
        &saved_bytes[..n],
        &modified_bytes[..],
        "incremental update must preserve original prefix byte-for-byte, protecting the signature"
    );

    // End-to-end: the incrementally-saved file must still be a loadable PDF and
    // the signature object must survive intact — not merely its prefix bytes.
    let reopened =
        lopdf::Document::load_mem(&saved_bytes).expect("incremental output must re-open");
    let sig = reopened
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| matches!(d.get(b"Type"), Ok(Object::Name(n)) if n.as_slice() == b"Sig"))
        .expect("signature object must survive incremental save");
    assert!(
        matches!(sig.get(b"ByteRange"), Ok(Object::Array(_))),
        "signature /ByteRange must be preserved after incremental save"
    );
}

#[test]
fn test_incremental_save_encrypted_returns_unsupported() {
    let mut doc = open_doc("simple.pdf");
    doc.encrypt(
        EncryptOptions::aes256()
            .with_user_password("user-pw")
            .with_owner_password("owner-pw"),
    )
    .expect("encrypt");

    // Attempting incremental save must fail with Error::Unsupported
    let res = doc.to_incremental_bytes();
    assert!(res.is_err(), "incremental save on encrypted PDF must fail");
    let err = res.unwrap_err();
    assert!(
        matches!(err, Error::Unsupported(_)),
        "expected Error::Unsupported, got {:?}",
        err
    );
    assert_eq!(err.code(), "E-UNSUPPORTED");
}

#[test]
fn test_incremental_save_created_from_scratch_returns_unsupported() {
    // A document created from scratch does not have backing original bytes
    let doc = PdfDocument::create();

    let res = doc.to_incremental_bytes();
    assert!(res.is_err(), "incremental save on created PDF must fail");
    let err = res.unwrap_err();
    assert!(
        matches!(err, Error::Unsupported(_)),
        "expected Error::Unsupported, got {:?}",
        err
    );
    assert_eq!(err.code(), "E-UNSUPPORTED");
}

#[test]
fn test_annotation_flattening_filtering() {
    let original_bytes = read_and_fix_simple_pdf();

    let mut lopdf_doc = lopdf::Document::load_mem(&original_bytes).expect("load lopdf");
    lopdf_doc.max_id = lopdf_doc
        .objects
        .keys()
        .map(|&(id, _)| id)
        .max()
        .unwrap_or(0);
    use lopdf::{dictionary, Object, Stream};

    let page_id = *lopdf_doc.get_pages().get(&1).expect("page 1 exists");

    let ap_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.0.into(), 0.0.into(), 50.0.into(), 50.0.into()],
        },
        b"q 1 0 0 rg 0 0 50 50 re f Q".to_vec(),
    );
    let ap_id = lopdf_doc.add_object(ap_stream);

    let make_annot = |subtype: &str, has_ap: bool| {
        let mut d = dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(subtype.as_bytes().to_vec()),
            "Rect" => vec![100.0.into(), 100.0.into(), 150.0.into(), 150.0.into()],
        };
        if has_ap {
            d.set("AP", dictionary! { "N" => Object::Reference(ap_id) });
        }
        d
    };

    let widget_id = lopdf_doc.add_object(make_annot("Widget", true));
    let link_id = lopdf_doc.add_object(make_annot("Link", true));
    let square_id = lopdf_doc.add_object(make_annot("Square", true));
    let circle_id = lopdf_doc.add_object(make_annot("Circle", true));
    let stamp_id = lopdf_doc.add_object(make_annot("Stamp", true));

    let annots = vec![
        Object::Reference(widget_id),
        Object::Reference(link_id),
        Object::Reference(square_id),
        Object::Reference(circle_id),
        Object::Reference(stamp_id),
    ];

    if let Ok(Object::Dictionary(ref mut page_dict)) = lopdf_doc.get_object_mut(page_id) {
        page_dict.set("Annots", Object::Array(annots));
    }

    let mut modified_bytes = Vec::new();
    lopdf_doc
        .save_to(&mut modified_bytes)
        .expect("save modified");

    // Open in pdfluent
    let mut doc =
        PdfDocument::from_bytes_with(&modified_bytes, OpenOptions::new()).expect("open from bytes");

    // Flatten
    doc.flatten_annotations().expect("flatten");

    // Save
    let saved_bytes = doc.to_bytes().expect("to_bytes");

    // Reopen and check
    let doc_reopened = lopdf::Document::load_mem(&saved_bytes).expect("reopen");
    let page_id_reopened = *doc_reopened.get_pages().get(&1).unwrap();
    let page_dict = doc_reopened.get_dictionary(page_id_reopened).unwrap();
    let arr = match page_dict.get(b"Annots").unwrap() {
        Object::Array(ref a) => a,
        _ => panic!("Annots must be array"),
    };
    assert_eq!(
        arr.len(),
        2,
        "Widget and Link annotations must remain interactive"
    );

    let mut subtypes = Vec::new();
    for obj in arr {
        let r = match obj {
            Object::Reference(id) => *id,
            _ => panic!("must be reference"),
        };
        let d = doc_reopened.get_dictionary(r).unwrap();
        let s = d.get(b"Subtype").unwrap().as_name().unwrap();
        subtypes.push(s.to_vec());
    }
    assert!(subtypes.contains(&b"Widget".to_vec()));
    assert!(subtypes.contains(&b"Link".to_vec()));

    assert!(
        !doc_reopened.objects.contains_key(&square_id),
        "Square annot must be deleted"
    );
    assert!(
        !doc_reopened.objects.contains_key(&circle_id),
        "Circle annot must be deleted"
    );
    assert!(
        !doc_reopened.objects.contains_key(&stamp_id),
        "Stamp annot must be deleted"
    );
}

#[test]
fn test_annotation_flattening_visual_equivalence_and_no_leak() {
    let original_bytes = read_and_fix_simple_pdf();

    let mut lopdf_doc = lopdf::Document::load_mem(&original_bytes).expect("load lopdf");
    lopdf_doc.max_id = lopdf_doc
        .objects
        .keys()
        .map(|&(id, _)| id)
        .max()
        .unwrap_or(0);
    use lopdf::{dictionary, Object, Stream};

    let page_id = *lopdf_doc.get_pages().get(&1).expect("page 1 exists");

    let ap_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.0.into(), 0.0.into(), 50.0.into(), 50.0.into()],
        },
        b"q 0 0 1 rg 0 0 50 50 re f Q".to_vec(),
    );
    let ap_id = lopdf_doc.add_object(ap_stream);

    let square_annot = dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Square".to_vec()),
        "Rect" => vec![100.0.into(), 100.0.into(), 150.0.into(), 150.0.into()],
        "AP" => dictionary! { "N" => Object::Reference(ap_id) },
    };
    let square_id = lopdf_doc.add_object(square_annot);

    if let Ok(Object::Dictionary(ref mut page_dict)) = lopdf_doc.get_object_mut(page_id) {
        page_dict.set("Annots", Object::Array(vec![Object::Reference(square_id)]));
    }

    let mut modified_bytes = Vec::new();
    lopdf_doc
        .save_to(&mut modified_bytes)
        .expect("save modified");

    // Open in pdfluent
    let mut doc =
        PdfDocument::from_bytes_with(&modified_bytes, OpenOptions::new()).expect("open from bytes");

    let render_before = doc
        .render_page(1, 150, ImageFormat::Png)
        .expect("render before");

    // Flatten
    doc.flatten_annotations().expect("flatten");

    let render_after = doc
        .render_page(1, 150, ImageFormat::Png)
        .expect("render after");

    let saved_bytes = doc.to_bytes().expect("to_bytes");

    assert_eq!(
        render_before, render_after,
        "Rendered output before and after flattening must be identical"
    );

    let doc_reopened = lopdf::Document::load_mem(&saved_bytes).expect("reopen");
    assert!(
        !doc_reopened.objects.contains_key(&square_id),
        "Annotation dictionary must be removed from serialized file"
    );
}

#[test]
fn test_annotation_flattening_removes_popup_companion_no_dangling_parent() {
    // A markup annotation with a /Popup companion: flattening the markup must
    // also remove the popup, so the popup's /Parent back-reference cannot dangle.
    let original_bytes = read_and_fix_simple_pdf();

    let mut lopdf_doc = lopdf::Document::load_mem(&original_bytes).expect("load lopdf");
    lopdf_doc.max_id = lopdf_doc
        .objects
        .keys()
        .map(|&(id, _)| id)
        .max()
        .unwrap_or(0);
    use lopdf::{dictionary, Object, Stream};

    let page_id = *lopdf_doc.get_pages().get(&1).expect("page 1 exists");

    let ap_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.0.into(), 0.0.into(), 50.0.into(), 50.0.into()],
        },
        b"q 0 1 0 rg 0 0 50 50 re f Q".to_vec(),
    );
    let ap_id = lopdf_doc.add_object(ap_stream);

    let square = dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Square".to_vec()),
        "Rect" => vec![100.0.into(), 100.0.into(), 150.0.into(), 150.0.into()],
        "AP" => dictionary! { "N" => Object::Reference(ap_id) },
    };
    let square_id = lopdf_doc.add_object(square);

    let popup = dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Popup".to_vec()),
        "Rect" => vec![160.0.into(), 100.0.into(), 260.0.into(), 200.0.into()],
        "Parent" => Object::Reference(square_id),
    };
    let popup_id = lopdf_doc.add_object(popup);

    if let Ok(Object::Dictionary(ref mut d)) = lopdf_doc.get_object_mut(square_id) {
        d.set("Popup", Object::Reference(popup_id));
    }
    if let Ok(Object::Dictionary(ref mut page_dict)) = lopdf_doc.get_object_mut(page_id) {
        page_dict.set(
            "Annots",
            Object::Array(vec![
                Object::Reference(square_id),
                Object::Reference(popup_id),
            ]),
        );
    }

    let mut modified_bytes = Vec::new();
    lopdf_doc
        .save_to(&mut modified_bytes)
        .expect("save modified");

    let mut doc =
        PdfDocument::from_bytes_with(&modified_bytes, OpenOptions::new()).expect("open from bytes");

    doc.flatten_annotations().expect("flatten");
    let saved_bytes = doc.to_bytes().expect("to_bytes");

    let reopened = lopdf::Document::load_mem(&saved_bytes).expect("reopen");
    assert!(
        !reopened.objects.contains_key(&square_id),
        "flattened Square markup annotation must be removed"
    );
    assert!(
        !reopened.objects.contains_key(&popup_id),
        "orphaned Popup companion must be removed (no dangling /Parent)"
    );

    // Defensive: no surviving annotation may carry a /Parent that points at a
    // now-missing object.
    let page_id_re = *reopened.get_pages().get(&1).unwrap();
    let page_dict = reopened.get_dictionary(page_id_re).unwrap();
    if let Ok(Object::Array(arr)) = page_dict.get(b"Annots") {
        for o in arr {
            if let Object::Reference(id) = o {
                if let Ok(d) = reopened.get_dictionary(*id) {
                    if let Ok(Object::Reference(parent)) = d.get(b"Parent") {
                        assert!(
                            reopened.objects.contains_key(parent),
                            "surviving annotation /Parent must not dangle"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn test_annotation_flattening_is_idempotent() {
    // Flattening an already-flattened document must be a safe no-op: there are
    // no flattenable annotations left, so the rendered output is unchanged and
    // no FlatAnnot name collision can corrupt the page.
    let original_bytes = read_and_fix_simple_pdf();

    let mut lopdf_doc = lopdf::Document::load_mem(&original_bytes).expect("load lopdf");
    lopdf_doc.max_id = lopdf_doc
        .objects
        .keys()
        .map(|&(id, _)| id)
        .max()
        .unwrap_or(0);
    use lopdf::{dictionary, Object, Stream};

    let page_id = *lopdf_doc.get_pages().get(&1).expect("page 1 exists");
    let ap_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.0.into(), 0.0.into(), 50.0.into(), 50.0.into()],
        },
        b"q 0 0 1 rg 0 0 50 50 re f Q".to_vec(),
    );
    let ap_id = lopdf_doc.add_object(ap_stream);
    let square = dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Square".to_vec()),
        "Rect" => vec![100.0.into(), 100.0.into(), 150.0.into(), 150.0.into()],
        "AP" => dictionary! { "N" => Object::Reference(ap_id) },
    };
    let square_id = lopdf_doc.add_object(square);
    if let Ok(Object::Dictionary(ref mut pd)) = lopdf_doc.get_object_mut(page_id) {
        pd.set("Annots", Object::Array(vec![Object::Reference(square_id)]));
    }
    let mut modified_bytes = Vec::new();
    lopdf_doc
        .save_to(&mut modified_bytes)
        .expect("save modified");

    let mut doc =
        PdfDocument::from_bytes_with(&modified_bytes, OpenOptions::new()).expect("open from bytes");

    // First flatten bakes the appearance and removes the annotation.
    doc.flatten_annotations().expect("first flatten");
    let render_once = doc
        .render_page(1, 150, ImageFormat::Png)
        .expect("render once");

    // Second flatten must not error and must not change the rendering.
    doc.flatten_annotations()
        .expect("second flatten must not error");
    let render_twice = doc
        .render_page(1, 150, ImageFormat::Png)
        .expect("render twice");

    assert_eq!(
        render_once, render_twice,
        "repeated flattening must be idempotent"
    );

    let saved = doc.to_bytes().expect("to_bytes");
    let reopened = lopdf::Document::load_mem(&saved).expect("reopen");
    assert!(
        !reopened.objects.contains_key(&square_id),
        "flattened annotation must stay removed across repeated flattening"
    );
}

#[test]
fn test_incremental_save_appends_only_changed_objects() {
    // A metadata-only change must append only the changed object(s) plus a fresh
    // xref/trailer — never re-emit the whole document. Verified on a 53-object
    // document: the appended increment stays small and independent of the total
    // object count, proving refresh_from_lopdf does not cause re-emission bloat.
    let path = mini("multi-page.pdf");
    let original = std::fs::read(&path).expect("read original bytes");

    let mut doc = open_doc("multi-page.pdf");
    doc.metadata_mut()
        .set_title("Incremental Bloat Guard")
        .commit()
        .expect("commit metadata");

    let incr = doc.to_incremental_bytes().expect("incremental save");
    assert_eq!(
        &incr[..original.len()],
        &original[..],
        "original prefix must be preserved verbatim"
    );
    let appended = incr.len() - original.len();
    assert!(
        appended < 4096,
        "metadata-only increment appended {appended} bytes; expected a small, \
         object-count-independent delta (regression: full document re-emission)"
    );
}

#[test]
fn test_annotation_flattening_clears_irt_and_objr_references() {
    // Flattening a markup annotation that is referenced by a reply (/IRT) and by
    // a tagged structure-tree OBJR must leave NO dangling reference to it.
    let original_bytes = read_and_fix_simple_pdf();
    let mut lopdf_doc = lopdf::Document::load_mem(&original_bytes).expect("load lopdf");
    lopdf_doc.max_id = lopdf_doc
        .objects
        .keys()
        .map(|&(id, _)| id)
        .max()
        .unwrap_or(0);
    use lopdf::{dictionary, Object, Stream, StringFormat};

    let page_id = *lopdf_doc.get_pages().get(&1).expect("page 1 exists");
    let ap_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.0.into(), 0.0.into(), 50.0.into(), 50.0.into()],
        },
        b"q 0 0 1 rg 0 0 50 50 re f Q".to_vec(),
    );
    let ap_id = lopdf_doc.add_object(ap_stream);

    // Flattenable markup annotation.
    let square = dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Square".to_vec()),
        "Rect" => vec![100.0.into(), 100.0.into(), 150.0.into(), 150.0.into()],
        "AP" => dictionary! { "N" => Object::Reference(ap_id) },
    };
    let square_id = lopdf_doc.add_object(square);

    // A reply annotation (no /AP, so it survives flatten) pointing at the square.
    let reply = dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Text".to_vec()),
        "Rect" => vec![200.0.into(), 100.0.into(), 220.0.into(), 120.0.into()],
        "IRT" => Object::Reference(square_id),
        "Contents" => Object::String(b"a reply".to_vec(), StringFormat::Literal),
    };
    let reply_id = lopdf_doc.add_object(reply);

    // A tagged structure-tree OBJR referencing the square.
    let objr = dictionary! {
        "Type" => Object::Name(b"OBJR".to_vec()),
        "Obj" => Object::Reference(square_id),
    };
    let objr_id = lopdf_doc.add_object(objr);
    let elem = dictionary! {
        "Type" => Object::Name(b"StructElem".to_vec()),
        "S" => Object::Name(b"Annot".to_vec()),
        "K" => Object::Reference(objr_id),
    };
    let elem_id = lopdf_doc.add_object(elem);
    let struct_root = dictionary! {
        "Type" => Object::Name(b"StructTreeRoot".to_vec()),
        "K" => Object::Reference(elem_id),
    };
    let struct_root_id = lopdf_doc.add_object(struct_root);

    if let Ok(Object::Dictionary(ref mut pd)) = lopdf_doc.get_object_mut(page_id) {
        pd.set(
            "Annots",
            Object::Array(vec![
                Object::Reference(square_id),
                Object::Reference(reply_id),
            ]),
        );
    }
    let catalog_id = lopdf_doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .expect("catalog /Root");
    if let Ok(Object::Dictionary(ref mut cat)) = lopdf_doc.get_object_mut(catalog_id) {
        cat.set("StructTreeRoot", Object::Reference(struct_root_id));
    }

    let mut modified_bytes = Vec::new();
    lopdf_doc
        .save_to(&mut modified_bytes)
        .expect("save modified");

    let mut doc =
        PdfDocument::from_bytes_with(&modified_bytes, OpenOptions::new()).expect("open from bytes");

    let render_before = doc
        .render_page(1, 150, ImageFormat::Png)
        .expect("render before");
    doc.flatten_annotations().expect("flatten");
    let render_after = doc
        .render_page(1, 150, ImageFormat::Png)
        .expect("render after");
    assert_eq!(
        render_before, render_after,
        "flatten must preserve render output"
    );

    let saved = doc.to_bytes().expect("to_bytes");
    let reopened = lopdf::Document::load_mem(&saved).expect("reopen");

    assert!(
        !reopened.objects.contains_key(&square_id),
        "flattened square must be removed"
    );
    let reply_dict = reopened.get_dictionary(reply_id).expect("reply survives");
    assert!(
        reply_dict.get(b"IRT").is_err(),
        "/IRT pointing at a flattened annotation must be removed"
    );
    let objr_dict = reopened.get_dictionary(objr_id).expect("OBJR survives");
    assert!(
        objr_dict.get(b"Obj").is_err(),
        "structure-tree OBJR /Obj pointing at a flattened annotation must be removed"
    );

    // Strongest guarantee: nothing anywhere still references the deleted square.
    fn refs_id(obj: &Object, id: lopdf::ObjectId) -> bool {
        match obj {
            Object::Reference(r) => *r == id,
            Object::Array(a) => a.iter().any(|o| refs_id(o, id)),
            Object::Dictionary(d) => d.iter().any(|(_, o)| refs_id(o, id)),
            Object::Stream(s) => s.dict.iter().any(|(_, o)| refs_id(o, id)),
            _ => false,
        }
    }
    for obj in reopened.objects.values() {
        assert!(
            !refs_id(obj, square_id),
            "no dangling reference to the flattened annotation may remain"
        );
    }
}
