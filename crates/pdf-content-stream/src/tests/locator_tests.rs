//! Text-run locator tests.

use crate::{extract_text_runs, find_span, find_spans_containing, ContentStreamParser};

fn ops(stream: &[u8]) -> Vec<crate::ParsedOp> {
    ContentStreamParser::new(stream).collect_ops().unwrap()
}

#[test]
fn l01_find_single_span() {
    let stream = b"BT /F1 12 Tf (Hello) Tj ET";
    let ops = ops(stream);
    let loc = find_span(&ops, b"Hello", 0).expect("should find Hello");
    assert_eq!(loc.text, b"Hello");
}

#[test]
fn l02_find_second_occurrence() {
    let stream = b"BT (A) Tj (B) Tj (A) Tj ET";
    let ops = ops(stream);
    let loc0 = find_span(&ops, b"A", 0).unwrap();
    let loc1 = find_span(&ops, b"A", 1).unwrap();
    // Two different operator positions
    assert_ne!(loc0.op_index, loc1.op_index);
}

#[test]
fn l03_not_found_returns_none() {
    let stream = b"BT (Hello) Tj ET";
    let ops = ops(stream);
    assert!(find_span(&ops, b"World", 0).is_none());
}

#[test]
fn l04_state_at_location_has_font() {
    let stream = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
    let ops = ops(stream);
    let loc = find_span(&ops, b"Hello", 0).unwrap();
    assert_eq!(loc.state.text.font_name, Some(b"F1".to_vec()));
    assert_eq!(loc.state.text.font_size, 12.0);
}

#[test]
fn l05_state_at_location_has_color() {
    let stream = b"BT 1 0 0 rg /F1 12 Tf (Red) Tj ET";
    let ops = ops(stream);
    let loc = find_span(&ops, b"Red", 0).unwrap();
    assert_eq!(loc.state.fill_color, Some(crate::Color::Rgb(1.0, 0.0, 0.0)));
}

#[test]
fn l06_byte_range_is_valid() {
    let stream = b"BT /F1 12 Tf (Hello) Tj ET";
    let ops = ops(stream);
    let loc = find_span(&ops, b"Hello", 0).unwrap();
    assert!(loc.byte_end > loc.byte_start);
    assert!(loc.byte_end <= stream.len());
}

#[test]
fn l07_find_spans_containing() {
    let stream = b"BT (Hello world) Tj (Another hello) Tj ET";
    let ops = ops(stream);
    let locs = find_spans_containing(&ops, b"hello");
    // "Hello world" doesn't contain "hello" (case-sensitive), only "Another hello" does
    assert_eq!(locs.len(), 1);
    assert!(locs[0].text.windows(5).any(|w| w == b"hello"));
}

#[test]
fn l08_extract_text_runs_count() {
    let stream = b"BT (A) Tj (B) Tj (C) Tj ET";
    let ops = ops(stream);
    let runs = extract_text_runs(&ops);
    assert_eq!(runs.len(), 3);
}

#[test]
fn l09_extract_text_runs_order() {
    let stream = b"BT (first) Tj (second) Tj ET";
    let ops = ops(stream);
    let runs = extract_text_runs(&ops);
    assert_eq!(runs[0].text, b"first");
    assert_eq!(runs[1].text, b"second");
}

#[test]
fn l10_locator_works_with_tj_array() {
    let stream = b"BT [(He) -50 (llo)] TJ ET";
    let ops = ops(stream);
    // TJ concatenates strings
    let runs = extract_text_runs(&ops);
    assert_eq!(runs.len(), 1);
    // Combined text from TJ array: "Hello"
    assert_eq!(runs[0].text, b"Hello");
}

#[test]
fn l11_locator_state_correct_after_color_change() {
    // Second span should see red fill (color changed before it)
    let stream = b"BT 0 G /F1 12 Tf (grey) Tj 1 0 0 rg (red) Tj ET";
    let ops = ops(stream);
    let grey_loc = find_span(&ops, b"grey", 0).unwrap();
    let red_loc = find_span(&ops, b"red", 0).unwrap();
    assert_eq!(grey_loc.state.stroke_color, Some(crate::Color::Gray(0.0)));
    assert_eq!(
        red_loc.state.fill_color,
        Some(crate::Color::Rgb(1.0, 0.0, 0.0))
    );
}

#[test]
fn l12_find_after_save_restore() {
    let stream = b"q 1 0 0 rg q 0 1 0 rg Q BT (outer) Tj ET Q";
    let ops = ops(stream);
    let loc = find_span(&ops, b"outer", 0).unwrap();
    // After two Q's, fill should be red (first color set)
    assert_eq!(loc.state.fill_color, Some(crate::Color::Rgb(1.0, 0.0, 0.0)));
}

#[test]
fn l13_op_index_points_to_correct_op() {
    let stream = b"BT /F1 12 Tf 100 700 Td (target) Tj ET";
    let ops = ops(stream);
    let loc = find_span(&ops, b"target", 0).unwrap();
    assert!(ops[loc.op_index].op.is_text_showing());
}

#[test]
fn l14_next_line_and_show_text_locatable() {
    let stream = b"BT /F1 12 Tf (l1) ' (l2) ' ET";
    let ops = ops(stream);
    let runs = extract_text_runs(&ops);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, b"l1");
    assert_eq!(runs[1].text, b"l2");
}

#[test]
fn l15_show_text_with_params_locatable() {
    let stream = b"BT 1 2 (quoted) \" ET";
    let ops = ops(stream);
    let runs = extract_text_runs(&ops);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, b"quoted");
}
