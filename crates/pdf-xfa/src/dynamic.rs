use std::collections::HashMap;

use formcalc_interpreter::{
    interpreter::Interpreter, lexer::tokenize, parser, som_bridge::SomResolver,
    value::Value as FormCalcValue,
};
use xfa_dom_resolver::som::{parse_som, SomExpression, SomIndex, SomRoot, SomSelector};
use xfa_layout_engine::form::{
    DrawContent, EventScript, FormNodeId, FormNodeType, FormTree, GroupKind, Presence,
    ScriptLanguage,
};

// XFA Spec 3.3 §9.3 — Dynamic Forms Re-Layout: after script execution the
// layout processor must re-run layout.  The spec does not prescribe a fixed
// pass limit; Adobe typically converges in 2-3 passes.  Our limit of 3 is a
// pragmatic cap that matches observed Adobe behavior.
const MAX_SCRIPT_PASSES: usize = 3;

/// Snapshot of field values and presence states, used for rollback.
/// NOTE: This rollback mechanism is our own heuristic — the XFA spec does not
/// define a rollback model.  It protects against scripts that blank out all
/// fields (broken SOM resolution, etc.).
struct FormSnapshot {
    field_values: Vec<(usize, String)>,
    presences: Vec<(usize, Presence)>,
    populated_count: usize,
}

fn snapshot_form(form: &FormTree) -> FormSnapshot {
    let mut field_values = Vec::new();
    let mut presences = Vec::new();
    let mut populated_count = 0usize;
    for (idx, node) in form.nodes.iter().enumerate() {
        if let FormNodeType::Field { value } = &node.node_type {
            field_values.push((idx, value.clone()));
            if !value.trim().is_empty() {
                populated_count += 1;
            }
        }
        presences.push((idx, form.metadata[idx].presence));
    }
    FormSnapshot {
        field_values,
        presences,
        populated_count,
    }
}

fn restore_snapshot(form: &mut FormTree, snapshot: &FormSnapshot) {
    for (idx, value) in &snapshot.field_values {
        if let FormNodeType::Field { value: fv } = &mut form.nodes[*idx].node_type {
            *fv = value.clone();
        }
    }
    for (idx, presence) in &snapshot.presences {
        form.metadata[*idx].presence = *presence;
    }
}

fn should_rollback(
    form: &FormTree,
    snapshot: &FormSnapshot,
    errors: usize,
    successes: usize,
) -> bool {
    if errors > 0 && errors > successes {
        return true;
    }
    if snapshot.populated_count >= 2 {
        let mut now_empty = 0usize;
        for (idx, old_value) in &snapshot.field_values {
            if old_value.trim().is_empty() {
                continue;
            }
            if let FormNodeType::Field { value } = &form.nodes[*idx].node_type {
                if value.trim().is_empty() {
                    now_empty += 1;
                }
            }
        }
        if now_empty * 2 > snapshot.populated_count {
            return true;
        }
    }
    false
}

// XFA Spec 3.3 §9.3 — Dynamic Forms: after data binding, scripts run in
// two phases: (1) initialize events fire once, (2) calculate events may
// iterate until stable (convergence) or MAX_SCRIPT_PASSES is reached.
// The spec (§14.3.2) defines the event model; our implementation runs
// initialize then calculate, matching Adobe's processing order.
//
// NOTE: §10.6 Rule 3 states the merge-completion order as:
//   value calcs → property calcs → validations → initialize events.
// Our order (initialize first) differs from the spec but matches Adobe's
// observed behavior on our 20K test corpus (97%+ SSIM). §28.2 (p1231)
// documents Adobe's event execution insert-at-position-2 algorithm.
pub fn apply_dynamic_scripts(form: &mut FormTree, root_id: FormNodeId) -> usize {
    let parents = build_parent_map(form, root_id);
    let scripts: Vec<(FormNodeId, Vec<EventScript>)> = form
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, _)| {
            let node_id = FormNodeId(idx);
            let scripts = form.meta(node_id).event_scripts.clone();
            (!scripts.is_empty()).then_some((node_id, scripts))
        })
        .collect();

    let snapshot = snapshot_form(form);
    let mut stats = ScriptStats::default();

    let changes = run_script_phase(
        form,
        root_id,
        &parents,
        &scripts,
        ScriptPhase::Initialize,
        1,
        &mut stats,
    ) + run_script_phase(
        form,
        root_id,
        &parents,
        &scripts,
        ScriptPhase::Calculate,
        MAX_SCRIPT_PASSES,
        &mut stats,
    );

    if should_rollback(form, &snapshot, stats.errors, stats.successes) {
        restore_snapshot(form, &snapshot);
        return 0;
    }

    changes
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptPhase {
    Initialize,
    Calculate,
}

#[derive(Default)]
struct ScriptStats {
    errors: usize,
    successes: usize,
}

struct ScriptResult {
    changes: usize,
    error: bool,
}

fn run_script_phase(
    form: &mut FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    scripts: &[(FormNodeId, Vec<EventScript>)],
    phase: ScriptPhase,
    max_passes: usize,
    stats: &mut ScriptStats,
) -> usize {
    let mut total_changes = 0;

    for _ in 0..max_passes {
        let mut pass_changes = 0;

        for (node_id, node_scripts) in scripts {
            if has_hidden_ancestor(form, parents, *node_id) {
                continue;
            }

            for script in node_scripts
                .iter()
                .filter(|script| should_run_script(script, phase))
            {
                let result = execute_event_script(form, root_id, parents, *node_id, script, phase);
                if result.error {
                    stats.errors += 1;
                } else {
                    stats.successes += 1;
                }
                pass_changes += result.changes;
            }
        }

        total_changes += pass_changes;
        if pass_changes == 0 {
            break;
        }
    }

    total_changes
}

fn should_run_script(script: &EventScript, phase: ScriptPhase) -> bool {
    match phase {
        ScriptPhase::Initialize => script.activity.as_deref() == Some("initialize"),
        ScriptPhase::Calculate => script.activity.as_deref() == Some("calculate"),
    }
}

fn execute_event_script(
    form: &mut FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    script: &EventScript,
    phase: ScriptPhase,
) -> ScriptResult {
    match script.language {
        ScriptLanguage::FormCalc => {
            execute_formcalc_script(form, root_id, parents, current_id, script, phase)
        }
        ScriptLanguage::JavaScript | ScriptLanguage::Other => {
            execute_javascript_script(form, root_id, parents, current_id, &script.script)
        }
    }
}

fn execute_formcalc_script(
    form: &mut FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    script: &EventScript,
    phase: ScriptPhase,
) -> ScriptResult {
    let Ok(tokens) = tokenize(&script.script) else {
        return ScriptResult {
            changes: 0,
            error: true,
        };
    };
    let Ok(ast) = parser::parse(tokens) else {
        return ScriptResult {
            changes: 0,
            error: true,
        };
    };

    let mut interpreter = Interpreter::new();
    let mut resolver = FormTreeSomResolver::new(form, root_id, parents, current_id);
    let Ok(result) = interpreter.exec_with_resolver(&ast, &mut resolver) else {
        return ScriptResult {
            changes: resolver.changes,
            error: true,
        };
    };

    if matches!(phase, ScriptPhase::Calculate) {
        resolver.changes += write_formcalc_value(
            resolver.form,
            current_id,
            ResolvedProperty::RawValue,
            result,
        );
    }

    ScriptResult {
        changes: resolver.changes,
        error: false,
    }
}

fn execute_javascript_script(
    form: &mut FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    script: &str,
) -> ScriptResult {
    let lines = preprocess_script(script);
    let mut idx = 0;
    let changes =
        execute_javascript_block(form, root_id, parents, current_id, &lines, &mut idx, false);
    let has_statements = lines.iter().any(|l| !l.trim().is_empty());
    ScriptResult {
        changes,
        error: has_statements && changes == 0,
    }
}

fn execute_javascript_block(
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
            if eval_condition_legacy(form, root_id, parents, current_id, &condition) {
                changes +=
                    execute_javascript_block(form, root_id, parents, current_id, lines, idx, true);
            } else {
                skip_block(lines, idx);
            }
            continue;
        }

        changes += execute_assignment_legacy(form, root_id, parents, current_id, line);
        *idx += 1;
    }

    changes
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedProperty {
    RawValue,
    Presence,
    SomExpression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolvedTarget {
    node_id: FormNodeId,
    property: ResolvedProperty,
}

struct FormTreeSomResolver<'a> {
    form: &'a mut FormTree,
    root_id: FormNodeId,
    parents: &'a HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    changes: usize,
}

impl<'a> FormTreeSomResolver<'a> {
    fn new(
        form: &'a mut FormTree,
        root_id: FormNodeId,
        parents: &'a HashMap<FormNodeId, FormNodeId>,
        current_id: FormNodeId,
    ) -> Self {
        Self {
            form,
            root_id,
            parents,
            current_id,
            changes: 0,
        }
    }

    fn resolve_target(&self, path: &str) -> Option<ResolvedTarget> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return None;
        }

        if matches!(trimmed, "rawValue" | "presence" | "somExpression") {
            return Some(ResolvedTarget {
                node_id: self.current_id,
                property: parse_property_name(trimmed)?,
            });
        }

        let (expr, property) = split_property_path(trimmed)?;
        let node_id = self.resolve_expression(&expr)?.into_iter().next()?;
        Some(ResolvedTarget { node_id, property })
    }

    fn count_targets(&self, path: &str) -> usize {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return 0;
        }
        if matches!(trimmed, "rawValue" | "presence" | "somExpression") {
            return 1;
        }
        let Some((expr, _property)) = split_property_path(trimmed) else {
            return 0;
        };
        self.resolve_expression(&expr)
            .map_or(0, |nodes| nodes.len())
    }

    fn resolve_expression(&self, expr: &SomExpression) -> Option<Vec<FormNodeId>> {
        match expr.root {
            SomRoot::Data | SomRoot::Record | SomRoot::Template => None,
            SomRoot::CurrentContainer => {
                if expr.segments.is_empty() {
                    Some(vec![self.current_id])
                } else {
                    Some(self.follow_absolute(vec![self.current_id], &expr.segments))
                }
            }
            SomRoot::Form => {
                if expr.segments.is_empty() {
                    Some(vec![self.root_id])
                } else {
                    Some(self.follow_absolute(vec![self.root_id], &expr.segments))
                }
            }
            SomRoot::Xfa => {
                let segments = strip_xfa_form_prefix(&expr.segments);
                if segments.is_empty() {
                    Some(vec![self.root_id])
                } else {
                    Some(self.follow_absolute(vec![self.root_id], segments))
                }
            }
            SomRoot::Unqualified => {
                if expr.segments.is_empty() {
                    Some(vec![self.current_id])
                } else {
                    Some(self.follow_unqualified(&expr.segments))
                }
            }
        }
    }

    fn follow_absolute(
        &self,
        mut current: Vec<FormNodeId>,
        segments: &[xfa_dom_resolver::som::SomSegment],
    ) -> Vec<FormNodeId> {
        for (idx, segment) in segments.iter().enumerate() {
            let allow_self = idx == 0;
            current = current
                .into_iter()
                .flat_map(|node_id| self.step_from_node(node_id, segment, allow_self))
                .collect();
            if current.is_empty() {
                break;
            }
        }
        current
    }

    fn follow_unqualified(
        &self,
        segments: &[xfa_dom_resolver::som::SomSegment],
    ) -> Vec<FormNodeId> {
        let Some((first, rest)) = segments.split_first() else {
            return vec![self.current_id];
        };

        let mut scope = Some(self.current_id);
        while let Some(scope_id) = scope {
            let anchors: Vec<_> = descendants_inclusive(self.form, scope_id)
                .into_iter()
                .filter(|node_id| self.node_matches_segment(*node_id, first))
                .collect();
            let matched = self.follow_remaining(anchors, rest);
            if !matched.is_empty() {
                return matched;
            }
            scope = self.parents.get(&scope_id).copied();
        }

        let anchors: Vec<_> = descendants_inclusive(self.form, self.root_id)
            .into_iter()
            .filter(|node_id| self.node_matches_segment(*node_id, first))
            .collect();
        self.follow_remaining(anchors, rest)
    }

    fn follow_remaining(
        &self,
        mut current: Vec<FormNodeId>,
        segments: &[xfa_dom_resolver::som::SomSegment],
    ) -> Vec<FormNodeId> {
        for segment in segments {
            current = current
                .into_iter()
                .flat_map(|node_id| self.step_from_node(node_id, segment, false))
                .collect();
            if current.is_empty() {
                break;
            }
        }
        current
    }

    fn step_from_node(
        &self,
        node_id: FormNodeId,
        segment: &xfa_dom_resolver::som::SomSegment,
        allow_self: bool,
    ) -> Vec<FormNodeId> {
        // XFA-F3-06: `..` (parent) navigation — a segment whose name is an
        // empty string (produced by the `.` separator after `..` in the raw path)
        // or literally ".." navigates to the parent node.
        if let SomSelector::Name(name) = &segment.selector {
            if name == ".." {
                // Navigate to parent
                if let Some(&parent_id) = self.parents.get(&node_id) {
                    return apply_index_to_single(parent_id, segment.index);
                }
                return Vec::new();
            }
        }

        if allow_self && self.node_matches_selector(node_id, &segment.selector) {
            return apply_index_to_single(node_id, segment.index);
        }

        let matches: Vec<_> = self
            .form
            .get(node_id)
            .children
            .iter()
            .copied()
            .filter(|child_id| self.node_matches_selector(*child_id, &segment.selector))
            .collect();

        apply_index(matches, segment.index)
    }

    fn node_matches_segment(
        &self,
        node_id: FormNodeId,
        segment: &xfa_dom_resolver::som::SomSegment,
    ) -> bool {
        if !self.node_matches_selector(node_id, &segment.selector) {
            return false;
        }

        match segment.index {
            SomIndex::All => true,
            SomIndex::None => self.sibling_position(node_id, &segment.selector) == Some(0),
            SomIndex::Specific(idx) => {
                self.sibling_position(node_id, &segment.selector) == Some(idx)
            }
        }
    }

    fn sibling_position(&self, node_id: FormNodeId, selector: &SomSelector) -> Option<usize> {
        let Some(parent_id) = self.parents.get(&node_id).copied() else {
            return self.node_matches_selector(node_id, selector).then_some(0);
        };

        self.form
            .get(parent_id)
            .children
            .iter()
            .copied()
            .filter(|candidate| self.node_matches_selector(*candidate, selector))
            .position(|candidate| candidate == node_id)
    }

    fn node_matches_selector(&self, node_id: FormNodeId, selector: &SomSelector) -> bool {
        match selector {
            SomSelector::Name(name) => self.form.get(node_id).name == *name,
            SomSelector::Class(class_name) => self.node_matches_class(node_id, class_name),
            SomSelector::AllChildren => true,
        }
    }

    fn node_matches_class(&self, node_id: FormNodeId, class_name: &str) -> bool {
        let class_name = class_name.to_ascii_lowercase();
        match class_name.as_str() {
            "subform" => matches!(
                self.form.get(node_id).node_type,
                FormNodeType::Root | FormNodeType::Subform
            ),
            "pageset" => {
                matches!(self.form.get(node_id).node_type, FormNodeType::PageSet)
            }
            "pagearea" => matches!(
                self.form.get(node_id).node_type,
                FormNodeType::PageArea { .. }
            ),
            "field" => matches!(self.form.get(node_id).node_type, FormNodeType::Field { .. }),
            "draw" => matches!(
                self.form.get(node_id).node_type,
                FormNodeType::Draw(_) | FormNodeType::Image { .. }
            ),
            "exclgroup" => self.form.meta(node_id).group_kind == GroupKind::ExclusiveChoice,
            _ => false,
        }
    }
}

impl SomResolver for FormTreeSomResolver<'_> {
    fn resolve_path(
        &mut self,
        path: &str,
    ) -> formcalc_interpreter::error::Result<Option<FormCalcValue>> {
        let Some(target) = self.resolve_target(path) else {
            // XFA-F3-06: log a warning instead of silently returning None so
            // that SOM path failures are diagnosable.
            if !path.trim().is_empty() {
                log::warn!("SOM bridge: path not resolved: {:?}", path.trim());
            }
            return Ok(None);
        };
        Ok(Some(read_formcalc_value(
            self.form,
            self.root_id,
            self.parents,
            target,
        )))
    }

    fn assign_path(
        &mut self,
        path: &str,
        value: FormCalcValue,
    ) -> formcalc_interpreter::error::Result<bool> {
        let Some(target) = self.resolve_target(path) else {
            // XFA-F3-06: descriptive warning on assignment failure.
            if !path.trim().is_empty() {
                log::warn!("SOM bridge: assignment target not found: {:?}", path.trim());
            }
            return Ok(false);
        };
        self.changes += write_formcalc_value(self.form, target.node_id, target.property, value);
        Ok(true)
    }

    fn count_path_matches(&mut self, path: &str) -> formcalc_interpreter::error::Result<usize> {
        Ok(self.count_targets(path))
    }
}

fn split_property_path(path: &str) -> Option<(SomExpression, ResolvedProperty)> {
    let normalized = if let Some(rest) = path.strip_prefix("this.") {
        format!("$.{rest}")
    } else if path == "this" {
        "$".to_string()
    } else {
        path.to_string()
    };

    let mut expr = parse_som(&normalized).ok()?;
    let property = if let Some(last) = expr.segments.last() {
        match &last.selector {
            SomSelector::Name(name) => {
                parse_property_name(name).unwrap_or(ResolvedProperty::RawValue)
            }
            _ => ResolvedProperty::RawValue,
        }
    } else {
        ResolvedProperty::RawValue
    };

    if matches!(
        expr.segments.last().map(|segment| &segment.selector),
        Some(SomSelector::Name(name)) if parse_property_name(name).is_some()
    ) {
        expr.segments.pop();
    }

    Some((expr, property))
}

fn parse_property_name(name: &str) -> Option<ResolvedProperty> {
    match name {
        "rawValue" => Some(ResolvedProperty::RawValue),
        "presence" => Some(ResolvedProperty::Presence),
        "somExpression" => Some(ResolvedProperty::SomExpression),
        _ => None,
    }
}

fn strip_xfa_form_prefix(
    segments: &[xfa_dom_resolver::som::SomSegment],
) -> &[xfa_dom_resolver::som::SomSegment] {
    match segments.first() {
        Some(segment)
            if matches!(&segment.selector, SomSelector::Name(name) if name == "form")
                && matches!(segment.index, SomIndex::None) =>
        {
            &segments[1..]
        }
        _ => segments,
    }
}

fn apply_index(matches: Vec<FormNodeId>, index: SomIndex) -> Vec<FormNodeId> {
    match index {
        SomIndex::None => matches.into_iter().take(1).collect(),
        SomIndex::Specific(idx) => matches.get(idx).copied().into_iter().collect(),
        SomIndex::All => matches,
    }
}

fn apply_index_to_single(node_id: FormNodeId, index: SomIndex) -> Vec<FormNodeId> {
    match index {
        SomIndex::None | SomIndex::Specific(0) | SomIndex::All => vec![node_id],
        SomIndex::Specific(_) => Vec::new(),
    }
}

fn read_formcalc_value(
    form: &FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    target: ResolvedTarget,
) -> FormCalcValue {
    match target.property {
        ResolvedProperty::RawValue => get_formcalc_raw_value(form, target.node_id),
        ResolvedProperty::Presence => FormCalcValue::String(
            match form.meta(target.node_id).presence {
                Presence::Visible => "visible",
                Presence::Hidden => "hidden",
                Presence::Invisible => "invisible",
                Presence::Inactive => "inactive",
            }
            .to_string(),
        ),
        ResolvedProperty::SomExpression => {
            FormCalcValue::String(build_som_expression(form, root_id, parents, target.node_id))
        }
    }
}

fn get_formcalc_raw_value(form: &FormTree, node_id: FormNodeId) -> FormCalcValue {
    match &form.get(node_id).node_type {
        FormNodeType::Field { value } => string_to_formcalc_value(value),
        _ if form.meta(node_id).group_kind == GroupKind::ExclusiveChoice => {
            for &child_id in &form.get(node_id).children {
                if let FormNodeType::Field { value } = &form.get(child_id).node_type {
                    if !value.is_empty() {
                        let selected = form.meta(child_id).item_value.as_deref().unwrap_or(value);
                        return string_to_formcalc_value(selected);
                    }
                }
            }
            FormCalcValue::Null
        }
        _ => FormCalcValue::Null,
    }
}

fn write_formcalc_value(
    form: &mut FormTree,
    node_id: FormNodeId,
    property: ResolvedProperty,
    value: FormCalcValue,
) -> usize {
    match property {
        ResolvedProperty::RawValue => set_raw_value(form, node_id, formcalc_to_script_value(value)),
        ResolvedProperty::Presence => {
            set_presence(form, node_id, ScriptValue::String(value.to_string_val()))
        }
        ResolvedProperty::SomExpression => 0,
    }
}

fn string_to_formcalc_value(value: &str) -> FormCalcValue {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        FormCalcValue::Null
    } else if let Ok(number) = trimmed.parse::<f64>() {
        FormCalcValue::Number(number)
    } else {
        FormCalcValue::String(value.to_string())
    }
}

fn formcalc_to_script_value(value: FormCalcValue) -> ScriptValue {
    match value {
        FormCalcValue::Null => ScriptValue::Null,
        FormCalcValue::Number(number) => ScriptValue::String(normalize_number(number)),
        FormCalcValue::String(value) => ScriptValue::String(value),
    }
}

fn build_som_expression(
    form: &FormTree,
    root_id: FormNodeId,
    parents: &HashMap<FormNodeId, FormNodeId>,
    node_id: FormNodeId,
) -> String {
    let mut parts = Vec::new();
    let mut cursor = Some(node_id);
    while let Some(current) = cursor {
        let node = form.get(current);
        if !node.name.is_empty() {
            let index = if let Some(parent_id) = parents.get(&current).copied() {
                form.get(parent_id)
                    .children
                    .iter()
                    .copied()
                    .filter(|sibling_id| form.get(*sibling_id).name == node.name)
                    .position(|sibling_id| sibling_id == current)
                    .unwrap_or(0)
            } else {
                0
            };
            parts.push(format!("{}[{index}]", node.name));
        }
        if current == root_id {
            break;
        }
        cursor = parents.get(&current).copied();
    }
    parts.reverse();

    if parts.is_empty() {
        "$form".to_string()
    } else {
        format!("$form.{}", parts.join("."))
    }
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

fn execute_assignment_legacy(
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
        let Some(node_id) =
            resolve_reference_legacy(form, root_id, parents, current_id, target.trim())
        else {
            return 0;
        };
        let value = eval_value_legacy(form, root_id, parents, current_id, rhs);
        return set_raw_value(form, node_id, value);
    }

    if let Some(target) = lhs.strip_suffix(".presence") {
        let target_trimmed = target.trim();
        let resolved = resolve_reference_legacy(form, root_id, parents, current_id, target_trimmed);
        let Some(node_id) = resolved else {
            return 0;
        };
        let value = eval_value_legacy(form, root_id, parents, current_id, rhs);
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

fn eval_condition_legacy(
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
            .any(|part| eval_condition_legacy(form, root_id, parents, current_id, part));
    }

    let and_parts = split_top_level(expr, "&&");
    if and_parts.len() > 1 {
        return and_parts
            .iter()
            .all(|part| eval_condition_legacy(form, root_id, parents, current_id, part));
    }

    if let Some((lhs, rhs, negated)) = split_comparison(expr) {
        let left = eval_value_legacy(form, root_id, parents, current_id, lhs);
        let right = eval_value_legacy(form, root_id, parents, current_id, rhs);
        let equals = values_equal(&left, &right);
        return if negated { !equals } else { equals };
    }

    match eval_value_legacy(form, root_id, parents, current_id, expr) {
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

fn eval_value_legacy(
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
        let Some(node_id) =
            resolve_reference_legacy(form, root_id, parents, current_id, target.trim())
        else {
            return ScriptValue::Null;
        };
        return get_raw_value(form, node_id);
    }

    if let Some(target) = expr.strip_suffix(".presence") {
        let Some(node_id) =
            resolve_reference_legacy(form, root_id, parents, current_id, target.trim())
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

fn resolve_reference_legacy(
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

    fn formcalc_script(script: &str, activity: &str) -> EventScript {
        EventScript::formcalc(script, Some(activity))
    }

    fn javascript_script(script: &str, activity: &str) -> EventScript {
        EventScript::javascript(script, Some(activity))
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
        tree.meta_mut(group).event_scripts = vec![formcalc_script(
            r#"
Details.presence = "hidden"
if (this.rawValue == 1) then
  Details.presence = "visible"
endif
"#,
            "initialize",
        )];
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
        tree.meta_mut(details).event_scripts = vec![formcalc_script(
            r#"
this.presence = "hidden"
if ((Opt1.rawValue == 1) or (Opt2.rawValue == 1)) then
  this.presence = "visible"
endif
"#,
            "calculate",
        )];

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

        tree.meta_mut(controller).event_scripts = vec![formcalc_script(
            r#"
if (this.rawValue == 1) then
  Target.rawValue = 1
endif
"#,
            "calculate",
        )];
        tree.meta_mut(details).presence = Presence::Hidden;
        tree.meta_mut(details).event_scripts = vec![formcalc_script(
            r#"
this.presence = "hidden"
if (Target.rawValue == 1) then
  this.presence = "visible"
endif
"#,
            "calculate",
        )];

        apply_dynamic_scripts(&mut tree, root);

        if let FormNodeType::Field { value } = &tree.get(target).node_type {
            assert_eq!(value, "1");
        } else {
            panic!("expected field");
        }
        assert_eq!(tree.meta(details).presence, Presence::Visible);
    }

    // ─── #1097: FormCalc SOM bridge hardening ────────────────────────────────

    /// SOM path `form1.#subform[0].field1.rawValue` resolves correctly on a
    /// simple form tree.
    #[test]
    fn som_path_resolves_on_simple_form_tree() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let form1 = add_node(&mut tree, "form1", FormNodeType::Subform);
        let subform = add_node(&mut tree, "subform1", FormNodeType::Subform);
        let field1 = add_node(
            &mut tree,
            "field1",
            FormNodeType::Field {
                value: "hello".to_string(),
            },
        );

        tree.get_mut(root).children = vec![form1];
        tree.get_mut(form1).children = vec![subform];
        tree.get_mut(subform).children = vec![field1];

        // Use a calculate script to read the value via absolute SOM path
        tree.meta_mut(root).event_scripts =
            vec![formcalc_script("form1.subform1.field1.rawValue", "calculate")];

        let parents = super::build_parent_map(&tree, root);
        let resolver = FormTreeSomResolver::new(&mut tree, root, &parents, root);
        let target = resolver.resolve_target("form1.subform1.field1.rawValue");
        assert!(target.is_some(), "SOM path must resolve to a node");
        let target = target.unwrap();
        let val = super::read_formcalc_value(&tree, root, &parents, target);
        match val {
            formcalc_interpreter::value::Value::String(s) => assert_eq!(s, "hello"),
            formcalc_interpreter::value::Value::Number(n) => {
                // number coercion: not expected here
                panic!("expected string, got number {n}")
            }
            _ => panic!("expected string value"),
        }
    }

    /// An invalid SOM path returns `None` (descriptive non-panic failure).
    #[test]
    fn invalid_som_path_returns_none_not_panic() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let parents = super::build_parent_map(&tree, root);
        let resolver = FormTreeSomResolver::new(&mut tree, root, &parents, root);

        // This should not panic — it should return None
        let result = resolver.resolve_target("nonexistent.deep.path.rawValue");
        assert!(
            result.is_none(),
            "invalid SOM path must return None, not panic"
        );
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
        tree.meta_mut(reset).event_scripts = vec![javascript_script(
            r#"xfa.resolveNode("formulier1.ADMIN.LockForm_AD").rawValue = 0;"#,
            "initialize",
        )];

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
        tree.meta_mut(empty).event_scripts =
            vec![javascript_script("Utils.hideIfEmpty(this);", "initialize")];

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
        tree.meta_mut(empty).event_scripts = vec![javascript_script(
            "Utils.deleteContainerIfEmpty(this);",
            "initialize",
        )];

        apply_dynamic_scripts(&mut tree, root);

        assert!(tree.meta(container).presence.is_not_visible());
    }

    #[test]
    fn default_meta_helper_is_constructible() {
        let meta = empty_meta();
        assert_eq!(meta.group_kind, GroupKind::None);
    }

    #[test]
    fn calculate_event_applies_formcalc_return_value() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let total = add_node(
            &mut tree,
            "Total",
            FormNodeType::Field {
                value: String::new(),
            },
        );

        tree.get_mut(root).children = vec![total];
        tree.meta_mut(total).event_scripts = vec![formcalc_script("40 + 2", "calculate")];

        apply_dynamic_scripts(&mut tree, root);

        match &tree.get(total).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "42"),
            _ => panic!("expected field"),
        }
    }

    #[test]
    fn calculate_event_resolves_bare_field_names_as_raw_values() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let section = add_node(&mut tree, "Section", FormNodeType::Subform);
        let number1 = add_node(
            &mut tree,
            "Number1",
            FormNodeType::Field {
                value: "40".to_string(),
            },
        );
        let number2 = add_node(
            &mut tree,
            "Number2",
            FormNodeType::Field {
                value: "2".to_string(),
            },
        );
        let total = add_node(
            &mut tree,
            "Total",
            FormNodeType::Field {
                value: String::new(),
            },
        );

        tree.get_mut(root).children = vec![section];
        tree.get_mut(section).children = vec![number1, number2, total];
        tree.meta_mut(total).event_scripts =
            vec![formcalc_script("Number1 + Number2", "calculate")];

        apply_dynamic_scripts(&mut tree, root);

        match &tree.get(total).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "42"),
            _ => panic!("expected field"),
        }
    }

    #[test]
    fn click_events_are_skipped_during_flatten() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let trigger = add_node(
            &mut tree,
            "Trigger",
            FormNodeType::Field {
                value: "1".to_string(),
            },
        );
        let details = add_node(&mut tree, "Details", FormNodeType::Subform);

        tree.get_mut(root).children = vec![trigger, details];
        tree.meta_mut(details).presence = Presence::Hidden;
        tree.meta_mut(trigger).event_scripts = vec![formcalc_script(
            r#"
Details.presence = "visible"
"#,
            "click",
        )];

        apply_dynamic_scripts(&mut tree, root);

        assert_eq!(tree.meta(details).presence, Presence::Hidden);
    }

    #[test]
    fn rollback_when_scripts_mostly_error() {
        // Set up a form with fields that have values.  Attach scripts that
        // will fail to parse so that errors > successes.  After
        // apply_dynamic_scripts the field values must be unchanged.
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let field_a = add_node(
            &mut tree,
            "FieldA",
            FormNodeType::Field {
                value: "hello".to_string(),
            },
        );
        let field_b = add_node(
            &mut tree,
            "FieldB",
            FormNodeType::Field {
                value: "world".to_string(),
            },
        );

        tree.get_mut(root).children = vec![field_a, field_b];

        // Two scripts that fail parsing (invalid FormCalc), zero successes.
        tree.meta_mut(field_a).event_scripts = vec![formcalc_script("@@INVALID@@", "initialize")];
        tree.meta_mut(field_b).event_scripts =
            vec![formcalc_script("@@ALSO_BROKEN@@", "initialize")];

        apply_dynamic_scripts(&mut tree, root);

        // Fields should retain their original values (rollback).
        match &tree.get(field_a).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "hello"),
            _ => panic!("expected field"),
        }
        match &tree.get(field_b).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "world"),
            _ => panic!("expected field"),
        }
    }

    #[test]
    fn rollback_when_populated_fields_go_empty() {
        // Calculate scripts returning Null clear field values. When >50%
        // of populated fields go empty, the rollback heuristic fires.
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let field_a = add_node(
            &mut tree,
            "FieldA",
            FormNodeType::Field {
                value: "keep".to_string(),
            },
        );
        let field_b = add_node(
            &mut tree,
            "FieldB",
            FormNodeType::Field {
                value: "also_keep".to_string(),
            },
        );

        tree.get_mut(root).children = vec![field_a, field_b];

        // Calculate scripts whose return value (Null) is written to the field,
        // blanking it.  The expression `Null()` is not a real FormCalc builtin,
        // but `0` would set the field to "0" (not empty).  Instead we use the
        // snapshot/rollback logic directly.
        // We test the heuristic by manually setting up the condition.
        let snapshot = super::snapshot_form(&tree);

        // Simulate scripts clearing both fields.
        if let FormNodeType::Field { value } = &mut tree.get_mut(field_a).node_type {
            *value = String::new();
        }
        if let FormNodeType::Field { value } = &mut tree.get_mut(field_b).node_type {
            *value = String::new();
        }

        assert!(super::should_rollback(&tree, &snapshot, 0, 2));

        super::restore_snapshot(&mut tree, &snapshot);

        match &tree.get(field_a).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "keep"),
            _ => panic!("expected field"),
        }
        match &tree.get(field_b).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "also_keep"),
            _ => panic!("expected field"),
        }
    }

    #[test]
    fn no_rollback_when_scripts_succeed() {
        // A single working script with no errors → no rollback, change persists.
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let total = add_node(
            &mut tree,
            "Total",
            FormNodeType::Field {
                value: String::new(),
            },
        );

        tree.get_mut(root).children = vec![total];
        tree.meta_mut(total).event_scripts = vec![formcalc_script("40 + 2", "calculate")];

        apply_dynamic_scripts(&mut tree, root);

        match &tree.get(total).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "42"),
            _ => panic!("expected field"),
        }
    }
}
