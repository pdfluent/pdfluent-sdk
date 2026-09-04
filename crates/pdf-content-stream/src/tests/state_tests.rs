//! State machine tests — verifies correct state tracking through G3 operators.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::{
    identity_matrix, matrix_concat, matrix_translate, Color, ContentOp, ContentStateMachine,
    ContentStreamParser,
};

fn state_after(stream: &[u8]) -> ContentStateMachine {
    let ops = ContentStreamParser::new(stream).collect_ops().unwrap();
    let mut machine = ContentStateMachine::new();
    for op in &ops {
        machine.apply(&op.op);
    }
    machine
}

// ── Font tracking ─────────────────────────────────────────────────────────────

#[test]
fn s01_font_name_tracked() {
    let machine = state_after(b"BT /F1 12 Tf ET");
    assert_eq!(machine.state().text.font_name, Some(b"F1".to_vec()));
    assert_eq!(machine.state().text.font_size, 12.0);
}

#[test]
fn s02_font_updated_on_second_tf() {
    let machine = state_after(b"BT /F1 10 Tf /F2 14 Tf ET");
    assert_eq!(machine.state().text.font_name, Some(b"F2".to_vec()));
    assert_eq!(machine.state().text.font_size, 14.0);
}

#[test]
fn s03_no_font_before_tf() {
    let machine = state_after(b"");
    assert_eq!(machine.state().text.font_name, None);
}

// ── Fill color tracking ───────────────────────────────────────────────────────

#[test]
fn s04_fill_rgb() {
    let machine = state_after(b"1 0 0 rg");
    assert_eq!(machine.state().fill_color, Some(Color::Rgb(1.0, 0.0, 0.0)));
}

#[test]
fn s05_fill_gray() {
    let machine = state_after(b"0.5 g");
    assert_eq!(machine.state().fill_color, Some(Color::Gray(0.5)));
}

#[test]
fn s06_fill_cmyk() {
    let machine = state_after(b"0 0 0 1 k");
    assert_eq!(
        machine.state().fill_color,
        Some(Color::Cmyk(0.0, 0.0, 0.0, 1.0))
    );
}

#[test]
fn s07_fill_color_updates() {
    let machine = state_after(b"1 0 0 rg 0.5 g");
    // Last color wins
    assert_eq!(machine.state().fill_color, Some(Color::Gray(0.5)));
}

// ── Stroke color tracking ─────────────────────────────────────────────────────

#[test]
fn s08_stroke_rgb() {
    let machine = state_after(b"0 1 0 RG");
    assert_eq!(
        machine.state().stroke_color,
        Some(Color::Rgb(0.0, 1.0, 0.0))
    );
}

#[test]
fn s09_stroke_gray() {
    let machine = state_after(b"0 G");
    assert_eq!(machine.state().stroke_color, Some(Color::Gray(0.0)));
}

#[test]
fn s10_stroke_cmyk() {
    let machine = state_after(b"0 0 0 1 K");
    assert_eq!(
        machine.state().stroke_color,
        Some(Color::Cmyk(0.0, 0.0, 0.0, 1.0))
    );
}

// ── Graphics state stack ──────────────────────────────────────────────────────

#[test]
fn s11_save_restore_color() {
    let machine = state_after(b"1 0 0 rg q 0 1 0 rg Q");
    // After Q, should restore to red fill
    assert_eq!(machine.state().fill_color, Some(Color::Rgb(1.0, 0.0, 0.0)));
}

#[test]
fn s12_save_restore_font() {
    let machine = state_after(b"/F1 12 Tf q /F2 14 Tf Q");
    // After Q, should restore to F1/12
    assert_eq!(machine.state().text.font_name, Some(b"F1".to_vec()));
    assert_eq!(machine.state().text.font_size, 12.0);
}

#[test]
fn s13_nested_save_restore() {
    let machine = state_after(b"1 0 0 rg q 0 1 0 rg q 0 0 1 rg Q Q");
    // After two Q's, back to red
    assert_eq!(machine.state().fill_color, Some(Color::Rgb(1.0, 0.0, 0.0)));
}

#[test]
fn s14_restore_without_save_is_no_op() {
    // Extra Q with empty stack should not panic
    let machine = state_after(b"1 0 0 rg Q");
    // Color stays (unmatched Q is ignored)
    assert_eq!(machine.state().fill_color, Some(Color::Rgb(1.0, 0.0, 0.0)));
}

// ── Text state parameters ─────────────────────────────────────────────────────

#[test]
fn s15_char_spacing() {
    let machine = state_after(b"0.5 Tc");
    assert_eq!(machine.state().text.char_spacing, 0.5);
}

#[test]
fn s16_word_spacing() {
    let machine = state_after(b"2 Tw");
    assert_eq!(machine.state().text.word_spacing, 2.0);
}

#[test]
fn s17_horizontal_scale() {
    let machine = state_after(b"80 Tz");
    assert_eq!(machine.state().text.horizontal_scale, 80.0);
}

#[test]
fn s18_leading() {
    let machine = state_after(b"14 TL");
    assert_eq!(machine.state().text.leading, 14.0);
}

#[test]
fn s19_rise() {
    let machine = state_after(b"3 Ts");
    assert_eq!(machine.state().text.rise, 3.0);
}

#[test]
fn s20_default_horizontal_scale_is_100() {
    let machine = ContentStateMachine::new();
    assert_eq!(machine.state().text.horizontal_scale, 100.0);
}

// ── Text matrix / position ────────────────────────────────────────────────────

#[test]
fn s21_set_text_matrix() {
    let machine = state_after(b"BT 1 0 0 1 100 200 Tm ET");
    // After ET, text matrix is still set (it's not reset on ET in our model)
    assert_eq!(machine.state().text.text_matrix[4], 100.0);
    assert_eq!(machine.state().text.text_matrix[5], 200.0);
}

#[test]
fn s22_bt_resets_text_matrix() {
    let machine = state_after(b"1 0 0 1 100 200 Tm BT");
    // BT resets Tm to identity
    assert_eq!(machine.state().text.text_matrix, identity_matrix());
}

#[test]
fn s23_td_updates_position() {
    let machine = state_after(b"BT 1 0 0 1 0 0 Tm 100 200 Td ET");
    // After Tm at origin, Td(100,200) should move to (100, 200)
    let (x, y) = crate::matrix_origin(&machine.state().text.text_matrix);
    assert!((x - 100.0).abs() < 0.01, "x={x}");
    assert!((y - 200.0).abs() < 0.01, "y={y}");
}

#[test]
fn s24_td_negative() {
    let machine = state_after(b"BT 1 0 0 1 100 200 Tm 0 -14 Td ET");
    let (x, y) = crate::matrix_origin(&machine.state().text.text_matrix);
    assert!((x - 100.0).abs() < 0.01);
    assert!((y - 186.0).abs() < 0.01, "y={y}");
}

#[test]
fn s25_td_updates_both_matrices() {
    let machine = state_after(b"BT 0 -12 Td ET");
    let tm = machine.state().text.text_matrix;
    let tlm = machine.state().text.text_line_matrix;
    assert_eq!(tm, tlm, "Tm and Tlm should be equal after Td");
}

#[test]
fn s26_large_td_updates_matrices() {
    let machine = state_after(b"BT 1 0 0 1 0 700 Tm 0 -12 TD ET");
    // TD also sets leading = -(-12) = 12
    assert_eq!(machine.state().text.leading, 12.0);
}

#[test]
fn s27_tstar_uses_leading() {
    let machine = state_after(b"BT 1 0 0 1 0 700 Tm 14 TL T* ET");
    let (_, y) = crate::matrix_origin(&machine.state().text.text_matrix);
    // T* moves by -leading = -14
    assert!((y - 686.0).abs() < 0.01, "y={y}");
}

// ── Text block tracking ───────────────────────────────────────────────────────

#[test]
fn s28_in_text_block_tracking() {
    let ops = ContentStreamParser::new(b"BT (x) Tj ET")
        .collect_ops()
        .unwrap();
    let mut machine = ContentStateMachine::new();

    machine.apply(&ops[0].op); // BT
    assert!(machine.in_text_block);
    machine.apply(&ops[1].op); // Tf / ShowText
    machine.apply(&ops[2].op); // ET
    assert!(!machine.in_text_block);
}

// ── CTM tracking ──────────────────────────────────────────────────────────────

#[test]
fn s29_ctm_starts_identity() {
    let machine = ContentStateMachine::new();
    assert_eq!(machine.state().ctm, identity_matrix());
}

#[test]
fn s30_ctm_updated_by_cm() {
    let machine = state_after(b"1 0 0 1 100 200 cm");
    assert_eq!(machine.state().ctm[4], 100.0);
    assert_eq!(machine.state().ctm[5], 200.0);
}

#[test]
fn s31_ctm_save_restore() {
    let machine = state_after(b"1 0 0 1 100 200 cm q 1 0 0 1 0 0 cm Q");
    // After Q, CTM restored to the (100,200) translation
    assert_eq!(machine.state().ctm[4], 100.0);
}

// ── Color.to_rgb conversion ───────────────────────────────────────────────────

#[test]
fn s32_color_gray_to_rgb() {
    let c = Color::Gray(0.5);
    let rgb = c.to_rgb();
    assert_eq!(rgb, [0.5, 0.5, 0.5]);
}

#[test]
fn s33_color_cmyk_to_rgb() {
    let c = Color::Cmyk(0.0, 0.0, 0.0, 0.0); // white
    let rgb = c.to_rgb();
    assert_eq!(rgb, [1.0, 1.0, 1.0]);
}

#[test]
fn s34_color_cmyk_black_to_rgb() {
    let c = Color::Cmyk(0.0, 0.0, 0.0, 1.0); // black
    let rgb = c.to_rgb();
    assert_eq!(rgb, [0.0, 0.0, 0.0]);
}

// ── Matrix helpers ────────────────────────────────────────────────────────────

#[test]
fn s35_identity_concat_is_identity() {
    let id = identity_matrix();
    assert_eq!(matrix_concat(&id, &id), id);
}

#[test]
fn s36_translate_adds_correctly() {
    let id = identity_matrix();
    let t = matrix_translate(&id, 100.0, 200.0);
    assert_eq!(t[4], 100.0);
    assert_eq!(t[5], 200.0);
}

#[test]
fn s37_state_machine_apply_all() {
    let ops = vec![
        ContentOp::SaveState,
        ContentOp::SetFillRgb(1.0, 0.0, 0.0),
        ContentOp::RestoreState,
    ];
    let mut machine = ContentStateMachine::new();
    machine.apply_all(&ops);
    // After save/set/restore, fill color should be None (initial state)
    assert_eq!(machine.state().fill_color, None);
}
