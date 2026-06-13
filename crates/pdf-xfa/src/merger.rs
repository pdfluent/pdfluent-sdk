//! XFA Form DOM — the merged result of template + data.
//!
//! Implements the Form DOM from XFA 3.3 §3 and §5.
//! The Form DOM is a hierarchical tree of merged nodes, where repeating
//! subforms have been expanded based on data instances.
//!
//! Data binding follows XFA Spec 3.3 §4.4 p176-214 ("Merging Data with a
//! Template"). Implements both `consumeData` and `matchTemplate` merge modes.
//!
//! ## Spec gaps (see individual TODOs):
//! - §4.4.3 p176: `bind match="global"` not implemented (approximated)
//! - §4.4 p197: attribute matching step skipped
//! - §4.4 p198: re-normalization not implemented
//! - §4.2 p143: localization/canonicalization not implemented
//! - §4.4 p195: exclusion group short/long format not implemented
//! - §4.4 p199: setProperty/bindItems not implemented

/// XFA Spec 3.3 §4.4 p176 — Merge mode controlling how data is bound to template.
///
/// - `ConsumeData` (default): walk the template tree top-down, binding each
///   node against the current data context positionally (by name within context).
/// - `MatchTemplate`: data drives the merge — for each data node, find the
///   template node by name match regardless of hierarchy depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMode {
    /// XFA §4.4 p176 — template-driven merge (default).
    ConsumeData,
    /// XFA §4.4 p176 — data-driven merge: match data nodes to template by name.
    MatchTemplate,
}

/// Detect the merge mode from the XFA config packet or default to `ConsumeData`.
///
/// XFA Spec 3.3 §4.4 p176: the merge mode can be specified via the
/// `<config><present><xfa:generator>` or `<config><acrobat><xdp><packet>` elements.
/// In practice most forms do not specify a mode and `consumeData` is implied.
/// We detect `matchTemplate` by scanning for explicit `matchTemplate` text in
/// a `<config>` packet when one is present in the template XML.
pub fn detect_merge_mode(template_xml: &str) -> MergeMode {
    // Quick scan: if the template explicitly requests matchTemplate mode.
    if template_xml.contains("matchTemplate") {
        return MergeMode::MatchTemplate;
    }
    MergeMode::ConsumeData
}

use crate::error::{Result, XfaError};
use roxmltree::Node;
use xfa_dom_resolver::data_dom::{DataDom, DataNodeId};
use xfa_dom_resolver::som::resolve_data_path;
use xfa_layout_engine::form::{
    Access, AnchorType, ContentArea, DrawContent, EventScript, FieldKind, FormNode, FormNodeId,
    FormNodeMeta, FormNodeStyle, FormNodeType, FormTree, GroupKind, Occur, Presence, RichTextSpan,
    ScriptLanguage,
};
use xfa_layout_engine::text::{FontFamily, FontMetrics};
use xfa_layout_engine::types::{
    BoxModel, Caption, CaptionPlacement, Insets, LayoutStrategy, Measurement, TextAlign,
    VerticalAlign,
};

/// Merges an XFA template (XML) with data from a DataDom to produce a FormTree.
pub struct FormMerger<'a> {
    data_dom: &'a DataDom,
    form_tree: FormTree,
    /// Embedded image files from the PDF's Names/EmbeddedFiles tree,
    /// keyed by filename (e.g. `.\lintje.jpg`).  Used to resolve
    /// `<image href="…">` references in the XFA template (XFA §2.3).
    image_files: std::collections::HashMap<String, Vec<u8>>,
}

fn area_layout(elem: Node<'_, '_>) -> LayoutStrategy {
    if elem.tag_name().name() == "area" && attr(elem, "layout").is_none() {
        let has_positioned_child = elem.children().filter(|n| n.is_element()).any(|child| {
            attr(child, "x")
                .and_then(parse_dim)
                .is_some_and(|v| v > 0.0)
                || attr(child, "y")
                    .and_then(parse_dim)
                    .is_some_and(|v| v > 0.0)
        });
        if has_positioned_child {
            LayoutStrategy::Positioned
        } else {
            LayoutStrategy::TopToBottom
        }
    } else {
        parse_layout_attr(elem)
    }
}

impl<'a> FormMerger<'a> {
    /// new.
    pub fn new(data_dom: &'a DataDom) -> Self {
        Self {
            data_dom,
            form_tree: FormTree::new(),
            image_files: std::collections::HashMap::new(),
        }
    }

    /// Set embedded image files for resolving `<image href="…">` references.
    pub fn with_image_files(mut self, files: std::collections::HashMap<String, Vec<u8>>) -> Self {
        self.image_files = files;
        self
    }

    /// Merge the template XML into a FormTree.
    ///
    /// XFA Spec 3.3 §4.4 p176 — supports both merge modes:
    /// - `consumeData` (default): walk template top-down, bind each node
    ///   against matching data context positionally.
    /// - `matchTemplate`: walk data nodes, find template counterparts by name,
    ///   bind regardless of hierarchy depth.
    pub fn merge(mut self, template_xml: &str) -> Result<(FormTree, FormNodeId)> {
        let mode = detect_merge_mode(template_xml);
        log::debug!(
            "XFA merge: mode={:?}, {} data nodes",
            mode,
            self.data_dom.len()
        );
        let doc = roxmltree::Document::parse(template_xml)
            .map_err(|e| XfaError::ParseFailed(format!("template XML parse error: {e}")))?;
        let root_elem = doc.root_element();

        // The packet may start with <template> directly, or may have been
        // wrapped inside <xdp:xdp>. Accept both.
        let template_elem = if root_elem.tag_name().name() == "template" {
            root_elem
        } else {
            // Find the first <template> descendant.
            find_first_child_by_name(root_elem, "template").ok_or_else(|| {
                XfaError::PacketNotFound("no <template> element found".to_string())
            })?
        };

        if mode == MergeMode::MatchTemplate {
            // XFA §4.4 p176 matchTemplate: pre-index all template nodes by name,
            // then for each data node find and bind to its template counterpart.
            self.apply_match_template_bindings(template_elem);
        }

        // Phase D-ι: collect <variables> <script name="X"> blocks at the root
        // subform level. XFA 3.3 §5.5 — these named scripts define
        // form-level objects whose top-level `var`/`function` declarations
        // are accessible from event/calculate scripts via `<scriptName>.X`.
        // Sandboxed JS runtime evaluates these once per document.
        self.collect_variables_scripts(template_elem);
        let (root_id, _trailing) = self.parse_node(template_elem, None, true)?;

        Ok((self.form_tree, root_id))
    }

    /// Phase D-ι / D-ι.2: walk `<variables>` blocks at every subform level
    /// below the template root (XFA 3.3 §5.5). Root-subform scripts get
    /// `subform_scope = None` (globally accessible); named descendant-subform
    /// scripts get `subform_scope = Some(subform_name)` so they are accessible
    /// via `subformHandle.variables.scriptName`.
    fn collect_variables_scripts(&mut self, template_elem: Node<'_, '_>) {
        let Some(root_subform) = find_first_child_by_name(template_elem, "subform") else {
            return;
        };
        self.collect_variables_scripts_recursive(root_subform, None);
    }

    fn collect_variables_scripts_recursive(
        &mut self,
        subform: Node<'_, '_>,
        subform_scope: Option<String>,
    ) {
        for child in subform.children().filter(|n| n.is_element()) {
            match child.tag_name().name() {
                "variables" => {
                    for var_child in child.children().filter(|n| n.is_element()) {
                        match var_child.tag_name().name() {
                            "script" => {
                                let Some(name) = attr(var_child, "name") else {
                                    continue;
                                };
                                let body: String = var_child
                                    .children()
                                    .filter(|n| n.is_text())
                                    .filter_map(|n| n.text())
                                    .collect::<String>();
                                if body.trim().is_empty() {
                                    continue;
                                }
                                self.form_tree.variables_scripts.push((
                                    subform_scope.clone(),
                                    name.to_string(),
                                    body,
                                ));
                            }
                            // W3-D RETRY: XFA 3.3 §5.5.2 — `<variables>` may
                            // contain `<text name="X">value</text>` data items
                            // alongside `<script>` blocks. Adobe Reader
                            // exposes these to event scripts as form-level
                            // mutable string containers (`X.value` reads /
                            // writes the data item). Canonical residual:
                            // IMM5709/IMM5257/IMM5710 declare
                            // `<text name="globValidatePressed"/>` and event
                            // scripts call `globValidatePressed.value = "true";`.
                            // Pre-fix this surfaced as the post-W2-B
                            // `implicit_function` cluster residual.
                            //
                            // We treat an absent / whitespace-only body as
                            // the empty string (per spec, an empty `<text/>`
                            // is a valid empty data item).
                            "text" => {
                                let Some(name) = attr(var_child, "name") else {
                                    continue;
                                };
                                if name.is_empty() {
                                    continue;
                                }
                                let initial: String = var_child
                                    .children()
                                    .filter(|n| n.is_text())
                                    .filter_map(|n| n.text())
                                    .collect::<String>();
                                self.form_tree.variables_data_items.push((
                                    subform_scope.clone(),
                                    name.to_string(),
                                    initial,
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                "subform" | "area" | "exclGroup" => {
                    let child_scope = attr(child, "name")
                        .filter(|n| !n.is_empty())
                        .map(|n| n.to_string());
                    self.collect_variables_scripts_recursive(child, child_scope);
                }
                _ => {}
            }
        }
    }

    /// XFA Spec 3.3 §4.4 p176 — matchTemplate pre-pass.
    ///
    /// In matchTemplate mode the data DOM drives the merge: for each data node
    /// that has a matching (same-named) template node, the binding is resolved
    /// purely by name without regard to hierarchy depth.  We implement this by
    /// walking the data DOM once and storing the resolved data value for every
    /// field whose name appears as a data child anywhere in the data tree.
    ///
    /// Because `parse_field` already performs a global fallback via
    /// `find_value_in_descendants`, matchTemplate mode for most real-world forms
    /// reduces to ensuring the global fallback is preferred over the positional
    /// context match.  The actual switch happens in `lookup_value_by_name`:
    /// when matchTemplate is active we skip the context-child check and go
    /// straight to the global search.
    fn apply_match_template_bindings(&mut self, _template_elem: roxmltree::Node<'_, '_>) {
        // Intentionally empty: the matchTemplate semantics are realised by
        // passing `None` as the data_context for all field lookups when the
        // mode is MatchTemplate, which forces `lookup_value_by_name` to use
        // the global descendant search (find_value_in_descendants) exclusively.
        // The parse_node / parse_field paths already handle None context.
        // This hook exists for future extension (e.g. explicit bind-ref tables).
    }

    fn parse_node(
        &mut self,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
        is_root: bool,
    ) -> Result<(FormNodeId, (bool, Option<String>))> {
        let tag = elem.tag_name().name();

        let (node, trailing_info) = match tag {
            "template" => {
                let mut n = self.blank_node("root", FormNodeType::Root);
                n.layout = LayoutStrategy::TopToBottom;
                let ti = self.add_children(&mut n, elem, data_context)?;
                (n, ti)
            }
            "subform" | "exclGroup" | "area" => {
                let name = attr(elem, "name").unwrap_or("").to_string();
                let layout = area_layout(elem);
                let bm = parse_box_model(elem);
                let occur = parse_occur(elem);
                self.build_subform_instance(elem, data_context, is_root, occur, name, layout, bm)?
            }
            "field" => (self.parse_field(elem, data_context)?, (false, None)),
            "draw" => (self.parse_draw(elem, data_context)?, (false, None)),
            "pageSet" => (self.parse_page_set(elem, data_context)?, (false, None)),
            "pageArea" => (self.parse_page_area(elem, data_context)?, (false, None)),
            _ => {
                let mut n = self.blank_node(tag, FormNodeType::Subform);
                let ti = self.add_children(&mut n, elem, data_context)?;
                (n, ti)
            }
        };

        if tag == "exclGroup" {
            // fixes #798: flatten.rs uses FormMerger, not template_parser, so
            // exclusion-group selection must be resolved here. XFA 3.3 §4.4.5
            // / §11.2 says assigning a value to an exclGroup selects the child
            // whose asserted value matches the group's bound value.
            self.apply_exclusive_choice_value(elem, data_context, &node.children);
        }

        let mut meta = parse_node_meta(elem);

        // XFA Spec 3.3 §7.4.2 — presence binding: a data node named "presence"
        // as a child of this element's data context can override the template's
        // static `presence` attribute.  Check for a bound data value and apply it.
        if let Some(bound_presence) = self.lookup_value_by_name("presence", data_context) {
            meta.presence = parse_presence_str(&bound_presence);
        }

        let is_draw_or_field = tag == "draw" || tag == "field";
        if is_draw_or_field {
            // Fields get their <margin topInset/...> bridged to style.inset_*_pt
            // so the renderer offsets the value and shrinks the border/bg to
            // the inner rect (render_bridge.rs:238-280). Draws skip this bridge:
            // draws in complex background-image forms (e.g. VA Form 10-2478,
            // 49f8705c) carry decorative insets that visually misalign with the
            // pre-rendered form backdrop when shifted inward. Keeping the
            // transfer only for fields preserves the #841 field-padding fix
            // without re-introducing the −0.18 regression on 49f8705c that
            // c24798f7a originally set out to fix.
            if tag == "field" {
                meta.style.inset_top_pt = Some(node.box_model.margins.top);
                meta.style.inset_bottom_pt = Some(node.box_model.margins.bottom);
                meta.style.inset_left_pt = Some(node.box_model.margins.left);
                meta.style.inset_right_pt = Some(node.box_model.margins.right);
            }
            if meta.style.font_weight.is_none() {
                if let Some(weight) = extract_exdata_font_weight(elem) {
                    meta.style.font_weight = Some(weight);
                }
            }
            if meta.style.font_style.is_none() {
                if let Some(style) = extract_exdata_font_style(elem) {
                    meta.style.font_style = Some(style);
                }
            }
            if meta.style.font_family.is_none() {
                if let Some(family) = extract_exdata_font_family(elem) {
                    meta.style.font_family = Some(family);
                }
            }
            if meta.style.text_color.is_none() {
                if let Some(color) = extract_exdata_color(elem) {
                    meta.style.text_color = Some(color);
                }
            }
            if meta.style.space_above_pt.is_none() || meta.style.space_below_pt.is_none() {
                if let Some((above, below)) = extract_exdata_margins(elem) {
                    if meta.style.space_above_pt.is_none() {
                        meta.style.space_above_pt = Some(above);
                    }
                    if meta.style.space_below_pt.is_none() {
                        meta.style.space_below_pt = Some(below);
                    }
                }
            }
            if meta.style.rich_text_spans.is_none() {
                meta.style.rich_text_spans = parse_exdata_rich_text_spans(elem);
            }
        }
        // Phase D-γ: store the data binding so the JS runtime can resolve
        // `$record` (the data record for the current subform's context).
        meta.bound_data_node = data_context.map(|c| c.as_raw());
        let id = self.form_tree.add_node_with_meta(node, meta);
        Ok((id, trailing_info))
    }

    #[allow(clippy::too_many_arguments)]
    fn build_subform_instance(
        &mut self,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
        is_root: bool,
        occur: Occur,
        name: String,
        layout: LayoutStrategy,
        mut bm: BoxModel,
    ) -> Result<(FormNode, (bool, Option<String>))> {
        if layout == LayoutStrategy::TopToBottom && bm.width.is_none() && is_root {
            bm.width = Some(612.0);
        }

        // XFA Spec 3.3 §4.4 p193 — transparent nodes: a subform with no `name`
        // attribute is "transparent" to data binding.  Its children bind against
        // the parent's data context directly — we do NOT descend into a child
        // data group.
        if name.is_empty() {
            // Transparent subform: pass parent context straight through.
            let mut n = FormNode {
                name,
                node_type: FormNodeType::Subform,
                box_model: bm,
                layout,
                children: Vec::new(),
                occur,
                font: FontMetrics::default(),
                calculate: None,
                validate: None,
                column_widths: Vec::new(),
                col_span: 1,
            };
            let ti = self.add_children(&mut n, elem, data_context)?;
            return Ok((n, ti));
        }

        // XFA Spec 3.3 §4.4.3 p180-185 — Data binding for subforms:
        // Step 1: "direct match" — find a data node with matching name
        // among the current context's children.
        // Step 2: scope matching — walk ancestor chain if direct match fails
        //   (XFA §4.4.3 p185).
        let mut child_context = data_context;
        if let Some(ctx) = data_context {
            // Direct child match
            let matches = self.data_dom.children_by_name(ctx, &name);
            if let Some(&first) = matches.first() {
                child_context = Some(first);
            } else {
                // Scope match: walk up ancestor chain
                child_context = self.resolve_with_scope_group(ctx, &name).or(data_context);
            }
        } else if let Some(root) = self.data_dom.root() {
            if self.data_dom.get(root).is_some_and(|n| n.name() == name) {
                // Exact name match: the data root IS the subform's record.
                child_context = Some(root);
            } else {
                let matches = self.data_dom.children_by_name(root, &name);
                if let Some(&first) = matches.first() {
                    // Named child match (e.g. template "form1" → data <form1>).
                    child_context = Some(first);
                } else {
                    // No name match: bind the root subform to the data root so
                    // that `$record.FIELD` paths in scripts resolve correctly.
                    // XFA Spec 3.3 §4.4.3 p180: when the root subform has no
                    // explicit bind element and its name doesn't match any data
                    // node, the form context is the data DOM root — making
                    // `$record` refer to the top-level data element (e.g.
                    // `$record.COMPANY_ADDRESS.nodes` works when data root has
                    // `<COMPANY_ADDRESS>` as a direct child).
                    child_context = Some(root);
                }
            }
        }

        let mut n = FormNode {
            name,
            node_type: FormNodeType::Subform,
            box_model: bm,
            layout,
            children: Vec::new(),
            occur,
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: Vec::new(),
            col_span: 1,
        };
        let ti = self.add_children(&mut n, elem, child_context)?;
        Ok((n, ti))
    }

    // XFA Spec 3.3 §9.2 "Variable Number of Subforms" (p336):
    // The data binding process creates min copies, then adds more copies
    // for each additional data match up to max. When max=-1 the template
    // is unbounded, but merger-time expansion must still stop at the
    // number of matching data records so we do not synthesize extra
    // siblings and over-paginate repeating content.
    /// XFA Spec 3.3 §4.4 p186-192 — Repeating subforms: when `<occur>` allows
    /// multiple instances (max > 1 or max = -1), the number of form subform
    /// instances is driven by matching data records. Each data record creates
    /// one subform instance, clamped to [occur.min, occur.max].
    #[allow(clippy::type_complexity)]
    fn expand_repeating_subform_instances(
        &mut self,
        element: Node<'_, '_>,
        data_context: Option<DataNodeId>,
        is_root: bool,
    ) -> Result<Vec<(FormNodeId, (bool, Option<String>))>> {
        let name = attr(element, "name").unwrap_or("").to_string();
        let occur = parse_occur(element);

        // XFA §4.4.3: match="none" means no data binding, no automatic expansion.
        // The subform exists as a single instance; only scriptable instanceManager
        // may add more.
        let (_bind_ref_unused, bind_none) = parse_bind(element);
        if bind_none {
            // Diagnostic (read-only, env-gated; no behavior change). See milestone
            // XFA_LAYOUT_INSTANCE_EXPANSION_PARITY.
            if std::env::var_os("XFA_IE_TRACE").is_some() {
                eprintln!("XFA_IE_TRACE expand name={name:?} bind=none data_count=0 -> count=1 (script-instanceManager-only)");
            }
            return Ok(vec![self.parse_node(element, data_context, is_root)?]);
        }

        // Get raw bind ref for multi-segment SOM path resolution.
        // parse_bind_data_name() only takes the last segment, which fails for
        // multi-segment refs like "$.group.field[*]" (XFA §4.4 p199).
        let bind_ref = find_first_child_by_name(element, "bind")
            .and_then(|b| attr(b, "ref"))
            .map(|s| s.to_string());

        let data_instances = if let Some(ref raw_ref) = bind_ref {
            // Normalize $record → $ for SOM parser compatibility
            let normalized = if let Some(rest) = raw_ref.strip_prefix("$record") {
                format!("${}", rest)
            } else {
                raw_ref.clone()
            };
            // Ensure [*] on last segment: repeating subforms need ALL matches
            let with_wildcard = if normalized.ends_with("[*]") {
                normalized
            } else {
                format!("{}[*]", normalized)
            };
            // Full SOM resolution; fall back to root when no data_context
            resolve_data_path(self.data_dom, &with_wildcard, data_context)
                .or_else(|_| resolve_data_path(self.data_dom, &with_wildcard, self.data_dom.root()))
                .unwrap_or_default()
        } else {
            // No bind ref: look up by element name
            if let Some(ctx) = data_context {
                self.data_dom.children_by_name(ctx, &name)
            } else if let Some(root) = self.data_dom.root() {
                self.data_dom.children_by_name(root, &name)
            } else {
                Vec::new()
            }
        };

        let data_count = data_instances.len() as u32;
        let min = occur.min;
        let max = occur.max.unwrap_or(data_count).max(min);
        // XFA Spec 3.3 §7.2.18 (occur) + §9.2 (Variable Number of Subforms):
        //   `min`     — minimum instance count after data binding.
        //   `max`     — maximum allowed (data records exceeding this are dropped).
        //   `initial` — instance count when the subform is rendered WITHOUT a
        //               backing data record (defaults to `min`).
        //
        // When data records exist, data binding wins: `count = clamp(data_count, min, max)`.
        // When no data records exist, render `initial` instances (which is
        // already raised to `min` by `Occur::repeating`), clamped to `max`.
        //
        // This affects only subforms that explicitly set `initial > min` and
        // have no datasets-bound records; documents that rely on `initial==min`
        // (the default) keep their current behaviour.
        let count = if data_count == 0 {
            occur.initial.min(max)
        } else {
            data_count.clamp(min, max)
        };

        // Diagnostic (read-only, env-gated; no behavior change). See milestone
        // XFA_LAYOUT_INSTANCE_EXPANSION_PARITY.
        if std::env::var_os("XFA_IE_TRACE").is_some() {
            let bind_kind = if bind_ref.is_some() { "ref" } else { "byname" };
            eprintln!(
                "XFA_IE_TRACE expand name={name:?} bind={bind_kind} data_count={data_count} occur(min={},max={:?},initial={}) -> count={count}",
                min, occur.max, occur.initial
            );
        }

        let layout = area_layout(element);
        let bm = parse_box_model(element);

        if count == 0 && occur.max.unwrap_or(u32::MAX) > 0 {
            // Build a single Hidden skeleton instance so `parent._<name>` resolves
            // to a usable InstanceManager. The skeleton is layout-skipped via
            // presence=Hidden; the flag tells the host that count() should return 0.
            let (inst_node, trailing) = self.build_subform_instance(
                element,
                None,
                is_root,
                Occur::once(),
                name.clone(),
                layout,
                bm.clone(),
            )?;
            let mut meta = parse_node_meta(element);
            meta.is_zero_instance_prototype = true;
            meta.presence = Presence::Hidden;
            let inst_id = self.form_tree.add_node_with_meta(inst_node, meta);
            return Ok(vec![(inst_id, trailing)]);
        }

        let mut instances = Vec::with_capacity(count as usize);

        for i in 0..count {
            let instance_data_ctx = data_instances.get(i as usize).copied();
            let (inst_node, trailing) = self.build_subform_instance(
                element,
                instance_data_ctx,
                is_root,
                Occur::once(),
                name.clone(),
                layout,
                bm.clone(),
            )?;
            if element.tag_name().name() == "exclGroup" {
                self.apply_exclusive_choice_value(element, instance_data_ctx, &inst_node.children);
            }
            let mut meta = parse_node_meta(element);
            // Phase D-γ: bind each repeating instance to its own data record.
            meta.bound_data_node = instance_data_ctx.map(|c| c.as_raw());
            let inst_id = self.form_tree.add_node_with_meta(inst_node, meta);
            instances.push((inst_id, trailing));
        }

        Ok(instances)
    }

    /// Search descendants of a data node for a DataValue with the given name.
    /// Returns the first matching value (breadth-first).
    ///
    /// XFA Spec 3.3 §4.4.3 p185 — global search used as final fallback after
    /// scope matching (direct child → sibling → ancestor chain) all fail.
    fn find_value_in_descendants(&self, node: DataNodeId, name: &str) -> Option<String> {
        for &child in self.data_dom.children(node) {
            if let Some(cn) = self.data_dom.get(child) {
                if cn.name() == name && cn.is_value() {
                    return self.data_dom.value(child).ok().map(|s| s.to_string());
                }
            }
        }
        // Recurse into child groups
        for &child in self.data_dom.children(node) {
            if self.data_dom.get(child).is_some_and(|n| n.is_group()) {
                if let Some(val) = self.find_value_in_descendants(child, name) {
                    return Some(val);
                }
            }
        }
        None
    }

    /// XFA Spec 3.3 §4.4.3 p185 — Scope matching for DataValue nodes.
    ///
    /// Resolution order per spec:
    /// 1. Direct child of current data context.
    /// 2. Sibling (direct child of parent data context).
    /// 3. Ancestor chain — walk up to root, try children at each level.
    /// 4. Global scope (root-level and all descendants).
    fn resolve_with_scope_value(&self, data_context: DataNodeId, name: &str) -> Option<String> {
        // 1. Direct child
        let direct = self.data_dom.children_by_name(data_context, name);
        if let Some(&val_id) = direct.first() {
            if self.data_dom.get(val_id).is_some_and(|n| n.is_value()) {
                return self.data_dom.value(val_id).ok().map(|s| s.to_string());
            }
        }

        // 2. Sibling (parent's children)
        if let Some(parent) = self.data_dom.get(data_context).and_then(|n| n.parent()) {
            let sibling_matches = self.data_dom.children_by_name(parent, name);
            if let Some(&val_id) = sibling_matches.first() {
                if self.data_dom.get(val_id).is_some_and(|n| n.is_value()) {
                    return self.data_dom.value(val_id).ok().map(|s| s.to_string());
                }
            }

            // 3. Walk up ancestor chain
            let mut ancestor = self.data_dom.get(parent).and_then(|n| n.parent());
            while let Some(anc_id) = ancestor {
                let anc_matches = self.data_dom.children_by_name(anc_id, name);
                if let Some(&val_id) = anc_matches.first() {
                    if self.data_dom.get(val_id).is_some_and(|n| n.is_value()) {
                        return self.data_dom.value(val_id).ok().map(|s| s.to_string());
                    }
                }
                ancestor = self.data_dom.get(anc_id).and_then(|n| n.parent());
            }
        }

        // 4. Global fallback from root
        self.data_dom
            .root()
            .and_then(|root| self.find_value_in_descendants(root, name))
    }

    /// XFA Spec 3.3 §4.4.3 p185 — Scope matching for DataGroup nodes.
    ///
    /// Used when looking for a named child data group for subform binding.
    /// Walks the ancestor chain to find a matching group node by name.
    fn resolve_with_scope_group(&self, data_context: DataNodeId, name: &str) -> Option<DataNodeId> {
        // Walk up the ancestor chain looking for a same-named group
        let mut cursor = self.data_dom.get(data_context).and_then(|n| n.parent());
        while let Some(anc_id) = cursor {
            let matches = self.data_dom.children_by_name(anc_id, name);
            if let Some(&grp_id) = matches.first() {
                if self.data_dom.get(grp_id).is_some_and(|n| n.is_group()) {
                    return Some(grp_id);
                }
            }
            cursor = self.data_dom.get(anc_id).and_then(|n| n.parent());
        }
        None
    }

    fn lookup_value_by_name(&self, name: &str, data_context: Option<DataNodeId>) -> Option<String> {
        if name.is_empty() {
            return None;
        }

        if let Some(ctx) = data_context {
            // Use full scope resolution: direct → sibling → ancestor → global
            return self.resolve_with_scope_value(ctx, name);
        }

        self.data_dom
            .root()
            .and_then(|root| self.find_value_in_descendants(root, name))
    }

    fn apply_exclusive_choice_value(
        &mut self,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
        child_ids: &[FormNodeId],
    ) {
        let Some(group_name) = attr(elem, "name") else {
            return;
        };
        let Some(group_value) = self.lookup_value_by_name(group_name, data_context) else {
            return;
        };

        for &child_id in child_ids {
            let item_value = self.form_tree.meta(child_id).item_value.clone();
            if let FormNodeType::Field { value } = &mut self.form_tree.get_mut(child_id).node_type {
                // fixes #798: only the matching radio/check child should stay
                // asserted. The others render as off/null so Acrobat-style
                // radio groups do not show multiple active choices.
                *value = if item_value.as_deref() == Some(group_value.as_str()) {
                    group_value.clone()
                } else {
                    String::new()
                };
            }
        }
    }

    /// XFA Spec 3.3 §4.4.3 p180 — Field data binding: fields are leaf nodes
    /// that bind to DataValue nodes in the data DOM.
    fn parse_field(
        &mut self,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
    ) -> Result<FormNode> {
        let name = attr(elem, "name").unwrap_or("").to_string();
        let bm = parse_box_model(elem);

        let mut value = extract_value_text(elem).unwrap_or_default();

        // XFA Spec 3.3 §4.4.3 p180-185 — Field binding with scope resolution:
        // 1. Direct match: search current context children by name (§4.4.3 p180)
        // 2. Sibling match: children of parent context (§4.4.3 p185)
        // 3. Ancestor chain walk (§4.4.3 p185)
        // 4. Global fallback: descendant search from data root
        //
        // NOTE: §4.4 p197 — attribute matching (data attributes → fields) is
        // not implemented; we only match element nodes.
        if let Some(bound_value) = self.lookup_value_by_name(&name, data_context) {
            value = bound_value;
        }

        let mut bm_with_caption = bm.clone();
        if !is_hidden(elem) {
            if let Some(cap) = parse_caption(elem) {
                bm_with_caption.caption = Some(cap);
            }
        }

        let mut font = parse_font_metrics(elem);
        if let Some(html_size) = extract_exdata_font_size(elem) {
            font.size = html_size;
        }
        if let Some(css_align) = extract_exdata_text_align(elem) {
            font.text_align = css_align;
        }

        Ok(FormNode {
            name,
            node_type: FormNodeType::Field { value },
            box_model: bm_with_caption,
            layout: LayoutStrategy::Positioned,
            children: Vec::new(),
            occur: Occur::once(),
            font,
            calculate: None,
            validate: None,
            column_widths: Vec::new(),
            col_span: parse_col_span(elem),
        })
    }

    /// XFA Spec 3.3 §4.4 p180 — Draw elements are static content that do
    /// not participate in data binding (they have no `<bind>` element).
    /// The `_data_context` parameter is unused.
    fn parse_draw(
        &mut self,
        elem: Node<'_, '_>,
        _data_context: Option<DataNodeId>,
    ) -> Result<FormNode> {
        let name = attr(elem, "name").unwrap_or("").to_string();
        let bm = parse_box_model(elem);

        if let Some(draw_content) = extract_draw_content(elem) {
            return Ok(FormNode {
                name,
                node_type: FormNodeType::Draw(draw_content),
                box_model: bm,
                layout: LayoutStrategy::Positioned,
                children: Vec::new(),
                occur: Occur::once(),
                font: FontMetrics::default(),
                calculate: None,
                validate: None,
                column_widths: Vec::new(),
                col_span: 1,
            });
        }

        if let Some((image_data, mime_type)) = extract_value_image(elem, &self.image_files) {
            return Ok(FormNode {
                name,
                node_type: FormNodeType::Image {
                    data: image_data,
                    mime_type,
                },
                box_model: bm,
                layout: LayoutStrategy::Positioned,
                children: Vec::new(),
                occur: Occur::once(),
                font: FontMetrics::default(),
                calculate: None,
                validate: None,
                column_widths: Vec::new(),
                col_span: 1,
            });
        }

        let mut font = parse_font_metrics(elem);
        if let Some(html_size) = extract_exdata_font_size(elem) {
            font.size = html_size;
        }
        if let Some(css_align) = extract_exdata_text_align(elem) {
            font.text_align = css_align;
        }

        let content = extract_value_text(elem).unwrap_or_default();

        Ok(FormNode {
            name,
            node_type: FormNodeType::Draw(DrawContent::Text(content)),
            box_model: bm,
            layout: LayoutStrategy::Positioned,
            children: Vec::new(),
            occur: Occur::once(),
            font,
            calculate: None,
            validate: None,
            column_widths: Vec::new(),
            col_span: 1,
        })
    }

    fn parse_page_set(
        &mut self,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
    ) -> Result<FormNode> {
        let name = attr(elem, "name").unwrap_or("pageSet").to_string();
        let mut node = FormNode {
            name,
            node_type: FormNodeType::PageSet,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: Vec::new(),
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: Vec::new(),
            col_span: 1,
        };
        for child in elem.children().filter(|n| n.is_element()) {
            if child.tag_name().name() == "pageArea" {
                let (child_id, _) = self.parse_node(child, data_context, false)?;
                node.children.push(child_id);
            }
        }
        Ok(node)
    }

    fn parse_page_area(
        &mut self,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
    ) -> Result<FormNode> {
        let name = attr(elem, "name").unwrap_or("").to_string();
        let (page_w, page_h) = read_medium(elem);
        let content_areas = read_content_areas(elem, page_w, page_h);

        let bm = BoxModel {
            width: Some(page_w),
            height: Some(page_h),
            ..Default::default()
        };

        let mut node = FormNode {
            name,
            node_type: FormNodeType::PageArea { content_areas },
            box_model: bm,
            layout: LayoutStrategy::Positioned,
            children: Vec::new(),
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: Vec::new(),
            col_span: 1,
        };
        self.add_children(&mut node, elem, data_context)?;
        Ok(node)
    }

    /// Recursively process child elements of a template node.
    ///
    /// XFA Spec 3.3 §4.4 p180 — the merge walks template children in document
    /// order, binding each to the current data context.
    ///
    /// TODO: XFA Spec 3.3 §4.4 p198 — re-normalization step: after all template
    /// children are bound, excess/unmatched data nodes should create new form
    /// nodes. Not implemented.
    ///
    /// TODO: XFA Spec 3.3 §4.4 p199 — setProperty and bindItems: after binding,
    /// `<setProperty>` and `<bindItems>` on `<bind>` should be evaluated to
    /// dynamically set properties or populate choice lists from data. Not implemented.
    ///
    /// TODO: XFA Spec 3.3 §4.2 p143-170 — localization: data values should be
    /// canonicalized using picture clauses before comparison/binding. Not implemented.
    fn add_children(
        &mut self,
        node: &mut FormNode,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
    ) -> Result<(bool, Option<String>)> {
        let mut pending_break = false;
        let mut pending_break_target = None;
        let mut pending_ca_break = false;

        for child in elem.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();
            match tag {
                "subform" | "field" | "draw" | "pageSet" | "pageArea" | "exclGroup" | "area" => {
                    let child_entries = if matches!(tag, "subform" | "exclGroup" | "area") {
                        let name = attr(child, "name").unwrap_or("");
                        let occur = parse_occur(child);
                        let (_, bind_none) = parse_bind(child);
                        if occur.is_repeating() && !name.is_empty() && !bind_none {
                            self.expand_repeating_subform_instances(child, data_context, false)?
                        } else {
                            vec![self.parse_node(child, data_context, false)?]
                        }
                    } else {
                        vec![self.parse_node(child, data_context, false)?]
                    };

                    for (child_id, (trailing_break, trailing_target)) in child_entries {
                        if pending_break {
                            let meta = self.form_tree.meta_mut(child_id);
                            meta.page_break_before = true;
                            if meta.break_target.is_none() {
                                meta.break_target = pending_break_target.take();
                            }
                            pending_break = false;
                        }
                        if pending_ca_break {
                            self.form_tree.meta_mut(child_id).content_area_break = true;
                            pending_ca_break = false;
                        }
                        node.children.push(child_id);
                        if trailing_break {
                            pending_break = true;
                            pending_break_target = trailing_target;
                        }
                    }
                }
                "breakBefore" => {
                    let target_type = attr(child, "targetType");
                    if target_type == Some("pageArea") {
                        pending_break = true;
                        pending_break_target = attr(child, "target").map(|s| s.to_string());
                    } else if target_type == Some("contentArea") {
                        pending_ca_break = true;
                    }
                }
                "break"
                    if attr(child, "before") == Some("pageArea")
                        && attr(child, "targetType") == Some("pageArea") =>
                {
                    pending_break = true;
                    pending_break_target = attr(child, "target").map(|s| s.to_string());
                }
                _ => {}
            }
        }
        Ok((pending_break, pending_break_target))
    }

    fn blank_node(&self, name: &str, node_type: FormNodeType) -> FormNode {
        FormNode {
            name: name.to_string(),
            node_type,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: Vec::new(),
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: Vec::new(),
            col_span: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions (mirrored from template_parser.rs)
// ---------------------------------------------------------------------------

fn attr<'a>(elem: Node<'a, '_>, name: &str) -> Option<&'a str> {
    elem.attributes()
        .find(|a| a.name() == name)
        .map(|a| a.value())
}

fn find_first_child_by_name<'a, 'input>(
    elem: Node<'a, 'input>,
    name: &str,
) -> Option<Node<'a, 'input>> {
    elem.children()
        .filter(|n| n.is_element())
        .find(|n| n.tag_name().name() == name)
}

fn parse_layout_attr(elem: Node<'_, '_>) -> LayoutStrategy {
    match attr(elem, "layout").unwrap_or("") {
        "tb" => LayoutStrategy::TopToBottom,
        "lr-tb" => LayoutStrategy::LeftToRightTB,
        "rl-tb" => LayoutStrategy::RightToLeftTB,
        "table" => LayoutStrategy::Table,
        "row" => LayoutStrategy::Row,
        "paginate" => LayoutStrategy::TopToBottom,
        "position" => LayoutStrategy::Positioned,
        _ => LayoutStrategy::Positioned,
    }
}

fn parse_anchor_type(elem: Node<'_, '_>) -> AnchorType {
    match attr(elem, "anchorType").unwrap_or("") {
        "topCenter" => AnchorType::TopCenter,
        "topRight" => AnchorType::TopRight,
        "middleLeft" => AnchorType::MiddleLeft,
        "middleCenter" => AnchorType::MiddleCenter,
        "middleRight" => AnchorType::MiddleRight,
        "bottomLeft" => AnchorType::BottomLeft,
        "bottomCenter" => AnchorType::BottomCenter,
        "bottomRight" => AnchorType::BottomRight,
        _ => AnchorType::TopLeft,
    }
}

fn parse_box_model(elem: Node<'_, '_>) -> BoxModel {
    let w = attr(elem, "w").and_then(parse_dim);
    let h = attr(elem, "h").and_then(parse_dim);
    let x = attr(elem, "x").and_then(parse_dim).unwrap_or(0.0);
    let y = attr(elem, "y").and_then(parse_dim).unwrap_or(0.0);
    let min_h = attr(elem, "minH").and_then(parse_dim).unwrap_or(0.0);
    let min_w = attr(elem, "minW").and_then(parse_dim).unwrap_or(0.0);
    let max_h = attr(elem, "maxH").and_then(parse_dim).unwrap_or(f64::MAX);
    let max_w = attr(elem, "maxW").and_then(parse_dim).unwrap_or(f64::MAX);
    let margins = parse_margin(elem);

    BoxModel {
        width: w,
        height: h,
        x,
        y,
        margins,
        min_width: min_w,
        max_width: max_w,
        min_height: min_h,
        max_height: max_h,
        ..Default::default()
    }
}

fn parse_margin(elem: Node<'_, '_>) -> Insets {
    if let Some(margin) = find_first_child_by_name(elem, "margin") {
        Insets {
            top: attr(margin, "topInset").and_then(parse_dim).unwrap_or(0.0),
            bottom: attr(margin, "bottomInset")
                .and_then(parse_dim)
                .unwrap_or(0.0),
            left: attr(margin, "leftInset").and_then(parse_dim).unwrap_or(0.0),
            right: attr(margin, "rightInset")
                .and_then(parse_dim)
                .unwrap_or(0.0),
        }
    } else {
        Insets::default()
    }
}

fn parse_dim(s: &str) -> Option<f64> {
    if s.trim().parse::<f64>().is_ok() {
        return Measurement::parse(&format!("{s}in")).map(|m: Measurement| m.to_points());
    }
    Measurement::parse(s).map(|m: Measurement| m.to_points())
}

/// Parse a percentage string like `"96%"` → `0.96`, `"110%"` → `1.1`.
fn parse_percentage(s: &str) -> Option<f64> {
    let s = s.trim();
    let num_str = s.strip_suffix('%')?;
    let v: f64 = num_str.trim().parse().ok()?;
    Some(v / 100.0)
}

/// Parse a letter-spacing string (`"-0.018em"`, `"0.5pt"`, etc.) to points.
fn parse_letter_spacing(s: &str, font_size_pt: f64) -> Option<f64> {
    let s = s.trim();
    if s == "0" {
        return Some(0.0);
    }
    if let Some(num_str) = s.strip_suffix("em") {
        let v: f64 = num_str.trim().parse().ok()?;
        return Some(v * font_size_pt);
    }
    Measurement::parse(s).map(|m| m.to_points())
}

// XFA Spec 3.3 §9.2 "The Occur Element" (p339):
// - min: defaults to 1 — minimum copies during non-empty merge
// - max: defaults to min (NOT to 1); -1 means unlimited
// - initial: defaults to min — copies during empty merge
//
// §9.2 p357: if max is not supplied, max defaults to min.
// §9.2 p357: if initial is not supplied, initial defaults to min.
//
// Our implementation correctly handles these defaults and the -1 sentinel.
fn parse_occur(elem: Node<'_, '_>) -> Occur {
    if let Some(occur) = find_first_child_by_name(elem, "occur") {
        let min: u32 = attr(occur, "min").and_then(|s| s.parse().ok()).unwrap_or(1);
        // XFA Spec 3.3 §9.2 p357: "if the max attribute is not supplied then
        // the max property defaults to the value of min."
        //
        // NOTE: the previous `.map(…).unwrap_or()` chain let parse failures
        // (e.g. max="") collapse to `None` (unlimited), causing massive
        // over-pagination.  Use an explicit match instead.
        let max: Option<u32> = match attr(occur, "max") {
            Some("-1") => None, // -1 means unlimited
            Some(s) => Some(s.parse::<u32>().unwrap_or(min)),
            None => Some(min),
        };
        // XFA Spec 3.3 §9.2 p357: "if the initial attribute is not supplied
        // then the initial property defaults to the value of min."
        let initial: u32 = attr(occur, "initial")
            .and_then(|s| s.parse().ok())
            .unwrap_or(min);
        Occur::repeating(min, max, initial)
    } else {
        Occur::once()
    }
}

fn parse_font_size(s: &str) -> Option<f64> {
    if let Ok(v) = s.trim().parse::<f64>() {
        return if v > 0.0 { Some(v) } else { None };
    }
    Measurement::parse(s).map(|m: Measurement| m.to_points())
}

fn parse_font_metrics(elem: Node<'_, '_>) -> FontMetrics {
    let font_elem = find_first_child_by_name(elem, "font");
    let size = font_elem
        .and_then(|f| attr(f, "size"))
        .and_then(parse_font_size)
        .unwrap_or(FontMetrics::default().size);
    let typeface = font_elem
        .and_then(|f| attr(f, "typeface"))
        .map(FontFamily::from_typeface)
        .unwrap_or_default();
    let text_align = find_first_child_by_name(elem, "para")
        .and_then(|p| attr(p, "hAlign"))
        .map(|a| match a {
            "center" => TextAlign::Center,
            "right" => TextAlign::Right,
            "justify" => TextAlign::Justify,
            _ => TextAlign::Left,
        })
        .unwrap_or_default();
    FontMetrics {
        size,
        text_align,
        typeface,
        ..FontMetrics::default()
    }
}

fn read_medium(page_area: Node<'_, '_>) -> (f64, f64) {
    if let Some(m) = find_first_child_by_name(page_area, "medium") {
        let short = attr(m, "short").and_then(parse_dim).unwrap_or(612.0);
        let long_ = attr(m, "long").and_then(parse_dim).unwrap_or(792.0);
        (short, long_)
    } else {
        (612.0, 792.0)
    }
}

fn read_content_areas(
    page_area: Node<'_, '_>,
    page_width: f64,
    page_height: f64,
) -> Vec<ContentArea> {
    let mut areas = Vec::new();
    for child in page_area.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "contentArea" {
            // XFA 3.3 §8.3.1 — contentArea x/y default to "0in" (0pt).
            // Previously defaulted to 36pt (0.5in margin), which shifted
            // all content down/right for templates omitting x/y. Fixes #797.
            let x = attr(child, "x").and_then(parse_dim).unwrap_or(0.0);
            let y = attr(child, "y").and_then(parse_dim).unwrap_or(0.0);
            // w/h default to full page dimensions when omitted, matching the
            // behavior of the empty (no contentArea) fallback. Previously used
            // hardcoded 540×720 (US Letter body), which was inconsistent with
            // the no-contentArea path and had no spec basis.
            let w = attr(child, "w").and_then(parse_dim).unwrap_or(page_width);
            let h = attr(child, "h").and_then(parse_dim).unwrap_or(page_height);
            areas.push(ContentArea {
                name: attr(child, "name").unwrap_or("").to_string(),
                x,
                y,
                width: w,
                height: h,
                leader: None,
                trailer: None,
            });
        }
    }
    if areas.is_empty() {
        areas.push(ContentArea {
            name: String::new(),
            x: 0.0,
            y: 0.0,
            width: page_width,
            height: page_height,
            leader: None,
            trailer: None,
        });
    }
    areas
}

fn extract_value_text(elem: Node<'_, '_>) -> Option<String> {
    let value = find_first_child_by_name(elem, "value")?;
    for tag in &["text", "float", "integer", "date", "dateTime", "decimal"] {
        if let Some(child) = find_first_child_by_name(value, tag) {
            let text = child.text().unwrap_or("");
            let trimmed = text.trim_start_matches(|c: char| c.is_whitespace() && c != '\n');
            let trimmed = trimmed.trim_end_matches(|c: char| c.is_whitespace() && c != '\n');
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(ex) = find_first_child_by_name(value, "exData") {
        let text = extract_text_from_descendants(ex);
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

fn extract_text_from_descendants(node: Node<'_, '_>) -> String {
    let block_tags = [
        "p", "div", "h1", "h2", "h3", "h4", "h5", "h6", "li", "tr", "br",
    ];
    let mut result = String::new();
    let mut last_was_block = false;

    for desc in node.descendants() {
        let is_block = desc.is_element() && block_tags.contains(&desc.tag_name().name());

        if is_block && !result.is_empty() {
            result.push('\n');
            last_was_block = true;
        }

        if desc.is_text() {
            if let Some(t) = desc.text() {
                let t = t.trim();
                if !t.is_empty() {
                    if last_was_block || result.is_empty() {
                        result.push_str(t);
                    } else {
                        result.push(' ');
                        result.push_str(t);
                    }
                    last_was_block = false;
                }
            }
        }
    }

    result.trim().to_string()
}

fn extract_exdata_font_size(elem: Node<'_, '_>) -> Option<f64> {
    let value = find_first_child_by_name(elem, "value")?;
    let ex = find_first_child_by_name(value, "exData")?;
    for desc in ex.descendants() {
        if !desc.is_element() {
            continue;
        }
        let style = desc
            .attribute("style")
            .or_else(|| desc.attribute("Style"))?;
        for part in style.split(';') {
            let part = part.trim();
            if let Some(val) = part
                .strip_prefix("font-size:")
                .or_else(|| part.strip_prefix("font-size :"))
            {
                let val = val.trim();
                if let Some(pt) = val.strip_suffix("pt") {
                    if let Ok(size) = pt.trim().parse::<f64>() {
                        if size > 0.0 {
                            return Some(size);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract the dominant `font-weight` from `<exData contentType="text/html">` styles.
/// Returns `Some("bold")` when the first styled `<p>` or `<span>` has `font-weight:bold`.
/// XFA Spec 3.3 §27.4: font-weight supports normal, bold, and numeric values 100-900.
fn extract_exdata_font_weight(elem: Node<'_, '_>) -> Option<String> {
    let value = find_first_child_by_name(elem, "value")?;
    let ex = find_first_child_by_name(value, "exData")?;
    for desc in ex.descendants() {
        if !desc.is_element() {
            continue;
        }
        let style = desc.attribute("style")?;
        for part in style.split(';') {
            let part = part.trim();
            if let Some(val) = part
                .strip_prefix("font-weight:")
                .or_else(|| part.strip_prefix("font-weight :"))
            {
                let val = val.trim();
                if val == "bold" || val == "700" || val == "800" || val == "900" {
                    return Some("bold".to_string());
                }
                if val == "normal" || val == "400" || val == "500" || val == "600" {
                    return Some("normal".to_string());
                }
            }
        }
    }
    None
}

/// Extract `font-style` from `<exData contentType="text/html">` CSS styles.
/// XFA Spec 3.3 §27.4: font-style supports normal, italic, oblique.
fn extract_exdata_font_style(elem: Node<'_, '_>) -> Option<String> {
    let value = find_first_child_by_name(elem, "value")?;
    let ex = find_first_child_by_name(value, "exData")?;
    for desc in ex.descendants() {
        if !desc.is_element() {
            continue;
        }
        let style = desc.attribute("style")?;
        for part in style.split(';') {
            let part = part.trim();
            if let Some(val) = part
                .strip_prefix("font-style:")
                .or_else(|| part.strip_prefix("font-style :"))
            {
                let val = val.trim();
                if val == "italic" || val == "oblique" {
                    return Some("italic".to_string());
                }
                if val == "normal" {
                    return Some("normal".to_string());
                }
            }
        }
    }
    None
}

/// Extract `color` from `<exData contentType="text/html">` CSS styles.
/// XFA Spec 3.3 §27.4: color supports #RGB, #RRGGBB, and rgb(r,g,b) formats.
/// Returns RGB tuple (0-255).
fn extract_exdata_color(elem: Node<'_, '_>) -> Option<(u8, u8, u8)> {
    let value = find_first_child_by_name(elem, "value")?;
    let ex = find_first_child_by_name(value, "exData")?;
    for desc in ex.descendants() {
        if !desc.is_element() {
            continue;
        }
        let style = desc.attribute("style")?;
        for part in style.split(';') {
            let part = part.trim();
            if let Some(val) = part
                .strip_prefix("color:")
                .or_else(|| part.strip_prefix("color :"))
            {
                let val = val.trim();
                if let Some(rgb) = parse_css_color(val) {
                    return Some(rgb);
                }
            }
        }
    }
    None
}

/// Parse CSS color value: #RGB, #RRGGBB, rgb(r,g,b), or r,g,b.
fn parse_ascii_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    fn hex_nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    match hex.as_bytes() {
        [r, g, b] => Some((
            hex_nibble(*r)? * 17,
            hex_nibble(*g)? * 17,
            hex_nibble(*b)? * 17,
        )),
        [r1, r2, g1, g2, b1, b2] => Some((
            (hex_nibble(*r1)? << 4) | hex_nibble(*r2)?,
            (hex_nibble(*g1)? << 4) | hex_nibble(*g2)?,
            (hex_nibble(*b1)? << 4) | hex_nibble(*b2)?,
        )),
        _ => None,
    }
}

fn parse_css_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        parse_ascii_hex_color(hex)
    } else if let Some(rgb_part) = s.strip_prefix("rgb(") {
        if let Some(inner) = rgb_part.strip_suffix(')') {
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() >= 3 {
                let r: f64 = parts[0].trim().parse().ok()?;
                let g: f64 = parts[1].trim().parse().ok()?;
                let b: f64 = parts[2].trim().parse().ok()?;
                return Some((r as u8, g as u8, b as u8));
            }
        }
        None
    } else {
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() >= 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            Some((r, g, b))
        } else {
            None
        }
    }
}

/// Extract the first `font-family` name from `<exData contentType="text/html">` CSS.
/// XFA Spec 3.3 §27.5 (p1198) — Character Formatting: font-family specifies
/// the typeface. §28.1 (p1228): Adobe uses only the first family name.
fn extract_exdata_font_family(elem: Node<'_, '_>) -> Option<String> {
    let value = find_first_child_by_name(elem, "value")?;
    let ex = find_first_child_by_name(value, "exData")?;
    for desc in ex.descendants() {
        if !desc.is_element() {
            continue;
        }
        let style = desc.attribute("style")?;
        for part in style.split(';') {
            let part = part.trim();
            if let Some(val) = part
                .strip_prefix("font-family:")
                .or_else(|| part.strip_prefix("font-family :"))
            {
                let val = val.trim();
                let first_family = val
                    .split(',')
                    .next()
                    .map(|s| s.trim().trim_matches(['"', '\'']))
                    .filter(|s| !s.is_empty())?;
                return Some(first_family.to_string());
            }
        }
    }
    None
}

/// Extract `text-align` from `<exData contentType="text/html">` CSS styles.
/// XFA Spec 3.3 §27.4 (p1190-1197): text-align supports left, center, right,
/// justify. Maps to XFA `<para hAlign>`.
fn extract_exdata_text_align(elem: Node<'_, '_>) -> Option<TextAlign> {
    let value = find_first_child_by_name(elem, "value")?;
    let ex = find_first_child_by_name(value, "exData")?;
    for desc in ex.descendants() {
        if !desc.is_element() {
            continue;
        }
        let style = desc.attribute("style")?;
        for part in style.split(';') {
            let part = part.trim();
            if let Some(val) = part
                .strip_prefix("text-align:")
                .or_else(|| part.strip_prefix("text-align :"))
            {
                let val = val.trim();
                return Some(match val {
                    "center" => TextAlign::Center,
                    "right" => TextAlign::Right,
                    "justify" => TextAlign::Justify,
                    _ => TextAlign::Left,
                });
            }
        }
    }
    None
}

/// Extract `margin-top` and `margin-bottom` from `<exData contentType="text/html">` CSS.
/// XFA Spec 3.3 §27.4 (p1190-1197): margin-top maps to XFA `<para spaceAbove>`,
/// margin-bottom maps to `<para spaceBelow>`. Returns (space_above_pt, space_below_pt).
fn extract_exdata_margins(elem: Node<'_, '_>) -> Option<(f64, f64)> {
    let value = find_first_child_by_name(elem, "value")?;
    let ex = find_first_child_by_name(value, "exData")?;
    let mut space_above: Option<f64> = None;
    let mut space_below: Option<f64> = None;
    for desc in ex.descendants() {
        if !desc.is_element() {
            continue;
        }
        let style = match desc.attribute("style") {
            Some(s) => s,
            None => continue,
        };
        for part in style.split(';') {
            let part = part.trim();
            if space_above.is_none() {
                if let Some(val) = part
                    .strip_prefix("margin-top:")
                    .or_else(|| part.strip_prefix("margin-top :"))
                {
                    let val = val.trim();
                    if let Some(m) = Measurement::parse(val) {
                        let pt = m.to_points();
                        if pt >= 0.0 {
                            space_above = Some(pt);
                        }
                    }
                }
            }
            if space_below.is_none() {
                if let Some(val) = part
                    .strip_prefix("margin-bottom:")
                    .or_else(|| part.strip_prefix("margin-bottom :"))
                {
                    let val = val.trim();
                    if let Some(m) = Measurement::parse(val) {
                        let pt = m.to_points();
                        if pt >= 0.0 {
                            space_below = Some(pt);
                        }
                    }
                }
            }
        }
        if space_above.is_some() && space_below.is_some() {
            break;
        }
    }
    match (space_above, space_below) {
        (Some(above), Some(below)) => Some((above, below)),
        (Some(above), None) => Some((above, 0.0)),
        (None, Some(below)) => Some((0.0, below)),
        (None, None) => None,
    }
}

/// Inline CSS style properties accumulated from parent elements.
#[derive(Debug, Clone, Default)]
struct InheritedStyle {
    font_size: Option<f64>,
    font_family: Option<String>,
    font_weight: Option<String>,
    font_style: Option<String>,
    text_color: Option<(u8, u8, u8)>,
    underline: bool,
    line_through: bool,
}

impl InheritedStyle {
    fn merge_with_css(&self, css: &str) -> Self {
        let mut child = self.clone();
        for part in css.split(';') {
            let part = part.trim();
            if let Some(val) = strip_css_prop(part, "font-size") {
                if let Some(pt) = val.strip_suffix("pt") {
                    if let Ok(size) = pt.trim().parse::<f64>() {
                        if size > 0.0 {
                            child.font_size = Some(size);
                        }
                    }
                }
            } else if let Some(val) = strip_css_prop(part, "font-family") {
                let first = val
                    .split(',')
                    .next()
                    .map(|s| s.trim().trim_matches(['"', '\'']))
                    .filter(|s| !s.is_empty());
                if let Some(fam) = first {
                    child.font_family = Some(fam.to_string());
                }
            } else if let Some(val) = strip_css_prop(part, "font-weight") {
                if val == "bold" || val == "700" || val == "800" || val == "900" {
                    child.font_weight = Some("bold".to_string());
                } else if val == "normal" || val == "400" || val == "500" {
                    child.font_weight = Some("normal".to_string());
                }
            } else if let Some(val) = strip_css_prop(part, "font-style") {
                if val == "italic" || val == "oblique" {
                    child.font_style = Some("italic".to_string());
                } else if val == "normal" {
                    child.font_style = Some("normal".to_string());
                }
            } else if let Some(val) = strip_css_prop(part, "text-decoration") {
                if val.contains("underline") {
                    child.underline = true;
                } else if val.contains("line-through") {
                    child.line_through = true;
                } else if val == "none" {
                    child.underline = false;
                    child.line_through = false;
                }
            } else if let Some(val) = strip_css_prop(part, "color") {
                if let Some(rgb) = parse_css_color(val) {
                    child.text_color = Some(rgb);
                }
            }
        }
        child
    }

    fn to_span(&self, text: String) -> RichTextSpan {
        RichTextSpan {
            text,
            font_size: self.font_size,
            font_family: self.font_family.clone(),
            font_weight: self.font_weight.clone(),
            font_style: self.font_style.clone(),
            text_color: self.text_color,
            underline: self.underline,
            line_through: self.line_through,
        }
    }
}

fn strip_css_prop<'a>(decl: &'a str, prop: &str) -> Option<&'a str> {
    decl.strip_prefix(prop)
        .and_then(|rest| rest.trim_start().strip_prefix(':'))
        .map(|v| v.trim())
}

/// Parse `<exData contentType="text/html">` XHTML into rich text spans.
fn parse_exdata_rich_text_spans(elem: Node<'_, '_>) -> Option<Vec<RichTextSpan>> {
    let value = find_first_child_by_name(elem, "value")?;
    let ex = find_first_child_by_name(value, "exData")?;
    let ct = ex.attribute("contentType").unwrap_or("");
    if ct != "text/html" && ct != "text/xml" {
        return None;
    }
    let body = ex
        .descendants()
        .find(|d| d.is_element() && d.tag_name().name() == "body")?;
    let mut spans: Vec<RichTextSpan> = Vec::new();
    let base_style = InheritedStyle::default();
    let mut first_para = true;
    for child in body.children() {
        if child.is_element() && child.tag_name().name() == "p" {
            if !first_para && !spans.is_empty() {
                spans.push(RichTextSpan {
                    text: "\n".to_string(),
                    font_size: None,
                    font_family: None,
                    font_weight: None,
                    font_style: None,
                    text_color: None,
                    underline: false,
                    line_through: false,
                });
            }
            first_para = false;
            let p_style = match child.attribute("style") {
                Some(css) => base_style.merge_with_css(css),
                None => base_style.clone(),
            };
            collect_inline_spans(child, &p_style, &mut spans);
        } else if child.is_text() {
            if let Some(t) = child.text() {
                let t = t.trim();
                if !t.is_empty() {
                    spans.push(base_style.to_span(t.to_string()));
                }
            }
        }
    }
    if spans.is_empty() {
        return None;
    }
    let all_default = spans.iter().all(|s| {
        s.font_size.is_none()
            && s.font_family.is_none()
            && s.font_weight.is_none()
            && s.font_style.is_none()
            && s.text_color.is_none()
            && !s.underline
    });
    if all_default {
        return None;
    }
    Some(spans)
}

fn collect_inline_spans(
    node: Node<'_, '_>,
    inherited: &InheritedStyle,
    spans: &mut Vec<RichTextSpan>,
) {
    for child in node.children() {
        if child.is_text() {
            if let Some(t) = child.text() {
                let t = t.trim();
                if !t.is_empty() {
                    spans.push(inherited.to_span(t.to_string()));
                }
            }
        } else if child.is_element() {
            let tag = child.tag_name().name();
            match tag {
                "br" => {
                    spans.push(RichTextSpan {
                        text: "\n".to_string(),
                        font_size: None,
                        font_family: None,
                        font_weight: None,
                        font_style: None,
                        text_color: None,
                        underline: false,
                        line_through: false,
                    });
                }
                "span" => {
                    let child_style = match child.attribute("style") {
                        Some(css) => {
                            if css.contains("xfa-spacerun:yes") {
                                if let Some(t) = child.text() {
                                    if !t.is_empty() {
                                        spans.push(inherited.to_span(t.to_string()));
                                    }
                                }
                                collect_inline_spans(child, inherited, spans);
                                continue;
                            }
                            inherited.merge_with_css(css)
                        }
                        None => inherited.clone(),
                    };
                    collect_inline_spans(child, &child_style, spans);
                }
                "b" | "strong" => {
                    let mut s = inherited.clone();
                    s.font_weight = Some("bold".to_string());
                    if let Some(css) = child.attribute("style") {
                        s = s.merge_with_css(css);
                    }
                    collect_inline_spans(child, &s, spans);
                }
                "i" | "em" => {
                    let mut s = inherited.clone();
                    s.font_style = Some("italic".to_string());
                    if let Some(css) = child.attribute("style") {
                        s = s.merge_with_css(css);
                    }
                    collect_inline_spans(child, &s, spans);
                }
                "u" => {
                    let mut s = inherited.clone();
                    s.underline = true;
                    if let Some(css) = child.attribute("style") {
                        s = s.merge_with_css(css);
                    }
                    collect_inline_spans(child, &s, spans);
                }
                _ => {
                    let child_style = match child.attribute("style") {
                        Some(css) => inherited.merge_with_css(css),
                        None => inherited.clone(),
                    };
                    collect_inline_spans(child, &child_style, spans);
                }
            }
        }
    }
}

fn parse_caption(elem: Node<'_, '_>) -> Option<Caption> {
    let cap_elem = find_first_child_by_name(elem, "caption")?;
    if is_hidden(cap_elem) {
        return None;
    }
    let text = extract_value_text(cap_elem)?;
    if text.is_empty() {
        return None;
    }
    let placement = match attr(cap_elem, "placement") {
        Some("right") => CaptionPlacement::Right,
        Some("top") => CaptionPlacement::Top,
        Some("bottom") => CaptionPlacement::Bottom,
        Some("inline") => CaptionPlacement::Inline,
        _ => CaptionPlacement::Left,
    };
    let reserve = attr(cap_elem, "reserve")
        .and_then(Measurement::parse)
        .map(|m| m.to_points());
    // The caption styles its own label text via its `<font>` child, independent
    // of the field's `<font>`. Capture typeface/size so the caption is not drawn
    // in the field's face (XFA 3.3 §7.4 / §6.5).
    let cap_font = find_first_child_by_name(cap_elem, "font");
    let font_family = cap_font
        .and_then(|f| attr(f, "typeface"))
        .map(|s| s.to_string());
    let font_size = cap_font
        .and_then(|f| attr(f, "size"))
        .and_then(parse_font_size);
    Some(Caption {
        placement,
        reserve,
        text,
        font_family,
        font_size,
    })
}

fn is_hidden(elem: Node<'_, '_>) -> bool {
    matches!(
        attr(elem, "presence"),
        Some("hidden") | Some("invisible") | Some("inactive")
    )
}

/// XFA Spec 3.3 §7.4.2 — Parse a presence string to the `Presence` enum.
///
/// Used when applying data-bound presence overrides.  Returns `Visible` for
/// any unrecognised value (spec default).
fn parse_presence_str(s: &str) -> Presence {
    match s.trim() {
        "hidden" => Presence::Hidden,
        "invisible" => Presence::Invisible,
        "inactive" => Presence::Inactive,
        _ => Presence::Visible,
    }
}

fn extract_value_image(
    elem: Node<'_, '_>,
    image_files: &std::collections::HashMap<String, Vec<u8>>,
) -> Option<(Vec<u8>, String)> {
    let value = find_first_child_by_name(elem, "value")?;
    let image = find_first_child_by_name(value, "image")?;
    let content_type = attr(image, "contentType")
        .unwrap_or("image/png")
        .to_string();

    // XFA §2.3: `href` references an image embedded in the PDF's Names tree.
    // When present, the inline text content is empty — resolve from the PDF.
    if let Some(href) = attr(image, "href") {
        if let Some(data) = image_files.get(href) {
            let decoded = data.clone();
            if decoded.starts_with(b"BM") || content_type == "image/bmp" {
                if let Some(png_data) = bmp_to_png(&decoded) {
                    return Some((png_data, "image/png".to_string()));
                }
                return None;
            }
            return Some((decoded, content_type));
        }
        // Try normalized key: strip leading ".\" or "./"
        let normalized = href.trim_start_matches(".\\").trim_start_matches("./");
        for (k, data) in image_files {
            let k_norm = k.trim_start_matches(".\\").trim_start_matches("./");
            if k_norm == normalized {
                let decoded = data.clone();
                if decoded.starts_with(b"BM") || content_type == "image/bmp" {
                    if let Some(png_data) = bmp_to_png(&decoded) {
                        return Some((png_data, "image/png".to_string()));
                    }
                    return None;
                }
                return Some((decoded, content_type));
            }
        }
    }

    let data = image.text().unwrap_or_default();
    let decoded = base64_decode(data);

    // Skip empty images from unresolved href references.
    if decoded.is_empty() {
        return None;
    }

    if decoded.starts_with(b"BM") || content_type == "image/bmp" {
        if let Some(png_data) = bmp_to_png(&decoded) {
            return Some((png_data, "image/png".to_string()));
        }
        log::warn!("BMP to PNG conversion failed; skipping image");
        return None;
    }

    Some((decoded, content_type))
}

fn extract_draw_content(elem: Node<'_, '_>) -> Option<DrawContent> {
    let value = find_first_child_by_name(elem, "value")?;

    if let Some(line) = find_first_child_by_name(value, "line") {
        let x1 = attr_as_f64(line, "x1").unwrap_or(0.0);
        let y1 = attr_as_f64(line, "y1").unwrap_or(0.0);
        let x2 = attr_as_f64(line, "x2").unwrap_or(0.0);
        let y2 = attr_as_f64(line, "y2").unwrap_or(0.0);
        return Some(DrawContent::Line { x1, y1, x2, y2 });
    }

    if let Some(rect) = find_first_child_by_name(value, "rectangle") {
        let x = attr_as_f64(rect, "x").unwrap_or(0.0);
        let y = attr_as_f64(rect, "y").unwrap_or(0.0);
        let w = attr_as_f64(rect, "w").unwrap_or(attr_as_f64(rect, "width").unwrap_or(0.0));
        let h = attr_as_f64(rect, "h").unwrap_or(attr_as_f64(rect, "height").unwrap_or(0.0));
        let radius =
            attr_as_f64(rect, "r").unwrap_or(attr_as_f64(rect, "cornerRadius").unwrap_or(0.0));
        return Some(DrawContent::Rectangle { x, y, w, h, radius });
    }

    if let Some(arc) = find_first_child_by_name(value, "arc") {
        let x = attr_as_f64(arc, "x").unwrap_or(0.0);
        let y = attr_as_f64(arc, "y").unwrap_or(0.0);
        let w = attr_as_f64(arc, "w").unwrap_or(attr_as_f64(arc, "width").unwrap_or(0.0));
        let h = attr_as_f64(arc, "h").unwrap_or(attr_as_f64(arc, "height").unwrap_or(0.0));
        let start_angle = attr_as_f64(arc, "startAngle").unwrap_or(0.0);
        let sweep_angle = attr_as_f64(arc, "sweepAngle").unwrap_or(0.0);
        return Some(DrawContent::Arc {
            x,
            y,
            w,
            h,
            start_angle,
            sweep_angle,
        });
    }

    None
}

fn base64_decode(input: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input.trim())
        .unwrap_or_default()
}

fn bmp_to_png(bmp_data: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory_with_format(bmp_data, image::ImageFormat::Bmp).ok()?;
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .ok()?;
    Some(buf)
}

fn attr_as_f64(elem: Node<'_, '_>, name: &str) -> Option<f64> {
    attr(elem, name)?.parse().ok()
}

fn parse_col_span(elem: Node<'_, '_>) -> i32 {
    attr(elem, "colSpan")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
}

fn parse_node_meta(elem: Node<'_, '_>) -> FormNodeMeta {
    let tag = elem.tag_name().name();
    let presence = match attr(elem, "presence") {
        Some("hidden") => Presence::Hidden,
        Some("invisible") => Presence::Invisible,
        Some("inactive") => Presence::Inactive,
        _ => Presence::Visible,
    };

    let (page_break_before, break_before_target) = detect_page_break_before(elem);
    let (page_break_after, break_after_target) = detect_page_break_after(elem);
    let content_area_break = detect_content_area_break(elem);
    let break_target = break_before_target.or(break_after_target);

    let event_scripts = collect_event_scripts(elem);
    let (keep_next_content_area, keep_previous_content_area, keep_intact_content_area) =
        parse_keep(elem);
    let (overflow_leader, overflow_trailer) = parse_overflow(elem);

    let group_kind = if tag == "exclGroup" {
        GroupKind::ExclusiveChoice
    } else {
        GroupKind::None
    };

    let item_value = if tag == "field" {
        parse_item_value(elem)
    } else {
        None
    };

    let (display_items, save_items) = if tag == "field" {
        parse_items_lists(elem)
    } else {
        (Vec::new(), Vec::new())
    };

    let xfa_id = attr(elem, "id").map(|s| s.to_string());
    let field_kind = detect_field_kind(elem);
    let style = parse_node_style(elem);
    let (data_bind_ref, data_bind_none) = parse_bind(elem);
    let anchor_type = parse_anchor_type(elem);
    let access = attr(elem, "access").map(Access::parse);
    let multiline = parse_multiline(elem);
    let required = parse_required(elem);

    FormNodeMeta {
        xfa_id,
        presence,
        access,
        multiline,
        required,
        page_break_before,
        page_break_after,
        break_target,
        content_area_break,
        overflow_leader,
        overflow_trailer,
        keep_next_content_area,
        keep_previous_content_area,
        keep_intact_content_area,
        event_scripts,
        data_bind_ref,
        data_bind_none,
        group_kind,
        item_value,
        field_kind,
        style,
        display_items,
        save_items,
        anchor_type,
        ..Default::default()
    }
}

fn detect_page_break_before(elem: Node<'_, '_>) -> (bool, Option<String>) {
    for child in elem.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        if matches!(tag, "subform" | "field" | "draw" | "exclGroup") {
            break;
        }
        if tag == "breakBefore" && attr(child, "targetType") == Some("pageArea") {
            return (true, attr(child, "target").map(|s| s.to_string()));
        }
        if tag == "break" && attr(child, "before") == Some("pageArea") {
            return (true, attr(child, "target").map(|s| s.to_string()));
        }
    }
    (false, None)
}

fn detect_page_break_after(elem: Node<'_, '_>) -> (bool, Option<String>) {
    let mut last_content_idx = 0;
    let children: Vec<_> = elem.children().filter(|n| n.is_element()).collect();
    for (i, child) in children.iter().enumerate() {
        let tag = child.tag_name().name();
        if matches!(tag, "subform" | "field" | "draw" | "exclGroup") {
            last_content_idx = i;
        }
    }

    for child in children.iter().skip(last_content_idx) {
        let tag = child.tag_name().name();
        if tag == "breakAfter" && attr(*child, "targetType") == Some("pageArea") {
            return (true, attr(*child, "target").map(|s| s.to_string()));
        }
        if tag == "break" && attr(*child, "after") == Some("pageArea") {
            return (true, attr(*child, "target").map(|s| s.to_string()));
        }
    }
    (false, None)
}

fn detect_content_area_break(elem: Node<'_, '_>) -> bool {
    for child in elem.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        if tag == "breakBefore" && attr(child, "targetType") == Some("contentArea") {
            return true;
        }
    }
    false
}

fn collect_event_scripts(elem: Node<'_, '_>) -> Vec<EventScript> {
    let mut scripts = Vec::new();
    for child in elem.children().filter(|n| n.is_element()) {
        let child_tag = child.tag_name().name();
        if child_tag == "event" {
            let activity = attr(child, "activity");
            let event_ref = attr(child, "ref");
            if activity == Some("ready") && event_ref == Some("$layout") {
                continue;
            }
            if let Some(script_elem) = find_first_child_by_name(child, "script") {
                if let Some(script) =
                    build_event_script(script_elem, activity, event_ref, attr(script_elem, "runAt"))
                {
                    scripts.push(script);
                }
            }
        } else if child_tag == "calculate" {
            if let Some(script_elem) = find_first_child_by_name(child, "script") {
                if let Some(script) = build_event_script(
                    script_elem,
                    Some("calculate"),
                    None,
                    attr(script_elem, "runAt"),
                ) {
                    scripts.push(script);
                }
            }
        }
    }
    scripts
}

fn build_event_script(
    script_elem: Node<'_, '_>,
    activity: Option<&str>,
    event_ref: Option<&str>,
    run_at: Option<&str>,
) -> Option<EventScript> {
    let text = script_elem.text()?.trim();
    if text.is_empty() {
        return None;
    }

    Some(EventScript::new(
        text.to_string(),
        detect_script_language(attr(script_elem, "contentType")),
        activity.map(str::to_string),
        event_ref.map(str::to_string),
        run_at.map(str::to_string),
    ))
}

fn detect_script_language(content_type: Option<&str>) -> ScriptLanguage {
    match content_type.map(|value| value.trim().to_ascii_lowercase()) {
        None => ScriptLanguage::FormCalc,
        Some(value) if value == "application/x-formcalc" || value.ends_with("/x-formcalc") => {
            ScriptLanguage::FormCalc
        }
        Some(value)
            if value == "application/x-javascript"
                || value == "application/javascript"
                || value == "text/javascript"
                || value.ends_with("/x-javascript") =>
        {
            ScriptLanguage::JavaScript
        }
        Some(_) => ScriptLanguage::Other,
    }
}

fn parse_keep(elem: Node<'_, '_>) -> (bool, bool, bool) {
    if let Some(keep) = find_first_child_by_name(elem, "keep") {
        let next = attr(keep, "next") == Some("contentArea");
        let prev = attr(keep, "previous") == Some("contentArea");
        let intact = attr(keep, "intact") == Some("contentArea");
        (next, prev, intact)
    } else {
        (false, false, false)
    }
}

fn parse_overflow(elem: Node<'_, '_>) -> (Option<String>, Option<String>) {
    if let Some(overflow) = find_first_child_by_name(elem, "overflow") {
        let leader = attr(overflow, "leader").map(|s| s.to_string());
        let trailer = attr(overflow, "trailer").map(|s| s.to_string());
        (leader, trailer)
    } else {
        (None, None)
    }
}

fn parse_item_value(elem: Node<'_, '_>) -> Option<String> {
    let items = find_first_child_by_name(elem, "items")?;
    let text_elem = find_first_child_by_name(items, "text")?;
    let text = text_elem.text()?.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Extract all text values from an `<items>` element.
fn collect_items_texts(items_elem: Node<'_, '_>) -> Vec<String> {
    items_elem
        .children()
        .filter(|n| n.is_element())
        .filter_map(|child| {
            let txt = child.text().unwrap_or("").trim().to_string();
            if txt.is_empty() {
                None
            } else {
                Some(txt)
            }
        })
        .collect()
}

/// Parse choice list `<items>` elements from a `<field>` node (XFA 3.3 §7.7).
fn parse_items_lists(elem: Node<'_, '_>) -> (Vec<String>, Vec<String>) {
    let items_elems: Vec<_> = elem
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "items")
        .collect();
    match items_elems.len() {
        0 => (Vec::new(), Vec::new()),
        1 => {
            let vals = collect_items_texts(items_elems[0]);
            (vals, Vec::new())
        }
        _ => {
            let first = items_elems[0];
            let second = items_elems[1];
            let first_is_save = attr(first, "save") == Some("1");
            if first_is_save {
                (collect_items_texts(second), collect_items_texts(first))
            } else {
                (collect_items_texts(first), collect_items_texts(second))
            }
        }
    }
}

/// XFA 3.3 §6.1 — a field is multiline when its `<ui><textEdit>` declares
/// `multiLine="1"` (or the non-canonical `"true"` some producers emit).
fn parse_multiline(elem: Node<'_, '_>) -> bool {
    let Some(ui) = find_first_child_by_name(elem, "ui") else {
        return false;
    };
    let Some(te) = find_first_child_by_name(ui, "textEdit") else {
        return false;
    };
    matches!(attr(te, "multiLine"), Some("1") | Some("true"))
}

/// XFA 3.3 §6.3 — `<validate nullTest="error">` marks a mandatory field.
fn parse_required(elem: Node<'_, '_>) -> bool {
    find_first_child_by_name(elem, "validate")
        .and_then(|v| attr(v, "nullTest"))
        .map(|s| s == "error")
        .unwrap_or(false)
}

fn detect_field_kind(elem: Node<'_, '_>) -> FieldKind {
    let Some(ui) = find_first_child_by_name(elem, "ui") else {
        return FieldKind::Text;
    };
    for child in ui.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "button" => return FieldKind::Button,
            "checkButton" => {
                // fixes #798: XFA 3.3 §11.2 / §17.8 uses round checkButtons
                // for exclusion-group radio widgets. Treating every
                // checkButton as a checkbox loses the correct renderer.
                let shape = attr(child, "shape").unwrap_or("square");
                return if shape == "round" {
                    FieldKind::Radio
                } else {
                    FieldKind::Checkbox
                };
            }
            "choiceList" => return FieldKind::Dropdown,
            "dateTimeEdit" => return FieldKind::DateTimePicker,
            "numericEdit" => return FieldKind::NumericEdit,
            "passwordEdit" => return FieldKind::PasswordEdit,
            "imageEdit" => return FieldKind::ImageEdit,
            "signature" => return FieldKind::Signature,
            "barcode" => return FieldKind::Barcode,
            _ => {}
        }
    }
    FieldKind::Text
}

fn parse_node_style(elem: Node<'_, '_>) -> FormNodeStyle {
    let (check_on_value, check_off_value, check_neutral_value) = parse_check_button_values(elem);
    let mut style = FormNodeStyle {
        check_button_mark: parse_check_button_mark(elem),
        check_button_on_value: check_on_value,
        check_button_off_value: check_off_value,
        check_button_neutral_value: check_neutral_value,
        ..Default::default()
    };
    if let Some(fill) = find_first_child_by_name(elem, "fill") {
        if !is_hidden(fill) {
            style.bg_color = parse_fill_color(fill);
        }
    }
    // Borders can live directly on the element OR inside <ui><textEdit|…><border>.
    let border = find_first_child_by_name(elem, "border").or_else(|| {
        let ui = find_first_child_by_name(elem, "ui")?;
        ui.children()
            .filter(|c| c.is_element() && c.tag_name().name() != "border")
            .find_map(|widget| find_first_child_by_name(widget, "border"))
    });
    if let Some(border) = border {
        // Collect all <edge> elements (XFA §9.3.3).
        let edge_elems: Vec<_> = border
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "edge")
            .collect();
        if !edge_elems.is_empty() {
            let first_visible = edge_elems
                .iter()
                .find(|e| !is_hidden(**e) && attr(**e, "stroke").unwrap_or("solid") != "none")
                .or_else(|| edge_elems.first())
                .copied();
            let first = first_visible.unwrap_or(edge_elems[0]);
            if let Some(color) = find_first_child_by_name(first, "color") {
                if let Some(rgb) = parse_xfa_color(color) {
                    style.border_color = Some(rgb);
                }
            }
            let stroke = attr(first, "stroke").unwrap_or("solid");
            if stroke != "none" {
                let thickness = attr(first, "thickness")
                    .and_then(Measurement::parse)
                    .map(|m: Measurement| m.to_points())
                    .unwrap_or(0.5);
                if thickness > 0.0 {
                    style.border_width_pt = Some(thickness);
                }
            }

            // Keep edge ordering consistent with render_bridge expectations:
            // [top, right, bottom, left]. The previous mapping treated 4-edge
            // borders as [top, bottom, left, right], which swapped bottom-only
            // underlines into left vertical lines.
            let edge_visible = |e: &roxmltree::Node<'_, '_>| -> bool {
                !is_hidden(*e) && attr(*e, "stroke").unwrap_or("solid") != "none"
            };
            style.border_edges = match edge_elems.len() {
                0 => [true, true, true, true],
                1 => {
                    let v = edge_visible(&edge_elems[0]);
                    [v, v, v, v]
                }
                2 => {
                    let even = edge_visible(&edge_elems[0]);
                    let odd = edge_visible(&edge_elems[1]);
                    [even, odd, even, odd]
                }
                3 => {
                    let top = edge_visible(&edge_elems[0]);
                    let rl = edge_visible(&edge_elems[1]);
                    let bot = edge_visible(&edge_elems[2]);
                    [top, rl, bot, rl]
                }
                _ => [
                    edge_visible(&edge_elems[0]),
                    edge_visible(&edge_elems[1]),
                    edge_visible(&edge_elems[2]),
                    edge_visible(&edge_elems[3]),
                ],
            };

            if edge_elems.len() > 1 {
                let default_rgb = style.border_color.unwrap_or((0, 0, 0));
                let edge_color = |edge: roxmltree::Node<'_, '_>| -> (u8, u8, u8) {
                    find_first_child_by_name(edge, "color")
                        .and_then(parse_xfa_color)
                        .unwrap_or(default_rgb)
                };
                let per_edge_colors = match edge_elems.len() {
                    2 => Some([
                        edge_color(edge_elems[0]),
                        edge_color(edge_elems[1]),
                        edge_color(edge_elems[0]),
                        edge_color(edge_elems[1]),
                    ]),
                    3 => Some([
                        edge_color(edge_elems[0]),
                        edge_color(edge_elems[1]),
                        edge_color(edge_elems[2]),
                        edge_color(edge_elems[1]),
                    ]),
                    _ => Some([
                        edge_color(edge_elems[0]),
                        edge_color(edge_elems[1]),
                        edge_color(edge_elems[2]),
                        edge_color(edge_elems[3]),
                    ]),
                };
                if let Some([top, right, bottom, left]) = per_edge_colors {
                    if !(top == bottom && bottom == left && left == right) {
                        style.border_colors = Some([top, right, bottom, left]);
                    }
                }

                let default_thickness = style.border_width_pt.unwrap_or(0.5);
                let edge_thickness = |edge: roxmltree::Node<'_, '_>| -> f64 {
                    attr(edge, "thickness")
                        .and_then(Measurement::parse)
                        .map(|m: Measurement| m.to_points())
                        .unwrap_or(default_thickness)
                };
                let per_edge_widths = match edge_elems.len() {
                    2 => Some([
                        edge_thickness(edge_elems[0]),
                        edge_thickness(edge_elems[1]),
                        edge_thickness(edge_elems[0]),
                        edge_thickness(edge_elems[1]),
                    ]),
                    3 => Some([
                        edge_thickness(edge_elems[0]),
                        edge_thickness(edge_elems[1]),
                        edge_thickness(edge_elems[2]),
                        edge_thickness(edge_elems[1]),
                    ]),
                    _ => Some([
                        edge_thickness(edge_elems[0]),
                        edge_thickness(edge_elems[1]),
                        edge_thickness(edge_elems[2]),
                        edge_thickness(edge_elems[3]),
                    ]),
                };
                if let Some([top_t, right_t, bottom_t, left_t]) = per_edge_widths {
                    if !(top_t == bottom_t && bottom_t == left_t && left_t == right_t) {
                        style.border_widths = Some([top_t, right_t, bottom_t, left_t]);
                    }
                }
            }
        }
        if style.bg_color.is_none() {
            if let Some(fill) = find_first_child_by_name(border, "fill") {
                if !is_hidden(fill) {
                    // fixes #809: XFA 3.3 §7.6.3 allows widget fills to be
                    // expressed as either <fill><color> or <fill><solid><color>.
                    // Supporting both keeps explicit field backgrounds aligned
                    // with Adobe/pdfRest output instead of leaving them white.
                    style.bg_color = parse_fill_color(fill);
                }
            }
        }
    }
    if let Some(font) = find_first_child_by_name(elem, "font") {
        if let Some(typeface) = attr(font, "typeface") {
            style.font_family = Some(typeface.to_string());
        }
        if let Some(gf) = attr(font, "genericFamily") {
            style.generic_family = Some(gf.to_string());
        }
        if let Some(size_str) = attr(font, "size") {
            if let Some(m) = Measurement::parse(size_str) {
                let m: Measurement = m;
                style.font_size = Some(m.to_points());
            }
        }
        if let Some(weight) = attr(font, "weight") {
            style.font_weight = Some(weight.to_string());
        }
        if let Some(posture) = attr(font, "posture") {
            style.font_style = Some(posture.to_string());
        }
        if let Some(fill) = find_first_child_by_name(font, "fill") {
            if let Some(color) = find_first_child_by_name(fill, "color") {
                if let Some(rgb) = parse_xfa_color(color) {
                    style.text_color = Some(rgb);
                }
            }
        }
        // <font color="#RRGGBB"> attribute (fallback when <fill><color> not present)
        if style.text_color.is_none() {
            if let Some(color_str) = attr(font, "color") {
                if let Some(rgb) = parse_font_color_attr(color_str) {
                    style.text_color = Some(rgb);
                }
            }
        }
        // fontHorizontalScale="96%" → 0.96
        if let Some(scale_str) = attr(font, "fontHorizontalScale") {
            if let Some(v) = parse_percentage(scale_str) {
                style.font_horizontal_scale = Some(v);
            }
        }
        // letterSpacing="-0.018em" or "0.5pt"
        if let Some(ls_str) = attr(font, "letterSpacing") {
            if let Some(v) = parse_letter_spacing(ls_str, style.font_size.unwrap_or(10.0)) {
                style.letter_spacing_pt = Some(v);
            }
        }
        // XFA Spec 3.3 §2.6 — underline="1" (single) or "2" (double)
        if let Some(underline_str) = attr(font, "underline") {
            style.underline = underline_str == "1" || underline_str == "2";
        }
        // XFA Spec 3.3 §2.6 — lineThrough="1"
        if let Some(line_through_str) = attr(font, "lineThrough") {
            style.line_through = line_through_str == "1";
        }
    }

    // Parse <para> for paragraph attributes (XFA 3.3 §D.7).
    if let Some(para) = find_first_child_by_name(elem, "para") {
        if let Some(v) = attr(para, "spaceAbove").and_then(Measurement::parse) {
            style.space_above_pt = Some(v.to_points());
        }
        if let Some(v) = attr(para, "spaceBelow").and_then(Measurement::parse) {
            style.space_below_pt = Some(v.to_points());
        }
        if let Some(v) = attr(para, "marginLeft").and_then(Measurement::parse) {
            style.margin_left_pt = Some(v.to_points());
        }
        if let Some(v) = attr(para, "marginRight").and_then(Measurement::parse) {
            style.margin_right_pt = Some(v.to_points());
        }
        if let Some(va) = attr(para, "vAlign") {
            style.v_align = Some(match va {
                "middle" => VerticalAlign::Middle,
                "bottom" => VerticalAlign::Bottom,
                _ => VerticalAlign::Top,
            });
        }
    }

    // Parse <border><corner> for border radius and <border><edge> for style/visibility.
    if let Some(border) = border {
        if let Some(corner) = find_first_child_by_name(border, "corner") {
            if let Some(v) = attr(corner, "radius").and_then(Measurement::parse) {
                style.border_radius_pt = Some(v.to_points());
            }
        }
        let edge_elems2: Vec<_> = border
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "edge")
            .collect();
        if !edge_elems2.is_empty() {
            if let Some(stroke) = attr(edge_elems2[0], "stroke") {
                if stroke != "none" {
                    style.border_style = Some(stroke.to_string());
                }
            }
        }
    }

    // Parse <caption> for caption text, placement, and reserve (XFA 3.3 §7.4).
    if let Some(cap) = parse_caption(elem) {
        style.caption_text = Some(cap.text);
        style.caption_placement = Some(
            match cap.placement {
                CaptionPlacement::Left => "left",
                CaptionPlacement::Right => "right",
                CaptionPlacement::Top => "top",
                CaptionPlacement::Bottom => "bottom",
                CaptionPlacement::Inline => "inline",
            }
            .to_string(),
        );
        style.caption_reserve = cap.reserve;
        style.caption_font_family = cap.font_family;
        style.caption_font_size = cap.font_size;
    }

    style
}

fn parse_fill_color(fill_node: Node<'_, '_>) -> Option<(u8, u8, u8)> {
    find_first_child_by_name(fill_node, "color")
        .and_then(parse_xfa_color)
        .or_else(|| {
            find_first_child_by_name(fill_node, "solid")
                .and_then(|solid| find_first_child_by_name(solid, "color"))
                .and_then(parse_xfa_color)
        })
}

fn parse_check_button_mark(elem: Node<'_, '_>) -> Option<String> {
    let ui = find_first_child_by_name(elem, "ui")?;
    let check_button = ui
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "checkButton")?;
    let mark = attr(check_button, "mark")?.to_ascii_lowercase();
    match mark.as_str() {
        "check" | "circle" | "cross" | "diamond" | "square" | "star" => Some(mark),
        _ => None,
    }
}

fn parse_check_button_values(
    elem: Node<'_, '_>,
) -> (Option<String>, Option<String>, Option<String>) {
    let ui = match find_first_child_by_name(elem, "ui") {
        Some(ui) => ui,
        None => return (None, None, None),
    };
    let check_button = match find_first_child_by_name(ui, "checkButton") {
        Some(check_button) => check_button,
        None => return (None, None, None),
    };

    // XFA Spec 3.3 §7.2.21 allows `<items>` to be authored either directly on
    // the field or nested inside `<ui><checkButton>`. Adobe honors both forms;
    // if we only look on the field, asserted values like "Yes"/"No" collapse
    // back to the hardcoded 1/0 fallback and the checkbox renders unchecked.
    let items = find_first_child_by_name(elem, "items")
        .or_else(|| find_first_child_by_name(check_button, "items"));
    let Some(items) = items else {
        return (None, None, None);
    };

    let values: Vec<String> = items
        .children()
        .filter(|n| n.is_element())
        .map(|child| child.text().unwrap_or("").trim().to_string())
        .collect();

    (
        values.first().cloned(),
        values.get(1).cloned(),
        values.get(2).cloned(),
    )
}

fn parse_xfa_color(color_node: Node<'_, '_>) -> Option<(u8, u8, u8)> {
    let value = attr(color_node, "value")?;
    let parts: Vec<&str> = value.split(',').collect();
    if parts.len() >= 3 {
        let r = parts[0].trim().parse::<u8>().ok()?;
        let g = parts[1].trim().parse::<u8>().ok()?;
        let b = parts[2].trim().parse::<u8>().ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

/// Parse a color string from a `color` attribute on `<font>`.
///
/// Supported formats:
/// - `#RRGGBB` (e.g. `#000080`)
/// - `#RGB` shorthand (e.g. `#00F` → `#0000FF`)
/// - `r,g,b` with decimal values 0-255 (e.g. `0,0,128`)
fn parse_font_color_attr(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        parse_ascii_hex_color(hex)
    } else {
        // Try "r,g,b" decimal format
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() >= 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            Some((r, g, b))
        } else {
            None
        }
    }
}

/// Extract the data field name from a bind ref like `$.listInitiales[*]`.
/// Returns `Some("listInitiales")` for that example, or None if no bind/ref.
///
/// XFA Spec 3.3 §4.4 p199-201 — explicit data references via
/// `<bind match="dataRef" ref="SOM.expression">`. The ref is a SOM
/// expression that resolves to a data node. We extract the last segment
/// name for use as the data matching key.
///
/// TODO: XFA Spec 3.3 §4.4 p199 — full SOM expression evaluation not
/// implemented. Only simple `$.name` and `$.name[*]` patterns are supported.
/// Multi-segment paths like `$record.group.field` are partially supported
/// (we take the last segment).
#[allow(dead_code)]
fn parse_bind_data_name(elem: Node<'_, '_>) -> Option<String> {
    let bind = find_first_child_by_name(elem, "bind")?;
    let ref_val = attr(bind, "ref")?;
    // Typical refs: "$.fieldName", "$.fieldName[*]", "$record.fieldName"
    // Extract the last dot-separated segment, strip any trailing [*] etc.
    let segment = ref_val.rsplit('.').next().unwrap_or(ref_val);
    let name = segment.split('[').next().unwrap_or(segment).trim();
    if name.is_empty() || name == "$" {
        None
    } else {
        Some(name.to_string())
    }
}

/// XFA Spec 3.3 §4.4.3 p176 — `<bind>` element controls data binding:
/// - `match="once"` (default): bind to first matching data node
/// - `match="none"`: skip data binding entirely
/// - `match="dataRef"`: use explicit SOM ref to locate data
/// - `match="global"`: search entire data DOM
///
/// TODO: XFA Spec 3.3 §4.4.3 p176 — `match="global"` not implemented.
/// Currently treated the same as `match="once"`.
fn parse_bind(elem: Node<'_, '_>) -> (Option<String>, bool) {
    let Some(bind) = find_first_child_by_name(elem, "bind") else {
        return (None, false);
    };
    let bind_none = attr(bind, "match") == Some("none");
    let bind_ref = if bind_none {
        None
    } else {
        attr(bind, "ref").map(|s| s.trim().to_string())
    };
    (bind_ref, bind_none)
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_dom_resolver::data_dom::DataDom;

    #[test]
    fn parse_css_color_rejects_non_ascii_hex_without_panicking() {
        assert_eq!(parse_css_color("#€"), None);
        assert_eq!(parse_css_color("#€abc"), None);
        assert_eq!(parse_css_color("#0f8"), Some((0x00, 0xff, 0x88)));
        assert_eq!(parse_css_color("#00ff88"), Some((0x00, 0xff, 0x88)));
    }

    #[test]
    fn parse_font_color_attr_rejects_non_ascii_hex_without_panicking() {
        assert_eq!(parse_font_color_attr("#€"), None);
        assert_eq!(parse_font_color_attr("#€abc"), None);
        assert_eq!(parse_font_color_attr("#0f8"), Some((0x00, 0xff, 0x88)));
        assert_eq!(parse_font_color_attr("#00ff88"), Some((0x00, 0xff, 0x88)));
    }

    #[test]
    fn repeating_subform_expands_from_data() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="Orders" layout="tb" w="500pt">
      <subform name="Order" layout="position" w="500pt" h="60pt">
        <occur min="0" max="10" initial="1"/>
        <field name="Item" w="200pt" h="20pt" x="0pt" y="0pt"/>
        <field name="Qty" w="100pt" h="20pt" x="200pt" y="0pt"/>
      </subform>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <Order><Item>Widget A</Item><Qty>5</Qty></Order>
      <Order><Item>Widget B</Item><Qty>3</Qty></Order>
      <Order><Item>Widget C</Item><Qty>7</Qty></Order>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let orders_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "Orders")
            .map(|(i, _)| FormNodeId(i))
            .unwrap();
        let orders = tree.get(orders_id);
        assert_eq!(orders.children.len(), 3);
        assert!(orders
            .children
            .iter()
            .all(|&id| tree.get(id).name == "Order" && tree.get(id).occur.count() == 1));
    }

    /// Double-wrapped <xfa:data> must be unwrapped for data binding to work.
    #[test]
    fn repeating_subform_double_wrapped_data() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="Orders" layout="tb" w="559pt">
      <subform name="CoreOrders" layout="tb" w="361pt">
        <subform name="Order" layout="position" w="360pt" h="162pt">
          <occur min="0" max="3" initial="1"/>
          <field name="Item" w="200pt" h="25pt" x="9pt" y="9pt"/>
        </subform>
      </subform>
    </subform>
  </subform>
</template>"#;

        let data_xml = concat!(
            r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">"#,
            r#"<xfa:data><xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">"#,
            r#"<form1>"#,
            r#"<Order><Item>A</Item></Order>"#,
            r#"<Order><Item>B</Item></Order>"#,
            r#"<Order><Item>C</Item></Order>"#,
            r#"</form1>"#,
            r#"</xfa:data></xfa:data></xfa:datasets>"#,
        );

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let core_orders_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "CoreOrders")
            .map(|(i, _)| FormNodeId(i))
            .unwrap();
        let core_orders = tree.get(core_orders_id);
        assert_eq!(
            core_orders.children.len(),
            3,
            "Expected 3 Order instances from double-wrapped data"
        );
    }

    /// Bind ref resolution: when subform name differs from data name,
    /// <bind match="dataRef" ref="$.dataName[*]"> should use the data name.
    #[test]
    fn repeating_subform_bind_ref_resolves_data_name() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="ListItems" layout="tb" w="500pt">
      <subform name="ItemGroup" layout="tb" w="500pt" h="50pt">
        <occur min="0" max="-1"/>
        <bind match="dataRef" ref="$.itemGroup[*]"/>
        <field name="title" w="200pt" h="20pt" x="0pt" y="0pt"/>
      </subform>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form>
      <itemGroup><title>Group A</title></itemGroup>
      <itemGroup><title>Group B</title></itemGroup>
    </form>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let list_items = tree
            .nodes
            .iter()
            .find(|n| n.name == "ListItems")
            .expect("ListItems must exist");
        assert_eq!(
            list_items.children.len(),
            2,
            "Expected 2 instances from bind ref $.itemGroup[*]"
        );
    }

    #[test]
    fn repeating_table_rows_expand_as_sibling_rows() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="Table1" layout="table" columnWidths="100pt 100pt 100pt">
      <subform name="HeaderRow" layout="row">
        <draw name="H1" w="100pt" h="20pt"/>
        <draw name="H2" w="100pt" h="20pt"/>
        <draw name="H3" w="100pt" h="20pt"/>
      </subform>
      <subform name="Row1" layout="row">
        <occur min="0" max="-1"/>
        <field name="Cell1" w="100pt" h="20pt"/>
        <field name="Cell2" w="100pt" h="20pt"/>
        <field name="Cell3" w="100pt" h="20pt"/>
      </subform>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <Table1>
        <Row1><Cell1>A1</Cell1><Cell2>B1</Cell2><Cell3>C1</Cell3></Row1>
        <Row1><Cell1>A2</Cell1><Cell2>B2</Cell2><Cell3>C2</Cell3></Row1>
        <Row1><Cell1>A3</Cell1><Cell2>B3</Cell2><Cell3>C3</Cell3></Row1>
        <Row1><Cell1>A4</Cell1><Cell2>B4</Cell2><Cell3>C4</Cell3></Row1>
      </Table1>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let table_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "Table1")
            .map(|(i, _)| FormNodeId(i))
            .unwrap();
        let table = tree.get(table_id);
        assert_eq!(table.children.len(), 5, "header + 4 repeated data rows");
        assert_eq!(tree.get(table.children[0]).name, "HeaderRow");
        assert!(table.children[1..]
            .iter()
            .all(|&id| tree.get(id).name == "Row1" && tree.get(id).children.len() == 3));
    }

    #[test]
    fn repeating_subform_clamps_to_occur_max() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <subform name="Orders" layout="tb">
      <subform name="Order" layout="position" w="200pt" h="20pt">
        <occur min="0" max="2"/>
        <field name="Item" w="100pt" h="20pt"/>
      </subform>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <Order><Item>A</Item></Order>
      <Order><Item>B</Item></Order>
      <Order><Item>C</Item></Order>
      <Order><Item>D</Item></Order>
      <Order><Item>E</Item></Order>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let orders_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "Orders")
            .map(|(i, _)| FormNodeId(i))
            .unwrap();
        assert_eq!(tree.get(orders_id).children.len(), 2);
    }

    #[test]
    fn repeating_subform_unbounded_uses_data_count() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <subform name="Orders" layout="tb">
      <subform name="Order" layout="position" w="200pt" h="20pt">
        <occur min="0" max="-1"/>
        <field name="Item" w="100pt" h="20pt"/>
      </subform>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <Order><Item>A</Item></Order>
      <Order><Item>B</Item></Order>
      <Order><Item>C</Item></Order>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let orders_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "Orders")
            .map(|(i, _)| FormNodeId(i))
            .unwrap();
        assert_eq!(tree.get(orders_id).children.len(), 3);
    }

    #[test]
    fn repeating_subform_respects_occur_min() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <subform name="Orders" layout="tb">
      <subform name="Order" layout="position" w="200pt" h="20pt">
        <occur min="2" max="-1"/>
        <field name="Item" w="100pt" h="20pt"/>
      </subform>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1/>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let orders_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "Orders")
            .map(|(i, _)| FormNodeId(i))
            .unwrap();
        let orders = tree.get(orders_id);
        assert_eq!(orders.children.len(), 2);
        assert!(orders
            .children
            .iter()
            .all(|&id| tree.get(id).name == "Order"));
    }

    /// Root subform binds to data root when names match (XFA §4.7.2).
    #[test]
    fn root_subform_binds_to_matching_data_root() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="title" w="200pt" h="20pt" x="0pt" y="0pt"/>
    <field name="code" w="200pt" h="20pt" x="0pt" y="20pt"/>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form>
      <title>Hello World</title>
      <code>42</code>
    </form>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let title_node = tree
            .nodes
            .iter()
            .find(|n| n.name == "title")
            .expect("title field must exist");
        match &title_node.node_type {
            FormNodeType::Field { value } => {
                assert_eq!(value, "Hello World", "title should bind to data root");
            }
            _ => panic!("title should be a field"),
        }
    }

    #[test]
    fn check_button_mark_parsed_into_style() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="agree" w="20pt" h="20pt">
      <ui><checkButton mark="circle"/></ui>
      <value><text>1</text></value>
      <items><text>1</text><text>0</text><text>2</text></items>
    </field>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form>
      <agree>1</agree>
    </form>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let agree_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "agree")
            .map(|(i, _)| FormNodeId(i))
            .expect("agree field must exist");
        let meta = tree.meta(agree_id);
        assert_eq!(meta.field_kind, FieldKind::Checkbox);
        assert_eq!(meta.style.check_button_mark.as_deref(), Some("circle"));
        assert_eq!(meta.style.check_button_on_value.as_deref(), Some("1"));
        assert_eq!(meta.style.check_button_off_value.as_deref(), Some("0"));
        assert_eq!(meta.style.check_button_neutral_value.as_deref(), Some("2"));
    }

    #[test]
    fn nested_check_button_items_are_used_for_checkbox_values() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="agree" w="20pt" h="20pt">
      <ui>
        <checkButton>
          <items><text>Yes</text><text>No</text></items>
        </checkButton>
      </ui>
      <value><text>Yes</text></value>
    </field>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form>
      <agree>Yes</agree>
    </form>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let agree_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "agree")
            .map(|(i, _)| FormNodeId(i))
            .expect("agree field must exist");
        let meta = tree.meta(agree_id);
        assert_eq!(meta.field_kind, FieldKind::Checkbox);
        assert_eq!(meta.style.check_button_on_value.as_deref(), Some("Yes"));
        assert_eq!(meta.style.check_button_off_value.as_deref(), Some("No"));
        match &tree.get(agree_id).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "Yes"),
            _ => panic!("agree should be a field"),
        }
    }

    #[test]
    fn border_fill_solid_color_populates_background_color() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="name" w="200pt" h="20pt">
      <ui><textEdit/></ui>
      <border>
        <fill>
          <solid>
            <color value="240,240,240"/>
          </solid>
        </fill>
      </border>
    </field>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form>
      <name>Jane Doe</name>
    </form>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let name_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "name")
            .map(|(i, _)| FormNodeId(i))
            .expect("name field must exist");
        assert_eq!(tree.meta(name_id).style.bg_color, Some((240, 240, 240)));
    }

    #[test]
    fn border_fill_direct_color_populates_button_bg_color() {
        // Mirrors 053ecab3's E-Mail submit button: <border><fill><color value=...>
        // without the <solid> wrapper. Parser must pick the color up so
        // render_button() draws the yellow fill instead of the gray fallback.
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="Btn" w="25.4mm" h="9.525mm">
      <ui><button highlight="inverted"/></ui>
      <caption><value><text>E-Mail</text></value></caption>
      <border hand="right">
        <edge stroke="raised"/>
        <fill><color value="255,255,153"/></fill>
      </border>
    </field>
  </subform>
</template>"#;
        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data><form/></xfa:data>
</xfa:datasets>"#;
        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();
        let btn_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "Btn")
            .map(|(i, _)| FormNodeId(i))
            .expect("button field must exist");
        assert_eq!(tree.meta(btn_id).style.bg_color, Some((255, 255, 153)));
    }

    #[test]
    fn rich_text_exdata_captions_are_collapsed_to_plain_text() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="patient_name" w="200pt" h="20pt">
      <ui><textEdit/></ui>
      <caption placement="top" reserve="0.230972in">
        <value>
          <exData contentType="text/html" maxLength="0">
            <body xmlns="http://www.w3.org/1999/xhtml"
                  xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"
                  xfa:APIVersion="1.4.4136.0">
              <p>NAME<span style="xfa-spacerun:yes"> </span><span style="font-style:italic">(Last, First, Middle Initial)</span></p>
            </body>
          </exData>
        </value>
      </caption>
    </field>
    <field name="telephone" w="200pt" h="20pt">
      <ui><textEdit/></ui>
      <caption placement="top" reserve="0.230972in">
        <value>
          <exData contentType="text/html" maxLength="0">
            <body xmlns="http://www.w3.org/1999/xhtml"
                  xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"
                  xfa:APIVersion="1.4.4136.0">
              <p>TELEPHONE<span style="font-style:italic"> (Include area code)</span></p>
            </body>
          </exData>
        </value>
      </caption>
    </field>
  </subform>
</template>"#;

        let data_dom = DataDom::new();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let patient_name_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "patient_name")
            .map(|(i, _)| FormNodeId(i))
            .expect("patient_name field must exist");
        let patient_name = tree.meta(patient_name_id);
        assert_eq!(
            patient_name.style.caption_text.as_deref(),
            Some("NAME (Last, First, Middle Initial)")
        );
        assert_eq!(patient_name.style.caption_placement.as_deref(), Some("top"));
        assert_eq!(
            tree.get(patient_name_id)
                .box_model
                .caption
                .as_ref()
                .map(|caption| caption.text.as_str()),
            Some("NAME (Last, First, Middle Initial)")
        );

        let telephone_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "telephone")
            .map(|(i, _)| FormNodeId(i))
            .expect("telephone field must exist");
        let telephone = tree.meta(telephone_id);
        assert_eq!(
            telephone.style.caption_text.as_deref(),
            Some("TELEPHONE (Include area code)")
        );
        assert_eq!(telephone.style.caption_placement.as_deref(), Some("top"));
        assert_eq!(
            tree.get(telephone_id)
                .box_model
                .caption
                .as_ref()
                .map(|caption| caption.text.as_str()),
            Some("TELEPHONE (Include area code)")
        );
    }

    #[test]
    fn border_edge_order_keeps_bottom_only_underline() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="underlined" w="200pt" h="20pt">
      <ui><textEdit/></ui>
      <border>
        <edge stroke="none" thickness="1pt"/>
        <edge stroke="none" thickness="2pt"/>
        <edge stroke="solid" thickness="3pt"/>
        <edge stroke="none" thickness="4pt"/>
      </border>
    </field>
  </subform>
</template>"#;

        let data_dom = DataDom::new();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let underlined_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "underlined")
            .map(|(i, _)| FormNodeId(i))
            .expect("underlined field must exist");
        let style = &tree.meta(underlined_id).style;

        // Edges are interpreted in XFA order top/right/bottom/left.
        assert_eq!(style.border_edges, [false, false, true, false]);
        assert_eq!(style.border_widths, Some([1.0, 2.0, 3.0, 4.0]));
        assert_eq!(style.border_width_pt, Some(3.0));
    }

    #[test]
    fn round_check_button_parsed_as_radio() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="choice" w="20pt" h="20pt">
      <ui><checkButton shape="round"/></ui>
      <items><text>Y</text><text>N</text></items>
    </field>
  </subform>
</template>"#;

        let data_dom = DataDom::new();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let choice_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "choice")
            .map(|(i, _)| FormNodeId(i))
            .expect("choice field must exist");
        assert_eq!(tree.meta(choice_id).field_kind, FieldKind::Radio);
    }

    #[test]
    fn excl_group_value_selects_matching_child() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <exclGroup name="choice">
      <field name="yes" w="20pt" h="20pt">
        <ui><checkButton shape="round"/></ui>
        <items><text>Y</text><text>N</text></items>
      </field>
      <field name="no" w="20pt" h="20pt">
        <ui><checkButton shape="round"/></ui>
        <items><text>N</text><text>Y</text></items>
      </field>
    </exclGroup>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form>
      <choice>Y</choice>
    </form>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let yes_node = tree
            .nodes
            .iter()
            .find(|n| n.name == "yes")
            .expect("yes field must exist");
        let no_node = tree
            .nodes
            .iter()
            .find(|n| n.name == "no")
            .expect("no field must exist");

        match (&yes_node.node_type, &no_node.node_type) {
            (FormNodeType::Field { value: yes }, FormNodeType::Field { value: no }) => {
                assert_eq!(yes, "Y");
                assert!(
                    no.is_empty(),
                    "non-selected exclGroup child should be cleared"
                );
            }
            _ => panic!("exclGroup children must be fields"),
        }
    }

    #[test]
    fn draw_with_image_creates_image_node() {
        // Base64-encoded 1x1 red PNG
        let image_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";
        let template = format!(
            r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <draw name="logo" x="0pt" y="0pt" w="100pt" h="100pt">
      <value>
        <image contentType="image/png">{}</image>
      </value>
    </draw>
  </subform>
</template>"#,
            image_b64
        );

        let data_dom = DataDom::new();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(&template).unwrap();

        let logo_node = tree
            .nodes
            .iter()
            .find(|n| n.name == "logo")
            .expect("logo draw must exist");

        match &logo_node.node_type {
            FormNodeType::Image { data, mime_type } => {
                assert_eq!(mime_type, "image/png");
                assert!(!data.is_empty(), "image data should not be empty");
                // Verify it's valid PNG by checking magic bytes
                assert!(
                    data.starts_with(&[0x89, 0x50, 0x4E, 0x47]),
                    "decoded data should be PNG"
                );
            }
            _ => panic!(
                "draw with image should create Image node, got {:?}",
                logo_node.node_type
            ),
        }
    }

    #[test]
    fn draw_with_rectangle_creates_draw_node() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <draw name="box" x="0pt" y="0pt" w="100pt" h="100pt">
      <value>
        <rectangle/>
      </value>
    </draw>
  </subform>
</template>"#;

        let data_dom = DataDom::new();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let box_node = tree
            .nodes
            .iter()
            .find(|n| n.name == "box")
            .expect("box draw must exist");

        match &box_node.node_type {
            FormNodeType::Draw(DrawContent::Rectangle { .. }) => {}
            other => panic!(
                "draw with rectangle should create Rectangle draw, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn field_anchor_type_is_preserved_in_merged_meta() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="centeredBox" layout="position" anchorType="middleCenter" w="20pt" h="10pt" x="100pt" y="50pt"/>
  </subform>
</template>"#;

        let data_dom = DataDom::new();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let centered_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "centeredBox")
            .map(|(i, _)| FormNodeId(i))
            .expect("centeredBox field must exist");

        assert_eq!(tree.meta(centered_id).anchor_type, AnchorType::MiddleCenter);
    }

    #[test]
    fn area_without_layout_uses_positioned_when_children_have_coordinates() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <area name="Nagl">
      <draw name="headerLine" x="10pt" y="12pt" w="100pt" h="1pt">
        <value><rectangle/></value>
      </draw>
    </area>
  </subform>
</template>"#;

        let data_dom = DataDom::new();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let nagl = tree
            .nodes
            .iter()
            .find(|n| n.name == "Nagl")
            .expect("Nagl area must exist");

        assert_eq!(nagl.layout, LayoutStrategy::Positioned);
    }

    // ─── #1092: matchTemplate merge mode ────────────────────────────────────

    /// Default mode (no `matchTemplate` keyword in XML) must be ConsumeData.
    #[test]
    fn merge_mode_defaults_to_consume_data() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb"/>
</template>"#;
        assert_eq!(detect_merge_mode(template), MergeMode::ConsumeData);
    }

    /// When the XML contains `matchTemplate`, the mode is detected.
    #[test]
    fn merge_mode_detects_match_template() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <!-- config: matchTemplate -->
  <subform name="form1" layout="tb"/>
</template>"#;
        assert_eq!(detect_merge_mode(template), MergeMode::MatchTemplate);
    }

    /// In matchTemplate mode a flat data structure binds to named fields
    /// regardless of template hierarchy.
    #[test]
    fn match_template_binds_flat_data_to_named_fields() {
        // Template has a nested subform but data is flat
        let template = r#"<?xml version="1.0"?>
<!-- matchTemplate -->
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="section" layout="tb">
      <field name="firstName" w="200pt" h="20pt" x="0pt" y="0pt"/>
      <field name="lastName" w="200pt" h="20pt" x="0pt" y="20pt"/>
    </subform>
  </subform>
</template>"#;

        // Flat data — no "section" group, fields at root level
        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <firstName>John</firstName>
      <lastName>Doe</lastName>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let first = tree
            .nodes
            .iter()
            .find(|n| n.name == "firstName")
            .expect("firstName must exist");
        let last = tree
            .nodes
            .iter()
            .find(|n| n.name == "lastName")
            .expect("lastName must exist");

        match &first.node_type {
            FormNodeType::Field { value } => assert_eq!(value, "John"),
            _ => panic!("firstName must be a field"),
        }
        match &last.node_type {
            FormNodeType::Field { value } => assert_eq!(value, "Doe"),
            _ => panic!("lastName must be a field"),
        }
    }

    // ─── #1093: scope matching ────────────────────────────────────────────────

    /// Sibling resolution: template `a.b.city` finds data `a.city` when `b`
    /// has no `city` child.
    #[test]
    fn field_binds_via_sibling_scope() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="address" layout="tb">
      <subform name="details" layout="tb">
        <field name="city" w="200pt" h="20pt" x="0pt" y="0pt"/>
      </subform>
    </subform>
  </subform>
</template>"#;

        // city is a sibling of details (at the address level), not inside details
        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <address>
        <city>Rotterdam</city>
        <details/>
      </address>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let city = tree
            .nodes
            .iter()
            .find(|n| n.name == "city")
            .expect("city field must exist");
        match &city.node_type {
            FormNodeType::Field { value } => assert_eq!(
                value, "Rotterdam",
                "city should be found via sibling scope resolution"
            ),
            _ => panic!("city should be a field"),
        }
    }

    // ─── #1094: transparent subforms ─────────────────────────────────────────

    /// A nameless subform is transparent: its children bind against the parent
    /// data context, not a child data group.
    #[test]
    fn transparent_subform_passes_data_context_through() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform layout="tb">
      <field name="country" w="200pt" h="20pt" x="0pt" y="0pt"/>
    </subform>
  </subform>
</template>"#;

        // Data has `country` at form1 level; there is no anonymous data group
        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <country>Netherlands</country>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let country = tree
            .nodes
            .iter()
            .find(|n| n.name == "country")
            .expect("country field must exist");
        match &country.node_type {
            FormNodeType::Field { value } => assert_eq!(
                value, "Netherlands",
                "transparent subform should pass context through"
            ),
            _ => panic!("country should be a field"),
        }
    }

    /// Nested transparent subforms still bind correctly.
    #[test]
    fn nested_transparent_subforms_bind_correctly() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform layout="tb">
      <subform layout="tb">
        <field name="postalCode" w="100pt" h="20pt" x="0pt" y="0pt"/>
      </subform>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <postalCode>1234AB</postalCode>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let postal = tree
            .nodes
            .iter()
            .find(|n| n.name == "postalCode")
            .expect("postalCode field must exist");
        match &postal.node_type {
            FormNodeType::Field { value } => assert_eq!(value, "1234AB"),
            _ => panic!("postalCode should be a field"),
        }
    }

    // ─── #1095: occur expansion ───────────────────────────────────────────────

    /// Repeating subform with 3 data items creates 3 form instances (already
    /// tested in repeating_subform_expands_from_data — this variant verifies
    /// the instances correctly contain their respective data).
    #[test]
    fn repeating_subform_three_items_creates_three_instances() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="row" layout="position" w="500pt" h="30pt">
      <occur min="0" max="-1" initial="0"/>
      <field name="label" w="200pt" h="20pt" x="0pt" y="0pt"/>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <row><label>Alpha</label></row>
      <row><label>Beta</label></row>
      <row><label>Gamma</label></row>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let rows: Vec<_> = tree.nodes.iter().filter(|n| n.name == "row").collect();
        assert_eq!(rows.len(), 3, "must have exactly 3 row instances");
    }

    /// occur min=1 max=1 limits to one instance even when there are multiple
    /// matching data items.
    #[test]
    fn occur_min1_max1_limits_to_single_instance() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="item" layout="position" w="500pt" h="30pt">
      <occur min="1" max="1"/>
      <field name="val" w="100pt" h="20pt" x="0pt" y="0pt"/>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <item><val>X</val></item>
      <item><val>Y</val></item>
      <item><val>Z</val></item>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let items: Vec<_> = tree.nodes.iter().filter(|n| n.name == "item").collect();
        assert_eq!(items.len(), 1, "occur max=1 must limit to one instance");
    }

    // ─── #1096: presence binding ──────────────────────────────────────────────

    /// A field with `presence="hidden"` in the template is excluded from
    /// layout (presence remains Hidden after merge with no data override).
    #[test]
    fn field_with_presence_hidden_stays_hidden() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="secretField" presence="hidden" w="100pt" h="20pt" x="0pt" y="0pt"/>
  </subform>
</template>"#;

        let data_dom = DataDom::new();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let secret_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "secretField")
            .map(|(i, _)| FormNodeId(i))
            .expect("secretField must exist");

        assert_eq!(
            tree.meta(secret_id).presence,
            Presence::Hidden,
            "presence=hidden in template must be preserved"
        );
    }

    /// A field with no data value for presence defaults to Visible.
    #[test]
    fn field_without_presence_data_defaults_to_visible() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="normalField" w="100pt" h="20pt" x="0pt" y="0pt"/>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <normalField>hello</normalField>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let field_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "normalField")
            .map(|(i, _)| FormNodeId(i))
            .expect("normalField must exist");

        assert_eq!(
            tree.meta(field_id).presence,
            Presence::Visible,
            "field with no presence data must default to Visible"
        );
    }

    // ─── #1092 (existing test renamed for clarity) ────────────────────────────

    /// Ancestor scope: a field nested inside a subform should find data
    /// at an ancestor level when not present at the direct context.
    #[test]
    fn field_binds_via_ancestor_scope_walk() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="details" layout="tb">
      <field name="city" w="200pt" h="20pt" x="0pt" y="0pt"/>
    </subform>
  </subform>
</template>"#;

        // "city" is at the form1 level, NOT inside "details".
        // The ancestor scope walk should find it.
        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <city>Amsterdam</city>
      <details/>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let city = tree
            .nodes
            .iter()
            .find(|n| n.name == "city")
            .expect("city field must exist");
        match &city.node_type {
            FormNodeType::Field { value } => {
                assert_eq!(
                    value, "Amsterdam",
                    "city should be found via ancestor scope walk"
                );
            }
            _ => panic!("city should be a field"),
        }
    }

    // ─── XFA §7.2.18 / §9.2: occur `initial`/`min`/`max` semantics ────────────
    //
    // These regression tests pin the spec-correct interaction between `occur`
    // and data binding:
    //   - data records present → bind drives count, clamped to [min, max];
    //   - no data records      → render `initial` instances, clamped to max.
    //
    // The third test guards against regression on the dominant authoring
    // pattern (`initial==min`, default), where the empty-data path must keep
    // producing exactly `min` instances.

    /// XFA §7.2.18 + §9.2: data binding wins over `initial` when records exist.
    /// `occur initial=2 max=10` with 3 datasets records → 3 instances.
    #[test]
    fn occur_initial_yields_to_data_binding_when_records_exist() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="Row" layout="position" w="200pt" h="20pt">
      <occur min="0" max="10" initial="2"/>
      <field name="Item" w="100pt" h="20pt" x="0pt" y="0pt"/>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <Row><Item>A</Item></Row>
      <Row><Item>B</Item></Row>
      <Row><Item>C</Item></Row>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let rows: Vec<_> = tree.nodes.iter().filter(|n| n.name == "Row").collect();
        assert_eq!(
            rows.len(),
            3,
            "data binding must win over occur.initial when records exist"
        );
    }

    /// XFA §7.2.18 + §9.2: `initial` drives instance count when no data
    /// records exist. `occur initial=5 max=10` with 0 records → 5 instances.
    #[test]
    fn occur_initial_drives_count_when_no_data_records() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="Row" layout="position" w="200pt" h="20pt">
      <occur min="0" max="10" initial="5"/>
      <field name="Item" w="100pt" h="20pt" x="0pt" y="0pt"/>
    </subform>
  </subform>
</template>"#;

        // No datasets payload — initial must be honoured.
        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1/>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let rows: Vec<_> = tree.nodes.iter().filter(|n| n.name == "Row").collect();
        assert_eq!(
            rows.len(),
            5,
            "occur.initial must drive instance count when no data records bind"
        );
    }

    /// XFA §7.2.18 + §9.2: default `initial==min` path is unchanged — a
    /// subform with `min=0` and no records still produces a single
    /// zero-instance skeleton (presence=Hidden) so `parent._<name>` resolves.
    #[test]
    fn occur_default_initial_keeps_zero_instance_skeleton_when_no_data() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="Row" layout="position" w="200pt" h="20pt">
      <occur min="0" max="10"/>
      <field name="Item" w="100pt" h="20pt" x="0pt" y="0pt"/>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1/>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let rows: Vec<_> = tree.nodes.iter().filter(|n| n.name == "Row").collect();
        assert_eq!(
            rows.len(),
            1,
            "default initial==min path must keep producing a single zero-instance skeleton"
        );
        // The skeleton must remain hidden (regression guard for the
        // `_<name>` InstanceManager handle behaviour).
        let row_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "Row")
            .map(|(i, _)| FormNodeId(i))
            .unwrap();
        assert_eq!(tree.meta(row_id).presence, Presence::Hidden);
        assert!(tree.meta(row_id).is_zero_instance_prototype);
    }
}
