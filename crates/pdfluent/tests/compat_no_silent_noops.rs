// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! No silent no-ops on the compatible surface (#643).
//!
//! Every compat member that cannot do what its name says fails with the
//! shared [`pdfluent::Error::Unsupported`] reason
//! `not supported in this version: <Class>.<member>; see KNOWN_DIFFERENCES`
//! instead of returning success. One test per changed member pins the class
//! and the message, except `resolve_link` and `set_layer_ui_config`, whose
//! L34 implementations win over the raise and pin behaviour instead, and
//! `bake` / `convert_to_pdf` / `get_bboxlog` / the journal /
//! `get_displaylist` / named `resolve_link` / `layer_ui_configs` /
//! `get_highlight_selection` / `write_text` / `util_ensure_widget_calc` /
//! `repair_mono_font` / `UpdateFontInfo`, which #648/#646/#643 implement
//! (see `compat_bake.rs` / `compat_convert.rs` / `compat_bboxlog.rs` /
//! `compat_journal.rs` / `compat_displaylist.rs` /
//! `compat_resolve_link.rs` / `compat_layer_ui.rs` / `compat_highlight.rs`
//! / `compat_write_text.rs` / `compat_widget_calc.rs` /
//! `compat_font_info.rs`); the guard below fails if a compat method body
//! still matches the silent-no-op shapes without being on the allow-list.

use lopdf::{dictionary, Document, Object, Stream};

use pdfluent::prelude::*;

fn one_page_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        b"BT\n/F1 12 Tf\n100 700 Td\n(Test) Tj\nET\n".to_vec(),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => resources_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("fixture saves");
    bytes
}

fn open_doc() -> PdfDocument {
    PdfDocument::from_bytes(&one_page_pdf()).expect("fixture opens")
}

fn assert_not_supported<T>(result: std::result::Result<T, pdfluent::Error>, member: &str) {
    match result {
        Err(pdfluent::Error::Unsupported(reason)) => assert_eq!(
            reason,
            format!("not supported in this version: {member}; see KNOWN_DIFFERENCES"),
            "wrong reason for {member}",
        ),
        Err(other) => panic!("{member}: wrong error class: {other:?}"),
        Ok(_) => panic!("{member}: silent success, expected Unsupported"),
    }
}

#[test]
fn layout_fails_loudly() {
    let mut doc = open_doc();
    assert_not_supported(doc.layout(None, 0.0, 0.0, 11.0), "Document.layout");
}

#[test]
fn apply_css_fails_loudly() {
    let mut doc = open_doc();
    assert_not_supported(doc.apply_css("", true), "Document.apply_css");
}

#[test]
fn repair_fails_loudly() {
    let mut doc = open_doc();
    assert_not_supported(doc.repair(), "Document.repair");
}

#[test]
fn resolve_link_keeps_working() {
    // The L34 implementation wins over the no-op raise (#643 merge): a
    // missing URI resolves to [-1, 0, 0] and a page fragment to its index.
    use pdfluent::compat::Value;
    let doc = open_doc();
    assert_eq!(
        doc.resolve_link(None, 0).expect("none resolves"),
        Value::List(vec![Value::Int(-1), Value::Int(0), Value::Int(0)])
    );
    assert_eq!(
        doc.resolve_link(Some("#page=2"), 0)
            .expect("fragment resolves"),
        Value::List(vec![Value::Int(1), Value::Null, Value::Null])
    );
}

#[test]
fn set_layer_ui_config_keeps_working() {
    // The L34 implementation wins over the no-op raise (#643 merge): the
    // fixture has no layer interfaces, so index 0 reports a bad value.
    let mut doc = open_doc();
    let err = doc
        .set_layer_ui_config(0, 0)
        .expect_err("missing interface fails");
    assert!(
        err.to_string().contains("no layer interface 0"),
        "wrong error: {err}"
    );
}

/// Allow-listed real no-ops: (file stem, method, reason).
const REAL_NOOPS: &[(&str, &str, &str)] = &[
    (
        "document",
        "close",
        "handle holds no external resource to release",
    ),
    (
        "document",
        "exit",
        "context-manager exit with no managed resource, like close",
    ),
    (
        "document",
        "enter",
        "context-manager enter must return self by contract",
    ),
    (
        "document",
        "is_pdf",
        "handle only ever opens PDF, true is correct",
    ),
    (
        "document",
        "this_is_pdf",
        "handle only ever opens PDF, true is correct",
    ),
    (
        "document",
        "is_reflowable",
        "fixed-layout PDF never reflows, false is correct",
    ),
    (
        "document",
        "is_repaired",
        "open performs no repair pass, false is correct",
    ),
    (
        "document",
        "is_dirty",
        "facade tracks no dirty flag; the door holds the real flag",
    ),
    (
        "document",
        "name",
        "facade keeps no path; the door holds the name",
    ),
    (
        "document",
        "chapter_count",
        "single-chapter documents only, 1 is correct",
    ),
    (
        "document",
        "version_count",
        "no version stack, 1 is correct",
    ),
    (
        "document",
        "init_doc",
        "open_compat already fully opens; finishing is idempotent",
    ),
    (
        "document",
        "resolve_names",
        "destinations resolve on read; nothing to precompute",
    ),
    (
        "document",
        "refresh",
        "reads come straight from the document; nothing is cached",
    ),
    (
        "document",
        "is_wrapped",
        "fresh pages are unwrapped; recording pins false for fresh",
    ),
    (
        "document",
        "get_layout",
        "fixed-layout has no reflow layout; None is correct",
    ),
    (
        "document",
        "get_oc_items",
        "recording pins empty for pages without layers",
    ),
    (
        "edit",
        "reset",
        "owned widget clears its own value: real effect",
    ),
    (
        "edit",
        "update",
        "owned widget without a live page has nowhere to sync; live widgets use update_widget",
    ),
    ("edit", "next", "owned widget without a page has no next"),
    (
        "edit",
        "is_writable",
        "font handles are always writable in this build",
    ),
    (
        "functions",
        "ensure_operation",
        "shared borrow is live by construction; nothing to check",
    ),
    (
        "functions",
        "restore_aliases",
        "no alias registry exists; nothing to restore, like close",
    ),
    (
        "text",
        "css_for_font",
        "echoes the supplied css input; uses a parameter",
    ),
    (
        "text",
        "on_highlight_char",
        "echoes the hits input; uses a parameter",
    ),
];

/// Guard: no compat method body is a silent no-op unless allow-listed above.
#[test]
fn no_silent_noop_without_allowance() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/compat");
    let mut offenders = Vec::new();
    for stem in [
        "document",
        "edit",
        "functions",
        "text",
        "pixmap",
        "geometry",
    ] {
        let path = root.join(format!("{stem}.rs"));
        let text = std::fs::read_to_string(&path).expect("compat source reads");
        for (name, body) in split_pub_fns(&text) {
            if is_silent_noop_shape(&name, &body)
                && !REAL_NOOPS.iter().any(|(f, m, _)| *f == stem && *m == name)
            {
                offenders.push(format!("{stem}.rs::{name}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "silent no-op bodies without allow-list reason: {}",
        offenders.join(", ")
    );
    // Every allow-list entry names a real method, so the list cannot rot.
    for (stem, method, reason) in REAL_NOOPS {
        assert!(!reason.is_empty(), "{stem}::{method} needs a reason");
        let path = root.join(format!("{stem}.rs"));
        let text = std::fs::read_to_string(&path).expect("compat source reads");
        assert!(
            text.contains(&format!("pub fn {method}")),
            "allow-list names missing method {stem}::{method}"
        );
    }
}

fn split_pub_fns(text: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(name) = pub_fn_name(lines[i]) else {
            i += 1;
            continue;
        };
        let mut j = i;
        while j < lines.len() && !lines[j].contains('{') {
            j += 1;
        }
        if j >= lines.len() {
            break;
        }
        let mut depth: i32 = 0;
        let mut k = j;
        let mut closed = false;
        while k < lines.len() {
            depth += lines[k].matches('{').count() as i32;
            depth -= lines[k].matches('}').count() as i32;
            if depth == 0 {
                closed = true;
                break;
            }
            k += 1;
        }
        if !closed {
            break;
        }
        out.push((name, lines[j..=k].join("\n")));
        i = k + 1;
    }
    out
}

fn pub_fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("pub fn ")?;
    let end = rest.find('(')?;
    Some(rest[..end].trim().to_string())
}

/// The step-1 shapes: ignored inputs with a success return and no effect.
fn is_silent_noop_shape(name: &str, body: &str) -> bool {
    if name.starts_with("with_") || name == "new" || name == "default" {
        return false;
    }
    let code: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//") && *l != "{" && *l != "}")
        .collect();
    if code.len() > 6 {
        return false;
    }
    let joined = code.join("\n");
    let has_let_underscore = joined.contains("let _ =");
    let returns_ok_unit = joined.contains("Ok(())");
    let returns_ok_null = joined.contains("Ok(Value::Null)")
        || joined.contains("Ok(None)")
        || joined.contains("Ok(Vec::new())")
        || joined.contains("Ok(String::new())");
    let mentions_noop = body.contains("nothing to ") || body.contains("no-op");
    // A lone Ok(()) with ignored inputs, or an ignored-input Ok(null/empty).
    (returns_ok_unit && (has_let_underscore || mentions_noop || code.len() == 1))
        || (returns_ok_null && has_let_underscore)
}
