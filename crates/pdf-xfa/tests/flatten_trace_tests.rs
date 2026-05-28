//! Phase 2 (DYNAMIC_XFA_REFLOW_TRACEABILITY_AND_FIX): tests for the env-gated
//! flatten trace.
//!
//! Verifies: (1) default OFF produces no trace, (2) `XFA_FLATTEN_TRACE=1`
//! produces a JSON trace file, (3) the JSON is well-formed and carries the
//! expected stage objects + basic counts.
//!
//! All env manipulation lives in a SINGLE test function so the parallel test
//! runner cannot race on the process-global env vars.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_xfa::flatten_xfa_to_pdf;

fn build_xfa_pdf(xdp: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");
    let xdp_bytes = xdp.as_bytes().to_vec();
    let xfa_stream = Stream::new(
        dictionary! { "Length" => Object::Integer(xdp_bytes.len() as i64) },
        xdp_bytes,
    );
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length" => Object::Integer(0_i64) },
        vec![],
    )));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type"     => Object::Name(b"Page".to_vec()),
        "Parent"   => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(content_id)
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type"  => Object::Name(b"Pages".to_vec()),
            "Kids"  => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1)
        }),
    );
    let acroform_id = doc.add_object(Object::Dictionary(dictionary! {
        "XFA"    => Object::Reference(xfa_id),
        "Fields" => Object::Array(vec![])
    }));
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type"     => Object::Name(b"Catalog".to_vec()),
        "Pages"    => Object::Reference(pages_id),
        "AcroForm" => Object::Reference(acroform_id),
        "NeedsRendering" => Object::Boolean(true)
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save xfa pdf");
    out
}

const XDP: &str = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet><pageArea name="P1"><contentArea x="10mm" y="10mm" w="190mm" h="270mm"/></pageArea></pageSet>
    <draw name="header" w="180mm" h="10mm"><value><text>Static Section Header</text></value></draw>
    <field name="naam" w="80mm" h="8mm"><caption><value><text>Naam:</text></value></caption>
      <ui><textEdit/></ui><value><text>Jansen</text></value></field>
    <field name="leeg" w="80mm" h="8mm"><caption><value><text>Leeg veld:</text></value></caption>
      <ui><textEdit/></ui></field>
    <draw name="footer" w="180mm" h="8mm"><value><text>Footer text</text></value></draw>
  </subform>
</template>
</xdp:xdp>"#;

/// Fixture with a `presence="hidden"` subform whose draws must be pruned.
const XDP_HIDDEN: &str = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet><pageArea name="P1"><contentArea x="10mm" y="10mm" w="190mm" h="270mm"/></pageArea></pageSet>
    <draw name="visible_header" w="180mm" h="10mm"><value><text>Visible Header</text></value></draw>
    <subform name="secret" presence="hidden" layout="tb">
      <draw name="h1" w="180mm" h="8mm"><value><text>Hidden Section A</text></value></draw>
      <draw name="h2" w="180mm" h="8mm"><value><text>Hidden Section B</text></value></draw>
      <field name="hf" w="80mm" h="8mm"><caption><value><text>Hidden field:</text></value></caption><ui><textEdit/></ui></field>
    </subform>
  </subform>
</template>
</xdp:xdp>"#;

/// Minimal balanced-brace + key-presence JSON sanity check (no serde dep).
fn json_is_wellformed(s: &str) -> bool {
    let s = s.trim();
    if !s.starts_with('{') || !s.ends_with('}') {
        return false;
    }
    let mut depth: i64 = 0;
    let mut in_str = false;
    let mut esc = false;
    for ch in s.chars() {
        if in_str {
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0 && !in_str
}

/// Extract an integer value for a `"key":<int>` pair (first match).
fn json_int(s: &str, key: &str) -> Option<i64> {
    let pat = format!("\"{key}\":");
    let idx = s.find(&pat)? + pat.len();
    let rest = &s[idx..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '-')
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

#[test]
fn flatten_trace_env_gated_and_wellformed() {
    let pdf = build_xfa_pdf(XDP);
    let dir = std::env::temp_dir();
    let off_path = dir.join(format!("xfa_trace_off_{}.json", std::process::id()));
    let on_path = dir.join(format!("xfa_trace_on_{}.json", std::process::id()));
    let _ = std::fs::remove_file(&off_path);
    let _ = std::fs::remove_file(&on_path);

    // (1) Default OFF: trace var unset -> no file produced even if PATH is set.
    std::env::remove_var("XFA_FLATTEN_TRACE");
    std::env::set_var("XFA_FLATTEN_TRACE_PATH", &off_path);
    let _ = flatten_xfa_to_pdf(&pdf).expect("flatten off");
    assert!(
        !off_path.exists(),
        "trace must NOT be written when XFA_FLATTEN_TRACE is unset"
    );

    // (2) ON: produces a JSON trace file.
    std::env::set_var("XFA_FLATTEN_TRACE", "1");
    std::env::set_var("XFA_FLATTEN_TRACE_PATH", &on_path);
    let _ = flatten_xfa_to_pdf(&pdf).expect("flatten on");
    assert!(on_path.exists(), "trace file must be produced when enabled");
    let body = std::fs::read_to_string(&on_path).expect("read trace");

    // (3) Well-formed + carries stage objects + basic counts.
    assert!(
        json_is_wellformed(&body),
        "trace JSON must be well-formed: {body}"
    );
    for key in [
        "\"schema_version\"",
        "\"parse\"",
        "\"bind\"",
        "\"script\"",
        "\"layout\"",
        "\"paint\"",
        "\"writer\"",
        "\"stage_first_divergence_hint\"",
    ] {
        assert!(body.contains(key), "trace JSON missing {key}: {body}");
    }
    // The template has 2 draws-with-text (header + footer) and 2 fields.
    assert_eq!(json_int(&body, "draws_with_text"), Some(2), "body={body}");
    assert_eq!(json_int(&body, "fields"), Some(2), "body={body}");
    // Structural XFA removal must be confirmed by the writer stage.
    assert!(
        body.contains("\"xfa_removed_structural\":true"),
        "body={body}"
    );
    assert!(body.contains("\"acroform_removed\":true"), "body={body}");

    // Per-page suppression diagnostics must be present with a decision.
    assert!(body.contains("\"suppression\":["), "body={body}");
    assert!(
        body.contains("\"keep\":true") && body.contains("\"reason\":\"single_page\""),
        "single-page form must record a single_page keep decision: {body}"
    );

    // Runtime-instance trace: repeating-subforms array present (empty for this
    // non-repeating fixture).
    assert!(body.contains("\"repeating_subforms\":["), "body={body}");

    // Layout provenance fields must be present; a fully-visible single page is
    // never a provenance-safe drop.
    assert!(body.contains("\"page_reason\":"), "body={body}");
    assert!(body.contains("\"under_repeating_subform\":"), "body={body}");
    assert!(body.contains("\"data_bound_nodes_count\":"), "body={body}");
    assert!(body.contains("\"provenance_confidence\":"), "body={body}");
    assert!(
        body.contains("\"suppression_safe_to_drop\":false"),
        "a visible single page must not be provenance-safe to drop: {body}"
    );
    assert!(
        !body.contains("\"suppression_safe_to_drop\":true"),
        "no page in a fully-visible form may be flagged safe-to-drop: {body}"
    );

    // No unexpected static-text drop: this form is fully visible, so nothing is
    // pruned and every text-draw is "visible".
    assert_eq!(
        json_int(&body, "nodes_under_hidden"),
        Some(0),
        "body={body}"
    );
    assert_eq!(
        json_int(&body, "visible_draws_with_text"),
        Some(2),
        "body={body}"
    );
    // The visible static draw text must survive into layout.
    assert!(
        json_int(&body, "layout_total_chars").unwrap_or(0) > 0,
        "body={body}"
    );

    // (4) Classifier: a presence="hidden" subtree must be reported as pruned.
    let hidden_pdf = build_xfa_pdf(XDP_HIDDEN);
    std::env::set_var("XFA_FLATTEN_TRACE_PATH", &on_path);
    let _ = flatten_xfa_to_pdf(&hidden_pdf).expect("flatten hidden");
    let hbody = std::fs::read_to_string(&on_path).expect("read hidden trace");
    assert!(
        json_is_wellformed(&hbody),
        "hidden trace must be well-formed: {hbody}"
    );
    assert!(
        json_int(&hbody, "nodes_under_hidden").unwrap_or(0) > 0,
        "hidden subtree must be counted as pruned: {hbody}"
    );

    // Cleanup env so other test binaries are unaffected (best-effort).
    std::env::remove_var("XFA_FLATTEN_TRACE");
    std::env::remove_var("XFA_FLATTEN_TRACE_PATH");
    let _ = std::fs::remove_file(&off_path);
    let _ = std::fs::remove_file(&on_path);
}
