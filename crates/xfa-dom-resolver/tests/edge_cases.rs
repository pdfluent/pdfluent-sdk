//! Edge case tests for DOM resolver and SOM parser.

use xfa_dom_resolver::data_dom::DataDom;
use xfa_dom_resolver::som::parse_som;

// =============================================================================
// SOM Parser Edge Cases
// =============================================================================

#[test]
fn som_empty_path() {
    let result = parse_som("");
    // Empty path is an error per spec
    assert!(result.is_err());
}

#[test]
fn som_very_deep_path() {
    let path = (0..50)
        .map(|i| format!("level{i}"))
        .collect::<Vec<_>>()
        .join(".");
    let result = parse_som(&path);
    assert!(result.is_ok());
}

#[test]
fn som_with_indices() {
    let result = parse_som("form1.item[0].field[*]");
    assert!(result.is_ok());
}

#[test]
fn som_single_element() {
    let result = parse_som("field");
    assert!(result.is_ok());
}

#[test]
fn som_with_dollar_prefix() {
    let result = parse_som("$data.Receipt.Tax");
    assert!(result.is_ok());
}

#[test]
fn som_with_hash_prefix() {
    let result = parse_som("#subform[1].field");
    assert!(result.is_ok());
}

// =============================================================================
// Data DOM Edge Cases
// =============================================================================

#[test]
fn data_dom_empty_xml() {
    let result = DataDom::from_xml("");
    assert!(result.is_err());
}

#[test]
fn data_dom_just_root() {
    let dom = DataDom::from_xml("<root/>").unwrap();
    assert!(dom.root().is_some());
    assert_eq!(dom.len(), 1);
}

#[test]
fn data_dom_deeply_nested() {
    let mut xml = String::new();
    for i in 0..20 {
        xml.push_str(&format!("<level{i}>"));
    }
    xml.push_str("deep");
    for i in (0..20).rev() {
        xml.push_str(&format!("</level{i}>"));
    }

    let dom = DataDom::from_xml(&xml).unwrap();
    assert!(dom.root().is_some());
    assert!(dom.len() >= 20);
}

#[test]
fn data_dom_very_long_value() {
    let long_value = "x".repeat(100_000);
    let xml = format!("<root><field>{long_value}</field></root>");
    let dom = DataDom::from_xml(&xml).unwrap();
    let root = dom.root().unwrap();
    let children = dom.children_by_name(root, "field");
    assert_eq!(children.len(), 1);
    assert_eq!(dom.value(children[0]).unwrap().len(), 100_000);
}

#[test]
fn data_dom_many_siblings() {
    let mut xml = String::from("<root>");
    for i in 0..500 {
        xml.push_str(&format!("<item{i}>val{i}</item{i}>"));
    }
    xml.push_str("</root>");

    let dom = DataDom::from_xml(&xml).unwrap();
    let root = dom.root().unwrap();
    let children = dom.children(root);
    assert_eq!(children.len(), 500);
}

#[test]
fn data_dom_xml_escaping_roundtrip() {
    let xml = r#"<form1><field>&lt;script&gt;alert('xss')&lt;/script&gt;</field></form1>"#;
    let dom = DataDom::from_xml(xml).unwrap();
    let output = dom.to_xml();
    assert!(
        !output.contains("<script>"),
        "should not produce unescaped tags"
    );
}

#[test]
fn data_dom_roundtrip_preserves_structure() {
    let xml = "<root><a><b>1</b><c>2</c></a><d>3</d></root>";
    let dom = DataDom::from_xml(xml).unwrap();
    let output = dom.to_xml();
    assert!(output.contains("<b>1</b>"));
    assert!(output.contains("<c>2</c>"));
    assert!(output.contains("<d>3</d>"));
}

#[test]
fn data_dom_with_attributes() {
    let xml = r#"<form1 version="3.3" locale="en_US">
  <field1 type="text">Value</field1>
</form1>"#;

    let dom = DataDom::from_xml(xml).unwrap();
    let root = dom.root().unwrap();
    let all_children = dom.children(root);
    assert!(!all_children.is_empty());
}

#[test]
fn data_dom_crud_operations() {
    let dom_result = DataDom::from_xml("<root><a>1</a></root>");
    assert!(dom_result.is_ok());
    let mut dom = dom_result.unwrap();
    let root = dom.root().unwrap();

    // Create
    let new_group = dom.create_group(root, "b").unwrap();
    let _new_value = dom.create_value(new_group, "c", "2").unwrap();

    // Verify
    let output = dom.to_xml();
    assert!(output.contains("<b>"));
    assert!(output.contains("<c>2</c>"));

    // Rename
    dom.rename(new_group, "renamed").unwrap();
    let output = dom.to_xml();
    assert!(output.contains("<renamed>"));
    assert!(!output.contains("<b>"));
}
