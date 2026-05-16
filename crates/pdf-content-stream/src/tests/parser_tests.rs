//! Parser unit tests — covers the G3 operator list and round-trip guarantee.

use crate::{
    serialize, verify_round_trip, ContentOp, ContentStreamError, ContentStreamParser, RawOperand,
    TjItem,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_ok(input: &[u8]) -> Vec<ContentOp> {
    ContentStreamParser::new(input)
        .collect_ops()
        .unwrap_or_else(|e| panic!("parse failed on {:?}: {e}", std::str::from_utf8(input)))
        .into_iter()
        .map(|p| p.op)
        .collect()
}

fn parse_ops(input: &[u8]) -> Vec<crate::ParsedOp> {
    ContentStreamParser::new(input).collect_ops().unwrap()
}

fn parse_err(input: &[u8]) -> ContentStreamError {
    ContentStreamParser::new(input)
        .collect_ops()
        .expect_err("expected parse error")
}

fn round_trips(input: &[u8]) {
    let ops = parse_ops(input);
    assert!(
        verify_round_trip(input, &ops),
        "round-trip failed for: {:?}",
        std::str::from_utf8(input)
    );
}

// ── Text block operators ──────────────────────────────────────────────────────

#[test]
fn t01_begin_end_text() {
    let ops = parse_ok(b"BT ET");
    assert_eq!(ops, [ContentOp::BeginText, ContentOp::EndText]);
}

#[test]
fn t02_begin_end_text_round_trip() {
    round_trips(b"BT ET");
}

// ── Font ──────────────────────────────────────────────────────────────────────

#[test]
fn t03_set_font_basic() {
    let ops = parse_ok(b"/F1 12 Tf");
    assert_eq!(
        ops,
        [ContentOp::SetFont {
            name: b"F1".to_vec(),
            size: 12.0
        }]
    );
}

#[test]
fn t04_set_font_fractional_size() {
    let ops = parse_ok(b"/Helvetica 10.5 Tf");
    assert_eq!(
        ops,
        [ContentOp::SetFont {
            name: b"Helvetica".to_vec(),
            size: 10.5
        }]
    );
}

#[test]
fn t05_set_font_round_trip() {
    round_trips(b"/F1 12 Tf");
}

// ── Show text ─────────────────────────────────────────────────────────────────

#[test]
fn t06_show_text_tj() {
    let ops = parse_ok(b"(Hello) Tj");
    assert_eq!(ops, [ContentOp::ShowText(b"Hello".to_vec())]);
}

#[test]
fn t07_show_text_hex_string() {
    // "AB" in hex
    let ops = parse_ok(b"<4142> Tj");
    assert_eq!(ops, [ContentOp::ShowText(vec![0x41, 0x42])]);
}

#[test]
fn t08_show_texts_tj_array_mixed() {
    let ops = parse_ok(b"[(Hel) -100 (lo)] TJ");
    let expected_items = vec![
        TjItem::Text(b"Hel".to_vec()),
        TjItem::Kern(-100.0),
        TjItem::Text(b"lo".to_vec()),
    ];
    assert_eq!(ops, [ContentOp::ShowTexts(expected_items)]);
}

#[test]
fn t09_next_line_and_show_text() {
    let ops = parse_ok(b"(Line) '");
    assert_eq!(ops, [ContentOp::NextLineAndShowText(b"Line".to_vec())]);
}

#[test]
fn t10_show_text_with_params() {
    let ops = parse_ok(b"2 3 (Text) \"");
    assert_eq!(
        ops,
        [ContentOp::ShowTextWithParams {
            aw: 2.0,
            ac: 3.0,
            text: b"Text".to_vec()
        }]
    );
}

// ── Text position ─────────────────────────────────────────────────────────────

#[test]
fn t11_move_text() {
    let ops = parse_ok(b"100 700 Td");
    assert_eq!(ops, [ContentOp::MoveText(100.0, 700.0)]);
}

#[test]
fn t12_move_text_and_leading() {
    let ops = parse_ok(b"0 -14 TD");
    assert_eq!(ops, [ContentOp::MoveTextAndSetLeading(0.0, -14.0)]);
}

#[test]
fn t13_next_line() {
    let ops = parse_ok(b"T*");
    assert_eq!(ops, [ContentOp::NextLine]);
}

#[test]
fn t14_set_text_matrix() {
    let ops = parse_ok(b"1 0 0 1 72 720 Tm");
    assert_eq!(
        ops,
        [ContentOp::SetTextMatrix([1.0, 0.0, 0.0, 1.0, 72.0, 720.0])]
    );
}

// ── Text state parameters ─────────────────────────────────────────────────────

#[test]
fn t15_char_spacing() {
    let ops = parse_ok(b"0.5 Tc");
    assert_eq!(ops, [ContentOp::SetCharSpacing(0.5)]);
}

#[test]
fn t16_word_spacing() {
    let ops = parse_ok(b"2 Tw");
    assert_eq!(ops, [ContentOp::SetWordSpacing(2.0)]);
}

#[test]
fn t17_horizontal_scale() {
    let ops = parse_ok(b"80 Tz");
    assert_eq!(ops, [ContentOp::SetHorizontalScale(80.0)]);
}

#[test]
fn t18_leading() {
    let ops = parse_ok(b"14 TL");
    assert_eq!(ops, [ContentOp::SetLeading(14.0)]);
}

#[test]
fn t19_rise() {
    let ops = parse_ok(b"3 Ts");
    assert_eq!(ops, [ContentOp::SetRise(3.0)]);
}

// ── Fill color ────────────────────────────────────────────────────────────────

#[test]
fn t20_fill_rgb() {
    let ops = parse_ok(b"1 0 0 rg");
    assert_eq!(ops, [ContentOp::SetFillRgb(1.0, 0.0, 0.0)]);
}

#[test]
fn t21_fill_gray() {
    let ops = parse_ok(b"0.5 g");
    assert_eq!(ops, [ContentOp::SetFillGray(0.5)]);
}

#[test]
fn t22_fill_cmyk() {
    let ops = parse_ok(b"0.1 0.2 0.3 0.4 k");
    assert_eq!(ops, [ContentOp::SetFillCmyk(0.1, 0.2, 0.3, 0.4)]);
}

// ── Stroke color ──────────────────────────────────────────────────────────────

#[test]
fn t23_stroke_rgb() {
    let ops = parse_ok(b"0 1 0 RG");
    assert_eq!(ops, [ContentOp::SetStrokeRgb(0.0, 1.0, 0.0)]);
}

#[test]
fn t24_stroke_gray() {
    let ops = parse_ok(b"0 G");
    assert_eq!(ops, [ContentOp::SetStrokeGray(0.0)]);
}

#[test]
fn t25_stroke_cmyk() {
    let ops = parse_ok(b"0 0 0 1 K");
    assert_eq!(ops, [ContentOp::SetStrokeCmyk(0.0, 0.0, 0.0, 1.0)]);
}

// ── Graphics state stack ──────────────────────────────────────────────────────

#[test]
fn t26_save_restore() {
    let ops = parse_ok(b"q Q");
    assert_eq!(ops, [ContentOp::SaveState, ContentOp::RestoreState]);
}

#[test]
fn t27_nested_save_restore() {
    let ops = parse_ok(b"q q Q Q");
    assert_eq!(
        ops,
        [
            ContentOp::SaveState,
            ContentOp::SaveState,
            ContentOp::RestoreState,
            ContentOp::RestoreState,
        ]
    );
}

// ── Transform ─────────────────────────────────────────────────────────────────

#[test]
fn t28_transform() {
    let ops = parse_ok(b"1 0 0 1 0 0 cm");
    assert_eq!(ops, [ContentOp::Transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])]);
}

// ── Pass-through (Other) ──────────────────────────────────────────────────────

#[test]
fn t29_other_path_ops_pass_through() {
    let ops = parse_ok(b"0 0 m 100 0 l h f");
    // m, l, h, f are path operators — all pass through as Other
    assert!(ops.iter().all(|op| matches!(op, ContentOp::Other { .. })));
}

#[test]
fn t30_other_preserves_operands() {
    let ops = parse_ok(b"0 0 100 100 re");
    assert_eq!(
        ops,
        [ContentOp::Other {
            name: b"re".to_vec(),
            operands: vec![
                RawOperand::Number(0.0),
                RawOperand::Number(0.0),
                RawOperand::Number(100.0),
                RawOperand::Number(100.0),
            ],
        }]
    );
}

// ── Comments ──────────────────────────────────────────────────────────────────

#[test]
fn t31_comments_skipped() {
    let ops = parse_ok(b"% This is a comment\nBT ET");
    assert_eq!(ops, [ContentOp::BeginText, ContentOp::EndText]);
}

#[test]
fn t32_comments_round_trip() {
    round_trips(b"% comment\nBT ET % inline\n");
}

// ── Whitespace variants ───────────────────────────────────────────────────────

#[test]
fn t33_tabs_and_newlines() {
    let ops = parse_ok(b"BT\t/F1\n12\r\nTf\r(Hello)\nTj\nET");
    assert_eq!(
        ops,
        [
            ContentOp::BeginText,
            ContentOp::SetFont {
                name: b"F1".to_vec(),
                size: 12.0
            },
            ContentOp::ShowText(b"Hello".to_vec()),
            ContentOp::EndText,
        ]
    );
}

#[test]
fn t34_tabs_and_newlines_round_trip() {
    round_trips(b"BT\t/F1\n12\r\nTf\r(Hello)\nTj\nET");
}

// ── String escapes ────────────────────────────────────────────────────────────

#[test]
fn t35_string_escape_newline() {
    let ops = parse_ok(b"(A\\nB) Tj");
    assert_eq!(ops, [ContentOp::ShowText(b"A\nB".to_vec())]);
}

#[test]
fn t36_string_escape_paren() {
    let ops = parse_ok(b"(A\\)B) Tj");
    assert_eq!(ops, [ContentOp::ShowText(b"A)B".to_vec())]);
}

#[test]
fn t37_string_nested_parens() {
    let ops = parse_ok(b"(A(B)C) Tj");
    assert_eq!(ops, [ContentOp::ShowText(b"A(B)C".to_vec())]);
}

#[test]
fn t38_string_octal_escape() {
    // \110 = 0x48 = 'H'
    let ops = parse_ok(b"(\\110ello) Tj");
    assert_eq!(ops, [ContentOp::ShowText(b"Hello".to_vec())]);
}

// ── Hex strings ───────────────────────────────────────────────────────────────

#[test]
fn t39_hex_string_mixed_case() {
    let ops = parse_ok(b"<48 65 6c 6c 6F> Tj");
    assert_eq!(ops, [ContentOp::ShowText(b"Hello".to_vec())]);
}

#[test]
fn t40_hex_string_odd_digits() {
    // Trailing nibble treated as 0: <41 4> → 'A' + 0x40
    let ops = parse_ok(b"<414> Tj");
    assert_eq!(ops, [ContentOp::ShowText(vec![0x41, 0x40])]);
}

// ── Complex streams ───────────────────────────────────────────────────────────

#[test]
fn t41_full_text_block() {
    let stream = b"BT /F1 12 Tf 100 700 Td (Hello world) Tj ET";
    let ops = parse_ok(stream);
    assert_eq!(
        ops,
        [
            ContentOp::BeginText,
            ContentOp::SetFont {
                name: b"F1".to_vec(),
                size: 12.0
            },
            ContentOp::MoveText(100.0, 700.0),
            ContentOp::ShowText(b"Hello world".to_vec()),
            ContentOp::EndText,
        ]
    );
}

#[test]
fn t42_full_text_block_round_trip() {
    round_trips(b"BT /F1 12 Tf 100 700 Td (Hello world) Tj ET");
}

#[test]
fn t43_multiline_text() {
    let stream = b"BT /F1 12 Tf 72 720 Td (Line 1) Tj T* (Line 2) Tj ET";
    let ops = parse_ok(stream);
    assert!(ops.iter().any(|op| *op == ContentOp::NextLine));
    assert!(ops
        .iter()
        .any(|op| *op == ContentOp::ShowText(b"Line 2".to_vec())));
}

#[test]
fn t44_multiline_round_trip() {
    round_trips(b"BT /F1 12 Tf 72 720 Td (Line 1) Tj T* (Line 2) Tj ET");
}

#[test]
fn t45_colored_text() {
    let stream = b"BT 1 0 0 rg /F1 12 Tf (Red) Tj ET";
    let ops = parse_ok(stream);
    assert!(ops.contains(&ContentOp::SetFillRgb(1.0, 0.0, 0.0)));
}

#[test]
fn t46_colored_text_round_trip() {
    round_trips(b"BT 1 0 0 rg /F1 12 Tf (Red) Tj ET");
}

#[test]
fn t47_graphics_state_and_text() {
    let stream = b"q 1 0 0 rg Q BT (x) Tj ET";
    let ops = parse_ok(stream);
    assert_eq!(ops[0], ContentOp::SaveState);
    assert_eq!(ops[2], ContentOp::RestoreState);
}

#[test]
fn t48_tj_only_numbers() {
    // TJ array with only kern values — edge case
    let ops = parse_ok(b"[100 -50] TJ");
    assert_eq!(
        ops,
        [ContentOp::ShowTexts(vec![
            TjItem::Kern(100.0),
            TjItem::Kern(-50.0)
        ])]
    );
}

#[test]
fn t49_tj_hex_strings() {
    let ops = parse_ok(b"[<4869>] TJ");
    assert_eq!(
        ops,
        [ContentOp::ShowTexts(vec![TjItem::Text(b"Hi".to_vec())])]
    );
}

#[test]
fn t50_empty_stream() {
    let ops = parse_ok(b"");
    assert!(ops.is_empty());
}

#[test]
fn t51_empty_stream_round_trip() {
    round_trips(b"");
}

#[test]
fn t52_whitespace_only_stream() {
    let ops = parse_ok(b"   \n\t  ");
    assert!(ops.is_empty());
}

#[test]
fn t53_whitespace_only_round_trip() {
    round_trips(b"   \n\t  ");
}

#[test]
fn t54_multiple_fonts() {
    let stream = b"BT /F1 10 Tf (A) Tj /F2 14 Tf (B) Tj ET";
    let ops = parse_ok(stream);
    let fonts: Vec<_> = ops
        .iter()
        .filter_map(|op| {
            if let ContentOp::SetFont { name, .. } = op {
                Some(name.as_slice())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(fonts, [b"F1" as &[u8], b"F2"]);
}

#[test]
fn t55_multiple_fonts_round_trip() {
    round_trips(b"BT /F1 10 Tf (A) Tj /F2 14 Tf (B) Tj ET");
}

#[test]
fn t56_rotation_matrix() {
    // 90-degree rotation
    let ops = parse_ok(b"0 1 -1 0 0 0 cm");
    assert_eq!(ops, [ContentOp::Transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0])]);
}

#[test]
fn t57_mixed_path_and_text() {
    let stream = b"q 0 0 m 100 0 l h S Q BT (x) Tj ET";
    round_trips(stream);
}

#[test]
fn t58_text_matrix_then_show() {
    let stream = b"BT 1 0 0 1 100 200 Tm (positioned) Tj ET";
    let ops = parse_ok(stream);
    assert!(ops.iter().any(|op| matches!(
        op,
        ContentOp::SetTextMatrix([1.0, 0.0, 0.0, 1.0, 100.0, 200.0])
    )));
}

#[test]
fn t59_string_with_backslash() {
    let ops = parse_ok(b"(A\\\\B) Tj");
    assert_eq!(ops, [ContentOp::ShowText(b"A\\B".to_vec())]);
}

#[test]
fn t60_negative_numbers() {
    let ops = parse_ok(b"-1.5 -2.5 Td");
    assert_eq!(ops, [ContentOp::MoveText(-1.5, -2.5)]);
}

#[test]
fn t61_plus_prefixed_number() {
    let ops = parse_ok(b"+1 +2 Td");
    assert_eq!(ops, [ContentOp::MoveText(1.0, 2.0)]);
}

#[test]
fn t62_zero_values() {
    let ops = parse_ok(b"0 0 Td");
    assert_eq!(ops, [ContentOp::MoveText(0.0, 0.0)]);
}

#[test]
fn t63_cmyk_fill_and_stroke() {
    round_trips(b"0 0 0 1 k 0 0 0 0 K");
}

#[test]
fn t64_text_state_params_combined() {
    let stream = b"0.5 Tc 1 Tw 100 Tz 12 TL 2 Ts";
    let ops = parse_ok(stream);
    assert_eq!(ops.len(), 5);
    assert!(ops.contains(&ContentOp::SetCharSpacing(0.5)));
    assert!(ops.contains(&ContentOp::SetWordSpacing(1.0)));
    assert!(ops.contains(&ContentOp::SetHorizontalScale(100.0)));
    assert!(ops.contains(&ContentOp::SetLeading(12.0)));
    assert!(ops.contains(&ContentOp::SetRise(2.0)));
}

#[test]
fn t65_text_state_params_round_trip() {
    round_trips(b"0.5 Tc 1 Tw 100 Tz 12 TL 2 Ts");
}

#[test]
fn t66_td_negative_leading() {
    let ops = parse_ok(b"0 -12 TD");
    assert_eq!(ops, [ContentOp::MoveTextAndSetLeading(0.0, -12.0)]);
}

#[test]
fn t67_quote_op() {
    let stream = b"BT /F1 12 Tf (line1) ' (line2) ' ET";
    round_trips(stream);
    let ops = parse_ok(stream);
    let quote_count = ops
        .iter()
        .filter(|op| matches!(op, ContentOp::NextLineAndShowText(_)))
        .count();
    assert_eq!(quote_count, 2);
}

#[test]
fn t68_double_quote_op() {
    let ops = parse_ok(b"1 2 (word) \"");
    assert_eq!(
        ops,
        [ContentOp::ShowTextWithParams {
            aw: 1.0,
            ac: 2.0,
            text: b"word".to_vec()
        }]
    );
}

#[test]
fn t69_inline_image_passthrough() {
    // BI...EI should pass through as Other.
    let stream = b"BI /W 1 /H 1 /CS /G /BPC 8 ID \xFF EI";
    let (ops, _err) = ContentStreamParser::new(stream).collect_ops_lenient();
    // BI should appear as Other
    assert!(ops
        .iter()
        .any(|p| matches!(&p.op, ContentOp::Other { name, .. } if name == b"BI")));
}

#[test]
fn t70_large_tj_array() {
    // 20-element TJ array alternating strings and kerns
    let mut stream = b"[".to_vec();
    for i in 0..10 {
        stream.extend_from_slice(b"(x)");
        if i < 9 {
            stream.extend_from_slice(b" -50 ");
        }
    }
    stream.extend_from_slice(b"] TJ");
    let ops = parse_ok(&stream);
    assert_eq!(ops.len(), 1);
    if let ContentOp::ShowTexts(items) = &ops[0] {
        assert_eq!(items.len(), 19); // 10 texts + 9 kerns
    } else {
        panic!("expected ShowTexts");
    }
}

// ── Error cases ───────────────────────────────────────────────────────────────

#[test]
fn t71_unterminated_string_error() {
    let err = parse_err(b"(unclosed");
    assert!(matches!(err, ContentStreamError::UnterminatedString { .. }));
}

#[test]
fn t72_unterminated_hex_string_error() {
    let err = parse_err(b"<AABB");
    assert!(matches!(
        err,
        ContentStreamError::UnterminatedHexString { .. }
    ));
}

#[test]
fn t73_unterminated_array_error() {
    let err = parse_err(b"[(hello)");
    assert!(matches!(err, ContentStreamError::UnterminatedArray { .. }));
}

#[test]
fn t74_malformed_tf_no_size() {
    // Tf with only one operand (name but no size) → malformed
    let err = parse_err(b"/F1 Tf");
    assert!(matches!(err, ContentStreamError::MalformedOperand { op, .. } if op == "Tf"));
}

#[test]
fn t75_malformed_rg_too_few_args() {
    let err = parse_err(b"1 0 rg");
    assert!(matches!(err, ContentStreamError::MalformedOperand { op, .. } if op == "rg"));
}

// ── Round-trip corpus (50+ streams) ──────────────────────────────────────────

macro_rules! rt {
    ($name:ident, $stream:expr) => {
        #[test]
        fn $name() {
            round_trips($stream);
        }
    };
}

rt!(rt01, b"q Q");
rt!(rt02, b"BT ET");
rt!(rt03, b"/F1 12 Tf");
rt!(rt04, b"(Hello) Tj");
rt!(rt05, b"[(A) -50 (B)] TJ");
rt!(rt06, b"100 200 Td");
rt!(rt07, b"0 -12 TD");
rt!(rt08, b"T*");
rt!(rt09, b"1 0 0 1 0 0 Tm");
rt!(rt10, b"0.5 Tc");
rt!(rt11, b"1 Tw");
rt!(rt12, b"100 Tz");
rt!(rt13, b"12 TL");
rt!(rt14, b"2 Ts");
rt!(rt15, b"1 0 0 rg");
rt!(rt16, b"0.5 g");
rt!(rt17, b"0 0 0 1 k");
rt!(rt18, b"0 0 1 RG");
rt!(rt19, b"0 G");
rt!(rt20, b"0 0 0 0 K");
rt!(rt21, b"1 0 0 1 72 720 cm");
rt!(rt22, b"BT /F1 10 Tf 72 720 Td (Page 1) Tj ET");
rt!(rt23, b"BT\n/F1 12 Tf\n100 700 Td\n(Hello) Tj\nET\n");
rt!(rt24, b"q 1 0 0 rg BT /F1 12 Tf (Red) Tj ET Q");
rt!(rt25, b"% header\nBT (body) Tj ET");
rt!(rt26, b"q q q Q Q Q");
rt!(rt27, b"(A\\nB\\tC) Tj");
rt!(rt28, b"<48656c6c6f> Tj");
rt!(rt29, b"[(word) 0 (2)] TJ");
rt!(rt30, b"BT 1 2 (x) \" ET");
rt!(rt31, b"(line) '");
rt!(rt32, b"0 0 m 100 100 l 50 200 l h f");
rt!(rt33, b"q 0.5 0.5 0.5 rg 0 0 100 100 re f Q");
rt!(rt34, b"BT /F1 8 Tf 0.2 Tc 1 Tw (spaced text) Tj ET");
rt!(rt35, b"0.3 0.7 0.1 rg 0 0 1 RG");
rt!(
    rt36,
    b"BT 12 TL /F1 12 Tf 72 720 Td (A) Tj T* (B) Tj T* (C) Tj ET"
);
rt!(rt37, b"/F1 0 Tf");
rt!(rt38, b"1 0 0 1 0 0 cm");
rt!(rt39, b"(\\101\\102\\103) Tj"); // octal ABC
rt!(rt40, b"BT 0 0 0 1 k /F1 12 Tf (black) Tj ET");
rt!(rt41, b"q 0 G 1 0 0 rg 0 0 200 200 re B Q");
rt!(rt42, b"(\\() Tj"); // escaped open paren
rt!(rt43, b"(\\)) Tj"); // escaped close paren
rt!(rt44, b"(\\\\) Tj"); // escaped backslash
rt!(rt45, b"BT /Font.Name 11 Tf (x) Tj ET");
rt!(rt46, b"1.0 0.0 0.0 1.0 100.0 200.0 Tm");
rt!(rt47, b"BT 1 0 0 1 0 0 Tm (origin) Tj ET");
rt!(rt48, b"[(x) -100 (y)] TJ"); // TJ array with kern value
rt!(rt49, b"q 0.1 0.2 0.3 0.4 k Q");
rt!(rt50, b"BT /F2 14 Tf (Large) Tj /F1 10 Tf (Normal) Tj ET");
