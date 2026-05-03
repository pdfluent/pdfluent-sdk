//! Phase C host bindings for the sandboxed JavaScript runtime.
//!
//! The adapter exposes a narrow, in-process view of the merged Form DOM:
//! bounded SOM resolution plus `field.rawValue` reads/writes. It never changes
//! tree structure or layout metadata.

use std::collections::HashMap;

use xfa_dom_resolver::som::{parse_som, SomExpression, SomIndex, SomRoot, SomSelector};
use xfa_layout_engine::form::{FormNodeId, FormNodeType, FormTree, GroupKind};

use super::RuntimeMetadata;

/// Maximum successful `rawValue` writes recorded for one document.
pub const MAX_MUTATIONS_PER_DOC: usize = 4096;
/// Maximum SOM resolution calls a single script may perform.
pub const MAX_RESOLVE_CALLS_PER_SCRIPT: u32 = 1024;
/// Maximum handles returned from one `xfa.resolveNodes` call.
pub const MAX_RESOLVE_RESULTS: usize = 256;
/// Maximum SOM segment depth accepted by the sandbox binding.
pub const MAX_SOM_DEPTH: usize = 16;

/// One successful `field.rawValue` write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationLogEntry {
    /// Mutated form node.
    pub node_id: FormNodeId,
    /// Zero-based script invocation index within the current document.
    pub script_idx: usize,
    /// Value before the write.
    pub before: String,
    /// Value after the write.
    pub after: String,
}

/// Host-side state shared by QuickJS binding closures.
#[derive(Debug)]
pub struct HostBindings {
    form: *mut FormTree,
    root_id: FormNodeId,
    current_id: Option<FormNodeId>,
    current_activity: Option<String>,
    current_script_idx: usize,
    next_script_idx: usize,
    generation: u64,
    mutation_log: Vec<MutationLogEntry>,
    resolve_count_this_script: u32,
    metadata: RuntimeMetadata,
    static_page_count: u32,
}

impl Default for HostBindings {
    fn default() -> Self {
        Self {
            form: std::ptr::null_mut(),
            root_id: FormNodeId(0),
            current_id: None,
            current_activity: None,
            current_script_idx: 0,
            next_script_idx: 0,
            generation: 0,
            mutation_log: Vec::new(),
            resolve_count_this_script: 0,
            metadata: RuntimeMetadata::default(),
            static_page_count: 0,
        }
    }
}

impl HostBindings {
    /// Create empty host-binding state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Install or clear the form pointer used by host bindings.
    pub fn set_form_handle(&mut self, form: *mut FormTree, root_id: FormNodeId) {
        self.form = form;
        self.root_id = root_id;
        if form.is_null() {
            self.current_id = None;
            self.current_activity = None;
            self.current_script_idx = 0;
        }
    }

    /// Reset counters and invalidate all existing handles for a new document.
    pub fn reset_per_document(&mut self) {
        self.form = std::ptr::null_mut();
        self.root_id = FormNodeId(0);
        self.current_id = None;
        self.current_activity = None;
        self.current_script_idx = 0;
        self.next_script_idx = 0;
        self.generation = self.generation.wrapping_add(1);
        self.mutation_log.clear();
        self.resolve_count_this_script = 0;
        self.metadata = RuntimeMetadata::default();
        self.static_page_count = 0;
    }

    /// Reset per-script state and install the current event context.
    pub fn reset_per_script(&mut self, current_id: FormNodeId, activity: Option<&str>) {
        self.current_id = Some(current_id);
        self.current_activity = activity.map(str::to_string);
        self.current_script_idx = self.next_script_idx;
        self.next_script_idx = self.next_script_idx.saturating_add(1);
        self.resolve_count_this_script = 0;
    }

    /// Cache the page count visible to read-only page-count bindings.
    pub fn set_static_page_count(&mut self, page_count: u32) {
        self.static_page_count = page_count;
    }

    /// Current handle generation. Handles capture this and are invalid after a
    /// document reset.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Current script node for `this`.
    pub fn current_node(&self) -> Option<FormNodeId> {
        self.current_id
    }

    /// Read and clear host metadata counters.
    pub fn take_metadata(&mut self) -> RuntimeMetadata {
        std::mem::take(&mut self.metadata)
    }

    /// Mutation log for tests and debug reporting.
    pub fn mutation_log(&self) -> &[MutationLogEntry] {
        &self.mutation_log
    }

    /// Read `field.rawValue`; returns `None` for stale handles, missing nodes,
    /// and non-field nodes.
    pub fn get_raw_value(&mut self, node_id: FormNodeId, generation: u64) -> Option<String> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if !self.handle_is_live(node_id, generation) {
            return None;
        }
        let form = self.form_ref()?;
        match &form.get(node_id).node_type {
            FormNodeType::Field { value } => Some(value.clone()),
            _ => None,
        }
    }

    /// Write `field.rawValue` when the activity and target are permitted.
    pub fn set_raw_value(&mut self, node_id: FormNodeId, value: String, generation: u64) -> bool {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if !self.write_activity_allowed()
            || !self.handle_is_live(node_id, generation)
            || self.mutation_log.len() >= MAX_MUTATIONS_PER_DOC
        {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return false;
        }

        let Some((before, after)) = self.write_field_value(node_id, value) else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return false;
        };

        self.mutation_log.push(MutationLogEntry {
            node_id,
            script_idx: self.current_script_idx,
            before,
            after,
        });
        self.metadata.mutations = self.metadata.mutations.saturating_add(1);
        true
    }

    /// Resolve a SOM path to the first field node.
    pub fn resolve_node(&mut self, path: &str) -> Option<FormNodeId> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        let nodes = match self.resolve_path(path) {
            ResolveOutcome::Ok(nodes) => nodes,
            ResolveOutcome::NoMatch => {
                self.metadata.resolve_failures = self.metadata.resolve_failures.saturating_add(1);
                return None;
            }
            ResolveOutcome::BindingError => {
                self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
                return None;
            }
        };

        let Some(form) = self.form_ref() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return None;
        };
        let found = nodes
            .into_iter()
            .find(|node_id| matches!(form.get(*node_id).node_type, FormNodeType::Field { .. }));
        if found.is_none() {
            self.metadata.resolve_failures = self.metadata.resolve_failures.saturating_add(1);
        }
        found
    }

    /// Resolve a SOM path to field handles, capped at
    /// [`MAX_RESOLVE_RESULTS`].
    pub fn resolve_nodes(&mut self, path: &str) -> Vec<FormNodeId> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        let nodes = match self.resolve_path(path) {
            ResolveOutcome::Ok(nodes) => nodes,
            ResolveOutcome::NoMatch => return Vec::new(),
            ResolveOutcome::BindingError => {
                self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
                self.metadata.resolve_failures = self.metadata.resolve_failures.saturating_add(1);
                return Vec::new();
            }
        };

        let Some(form) = self.form_ref() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Vec::new();
        };
        nodes
            .into_iter()
            .filter(|node_id| matches!(form.get(*node_id).node_type, FormNodeType::Field { .. }))
            .take(MAX_RESOLVE_RESULTS)
            .collect()
    }

    /// Read-only static page count visible to Phase C scripts.
    pub fn num_pages(&mut self) -> u32 {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        self.static_page_count
    }

    /// Record a binding-level failure for explicit no-op stubs.
    pub fn metadata_binding_error(&mut self) {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
    }

    fn resolve_path(&mut self, path: &str) -> ResolveOutcome {
        if self.resolve_count_this_script >= MAX_RESOLVE_CALLS_PER_SCRIPT {
            return ResolveOutcome::BindingError;
        }
        self.resolve_count_this_script = self.resolve_count_this_script.saturating_add(1);

        let Some(form) = self.form_ref() else {
            return ResolveOutcome::BindingError;
        };
        let Some(current_id) = self.current_id else {
            return ResolveOutcome::BindingError;
        };
        if current_id.0 >= form.nodes.len() || self.root_id.0 >= form.nodes.len() {
            return ResolveOutcome::BindingError;
        }

        let normalized = normalize_resolve_path(path.trim());
        let mut expr = match parse_som(&normalized) {
            Ok(expr) => expr,
            Err(_) => return ResolveOutcome::BindingError,
        };
        if expr.segments.len() > MAX_SOM_DEPTH {
            return ResolveOutcome::BindingError;
        }
        if matches!(
            expr.segments.last().map(|segment| &segment.selector),
            Some(SomSelector::Name(name)) if name == "rawValue"
        ) {
            expr.segments.pop();
        }

        let parents = build_parent_map(form, self.root_id);
        let resolver = HostSomResolver {
            form,
            root_id: self.root_id,
            parents: &parents,
            current_id,
        };
        match resolver.resolve_expression(&expr) {
            Some(nodes) if !nodes.is_empty() => ResolveOutcome::Ok(nodes),
            _ => ResolveOutcome::NoMatch,
        }
    }

    fn write_activity_allowed(&self) -> bool {
        matches!(
            self.current_activity.as_deref(),
            Some("initialize") | Some("calculate")
        )
    }

    fn handle_is_live(&self, node_id: FormNodeId, generation: u64) -> bool {
        generation == self.generation
            && self
                .form_ref()
                .is_some_and(|form| node_id.0 < form.nodes.len())
    }

    fn write_field_value(
        &mut self,
        node_id: FormNodeId,
        value: String,
    ) -> Option<(String, String)> {
        let form = self.form_mut()?;
        let node = form.nodes.get_mut(node_id.0)?;
        let FormNodeType::Field { value: field_value } = &mut node.node_type else {
            return None;
        };
        let before = field_value.clone();
        *field_value = value;
        Some((before, field_value.clone()))
    }

    fn form_ref(&self) -> Option<&FormTree> {
        if self.form.is_null() {
            None
        } else {
            // SAFETY: `dynamic.rs` installs a pointer derived from its live
            // `&mut FormTree` before script dispatch and clears it before
            // returning. Host methods never store references derived from it.
            unsafe { self.form.as_ref() }
        }
    }

    fn form_mut(&mut self) -> Option<&mut FormTree> {
        if self.form.is_null() {
            None
        } else {
            // SAFETY: See `form_ref`; the dispatch path is the sole owner of
            // the mutable form borrow while QuickJS closures execute.
            unsafe { self.form.as_mut() }
        }
    }
}

enum ResolveOutcome {
    Ok(Vec<FormNodeId>),
    NoMatch,
    BindingError,
}

fn normalize_resolve_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("this.") {
        format!("$.{rest}")
    } else if path == "this" {
        "$".to_string()
    } else {
        path.to_string()
    }
}

struct HostSomResolver<'a> {
    form: &'a FormTree,
    root_id: FormNodeId,
    parents: &'a HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
}

impl HostSomResolver<'_> {
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
        if let SomSelector::Name(name) = &segment.selector {
            if name == ".." {
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
            "pageset" => matches!(self.form.get(node_id).node_type, FormNodeType::PageSet),
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

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_layout_engine::form::{FormNode, Occur};
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

    #[test]
    fn raw_value_get_set_and_generation_guard() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let field = add_node(
            &mut tree,
            "Field1",
            FormNodeType::Field {
                value: "old".to_string(),
            },
        );
        tree.get_mut(root).children = vec![field];

        let mut host = HostBindings::new();
        host.reset_per_document();
        host.set_form_handle(&mut tree as *mut FormTree, root);
        host.reset_per_script(field, Some("calculate"));
        let generation = host.generation();

        assert_eq!(
            host.get_raw_value(field, generation),
            Some("old".to_string())
        );
        assert!(host.set_raw_value(field, "new".to_string(), generation));
        assert_eq!(
            host.get_raw_value(field, generation),
            Some("new".to_string())
        );

        host.reset_per_document();
        assert_eq!(host.get_raw_value(field, generation), None);
    }

    #[test]
    fn resolve_node_rejects_over_depth() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let field = add_node(
            &mut tree,
            "Field1",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        tree.get_mut(root).children = vec![field];

        let mut host = HostBindings::new();
        host.reset_per_document();
        host.set_form_handle(&mut tree as *mut FormTree, root);
        host.reset_per_script(root, Some("calculate"));
        let long_path = (0..=MAX_SOM_DEPTH)
            .map(|idx| format!("n{idx}"))
            .collect::<Vec<_>>()
            .join(".");

        assert_eq!(host.resolve_node(&long_path), None);
        assert_eq!(host.take_metadata().binding_errors, 1);
    }
}
