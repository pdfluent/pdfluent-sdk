// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! An outline entry without a destination, and the 1.x shape of
//! `BookmarkAction`.
//!
//! `BookmarkAction` is an exhaustive public enum published in 1.0.0: a new
//! variant breaks every downstream exhaustive match, so "no destination" is
//! carried by `OutlineEntry::action == None` instead. These tests pin both
//! halves: the new surface keeps the difference, the old one keeps its
//! 1.0.0 answer, and the enum keeps its 1.0.0 variant set.

use lopdf::{dictionary, Document, Object, ObjectId};
use pdf_manip::bookmarks::{
    read_bookmarks, read_outline, write_outline, BookmarkAction, FitType, OutlineEntry,
};

/// A document with `pages` empty pages and no outline. Returns the page ids.
fn doc_with_pages(pages: usize) -> (Document, Vec<ObjectId>) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    let mut ids = Vec::new();
    for _ in 0..pages {
        let page = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        };
        let id = doc.add_object(Object::Dictionary(page));
        kids.push(Object::Reference(id));
        ids.push(id);
    }
    let pages_dict = dictionary! {
        "Type" => "Pages",
        "Count" => Object::Integer(pages as i64),
        "Kids" => Object::Array(kids),
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
    let catalog = dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    };
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    (doc, ids)
}

/// Two outline items: "Bare" with neither /Dest nor /A, then "Third" with
/// an explicit /Dest to page 3.
fn doc_with_a_bare_entry() -> Document {
    let (mut doc, pages) = doc_with_pages(3);
    let outlines_id = doc.new_object_id();
    let bare_id = doc.new_object_id();
    let third_id = doc.new_object_id();
    let bare = dictionary! {
        "Title" => Object::string_literal("Bare"),
        "Parent" => Object::Reference(outlines_id),
        "Next" => Object::Reference(third_id),
    };
    let third = dictionary! {
        "Title" => Object::string_literal("Third"),
        "Parent" => Object::Reference(outlines_id),
        "Prev" => Object::Reference(bare_id),
        "Dest" => Object::Array(vec![Object::Reference(pages[2]), Object::Name(b"Fit".to_vec())]),
    };
    let outlines = dictionary! {
        "Type" => "Outlines",
        "First" => Object::Reference(bare_id),
        "Last" => Object::Reference(third_id),
        "Count" => Object::Integer(2),
    };
    doc.objects.insert(bare_id, Object::Dictionary(bare));
    doc.objects.insert(third_id, Object::Dictionary(third));
    doc.objects
        .insert(outlines_id, Object::Dictionary(outlines));
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    doc.get_dictionary_mut(root)
        .unwrap()
        .set("Outlines", Object::Reference(outlines_id));
    doc
}

/// The outline item dictionaries, in tree order (depth first).
fn item_dicts(doc: &Document) -> Vec<lopdf::Dictionary> {
    fn walk(doc: &Document, first: Option<ObjectId>, out: &mut Vec<lopdf::Dictionary>) {
        let mut next = first;
        while let Some(id) = next {
            let dict = doc.get_dictionary(id).unwrap().clone();
            let child = dict.get(b"First").ok().and_then(|o| o.as_reference().ok());
            next = dict.get(b"Next").ok().and_then(|o| o.as_reference().ok());
            out.push(dict);
            walk(doc, child, out);
        }
    }
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let catalog = doc.get_dictionary(root).unwrap();
    let outlines_id = catalog.get(b"Outlines").unwrap().as_reference().unwrap();
    let first = doc
        .get_dictionary(outlines_id)
        .unwrap()
        .get(b"First")
        .ok()
        .and_then(|o| o.as_reference().ok());
    let mut out = Vec::new();
    walk(doc, first, &mut out);
    out
}

#[test]
fn read_outline_gives_none_for_an_entry_without_dest_or_action() {
    let doc = doc_with_a_bare_entry();
    let entries = read_outline(&doc).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].title, "Bare");
    assert!(
        entries[0].action.is_none(),
        "an entry without /Dest and /A must read as no destination, got {:?}",
        entries[0].action
    );
    assert!(
        matches!(
            entries[1].action,
            Some(BookmarkAction::GoTo {
                page: 3,
                fit: FitType::Fit
            })
        ),
        "the explicit /Dest must still read as page 3, got {:?}",
        entries[1].action
    );
}

#[test]
fn read_bookmarks_keeps_the_1_0_0_page_one_fallback_for_the_same_entry() {
    let doc = doc_with_a_bare_entry();
    let bookmarks = read_bookmarks(&doc).unwrap();
    assert_eq!(bookmarks.len(), 2);
    assert!(
        matches!(
            bookmarks[0].action,
            BookmarkAction::GoTo {
                page: 1,
                fit: FitType::Fit
            }
        ),
        "1.0.0 read an entry without a destination as GoTo page 1 / Fit, got {:?}",
        bookmarks[0].action
    );
    assert!(matches!(
        bookmarks[1].action,
        BookmarkAction::GoTo { page: 3, .. }
    ));
}

#[test]
fn write_outline_with_none_writes_neither_dest_nor_action_and_round_trips() {
    let (mut doc, _) = doc_with_pages(2);
    let mut parent = OutlineEntry::new("Parent", None);
    parent.open = true;
    parent.children.push(OutlineEntry::new(
        "Child",
        Some(BookmarkAction::GoTo {
            page: 2,
            fit: FitType::Fit,
        }),
    ));
    let entries = vec![parent, OutlineEntry::new("Loose", None)];
    write_outline(&mut doc, &entries).unwrap();

    let dicts = item_dicts(&doc);
    assert_eq!(dicts.len(), 3);
    for (dict, has_target) in dicts.iter().zip([false, true, false]) {
        let title = dict.get(b"Title").unwrap().as_str().unwrap();
        let title = String::from_utf8_lossy(title).into_owned();
        assert_eq!(
            dict.has(b"Dest") || dict.has(b"A"),
            has_target,
            "entry {title:?}: /Dest or /A present must be {has_target}"
        );
    }

    let back = read_outline(&doc).unwrap();
    assert_eq!(back.len(), 2);
    assert!(back[0].action.is_none(), "got {:?}", back[0].action);
    assert!(back[0].open);
    assert!(matches!(
        back[0].children[0].action,
        Some(BookmarkAction::GoTo { page: 2, .. })
    ));
    assert!(back[1].action.is_none(), "got {:?}", back[1].action);
}

/// The variant set of 1.0.0, matched exhaustively from outside the crate.
///
/// Adding a variant (hidden or not) or marking the enum `#[non_exhaustive]`
/// makes this match fail to compile: both are breaking changes for a 1.x
/// crate, which is what this test is for.
fn variant_name(action: &BookmarkAction) -> &'static str {
    match action {
        BookmarkAction::GoTo { page: _, fit: _ } => "GoTo",
        BookmarkAction::GoToR { file: _, page: _ } => "GoToR",
        BookmarkAction::Uri(_) => "Uri",
        BookmarkAction::Named(_) => "Named",
    }
}

#[test]
fn bookmark_action_keeps_its_1_0_0_variant_set() {
    let all = [
        BookmarkAction::GoTo {
            page: 1,
            fit: FitType::Fit,
        },
        BookmarkAction::GoToR {
            file: "other.pdf".into(),
            page: 1,
        },
        BookmarkAction::Uri("https://example.com".into()),
        BookmarkAction::Named("NextPage".into()),
    ];
    let names: Vec<_> = all.iter().map(variant_name).collect();
    assert_eq!(names, ["GoTo", "GoToR", "Uri", "Named"]);
}
