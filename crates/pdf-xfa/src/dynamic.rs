use std::collections::HashMap;

use xfa_layout_engine::form::{DrawContent, FormNodeId, FormNodeType, FormTree, GroupKind, Presence};

const MAX_SCRIPT_PASSES: usize = 8;

pub fn apply_dynamic_scripts(form: &mut FormTree, root_id: FormNodeId) -> usize {
    let parents = build_parent_map(form, root_id);
    let scripts: Vec<(FormNodeId, Vec<String>)> = form
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, _)| {
            let node_id = FormNodeId(idx);
            let scripts = form.meta(node_id).event_scripts.clone();
            (!scripts.is_empty()).then_some((node_id, scripts))
        })
        .collect();

    let mut total_changes = 0;
    for _ in 0..MAX_SCRIPT_PASSES {
        let mut pass_changes = 0;
        for (node_id, node_scripts) in &scripts {
            // Skip scripts on nodes whose ancestors are hidden — mirrors
            // Adobe Reader behavior where hidden containers don't fire events.
            if has_hidden_ancestor(form, &parents, *node_id) {
                continue;
            }
            for script in node_scripts {
                pass_changes += execute_script(form, root_id, &parents, *node_id, script);
            }
        }

        total_changes += pass_changes;
        if pass_changes == 0 {
            break;
        }
    }

    total_changes
}

fn has_hidden_ancestor(
    form: &FormTree,
    parents: &HashMap<FormNodeId, FormNodeId>,
    node_id: FormNodeId,
) -> bool {
    let mut cursor = parents.get(&node_id).copied();
    while let Some(ancestor) = cursor {
        if form.meta(ancestor).presence.is_not_visible() {
            return true;
        }
        cursor = parents.get(&ancestor).copied();
    }
    false
}

fn execute_script(
    form: &mut FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    script: &str,
) -> usize {
    let lines = preprocess_script(script);
    let mut idx = 0;
    execute_block(form, root_id, parents, current_id, &lines, &mut idx, false)
}

fn execute_block(
    form: &mut FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    lines: &[String],
    idx: &mut usize,
    stop_on_closing: bool,
) -> usize {
    let mut changes = 0;

    while *idx < lines.len() {
        let line = lines[*idx].trim();
        if line.is_empty() {
            *idx += 1;
            continue;
        }

        if line.starts_with('}') {
            *idx += 1;
            if stop_on_closing {
                break;
            }
            continue;
        }

        if let Some(condition) = parse_if_condition(line) {
            *idx += 1;
            if eval_condition(form, root_id, parents, current_id, &condition) {
                changes += execute_block(form, root_id, parents, current_id, lines, idx, true);
            } else {
                skip_block(lines, idx);
            }
            continue;
        }

        changes += execute_assignment(form, root_id, parents, current_id, line);
        *idx += 1;
    }

    changes
}

fn preprocess_script(script: &str) -> Vec<String> {
    let raw: Vec<String> = script
        .lines()
        .map(strip_line_comment)
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect();

    // Join continuation lines: a line ending with || or && or a trailing
    // comma is merged with the next line.  This handles multi-line `if`
    // conditions that span several source lines.
    let mut merged = Vec::with_capacity(raw.len());
    let mut buf = String::new();
    for line in &raw {
        if buf.is_empty() {
            buf = line.clone();
        } else {
            buf.push(' ');
            buf.push_str(line);
        }
        let trimmed = buf.trim_end();
        if trimmed.ends_with("||") || trimmed.ends_with("&&") || trimmed.ends_with(',') {
            // continuation — keep accumulating
            continue;
        }
        merged.push(std::mem::take(&mut buf));
    }
    if !buf.is_empty() {
        merged.push(buf);
    }
    merged
}

fn strip_line_comment(line: &str) -> &str {
    line.find("//").map(|idx| &line[..idx]).unwrap_or(line)
}

fn parse_if_condition(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("if") {
        return None;
    }

    let open = trimmed.find('(')?;
    let close = trimmed.rfind(')')?;
    (close > open).then(|| trimmed[open + 1..close].trim().to_string())
}

fn skip_block(lines: &[String], idx: &mut usize) {
    let mut depth = 1usize;
    while *idx < lines.len() && depth > 0 {
        let line = lines[*idx].trim();
        depth += line.chars().filter(|&ch| ch == '{').count();
        depth = depth.saturating_sub(line.chars().filter(|&ch| ch == '}').count());
        *idx += 1;
    }
}

fn execute_assignment(
    form: &mut FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    line: &str,
) -> usize {
    let statement = line.trim().trim_end_matches(';').trim();

    if statement == "Utils.hideIfEmpty(this)" {
        return hide_if_empty(form, current_id);
    }

    if statement == "Utils.deleteContainerIfEmpty(this)" {
        return delete_container_if_empty(form, parents, current_id);
    }

    let Some(eq_pos) = find_assignment_operator(statement) else {
        return 0;
    };

    let lhs = statement[..eq_pos].trim();
    let rhs = statement[eq_pos + 1..].trim();

    if let Some(target) = lhs.strip_suffix(".rawValue") {
        let Some(node_id) = resolve_reference(form, root_id, parents, current_id, target.trim())
        else {
            return 0;
        };
        let value = eval_value(form, root_id, parents, current_id, rhs);
        return set_raw_value(form, node_id, value);
    }

    if let Some(target) = lhs.strip_suffix(".presence") {
        let target_trimmed = target.trim();
        let resolved = resolve_reference(form, root_id, parents, current_id, target_trimmed);
        let Some(node_id) = resolved else {
            return 0;
        };
        let value = eval_value(form, root_id, parents, current_id, rhs);
        return set_presence(form, node_id, value);
    }

    0
}

fn hide_if_empty(form: &mut FormTree, node_id: FormNodeId) -> usize {
    if !node_is_empty(form, node_id) {
        return 0;
    }
    set_presence(form, node_id, ScriptValue::String("hidden".into()))
}

fn delete_container_if_empty(
    form: &mut FormTree,
    parents: &HashMap<FormNodeId, FormNodeId>,
    node_id: FormNodeId,
) -> usize {
    if !node_is_empty(form, node_id) {
        return 0;
    }
    let Some(parent_id) = parents.get(&node_id).copied() else {
        return 0;
    };
    set_presence(form, parent_id, ScriptValue::String("hidden".into()))
}

fn node_is_empty(form: &FormTree, node_id: FormNodeId) -> bool {
    match &form.get(node_id).node_type {
        FormNodeType::Field { value } => value.trim().is_empty(),
        FormNodeType::Draw(DrawContent::Text(content)) => content.trim().is_empty(),
        FormNodeType::Subform => form
            .get(node_id)
            .children
            .iter()
            .all(|&child_id| node_is_empty(form, child_id)),
        _ => false,
    }
}

fn find_assignment_operator(statement: &str) -> Option<usize> {
    let bytes = statement.as_bytes();
    for idx in 0..bytes.len() {
        if bytes[idx] != b'=' {
            continue;
        }
        let prev = idx.checked_sub(1).and_then(|prev| bytes.get(prev)).copied();
        let next = bytes.get(idx + 1).copied();
        if prev == Some(b'=') || prev == Some(b'!') || next == Some(b'=') {
            continue;
        }
        return Some(idx);
    }
    None
}

fn eval_condition(
    form: &FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    expr: &str,
) -> bool {
    let expr = strip_outer_parens(expr.trim());

    let or_parts = split_top_level(expr, "||");
    if or_parts.len() > 1 {
        return or_parts
            .iter()
            .any(|part| eval_condition(form, root_id, parents, current_id, part));
    }

    let and_parts = split_top_level(expr, "&&");
    if and_parts.len() > 1 {
        return and_parts
            .iter()
            .all(|part| eval_condition(form, root_id, parents, current_id, part));
    }

    if let Some((lhs, rhs, negated)) = split_comparison(expr) {
        let left = eval_value(form, root_id, parents, current_id, lhs);
        let right = eval_value(form, root_id, parents, current_id, rhs);
        let equals = values_equal(&left, &right);
        return if negated { !equals } else { equals };
    }

    match eval_value(form, root_id, parents, current_id, expr) {
        ScriptValue::Null => false,
        ScriptValue::String(value) => !value.trim().is_empty() && value.trim() != "0",
    }
}

fn split_comparison(expr: &str) -> Option<(&str, &str, bool)> {
    find_top_level_operator(expr, "==")
        .map(|idx| (&expr[..idx], &expr[idx + 2..], false))
        .or_else(|| {
            find_top_level_operator(expr, "!=").map(|idx| (&expr[..idx], &expr[idx + 2..], true))
        })
}

fn eval_value(
    form: &FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    expr: &str,
) -> ScriptValue {
    let expr = strip_outer_parens(expr.trim()).trim_end_matches(';').trim();
    if expr.is_empty() {
        return ScriptValue::Null;
    }

    if let Some(quoted) = parse_quoted_string(expr) {
        return ScriptValue::String(quoted);
    }

    if expr.eq_ignore_ascii_case("null") {
        return ScriptValue::Null;
    }

    if let Ok(number) = expr.parse::<f64>() {
        return ScriptValue::String(normalize_number(number));
    }

    if let Some(target) = expr.strip_suffix(".rawValue") {
        let Some(node_id) = resolve_reference(form, root_id, parents, current_id, target.trim())
        else {
            return ScriptValue::Null;
        };
        return get_raw_value(form, node_id);
    }

    if let Some(target) = expr.strip_suffix(".presence") {
        let Some(node_id) = resolve_reference(form, root_id, parents, current_id, target.trim())
        else {
            return ScriptValue::Null;
        };
        let meta = form.meta(node_id);
        return ScriptValue::String(
            match meta.presence {
                Presence::Visible => "visible",
                Presence::Hidden => "hidden",
                Presence::Invisible => "invisible",
                Presence::Inactive => "inactive",
            }
            .to_string(),
        );
    }

    ScriptValue::Null
}

fn parse_quoted_string(expr: &str) -> Option<String> {
    let bytes = expr.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"'))
    {
        return Some(expr[1..expr.len() - 1].to_string());
    }
    None
}

fn resolve_reference(
    form: &FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    target: &str,
) -> Option<FormNodeId> {
    let target = target.trim();
    if target == "this" {
        return Some(current_id);
    }

    let resolved_target = if let Some(path) = parse_resolve_node_call(target) {
        path
    } else {
        target.to_string()
    };

    let parts: Vec<&str> = resolved_target
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }

    let mut cursor = Some(current_id);
    while let Some(node_id) = cursor {
        for candidate in descendants_inclusive(form, node_id) {
            if form.get(candidate).name == parts[0] {
                if let Some(found) = follow_path(form, candidate, &parts) {
                    return Some(found);
                }
            }
        }
        cursor = parents.get(&node_id).copied();
    }

    for candidate in descendants_inclusive(form, root_id) {
        if form.get(candidate).name == parts[0] {
            if let Some(found) = follow_path(form, candidate, &parts) {
                return Some(found);
            }
        }
    }

    if parts.len() == 1 {
        for candidate in descendants_inclusive(form, root_id) {
            if form.get(candidate).name == parts[0] {
                return Some(candidate);
            }
        }
    }

    None
}

fn parse_resolve_node_call(target: &str) -> Option<String> {
    let target = target.trim();
    if !target.starts_with("xfa.resolveNode(") {
        return None;
    }
    let open = target.find('(')?;
    let close = target.rfind(')')?;
    parse_quoted_string(target[open + 1..close].trim())
}

fn follow_path(form: &FormTree, start: FormNodeId, parts: &[&str]) -> Option<FormNodeId> {
    if form.get(start).name != parts[0] {
        return None;
    }
    let mut current = start;
    for part in &parts[1..] {
        current = form
            .get(current)
            .children
            .iter()
            .copied()
            .find(|child_id| form.get(*child_id).name == *part)?;
    }
    Some(current)
}

fn descendants_inclusive(form: &FormTree, root_id: FormNodeId) -> Vec<FormNodeId> {
    let mut out = Vec::new();
    collect_descendants(form, root_id, &mut out);
    out
}

fn collect_descendants(form: &FormTree, node_id: FormNodeId, out: &mut Vec<FormNodeId>) {
    out.push(node_id);
    for &child_id in &form.get(node_id).children {
        collect_descendants(form, child_id, out);
    }
}

fn build_parent_map(form: &FormTree, root_id: FormNodeId) -> HashMap<FormNodeId, FormNodeId> {
    let mut parents = HashMap::new();
    populate_parent_map(form, root_id, &mut parents);
    parents
}

fn populate_parent_map(
    form: &FormTree,
    node_id: FormNodeId,
    parents: &mut HashMap<FormNodeId, FormNodeId>,
) {
    for &child_id in &form.get(node_id).children {
        parents.insert(child_id, node_id);
        populate_parent_map(form, child_id, parents);
    }
}

fn get_raw_value(form: &FormTree, node_id: FormNodeId) -> ScriptValue {
    match &form.get(node_id).node_type {
        FormNodeType::Field { value } => {
            if value.is_empty() {
                ScriptValue::Null
            } else {
                ScriptValue::String(value.clone())
            }
        }
        _ if form.meta(node_id).group_kind == GroupKind::ExclusiveChoice => {
            for &child_id in &form.get(node_id).children {
                if let FormNodeType::Field { value } = &form.get(child_id).node_type {
                    if !value.is_empty() {
                        return ScriptValue::String(
                            form.meta(child_id)
                                .item_value
                                .clone()
                                .unwrap_or_else(|| value.clone()),
                        );
                    }
                }
            }
            ScriptValue::Null
        }
        _ => ScriptValue::Null,
    }
}

fn set_raw_value(form: &mut FormTree, node_id: FormNodeId, value: ScriptValue) -> usize {
    let value = match value {
        ScriptValue::Null => String::new(),
        ScriptValue::String(value) => value,
    };

    if form.meta(node_id).group_kind == GroupKind::ExclusiveChoice {
        let mut changes = 0;
        for &child_id in &form.get(node_id).children.clone() {
            let item_value = form.meta(child_id).item_value.clone();
            let next = if item_value.as_deref() == Some(value.as_str()) {
                value.clone()
            } else {
                String::new()
            };
            if let FormNodeType::Field { value: field_value } =
                &mut form.get_mut(child_id).node_type
            {
                if *field_value != next {
                    *field_value = next;
                    changes += 1;
                }
            }
        }
        return changes;
    }

    if let FormNodeType::Field { value: field_value } = &mut form.get_mut(node_id).node_type {
        if *field_value != value {
            *field_value = value;
            return 1;
        }
    }

    0
}

fn set_presence(form: &mut FormTree, node_id: FormNodeId, value: ScriptValue) -> usize {
    let value = match value {
        ScriptValue::Null => return 0,
        ScriptValue::String(value) => value,
    };
    let normalized = value.trim().to_ascii_lowercase();
    let new_presence = match normalized.as_str() {
        "visible" | "open" => Presence::Visible,
        "hidden" => Presence::Hidden,
        "invisible" => Presence::Invisible,
        "inactive" => Presence::Inactive,
        _ => return 0,
    };

    let meta = form.meta_mut(node_id);
    if meta.presence == new_presence {
        return 0;
    }
    meta.presence = new_presence;
    1
}

fn split_top_level<'a>(expr: &'a str, op: &str) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let chars: Vec<(usize, char)> = expr.char_indices().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let (byte_idx, ch) = chars[i];
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && expr[byte_idx..].starts_with(op) {
            parts.push(expr[start..byte_idx].trim());
            start = byte_idx + op.len();
        }
        i += 1;
    }
    if parts.is_empty() {
        return vec![expr.trim()];
    }
    parts.push(expr[start..].trim());
    parts
}

fn find_top_level_operator(expr: &str, op: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (idx, ch) in expr.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && expr[idx..].starts_with(op) {
            return Some(idx);
        }
    }
    None
}

fn strip_outer_parens(expr: &str) -> &str {
    let mut current = expr.trim();
    loop {
        if !(current.starts_with('(') && current.ends_with(')')) {
            return current;
        }
        let mut depth = 0i32;
        let mut wraps = true;
        for (idx, ch) in current.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && idx != current.len() - 1 {
                        wraps = false;
                        break;
                    }
                }
                _ => {}
            }
        }
        if wraps {
            current = current[1..current.len() - 1].trim();
        } else {
            return current;
        }
    }
}

fn values_equal(left: &ScriptValue, right: &ScriptValue) -> bool {
    match (left, right) {
        (ScriptValue::Null, ScriptValue::Null) => true,
        (ScriptValue::String(left), ScriptValue::String(right)) => {
            if let (Ok(left_num), Ok(right_num)) = (left.parse::<f64>(), right.parse::<f64>()) {
                return (left_num - right_num).abs() < f64::EPSILON;
            }
            left == right
        }
        (ScriptValue::Null, ScriptValue::String(value))
        | (ScriptValue::String(value), ScriptValue::Null) => value.is_empty(),
    }
}

fn normalize_number(number: f64) -> String {
    if number.fract().abs() < f64::EPSILON {
        (number as i64).to_string()
    } else {
        number.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScriptValue {
    Null,
    String(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_layout_engine::form::{
        FieldKind, FormNode, FormNodeMeta, FormNodeStyle, GroupKind, Occur,
    };
    use xfa_layout_engine::text::FontMetrics;
    use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

    fn add_node(tree: &mut FormTree, name: &str, node_type: FormNodeType) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type,
            box_model: BoxModel::default(),
            layout: LayoutStrategy::TopToBottom,
            children: Vec::new(),
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: Vec::new(),
            col_span: 1,
        })
    }

    fn empty_meta() -> FormNodeMeta {
        FormNodeMeta {
            field_kind: FieldKind::Text,
            group_kind: GroupKind::None,
            style: FormNodeStyle::default(),
            ..Default::default()
        }
    }

    #[test]
    fn change_event_toggles_relative_hidden_subform() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let section = add_node(&mut tree, "Section", FormNodeType::Subform);
        let group = add_node(&mut tree, "Choice", FormNodeType::Subform);
        let option1 = add_node(
            &mut tree,
            "Option1",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );
        let option2 = add_node(
            &mut tree,
            "Option2",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        let details = add_node(&mut tree, "Details", FormNodeType::Subform);

        tree.get_mut(root).children = vec![section];
        tree.get_mut(section).children = vec![group, details];
        tree.get_mut(group).children = vec![option1, option2];

        tree.meta_mut(group).group_kind = GroupKind::ExclusiveChoice;
        tree.meta_mut(group).event_scripts = vec![
            "Details.presence = 'hidden';\nif (this.rawValue == 1) {\n  Details.presence = 'visible';\n}".into(),
        ];
        tree.meta_mut(option1).item_value = Some("1".into());
        tree.meta_mut(option2).item_value = Some("2".into());
        tree.meta_mut(details).presence = Presence::Hidden;

        apply_dynamic_scripts(&mut tree, root);

        assert_eq!(tree.meta(details).presence, Presence::Visible);
    }

    #[test]
    fn calculate_script_on_hidden_block_uses_sibling_values() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let section = add_node(&mut tree, "Section", FormNodeType::Subform);
        let option1 = add_node(
            &mut tree,
            "Opt1",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );
        let option2 = add_node(
            &mut tree,
            "Opt2",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        let details = add_node(&mut tree, "Details", FormNodeType::Subform);

        tree.get_mut(root).children = vec![section];
        tree.get_mut(section).children = vec![option1, option2, details];
        tree.meta_mut(details).presence = Presence::Hidden;
        tree.meta_mut(details).event_scripts = vec![
            "this.presence = 'hidden';\nif ((Opt1.rawValue == 1) || (Opt2.rawValue == 1)) {\n  this.presence = 'visible';\n}".into(),
        ];

        apply_dynamic_scripts(&mut tree, root);

        assert_eq!(tree.meta(details).presence, Presence::Visible);
    }

    #[test]
    fn multi_pass_scripts_propagate_raw_values() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let section = add_node(&mut tree, "Section", FormNodeType::Subform);
        let controller = add_node(
            &mut tree,
            "Controller",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );
        let target = add_node(
            &mut tree,
            "Target",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        let details = add_node(&mut tree, "Details", FormNodeType::Subform);

        tree.get_mut(root).children = vec![section];
        tree.get_mut(section).children = vec![controller, target, details];

        tree.meta_mut(controller).event_scripts =
            vec!["if (this.rawValue == 1) {\n  Target.rawValue = 1;\n}".into()];
        tree.meta_mut(details).presence = Presence::Hidden;
        tree.meta_mut(details).event_scripts = vec![
            "this.presence = 'hidden';\nif (Target.rawValue == 1) {\n  this.presence = 'visible';\n}".into(),
        ];

        apply_dynamic_scripts(&mut tree, root);

        if let FormNodeType::Field { value } = &tree.get(target).node_type {
            assert_eq!(value, "1");
        } else {
            panic!("expected field");
        }
        assert_eq!(tree.meta(details).presence, Presence::Visible);
    }

    #[test]
    fn resolve_node_calls_are_supported() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let form = add_node(&mut tree, "formulier1", FormNodeType::Subform);
        let admin = add_node(&mut tree, "ADMIN", FormNodeType::Subform);
        let lock = add_node(
            &mut tree,
            "LockForm_AD",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );
        let reset = add_node(
            &mut tree,
            "Reset",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );

        tree.get_mut(root).children = vec![form];
        tree.get_mut(form).children = vec![admin, reset];
        tree.get_mut(admin).children = vec![lock];
        tree.meta_mut(reset).event_scripts =
            vec!["xfa.resolveNode(\"formulier1.ADMIN.LockForm_AD\").rawValue = 0;".into()];

        apply_dynamic_scripts(&mut tree, root);

        if let FormNodeType::Field { value } = &tree.get(lock).node_type {
            assert_eq!(value, "0");
        } else {
            panic!("expected field");
        }
    }

    #[test]
    fn utils_hide_if_empty_hides_current_node() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let empty = add_node(
            &mut tree,
            "EmptyField",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        tree.get_mut(root).children = vec![empty];
        tree.meta_mut(empty).event_scripts = vec!["Utils.hideIfEmpty(this);".into()];

        apply_dynamic_scripts(&mut tree, root);

        assert!(tree.meta(empty).presence.is_not_visible());
    }

    #[test]
    fn utils_delete_container_if_empty_hides_parent_container() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let container = add_node(&mut tree, "Container", FormNodeType::Subform);
        let empty = add_node(
            &mut tree,
            "EmptyField",
            FormNodeType::Field {
                value: String::new(),
            },
        );

        tree.get_mut(root).children = vec![container];
        tree.get_mut(container).children = vec![empty];
        tree.meta_mut(empty).event_scripts = vec!["Utils.deleteContainerIfEmpty(this);".into()];

        apply_dynamic_scripts(&mut tree, root);

        assert!(tree.meta(container).presence.is_not_visible());
    }

    #[test]
    fn default_meta_helper_is_constructible() {
        let meta = empty_meta();
        assert_eq!(meta.group_kind, GroupKind::None);
    }
}
