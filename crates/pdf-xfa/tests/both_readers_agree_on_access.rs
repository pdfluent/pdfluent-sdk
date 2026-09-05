// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A template element must mean the same thing on both reading routes.
//!
//! `merger.rs` and `template_parser.rs` each carry a `parse_node_meta`, both
//! returning the same `FormNodeMeta`. They drifted: the merger read the
//! `access` attribute and the template parser left it at its default, which
//! `session.rs` resolves to `Access::Open`. A field the template declares
//! `access="readOnly"` was therefore protected on one route and editable on the
//! other, from the same file.
//!
//! That is the failure #208 is about, and it is quiet by nature -- the route
//! you happen to test reads the element, the route the customer hits does not.

use xfa_layout_engine::form::Access;

const READONLY_TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="root" layout="tb">
    <field name="locked" access="readOnly" w="80mm" h="8mm">
      <ui><textEdit multiLine="1"/></ui>
      <validate nullTest="error"/>
      <value><text>fixed</text></value>
    </field>
    <field name="open" w="80mm" h="8mm">
      <ui><textEdit/></ui>
      <value><text>editable</text></value>
    </field>
  </subform>
</template>"#;

#[test]
fn the_template_reader_honours_access_readonly() {
    let (tree, root) =
        pdf_xfa::template_parser::parse_template(READONLY_TEMPLATE, None).expect("template parses");

    let mut seen: Vec<(String, Option<Access>)> = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let node = tree.get(id);
        seen.push((node.name.clone(), tree.meta(id).access));
        stack.extend(node.children.iter().copied());
    }

    let locked = seen
        .iter()
        .find(|(name, _)| name == "locked")
        .unwrap_or_else(|| {
            panic!("the field is not in the tree at all; the fixture reads: {seen:?}")
        });
    assert_eq!(
        locked.1,
        Some(Access::ReadOnly),
        "the template declares access=\"readOnly\" and this reader returned {:?}. \
         The merger's reader has always returned ReadOnly for the same file, so a \
         protected field is editable on one route and not the other.",
        locked.1
    );

    let open = seen
        .iter()
        .find(|(name, _)| name == "open")
        .expect("the second field is missing");
    assert_eq!(
        open.1, None,
        "a field without an access attribute must stay unset, so the inherited \
         value keeps applying"
    );
}

/// `multiLine` and `nullTest="error"` drifted the same way and now come from
/// one shared reader. Same template, same struct, so the same answer.
#[test]
fn the_template_reader_honours_multiline_and_required() {
    let (tree, root) =
        pdf_xfa::template_parser::parse_template(READONLY_TEMPLATE, None).expect("template parses");

    let mut seen: Vec<(String, bool, bool)> = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let node = tree.get(id);
        let meta = tree.meta(id);
        seen.push((node.name.clone(), meta.multiline, meta.required));
        stack.extend(node.children.iter().copied());
    }

    let (_, multiline, required) = seen
        .iter()
        .find(|(name, _, _)| name == "locked")
        .unwrap_or_else(|| panic!("the field is not in the tree: {seen:?}"));
    assert!(
        *multiline,
        "the field declares <textEdit multiLine=\"1\"> and this reader returned false; \
         the merger's reader has always returned true for the same file"
    );
    assert!(
        *required,
        "the field declares <validate nullTest=\"error\"> and this reader returned false"
    );

    let (_, m_open, r_open) = seen
        .iter()
        .find(|(name, _, _)| name == "open")
        .expect("the second field is missing");
    assert!(
        !m_open && !r_open,
        "a plain field must be neither multiline nor required"
    );
}
