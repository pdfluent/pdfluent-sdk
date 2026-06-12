//! Complete AcroForm model export for editor/binding consumption (C-chain).
//!
//! [`build_form_model`] flattens a parsed [`FieldTree`] into a list of
//! [`FormFieldModel`] values — one per *logical* field — with everything an
//! interactive form UI needs: typed kind, per-page widget rectangles, radio
//! on-state values, checkbox on-states, choice options, comb/multiline
//! flags, length limits, access flags and resolved default-appearance info.
//!
//! The model is deliberately plain data (no lifetimes, no arena ids) so
//! facades and language bindings can serialize it as-is.

use crate::appearance::parse_da;
use crate::button::{button_kind, ButtonKind};
use crate::flags::FieldFlags;
use crate::tree::{ChoiceOption, FieldId, FieldTree, FieldType, FieldValue, Quadding};

/// The kind of a logical form field, with kind-specific data inline.
#[derive(Debug, Clone, PartialEq)]
pub enum FormFieldKind {
    /// Single-line or multiline text input.
    Text {
        /// Multiline flag (`/Ff` bit 13).
        multiline: bool,
        /// Comb flag (`/Ff` bit 25) — fixed character cells; only meaningful
        /// together with `max_len`.
        comb: bool,
        /// Password flag (`/Ff` bit 14) — render input masked.
        password: bool,
    },
    /// A two-state checkbox.
    Checkbox {
        /// The widget's on-state name (`/AP /N` key); `Yes` when the widget
        /// declares none. Pass to setters; `Off` is always the off state.
        on_state: String,
        /// Whether the box is currently checked.
        checked: bool,
    },
    /// A radio group: one logical field, one widget per option.
    RadioGroup {
        /// Export (on-state) value per option, in widget order. The widget
        /// at `widgets[i]` toggles on when the group value is `options[i]`.
        options: Vec<String>,
    },
    /// Combo box (dropdown), optionally editable.
    ComboBox {
        /// Edit flag (`/Ff` bit 19) — free text allowed besides options.
        editable: bool,
        /// `/Opt` entries.
        options: Vec<ChoiceOption>,
    },
    /// List box, optionally multi-select.
    ListBox {
        /// MultiSelect flag (`/Ff` bit 22).
        multi_select: bool,
        /// `/Opt` entries.
        options: Vec<ChoiceOption>,
    },
    /// Push button — no persistent value.
    PushButton,
    /// Signature field.
    Signature,
}

/// One widget annotation of a logical field.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetModel {
    /// 0-based page index, when the widget was found on a page.
    pub page_index: Option<usize>,
    /// Widget rectangle `[x0, y0, x1, y1]` in PDF user space.
    pub rect: [f32; 4],
    /// Button widgets: this widget's on-state name from its `/AP /N`.
    pub on_state: Option<String>,
    /// Current appearance state (`/AS`), when present.
    pub appearance_state: Option<String>,
}

/// Resolved default-appearance info for a field (from `/DA`, inherited).
#[derive(Debug, Clone, PartialEq)]
pub struct DaInfo {
    /// Font resource name (e.g. `Helv`), without the leading slash.
    pub font_name: Option<String>,
    /// Font size in points; `0` means auto-size.
    pub font_size: f32,
    /// Text color components (1 = gray, 3 = RGB, 4 = CMYK).
    pub color: Vec<f32>,
}

/// A logical form field with everything an editor UI needs.
#[derive(Debug, Clone, PartialEq)]
pub struct FormFieldModel {
    /// Fully-qualified field name — the handle for all get/set operations.
    pub name: String,
    /// Typed kind with kind-specific data.
    pub kind: FormFieldKind,
    /// Current value: text fields the text, buttons the on-state name or
    /// `Off`, choice fields the selected export value (joined with `", "` for
    /// multi-select list boxes — use `selected_values` for the array form).
    pub value: Option<String>,
    /// Selected export values for multi-select list boxes (`/Ff` bit 22 set).
    /// `None` for all other field types. Empty vec means nothing selected.
    pub selected_values: Option<Vec<String>>,
    /// Default value (`/DV`).
    pub default_value: Option<String>,
    /// User-facing label (`/TU`, the accessibility/tooltip name).
    pub tooltip: Option<String>,
    /// ReadOnly flag (`/Ff` bit 1, inherited).
    pub read_only: bool,
    /// Required flag (`/Ff` bit 2, inherited).
    pub required: bool,
    /// `/MaxLen` for text fields (inherited).
    pub max_len: Option<u32>,
    /// Text alignment.
    pub quadding: Quadding,
    /// Resolved `/DA` font/size/color info.
    pub da: DaInfo,
    /// All widget annotations of this field (one per visual occurrence).
    pub widgets: Vec<WidgetModel>,
}

/// Build the editor-facing model from a parsed field tree.
///
/// Logical fields are tree nodes that carry (or inherit) a field type and
/// have no *named* children: a radio group whose kids are unnamed widgets is
/// ONE logical field; a container with named children contributes its
/// children, not itself.
pub fn build_form_model(tree: &FieldTree) -> Vec<FormFieldModel> {
    let mut out = Vec::new();
    for id in tree.all_ids() {
        if !is_logical_field(tree, id) {
            continue;
        }
        if let Some(model) = field_model(tree, id) {
            out.push(model);
        }
    }
    out
}

/// A node is a logical field when it resolves to a field type and none of
/// its children are named fields themselves.
fn is_logical_field(tree: &FieldTree, id: FieldId) -> bool {
    if tree.effective_field_type(id).is_none() {
        return false;
    }
    let node = tree.get(id);
    // Unnamed nodes are widgets of their parent, not fields.
    if node.partial_name.is_empty() && node.parent.is_some() {
        return false;
    }
    // A node with named children is a container.
    !node
        .children
        .iter()
        .any(|&c| !tree.get(c).partial_name.is_empty())
}

fn field_model(tree: &FieldTree, id: FieldId) -> Option<FormFieldModel> {
    let node = tree.get(id);
    let ft = tree.effective_field_type(id)?;
    let flags = effective_flags_deep(tree, id);

    let widgets = collect_widgets(tree, id);
    let raw_value = tree.effective_value(id);
    let selected_values = match &raw_value {
        Some(FieldValue::StringArray(arr)) => Some(arr.clone()),
        _ => None,
    };
    let value = raw_value.map(value_to_string);
    let default_value = node.default_value.as_ref().map(value_to_string);

    let kind = match ft {
        FieldType::Text => FormFieldKind::Text {
            multiline: flags.multiline(),
            comb: flags.comb(),
            password: flags.password(),
        },
        FieldType::Button => match button_kind(flags) {
            ButtonKind::PushButton => FormFieldKind::PushButton,
            ButtonKind::Checkbox => {
                let on_state = widgets
                    .iter()
                    .find_map(|w| w.on_state.clone())
                    .unwrap_or_else(|| "Yes".to_string());
                let checked = value.as_deref().is_some_and(|v| v != "Off")
                    || widgets
                        .iter()
                        .any(|w| w.appearance_state.as_deref().is_some_and(|s| s != "Off"));
                FormFieldKind::Checkbox { on_state, checked }
            }
            ButtonKind::Radio => FormFieldKind::RadioGroup {
                options: widgets
                    .iter()
                    .map(|w| w.on_state.clone().unwrap_or_default())
                    .collect(),
            },
        },
        FieldType::Choice => {
            if flags.combo() {
                FormFieldKind::ComboBox {
                    editable: flags.edit(),
                    options: node.options.clone(),
                }
            } else {
                FormFieldKind::ListBox {
                    multi_select: flags.multi_select(),
                    options: node.options.clone(),
                }
            }
        }
        FieldType::Signature => FormFieldKind::Signature,
    };

    let da_str = tree.effective_da(id).unwrap_or("/Helv 0 Tf 0 g");
    let da = parse_da(da_str);

    Some(FormFieldModel {
        name: tree.fully_qualified_name(id),
        kind,
        value,
        selected_values,
        default_value,
        tooltip: node.alternate_name.clone(),
        read_only: flags.read_only(),
        required: flags.required(),
        max_len: tree.effective_max_len(id),
        quadding: tree.effective_quadding(id),
        da: DaInfo {
            font_name: da.font_name,
            font_size: da.font_size,
            color: da.color,
        },
        widgets,
    })
}

/// Effective flags: the node's own, else the nearest ancestor's (the /Ff
/// entry is inheritable per ISO 32000-1 Table 220).
fn effective_flags_deep(tree: &FieldTree, id: FieldId) -> FieldFlags {
    let mut cur = Some(id);
    while let Some(cid) = cur {
        let node = tree.get(cid);
        if node.flags.bits() != 0 {
            return node.flags;
        }
        cur = node.parent;
    }
    FieldFlags::empty()
}

fn collect_widgets(tree: &FieldTree, id: FieldId) -> Vec<WidgetModel> {
    let node = tree.get(id);
    let mut widgets = Vec::new();
    if node.children.is_empty() {
        if let Some(rect) = node.rect {
            widgets.push(WidgetModel {
                page_index: node.page_index,
                rect,
                on_state: node.on_state.clone(),
                appearance_state: node.appearance_state.clone(),
            });
        }
    } else {
        for &kid in &node.children {
            let k = tree.get(kid);
            if let Some(rect) = k.rect {
                widgets.push(WidgetModel {
                    page_index: k.page_index,
                    rect,
                    on_state: k.on_state.clone(),
                    appearance_state: k.appearance_state.clone(),
                });
            }
        }
    }
    widgets
}

fn value_to_string(v: &FieldValue) -> String {
    match v {
        FieldValue::Text(s) => s.clone(),
        FieldValue::StringArray(arr) => arr.join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::FieldNode;

    fn blank_node(name: &str) -> FieldNode {
        FieldNode {
            partial_name: name.into(),
            alternate_name: None,
            mapping_name: None,
            field_type: None,
            flags: FieldFlags::empty(),
            value: None,
            default_value: None,
            default_appearance: None,
            quadding: None,
            max_len: None,
            options: vec![],
            top_index: None,
            rect: None,
            appearance_state: None,
            on_state: None,
            page_index: None,
            parent: None,
            children: vec![],
            object_id: None,
            has_actions: false,
            mk: None,
            border_style: None,
        }
    }

    #[test]
    fn radio_group_is_one_logical_field_with_widget_options() {
        let mut tree = FieldTree::new();
        let mut group = blank_node("kleur");
        group.field_type = Some(FieldType::Button);
        group.flags = FieldFlags::from_bits(1 << 15); // Radio
        let gid = tree.alloc(group);
        for (i, state) in ["Rood", "Blauw"].iter().enumerate() {
            let mut w = blank_node("");
            w.parent = Some(gid);
            w.rect = Some([0.0, i as f32 * 20.0, 10.0, i as f32 * 20.0 + 10.0]);
            w.on_state = Some(state.to_string());
            w.appearance_state = Some("Off".into());
            let wid = tree.alloc(w);
            tree.get_mut(gid).children.push(wid);
        }

        let model = build_form_model(&tree);
        assert_eq!(model.len(), 1, "group + 2 widgets must be ONE field");
        let f = &model[0];
        assert_eq!(f.name, "kleur");
        assert_eq!(
            f.kind,
            FormFieldKind::RadioGroup {
                options: vec!["Rood".into(), "Blauw".into()]
            }
        );
        assert_eq!(f.widgets.len(), 2);
        assert_eq!(f.widgets[1].on_state.as_deref(), Some("Blauw"));
    }

    #[test]
    fn comb_and_multiline_flags_surface_in_kind() {
        let mut tree = FieldTree::new();
        let mut n = blank_node("bsn");
        n.field_type = Some(FieldType::Text);
        n.flags = FieldFlags::from_bits(1 << 24); // Comb
        n.max_len = Some(9);
        n.rect = Some([0.0, 0.0, 90.0, 12.0]);
        tree.alloc(n);

        let model = build_form_model(&tree);
        assert_eq!(model.len(), 1);
        assert_eq!(
            model[0].kind,
            FormFieldKind::Text {
                multiline: false,
                comb: true,
                password: false
            }
        );
        assert_eq!(model[0].max_len, Some(9));
    }

    #[test]
    fn container_with_named_children_is_not_a_field() {
        let mut tree = FieldTree::new();
        let mut parent = blank_node("adres");
        parent.field_type = Some(FieldType::Text);
        let pid = tree.alloc(parent);
        let mut kid = blank_node("straat");
        kid.parent = Some(pid);
        kid.rect = Some([0.0, 0.0, 100.0, 12.0]);
        let kid_id = tree.alloc(kid);
        tree.get_mut(pid).children.push(kid_id);

        let model = build_form_model(&tree);
        assert_eq!(model.len(), 1);
        assert_eq!(model[0].name, "adres.straat");
    }

    #[test]
    fn checkbox_reports_on_state_and_checked() {
        let mut tree = FieldTree::new();
        let mut n = blank_node("akkoord");
        n.field_type = Some(FieldType::Button);
        n.rect = Some([0.0, 0.0, 12.0, 12.0]);
        n.on_state = Some("Akkoord".into());
        n.appearance_state = Some("Akkoord".into());
        n.value = Some(FieldValue::Text("Akkoord".into()));
        tree.alloc(n);

        let model = build_form_model(&tree);
        assert_eq!(
            model[0].kind,
            FormFieldKind::Checkbox {
                on_state: "Akkoord".into(),
                checked: true
            }
        );
    }
}
