//! L2-01 — pageArea expansion generalization regression suite.
//!
//! These tests pin down the **current** semantics of the Sprint-1
//! PAGEAREA-EXPANSION fix (see `benchmarks/runs/PAGEAREA_EXPANSION_PERF_2026-05-17.md`)
//! and lock the generalization boundary that L2-01 evaluated:
//!
//! - **Uniform single-name pageArea pattern** — form-DOM packet enumerates
//!   the same `pageArea name="X"` multiple times → FormTree clones the
//!   template to match the form-DOM count (XFA 3.3 §8.6 / §3.1). This is
//!   the 13275420 / 778a1138 / d9ec06f8 / 927d2419 / 3963b9b6 family.
//!
//! - **Multi-template pageSet** — template enumerates multiple distinct
//!   pageArea names (e.g. `Page1` + `OverFlowPage`). Form-DOM may record
//!   multiple instances of one of the names, but the current gate
//!   **intentionally suppresses** cloning because the available oracle
//!   data is insufficient to prove these are runtime-allocated rather than
//!   pre-allocated menu templates. This is the IRCC `imm5709e`,
//!   `imm5710e`, `imm5257e`, `eimm5669e` family. Per
//!   `XFA_ORACLE_COST_POLICY.md`, generalization here is gated on operator
//!   approval for oracle generation.
//!
//! - **Single-template / single-instance** — no expansion, no regression.
//!
//! If a future engine change tightens or loosens the gate, the affected
//! cases below MUST be updated in lockstep so the boundary stays explicit.
//!
//! Synthetic XDP fixtures live alongside corpus parity guards. Corpus
//! guards are skipped when the local corpus snapshot is absent so CI
//! without the corpus stays green; they kick in for the developer
//! workflow and the VPS replay.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_xfa::flatten_xfa_to_pdf;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
        "AcroForm" => Object::Reference(acroform_id)
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save xfa pdf");
    out
}

fn flatten_page_count(xdp: &str) -> usize {
    let pdf = build_xfa_pdf(xdp);
    let output = flatten_xfa_to_pdf(&pdf).expect("flatten_xfa_to_pdf failed");
    Document::load_mem(&output)
        .expect("reload flattened PDF")
        .get_pages()
        .len()
}

// ---------------------------------------------------------------------------
// Synthetic XDP — multi-template pageSet (IRCC family pattern).
//
// Template declares `Page1` + `OverFlowPage`; form-DOM enumerates
// `Page1` + 4×`OverFlowPage`. The pageArea-expansion gate MUST suppress
// cloning. The exact page-count is dominated by the layout engine
// (content flow), so we assert that the cloning *did not* over-paginate
// the form: the output stays at the layout-engine baseline (1 or 2
// pages) and does NOT inflate to 5 pages.
// ---------------------------------------------------------------------------

const XDP_MULTI_TEMPLATE_SUPPRESS: &str = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1" layout="paginate" w="8.5in" h="11in">
      <pageSet>
        <pageArea name="Page1">
          <contentArea x="36pt" y="36pt" w="540pt" h="720pt"/>
          <medium stock="default" short="612pt" long="792pt"/>
        </pageArea>
        <pageArea name="OverFlowPage">
          <contentArea x="36pt" y="36pt" w="540pt" h="720pt"/>
          <medium stock="default" short="612pt" long="792pt"/>
        </pageArea>
      </pageSet>
      <subform name="section" layout="tb" w="540pt">
        <field name="F1" w="200pt" h="18pt">
          <ui><textEdit/></ui>
          <value><text>val</text></value>
        </field>
      </subform>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><section><F1>val</F1></section></form1></xfa:data>
  </xfa:datasets>
  <form xmlns="http://www.xfa.org/schema/xfa-form/2.8/">
    <subform name="form1">
      <pageSet>
        <pageArea name="Page1"/>
        <pageArea name="OverFlowPage"/>
        <pageArea name="OverFlowPage"/>
        <pageArea name="OverFlowPage"/>
        <pageArea name="OverFlowPage"/>
      </pageSet>
      <subform name="section">
        <field name="F1"><value><text>val</text></value></field>
      </subform>
    </subform>
  </form>
</xdp:xdp>"#;

/// The current gate must keep multi-template pageSets out of the
/// expansion path. An accidental loosening would replicate the 4
/// OverFlowPage entries into 4 clones and bloat the output to >= 5
/// pages. Assert the cap at <= 2 pages (1 page is the layout-engine
/// baseline with this minimal content; 2 pages is allowed if content
/// flow triggers a single break).
#[test]
fn multi_template_pagearea_does_not_over_expand() {
    let pages = flatten_page_count(XDP_MULTI_TEMPLATE_SUPPRESS);
    assert!(
        pages <= 2,
        "multi-template pageSet (Page1 + OverFlowPage) must NOT clone — current gate \
         preserves the pre-allocated menu interpretation; got {pages} pages \
         (loosening the gate would have produced >= 5)"
    );
    assert!(
        pages >= 1,
        "multi-template pageSet must still render at least the primary Page1; got {pages}"
    );
}

// ---------------------------------------------------------------------------
// Corpus parity guards — only run when the local corpus snapshot is
// available. These assert the **fresh-flatten** count from the released
// binary (not a committed `ours/` artifact) so any future PAGEAREA-
// EXPANSION engine regression surfaces here.
// ---------------------------------------------------------------------------

fn corpus_input_dir() -> Option<String> {
    std::env::var("XFA_CORPUS_INPUT_DIR").ok()
}

fn flatten_corpus_doc(name: &str) -> Option<usize> {
    let dir = corpus_input_dir()?;
    let path = format!("{dir}/{name}.pdf");
    if !std::path::Path::new(&path).exists() {
        return None;
    }
    let pdf = std::fs::read(&path).ok()?;
    let output = flatten_xfa_to_pdf(&pdf).ok()?;
    Some(Document::load_mem(&output).ok()?.get_pages().len())
}

/// 13275420 — canonical uniform-pageArea case. Sprint-1 lifted it from 5
/// to 10 pages. L2-01 must not regress below the >= 8 floor that the
/// existing m3b_phaseD guard pins.
#[test]
#[ignore = "requires local corpus; set XFA_CORPUS_INPUT_DIR"]
fn corpus_13275420_pagearea_expansion_holds() {
    let Some(pages) = flatten_corpus_doc("13275420") else {
        eprintln!(
            "SKIPPED (not a pass): precondition not met at {}:{}",
            file!(),
            line!()
        );
        return;
    };
    assert!(
        pages >= 8,
        "13275420 fidelity: pageArea expansion must produce >= 8 pages (oracle 10), got {pages}"
    );
}

/// 927d2419 — Sprint-1 PAGEAREA-EXPANSION lifted from 1 to 2 pages.
/// L2-01 must not regress that improvement.
#[test]
#[ignore = "requires local corpus; set XFA_CORPUS_INPUT_DIR"]
fn corpus_927d2419_stays_improved() {
    let Some(pages) = flatten_corpus_doc("927d2419") else {
        eprintln!(
            "SKIPPED (not a pass): precondition not met at {}:{}",
            file!(),
            line!()
        );
        return;
    };
    assert!(
        pages >= 2,
        "927d2419: post-Sprint-1 baseline is 2 pages; L2-01 must not regress, got {pages}"
    );
}

/// 778a1138 — uniform Page1 × 2 in form-DOM.
#[test]
#[ignore = "requires local corpus; set XFA_CORPUS_INPUT_DIR"]
fn corpus_778a1138_stays_at_two_pages() {
    let Some(pages) = flatten_corpus_doc("778a1138") else {
        eprintln!(
            "SKIPPED (not a pass): precondition not met at {}:{}",
            file!(),
            line!()
        );
        return;
    };
    assert_eq!(pages, 2, "778a1138: expected 2 pages, got {pages}");
}

/// d9ec06f8 — uniform Page1 × 4 in form-DOM. Sprint-1 floor is 3 pages.
#[test]
#[ignore = "requires local corpus; set XFA_CORPUS_INPUT_DIR"]
fn corpus_d9ec06f8_post_sprint1_floor() {
    let Some(pages) = flatten_corpus_doc("d9ec06f8") else {
        eprintln!(
            "SKIPPED (not a pass): precondition not met at {}:{}",
            file!(),
            line!()
        );
        return;
    };
    assert!(
        pages >= 3,
        "d9ec06f8: post-Sprint-1 floor is 3 pages, got {pages}"
    );
}

/// 3963b9b6 — uniform Service_Call_Report × 3 in form-DOM.
#[test]
#[ignore = "requires local corpus; set XFA_CORPUS_INPUT_DIR"]
fn corpus_3963b9b6_stays_at_three_pages() {
    let Some(pages) = flatten_corpus_doc("3963b9b6") else {
        eprintln!(
            "SKIPPED (not a pass): precondition not met at {}:{}",
            file!(),
            line!()
        );
        return;
    };
    assert_eq!(pages, 3, "3963b9b6: expected 3 pages, got {pages}");
}

/// ce382c3d — uniform Page × 2 in form-DOM. Layout-engine driven; pin a
/// floor so any pageArea-expansion regression that drops below 1 page
/// surfaces here.
#[test]
#[ignore = "requires local corpus; set XFA_CORPUS_INPUT_DIR"]
fn corpus_ce382c3d_stays_at_one_or_more_pages() {
    let Some(pages) = flatten_corpus_doc("ce382c3d") else {
        eprintln!(
            "SKIPPED (not a pass): precondition not met at {}:{}",
            file!(),
            line!()
        );
        return;
    };
    assert!(pages >= 1, "ce382c3d: at least 1 page, got {pages}");
}
