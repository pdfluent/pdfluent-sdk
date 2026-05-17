//! Phase C host bindings for the sandboxed JavaScript runtime.
//!
//! The adapter exposes a narrow, in-process view of the merged Form DOM:
//! bounded SOM resolution plus `field.rawValue` reads/writes. It never changes
//! tree structure or layout metadata.

use std::collections::HashMap;

use xfa_dom_resolver::data_dom::{DataDom, DataNodeId};
use xfa_dom_resolver::som::{
    parse_som, resolve_data_path, SomExpression, SomIndex, SomRoot, SomSelector,
};
use xfa_layout_engine::form::{FormNodeId, FormNodeType, FormTree, GroupKind};

use super::RuntimeMetadata;

/// Maximum successful `rawValue` writes recorded for one document.
pub const MAX_MUTATIONS_PER_DOC: usize = 4096;
/// Maximum live instances allowed for one script-managed subform run.
pub const MAX_INSTANCES_PER_SUBFORM: u32 = 256;
/// Maximum items allowed in a single runtime-populated listbox.
pub const MAX_ITEMS_PER_LISTBOX: u32 = 4096;
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
    mutation_count_this_doc: usize,
    resolve_count_this_script: u32,
    metadata: RuntimeMetadata,
    static_page_count: u32,
    zero_instance_runs: HashMap<(FormNodeId, String), u64>,
    /// Phase D-γ: read-only pointer to the DataDom for the current document.
    /// Set from a stack reference in `flatten.rs` that outlives script execution.
    /// `None` when no data packet is present or the feature is inactive.
    data_dom: Option<*const DataDom>,
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
            mutation_count_this_doc: 0,
            resolve_count_this_script: 0,
            metadata: RuntimeMetadata::default(),
            static_page_count: 0,
            zero_instance_runs: HashMap::new(),
            data_dom: None,
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
            self.zero_instance_runs.clear();
        }
    }

    /// Reset counters and invalidate all existing handles for a new document.
    ///
    /// Note: `data_dom` is intentionally NOT cleared here. The caller sets it
    /// explicitly via `set_data_handle` before calling
    /// `apply_dynamic_scripts_with_runtime`, and `reset_for_new_document` (which
    /// calls this) runs inside the dispatch function — after `set_data_handle`.
    /// Clearing it here would wipe the pointer before any scripts execute.
    /// The caller is responsible for managing DataDom lifetime.
    pub fn reset_per_document(&mut self) {
        self.form = std::ptr::null_mut();
        self.root_id = FormNodeId(0);
        self.current_id = None;
        self.current_activity = None;
        self.current_script_idx = 0;
        self.next_script_idx = 0;
        self.generation = self.generation.wrapping_add(1);
        self.mutation_log.clear();
        self.mutation_count_this_doc = 0;
        self.resolve_count_this_script = 0;
        self.metadata = RuntimeMetadata::default();
        self.static_page_count = 0;
        self.zero_instance_runs.clear();
        // data_dom is NOT reset here — see doc comment above.
    }

    /// Phase D-γ: install the DataDom pointer for the current document.
    /// # Safety
    /// `dom` must outlive all script execution for this document.
    pub fn set_data_handle(&mut self, dom: *const DataDom) {
        self.data_dom = Some(dom);
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
        let Some(form) = self.form_ref() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return None;
        };
        match &form.get(node_id).node_type {
            FormNodeType::Field { value } => Some(value.clone()),
            _ => None,
        }
    }

    /// Return the `name` attribute of any live node. Used by D-ι.2 to expose
    /// `subformHandle.variables` as the subform's own variables namespace.
    pub fn node_name(&self, node_id: FormNodeId, generation: u64) -> Option<String> {
        if !self.handle_is_live(node_id, generation) {
            return None;
        }
        let form = self.form_ref()?;
        Some(form.get(node_id).name.clone())
    }

    /// Write `field.rawValue` when the activity and target are permitted.
    pub fn set_raw_value(&mut self, node_id: FormNodeId, value: String, generation: u64) -> bool {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if !self.write_activity_allowed()
            || !self.handle_is_live(node_id, generation)
            || self.mutation_count_this_doc >= MAX_MUTATIONS_PER_DOC
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
        self.mutation_count_this_doc = self.mutation_count_this_doc.saturating_add(1);
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

    /// Resolve an implicit JavaScript identifier from the current XFA scope.
    ///
    /// Adobe's XFA JavaScript environment makes sibling and ancestor-scoped
    /// SOM nodes visible as bare identifiers. This method searches from the
    /// supplied current node upward, returning the first descendant with the
    /// requested name at each scope.
    pub fn resolve_implicit(&mut self, current_id: FormNodeId, name: &str) -> Option<FormNodeId> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        match self.resolve_implicit_inner(current_id, name) {
            ResolveOutcome::Ok(nodes) => nodes.into_iter().next(),
            ResolveOutcome::NoMatch => {
                self.metadata.resolve_failures = self.metadata.resolve_failures.saturating_add(1);
                None
            }
            ResolveOutcome::BindingError => {
                self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
                None
            }
        }
    }

    /// Resolve all viable implicit JavaScript identifier candidates from the
    /// current XFA scope. The first candidate is identical to the result of
    /// `resolve_implicit`; later candidates preserve same-name alternatives
    /// so the JS proxy can filter them when a chained property supplies the
    /// next SOM segment.
    pub fn resolve_implicit_candidates(
        &mut self,
        current_id: FormNodeId,
        name: &str,
    ) -> Vec<FormNodeId> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        match self.resolve_implicit_inner(current_id, name) {
            ResolveOutcome::Ok(nodes) => nodes,
            ResolveOutcome::NoMatch => {
                self.metadata.resolve_failures = self.metadata.resolve_failures.saturating_add(1);
                Vec::new()
            }
            ResolveOutcome::BindingError => {
                self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
                Vec::new()
            }
        }
    }

    /// Resolve a direct child node for chained dotted JavaScript access.
    pub fn resolve_child(&mut self, parent_id: FormNodeId, name: &str) -> Option<FormNodeId> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        match self.resolve_child_inner(parent_id, name) {
            ResolveOutcome::Ok(nodes) => nodes.into_iter().next(),
            ResolveOutcome::NoMatch => {
                self.metadata.resolve_failures = self.metadata.resolve_failures.saturating_add(1);
                None
            }
            ResolveOutcome::BindingError => {
                self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
                None
            }
        }
    }

    /// Resolve chained child candidates from an ordered parent candidate set.
    ///
    /// Direct children are preferred. If none match, this uses the same
    /// bounded descendant heuristic as the implicit resolver inside each
    /// parent, which matches the historical permissiveness of XFA SOM dotted
    /// access without inventing handles when the form structure is absent.
    pub fn resolve_child_candidates(
        &mut self,
        parent_ids: &[FormNodeId],
        name: &str,
    ) -> Vec<FormNodeId> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        match self.resolve_child_candidates_inner(parent_ids, name) {
            ResolveOutcome::Ok(nodes) => nodes,
            ResolveOutcome::NoMatch => {
                self.metadata.resolve_failures = self.metadata.resolve_failures.saturating_add(1);
                Vec::new()
            }
            ResolveOutcome::BindingError => {
                self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
                Vec::new()
            }
        }
    }

    /// Resolve a property name from each candidate's own implicit scope.
    ///
    /// This is the last fallback used by JS-side candidate filtering, covering
    /// forms that author a later segment as an ancestor-scoped implicit name
    /// rather than as a direct child of the previous segment.
    pub fn resolve_scoped_candidates(
        &mut self,
        scope_ids: &[FormNodeId],
        name: &str,
    ) -> Vec<FormNodeId> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        match self.resolve_scoped_candidates_inner(scope_ids, name) {
            ResolveOutcome::Ok(nodes) => nodes,
            ResolveOutcome::NoMatch => {
                self.metadata.resolve_failures = self.metadata.resolve_failures.saturating_add(1);
                Vec::new()
            }
            ResolveOutcome::BindingError => {
                self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
                Vec::new()
            }
        }
    }

    /// Phase D-θ: implicit-scope resolution with single-segment lookahead.
    ///
    /// First runs the standard implicit walk. When `next_hint` is non-empty
    /// it filters those candidates to ones whose subtree contains the hint
    /// name; if at least one survives that becomes the result. If the
    /// nearest-scope candidate set is hint-empty, the search is **widened**:
    /// every ancestor scope is rescanned for same-name candidates and only
    /// those satisfying the hint are kept. This is what unlocks chains like
    /// `F.P1.X.rawValue` where the nearest `F` lacks a `P1.X` descendant but
    /// an `F` higher in the tree does. When even the widened search finds
    /// nothing, the un-hinted baseline is returned so single-token reads
    /// behave identically to [`resolve_implicit_candidates`].
    pub fn resolve_implicit_candidates_hinted(
        &mut self,
        current_id: FormNodeId,
        name: &str,
        next_hint: &str,
    ) -> Vec<FormNodeId> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        let name = name.trim();
        let hint = next_hint.trim();
        if name.is_empty() || !self.consume_resolve_call() {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Vec::new();
        }
        let Some(form) = self.form_ref() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Vec::new();
        };
        if current_id.0 >= form.nodes.len() || self.root_id.0 >= form.nodes.len() {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Vec::new();
        }
        let parents = build_parent_map(form, self.root_id);
        let baseline =
            match resolve_implicit_candidates_in_scope(form, &parents, current_id, name) {
                ResolveOutcome::Ok(nodes) => nodes,
                ResolveOutcome::NoMatch => {
                    self.metadata.resolve_failures =
                        self.metadata.resolve_failures.saturating_add(1);
                    return Vec::new();
                }
                ResolveOutcome::BindingError => {
                    self.metadata.binding_errors =
                        self.metadata.binding_errors.saturating_add(1);
                    return Vec::new();
                }
            };
        if hint.is_empty() {
            return baseline;
        }
        let baseline_hit: Vec<FormNodeId> = baseline
            .iter()
            .copied()
            .filter(|n| subtree_contains_name(form, *n, hint, MAX_SOM_DEPTH))
            .collect();
        if !baseline_hit.is_empty() {
            return baseline_hit;
        }
        // Widen: walk every ancestor scope, collect same-name candidates,
        // keep only those whose subtree contains the hint. Bounded by
        // MAX_SOM_DEPTH and MAX_RESOLVE_CANDIDATES.
        let mut widened: Vec<FormNodeId> = Vec::new();
        let mut scope = Some(current_id);
        let mut depth = 0usize;
        while let Some(scope_id) = scope {
            if depth > MAX_SOM_DEPTH || scope_id.0 >= form.nodes.len() {
                break;
            }
            let mut candidates =
                collect_named_descendant_candidates(form, scope_id, name, MAX_SOM_DEPTH);
            order_candidates(form, &mut candidates);
            for node_id in candidates {
                if !widened.contains(&node_id)
                    && subtree_contains_name(form, node_id, hint, MAX_SOM_DEPTH)
                {
                    widened.push(node_id);
                    if widened.len() >= MAX_RESOLVE_CANDIDATES {
                        return widened;
                    }
                }
            }
            scope = parents.get(&scope_id).copied();
            depth += 1;
        }
        if widened.is_empty() {
            baseline
        } else {
            widened
        }
    }

    /// Phase D-θ: child resolution with single-segment lookahead.
    ///
    /// Returns the same candidates as [`resolve_child_candidates`] but,
    /// when `next_hint` is non-empty, keeps only candidates whose subtree
    /// contains a node named `next_hint`. Falls back to the un-hinted list
    /// when the filter would empty the result.
    pub fn resolve_child_candidates_hinted(
        &mut self,
        parent_ids: &[FormNodeId],
        name: &str,
        next_hint: &str,
    ) -> Vec<FormNodeId> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        let candidates = match self.resolve_child_candidates_inner(parent_ids, name) {
            ResolveOutcome::Ok(nodes) => nodes,
            ResolveOutcome::NoMatch => {
                self.metadata.resolve_failures = self.metadata.resolve_failures.saturating_add(1);
                return Vec::new();
            }
            ResolveOutcome::BindingError => {
                self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
                return Vec::new();
            }
        };
        self.apply_lookahead_filter(candidates, next_hint)
    }

    fn apply_lookahead_filter(
        &self,
        candidates: Vec<FormNodeId>,
        next_hint: &str,
    ) -> Vec<FormNodeId> {
        let hint = next_hint.trim();
        if hint.is_empty() || candidates.len() <= 1 {
            return candidates;
        }
        let Some(form) = self.form_ref() else {
            return candidates;
        };
        let filtered: Vec<FormNodeId> = candidates
            .iter()
            .copied()
            .filter(|node_id| subtree_contains_name(form, *node_id, hint, MAX_SOM_DEPTH))
            .collect();
        if filtered.is_empty() {
            candidates
        } else {
            filtered
        }
    }

    /// Count live sibling instances with the same name as `parent_id`.
    pub fn instance_count(&mut self, parent_id: FormNodeId) -> u32 {
        self.instance_count_inner(parent_id, None)
    }

    /// Count live sibling instances for a JS handle with generation checking.
    pub fn instance_count_for_handle(&mut self, parent_id: FormNodeId, generation: u64) -> u32 {
        self.instance_count_inner(parent_id, Some(generation))
    }

    /// Return the zero-based sibling index among instances with the same name.
    pub fn instance_index(&mut self, node_id: FormNodeId) -> u32 {
        self.instance_index_inner(node_id, None)
    }

    /// Return the zero-based sibling index for a JS handle.
    pub fn instance_index_for_handle(&mut self, node_id: FormNodeId, generation: u64) -> u32 {
        self.instance_index_inner(node_id, Some(generation))
    }

    /// Whether `parent_id._name` refers to an instance run that was explicitly
    /// set to zero during this document. This lets the JS shorthand return a
    /// read-only empty manager only for a real prior instance run, while
    /// keeping unrelated private-looking `_id` properties hidden.
    pub fn has_zero_instance_run(
        &mut self,
        parent_id: FormNodeId,
        generation: u64,
        name: &str,
    ) -> bool {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        let name = name.trim();
        if name.is_empty() || !self.consume_resolve_call() {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return false;
        }
        let Some(form) = self.form_ref() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return false;
        };
        if generation != self.generation || parent_id.0 >= form.nodes.len() {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return false;
        }
        self.zero_instance_runs
            .contains_key(&(parent_id, name.to_string()))
    }

    /// Replace the live same-name sibling run with exactly `n` instances,
    /// clamped to the prototype's occur limits and the sandbox safety cap.
    #[allow(clippy::result_unit_err)]
    pub fn instance_set(&mut self, parent_id: FormNodeId, n: u32) -> Result<u32, ()> {
        self.instance_set_inner(parent_id, None, n)
    }

    /// Generation-checked variant used by the QuickJS bridge.
    #[allow(clippy::result_unit_err)]
    pub fn instance_set_for_handle(
        &mut self,
        parent_id: FormNodeId,
        generation: u64,
        n: u32,
    ) -> Result<u32, ()> {
        self.instance_set_inner(parent_id, Some(generation), n)
    }

    /// Append one cloned instance to the end of the live same-name sibling run.
    #[allow(clippy::result_unit_err)]
    pub fn instance_add(&mut self, parent_id: FormNodeId) -> Result<FormNodeId, ()> {
        self.instance_add_inner(parent_id, None)
    }

    /// Generation-checked variant used by the QuickJS bridge.
    #[allow(clippy::result_unit_err)]
    pub fn instance_add_for_handle(
        &mut self,
        parent_id: FormNodeId,
        generation: u64,
    ) -> Result<FormNodeId, ()> {
        self.instance_add_inner(parent_id, Some(generation))
    }

    /// Remove one live same-name sibling instance by zero-based index.
    #[allow(clippy::result_unit_err)]
    pub fn instance_remove(&mut self, parent_id: FormNodeId, index: u32) -> Result<(), ()> {
        self.instance_remove_inner(parent_id, None, index)
    }

    /// Generation-checked variant used by the QuickJS bridge.
    #[allow(clippy::result_unit_err)]
    pub fn instance_remove_for_handle(
        &mut self,
        parent_id: FormNodeId,
        generation: u64,
        index: u32,
    ) -> Result<(), ()> {
        self.instance_remove_inner(parent_id, Some(generation), index)
    }

    /// Clear all runtime-populated listbox items on a field.
    #[allow(clippy::result_unit_err)]
    pub fn list_clear(&mut self, field_id: FormNodeId) -> Result<(), ()> {
        self.list_clear_inner(field_id, None)
    }

    /// Generation-checked variant used by the QuickJS bridge.
    #[allow(clippy::result_unit_err)]
    pub fn list_clear_for_handle(
        &mut self,
        field_id: FormNodeId,
        generation: u64,
    ) -> Result<(), ()> {
        self.list_clear_inner(field_id, Some(generation))
    }

    /// Append one item to a field's runtime listbox options.
    #[allow(clippy::result_unit_err)]
    pub fn list_add(
        &mut self,
        field_id: FormNodeId,
        display: String,
        save: Option<String>,
    ) -> Result<(), ()> {
        self.list_add_inner(field_id, None, display, save)
    }

    /// Generation-checked variant used by the QuickJS bridge.
    #[allow(clippy::result_unit_err)]
    pub fn list_add_for_handle(
        &mut self,
        field_id: FormNodeId,
        generation: u64,
        display: String,
        save: Option<String>,
    ) -> Result<(), ()> {
        self.list_add_inner(field_id, Some(generation), display, save)
    }

    /// XFA 3.3 §App A `boundItem` — listbox display→save lookup.
    ///
    /// Returns the save value associated with `display_value` for a listbox
    /// or dropdown field. Lookup order:
    /// 1. Runtime listbox items populated via D-β `addItem` (matched first).
    /// 2. Static `<items>` parsed from the template at merge time.
    ///
    /// Adobe's documented behaviour returns the input unchanged when no
    /// match exists (passthrough). Empty input returns empty string. Stale
    /// or non-field handles return the input unchanged.
    pub fn bound_item_for_handle(
        &mut self,
        field_id: FormNodeId,
        generation: u64,
        display_value: String,
    ) -> String {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if !self.handle_is_live(field_id, generation) {
            return display_value;
        }
        let Some(form) = self.form_ref() else {
            return display_value;
        };
        if !matches!(form.get(field_id).node_type, FormNodeType::Field { .. }) {
            return display_value;
        }
        let meta = form.meta(field_id);
        for (display, save) in &meta.runtime_listbox_items {
            if display == &display_value {
                return save.clone();
            }
        }
        for (idx, display) in meta.display_items.iter().enumerate() {
            if display == &display_value {
                return meta
                    .save_items
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| display_value.clone());
            }
        }
        display_value
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

    /// Record use of an intentionally approximate read-only stub.
    pub fn metadata_resolve_failure(&mut self) {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        self.metadata.resolve_failures = self.metadata.resolve_failures.saturating_add(1);
    }

    fn consume_resolve_call(&mut self) -> bool {
        if self.resolve_count_this_script >= MAX_RESOLVE_CALLS_PER_SCRIPT {
            return false;
        }
        self.resolve_count_this_script = self.resolve_count_this_script.saturating_add(1);
        true
    }

    fn resolve_path(&mut self, path: &str) -> ResolveOutcome {
        if !self.consume_resolve_call() {
            return ResolveOutcome::BindingError;
        }

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

    fn resolve_implicit_inner(&mut self, current_id: FormNodeId, name: &str) -> ResolveOutcome {
        let name = name.trim();
        if name.is_empty() || !self.consume_resolve_call() {
            return ResolveOutcome::BindingError;
        }

        let Some(form) = self.form_ref() else {
            return ResolveOutcome::BindingError;
        };
        if current_id.0 >= form.nodes.len() || self.root_id.0 >= form.nodes.len() {
            return ResolveOutcome::BindingError;
        }

        let parents = build_parent_map(form, self.root_id);
        resolve_implicit_candidates_in_scope(form, &parents, current_id, name)
    }

    fn resolve_child_inner(&mut self, parent_id: FormNodeId, name: &str) -> ResolveOutcome {
        self.resolve_child_candidates_inner(&[parent_id], name)
    }

    fn resolve_child_candidates_inner(
        &mut self,
        parent_ids: &[FormNodeId],
        name: &str,
    ) -> ResolveOutcome {
        let name = name.trim();
        if name.is_empty() || !self.consume_resolve_call() {
            return ResolveOutcome::BindingError;
        }

        let Some(form) = self.form_ref() else {
            return ResolveOutcome::BindingError;
        };
        if parent_ids.is_empty()
            || parent_ids
                .iter()
                .any(|node_id| node_id.0 >= form.nodes.len())
        {
            return ResolveOutcome::BindingError;
        }

        let mut direct = Vec::new();
        for &parent_id in parent_ids {
            for &child_id in &form.get(parent_id).children {
                if form.get(child_id).name == name {
                    push_unique_candidate(&mut direct, child_id);
                    if direct.len() >= MAX_RESOLVE_CANDIDATES {
                        return ResolveOutcome::Ok(direct);
                    }
                }
            }
        }
        if !direct.is_empty() {
            return ResolveOutcome::Ok(direct);
        }

        let mut descendants = Vec::new();
        for &parent_id in parent_ids {
            let mut local =
                collect_named_descendant_candidates(form, parent_id, name, MAX_SOM_DEPTH);
            order_candidates(form, &mut local);
            for node_id in local {
                push_unique_candidate(&mut descendants, node_id);
                if descendants.len() >= MAX_RESOLVE_CANDIDATES {
                    return ResolveOutcome::Ok(descendants);
                }
            }
        }
        if descendants.is_empty() {
            ResolveOutcome::NoMatch
        } else {
            ResolveOutcome::Ok(descendants)
        }
    }

    fn resolve_scoped_candidates_inner(
        &mut self,
        scope_ids: &[FormNodeId],
        name: &str,
    ) -> ResolveOutcome {
        let name = name.trim();
        if name.is_empty() || !self.consume_resolve_call() {
            return ResolveOutcome::BindingError;
        }

        let Some(form) = self.form_ref() else {
            return ResolveOutcome::BindingError;
        };
        if scope_ids.is_empty()
            || self.root_id.0 >= form.nodes.len()
            || scope_ids
                .iter()
                .any(|node_id| node_id.0 >= form.nodes.len())
        {
            return ResolveOutcome::BindingError;
        }

        let parents = build_parent_map(form, self.root_id);
        let mut out = Vec::new();
        for &scope_id in scope_ids {
            if let ResolveOutcome::Ok(nodes) =
                resolve_implicit_candidates_in_scope(form, &parents, scope_id, name)
            {
                for node_id in nodes {
                    push_unique_candidate(&mut out, node_id);
                    if out.len() >= MAX_RESOLVE_CANDIDATES {
                        return ResolveOutcome::Ok(out);
                    }
                }
            }
        }
        if out.is_empty() {
            ResolveOutcome::NoMatch
        } else {
            ResolveOutcome::Ok(out)
        }
    }

    fn instance_count_inner(&mut self, parent_id: FormNodeId, generation: Option<u64>) -> u32 {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        let Some(run) = self.read_instance_run(parent_id, generation) else {
            return 0;
        };
        run.nodes.len() as u32
    }

    fn instance_index_inner(&mut self, node_id: FormNodeId, generation: Option<u64>) -> u32 {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        let Some(run) = self.read_instance_run(node_id, generation) else {
            return 0;
        };
        run.nodes
            .iter()
            .position(|candidate| *candidate == node_id)
            .unwrap_or(0) as u32
    }

    fn instance_set_inner(
        &mut self,
        parent_id: FormNodeId,
        generation: Option<u64>,
        n: u32,
    ) -> Result<u32, ()> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if !self.write_activity_allowed()
            || self.mutation_count_this_doc >= MAX_MUTATIONS_PER_DOC
            || !self.consume_resolve_call()
        {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }

        let Some(run) = self.live_instance_run(parent_id, generation) else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };
        let Some(target_count) = self.clamped_instance_count(run.prototype_id, n) else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };

        let target_count = target_count as usize;
        let prototype_id = run.prototype_id;
        let parent_id = run.parent_id;
        let first_pos = run.first_position;
        let Some(prototype_name) = self
            .form_ref()
            .and_then(|form| form.nodes.get(prototype_id.0))
            .map(|node| node.name.clone())
        else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };
        let remove_ids = run.nodes;

        let mut new_ids = Vec::with_capacity(target_count);
        if target_count > 0 {
            self.normalize_instance_occurrence(prototype_id);
            new_ids.push(prototype_id);
            for _ in 1..target_count {
                let cloned_id = self.clone_subtree(prototype_id)?;
                new_ids.push(cloned_id);
            }
        }

        let Some(form) = self.form_mut() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };
        let parent = form.get_mut(parent_id);
        parent
            .children
            .retain(|child_id| !remove_ids.contains(child_id));
        let insert_pos = first_pos.min(parent.children.len());
        for (offset, node_id) in new_ids.iter().copied().enumerate() {
            parent.children.insert(insert_pos + offset, node_id);
        }

        let key = (parent_id, prototype_name);
        if target_count == 0 {
            self.zero_instance_runs.insert(key, self.generation);
        } else {
            self.zero_instance_runs.remove(&key);
        }
        self.record_instance_write();
        Ok(target_count as u32)
    }

    fn instance_add_inner(
        &mut self,
        parent_id: FormNodeId,
        generation: Option<u64>,
    ) -> Result<FormNodeId, ()> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if !self.write_activity_allowed()
            || self.mutation_count_this_doc >= MAX_MUTATIONS_PER_DOC
            || !self.consume_resolve_call()
        {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }

        let Some(run) = self.live_instance_run(parent_id, generation) else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };
        let zero_key = self
            .form_ref()
            .and_then(|form| form.nodes.get(run.prototype_id.0))
            .map(|node| (run.parent_id, node.name.clone()));
        let Some(max_allowed) = self.max_instances_for(run.prototype_id) else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };
        if run.nodes.len() as u32 >= max_allowed {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }

        let cloned_id = self.clone_subtree(run.prototype_id)?;
        let Some(form) = self.form_mut() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };
        form.get_mut(run.parent_id)
            .children
            .insert(run.last_position + 1, cloned_id);

        if let Some(key) = zero_key {
            self.zero_instance_runs.remove(&key);
        }
        self.record_instance_write();
        Ok(cloned_id)
    }

    fn instance_remove_inner(
        &mut self,
        parent_id: FormNodeId,
        generation: Option<u64>,
        index: u32,
    ) -> Result<(), ()> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if !self.write_activity_allowed()
            || self.mutation_count_this_doc >= MAX_MUTATIONS_PER_DOC
            || !self.consume_resolve_call()
        {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }

        let Some(run) = self.live_instance_run(parent_id, generation) else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };
        let zero_key = self
            .form_ref()
            .and_then(|form| form.nodes.get(run.prototype_id.0))
            .map(|node| (run.parent_id, node.name.clone()));
        let min_allowed = self
            .form_ref()
            .and_then(|form| form.nodes.get(run.prototype_id.0))
            .map(|node| node.occur.min)
            .unwrap_or(1);
        if run.nodes.len() as u32 <= min_allowed {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }
        let Some(remove_position) = run.positions.get(index as usize).copied() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };

        let Some(form) = self.form_mut() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };
        form.get_mut(run.parent_id).children.remove(remove_position);

        if let Some(key) = zero_key {
            if run.nodes.len() == 1 {
                self.zero_instance_runs.insert(key, self.generation);
            } else {
                self.zero_instance_runs.remove(&key);
            }
        }
        self.record_instance_write();
        Ok(())
    }

    fn read_instance_run(
        &mut self,
        node_id: FormNodeId,
        generation: Option<u64>,
    ) -> Option<InstanceRun> {
        if !self.consume_resolve_call() {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return None;
        }
        let Some(form) = self.form_ref() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return None;
        };
        if generation.is_some_and(|value| value != self.generation) {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return None;
        }
        if node_id.0 >= form.nodes.len() || self.root_id.0 >= form.nodes.len() {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return None;
        }

        let parents = build_parent_map(form, self.root_id);
        if !parents.contains_key(&node_id) {
            if node_id == self.root_id {
                return Some(InstanceRun {
                    parent_id: node_id,
                    positions: vec![0],
                    nodes: vec![node_id],
                    prototype_id: node_id,
                    first_position: 0,
                    last_position: 0,
                });
            }
            return None;
        }

        build_instance_run(form, &parents, node_id)
    }

    fn live_instance_run(
        &self,
        node_id: FormNodeId,
        generation: Option<u64>,
    ) -> Option<InstanceRun> {
        let form = self.form_ref()?;
        if generation.is_some_and(|value| value != self.generation)
            || node_id.0 >= form.nodes.len()
            || self.root_id.0 >= form.nodes.len()
            || !is_instance_node(&form.get(node_id).node_type)
        {
            return None;
        }
        let parents = build_parent_map(form, self.root_id);
        build_instance_run(form, &parents, node_id)
    }

    fn clamped_instance_count(&self, prototype_id: FormNodeId, requested: u32) -> Option<u32> {
        let min_allowed = self.form_ref()?.get(prototype_id).occur.min;
        let max_allowed = self.max_instances_for(prototype_id)?;
        if min_allowed > max_allowed {
            return None;
        }
        Some(requested.clamp(min_allowed, max_allowed))
    }

    fn max_instances_for(&self, prototype_id: FormNodeId) -> Option<u32> {
        let occur = &self.form_ref()?.get(prototype_id).occur;
        let max = occur.max.unwrap_or(u32::MAX);
        Some(max.min(MAX_INSTANCES_PER_SUBFORM))
    }

    fn clone_subtree(&mut self, source_id: FormNodeId) -> Result<FormNodeId, ()> {
        let (mut new_node, mut new_meta, child_ids) = {
            let Some(form) = self.form_ref() else {
                self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
                return Err(());
            };
            if source_id.0 >= form.nodes.len() {
                self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
                return Err(());
            }
            (
                form.get(source_id).clone(),
                form.meta(source_id).clone(),
                form.get(source_id).children.clone(),
            )
        };

        let mut new_children = Vec::with_capacity(child_ids.len());
        for child_id in child_ids {
            new_children.push(self.clone_subtree(child_id)?);
        }
        new_node.children = new_children;
        // Runtime-created instances are represented as concrete siblings, so
        // each physical clone should lay out once while retaining min/max.
        new_node.occur.initial = 1;
        new_meta.xfa_id = None;

        let Some(form) = self.form_mut() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };
        Ok(form.add_node_with_meta(new_node, new_meta))
    }

    fn normalize_instance_occurrence(&mut self, node_id: FormNodeId) {
        if let Some(form) = self.form_mut() {
            if let Some(node) = form.nodes.get_mut(node_id.0) {
                // See clone_subtree: live instance count is encoded by
                // sibling multiplicity after an instanceManager write.
                node.occur.initial = 1;
            }
        }
    }

    fn record_instance_write(&mut self) {
        self.metadata.instance_writes = self.metadata.instance_writes.saturating_add(1);
        self.mutation_count_this_doc = self.mutation_count_this_doc.saturating_add(1);
    }

    fn record_list_write(&mut self) {
        self.metadata.list_writes = self.metadata.list_writes.saturating_add(1);
        self.mutation_count_this_doc = self.mutation_count_this_doc.saturating_add(1);
    }

    fn list_clear_inner(
        &mut self,
        field_id: FormNodeId,
        generation: Option<u64>,
    ) -> Result<(), ()> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if !self.write_activity_allowed()
            || self.mutation_count_this_doc >= MAX_MUTATIONS_PER_DOC
            || !self.consume_resolve_call()
        {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }
        let current_generation = self.generation;
        let Some(form) = self.form_mut() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };
        if generation.is_some_and(|value| value != current_generation)
            || field_id.0 >= form.nodes.len()
        {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }
        if !matches!(form.get(field_id).node_type, FormNodeType::Field { .. }) {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }
        form.meta_mut(field_id).runtime_listbox_items.clear();
        self.record_list_write();
        Ok(())
    }

    fn list_add_inner(
        &mut self,
        field_id: FormNodeId,
        generation: Option<u64>,
        display: String,
        save: Option<String>,
    ) -> Result<(), ()> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if !self.write_activity_allowed()
            || self.mutation_count_this_doc >= MAX_MUTATIONS_PER_DOC
            || !self.consume_resolve_call()
        {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }
        let current_generation = self.generation;
        let Some(form) = self.form_mut() else {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        };
        if generation.is_some_and(|value| value != current_generation)
            || field_id.0 >= form.nodes.len()
        {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }
        if !matches!(form.get(field_id).node_type, FormNodeType::Field { .. }) {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }
        let meta = form.meta_mut(field_id);
        if meta.runtime_listbox_items.len() >= MAX_ITEMS_PER_LISTBOX as usize {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Err(());
        }
        let save_value = save.unwrap_or_else(|| display.clone());
        meta.runtime_listbox_items.push((display, save_value));
        self.record_list_write();
        Ok(())
    }

    fn write_activity_allowed(&self) -> bool {
        matches!(
            self.current_activity.as_deref(),
            Some("initialize")
                | Some("calculate")
                | Some("validate")
                | Some("docReady")
                | Some("layoutReady")
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

    /// Phase D-γ: obtain a read-only reference to the DataDom.
    fn data_dom_ref(&self) -> Option<&DataDom> {
        // SAFETY: `data_dom` is set from `&data_dom` in flatten.rs where the
        // DataDom lives on the stack and outlives all script execution. We only
        // ever read through this pointer, never write.
        self.data_dom.map(|ptr| unsafe { &*ptr })
    }

    /// Phase D-γ: children of a DataDom node, returned as raw indices.
    /// Capped at `MAX_RESOLVE_RESULTS`. Returns empty vec on invalid input.
    pub fn data_children(&mut self, raw_id: usize) -> Vec<usize> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        // Scope the borrow so we can increment metadata after.
        let result = {
            let Some(dom) = self.data_dom_ref() else {
                return Vec::new();
            };
            let id = DataNodeId::from_raw(raw_id);
            if dom.get(id).is_none() {
                return Vec::new();
            }
            dom.children(id)
                .iter()
                .take(MAX_RESOLVE_RESULTS)
                .map(|c| c.as_raw())
                .collect::<Vec<_>>()
        };
        self.metadata.data_reads = self.metadata.data_reads.saturating_add(1);
        result
    }

    /// Phase D-γ: text value of a DataValue node.
    /// Returns `None` for DataGroup nodes or out-of-bounds indices.
    pub fn data_value(&mut self, raw_id: usize) -> Option<String> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        let result = {
            let dom = self.data_dom_ref()?;
            let id = DataNodeId::from_raw(raw_id);
            dom.get(id)?;
            dom.value(id).ok().map(|s| s.to_owned())
        };
        if result.is_some() {
            self.metadata.data_reads = self.metadata.data_reads.saturating_add(1);
        }
        result
    }

    /// Phase D-γ: first child of `parent_raw` whose name matches `name`.
    /// Returns `None` if not found or parent is out-of-bounds.
    pub fn data_child_by_name(&mut self, parent_raw: usize, name: &str) -> Option<usize> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        let result = {
            let dom = self.data_dom_ref()?;
            let parent = DataNodeId::from_raw(parent_raw);
            dom.get(parent)?;
            dom.children_by_name(parent, name)
                .into_iter()
                .next()
                .map(|id| id.as_raw())
        };
        if result.is_some() {
            self.metadata.data_reads = self.metadata.data_reads.saturating_add(1);
        }
        result
    }

    /// Phase D-γ: raw DataDom index bound to a FormTree node.
    /// Returns `None` when the node is unbound or the handle is stale.
    pub fn data_bound_record(
        &mut self,
        form_node_id: FormNodeId,
        generation: u64,
    ) -> Option<usize> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if generation != self.generation {
            return None;
        }
        let form = self.form_ref()?;
        if form_node_id.0 >= form.nodes.len() {
            return None;
        }
        form.meta(form_node_id).bound_data_node
    }

    /// Phase D-γ: resolve a data SOM path to the first matching node.
    /// Consumes one resolve-call budget slot. Returns `None` on budget
    /// exhaustion, parse failure, or no match.
    pub fn data_resolve_node(&mut self, path: &str) -> Option<usize> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if !self.consume_resolve_call() {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return None;
        }
        // Scope borrow so we can use self after.
        {
            let dom = self.data_dom_ref()?;
            resolve_data_path(dom, path, None)
                .ok()?
                .into_iter()
                .next()
                .map(|id| id.as_raw())
        }
    }

    /// Phase D-γ: resolve a data SOM path to all matching nodes (capped).
    /// Consumes one resolve-call budget slot.
    pub fn data_resolve_nodes(&mut self, path: &str) -> Vec<usize> {
        self.metadata.host_calls = self.metadata.host_calls.saturating_add(1);
        if !self.consume_resolve_call() {
            self.metadata.binding_errors = self.metadata.binding_errors.saturating_add(1);
            return Vec::new();
        }
        {
            let Some(dom) = self.data_dom_ref() else {
                return Vec::new();
            };
            resolve_data_path(dom, path, None)
                .unwrap_or_default()
                .into_iter()
                .take(MAX_RESOLVE_RESULTS)
                .map(|id| id.as_raw())
                .collect()
        }
    }
}

enum ResolveOutcome {
    Ok(Vec<FormNodeId>),
    NoMatch,
    BindingError,
}

#[derive(Debug, Clone)]
struct InstanceRun {
    parent_id: FormNodeId,
    positions: Vec<usize>,
    nodes: Vec<FormNodeId>,
    prototype_id: FormNodeId,
    first_position: usize,
    last_position: usize,
}

fn build_instance_run(
    form: &FormTree,
    parents: &HashMap<FormNodeId, FormNodeId>,
    node_id: FormNodeId,
) -> Option<InstanceRun> {
    let parent_id = parents.get(&node_id).copied()?;
    let name = form.get(node_id).name.clone();
    let parent = form.get(parent_id);
    let mut positions = Vec::new();
    let mut nodes = Vec::new();
    for (position, child_id) in parent.children.iter().copied().enumerate() {
        if form.get(child_id).name == name {
            positions.push(position);
            nodes.push(child_id);
        }
    }
    if !nodes.contains(&node_id) {
        return None;
    }
    Some(InstanceRun {
        parent_id,
        prototype_id: nodes[0],
        first_position: positions[0],
        last_position: *positions.last()?,
        positions,
        nodes,
    })
}

fn is_instance_node(node_type: &FormNodeType) -> bool {
    matches!(
        node_type,
        FormNodeType::Root
            | FormNodeType::Subform
            | FormNodeType::Area
            | FormNodeType::ExclGroup
            | FormNodeType::SubformSet
    )
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

/// Maximum same-name candidates collected during one bare-identifier
/// resolution. XFA forms commonly include multiple subforms with the same
/// `name` (e.g. layout-only stubs, signed-data placeholders, real content).
/// 32 is far beyond any real corpus we have observed.
const MAX_RESOLVE_CANDIDATES: usize = 32;

/// Locate a same-name descendant of `scope_id`, biased toward the candidate
/// most likely to be the script-meant target.
///
/// XFA Spec 3.3 §S15 implicit identifier resolution: when a script reads
/// `Foo.Bar.Baz`, the leading `Foo` walks the scope chain upward and runs
/// a depth-first search for a descendant named `Foo` at each scope. Real
/// XFA templates frequently contain multiple subforms named `Foo` —
/// typically one empty layout-stub plus one populated content node —
/// because authoring tools split layout and data definitions. A naive
/// first-hit DFS lands on whichever appears first in the merged form
/// tree, which is usually the empty stub.
///
/// We collect up to [`MAX_RESOLVE_CANDIDATES`] same-name descendants and
/// pick the one with the most direct children, breaking ties by encounter
/// order (DFS preorder). This keeps single-candidate behaviour identical
/// to the prior first-hit semantics while disambiguating the multi-stub
/// case to the populated branch.
///
/// Phase D-η: name-collision tie-break for the implicit resolver.
fn collect_named_descendant_candidates(
    form: &FormTree,
    scope_id: FormNodeId,
    name: &str,
    max_depth: usize,
) -> Vec<FormNodeId> {
    let mut candidates: Vec<FormNodeId> = Vec::new();
    find_named_descendant_inner(form, scope_id, name, 0, max_depth, &mut candidates);
    candidates
}

fn order_candidates(form: &FormTree, candidates: &mut [FormNodeId]) {
    // Phase D-η refinement (Codex review feedback): when same-name
    // candidates include a `Field` and a sibling subform/container,
    // prefer the Field. Fields have zero children, so naïve children-
    // bias would pick the container — but bare `Amount.rawValue` reads
    // and writes mean the field, not the container. Picking the
    // container would silently return null for reads and refuse writes
    // via `set_raw_value`. Preserve first-DFS-hit semantics among
    // Field candidates so the pre-D-η field-resolution behaviour is
    // unchanged when a field candidate exists at all.
    candidates.sort_by(|left, right| {
        let left_is_field = matches!(form.get(*left).node_type, FormNodeType::Field { .. });
        let right_is_field = matches!(form.get(*right).node_type, FormNodeType::Field { .. });
        match (left_is_field, right_is_field) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (true, true) => std::cmp::Ordering::Equal,
            (false, false) => form
                .get(*right)
                .children
                .len()
                .cmp(&form.get(*left).children.len()),
        }
    });
}

fn push_unique_candidate(candidates: &mut Vec<FormNodeId>, node_id: FormNodeId) {
    if candidates.len() < MAX_RESOLVE_CANDIDATES && !candidates.contains(&node_id) {
        candidates.push(node_id);
    }
}

fn resolve_implicit_candidates_in_scope(
    form: &FormTree,
    parents: &HashMap<FormNodeId, FormNodeId>,
    current_id: FormNodeId,
    name: &str,
) -> ResolveOutcome {
    // Phase D-η resolution. The implicit-identifier scope walk has three
    // passes; the first hit wins. Phase D-κ keeps that first candidate
    // byte-for-byte compatible for one-token reads, but returns same-scope
    // alternatives after it so the JS proxy can filter chained SOM access.
    let mut scope = Some(current_id);
    let mut depth = 0usize;
    while let Some(scope_id) = scope {
        if depth > MAX_SOM_DEPTH {
            return ResolveOutcome::BindingError;
        }
        if scope_id.0 >= form.nodes.len() {
            return ResolveOutcome::BindingError;
        }
        let mut candidates =
            collect_named_descendant_candidates(form, scope_id, name, MAX_SOM_DEPTH);
        if !candidates.is_empty() {
            order_candidates(form, &mut candidates);
            if !form.get(candidates[0]).children.is_empty() {
                return ResolveOutcome::Ok(candidates);
            }
        }
        scope = parents.get(&scope_id).copied();
        depth += 1;
    }

    // Pass 2: ancestor self-name walk. Only reached when no scope's
    // children-biased descendant DFS produced a populated node.
    let mut scope = Some(current_id);
    let mut depth = 0usize;
    while let Some(scope_id) = scope {
        if depth > MAX_SOM_DEPTH {
            return ResolveOutcome::BindingError;
        }
        if scope_id.0 >= form.nodes.len() {
            return ResolveOutcome::BindingError;
        }
        if form.get(scope_id).name == name {
            return ResolveOutcome::Ok(vec![scope_id]);
        }
        scope = parents.get(&scope_id).copied();
        depth += 1;
    }

    // Pass 3: accept a stub descendant as last resort.
    let mut scope = Some(current_id);
    let mut depth = 0usize;
    while let Some(scope_id) = scope {
        if depth > MAX_SOM_DEPTH {
            return ResolveOutcome::BindingError;
        }
        if scope_id.0 >= form.nodes.len() {
            return ResolveOutcome::BindingError;
        }
        let mut candidates =
            collect_named_descendant_candidates(form, scope_id, name, MAX_SOM_DEPTH);
        if !candidates.is_empty() {
            order_candidates(form, &mut candidates);
            return ResolveOutcome::Ok(candidates);
        }
        scope = parents.get(&scope_id).copied();
        depth += 1;
    }

    ResolveOutcome::NoMatch
}

/// Phase D-θ: bounded subtree check used by lookahead disambiguation.
///
/// Returns true when `node_id`'s subtree contains a descendant named `name`
/// (or a direct child) within `MAX_SOM_DEPTH` levels. The starting node
/// itself is excluded — only descendants count. Recursion is bounded by
/// [`MAX_SOM_DEPTH`] so malformed templates cannot starve the sandbox.
fn subtree_contains_name(
    form: &FormTree,
    node_id: FormNodeId,
    name: &str,
    max_depth: usize,
) -> bool {
    if node_id.0 >= form.nodes.len() {
        return false;
    }
    subtree_contains_name_inner(form, node_id, name, 0, max_depth)
}

fn subtree_contains_name_inner(
    form: &FormTree,
    node_id: FormNodeId,
    name: &str,
    depth: usize,
    max_depth: usize,
) -> bool {
    if depth >= max_depth {
        return false;
    }
    for &child_id in &form.get(node_id).children {
        if form.get(child_id).name == name {
            return true;
        }
        if subtree_contains_name_inner(form, child_id, name, depth + 1, max_depth) {
            return true;
        }
    }
    false
}

fn find_named_descendant_inner(
    form: &FormTree,
    node_id: FormNodeId,
    name: &str,
    depth: usize,
    max_depth: usize,
    candidates: &mut Vec<FormNodeId>,
) {
    if depth >= max_depth || candidates.len() >= MAX_RESOLVE_CANDIDATES {
        return;
    }
    for &child_id in &form.get(node_id).children {
        if candidates.len() >= MAX_RESOLVE_CANDIDATES {
            return;
        }
        if form.get(child_id).name == name {
            candidates.push(child_id);
            // Mirror prior semantics: do not recurse INTO a matched node;
            // continue scanning siblings so all top-level same-name hits
            // at this scope are visible to the bias selection.
        } else {
            find_named_descendant_inner(form, child_id, name, depth + 1, max_depth, candidates);
        }
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
    fn multi_segment_som_chain_lookahead() {
        // Phase D-θ: when two same-named siblings exist (`P1` × 2) but only
        // one contains a child called `X`, hinted child resolution must
        // collapse onto the P1 that owns `X`. Without lookahead the proxy
        // chain may pick the empty P1 (D-η ordering ties) and lose access
        // to `X.rawValue`.
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let f = add_node(
            &mut tree,
            "F",
            FormNodeType::Subform,
        );
        let p1_empty = add_node(
            &mut tree,
            "P1",
            FormNodeType::Subform,
        );
        let stub = add_node(
            &mut tree,
            "Stub",
            FormNodeType::Subform,
        );
        let p1_rich = add_node(
            &mut tree,
            "P1",
            FormNodeType::Subform,
        );
        let x = add_node(
            &mut tree,
            "X",
            FormNodeType::Field {
                value: "answer".to_string(),
            },
        );
        tree.get_mut(p1_empty).children = vec![stub];
        tree.get_mut(p1_rich).children = vec![x];
        tree.get_mut(f).children = vec![p1_empty, p1_rich];
        tree.get_mut(root).children = vec![f];

        let mut host = HostBindings::new();
        host.reset_per_document();
        host.set_form_handle(&mut tree as *mut FormTree, root);
        host.reset_per_script(root, Some("calculate"));

        // Without a hint, both P1's are returned in scope order.
        let plain = host.resolve_child_candidates(&[f], "P1");
        assert!(plain.contains(&p1_empty) && plain.contains(&p1_rich));

        // With the hint "X" the lookahead must keep ONLY the P1 that
        // actually has an X descendant.
        let hinted = host.resolve_child_candidates_hinted(&[f], "P1", "X");
        assert_eq!(hinted, vec![p1_rich]);

        // Hint that no candidate satisfies must fall back to the un-hinted
        // candidate set so chained access still has something to walk.
        let hinted_none = host.resolve_child_candidates_hinted(&[f], "P1", "Nope");
        assert!(hinted_none.contains(&p1_empty) && hinted_none.contains(&p1_rich));

        // Implicit-scope variant: from root the hint must still pin to the
        // populated P1 branch.
        let implicit = host.resolve_implicit_candidates_hinted(root, "P1", "X");
        assert_eq!(implicit, vec![p1_rich]);
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
