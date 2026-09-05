// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Appearance-stream generation: what actually ends up in the content stream.
//!
//! WHY THESE FIVE
//!
//! `generate_appearances`, `draw_appearance`, `multiline_appearance` and
//! `execute_commands` were reachable from no test at all, and this is where the
//! multiline-collapse bugs have lived: several lines of text going in, one line
//! of text coming out, and a PDF that opens fine and looks plausible. Nothing in
//! the type system notices that, and neither does a page-count assertion.
//!
//! So the assertions here are about the shape of the emitted operators rather
//! than their exact bytes: how many `Tj` come out for N lines, whether the
//! vertical advance between them is the line height, whether a nested node keeps
//! its ancestor's offset, whether the second image gets `/Im1`. Each of those is
//! a bug that has a plausible way of being written and no other alarm.

use pdf_interpret::color::Color;
use pdf_xfa::appearance_bridge::{
    draw_appearance, generate_appearances, multiline_appearance, AppearanceConfig, AppearanceStream,
};
use pdf_xfa::paint_bridge::{execute_commands, XfaPaintCommand};
use xfa_layout_engine::form::{FieldKind, FormNodeId, FormNodeStyle};
use xfa_layout_engine::layout::{LayoutContent, LayoutDom, LayoutNode, LayoutPage};
use xfa_layout_engine::text::FontFamily;
use xfa_layout_engine::types::{Rect, TextAlign};

// ── helpers ──────────────────────────────────────────────────────────────────

fn stream_text(ap: &AppearanceStream) -> String {
    String::from_utf8_lossy(&ap.content).into_owned()
}

/// The operands of every `Td` in a content stream, in order.
///
/// `Td` is relative to the previous text-line origin, so the second and later
/// entries are the *advance*, not an absolute position. That distinction is the
/// whole point: a multiline stream that emits an absolute `Td` per line reads
/// identically at a glance and stacks every line on top of the first.
fn td_operands(stream: &str) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let woorden: Vec<&str> = stream.split_whitespace().collect();
    for (i, w) in woorden.iter().enumerate() {
        if *w == "Td" && i >= 2 {
            if let (Ok(x), Ok(y)) = (woorden[i - 2].parse(), woorden[i - 1].parse()) {
                out.push((x, y));
            }
        }
    }
    out
}

/// The string inside each `(...) Tj`, in order, still escaped.
///
/// Scans for the *unescaped* delimiters. Taking the last `(` before a `) Tj`
/// looks equivalent and is not: on `(Total \(net\)) Tj` it starts inside the
/// escaped pair and reports `net\)`, which would turn this helper into a source
/// of false failures on exactly the values the escaping exists for.
fn shown_text(stream: &str) -> Vec<String> {
    let bytes: Vec<char> = stream.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != '(' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut diep = 1;
        while j < bytes.len() && diep > 0 {
            match bytes[j] {
                '\\' => j += 1,
                '(' => diep += 1,
                ')' => diep -= 1,
                _ => {}
            }
            j += 1;
        }
        if diep == 0 && stream[..].chars().skip(j).take(3).collect::<String>() == " Tj" {
            out.push(bytes[i + 1..j - 1].iter().collect());
        }
        i = j.max(i + 1);
    }
    out
}

fn node(
    name_str: &str,
    rect: Rect,
    content: LayoutContent,
    children_nodes: Vec<LayoutNode>,
) -> LayoutNode {
    LayoutNode {
        form_node: FormNodeId(0),
        rect,
        name: name_str.to_string(),
        content,
        children: children_nodes,
        style: FormNodeStyle::default(),
        display_items: Vec::new(),
        save_items: Vec::new(),
    }
}

fn page(width_pt: f64, height_pt: f64, nodes: Vec<LayoutNode>) -> LayoutPage {
    LayoutPage {
        width: width_pt,
        height: height_pt,
        nodes,
        runtime_instantiated: false,
    }
}

// ── multiline_appearance ─────────────────────────────────────────────────────

/// Four lines in, four lines out. The bug this rules out is the one that has
/// actually happened: the lines collapse into one and the output is a valid PDF
/// showing a quarter of the text.
#[test]
fn multiline_draws_one_show_operator_per_line() {
    let lines: Vec<String> = ["alpha", "bravo", "charlie", "delta"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let ap = multiline_appearance(
        &lines,
        10.0,
        12.0,
        200.0,
        200.0,
        &AppearanceConfig::default(),
    );
    let stream = stream_text(&ap);

    assert_eq!(
        shown_text(&stream),
        lines,
        "every line must be shown, in order; a collapse to one Tj still yields a \
         valid content stream and a PDF that opens"
    );
}

/// The vertical step between consecutive lines is the line height, and it is
/// relative. Emitting absolute `Td` per line prints all lines on one baseline —
/// visually a collapse, textually four intact strings, so `shown_text`
/// above would not catch it on its own.
#[test]
fn multiline_advances_by_exactly_one_line_height() {
    let lines: Vec<String> = ["one", "two", "three"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let height_pt = 200.0;
    let font = 10.0;
    let line_height = 14.0;
    let config = AppearanceConfig::default();
    let ap = multiline_appearance(&lines, font, line_height, 300.0, height_pt, &config);
    let tds = td_operands(&stream_text(&ap));

    assert_eq!(tds.len(), lines.len(), "one Td per line");
    assert_eq!(
        tds[0],
        (config.text_padding, height_pt - font - config.text_padding),
        "the first line is positioned absolutely, inside the box"
    );
    for (i, (dx, dy)) in tds.iter().enumerate().skip(1) {
        assert_eq!(
            (*dx, *dy),
            (0.0, -line_height),
            "line {i} must advance exactly one line height downward and not \
             re-position horizontally"
        );
    }
}

/// Lines that run off the bottom are dropped rather than drawn below the box.
/// The clip must not take the whole run with it: two lines fit here and both
/// have to survive.
#[test]
fn multiline_clips_at_the_bottom_and_keeps_what_fits() {
    let lines: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
    // height 30, font 10, padding 2 → first baseline at 18, step 12 → 18, 6, -6…
    let ap = multiline_appearance(
        &lines,
        10.0,
        12.0,
        200.0,
        30.0,
        &AppearanceConfig::default(),
    );
    let shown = shown_text(&stream_text(&ap));

    assert_eq!(
        shown,
        vec!["line0".to_string(), "line1".to_string()],
        "only the lines with a non-negative baseline may be drawn, and every one \
         of those must be"
    );
}

/// An unescaped `(` in a value terminates the string early and corrupts every
/// operator after it. Values with parentheses arrive from real form data.
#[test]
fn multiline_escapes_parentheses_and_backslashes() {
    let lines = vec![r"a(b)c\d".to_string()];
    let stream = stream_text(&multiline_appearance(
        &lines,
        10.0,
        12.0,
        200.0,
        200.0,
        &AppearanceConfig::default(),
    ));

    assert!(
        stream.contains(r"\(b\)") && stream.contains(r"\\d"),
        "parentheses and backslashes must be escaped before they reach the \
         content stream; got: {stream}"
    );
}

/// No lines means no text object at all — and, crucially, no font resource. An
/// appearance that declares `/F1` it never uses drags a font dependency into the
/// page resources for nothing.
#[test]
fn multiline_without_lines_declares_no_font() {
    let ap = multiline_appearance(&[], 10.0, 12.0, 200.0, 100.0, &AppearanceConfig::default());
    let stream = stream_text(&ap);

    assert!(!stream.contains("BT"), "no text object without lines");
    assert!(
        ap.font_resources.is_empty(),
        "no font may be declared when nothing is drawn"
    );
    assert_eq!(ap.bbox, [0.0, 0.0, 200.0, 100.0]);
}

// ── draw_appearance ──────────────────────────────────────────────────────────

/// Static text is shown once, escaped, on a baseline inside the box, and the
/// font it uses is the one it declares.
#[test]
fn draw_shows_its_text_and_declares_the_font_it_uses() {
    let config = AppearanceConfig::default();
    let ap = draw_appearance("Total (net)", 120.0, 40.0, &config);
    let stream = stream_text(&ap);

    assert_eq!(shown_text(&stream), vec![r"Total \(net\)".to_string()]);
    assert_eq!(
        ap.font_resources,
        vec![("F1".to_string(), config.default_font.clone())],
        "the stream writes /F1, so /F1 has to be in the resources"
    );
    assert!(
        stream.contains("/F1 "),
        "the declared resource must be the one selected: {stream}"
    );

    let tds = td_operands(&stream);
    assert_eq!(tds.len(), 1);
    let (x, y) = tds[0];
    assert!(
        (0.0..120.0).contains(&x) && (0.0..40.0).contains(&y),
        "the baseline must sit inside the bbox, got ({x}, {y})"
    );
}

/// Empty text draws nothing and declares nothing. The background fill still
/// happens — that is what distinguishes "no text" from "no appearance".
#[test]
fn draw_with_empty_text_declares_no_font_but_still_fills() {
    let mut config = AppearanceConfig::default();
    config.background_color = Some([1.0, 1.0, 1.0]);
    let ap = draw_appearance("", 120.0, 40.0, &config);
    let stream = stream_text(&ap);

    assert!(!stream.contains("Tj"), "nothing to show, nothing shown");
    assert!(
        ap.font_resources.is_empty(),
        "no font for text that is not there"
    );
    assert!(
        stream.contains(" re\nf\n"),
        "the background fill is not text and must survive: {stream}"
    );
}

// ── generate_appearances ─────────────────────────────────────────────────────

/// A nested node is positioned relative to its parent. Dropping the ancestor
/// offset puts every nested field at the page origin — a stacked pile of fields
/// in the bottom-left corner, from code that reads correct.
#[test]
fn generate_accumulates_ancestor_offsets() {
    let child = node(
        "inner",
        Rect::new(5.0, 7.0, 50.0, 20.0),
        LayoutContent::Text("nested".to_string()),
        vec![],
    );
    let parent = node(
        "outer",
        Rect::new(100.0, 200.0, 300.0, 100.0),
        LayoutContent::Text("parent".to_string()),
        vec![child],
    );
    let dom = LayoutDom {
        pages: vec![page(595.0, 842.0, vec![parent])],
    };

    let pages_out = generate_appearances(&dom, &AppearanceConfig::default()).expect("layout");
    assert_eq!(pages_out.len(), 1);
    assert_eq!(pages_out[0].width, 595.0);
    assert_eq!(pages_out[0].height, 842.0);

    let positions: Vec<(&str, f64, f64)> = pages_out[0]
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.abs_x, e.abs_y))
        .collect();
    assert_eq!(
        positions,
        vec![("outer", 100.0, 200.0), ("inner", 105.0, 207.0)],
        "the child's absolute position is parent + own offset; a parent that \
         produced an entry itself must still be descended into"
    );
}

/// Only content that draws anything produces an entry. `Image` and `Draw` are
/// handled elsewhere in the pipeline, and an empty entry for them would put an
/// empty appearance stream over the real one.
#[test]
fn generate_skips_content_that_draws_nothing() {
    let dom = LayoutDom {
        pages: vec![page(
            595.0,
            842.0,
            vec![
                node(
                    "empty",
                    Rect::new(0.0, 0.0, 10.0, 10.0),
                    LayoutContent::None,
                    vec![],
                ),
                node(
                    "picture",
                    Rect::new(0.0, 0.0, 10.0, 10.0),
                    LayoutContent::Image {
                        data: vec![0u8; 4],
                        mime_type: "image/png".to_string(),
                    },
                    vec![],
                ),
                node(
                    "text",
                    Rect::new(0.0, 0.0, 10.0, 10.0),
                    LayoutContent::Text("x".to_string()),
                    vec![],
                ),
            ],
        )],
    };

    let pages_out = generate_appearances(&dom, &AppearanceConfig::default()).expect("layout");
    let names: Vec<&str> = pages_out[0]
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, vec!["text"]);
}

/// A checked checkbox routes to the checkbox appearance, which draws a mark; an
/// unchecked one draws only the box. Routing a checkbox through the text path
/// instead would render the literal string "1" in the box.
#[test]
fn generate_routes_checkboxes_to_the_checkbox_appearance() {
    let build = |value: &str| LayoutDom {
        pages: vec![page(
            100.0,
            100.0,
            vec![node(
                "box",
                Rect::new(0.0, 0.0, 12.0, 12.0),
                LayoutContent::Field {
                    value: value.to_string(),
                    field_kind: FieldKind::Checkbox,
                    font_size: 10.0,
                    font_family: FontFamily::SansSerif,
                },
                vec![],
            )],
        )],
    };

    let config = AppearanceConfig::default();
    let checked = generate_appearances(&build("1"), &config).expect("layout");
    let out = generate_appearances(&build("0"), &config).expect("layout");

    let checked_stream = stream_text(&checked[0].entries[0].appearance);
    let unchecked_stream = stream_text(&out[0].entries[0].appearance);

    assert!(
        !checked_stream.contains("Tj") && !unchecked_stream.contains("Tj"),
        "a checkbox is drawn, not typeset; the raw value must never be shown as text"
    );
    assert!(
        checked_stream.matches(" l\n").count() == 2,
        "a checked box draws two diagonals: {checked_stream}"
    );
    assert!(
        !unchecked_stream.contains(" l\n"),
        "an unchecked box draws no mark: {unchecked_stream}"
    );
}

// ── execute_commands ─────────────────────────────────────────────────────────

/// Everything the commands emit sits inside one save/restore pair, so the page
/// graphics state survives the overlay.
#[test]
fn execute_wraps_everything_in_save_restore() {
    let out = execute_commands(&[XfaPaintCommand::FillRect {
        x: 1.0,
        y: 2.0,
        w: 3.0,
        h: 4.0,
        color: Color::from_device_rgb(1.0, 0.0, 0.0),
    }]);
    let stream = String::from_utf8_lossy(&out).into_owned();

    assert!(stream.starts_with("q\n"), "must open with q: {stream}");
    assert!(stream.ends_with("Q\n"), "must close with Q: {stream}");
    assert!(
        stream.contains(" re\nf\n"),
        "the fill itself must be there: {stream}"
    );
}

/// Each image gets its own XObject name. Emitting `/Im0` for all of them makes
/// every image on the page render as the first one — a bug that is invisible on
/// any page carrying exactly one image, which is most of them.
#[test]
fn execute_numbers_images_in_order() {
    let plaatje = |x: f64| XfaPaintCommand::DrawImage {
        x,
        y: 0.0,
        w: 10.0,
        h: 10.0,
        image_data: vec![0u8; 4],
        mime_type: "image/png".to_string(),
    };
    let stream = String::from_utf8_lossy(&execute_commands(&[
        plaatje(0.0),
        plaatje(20.0),
        plaatje(40.0),
    ]))
    .into_owned();

    for n in 0..3 {
        assert!(
            stream.contains(&format!("/Im{n} Do")),
            "image {n} must reference /Im{n}: {stream}"
        );
    }
}

/// The same collapse guard as for `multiline_appearance`, on the other path into
/// a content stream: paint commands.
#[test]
fn execute_draws_every_line_of_multiline_text() {
    let lines: Vec<String> = ["one", "two", "three"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let stream =
        String::from_utf8_lossy(&execute_commands(&[XfaPaintCommand::DrawMultilineText {
            x: 10.0,
            y: 700.0,
            lines: lines.clone(),
            font_family: FontFamily::SansSerif,
            font_size: 10.0,
            line_height: 12.0,
            color: Color::from_device_rgb(0.0, 0.0, 0.0),
            text_align: TextAlign::Left,
            container_width: 200.0,
            text_padding: 2.0,
        }]))
        .into_owned();

    assert_eq!(shown_text(&stream), lines, "no line may be dropped");
    let tds = td_operands(&stream);
    assert_eq!(tds.len(), 3);
    assert_eq!(tds[0], (12.0, 700.0), "first line absolute at x + padding");
    for (dx, dy) in &tds[1..] {
        assert_eq!(
            (*dx, *dy),
            (0.0, -12.0),
            "left-aligned lines advance straight down by the line height"
        );
    }
}

/// Text arriving from form data can contain parentheses, on this path too.
#[test]
fn execute_escapes_text_before_it_reaches_the_stream() {
    let stream = String::from_utf8_lossy(&execute_commands(&[XfaPaintCommand::DrawText {
        x: 0.0,
        y: 0.0,
        text: r"a(b)\c".to_string(),
        font_family: FontFamily::SansSerif,
        font_size: 10.0,
        color: Color::from_device_rgb(0.0, 0.0, 0.0),
    }]))
    .into_owned();

    assert!(
        stream.contains(r"\(b\)") && stream.contains(r"\\c"),
        "unescaped parentheses terminate the string and corrupt the rest of the \
         stream: {stream}"
    );
}
