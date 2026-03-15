//! Default Appearance (DA) parsing and appearance stream generation (B.6).

use crate::button::{button_kind, ButtonKind};
use crate::choice::choice_kind;
use crate::text::text_field_kind;
use crate::tree::*;
use std::io::Write as _;

/// Parsed default appearance components.
#[derive(Debug, Clone, Default)]
pub struct DefaultAppearance {
    /// Font name (PDF name without leading /).
    pub font_name: Option<String>,
    /// Font size (0 = auto-size).
    pub font_size: f32,
    /// Fill color components (gray, RGB, or CMYK).
    pub color: Vec<f32>,
    /// Color space operator: "g", "rg", or "k".
    pub color_op: Option<String>,
}

/// Parse a /DA string like "0 0 0 rg /Helv 12 Tf".
pub fn parse_da(da: &str) -> DefaultAppearance {
    let mut result = DefaultAppearance::default();
    let tokens: Vec<&str> = da.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "Tf" if i >= 2 => {
                result.font_size = tokens[i - 1].parse().unwrap_or(0.0);
                let name = tokens[i - 2];
                result.font_name = Some(name.strip_prefix('/').unwrap_or(name).to_string());
            }
            "g" if i >= 1 => {
                if let Ok(g) = tokens[i - 1].parse::<f32>() {
                    result.color = vec![g];
                    result.color_op = Some("g".into());
                }
            }
            "rg" if i >= 3 => {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    tokens[i - 3].parse::<f32>(),
                    tokens[i - 2].parse::<f32>(),
                    tokens[i - 1].parse::<f32>(),
                ) {
                    result.color = vec![r, g, b];
                    result.color_op = Some("rg".into());
                }
            }
            "k" if i >= 4 => {
                if let (Ok(c), Ok(m), Ok(y), Ok(k)) = (
                    tokens[i - 4].parse::<f32>(),
                    tokens[i - 3].parse::<f32>(),
                    tokens[i - 2].parse::<f32>(),
                    tokens[i - 1].parse::<f32>(),
                ) {
                    result.color = vec![c, m, y, k];
                    result.color_op = Some("k".into());
                }
            }
            _ => {}
        }
        i += 1;
    }
    result
}

/// Generate a raw PDF content stream for a text field appearance.
pub fn generate_text_appearance(
    tree: &FieldTree,
    id: FieldId,
    da: &DefaultAppearance,
    text: &str,
) -> Vec<u8> {
    let node = tree.get(id);
    let rect = node.rect.unwrap_or([0.0, 0.0, 100.0, 20.0]);
    let width = rect[2] - rect[0];
    let height = rect[3] - rect[1];
    let quadding = tree.effective_quadding(id);
    let flags = tree.effective_flags(id);
    let font_name = da.font_name.as_deref().unwrap_or("Helv");
    let font_size = if da.font_size > 0.0 {
        da.font_size
    } else {
        (height - 2.0).clamp(4.0, 24.0)
    };
    let kind = text_field_kind(flags);
    let mut buf = Vec::new();
    let bw = node.border_style.as_ref().map(|b| b.width).unwrap_or(1.0);

    if let Some(ref mk) = node.mk {
        if let Some(ref bg) = mk.background_color {
            write_color(&mut buf, bg, false);
            let _ = writeln!(buf, "{} {} {} {} re f", 0.0, 0.0, width, height);
        }
        if let Some(ref bc) = mk.border_color {
            write_color(&mut buf, bc, true);
            let _ = writeln!(
                buf,
                "{} w {} {} {} {} re S",
                bw,
                bw / 2.0,
                bw / 2.0,
                width - bw,
                height - bw
            );
        }
    }

    let margin = bw + 1.0;
    let _ = writeln!(
        buf,
        "{} {} {} {} re W n",
        margin,
        margin,
        width - margin * 2.0,
        height - margin * 2.0
    );
    buf.extend_from_slice(b"BT\n");
    if !da.color.is_empty() {
        write_color(&mut buf, &da.color, false);
    }
    let _ = writeln!(buf, "/{} {} Tf", font_name, font_size);

    let display_text = if flags.password() {
        "*".repeat(text.len())
    } else {
        text.to_string()
    };

    match kind {
        crate::text::TextFieldKind::Comb => {
            if let Some(max_len) = tree.effective_max_len(id) {
                let cell_w = width / max_len as f32;
                for (i, ch) in display_text.chars().take(max_len as usize).enumerate() {
                    let x = margin + cell_w * i as f32 + cell_w * 0.25;
                    let y = margin + (height - margin * 2.0 - font_size) / 2.0;
                    let _ = writeln!(buf, "{} {} Td ({}) Tj", x, y, escape_pdf_string_char(ch));
                }
            }
        }
        crate::text::TextFieldKind::Multiline => {
            let leading = font_size * 1.2;
            let _ = writeln!(buf, "{} TL", leading);
            let _ = writeln!(buf, "{} {} Td", margin, height - margin - font_size);
            for (i, line) in display_text.lines().enumerate() {
                if i > 0 {
                    buf.extend_from_slice(b"T*\n");
                }
                let _ = writeln!(buf, "({}) Tj", escape_pdf_string(line));
            }
        }
        _ => {
            let approx_w = display_text.len() as f32 * font_size * 0.5;
            let x = match quadding {
                Quadding::Center => margin + (width - margin * 2.0 - approx_w) / 2.0,
                Quadding::Right => width - margin - approx_w,
                Quadding::Left => margin,
            }
            .max(margin);
            let y = margin + (height - margin * 2.0 - font_size) / 2.0;
            let _ = writeln!(buf, "{} {} Td", x, y);
            let _ = writeln!(buf, "({}) Tj", escape_pdf_string(&display_text));
        }
    }
    buf.extend_from_slice(b"ET\n");
    buf
}

/// Generate appearance stream for a checkbox.
pub fn generate_checkbox_appearance(tree: &FieldTree, id: FieldId, checked: bool) -> Vec<u8> {
    let node = tree.get(id);
    let rect = node.rect.unwrap_or([0.0, 0.0, 12.0, 12.0]);
    let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
    let mut buf = Vec::new();
    let _ = writeln!(buf, "1 g 0 0 {} {} re f", w, h);
    let _ = writeln!(buf, "0 g 0.5 w 0 0 {} {} re S", w, h);
    if checked {
        let m = w * 0.15;
        let _ = writeln!(
            buf,
            "0 g 1.5 w {} {} m {} {} l {} {} l S",
            m,
            h * 0.5,
            w * 0.4,
            m,
            w - m,
            h - m
        );
    }
    buf
}

/// Generate appearance stream for a radio button.
pub fn generate_radio_appearance(tree: &FieldTree, id: FieldId, selected: bool) -> Vec<u8> {
    let node = tree.get(id);
    let rect = node.rect.unwrap_or([0.0, 0.0, 12.0, 12.0]);
    let size = (rect[2] - rect[0]).min(rect[3] - rect[1]);
    let (cx, cy, r) = (size / 2.0, size / 2.0, size / 2.0 - 1.0);
    let k = 0.5523_f32;
    let mut buf = Vec::new();
    write_circle(&mut buf, cx, cy, r, k, "1 g", "f");
    write_circle(&mut buf, cx, cy, r, k, "0 g 0.5 w", "S");
    if selected {
        write_circle(&mut buf, cx, cy, r * 0.4, k, "0 g", "f");
    }
    buf
}

fn write_circle(buf: &mut Vec<u8>, cx: f32, cy: f32, r: f32, k: f32, prefix: &str, op: &str) {
    let kr = k * r;
    let _ = writeln!(
        buf,
        "{prefix} {cx} {bot} m {r1} {bot} {right} {b1} {right} {cy} c {right} {t1} {r1} {top} {cx} {top} c {l1} {top} {left} {t1} {left} {cy} c {left} {b1} {l1} {bot} {cx} {bot} c {op}",
        prefix = prefix,
        op = op,
        cx = cx,
        cy = cy,
        bot = cy - r,
        top = cy + r,
        left = cx - r,
        right = cx + r,
        r1 = cx + kr,
        l1 = cx - kr,
        t1 = cy + kr,
        b1 = cy - kr,
    );
}

/// Generate appearance stream for a choice field.
pub fn generate_choice_appearance(
    tree: &FieldTree,
    id: FieldId,
    da: &DefaultAppearance,
) -> Vec<u8> {
    let node = tree.get(id);
    let rect = node.rect.unwrap_or([0.0, 0.0, 150.0, 20.0]);
    let (width, height) = (rect[2] - rect[0], rect[3] - rect[1]);
    let flags = tree.effective_flags(id);
    let kind = choice_kind(flags);
    let font_name = da.font_name.as_deref().unwrap_or("Helv");
    let font_size = if da.font_size > 0.0 {
        da.font_size
    } else {
        (height - 4.0).clamp(4.0, 12.0)
    };
    let mut buf = Vec::new();
    let _ = writeln!(buf, "1 g 0 0 {} {} re f", width, height);
    let _ = writeln!(buf, "0 g 0.5 w 0 0 {} {} re S", width, height);
    match kind {
        crate::choice::ChoiceKind::ComboBox | crate::choice::ChoiceKind::EditableCombo => {
            let selected = crate::choice::get_selection(tree, id);
            let text = selected.first().map(|s| s.as_str()).unwrap_or("");
            let arrow_w = height.min(20.0);
            let _ = writeln!(
                buf,
                "0.9 g {} 0 {} {} re f",
                width - arrow_w,
                arrow_w,
                height
            );
            let (ax, aw) = (width - arrow_w / 2.0, arrow_w * 0.25);
            let _ = writeln!(
                buf,
                "0 g {} {} m {} {} l {} {} l f",
                ax - aw,
                height * 0.65,
                ax + aw,
                height * 0.65,
                ax,
                height * 0.35
            );
            buf.extend_from_slice(b"BT\n");
            write_color(&mut buf, &da.color, false);
            let _ = writeln!(buf, "/{} {} Tf", font_name, font_size);
            let _ = writeln!(buf, "2 {} Td", (height - font_size) / 2.0);
            let _ = writeln!(buf, "({}) Tj", escape_pdf_string(text));
            buf.extend_from_slice(b"ET\n");
        }
        _ => {
            let leading = font_size * 1.2;
            let selected = crate::choice::get_selection(tree, id);
            let top_idx = node.top_index.unwrap_or(0) as usize;
            let visible = (height / leading).floor() as usize;
            buf.extend_from_slice(b"BT\n");
            write_color(&mut buf, &da.color, false);
            let _ = writeln!(buf, "/{} {} Tf", font_name, font_size);
            let _ = writeln!(buf, "{} TL", leading);
            let y_start = height - 2.0 - font_size;
            let _ = writeln!(buf, "2 {} Td", y_start);
            for (i, opt) in node.options.iter().skip(top_idx).take(visible).enumerate() {
                if selected.contains(&opt.export) || selected.contains(&opt.display) {
                    buf.extend_from_slice(b"ET\n");
                    let _ = writeln!(
                        buf,
                        "0.6 0.75 0.95 rg 0 {} {} {} re f",
                        y_start - i as f32 * leading - 1.0,
                        width,
                        leading
                    );
                    buf.extend_from_slice(b"BT\n");
                    write_color(&mut buf, &da.color, false);
                    let _ = writeln!(buf, "/{} {} Tf", font_name, font_size);
                    let _ = writeln!(buf, "2 {} Td", y_start - i as f32 * leading);
                }
                if i > 0 {
                    buf.extend_from_slice(b"T*\n");
                }
                let _ = writeln!(buf, "({}) Tj", escape_pdf_string(&opt.display));
            }
            buf.extend_from_slice(b"ET\n");
        }
    }
    buf
}

/// Generate the appropriate appearance stream for any field type.
pub fn generate_appearance(tree: &FieldTree, id: FieldId) -> Option<Vec<u8>> {
    let ft = tree.effective_field_type(id)?;
    let da_str = tree.effective_da(id).unwrap_or("0 g /Helv 12 Tf");
    let da = parse_da(da_str);
    match ft {
        FieldType::Text => {
            let text = crate::text::get_text_value(tree, id).unwrap_or_default();
            Some(generate_text_appearance(tree, id, &da, &text))
        }
        FieldType::Button => {
            let flags = tree.effective_flags(id);
            match button_kind(flags) {
                ButtonKind::Checkbox => Some(generate_checkbox_appearance(
                    tree,
                    id,
                    crate::button::is_checked(tree, id),
                )),
                ButtonKind::Radio => Some(generate_radio_appearance(
                    tree,
                    id,
                    crate::button::is_checked(tree, id),
                )),
                ButtonKind::PushButton => {
                    let caption = tree
                        .get(id)
                        .mk
                        .as_ref()
                        .and_then(|m| m.caption.as_deref())
                        .unwrap_or("");
                    Some(generate_text_appearance(tree, id, &da, caption))
                }
            }
        }
        FieldType::Choice => Some(generate_choice_appearance(tree, id, &da)),
        FieldType::Signature => None,
    }
}

fn write_color(buf: &mut Vec<u8>, color: &[f32], stroke: bool) {
    let op = match (color.len(), stroke) {
        (1, false) => "g",
        (1, true) => "G",
        (3, false) => "rg",
        (3, true) => "RG",
        (4, false) => "k",
        (4, true) => "K",
        _ => return,
    };
    for c in color {
        let _ = write!(buf, "{} ", c);
    }
    let _ = writeln!(buf, "{}", op);
}

fn escape_pdf_string(s: &str) -> String {
    s.chars()
        .map(|ch| match ch {
            '(' => "\\(".to_string(),
            ')' => "\\)".to_string(),
            '\\' => "\\\\".to_string(),
            _ => ch.to_string(),
        })
        .collect()
}

fn escape_pdf_string_char(ch: char) -> String {
    match ch {
        '(' => "\\(".into(),
        ')' => "\\)".into(),
        '\\' => "\\\\".into(),
        _ => ch.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::FieldFlags;

    #[test]
    fn test_parse_da_simple() {
        let da = parse_da("0 g /Helv 12 Tf");
        assert_eq!(da.font_name.as_deref(), Some("Helv"));
        assert_eq!(da.font_size, 12.0);
        assert_eq!(da.color, vec![0.0]);
    }
    #[test]
    fn test_parse_da_rgb() {
        let da = parse_da("0 0 1 rg /Cour 10 Tf");
        assert_eq!(da.font_name.as_deref(), Some("Cour"));
        assert_eq!(da.color, vec![0.0, 0.0, 1.0]);
    }
    #[test]
    fn test_escape() {
        assert_eq!(escape_pdf_string("a(b)"), "a\\(b\\)");
    }
    #[test]
    fn test_checkbox_appearance() {
        let mut tree = FieldTree::new();
        let id = tree.alloc(FieldNode {
            partial_name: "cb".into(),
            alternate_name: None,
            mapping_name: None,
            field_type: Some(FieldType::Button),
            flags: FieldFlags::empty(),
            value: None,
            default_value: None,
            default_appearance: None,
            quadding: None,
            max_len: None,
            options: vec![],
            top_index: None,
            rect: Some([0.0, 0.0, 12.0, 12.0]),
            appearance_state: None,
            page_index: None,
            parent: None,
            children: vec![],
            object_id: None,
            has_actions: false,
            mk: None,
            border_style: None,
        });
        assert!(
            generate_checkbox_appearance(&tree, id, true).len()
                > generate_checkbox_appearance(&tree, id, false).len()
        );
    }
}
