//! Layout engine — positions form nodes into layout rectangles.
//!
//! Implements XFA 3.3 §4 (Box Model) and §8 (Layout for Growable Objects).
//! Supports positioned layout and flowed layout (tb, lr-tb, rl-tb).
//!
//! # XFA Spec 3.3 Chapter 8 — Layout for Growable Objects
//!
//! This module implements the core layout algorithm described in §8.6 (p288):
//! a content-driven single traversal of the Form DOM, adding layout nodes
//! to a Layout DOM as content is placed into containers. When a container
//! fills up, the engine traverses to a new container (next contentArea,
//! next pageArea, or a new page).
//!
//! ## Spec coverage status (reviewed 2026-04-19):
//!
//! - §8.1 Text Placement in Growable Containers: ✅ implemented (anchorType via Appendix A)
//! - §8.2 Flowing Layout (TB, LR-TB, RL-TB):    ✅ implemented
//! - §8.3 hAlign in various layouts:              ✅ hAlign on children in TB/LR-TB/RL-TB
//! - §8.4 Growable + Flowed interaction:          ✅ resize then reflow
//! - §8.5 Layout DOM structure:                   ✅ pages > nodes hierarchy
//! - §8.6 Layout Algorithm:                       ✅ content-driven traversal
//! - §8.7 Content Splitting:                      ✅ text-line splitting + container splitting
//! - §8.8 Pagination Strategies:                  ✅ orderedOccurrence (sequential by default)
//! - §8.9 Adhesion (keep):                        ✅ keep-chain look-ahead (keep.next/previous)
//! - §8.10 Leaders/Trailers:                      ✅ per-page leader/trailer; ⚠️ overflow/bookend
//! - §8.11 Tables:                                ✅ columnWidths, colSpan, row equalization
//! - Appendix A: Coordinate algorithms:           ✅ anchorType (all 9 variants)
//! - Appendix B: Layout Objects:                  ✅ area, exclGroup, subformSet

use crate::error::Result;
use crate::form::{
    ContentArea, DrawContent, FieldKind, FormNode, FormNodeId, FormNodeMeta, FormNodeType, FormTree,
};
use crate::text::{self, FontFamily};
use crate::types::{LayoutStrategy, Rect, Size, TextAlign};
use std::sync::{Mutex, OnceLock};

/// Resolve the display value for a field.
///
/// - **Dropdown**: if the field has save-items and the current value matches
///   one of them, return the corresponding display-item.
/// - **NumericEdit**: strip unnecessary trailing zeros from float strings
///   (e.g. "1.00000000" → "1", "3.50" → "3.5").
/// - **DateTimePicker**: if the raw value starts with an ISO date prefix,
///   collapse it to `YYYY-MM-DD`.
/// - Otherwise return the value as-is.
fn resolve_display_value<'a>(value: &'a str, meta: &'a FormNodeMeta) -> std::borrow::Cow<'a, str> {
    if value.is_empty() {
        return std::borrow::Cow::Borrowed(value);
    }
    // Dropdown: resolve save-item → display-item.
    if meta.field_kind == FieldKind::Dropdown {
        if !meta.save_items.is_empty() {
            if let Some(idx) = meta.save_items.iter().position(|s| s == value) {
                if let Some(display) = meta.display_items.get(idx) {
                    return std::borrow::Cow::Borrowed(display.as_str());
                }
            }
        }
        return std::borrow::Cow::Borrowed(value);
    }
    // NumericEdit: format raw float values by stripping trailing zeros.
    if meta.field_kind == FieldKind::NumericEdit {
        if let Ok(num) = value.parse::<f64>() {
            // Format with enough precision, then strip trailing zeros.
            let formatted = format!("{}", num);
            return std::borrow::Cow::Owned(formatted);
        }
    }
    if meta.field_kind == FieldKind::DateTimePicker {
        if let Some(date) = extract_iso_date_prefix(value) {
            return std::borrow::Cow::Owned(date.to_string());
        }
    }
    std::borrow::Cow::Borrowed(value)
}

fn extract_iso_date_prefix(value: &str) -> Option<&str> {
    let prefix = value.get(0..10)?;
    let bytes = prefix.as_bytes();
    if bytes.len() != 10
        || !bytes[0..4].iter().all(u8::is_ascii_digit)
        || bytes[4] != b'-'
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || bytes[7] != b'-'
        || !bytes[8..10].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    Some(prefix)
}

/// A unique identifier for a layout node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutNodeId(pub usize);

/// The output of the layout engine: positioned rectangles on pages.
#[derive(Debug)]
pub struct LayoutDom {
    /// Laid out pages.
    pub pages: Vec<LayoutPage>,
}

/// Per-page pagination diagnostics collected only on opt-in code paths.
#[derive(Debug, Clone, Default)]
pub struct LayoutProfile {
    /// Per-page profile data.
    pub pages: Vec<LayoutProfilePage>,
}

/// Minimal vertical-space profiling metadata for one laid out page.
#[derive(Debug, Clone)]
pub struct LayoutProfilePage {
    /// Page height.
    pub page_height: f64,
    /// Used height.
    pub used_height: f64,
    /// Whether content overflowed to the next page.
    pub overflow_to_next: bool,
    /// First element that overflowed.
    pub first_overflow_element: Option<String>,
}

impl LayoutDom {
    /// Estimate the total heap bytes consumed by this layout tree.
    ///
    /// Walks every page and every node recursively, summing the heap
    /// contributions of `Vec` fields (children, display_items, save_items)
    /// and inline `String` fields (name, content strings).  The estimate is
    /// a lower bound — it does not account for `Vec` capacity overheads or
    /// internal allocator padding.
    ///
    /// Primary use: memory-usage regression tests and profiling dashboards.
    pub fn estimated_heap_bytes(&self) -> usize {
        fn node_bytes(n: &LayoutNode) -> usize {
            // Name string heap allocation
            let mut total = n.name.len();
            // Children vec + each child
            total += n.children.capacity() * std::mem::size_of::<LayoutNode>();
            for child in &n.children {
                total += node_bytes(child);
            }
            // display_items / save_items
            for s in &n.display_items {
                total += s.len();
            }
            for s in &n.save_items {
                total += s.len();
            }
            // Content strings
            total += match &n.content {
                LayoutContent::None => 0,
                LayoutContent::Text(t) => t.len(),
                LayoutContent::Field { value, .. } => value.len(),
                LayoutContent::WrappedText { lines, .. } => {
                    lines.iter().map(|l| l.len()).sum::<usize>()
                }
                LayoutContent::Image { data, mime_type } => data.len() + mime_type.len(),
                LayoutContent::Draw(_) => 0,
            };
            total
        }

        let mut total = self.pages.capacity() * std::mem::size_of::<LayoutPage>();
        for page in &self.pages {
            total += page.nodes.capacity() * std::mem::size_of::<LayoutNode>();
            for node in &page.nodes {
                total += node_bytes(node);
            }
        }
        total
    }
}

/// Absolute maximum number of pages to prevent pagination explosion.
/// Used as a hard upper bound; the dynamic limit from
/// `estimate_page_limit` is preferred. (#729, #764)
///
/// XFA Spec 3.3 §9.3 "Layout for Dynamic Forms" (p357): the layout
/// processor repeats page templates as needed for overflow content.
/// The spec sets no explicit limit; this is our safety cap.
const MAX_PAGES: usize = 500;

/// A single page in the layout output.
#[derive(Debug, Default)]
pub struct LayoutPage {
    /// Page width.
    pub width: f64,
    /// Page height.
    pub height: f64,
    /// Layout nodes on this page.
    pub nodes: Vec<LayoutNode>,
    /// True when this page was emitted onto a pageArea that the XFA runtime
    /// recorded in the form-DOM packet (XFA 3.3 §8.6 / §3.1).  Downstream
    /// pipelines must NOT drop such pages on data-empty heuristics — the
    /// runtime already committed to emitting them.
    pub runtime_instantiated: bool,
}

/// A positioned element on a page.
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// The form node this layout node represents.
    pub form_node: FormNodeId,
    /// Bounding rectangle in page coordinates (points).
    pub rect: Rect,
    /// The node's display name (for debugging).
    pub name: String,
    /// Content for leaf nodes.
    pub content: LayoutContent,
    /// Children laid out within this node.
    pub children: Vec<LayoutNode>,
    /// Per-node visual style (colors, borders) from the XFA template.
    pub style: crate::form::FormNodeStyle,
    /// Display items for choice list fields (XFA 3.3 §7.7).
    pub display_items: Vec<String>,
    /// Save items for choice list fields (XFA 3.3 §7.7).
    pub save_items: Vec<String>,
}

/// Content type for layout leaf nodes.
#[derive(Debug, Clone)]
pub enum LayoutContent {
    /// No content.
    None,
    /// Text content.
    Text(String),
    /// Field content.
    Field {
        /// Field value.
        value: String,
        /// Field kind.
        field_kind: crate::form::FieldKind,
        /// Font size.
        font_size: f64,
        /// Font family.
        font_family: FontFamily,
    },
    /// Pre-wrapped text lines for rendering.
    WrappedText {
        /// Wrapped text lines.
        lines: Vec<String>,
        /// Per-line flag: `true` when the line is the first line of a paragraph.
        first_line_of_para: Vec<bool>,
        /// Font size.
        font_size: f64,
        /// Horizontal text alignment (from XFA `<para hAlign>`).
        text_align: TextAlign,
        /// Font family for selecting the correct PDF font resource.
        font_family: FontFamily,
        /// Additional space above the first line of text (from XFA `<para spaceAbove>`).
        space_above_pt: Option<f64>,
        /// Additional space below the last line of text (from XFA `<para spaceBelow>`).
        space_below_pt: Option<f64>,
        /// True when this text originates from a field value (not a draw element).
        from_field: bool,
    },
    /// A static image.
    Image {
        /// Image data.
        data: Vec<u8>,
        /// Image MIME type.
        mime_type: String,
    },
    /// A static draw element (line, rectangle, arc, text).
    Draw(DrawContent),
}

/// A content node queued for pagination, carrying page-break flags.
#[derive(Debug, Clone)]
struct QueuedNode {
    id: FormNodeId,
    break_before: bool,
    break_after: bool,
    #[allow(dead_code)]
    break_target: Option<String>,
    /// Optional override for the children of this node (used for splitting subforms/tables).
    children_override: Option<Vec<FormNodeId>>,
    /// Remaining text lines for a text leaf split across pages (§8.7).
    text_lines_override: Option<Vec<String>>,
    /// Per-child overrides propagated from nested splits.  When this node is
    /// processed, children whose ID appears here use the associated list as
    /// their own `children_override`, preventing duplication of already-placed
    /// content.
    nested_child_overrides: Option<Vec<(FormNodeId, Vec<FormNodeId>)>>,
}

type FittingResult = (
    LayoutPage,
    Vec<QueuedNode>,
    bool,
    Option<String>,
    Option<LayoutProfilePage>,
);

#[derive(Debug, Default)]
struct GroundTruthTraceState {
    last_break_before_read: Option<String>,
}

static GROUNDTRUTH_TRACE_STATE: OnceLock<Mutex<GroundTruthTraceState>> = OnceLock::new();

fn groundtruth_trace_enabled() -> bool {
    std::env::var("XFA_TRACE_DOC")
        .map(|value| !value.is_empty())
        .unwrap_or(false)
}

fn groundtruth_trace_state() -> &'static Mutex<GroundTruthTraceState> {
    GROUNDTRUTH_TRACE_STATE.get_or_init(|| Mutex::new(GroundTruthTraceState::default()))
}

/// The layout engine.
pub struct LayoutEngine<'a> {
    form: &'a FormTree,
    /// Experimental, **default-OFF** continuation-guard relaxation.
    ///
    /// When enabled (env `XFA_OVERFLOW_STATIC_BODY_CONTINUATION` set), a queued
    /// node may populate a continuation pageArea if it has *substantial visible
    /// body content* (Draw/Image/Field) filling ≥ 50 % of the continuation
    /// content area — not only data-backed fields. This fixes
    /// `UNDER_PAGINATED_LOW_RECALL` docs whose trailing full-page positioned
    /// subforms carry static (non-data-bound) body and are wrongly dropped by
    /// the §8.6 guard. Default-off so behavior is byte-equivalent to the prior
    /// guard until the validation gate graduates it. See milestone
    /// `XFA_LAYOUT_OVERFLOW_CONTINUATION_GUARD_FIX`.
    static_body_continuation: bool,
}

impl<'a> LayoutEngine<'a> {
    /// Create a new layout engine.
    pub fn new(form: &'a FormTree) -> Self {
        Self {
            form,
            static_body_continuation: std::env::var_os("XFA_OVERFLOW_STATIC_BODY_CONTINUATION")
                .is_some(),
        }
    }

    /// Resolve a SOM-string reference (typically a subform name) to a
    /// `FormNodeId` by walking from `anchor` up through parents and looking
    /// for a sibling whose `name` matches.  Falls back to a `find_by_xfa_id`
    /// lookup for templates that addressed the target by its `id` attribute.
    ///
    /// XFA Spec 3.3 §17 — overflow leader/trailer attributes are SOM
    /// references to subforms.  The canonical form is a sibling subform
    /// of the overflowing subform; matching by simple name + ancestor
    /// walk covers the cases observed in the parsed templates without
    /// pulling in the full SOM resolver.
    fn lookup_overflow_target(&self, anchor: FormNodeId, name: &str) -> Option<FormNodeId> {
        if name.is_empty() {
            return None;
        }
        if let Some(id) = self.form.find_by_xfa_id(name) {
            return Some(id);
        }
        // Walk up the parent chain; at each level search siblings (and the
        // root node's children when no parent is found).
        let mut cursor = Some(anchor);
        while let Some(node_id) = cursor {
            let parent = self.trace_parent_id(node_id);
            let scope = parent.map(|p| self.form.get(p).children.as_slice());
            let candidates = scope.unwrap_or_else(|| self.form.get(node_id).children.as_slice());
            for &candidate in candidates {
                if candidate != node_id && self.form.get(candidate).name == name {
                    return Some(candidate);
                }
            }
            cursor = parent;
        }
        None
    }

    /// Resolve `<overflow leader trailer>` SOM-string references attached to
    /// `node_id`'s `FormNodeMeta` into concrete `FormNodeId`s.
    ///
    /// Returns `(leader, trailer)` — either may be `None` either because the
    /// meta did not declare a reference or because the reference did not
    /// resolve to any node in the form tree (the unresolved case is silently
    /// ignored, matching the engine's prior behavior).
    fn resolve_overflow_refs(
        &self,
        node_id: FormNodeId,
    ) -> (Option<FormNodeId>, Option<FormNodeId>) {
        let meta = self.form.meta(node_id);
        let leader = meta
            .overflow_leader
            .as_deref()
            .and_then(|name| self.lookup_overflow_target(node_id, name));
        let trailer = meta
            .overflow_trailer
            .as_deref()
            .and_then(|name| self.lookup_overflow_target(node_id, name));
        (leader, trailer)
    }

    /// Return a `ContentArea` clone with overflow leader/trailer applied.
    /// `refs` take precedence over any existing `leader`/`trailer` already
    /// declared on the area (this is the overflow-page path; the base area
    /// values represent per-page leaders/trailers which the overflow refs
    /// override on continuation pages per XFA §8.10 + §17).
    fn content_area_with_overflow(
        ca: &ContentArea,
        refs: (Option<FormNodeId>, Option<FormNodeId>),
    ) -> ContentArea {
        let (leader, trailer) = refs;
        ContentArea {
            name: ca.name.clone(),
            x: ca.x,
            y: ca.y,
            width: ca.width,
            height: ca.height,
            leader: leader.or(ca.leader),
            trailer: trailer.or(ca.trailer),
        }
    }

    /// QF1-F — Per-subform overflow extension (XFA Spec 3.3 §17).
    ///
    /// Collect every (leader, trailer) `FormNodeId` referenced by any
    /// subform anywhere in the form tree.  The result is used to exclude
    /// those subforms from the regular content flow (preventing
    /// double-render — same logic as W2-A applies, but extended to every
    /// subform's refs, not just the root's).
    ///
    /// W2-A only filtered the root subform's overflow refs.  Per-subform
    /// extension must filter every subform's refs because any of them may
    /// produce a leader/trailer rendering on a continuation page.
    fn collect_all_overflow_subform_ids(&self) -> Vec<FormNodeId> {
        let mut ids = Vec::new();
        for (idx, _node) in self.form.nodes.iter().enumerate() {
            let node_id = FormNodeId(idx);
            let meta = self.form.meta(node_id);
            if meta.overflow_leader.is_none() && meta.overflow_trailer.is_none() {
                continue;
            }
            if let Some(name) = meta.overflow_leader.as_deref() {
                if let Some(target) = self.lookup_overflow_target(node_id, name) {
                    if !ids.contains(&target) {
                        ids.push(target);
                    }
                }
            }
            if let Some(name) = meta.overflow_trailer.as_deref() {
                if let Some(target) = self.lookup_overflow_target(node_id, name) {
                    if !ids.contains(&target) {
                        ids.push(target);
                    }
                }
            }
        }
        ids
    }

    /// QF1-F — Per-subform overflow resolution at break time.
    ///
    /// Given the first queued node about to be placed on a continuation
    /// page (i.e. the "next item" returned in `rest` from the previous
    /// page's `layout_content_fitting`), walk up the parent chain from
    /// that node looking for the closest ancestor subform whose
    /// `FormNodeMeta` declares an `overflow_leader` or `overflow_trailer`.
    /// Resolve those refs using the same `lookup_overflow_target` logic
    /// as W2-A.
    ///
    /// Returns the per-subform refs when found; otherwise falls back to
    /// `root_refs` (preserving W2-A bounded MVP semantics for documents
    /// that only declare overflow at the root subform).
    ///
    /// XFA §17 — `<overflow>` on a subform applies to continuation pages
    /// where that subform's content overflows. The closest ancestor wins
    /// because it represents the most specific overflow context.
    fn resolve_active_overflow_refs(
        &self,
        active_node: Option<FormNodeId>,
        root_refs: (Option<FormNodeId>, Option<FormNodeId>),
    ) -> (Option<FormNodeId>, Option<FormNodeId>) {
        let Some(start) = active_node else {
            return root_refs;
        };
        // Walk up the parent chain looking for any ancestor (or the node
        // itself) that declares overflow refs.
        let mut cursor: Option<FormNodeId> = Some(start);
        while let Some(node_id) = cursor {
            let meta = self.form.meta(node_id);
            if meta.overflow_leader.is_some() || meta.overflow_trailer.is_some() {
                let refs = self.resolve_overflow_refs(node_id);
                if refs.0.is_some() || refs.1.is_some() {
                    return refs;
                }
            }
            cursor = self.trace_parent_id(node_id);
        }
        root_refs
    }

    fn trace_node_id(&self, id: FormNodeId) -> String {
        let node = self.form.get(id);
        self.form
            .meta(id)
            .xfa_id
            .clone()
            .filter(|value| !value.is_empty())
            .or_else(|| (!node.name.is_empty()).then(|| node.name.clone()))
            .unwrap_or_else(|| id.0.to_string())
    }

    fn trace_node_name(&self, id: FormNodeId) -> String {
        let node = self.form.get(id);
        if !node.name.is_empty() {
            node.name.clone()
        } else {
            self.trace_node_id(id)
        }
    }

    fn trace_parent_id(&self, child_id: FormNodeId) -> Option<FormNodeId> {
        self.form
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| node.children.contains(&child_id).then_some(FormNodeId(idx)))
    }

    fn trace_path_segment(&self, id: FormNodeId) -> String {
        let node = self.form.get(id);
        format!(
            "{}#{}",
            Self::form_node_type_name(&node.node_type),
            self.trace_node_id(id)
        )
    }

    fn trace_node_path(&self, id: FormNodeId) -> String {
        let mut chain = vec![id];
        let mut cursor = id;
        while let Some(parent_id) = self.trace_parent_id(cursor) {
            chain.push(parent_id);
            cursor = parent_id;
        }
        chain.reverse();
        chain
            .into_iter()
            .map(|node_id| self.trace_path_segment(node_id))
            .collect::<Vec<_>>()
            .join("/")
    }

    #[track_caller]
    fn trace_vertical_state(
        &self,
        function: &'static str,
        current_y: Option<f64>,
        remaining_space: Option<f64>,
        node_id: Option<FormNodeId>,
        message: impl AsRef<str>,
    ) {
        if !groundtruth_trace_enabled() {
            return;
        }

        let location = std::panic::Location::caller();
        let (triggering_id, node_type, node_name) = if let Some(node_id) = node_id {
            let node = self.form.get(node_id);
            (
                self.trace_node_id(node_id),
                Self::form_node_type_name(&node.node_type).to_string(),
                self.trace_node_name(node_id),
            )
        } else {
            ("-".to_string(), "-".to_string(), "-".to_string())
        };

        eprintln!(
            "{}:{} fn={} current_y={} remaining_space={} triggering_element_id={} node_type={} node_name={} {}",
            location.file(),
            location.line(),
            function,
            current_y
                .map(|value| format!("{value:.3}"))
                .unwrap_or_else(|| "-".to_string()),
            remaining_space
                .map(|value| format!("{value:.3}"))
                .unwrap_or_else(|| "-".to_string()),
            triggering_id,
            node_type,
            node_name,
            message.as_ref(),
        );
    }

    #[track_caller]
    fn trace_break_before_read(
        &self,
        current_y: f64,
        remaining_space: f64,
        node_id: FormNodeId,
        break_before: bool,
        placed_count: usize,
        header_node_count: usize,
    ) {
        if !groundtruth_trace_enabled() {
            return;
        }

        let location = std::panic::Location::caller();
        let preceding = format!(
            "{}:{} break_before={} placed_count={} header_node_count={}",
            location.file(),
            location.line(),
            break_before,
            placed_count,
            header_node_count
        );
        if let Ok(mut state) = groundtruth_trace_state().lock() {
            state.last_break_before_read = Some(preceding);
        }
        self.trace_vertical_state(
            "layout_content_fitting",
            Some(current_y),
            Some(remaining_space),
            Some(node_id),
            format!(
                "break_before check break_before={} placed_count={} header_node_count={} event_type=read",
                break_before,
                placed_count,
                header_node_count
            ),
        );
    }

    #[track_caller]
    fn trace_commit_page_boundary(
        &self,
        current_page: usize,
        used_height: Option<f64>,
        page_height: Option<f64>,
        next_item: Option<&QueuedNode>,
    ) {
        if !groundtruth_trace_enabled() {
            return;
        }

        let remaining_space = match (used_height, page_height) {
            (Some(used), Some(total)) => Some(total - used),
            _ => None,
        };
        let preceding_break_read = groundtruth_trace_state()
            .lock()
            .ok()
            .and_then(|state| state.last_break_before_read.clone())
            .unwrap_or_else(|| "none".to_string());
        let break_before = next_item.map(|queued| queued.break_before).unwrap_or(false);
        let subform_path = next_item
            .map(|queued| self.trace_node_path(queued.id))
            .unwrap_or_else(|| "none".to_string());

        self.trace_vertical_state(
            "layout_internal",
            used_height,
            remaining_space,
            next_item.map(|queued| queued.id),
            format!(
                "event_type=page_commit COMMIT page_boundary page={} -> page={} break_before={} preceding_break_read=\"{}\" subform_path={}",
                current_page,
                current_page + 1,
                break_before,
                preceding_break_read,
                subform_path
            ),
        );
    }

    /// Perform layout and collect pagination diagnostics for each emitted page.
    pub fn layout_with_profile(&self, root: FormNodeId) -> Result<(LayoutDom, LayoutProfile)> {
        let mut profile = LayoutProfile::default();
        let dom = self.layout_internal(root, Some(&mut profile))?;
        Ok((dom, profile))
    }

    /// Perform layout on the entire form tree starting from the root node.
    ///
    /// XFA Spec 3.3 §8.6 — The Layout Algorithm (p288): content-driven single
    /// traversal of the Form DOM, placing nodes into the Layout DOM. When a
    /// container fills, traverse to the next container (§8.7 splitting, §8.8
    /// pagination).
    ///
    /// Supports multi-page pagination: when content overflows a page's content
    /// area, remaining nodes are placed on subsequent pages. The last page
    /// template is repeated as needed for overflow content.
    ///
    /// TODO §8.8: simplexPaginated/duplexPaginated, pagePosition/oddOrEven/
    /// blankOrNotBlank qualifications, termination processing (last/only page).
    pub fn layout(&self, root: FormNodeId) -> Result<LayoutDom> {
        self.layout_internal(root, None)
    }

    fn layout_internal(
        &self,
        root: FormNodeId,
        mut profile: Option<&mut LayoutProfile>,
    ) -> Result<LayoutDom> {
        self.trace_vertical_state(
            "layout_internal",
            Some(0.0),
            None,
            Some(root),
            format!("enter collect_profile={}", profile.is_some()),
        );
        let root_node = self.form.get(root);
        let collect_profile = profile.is_some();

        // XFA Spec 3.3 §17 — `<overflow leader trailer>` SOM-references on a
        // subform that overflows pages.  Resolve once for the root so that
        // overflow continuation pages can render the declared leader at the
        // top of the content area and the trailer at the bottom.
        let root_overflow_refs = self.resolve_overflow_refs(root);

        let (page_areas, raw_content_nodes) = self.extract_page_structure(root_node)?;
        self.trace_vertical_state(
            "layout_internal",
            Some(0.0),
            None,
            Some(root),
            format!(
                "page structure extracted page_areas={} raw_content_nodes={} event_type=runtime_allocation",
                page_areas.len(),
                raw_content_nodes.len()
            ),
        );
        // XFA §17: subforms that are resolved as overflow leaders or trailers
        // must not also be rendered in the regular content flow — they would
        // double-render (once as content, once as the per-page decoration).
        //
        // QF1-F per-subform extension: the W2-A bounded MVP only filtered
        // the root subform's overflow refs.  Per-subform overflow refs may
        // also surface on continuation pages, so any subform's resolved
        // leader/trailer ids are filtered out here as well.  This keeps
        // the no-refs branch identical to baseline (the collected list is
        // empty), and the root-only case identical to W2-A (only the root
        // refs are in the list).
        let all_overflow_ids = self.collect_all_overflow_subform_ids();
        let raw_content_nodes: Vec<FormNodeId> = raw_content_nodes
            .into_iter()
            .filter(|id| !all_overflow_ids.contains(id))
            .collect();
        // Build queued nodes with break_before flags and occur expansion.
        let content_queued = self.queue_content(&raw_content_nodes);

        let mut pages = Vec::new();

        if page_areas.is_empty() {
            // No explicit page structure — use root's dimensions
            let page_w = root_node.box_model.width.unwrap_or(612.0);
            let page_h = root_node.box_model.height.unwrap_or(792.0);
            let area = ContentArea {
                name: String::new(),
                x: 0.0,
                y: 0.0,
                width: page_w,
                height: page_h,
                leader: None,
                trailer: None,
            };

            if root_node.layout == LayoutStrategy::TopToBottom {
                // TB layout supports pagination: split content across pages
                let mut remaining = content_queued;
                let page_limit = self.estimate_page_limit(&remaining, page_h);
                // XFA §17: overflow leader/trailer apply to continuation
                // pages (page 2 onwards).  QF1-F: per-subform refs win
                // over root refs when the next queued node sits inside a
                // subform that declares its own overflow.  The continuation
                // area is recomputed per-page since the active subform may
                // change as pagination advances.
                while !remaining.is_empty() {
                    if pages.len() >= page_limit {
                        eprintln!(
                            "WARNING: Page limit ({}) reached, truncating layout for {}",
                            page_limit, root_node.name
                        );
                        break;
                    }
                    let active_refs = self.resolve_active_overflow_refs(
                        remaining.first().map(|qn| qn.id),
                        root_overflow_refs,
                    );
                    let overflow_area = Self::content_area_with_overflow(&area, active_refs);
                    let area_for_page = if pages.is_empty() {
                        &area
                    } else {
                        &overflow_area
                    };
                    let (page, rest, consumed_break_only, _, page_profile) = self
                        .layout_content_fitting(
                            area_for_page,
                            &remaining,
                            page_w,
                            page_h,
                            collect_profile,
                        )?;
                    if page.nodes.is_empty() && !consumed_break_only {
                        // Force place one item to prevent infinite loop
                        let forced = self.layout_content_on_page(
                            area_for_page,
                            page_w,
                            page_h,
                            &[remaining[0].id],
                            root_node.layout,
                        )?;
                        let next_remaining = remaining[1..].to_vec();
                        log::debug!(
                            "XFA layout: processing page {}/{} (forced)",
                            pages.len() + 1,
                            page_limit
                        );
                        if let Some(profile) = profile.as_deref_mut() {
                            profile.pages.push(
                                self.profile_page_from_nodes(
                                    &forced,
                                    area_for_page.y,
                                    area_for_page.height,
                                    !next_remaining.is_empty(),
                                    next_remaining
                                        .first()
                                        .map(|qn| self.describe_queued_node(qn)),
                                ),
                            );
                        }
                        pages.push(forced);
                        remaining = next_remaining;
                    } else if consumed_break_only {
                        // Break-only page: skip the blank page, continue with rest
                        remaining = rest;
                    } else {
                        log::debug!(
                            "XFA layout: processing page {}/{}",
                            pages.len() + 1,
                            page_limit
                        );
                        if !rest.is_empty() {
                            self.trace_commit_page_boundary(
                                pages.len() + 1,
                                page_profile.as_ref().map(|profile| profile.used_height),
                                page_profile.as_ref().map(|profile| profile.page_height),
                                rest.first(),
                            );
                        }
                        if let (Some(profile), Some(page_profile)) =
                            (profile.as_deref_mut(), page_profile)
                        {
                            profile.pages.push(page_profile);
                        }
                        pages.push(page);
                        remaining = rest;
                    }
                }
            } else {
                // Non-TB layouts: place everything on one page (layout_children expands occur)
                let page = self.layout_content_on_page(
                    &area,
                    page_w,
                    page_h,
                    &raw_content_nodes,
                    root_node.layout,
                )?;
                if let Some(profile) = profile.as_deref_mut() {
                    profile.pages.push(self.profile_page_from_nodes(
                        &page,
                        area.y,
                        area.height,
                        false,
                        None,
                    ));
                }
                pages.push(page);
            }
        } else {
            // XFA §4.2 — Positioned content: when ALL content nodes use
            // positioned layout (absolute x/y coordinates), they share a
            // single page.  Flowing them top-to-bottom across pages is
            // incorrect and causes over-pagination (e.g. 2-page output for
            // a 1-page form whose template defines two positioned subforms
            // overlaid on the same pageArea).
            //
            // Guard: overlay heuristic only applies when every positioned
            // content subform is *small* relative to the page (height ≤ 50%
            // of the content area).  Page-sized subforms represent separate
            // pages and must flow through the normal pagination path — cramming
            // them onto one page causes under-pagination (#GATE-22).
            let multi_positioned = content_queued.len() > 1
                && content_queued.iter().all(|qn| {
                    let node = self.form.get(qn.id);
                    node.layout == LayoutStrategy::Positioned
                        && matches!(
                            node.node_type,
                            FormNodeType::Subform | FormNodeType::Area | FormNodeType::ExclGroup
                        )
                        && !qn.break_before
                })
                && {
                    let pa = &page_areas[0];
                    let ca = primary_content_area(pa);
                    let half_page = ca.height * 0.5;
                    content_queued
                        .iter()
                        .all(|qn| self.compute_extent(qn.id).height <= half_page)
                };

            // #794 — Single positioned subform delegation: when there is
            // exactly 1 content node that is a Positioned subform whose
            // children are ALL Positioned AND fit within the page, the
            // parent TopToBottom flow would compute the child's full
            // envelope height and overflow to extra pages.  Delegate to
            // positioned layout instead.  If the content overflows the
            // page, let it paginate normally (#736).
            let single_positioned_delegate = content_queued.len() == 1 && {
                let qn = &content_queued[0];
                let node = self.form.get(qn.id);
                if node.layout == LayoutStrategy::Positioned
                    && matches!(
                        node.node_type,
                        FormNodeType::Subform | FormNodeType::Area | FormNodeType::ExclGroup
                    )
                    && !qn.break_before
                    && !node.children.is_empty()
                    && node.children.iter().all(|&cid| {
                        let child = self.form.get(cid);
                        child.layout == LayoutStrategy::Positioned
                    })
                {
                    // Check that the positioned content fits on one page.
                    let pa = &page_areas[0];
                    let ca = primary_content_area(pa);
                    let child_extent = self.compute_extent(qn.id);
                    child_extent.height <= ca.height
                } else {
                    false
                }
            };

            let all_content_positioned = multi_positioned || single_positioned_delegate;

            // Diagnostic (read-only, env-gated; no behavior change). Milestone
            // XFA_LAYOUT_OVERFLOW_PAGINATION_PARITY.
            if std::env::var_os("XFA_OF_TRACE").is_some() {
                let ca_h = page_areas.first().map(|pa| primary_content_area(pa).height);
                let parts: Vec<String> = content_queued
                    .iter()
                    .map(|qn| {
                        let n = self.form.get(qn.id);
                        format!(
                            "[{:?} h={:.0} bb={}]",
                            n.layout,
                            self.compute_extent(qn.id).height,
                            qn.break_before
                        )
                    })
                    .collect();
                eprintln!(
                    "XFA_OF_TRACE page_areas={} content_queued={} ca_h={:?} multi_pos={} single_deleg={} all_pos={} nodes={}",
                    page_areas.len(),
                    content_queued.len(),
                    ca_h,
                    multi_positioned,
                    single_positioned_delegate,
                    all_content_positioned,
                    parts.join(",")
                );
            }

            if all_content_positioned {
                let pa = &page_areas[0];
                let ca = primary_content_area(pa);
                let ids: Vec<FormNodeId> = content_queued.iter().map(|qn| qn.id).collect();
                let mut page = self.layout_content_on_page(
                    ca,
                    pa.page_width,
                    pa.page_height,
                    &ids,
                    LayoutStrategy::Positioned,
                )?;
                let page_profile = if collect_profile {
                    Some(self.profile_page_from_nodes(&page, ca.y, ca.height, false, None))
                } else {
                    None
                };
                self.prepend_fixed_nodes(&pa.fixed_nodes, &mut page)?;
                page.runtime_instantiated = pa.runtime_instantiated;
                if Self::has_visible_content(&page.nodes) {
                    if let (Some(profile), Some(page_profile)) =
                        (profile.as_deref_mut(), page_profile)
                    {
                        profile.pages.push(page_profile);
                    }
                    pages.push(page);
                }
            }

            // Layout content across page areas, then repeat last template for overflow.
            let mut remaining = if all_content_positioned {
                Vec::new()
            } else {
                content_queued
            };
            let data_driven_body_queue =
                self.queued_nodes_have_data_backed_body_content(&remaining);
            let page_area_continuation_needs_body_content =
                page_areas.len() > 1 || data_driven_body_queue;
            for pa in &page_areas {
                if remaining.is_empty() {
                    break;
                }
                // XFA 3.3 §8.6: layout termination is content-driven.  A
                // queued pageArea continuation after an emitted page is used
                // only when body content, an explicit page anchor, or an
                // accepted split remainder still needs that next pageArea.
                if !pages.is_empty()
                    && !self.queued_nodes_can_populate_continuation_page_area(
                        &remaining,
                        page_area_continuation_needs_body_content,
                    )
                    // Experimental (default-off) relaxation: keep the continuation
                    // when a remaining node has substantial visible *static* body
                    // content filling this pageArea. See `static_body_continuation`.
                    && !(self.static_body_continuation
                        && self.queued_nodes_have_substantial_visible_body(
                            &remaining,
                            primary_content_area(pa).height,
                        ))
                {
                    // Diagnostic (read-only, env-gated; no behavior change).
                    // Milestone XFA_LAYOUT_OVERFLOW_PAGINATION_PARITY.
                    if std::env::var_os("XFA_OF_TRACE").is_some() {
                        eprintln!(
                            "XFA_OF_TRACE guard-drop: remaining={} after pages_committed={} (queued_nodes_can_populate_continuation_page_area=false -> continuation suppressed)",
                            remaining.len(),
                            pages.len()
                        );
                    }
                    remaining.clear();
                    break;
                }
                let ca = primary_content_area(pa);
                let (mut placed, rest, consumed_break_only, _, page_profile) = self
                    .layout_content_fitting(
                        ca,
                        &remaining,
                        pa.page_width,
                        pa.page_height,
                        collect_profile,
                    )?;
                let commit_used_height = page_profile.as_ref().map(|profile| profile.used_height);
                let commit_page_height = page_profile.as_ref().map(|profile| profile.page_height);
                let mut page_committed = false;
                if consumed_break_only {
                    remaining = rest;
                } else if Self::has_visible_content(&placed.nodes) {
                    self.prepend_fixed_nodes(&pa.fixed_nodes, &mut placed)?;
                    placed.runtime_instantiated = pa.runtime_instantiated;
                    if let (Some(profile), Some(page_profile)) =
                        (profile.as_deref_mut(), page_profile)
                    {
                        profile.pages.push(page_profile);
                    }
                    pages.push(placed);
                    page_committed = true;
                    remaining = rest;
                } else if !pa.fixed_nodes.is_empty() {
                    // Content nodes are invisible but the page area has
                    // fixed elements (headers, footers, decorations).
                    // Create the page with just the fixed chrome — this
                    // matches Adobe's behavior for explicit page areas
                    // whose flowing content is blank/hidden.
                    self.prepend_fixed_nodes(&pa.fixed_nodes, &mut placed)?;
                    placed.runtime_instantiated = pa.runtime_instantiated;
                    if Self::has_visible_content(&placed.nodes) {
                        if let (Some(profile), Some(page_profile)) =
                            (profile.as_deref_mut(), page_profile)
                        {
                            profile.pages.push(page_profile);
                        }
                        pages.push(placed);
                        page_committed = true;
                    }
                    remaining = rest;
                } else {
                    // Content nodes are all hidden/invisible and the page
                    // area has no fixed elements — suppress the blank page.
                    remaining = rest;
                }
                if page_committed && !remaining.is_empty() {
                    self.trace_commit_page_boundary(
                        pages.len(),
                        commit_used_height,
                        commit_page_height,
                        remaining.first(),
                    );
                }
            }

            // XFA 3.3 §3.1 — when ALL pageAreas are runtime-instantiated
            // (i.e. the form DOM recorded an explicit page-tree allocation),
            // overflow beyond the recorded count is suppressed.  The runtime
            // already committed to N pages; emitting more is over-pagination
            // relative to the saved form state.  Excess body content is
            // dropped from the visible output.
            if !page_areas.is_empty() && page_areas.iter().all(|pa| pa.runtime_instantiated) {
                remaining.clear();
            }

            // Overflow: repeat page templates until all content is placed.
            if !remaining.is_empty() {
                let last_idx = page_areas.len() - 1;
                let overflow_ca_ref = primary_content_area(&page_areas[last_idx]);
                // XFA §17: apply the active subform's overflow leader/trailer
                // to each continuation page.  QF1-F per-subform extension:
                // the active refs are recomputed per-page from the first
                // remaining queued node's ancestor chain; the root refs are
                // the W2-A fallback when no nested subform declares its own.
                // Dynamic page limit: estimated pages for remaining content + already placed.
                let page_limit =
                    self.estimate_page_limit(&remaining, overflow_ca_ref.height) + pages.len();
                while !remaining.is_empty() {
                    if pages.len() >= page_limit {
                        eprintln!(
                            "WARNING: Page limit ({}) reached, truncating layout overflow for {}",
                            page_limit, root_node.name
                        );
                        break;
                    }
                    let pa_idx = last_idx;
                    let pa = &page_areas[pa_idx];
                    // Use the overflow-augmented content area for every page
                    // in this loop — XFA §17 says the leader/trailer apply
                    // to overflow continuation pages.
                    let active_refs = self.resolve_active_overflow_refs(
                        remaining.first().map(|qn| qn.id),
                        root_overflow_refs,
                    );
                    let overflow_ca =
                        Self::content_area_with_overflow(overflow_ca_ref, active_refs);
                    let ca = &overflow_ca;

                    let (mut page, rest, consumed_break_only, _, page_profile) = self
                        .layout_content_fitting(
                            ca,
                            &remaining,
                            pa.page_width,
                            pa.page_height,
                            collect_profile,
                        )?;
                    if page.nodes.is_empty() && !consumed_break_only {
                        let forced = self.layout_content_on_page(
                            ca,
                            pa.page_width,
                            pa.page_height,
                            &[remaining[0].id],
                            LayoutStrategy::TopToBottom,
                        )?;
                        let next_remaining = remaining[1..].to_vec();
                        if Self::has_visible_content(&forced.nodes) {
                            let mut forced = forced;
                            let forced_profile = if collect_profile {
                                Some(
                                    self.profile_page_from_nodes(
                                        &forced,
                                        ca.y,
                                        ca.height,
                                        !next_remaining.is_empty(),
                                        next_remaining
                                            .first()
                                            .map(|qn| self.describe_queued_node(qn)),
                                    ),
                                )
                            } else {
                                None
                            };
                            self.prepend_fixed_nodes(&pa.fixed_nodes, &mut forced)?;
                            forced.runtime_instantiated = pa.runtime_instantiated;
                            if let (Some(profile), Some(page_profile)) =
                                (profile.as_deref_mut(), forced_profile)
                            {
                                profile.pages.push(page_profile);
                            }
                            pages.push(forced);
                        }
                        remaining = next_remaining;
                    } else if consumed_break_only {
                        remaining = rest;
                    } else {
                        if Self::has_visible_content(&page.nodes) {
                            self.prepend_fixed_nodes(&pa.fixed_nodes, &mut page)?;
                            page.runtime_instantiated = pa.runtime_instantiated;
                            if !rest.is_empty() {
                                self.trace_commit_page_boundary(
                                    pages.len() + 1,
                                    page_profile.as_ref().map(|profile| profile.used_height),
                                    page_profile.as_ref().map(|profile| profile.page_height),
                                    rest.first(),
                                );
                            }
                            if let (Some(profile), Some(page_profile)) =
                                (profile.as_deref_mut(), page_profile)
                            {
                                profile.pages.push(page_profile);
                            }
                            pages.push(page);
                        }
                        remaining = rest;
                    }
                }
            }
        }

        Ok(LayoutDom { pages })
    }

    // -------------------------------------------------------------------
    // Helper methods for queued/hidden-aware pagination
    // -------------------------------------------------------------------

    fn queued_nodes_can_populate_continuation_page_area(
        &self,
        nodes: &[QueuedNode],
        needs_body_content: bool,
    ) -> bool {
        if !needs_body_content {
            return true;
        }

        nodes
            .iter()
            .any(|node| self.queued_node_can_populate_continuation_page_area(node))
    }

    fn queued_node_can_populate_continuation_page_area(&self, node: &QueuedNode) -> bool {
        self.queued_node_has_explicit_page_anchor(node)
            || Self::queued_node_is_split_remainder(node)
            || self.queued_node_has_data_backed_body_content(node)
    }

    /// Experimental (default-off) continuation predicate: does any remaining
    /// queued node have substantial visible body content for this continuation
    /// content area? Gated by `static_body_continuation`.
    fn queued_nodes_have_substantial_visible_body(
        &self,
        nodes: &[QueuedNode],
        content_area_height: f64,
    ) -> bool {
        nodes
            .iter()
            .any(|node| self.queued_node_has_substantial_visible_body(node, content_area_height))
    }

    /// A queued node can populate a continuation pageArea when it has visible
    /// body content (Draw / Image / Field — not only data-backed) AND its
    /// content extent fills ≥ 50 % of the continuation content area. The size
    /// floor distinguishes a real static *body* page (e.g. a near-full-page
    /// positioned instructions/terms subform) from small *chrome*
    /// (headers/footers, which are handled via `pa.fixed_nodes`). This is a
    /// content-geometry invariant — not an oracle page-count shortcut.
    fn queued_node_has_substantial_visible_body(
        &self,
        node: &QueuedNode,
        content_area_height: f64,
    ) -> bool {
        if content_area_height <= 0.0 {
            return false;
        }
        self.subtree_has_visible_body_content(node.id)
            && self.compute_extent(node.id).height >= 0.5 * content_area_height
    }

    /// Visible body content = a non-layout-hidden Draw / Image / Field leaf, or
    /// any container whose occur-expanded subtree contains one. Unlike
    /// `subtree_has_data_backed_body_content`, static draws/images count here.
    fn subtree_has_visible_body_content(&self, id: FormNodeId) -> bool {
        if self.is_layout_hidden(id) {
            return false;
        }
        let node = self.form.get(id);
        match &node.node_type {
            FormNodeType::Draw(_) | FormNodeType::Image { .. } | FormNodeType::Field { .. } => true,
            FormNodeType::Root
            | FormNodeType::Subform
            | FormNodeType::Area
            | FormNodeType::ExclGroup
            | FormNodeType::SubformSet => self
                .expand_occur(&node.children)
                .iter()
                .any(|&child_id| self.subtree_has_visible_body_content(child_id)),
            FormNodeType::PageSet | FormNodeType::PageArea { .. } => false,
        }
    }

    fn queued_node_has_explicit_page_anchor(&self, node: &QueuedNode) -> bool {
        node.break_before
            || node.break_target.is_some()
            || self.form.meta(node.id).page_break_before
    }

    fn queued_node_is_split_remainder(node: &QueuedNode) -> bool {
        node.text_lines_override.is_some()
            || node.children_override.is_some()
            || node
                .nested_child_overrides
                .as_ref()
                .is_some_and(|overrides| !overrides.is_empty())
    }

    fn queued_nodes_have_data_backed_body_content(&self, nodes: &[QueuedNode]) -> bool {
        nodes
            .iter()
            .any(|node| self.queued_node_has_data_backed_body_content(node))
    }

    fn queued_node_has_data_backed_body_content(&self, node: &QueuedNode) -> bool {
        self.subtree_has_data_backed_body_content(
            node.id,
            node.children_override.as_deref(),
            node.nested_child_overrides.as_deref(),
            false,
        )
    }

    /// XFA 3.3 §8.6 — layout termination is driven by remaining content.
    /// Static template chrome may render on an already-required page, but it
    /// does not by itself force an extra pageArea continuation after the
    /// data-backed body content has ended.
    fn subtree_has_data_backed_body_content(
        &self,
        id: FormNodeId,
        children_override: Option<&[FormNodeId]>,
        nested_overrides: Option<&[(FormNodeId, Vec<FormNodeId>)]>,
        inherited_data_context: bool,
    ) -> bool {
        if self.is_layout_hidden(id) {
            return false;
        }

        let node = self.form.get(id);
        let meta = self.form.meta(id);
        let own_data_context = !meta.data_bind_none && meta.data_bind_ref.is_some();
        let data_context = inherited_data_context || own_data_context;

        match &node.node_type {
            FormNodeType::Field { value } => data_context && !value.trim().is_empty(),
            FormNodeType::Root
            | FormNodeType::Subform
            | FormNodeType::Area
            | FormNodeType::ExclGroup
            | FormNodeType::SubformSet => {
                let children = children_override.unwrap_or(&node.children);
                let expanded = if children_override.is_some() {
                    children.to_vec()
                } else {
                    self.expand_occur(children)
                };
                expanded.iter().any(|&child_id| {
                    let child_override = nested_overrides
                        .and_then(|overrides| {
                            overrides
                                .iter()
                                .find(|(override_id, _)| *override_id == child_id)
                        })
                        .map(|(_, child_override)| child_override.as_slice());
                    self.subtree_has_data_backed_body_content(
                        child_id,
                        child_override,
                        nested_overrides,
                        data_context,
                    )
                })
            }
            FormNodeType::PageSet
            | FormNodeType::PageArea { .. }
            | FormNodeType::Draw(_)
            | FormNodeType::Image { .. } => false,
        }
    }

    /// Estimate a dynamic page limit based on total content height vs page
    /// content area height.  Returns `min(estimated * 2, MAX_PAGES)` with a
    /// floor of 10 so tiny forms still have room for splitting overhead. (#764)
    fn estimate_page_limit(&self, content: &[QueuedNode], page_height: f64) -> usize {
        if page_height <= 0.0 {
            return MAX_PAGES;
        }
        let total_height: f64 = content
            .iter()
            .map(|qn| {
                self.compute_extent_with_available_and_override(
                    qn.id,
                    None,
                    qn.children_override.as_deref(),
                )
                .height
            })
            .sum();
        let estimated = (total_height / page_height).ceil() as usize;
        (estimated * 2).clamp(10, MAX_PAGES)
    }

    /// Returns true if the layout nodes contain at least one visually rendered
    /// element (non-`None` content or a child with rendered content).
    ///
    /// Used to suppress blank pages whose content nodes are all hidden/invisible
    /// (e.g. conditional contact-form pages that are collapsed in the data).
    fn has_visible_content(nodes: &[LayoutNode]) -> bool {
        nodes.iter().any(|n| {
            !matches!(n.content, LayoutContent::None) || Self::has_visible_content(&n.children)
        })
    }

    /// Returns true if the node should be completely skipped during layout
    /// (no layout space consumed).
    ///
    /// Adobe empirical behavior (not strictly per spec):
    /// - `hidden` / `invisible` / `inactive` -- no layout space, not rendered.
    /// - `visible` -- normal.
    ///
    /// See `Presence::is_layout_hidden()` for rationale (fixes #806).
    fn is_layout_hidden(&self, id: FormNodeId) -> bool {
        let meta = self.form.meta(id);
        if meta.presence.is_layout_hidden() {
            return true;
        }
        // Content-area-targeted nodes (breakBefore targetType="contentArea")
        // are excluded from the primary flow ONLY when they are small
        // decorative elements (< 50pt tall).  Large content subforms that
        // target the primary contentArea should still be laid out.
        if meta.content_area_break {
            let node = self.form.get(id);
            let h = node.box_model.height.unwrap_or(f64::MAX);
            return h < 50.0;
        }
        false
    }

    /// Build a queue of `QueuedNode`s from a list of child IDs, skipping
    /// nodes that are layout-hidden and expanding occur rules.
    fn queue_content(&self, children: &[FormNodeId]) -> Vec<QueuedNode> {
        let expanded = self.expand_occur(children);
        expanded
            .into_iter()
            .filter(|&id| !self.is_layout_hidden(id))
            .map(|id| {
                let meta = self.form.meta(id);
                self.trace_vertical_state(
                    "queue_content",
                    None,
                    None,
                    Some(id),
                    format!(
                        "copy FormNodeMeta -> QueuedNode break_before={} break_after={} break_target={} event_type=copy_merge",
                        meta.page_break_before,
                        meta.page_break_after,
                        meta.break_target.clone().unwrap_or_default()
                    ),
                );
                QueuedNode {
                    id,
                    break_before: meta.page_break_before,
                    break_after: meta.page_break_after,
                    break_target: meta.break_target.clone(),
                    children_override: None,
                    text_lines_override: None,
                    nested_child_overrides: None,
                }
            })
            .collect()
    }

    fn subtree_is_blank(&self, id: FormNodeId) -> bool {
        let node = self.form.get(id);
        match &node.node_type {
            FormNodeType::Field { value } => value.is_empty(),
            FormNodeType::Draw(ref content) => {
                if let DrawContent::Text(text) = content {
                    text.is_empty()
                } else {
                    false
                }
            }
            FormNodeType::Image { data, .. } => data.is_empty(),
            FormNodeType::Root | FormNodeType::PageSet | FormNodeType::PageArea { .. } => true,
            // Subform, Area, ExclGroup, SubformSet: blank when all children blank.
            FormNodeType::Subform
            | FormNodeType::Area
            | FormNodeType::ExclGroup
            | FormNodeType::SubformSet => node.children.iter().all(|&c| self.subtree_is_blank(c)),
        }
    }

    /// XFA Spec 3.3 §8.9 — Adhesion (p311-314): whether `current_id` has a
    /// keep-with-next constraint relative to `next_id`, or `next_id` has
    /// keep-with-previous. Per spec, two adjacent objects adhere if the first
    /// declares next=contentArea OR the second declares previous=contentArea.
    /// Adhesion is restricted to siblings in the Form DOM (§8.9 p314).
    ///
    /// TODO §8.9: keep.pageArea level (must be on same page, not just same CA).
    #[allow(dead_code)]
    fn keep_links_content(&self, current_id: FormNodeId, next_id: FormNodeId) -> bool {
        let cur_meta = self.form.meta(current_id);
        let nxt_meta = self.form.meta(next_id);
        cur_meta.keep_next_content_area
            || nxt_meta.keep_previous_content_area
            || self.is_spacer_keep_with_next(current_id)
    }

    /// Compute the cumulative height of a keep-chain starting at
    /// `start_idx` within `children`.
    #[allow(dead_code)]
    fn keep_chain_height(&self, children: &[FormNodeId], start_idx: usize, available: Size) -> f64 {
        let mut total = 0.0;
        for i in start_idx..children.len() {
            let sz = self.compute_extent_with_available(children[i], Some(available));
            total += sz.height;
            if i + 1 < children.len() && !self.keep_links_content(children[i], children[i + 1]) {
                break;
            }
        }
        total
    }

    /// Compute the cumulative height of a keep-chain starting at
    /// `start_idx` within a queued-node slice.
    #[allow(dead_code)]
    fn queued_keep_chain_height(
        &self,
        children: &[QueuedNode],
        start_idx: usize,
        available: Size,
    ) -> f64 {
        let mut total = 0.0;
        for i in start_idx..children.len() {
            let sz = self.compute_extent_with_available(children[i].id, Some(available));
            total += sz.height;
            if i + 1 < children.len()
                && !self.keep_links_content(children[i].id, children[i + 1].id)
            {
                break;
            }
        }
        total
    }

    /// Returns true when the node is a blank spacer with a keep-intact
    /// constraint (used as a "glue" between siblings).
    fn is_spacer_keep_with_next(&self, id: FormNodeId) -> bool {
        let meta = self.form.meta(id);
        meta.keep_intact_content_area && self.subtree_is_blank(id)
    }

    /// Compute the cumulative height of a keep-chain starting at index
    /// `start_idx` within a filtered visible-ids list. The chain extends
    /// as long as consecutive nodes have `keep_next_content_area` or the
    /// next node has `keep_previous_content_area`.
    /// Returns (chain_height, chain_length).  A chain of length 1 means no
    /// keep properties link the node to its neighbour — it should NOT trigger
    /// a keep-chain page break and instead go through normal overflow/split.
    fn visible_keep_chain_height(
        &self,
        visible_ids: &[(usize, FormNodeId)],
        start_idx: usize,
        available: Size,
    ) -> (f64, usize) {
        self.trace_vertical_state(
            "visible_keep_chain_height",
            Some(0.0),
            Some(available.height),
            visible_ids.get(start_idx).map(|(_, id)| *id),
            format!(
                "enter start_idx={} visible_ids={}",
                start_idx,
                visible_ids.len()
            ),
        );
        let mut total = 0.0;
        let mut count = 0usize;
        for i in start_idx..visible_ids.len() {
            let (_, id) = visible_ids[i];
            let sz = self.compute_extent_with_available(id, Some(available));
            total += sz.height;
            count += 1;
            self.trace_vertical_state(
                "visible_keep_chain_height",
                Some(total),
                Some(available.height - total),
                Some(id),
                format!(
                    "accumulate keep chain index={} node_height={:.3} running_total={:.3} count={} event_type=read",
                    i,
                    sz.height,
                    total,
                    count
                ),
            );
            // Check if the chain continues to the next node.
            if i + 1 < visible_ids.len() {
                let (_, next_id) = visible_ids[i + 1];
                let cur_meta = self.form.meta(id);
                let nxt_meta = self.form.meta(next_id);
                let keep = cur_meta.keep_next_content_area
                    || nxt_meta.keep_previous_content_area
                    || (cur_meta.keep_intact_content_area && self.subtree_is_blank(id));
                self.trace_vertical_state(
                    "visible_keep_chain_height",
                    Some(total),
                    Some(available.height - total),
                    Some(id),
                    format!(
                        "keep-chain continuation check next={} cur.keep_next={} next.keep_previous={} cur.keep_intact_blank={} result={} event_type=read",
                        self.trace_node_id(next_id),
                        cur_meta.keep_next_content_area,
                        nxt_meta.keep_previous_content_area,
                        cur_meta.keep_intact_content_area && self.subtree_is_blank(id),
                        keep
                    ),
                );
                if !keep {
                    break;
                }
            }
        }
        self.trace_vertical_state(
            "visible_keep_chain_height",
            Some(total),
            Some(available.height - total),
            visible_ids.get(start_idx).map(|(_, id)| *id),
            format!(
                "return chain_height={:.3} chain_len={} event_type=read",
                total, count
            ),
        );
        (total, count)
    }

    fn extract_page_structure(
        &self,
        root: &FormNode,
    ) -> Result<(Vec<PageAreaInfo>, Vec<FormNodeId>)> {
        let mut page_areas = Vec::new();
        let mut content_nodes = Vec::new();

        for &child_id in &root.children {
            let child = self.form.get(child_id);
            match &child.node_type {
                FormNodeType::PageSet => {
                    for &pa_id in &child.children {
                        let pa_node = self.form.get(pa_id);
                        if let FormNodeType::PageArea { content_areas } = &pa_node.node_type {
                            let pa_meta = self.form.meta(pa_id);
                            let fixed: Vec<FormNodeId> = pa_node
                                .children
                                .iter()
                                .copied()
                                .filter(|&cid| {
                                    matches!(
                                        self.form.get(cid).node_type,
                                        FormNodeType::Draw(..)
                                            | FormNodeType::Subform
                                            | FormNodeType::Area
                                            | FormNodeType::ExclGroup
                                    )
                                })
                                .collect();
                            page_areas.push(PageAreaInfo {
                                name: pa_node.name.clone(),
                                xfa_id: pa_meta.xfa_id.clone(),
                                content_areas: content_areas.clone(),
                                page_width: pa_node.box_model.width.unwrap_or(612.0),
                                page_height: pa_node.box_model.height.unwrap_or(792.0),
                                fixed_nodes: fixed,
                                runtime_instantiated: pa_meta.runtime_instantiated_page,
                            });
                        }
                    }
                }
                FormNodeType::PageArea { content_areas } => {
                    let pa_meta = self.form.meta(child_id);
                    let fixed: Vec<FormNodeId> = child
                        .children
                        .iter()
                        .copied()
                        .filter(|&cid| {
                            matches!(
                                self.form.get(cid).node_type,
                                FormNodeType::Draw(..)
                                    | FormNodeType::Subform
                                    | FormNodeType::Area
                                    | FormNodeType::ExclGroup
                            )
                        })
                        .collect();
                    page_areas.push(PageAreaInfo {
                        name: child.name.clone(),
                        xfa_id: pa_meta.xfa_id.clone(),
                        content_areas: content_areas.clone(),
                        page_width: child.box_model.width.unwrap_or(612.0),
                        page_height: child.box_model.height.unwrap_or(792.0),
                        fixed_nodes: fixed,
                        runtime_instantiated: pa_meta.runtime_instantiated_page,
                    });
                }
                // XFA's canonical nesting: <subform layout="paginate"> wraps
                // <pageSet> and content siblings.  Look one level deeper so the
                // inner pageSet defines the page geometry and the remaining
                // children become the content nodes.  If no pageSet is found
                // inside, fall through and treat the subform as content.
                // (Fixes xl_02_row_layout.pdf and xl_09_field_types.pdf blank
                // render: the pageSet's 792pt height was consuming the full page
                // and pushing form content to page 2.)
                FormNodeType::Subform
                // Area is a positioned container — same page-structure logic as Subform.
                | FormNodeType::Area
                // ExclGroup is a radio-button group — treat as a TB subform container.
                | FormNodeType::ExclGroup => {
                    // Only recurse into subforms that directly contain a
                    // PageSet child.  Blind recursion into content subforms
                    // (especially Positioned ones) would shatter their
                    // internal structure and lose breakBefore/overflow
                    // semantics.
                    let has_pageset = child
                        .children
                        .iter()
                        .any(|&cid| matches!(self.form.get(cid).node_type, FormNodeType::PageSet));
                    if has_pageset {
                        let (inner_areas, inner_content) = self.extract_page_structure(child)?;
                        page_areas.extend(inner_areas);
                        content_nodes.extend(inner_content);
                    } else if child.layout == LayoutStrategy::TopToBottom {
                        // TB subform without inner PageSet — still recurse in
                        // case an inner TB child wraps a PageSet.
                        let (inner_areas, inner_content) = self.extract_page_structure(child)?;
                        if !inner_areas.is_empty() {
                            page_areas.extend(inner_areas);
                            content_nodes.extend(inner_content);
                        } else {
                            content_nodes.push(child_id);
                        }
                    } else {
                        content_nodes.push(child_id);
                    }
                }
                // SubformSet is transparent: process its children as direct content.
                // XFA 3.3 §7.1 — a subformSet is a conditional grouping with no
                // layout contribution of its own.
                FormNodeType::SubformSet => {
                    let (inner_areas, inner_content) = self.extract_page_structure(child)?;
                    page_areas.extend(inner_areas);
                    content_nodes.extend(inner_content);
                }
                _ => {
                    content_nodes.push(child_id);
                }
            }
        }

        Ok((page_areas, content_nodes))
    }

    fn layout_content_on_page(
        &self,
        content_area: &ContentArea,
        page_width: f64,
        page_height: f64,
        content_ids: &[FormNodeId],
        strategy: LayoutStrategy,
    ) -> Result<LayoutPage> {
        let mut page = LayoutPage {
            width: page_width,
            height: page_height,
            nodes: Vec::new(),
            runtime_instantiated: false,
        };

        let available = Size {
            width: content_area.width,
            height: content_area.height,
        };

        let nodes = self.layout_children(content_ids, available, strategy)?;

        // Offset nodes to content area position
        for mut node in nodes {
            node.rect.x += content_area.x;
            node.rect.y += content_area.y;
            page.nodes.push(node);
        }

        Ok(page)
    }

    fn profile_page_from_nodes(
        &self,
        page: &LayoutPage,
        content_y: f64,
        usable_height: f64,
        overflow_to_next: bool,
        first_overflow_element: Option<String>,
    ) -> LayoutProfilePage {
        let used_height = page
            .nodes
            .iter()
            .map(|node| (node.rect.y + node.rect.height) - content_y)
            .fold(0.0_f64, f64::max)
            .clamp(0.0, usable_height.max(0.0));

        LayoutProfilePage {
            page_height: usable_height.max(0.0),
            used_height,
            overflow_to_next,
            first_overflow_element,
        }
    }

    /// Lay out page-area fixed nodes (headers, footers, lines) at their
    /// absolute positions and prepend them to the page's node list so they
    /// render behind flowing content.
    fn prepend_fixed_nodes(&self, fixed_ids: &[FormNodeId], page: &mut LayoutPage) -> Result<()> {
        if fixed_ids.is_empty() {
            return Ok(());
        }
        let fixed_laid = self.layout_positioned(fixed_ids)?;
        let mut merged = fixed_laid;
        merged.append(&mut page.nodes);
        page.nodes = merged;
        Ok(())
    }

    // TODO #1364 / #1376 audit-followup: refactor the 5-tuple return into a
    // dedicated `LayoutFittingResult` struct for readability. Suppressing
    // clippy::type_complexity here as an interim step so downstream crates
    // (notably pdf-xfa lib tests + clippy) build clean for security/policy
    // work — the M8 audit found this baseline blocked validation across
    // multiple PRs. Revisit when the layout-engine return shapes get a
    // proper type-design pass.
    #[allow(clippy::type_complexity)]
    fn layout_content_fitting(
        &self,
        content_area: &ContentArea,
        content_ids: &[QueuedNode],
        page_width: f64,
        page_height: f64,
        profile_enabled: bool,
    ) -> Result<FittingResult> {
        let mut page = LayoutPage {
            width: page_width,
            height: page_height,
            nodes: Vec::new(),
            runtime_instantiated: false,
        };

        // XFA Spec 3.3 §8.10 — Leaders and Trailers (p314-326).
        //
        // Current implementation: ✅ per-contentArea leader (placed at top) and
        // trailer (placed at bottom) on every page that uses the content area,
        // including overflow pages.
        //
        // Not yet implemented:
        //   - ⚠️ break leaders/trailers (appear only on page-break pages)
        //   - ⚠️ bookend leaders/trailers (appear on first/last occurrence pages)
        //   - ⚠️ overflow leaders/trailers with occurrence limits and inheritance
        //   - ⚠️ the `<overflow leader="..." trailer="...">` SOM-reference form
        //     (currently leaders/trailers must be set on ContentArea directly)
        let mut leader_height = 0.0;
        let mut trailer_height = 0.0;

        if let Some(leader_id) = content_area.leader {
            let leader_size = self.compute_extent(leader_id);
            leader_height = leader_size.height;
            let leader_node = self.form.get(leader_id);
            let node = self.layout_single_node(leader_id, leader_node, 0.0, 0.0, None)?;
            let mut offset = node;
            offset.rect.x += content_area.x;
            offset.rect.y += content_area.y;
            page.nodes.push(offset);
        }

        if let Some(trailer_id) = content_area.trailer {
            let trailer_size = self.compute_extent(trailer_id);
            trailer_height = trailer_size.height;
            // Trailer is placed at the bottom of the content area
            let trailer_y = content_area.height - trailer_height;
            let trailer_node = self.form.get(trailer_id);
            let node = self.layout_single_node(trailer_id, trailer_node, 0.0, trailer_y, None)?;
            let mut offset = node;
            offset.rect.x += content_area.x;
            offset.rect.y += content_area.y;
            page.nodes.push(offset);
        }

        // Available height for content = total - leader - trailer.
        // Content area height: only extend beyond the declared height if
        // the content area reaches close to the page bottom. When there is
        // significant space between the content area bottom edge and the
        // page bottom (typically occupied by page-area fixed elements like
        // footers), respect the declared height so content overflows to the
        // next page rather than overlapping fixed chrome.
        let effective_ca_height = {
            let ca_bottom = content_area.y + content_area.height;
            let gap_below_ca = page_height - ca_bottom;
            // Allow extension only if the gap below is less than 36pt (~0.5in).
            // Larger gaps indicate page-area fixed elements (footers, disclaimers)
            // that content should not overlap.
            if gap_below_ca < 36.0 {
                let remaining_page = page_height - content_area.y;
                content_area.height.max(remaining_page)
            } else {
                content_area.height
            }
        };
        let content_height = effective_ca_height - leader_height - trailer_height;
        let available = Size {
            width: content_area.width,
            height: content_height,
        };

        let mut y_cursor = leader_height;
        let mut placed_count = 0;
        let mut split_remaining: Vec<QueuedNode> = Vec::new();
        let content_bottom = leader_height + content_height;
        let mut consumed_break_only = false;
        let break_target = None;
        let mut max_used_bottom = 0.0_f64;

        // Count leader/trailer nodes placed so far (for force-place detection).
        let header_node_count = (if content_area.leader.is_some() { 1 } else { 0 })
            + (if content_area.trailer.is_some() { 1 } else { 0 });

        // Pre-compute a visible-node list for keep-chain look-ahead.
        // Each entry is (index-into-content_ids, FormNodeId).
        let visible_ids: Vec<(usize, FormNodeId)> = content_ids
            .iter()
            .enumerate()
            .filter(|(_, qn)| !self.is_layout_hidden(qn.id))
            .map(|(i, qn)| (i, qn.id))
            .collect();
        let mut vis_pos = 0; // current position in visible_ids

        self.trace_vertical_state(
            "layout_content_fitting",
            Some(y_cursor),
            Some(content_height),
            content_ids.first().map(|queued| queued.id),
            format!(
                "enter page_width={:.3} page_height={:.3} content_area={} content_nodes={} leader_height={:.3} trailer_height={:.3} effective_ca_height={:.3}",
                page_width,
                page_height,
                content_area.name,
                content_ids.len(),
                leader_height,
                trailer_height,
                effective_ca_height
            ),
        );
        self.trace_vertical_state(
            "layout_content_fitting",
            Some(y_cursor),
            Some(content_height),
            content_ids.first().map(|queued| queued.id),
            format!(
                "visible_ids prepared count={} event_type=runtime_allocation",
                visible_ids.len()
            ),
        );

        for (idx, qn) in content_ids.iter().enumerate() {
            let child_id = qn.id;

            // Skip layout-hidden nodes — they consume no space.
            if self.is_layout_hidden(child_id) {
                placed_count += 1;
                continue;
            }

            self.trace_vertical_state(
                "layout_content_fitting",
                Some(y_cursor),
                Some(content_bottom - y_cursor),
                Some(child_id),
                format!(
                    "loop entry idx={} placed_count={} vis_pos={} break_before={} break_after={}",
                    idx, placed_count, vis_pos, qn.break_before, qn.break_after
                ),
            );

            // Handle break_before: if this node requests a page break and
            // we already placed content on this page, stop here so the
            // caller starts a new page with this node.
            self.trace_break_before_read(
                y_cursor,
                content_bottom - y_cursor,
                child_id,
                qn.break_before,
                placed_count,
                header_node_count,
            );
            if qn.break_before && placed_count > 0 {
                // Check if all placed content is blank spacers — if so,
                // fold the blank page: mark as consumed_break_only so the
                // caller skips the empty page.
                let only_blanks = page.nodes.len() <= header_node_count;
                if only_blanks {
                    consumed_break_only = true;
                }
                break;
            }

            let child = self.form.get(child_id);
            let child_size = if let Some(ref override_lines) = qn.text_lines_override {
                let style_lh = self.form.meta(child_id).style.line_height_pt;
                let lh = style_lh.unwrap_or_else(|| child.font.line_height_pt());
                let w = self.compute_extent(child_id).width;
                Size {
                    width: w,
                    height: override_lines.len() as f64 * lh,
                }
            } else {
                self.compute_extent_with_available_and_override(
                    child_id,
                    Some(available),
                    qn.children_override.as_deref(),
                )
            };

            // Keep-chain look-ahead: if this node starts a keep chain and
            // the chain doesn't fit in remaining space (but WOULD fit on a
            // fresh page), break now so the chain starts on the next page.
            // Only applies to actual multi-node chains (chain_len > 1), OR
            // single non-splittable nodes.  A splittable single node should go
            // through normal overflow/split logic instead of being pushed to a
            // fresh page, which wastes space and causes over-pagination (#866).
            if placed_count > 0 && vis_pos < visible_ids.len() {
                let (chain_height, chain_len) =
                    self.visible_keep_chain_height(&visible_ids, vis_pos, available);
                let remaining_on_page = content_bottom - y_cursor;
                let is_single_splittable = chain_len == 1 && self.can_split(child_id);
                self.trace_vertical_state(
                    "layout_content_fitting",
                    Some(y_cursor),
                    Some(remaining_on_page),
                    Some(child_id),
                    format!(
                        "keep look-ahead chain_height={:.3} chain_len={} remaining_on_page={:.3} content_height={:.3} is_single_splittable={} event_type=read",
                        chain_height,
                        chain_len,
                        remaining_on_page,
                        content_height,
                        is_single_splittable
                    ),
                );
                if !is_single_splittable
                    && chain_height > remaining_on_page
                    && chain_height <= content_height
                {
                    // Chain (or non-splittable single node) fits on a fresh page — break now.
                    self.trace_vertical_state(
                        "layout_content_fitting",
                        Some(y_cursor),
                        Some(remaining_on_page),
                        Some(child_id),
                        "keep look-ahead fired: current chain does not fit remaining space but fits on a fresh page event_type=rule_decision",
                    );
                    break;
                }
                // Unsatisfiable keep chain (exceeds page height):
                // fall through and use child's own height for placement.
            }

            if y_cursor + child_size.height > content_bottom {
                let remaining_height = content_bottom - y_cursor;

                // §8.7 Text leaf splitting at page boundary.
                if remaining_height > 0.0
                    && (qn.text_lines_override.is_some() || self.is_splittable_text_leaf(child_id))
                {
                    let lines = if let Some(ref ol) = qn.text_lines_override {
                        ol.clone()
                    } else {
                        let txt = match &child.node_type {
                            FormNodeType::Draw(DrawContent::Text(t)) => t.as_str(),
                            FormNodeType::Field { value } => value.as_str(),
                            _ => "",
                        };
                        let child_style = &self.form.meta(child_id).style;
                        let para_margins = child_style
                            .margin_left_pt
                            .unwrap_or(crate::types::DEFAULT_TEXT_PADDING)
                            + child_style
                                .margin_right_pt
                                .unwrap_or(crate::types::DEFAULT_TEXT_PADDING);
                        let child_border_w = child_style
                            .border_width_pt
                            .unwrap_or(child.box_model.border_width);
                        let insets_w = child.box_model.margins.horizontal()
                            + child_border_w * 2.0
                            + para_margins;
                        let max_w = (child_size.width - insets_w).max(1.0);
                        text::wrap_text(
                            txt,
                            max_w,
                            &child.font,
                            child_style.text_indent_pt.unwrap_or(0.0),
                            child_style.line_height_pt,
                        )
                        .lines
                    };
                    let (partial, rest_nodes) =
                        self.split_text_node(child_id, y_cursor, remaining_height, &lines)?;
                    if partial.rect.height > 0.0 && partial.rect.height <= remaining_height + 1.0 {
                        if profile_enabled {
                            max_used_bottom = max_used_bottom
                                .max((y_cursor + partial.rect.height - leader_height).max(0.0));
                        }
                        let mut offset_node = partial;
                        offset_node.rect.x += content_area.x;
                        offset_node.rect.y += content_area.y;
                        page.nodes.push(offset_node);
                        placed_count += 1;
                        split_remaining = rest_nodes;
                    } else if placed_count > 0 {
                        break;
                    }
                // Try to split this node if it's a splittable container.
                } else if remaining_height > 0.0 && self.can_split(child_id) {
                    let (partial, rest_nodes) = self.split_tb_node(
                        child_id,
                        y_cursor,
                        remaining_height,
                        available,
                        qn.children_override.as_deref(),
                        qn.nested_child_overrides.as_deref(),
                    )?;

                    let partial_fits = partial.rect.height <= remaining_height + 1.0;
                    let split_productive = !partial.children.is_empty()
                        && (partial_fits || partial.children.len() > 1);
                    if !partial.children.is_empty() && (partial_fits || split_productive) {
                        if profile_enabled {
                            max_used_bottom = max_used_bottom
                                .max((y_cursor + partial.rect.height - leader_height).max(0.0));
                        }
                        let mut offset_node = partial;
                        offset_node.rect.x += content_area.x;
                        offset_node.rect.y += content_area.y;
                        page.nodes.push(offset_node);
                        placed_count += 1;
                        split_remaining = rest_nodes;
                    } else if placed_count > 0 {
                        // Single oversized child doesn't fit and page has
                        // content — defer to a fresh page where re-split
                        // with full height can do better.
                        break;
                    }
                } else if idx == 0 || page.nodes.len() <= header_node_count {
                    // First content item too large and can't split — force place it
                    let x = self.child_h_align_offset(child_id, child_size.width, available.width);
                    if profile_enabled {
                        max_used_bottom = max_used_bottom
                            .max((y_cursor + child_size.height - leader_height).max(0.0));
                    }
                    let node = self.layout_single_node_with_extent(
                        child_id,
                        child,
                        x,
                        y_cursor,
                        child_size,
                        qn.children_override.as_deref(),
                    )?;
                    let mut offset_node = node;
                    offset_node.rect.x += content_area.x;
                    offset_node.rect.y += content_area.y;
                    page.nodes.push(offset_node);
                    placed_count += 1;
                }
                break;
            }

            // Proactive split: if the child fits on the page but contains
            // inner page-break-before children, split it using the FULL
            // content height so the inner break is detected.
            if self.has_inner_break(child_id) && self.can_split(child_id) {
                let (partial, rest_nodes) = self.split_tb_node(
                    child_id,
                    y_cursor,
                    content_height,
                    available,
                    qn.children_override.as_deref(),
                    qn.nested_child_overrides.as_deref(),
                )?;
                let remaining_on_page = content_bottom - y_cursor;
                if !partial.children.is_empty() && partial.rect.height <= remaining_on_page {
                    if profile_enabled {
                        max_used_bottom = max_used_bottom
                            .max((y_cursor + partial.rect.height - leader_height).max(0.0));
                    }
                    let mut offset_node = partial;
                    offset_node.rect.x += content_area.x;
                    offset_node.rect.y += content_area.y;
                    page.nodes.push(offset_node);
                    placed_count += 1;
                    split_remaining = rest_nodes;
                } else if placed_count > 0 {
                    break;
                } else {
                    if !partial.children.is_empty() {
                        if profile_enabled {
                            max_used_bottom = max_used_bottom
                                .max((y_cursor + partial.rect.height - leader_height).max(0.0));
                        }
                        let mut offset_node = partial;
                        offset_node.rect.x += content_area.x;
                        offset_node.rect.y += content_area.y;
                        page.nodes.push(offset_node);
                    }
                    placed_count += 1;
                    split_remaining = rest_nodes;
                }
                break;
            }

            // XFA Spec 3.3 §8.3 — hAlign positions child within content area
            let x = self.child_h_align_offset(child_id, child_size.width, available.width);
            let node = if let Some(ref override_lines) = qn.text_lines_override {
                let child_style = &self.form.meta(child_id).style;
                LayoutNode {
                    form_node: child_id,
                    rect: Rect::new(x, y_cursor, child_size.width, child_size.height),
                    name: child.name.clone(),
                    content: LayoutContent::WrappedText {
                        lines: override_lines.clone(),
                        first_line_of_para: vec![false; override_lines.len()],
                        font_size: child.font.size,
                        text_align: child.font.text_align,
                        font_family: child.font.typeface,
                        space_above_pt: child_style.space_above_pt,
                        space_below_pt: child_style.space_below_pt,
                        from_field: matches!(child.node_type, FormNodeType::Field { .. }),
                    },
                    children: Vec::new(),
                    style: self.form.meta(child_id).style.clone(),
                    display_items: self.form.meta(child_id).display_items.clone(),
                    save_items: self.form.meta(child_id).save_items.clone(),
                }
            } else {
                self.layout_single_node_with_extent(
                    child_id,
                    child,
                    x,
                    y_cursor,
                    child_size,
                    qn.children_override.as_deref(),
                )?
            };
            if profile_enabled {
                max_used_bottom =
                    max_used_bottom.max((y_cursor + child_size.height - leader_height).max(0.0));
            }
            let mut offset_node = node;
            offset_node.rect.x += content_area.x;
            offset_node.rect.y += content_area.y;
            page.nodes.push(offset_node);

            y_cursor += child_size.height;
            placed_count += 1;
            vis_pos += 1;

            self.trace_vertical_state(
                "layout_content_fitting",
                Some(y_cursor),
                Some(content_bottom - y_cursor),
                Some(child_id),
                format!(
                    "placed node child_height={:.3} new_y_cursor={:.3} placed_count={} vis_pos={}",
                    child_size.height, y_cursor, placed_count, vis_pos
                ),
            );

            if qn.break_after {
                self.trace_vertical_state(
                    "layout_content_fitting",
                    Some(y_cursor),
                    Some(content_bottom - y_cursor),
                    Some(child_id),
                    format!("break_after check break_after={}", qn.break_after),
                );
                break;
            }
        }

        let mut remaining = split_remaining;
        remaining.extend(content_ids[placed_count..].iter().cloned());
        self.trace_vertical_state(
            "layout_content_fitting",
            Some(y_cursor),
            Some(content_bottom - y_cursor),
            remaining.first().map(|queued| queued.id),
            format!(
                "return remaining_nodes={} consumed_break_only={} first_remaining={}",
                remaining.len(),
                consumed_break_only,
                remaining
                    .first()
                    .map(|queued| self.describe_queued_node(queued))
                    .unwrap_or_else(|| "none".to_string())
            ),
        );
        let page_profile = if profile_enabled {
            Some(LayoutProfilePage {
                page_height: content_height.max(0.0),
                used_height: max_used_bottom.clamp(0.0, content_height.max(0.0)),
                overflow_to_next: !remaining.is_empty() && !consumed_break_only,
                first_overflow_element: if !remaining.is_empty() && !consumed_break_only {
                    Some(self.describe_queued_node(&remaining[0]))
                } else {
                    None
                },
            })
        } else {
            None
        };
        Ok((
            page,
            remaining,
            consumed_break_only,
            break_target,
            page_profile,
        ))
    }

    fn describe_queued_node(&self, queued: &QueuedNode) -> String {
        let node = self.form.get(queued.id);
        let identifier = self
            .form
            .meta(queued.id)
            .xfa_id
            .clone()
            .filter(|id| !id.is_empty())
            .or_else(|| (!node.name.is_empty()).then(|| node.name.clone()))
            .unwrap_or_else(|| queued.id.0.to_string());
        let height = self
            .compute_extent_with_available_and_override(
                queued.id,
                None,
                queued.children_override.as_deref(),
            )
            .height;
        format!(
            "{}#{} (h={height:.1})",
            Self::form_node_type_name(&node.node_type),
            identifier
        )
    }

    fn form_node_type_name(node_type: &FormNodeType) -> &'static str {
        match node_type {
            FormNodeType::Root => "root",
            FormNodeType::PageSet => "pageSet",
            FormNodeType::PageArea { .. } => "pageArea",
            FormNodeType::Subform => "subform",
            FormNodeType::Area => "area",
            FormNodeType::ExclGroup => "exclGroup",
            FormNodeType::SubformSet => "subformSet",
            FormNodeType::Field { .. } => "field",
            FormNodeType::Draw(_) => "draw",
            FormNodeType::Image { .. } => "image",
        }
    }

    /// XFA Spec 3.3 §8.7 — Content Splitting (p290): determines whether a node
    /// can be split across pages. Per Appendix B (p1520), subforms are splittable
    /// "in margins and where consensus exists among contained objects".
    ///
    /// Split restrictions (§8.7 p291): barcode, geometric figure, image = no split.
    /// Text = split between lines only. Widget = no split.
    /// keep.intact controls: none (free), contentArea (within CA), pageArea (within page).
    ///
    /// TODO §8.7: text-level splitting (between lines), orphan/widow controls,
    /// split consensus algorithm (p294), per-type default intact values.
    fn can_split(&self, id: FormNodeId) -> bool {
        let node = self.form.get(id);
        if node.children.is_empty() {
            return self.is_splittable_text_leaf(id);
        }
        if let Some(explicit_h) = node.box_model.height {
            // TB subforms with explicit height: allow splitting only when
            // content actually overflows the declared height (#768).
            if node.layout == LayoutStrategy::TopToBottom {
                let extent = self.compute_extent(id);
                return extent.height > explicit_h;
            }
            return false;
        }
        // (#764) Positioned subforms with a finite maxH are bounded — treat
        // them like explicit-height nodes and don't split.
        if node.layout == LayoutStrategy::Positioned && node.box_model.max_height < f64::MAX {
            return false;
        }
        matches!(
            node.layout,
            LayoutStrategy::TopToBottom
                | LayoutStrategy::LeftToRightTB
                | LayoutStrategy::RightToLeftTB
                | LayoutStrategy::Table
                | LayoutStrategy::Positioned
        )
    }

    /// Check if a childless node is a text leaf that can be split between lines.
    ///
    /// XFA Spec 3.3 §8.7 (p291): text may be split between lines.
    /// Images, barcodes, geometric figures, and widgets may NOT be split.
    fn is_splittable_text_leaf(&self, id: FormNodeId) -> bool {
        let node = self.form.get(id);
        if !node.children.is_empty() {
            return false;
        }
        if self.form.meta(id).keep_intact_content_area {
            return false;
        }
        let style = &self.form.meta(id).style;
        let para_margins = style
            .margin_left_pt
            .unwrap_or(crate::types::DEFAULT_TEXT_PADDING)
            + style
                .margin_right_pt
                .unwrap_or(crate::types::DEFAULT_TEXT_PADDING);
        match &node.node_type {
            FormNodeType::Draw(DrawContent::Text(t)) => {
                let line_count = text::wrap_text(
                    t,
                    (node.box_model.content_width() - para_margins).max(1.0),
                    &node.font,
                    style.text_indent_pt.unwrap_or(0.0),
                    style.line_height_pt,
                )
                .lines
                .len();
                line_count > 1
            }
            FormNodeType::Field { value } if !value.is_empty() => {
                let line_count = text::wrap_text(
                    value,
                    (node.box_model.content_width() - para_margins).max(1.0),
                    &node.font,
                    style.text_indent_pt.unwrap_or(0.0),
                    style.line_height_pt,
                )
                .lines
                .len();
                line_count > 1
            }
            _ => false,
        }
    }

    /// Split a text leaf node at a line boundary so the first portion fits
    /// within `remaining_height`.
    ///
    /// XFA Spec 3.3 §8.7 (p291): text may be split between lines only.
    fn split_text_node(
        &self,
        id: FormNodeId,
        y_offset: f64,
        remaining_height: f64,
        lines: &[String],
    ) -> Result<(LayoutNode, Vec<QueuedNode>)> {
        let node = self.form.get(id);
        let style_lh = self.form.meta(id).style.line_height_pt;
        let lh = style_lh.unwrap_or_else(|| node.font.line_height_pt());
        let split_points = text::text_split_points(lines.len(), lh);

        let mut split_at = 0;
        for &sp in &split_points {
            if sp <= remaining_height + 0.5 {
                split_at += 1;
            } else {
                break;
            }
        }

        if split_at == 0 {
            let full_height = lines.len() as f64 * lh;
            let split_style = &self.form.meta(id).style;
            let full_node = LayoutNode {
                form_node: id,
                rect: Rect::new(0.0, y_offset, self.compute_extent(id).width, full_height),
                name: node.name.clone(),
                content: LayoutContent::WrappedText {
                    lines: lines.to_vec(),
                    first_line_of_para: vec![false; lines.len()],
                    font_size: node.font.size,
                    text_align: node.font.text_align,
                    font_family: node.font.typeface,
                    space_above_pt: split_style.space_above_pt,
                    space_below_pt: split_style.space_below_pt,
                    from_field: matches!(node.node_type, FormNodeType::Field { .. }),
                },
                children: Vec::new(),
                style: self.form.meta(id).style.clone(),
                display_items: self.form.meta(id).display_items.clone(),
                save_items: self.form.meta(id).save_items.clone(),
            };
            return Ok((full_node, Vec::new()));
        }

        let top_lines: Vec<String> = lines[..split_at].to_vec();
        let bottom_lines: Vec<String> = lines[split_at..].to_vec();
        let partial_height = split_at as f64 * lh;
        let node_width = self.compute_extent(id).width;
        let split_style = &self.form.meta(id).style;

        let partial_node = LayoutNode {
            form_node: id,
            rect: Rect::new(0.0, y_offset, node_width, partial_height),
            name: node.name.clone(),
            content: LayoutContent::WrappedText {
                lines: top_lines.clone(),
                first_line_of_para: vec![false; top_lines.len()],
                font_size: node.font.size,
                text_align: node.font.text_align,
                font_family: node.font.typeface,
                space_above_pt: split_style.space_above_pt,
                space_below_pt: split_style.space_below_pt,
                from_field: matches!(node.node_type, FormNodeType::Field { .. }),
            },
            children: Vec::new(),
            style: self.form.meta(id).style.clone(),
            display_items: self.form.meta(id).display_items.clone(),
            save_items: self.form.meta(id).save_items.clone(),
        };

        let rest = if bottom_lines.is_empty() {
            Vec::new()
        } else {
            vec![QueuedNode {
                id,
                break_before: false,
                break_after: self.form.meta(id).page_break_after,
                break_target: None,
                children_override: None,
                text_lines_override: Some(bottom_lines),
                nested_child_overrides: None,
            }]
        };

        Ok((partial_node, rest))
    }

    /// Check if any direct (expanded) child of a tb-layout subform has
    /// `page_break_before` set, meaning the node must be split at that
    /// point even if it fits in the remaining space.
    fn has_inner_break(&self, id: FormNodeId) -> bool {
        let node = self.form.get(id);
        if !matches!(
            node.layout,
            LayoutStrategy::TopToBottom
                | LayoutStrategy::LeftToRightTB
                | LayoutStrategy::RightToLeftTB
        ) {
            return false;
        }
        let expanded = self.expand_occur(&node.children);
        expanded
            .iter()
            .any(|&cid| self.form.meta(cid).page_break_before)
    }

    /// XFA Spec 3.3 §8.7 — Content Splitting (p290-294): split a tb-layout node
    /// by placing children that fit in `remaining_height`, returning the rest.
    ///
    /// Per §8.7 p294 "split consensus": the split location should be the lowest
    /// point acceptable to all contained objects. Our implementation splits at
    /// child boundaries (not within text lines).
    ///
    /// Respects keep constraints (§8.9 Adhesion): if a child has
    /// `keep_next_content_area`, the split will not occur between that child
    /// and its successor. Also respects `page_break_before` as mandatory splits.
    fn split_tb_node(
        &self,
        id: FormNodeId,
        y_offset: f64,
        remaining_height: f64,
        _available: Size,
        children_override: Option<&[FormNodeId]>,
        nested_overrides: Option<&[(FormNodeId, Vec<FormNodeId>)]>,
    ) -> Result<(LayoutNode, Vec<QueuedNode>)> {
        let node = self.form.get(id);

        // Delegate to positioned splitter for positioned subforms (#736).
        if node.layout == LayoutStrategy::Positioned {
            return self.split_positioned_node(id, y_offset, remaining_height, children_override);
        }

        let node_children = children_override.unwrap_or(&node.children);
        // (#764) When children_override is set, the list already contains
        // occur-expanded IDs from a previous split.  Re-expanding would
        // duplicate children on every page, causing infinite pagination.
        let expanded_children = if children_override.is_some() {
            node_children.to_vec()
        } else {
            self.expand_occur(node_children)
        };

        let mut placed_children = Vec::new();
        let mut child_y = 0.0;
        let mut split_idx = 0;
        // Track the last valid split point (respecting keep constraints).
        let mut last_valid_split = 0;
        let mut last_valid_y = 0.0_f64;
        let mut split_rest_override: Option<Vec<QueuedNode>> = None;

        for (i, &child_id) in expanded_children.iter().enumerate() {
            let child = self.form.get(child_id);
            // Look up per-child override from a previous nested split so
            // that partially-split children use their reduced children list.
            let child_co = nested_overrides
                .and_then(|no| no.iter().find(|(nid, _)| *nid == child_id))
                .map(|(_, co)| co.as_slice());
            let child_size = self.compute_extent_with_override(child_id, child_co);
            let child_meta = self.form.meta(child_id);

            // If this child has keep_intact and doesn't fit, split BEFORE it
            // so it moves to the next page entirely.
            if child_meta.keep_intact_content_area
                && child_y + child_size.height > remaining_height
                && !placed_children.is_empty()
            {
                split_idx = i;
                break;
            }

            // §8.7 Text leaf splitting: if the overflowing child is a text
            // leaf, split at a line boundary instead of a child boundary.
            if child_y + child_size.height > remaining_height
                && !child_meta.keep_intact_content_area
                && self.is_splittable_text_leaf(child_id)
            {
                let child_remaining = (remaining_height - child_y).max(0.0);
                if child_remaining > 0.0 {
                    let cnode = self.form.get(child_id);
                    let txt = match &cnode.node_type {
                        FormNodeType::Draw(DrawContent::Text(t)) => t.as_str(),
                        FormNodeType::Field { value } => value.as_str(),
                        _ => "",
                    };
                    let cstyle = &self.form.meta(child_id).style;
                    let cpara = cstyle
                        .margin_left_pt
                        .unwrap_or(crate::types::DEFAULT_TEXT_PADDING)
                        + cstyle
                            .margin_right_pt
                            .unwrap_or(crate::types::DEFAULT_TEXT_PADDING);
                    let cborder_w = cstyle
                        .border_width_pt
                        .unwrap_or(cnode.box_model.border_width);
                    let insets_w = cnode.box_model.margins.horizontal() + cborder_w * 2.0 + cpara;
                    let max_w = (self.compute_extent(child_id).width - insets_w).max(1.0);
                    let wrapped = text::wrap_text(
                        txt,
                        max_w,
                        &cnode.font,
                        cstyle.text_indent_pt.unwrap_or(0.0),
                        cstyle.line_height_pt,
                    );
                    let (partial_child, child_rest) =
                        self.split_text_node(child_id, child_y, child_remaining, &wrapped.lines)?;

                    if partial_child.rect.height > 0.0
                        && partial_child.rect.height <= child_remaining + 0.5
                    {
                        placed_children.push(partial_child);
                        // SAFETY: we just pushed partial_child above, so last() is always Some.
                        child_y += placed_children
                            .last()
                            .expect("just pushed above")
                            .rect
                            .height;
                        split_idx = i + 1;

                        let mut rest: Vec<QueuedNode> = child_rest;
                        rest.extend(expanded_children[i + 1..].iter().map(|&cid| QueuedNode {
                            id: cid,
                            break_before: self.form.meta(cid).page_break_before,
                            break_after: self.form.meta(cid).page_break_after,
                            break_target: None,
                            children_override: None,
                            text_lines_override: None,
                            nested_child_overrides: None,
                        }));
                        split_rest_override = Some(rest);
                        break;
                    }
                }
            }

            // If the next child itself is a splittable container and it is
            // the first overflowing child, split it recursively instead of
            // forcing the entire container onto a single page.
            if child_y + child_size.height > remaining_height
                && !child_meta.keep_intact_content_area
                && self.can_split(child_id)
            {
                let child_remaining = (remaining_height - child_y).max(0.0);
                if child_remaining > 0.0 {
                    let child_available = Size {
                        width: child.box_model.content_width().min(child_size.width),
                        height: child.box_model.content_height().min(child_size.height),
                    };
                    let (partial_child, child_rest) = self.split_tb_node(
                        child_id,
                        child_y,
                        child_remaining,
                        child_available,
                        child_co,
                        nested_overrides,
                    )?;

                    let partial_fits = partial_child.rect.height <= child_remaining + 1.0;
                    let split_productive = !partial_child.children.is_empty()
                        && (partial_fits || partial_child.children.len() > 1);

                    if split_productive {
                        placed_children.push(partial_child);
                        // SAFETY: we just pushed partial_child above, so last() is always Some.
                        child_y += placed_children
                            .last()
                            .expect("just pushed above")
                            .rect
                            .height;
                        split_idx = i + 1;

                        // When the recursive split returns a single QueuedNode
                        // that wraps the same parent (e.g. a positioned subform
                        // split via split_positioned_node), preserve its
                        // children_override so the next page only processes the
                        // remaining children.  Without this, the override is
                        // discarded and the full subform is re-split every page,
                        // causing an infinite pagination loop (#737).
                        //
                        // For other cases (multiple QueuedNodes, or a single
                        // node for a different child), collect IDs as before
                        // but also preserve any children_override from each
                        // node as nested_child_overrides.  This prevents
                        // duplication when a deeply nested child was partially
                        // split — without this, its override is lost and the
                        // child re-renders all content on the next page.
                        let (child_override, child_nested) = if child_rest.len() == 1
                            && child_rest[0].id == child_id
                            && child_rest[0].children_override.is_some()
                        {
                            // SAFETY: child_rest.len() == 1 is checked on the line above.
                            let qn = child_rest
                                .into_iter()
                                .next()
                                .expect("child_rest.len() == 1");
                            (qn.children_override, qn.nested_child_overrides)
                        } else {
                            let mut ids = Vec::new();
                            let mut nested = Vec::new();
                            for qn in child_rest {
                                ids.push(qn.id);
                                if let Some(co) = qn.children_override {
                                    nested.push((qn.id, co));
                                }
                                if let Some(nco) = qn.nested_child_overrides {
                                    nested.extend(nco);
                                }
                            }
                            (
                                Some(ids),
                                if nested.is_empty() {
                                    None
                                } else {
                                    Some(nested)
                                },
                            )
                        };

                        let mut rest = vec![QueuedNode {
                            id: child_id,
                            break_before: false,
                            break_after: self.form.meta(child_id).page_break_after,
                            break_target: None,
                            children_override: child_override,
                            text_lines_override: None,
                            nested_child_overrides: child_nested,
                        }];
                        rest.extend(expanded_children[i + 1..].iter().map(|&cid| QueuedNode {
                            id: cid,
                            break_before: self.form.meta(cid).page_break_before,
                            break_after: self.form.meta(cid).page_break_after,
                            break_target: None,
                            children_override: None,
                            text_lines_override: None,
                            nested_child_overrides: None,
                        }));
                        split_rest_override = Some(rest);
                        break;
                    }
                }
            }

            // Overflow detection: child doesn't fit in remaining space.
            // 0.5pt tolerance for sub-point rounding (#971).
            if child_y + child_size.height > remaining_height + 0.5 && !placed_children.is_empty() {
                // Overflow: split at the last valid split point.
                if last_valid_split > 0 && last_valid_split < placed_children.len() {
                    // Trim placed_children to the last valid split point.
                    placed_children.truncate(last_valid_split);
                    child_y = last_valid_y;
                    split_idx = last_valid_split;
                } else {
                    split_idx = i;
                }
                break;
            }

            let child_node = self.layout_single_node(child_id, child, 0.0, child_y, child_co)?;
            placed_children.push(child_node);
            child_y += child_size.height;
            split_idx = i + 1;

            // Check if this is a valid split point (no keep constraint).
            let has_keep = child_meta.keep_next_content_area;
            let next_has_keep_prev = if i + 1 < expanded_children.len() {
                self.form
                    .meta(expanded_children[i + 1])
                    .keep_previous_content_area
            } else {
                false
            };
            if !has_keep && !next_has_keep_prev {
                last_valid_split = split_idx;
                last_valid_y = child_y;
            }
        }

        let content = match &node.node_type {
            FormNodeType::Field { value } => {
                let meta = self.form.meta(id);
                LayoutContent::Field {
                    value: resolve_display_value(value, meta).to_string(),
                    field_kind: meta.field_kind,
                    font_size: node.font.size,
                    font_family: node.font.typeface,
                }
            }
            FormNodeType::Draw(DrawContent::Text(content)) => LayoutContent::Text(content.clone()),
            FormNodeType::Draw(dc) => LayoutContent::Draw(dc.clone()),
            FormNodeType::Image { data, mime_type } => LayoutContent::Image {
                data: data.clone(),
                mime_type: mime_type.clone(),
            },
            _ => LayoutContent::None,
        };

        // Compute partial extent: full width, height = content that fit
        let partial_width = self
            .compute_extent_with_override(id, children_override)
            .width;

        let partial_node = LayoutNode {
            form_node: id,
            rect: Rect::new(0.0, y_offset, partial_width, child_y),
            name: node.name.clone(),
            content,
            children: placed_children,
            style: self.form.meta(id).style.clone(),
            display_items: self.form.meta(id).display_items.clone(),
            save_items: self.form.meta(id).save_items.clone(),
        };

        let rest = split_rest_override.unwrap_or_else(|| {
            expanded_children[split_idx..]
                .iter()
                .map(|&cid| QueuedNode {
                    id: cid,
                    break_before: self.form.meta(cid).page_break_before,
                    break_after: self.form.meta(cid).page_break_after,
                    break_target: None,
                    children_override: None,
                    text_lines_override: None,
                    nested_child_overrides: None,
                })
                .collect()
        });
        Ok((partial_node, rest))
    }

    /// Split a positioned-layout subform across pages (#736).
    ///
    /// Children are sorted by their y-coordinate.  Those whose bottom edge
    /// (y + height, shifted by y_base) fits within `remaining_height` are
    /// placed on the current page.  Remaining children are returned for
    /// layout on subsequent pages.
    ///
    /// When called for overflow children (via `children_override`), a y_base
    /// shift is computed from the minimum y-position so they render starting
    /// near the top of the new page.  The same shift is applied by
    /// `compute_extent_with_available_and_override` so the reported height
    /// is consistent.
    fn split_positioned_node(
        &self,
        id: FormNodeId,
        y_offset: f64,
        remaining_height: f64,
        children_override: Option<&[FormNodeId]>,
    ) -> Result<(LayoutNode, Vec<QueuedNode>)> {
        let node = self.form.get(id);
        let node_children = children_override.unwrap_or(&node.children);
        // (#764) Skip occur re-expansion when override is set — it already
        // contains expanded IDs from the previous split.
        let expanded_children = if children_override.is_some() {
            node_children.to_vec()
        } else {
            self.expand_occur(node_children)
        };

        // Sort children by their y-position for deterministic splitting.
        let mut sorted: Vec<FormNodeId> = expanded_children.clone();
        sorted.sort_by(|&a, &b| {
            let ay = self.form.get(a).box_model.y;
            let by = self.form.get(b).box_model.y;
            ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal)
        });

        // When processing overflow children (children_override is set),
        // shift all y-positions so the topmost child starts at y=0.
        // This mirrors the y_base logic in compute_extent.
        let y_base = if children_override.is_some() {
            sorted
                .first()
                .map(|&cid| self.form.get(cid).box_model.y)
                .unwrap_or(0.0)
        } else {
            0.0
        };
        let parent_margin_x = node.box_model.margins.left;
        let parent_margin_y = node.box_model.margins.top;

        let mut placed_children = Vec::new();
        let mut rest_children = Vec::new();
        let mut max_placed_bottom = 0.0_f64;

        for &child_id in &sorted {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent(child_id);
            let shifted_y = child.box_model.y - y_base + parent_margin_y;
            let child_bottom = shifted_y + child_size.height;

            if child_bottom <= remaining_height + 1.0 {
                // Child fits on this page -- place at shifted position.
                let child_node = self.layout_single_node(
                    child_id,
                    child,
                    child.box_model.x + parent_margin_x,
                    shifted_y,
                    None,
                )?;
                max_placed_bottom = max_placed_bottom.max(child_bottom);
                placed_children.push(child_node);
            } else {
                // Child overflows -- defer to next page.
                rest_children.push(child_id);
            }
        }

        // (#764) Ensure progress: if nothing was placed, force-place the
        // first child so the split always advances.  Without this guard,
        // positioned subforms whose first child exceeds the remaining
        // height return an empty partial on every page, causing the
        // caller to loop indefinitely.
        if placed_children.is_empty() && !sorted.is_empty() {
            let first_id = sorted[0];
            let first = self.form.get(first_id);
            let first_size = self.compute_extent(first_id);
            let shifted_y = first.box_model.y - y_base + parent_margin_y;
            let child_node = self.layout_single_node(
                first_id,
                first,
                first.box_model.x + parent_margin_x,
                shifted_y,
                None,
            )?;
            max_placed_bottom = shifted_y + first_size.height;
            placed_children.push(child_node);
            // Remove the force-placed child from rest.
            rest_children.retain(|&cid| cid != first_id);
        }

        let content = match &node.node_type {
            FormNodeType::Field { value } => {
                let meta = self.form.meta(id);
                LayoutContent::Field {
                    value: resolve_display_value(value, meta).to_string(),
                    field_kind: meta.field_kind,
                    font_size: node.font.size,
                    font_family: node.font.typeface,
                }
            }
            FormNodeType::Draw(DrawContent::Text(content)) => LayoutContent::Text(content.clone()),
            FormNodeType::Draw(dc) => LayoutContent::Draw(dc.clone()),
            FormNodeType::Image { data, mime_type } => LayoutContent::Image {
                data: data.clone(),
                mime_type: mime_type.clone(),
            },
            _ => LayoutContent::None,
        };

        let partial_width = self
            .compute_extent_with_override(id, children_override)
            .width;

        let partial_node = LayoutNode {
            form_node: id,
            rect: Rect::new(0.0, y_offset, partial_width, max_placed_bottom),
            name: node.name.clone(),
            content,
            children: placed_children,
            style: self.form.meta(id).style.clone(),
            display_items: self.form.meta(id).display_items.clone(),
            save_items: self.form.meta(id).save_items.clone(),
        };

        // Remaining children are wrapped in a QueuedNode for the same
        // parent with children_override.  On the next page, compute_extent
        // will apply y_base shifting to produce a correct relative height,
        // and split_positioned_node will shift y-positions so children
        // render near the top of the new page.
        let rest = if rest_children.is_empty() {
            Vec::new()
        } else {
            vec![QueuedNode {
                id,
                break_before: false,
                break_after: self.form.meta(id).page_break_after,
                break_target: None,
                children_override: Some(rest_children),
                text_lines_override: None,
                nested_child_overrides: None,
            }]
        };

        Ok((partial_node, rest))
    }

    /// Layout children within available space using the given strategy.
    ///
    /// Children with `occur.count() > 1` are expanded into multiple instances.
    /// Primary layout hot path.
    ///
    /// Performance: this function is called recursively for every container in
    /// the form tree (n = total nodes) and for every page during pagination
    /// (m = pages).  The effective complexity is O(n * m) in the worst case
    /// (e.g. 100-occurrence repeating subforms with overflow across 50 pages).
    ///
    /// Known hotspot: `expand_occur` allocates a new Vec on every call.
    /// Memoizing subform heights for repeated occurrences of the same
    /// `FormNodeId` would eliminate redundant `compute_extent` calls.
    fn layout_children(
        &self,
        children: &[FormNodeId],
        available: Size,
        strategy: LayoutStrategy,
    ) -> Result<Vec<LayoutNode>> {
        let expanded = self.expand_occur(children);
        match strategy {
            LayoutStrategy::Positioned => self.layout_positioned(&expanded),
            LayoutStrategy::TopToBottom => self.layout_tb(&expanded, available),
            LayoutStrategy::LeftToRightTB => self.layout_lr_tb(&expanded, available),
            LayoutStrategy::RightToLeftTB => self.layout_rl_tb(&expanded, available),
            LayoutStrategy::Table => self.layout_table(&expanded, available),
            LayoutStrategy::Row => self.layout_row(&expanded, available),
        }
    }

    /// Expand children based on occur rules.
    ///
    /// A child with `occur.count() == 3` produces three entries in the output.
    /// Each entry refers to the same FormNodeId (the template), which the layout
    /// engine treats as separate instances at different positions.
    ///
    /// Nodes with non-visible presence (hidden/invisible/inactive) are skipped
    /// entirely — they consume no layout space (Adobe empirical, fixes #806).
    // XFA Spec 3.3 §7.4 / §9.2 — Repeating Elements using Occurrence Limits:
    // At layout time, the occur.count() (= initial for empty merge, or
    // data-driven count) determines how many copies appear.  Blank repeating
    // subforms are capped at occur.min to avoid empty rows (#701).
    fn expand_occur(&self, children: &[FormNodeId]) -> Vec<FormNodeId> {
        let mut expanded = Vec::new();
        for &child_id in children {
            if self.is_layout_hidden(child_id) {
                continue;
            }
            let child = self.form.get(child_id);
            // #701: limit blank repeating subforms to occur.min
            let count = if child.occur.is_repeating()
                && child.occur.count() > child.occur.min
                && self.has_field_descendants(child_id)
                && self.subtree_is_blank(child_id)
            {
                child.occur.min
            } else {
                child.occur.count()
            };
            // #865: Script-controlled pages have occur min=0/max=0/initial=0.
            // Without a script engine, these would be invisible.  Show them
            // once so the static content is rendered.
            let count = if count == 0
                && matches!(
                    child.node_type,
                    FormNodeType::Subform | FormNodeType::Area | FormNodeType::ExclGroup
                )
                && self.has_field_descendants(child_id)
            {
                1
            } else {
                count
            };
            for _ in 0..count {
                expanded.push(child_id);
            }
        }
        expanded
    }

    /// Returns true if the subtree contains at least one Field/Draw/Image node.
    fn has_field_descendants(&self, id: FormNodeId) -> bool {
        let node = self.form.get(id);
        match &node.node_type {
            FormNodeType::Field { .. } | FormNodeType::Draw(..) | FormNodeType::Image { .. } => {
                true
            }
            // Area, ExclGroup, SubformSet: recurse like Subform.
            FormNodeType::Subform
            | FormNodeType::Area
            | FormNodeType::ExclGroup
            | FormNodeType::SubformSet => {
                node.children.iter().any(|&c| self.has_field_descendants(c))
            }
            _ => false,
        }
    }

    /// XFA Spec 3.3 §8.2 — Positioned Layout: each child uses its own x,y
    /// coordinates. pageArea and contentArea always use positioned layout.
    /// Subforms default to positioned when no `layout` attribute is present.
    ///
    /// §2.6 + Appendix A (p1510): the `anchorType` attribute determines which
    /// point of the element's nominal extent is placed at (x,y).  The default
    /// is `topLeft` (no adjustment).
    fn layout_positioned(&self, children: &[FormNodeId]) -> Result<Vec<LayoutNode>> {
        use crate::form::AnchorType;

        let mut nodes = Vec::new();
        for &child_id in children {
            let child = self.form.get(child_id);
            let anchor = self.form.meta(child_id).anchor_type;

            // Compute nominal extent so we can offset for the anchor point.
            let extent = self.compute_extent(child_id);
            let w = extent.width;
            let h = extent.height;

            let (dx, dy) = match anchor {
                AnchorType::TopLeft => (0.0, 0.0),
                AnchorType::TopCenter => (-w / 2.0, 0.0),
                AnchorType::TopRight => (-w, 0.0),
                AnchorType::MiddleLeft => (0.0, -h / 2.0),
                AnchorType::MiddleCenter => (-w / 2.0, -h / 2.0),
                AnchorType::MiddleRight => (-w, -h / 2.0),
                AnchorType::BottomLeft => (0.0, -h),
                AnchorType::BottomCenter => (-w / 2.0, -h),
                AnchorType::BottomRight => (-w, -h),
            };

            let node = self.layout_single_node_with_extent(
                child_id,
                child,
                child.box_model.x + dx,
                child.box_model.y + dy,
                extent,
                None,
            )?;
            nodes.push(node);
        }
        Ok(nodes)
    }

    /// XFA Spec 3.3 §8.3 (p282-284) — compute x offset for a child based on
    /// its `<para hAlign>` within the parent container width.
    fn child_h_align_offset(&self, child_id: FormNodeId, child_w: f64, parent_w: f64) -> f64 {
        match self.form.meta(child_id).style.h_align {
            Some(TextAlign::Center) => ((parent_w - child_w) / 2.0).max(0.0),
            Some(TextAlign::Right) => (parent_w - child_w).max(0.0),
            _ => 0.0,
        }
    }

    /// Shift all nodes in a completed LR-TB row by the hAlign-derived offset.
    /// Uses the first node's hAlign to determine the row alignment
    /// (XFA Spec 3.3 §8.3 Example 8.12, p284).
    fn shift_row_h_align(&self, row: &mut [LayoutNode], row_width: f64, parent_width: f64) {
        let align = row
            .first()
            .and_then(|n| self.form.meta(n.form_node).style.h_align);
        let offset = match align {
            Some(TextAlign::Center) => ((parent_width - row_width) / 2.0).max(0.0),
            Some(TextAlign::Right) => (parent_width - row_width).max(0.0),
            _ => return,
        };
        for node in row {
            node.rect.x += offset;
        }
    }

    /// XFA Spec 3.3 §8.2 — Top-to-Bottom Layout (p280).
    /// §8.3 (p282-284): child hAlign offsets x within parent width.
    fn layout_tb(&self, children: &[FormNodeId], available: Size) -> Result<Vec<LayoutNode>> {
        let mut nodes = Vec::new();
        let mut y_cursor = 0.0;

        for &child_id in children {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent_with_available(child_id, Some(available));
            let child_bottom = y_cursor + child_size.height;

            // 0.5pt tolerance prevents marginal overflows from triggering
            // pagination — sub-point rounding differences should not create
            // extra pages (#971).
            if child_bottom > available.height + 0.5 {
                let remaining_height = (available.height - y_cursor).max(0.0);

                // Use the same overflow semantics as layout_content_fitting():
                // split text leaves at line boundaries, split splittable TB
                // containers at child boundaries, then stop placing further
                // children in this container pass.
                if remaining_height > 0.0 && self.is_splittable_text_leaf(child_id) {
                    let txt = match &child.node_type {
                        FormNodeType::Draw(DrawContent::Text(t)) => t.as_str(),
                        FormNodeType::Field { value } => value.as_str(),
                        _ => "",
                    };
                    let child_style = &self.form.meta(child_id).style;
                    let para_margins = child_style
                        .margin_left_pt
                        .unwrap_or(crate::types::DEFAULT_TEXT_PADDING)
                        + child_style
                            .margin_right_pt
                            .unwrap_or(crate::types::DEFAULT_TEXT_PADDING);
                    let child_border_w = child_style
                        .border_width_pt
                        .unwrap_or(child.box_model.border_width);
                    let insets_w =
                        child.box_model.margins.horizontal() + child_border_w * 2.0 + para_margins;
                    let max_w = (child_size.width - insets_w).max(1.0);
                    let wrapped = text::wrap_text(
                        txt,
                        max_w,
                        &child.font,
                        child_style.text_indent_pt.unwrap_or(0.0),
                        child_style.line_height_pt,
                    );
                    let (partial, _) =
                        self.split_text_node(child_id, y_cursor, remaining_height, &wrapped.lines)?;

                    if partial.rect.height > 0.0 && partial.rect.height <= remaining_height + 1.0 {
                        let x = self.child_h_align_offset(
                            child_id,
                            partial.rect.width,
                            available.width,
                        );
                        let mut partial = partial;
                        partial.rect.x = x;
                        nodes.push(partial);
                    } else if nodes.is_empty() {
                        // Keep progress when the first child is oversized.
                        let x =
                            self.child_h_align_offset(child_id, child_size.width, available.width);
                        let node = self.layout_single_node_with_extent(
                            child_id, child, x, y_cursor, child_size, None,
                        )?;
                        nodes.push(node);
                    }
                } else if remaining_height > 0.0 && self.can_split(child_id) {
                    let (partial, _) = self.split_tb_node(
                        child_id,
                        y_cursor,
                        remaining_height,
                        available,
                        None,
                        None,
                    )?;
                    if partial.rect.height > 0.0 && partial.rect.height <= remaining_height + 1.0 {
                        let x = self.child_h_align_offset(
                            child_id,
                            partial.rect.width,
                            available.width,
                        );
                        let mut partial = partial;
                        partial.rect.x = x;
                        nodes.push(partial);
                    } else if nodes.is_empty() {
                        // Keep progress when the first child is oversized.
                        let x =
                            self.child_h_align_offset(child_id, child_size.width, available.width);
                        let node = self.layout_single_node_with_extent(
                            child_id, child, x, y_cursor, child_size, None,
                        )?;
                        nodes.push(node);
                    }
                } else if nodes.is_empty() {
                    // Keep progress when the first child is oversized.
                    let x = self.child_h_align_offset(child_id, child_size.width, available.width);
                    let node = self.layout_single_node_with_extent(
                        child_id, child, x, y_cursor, child_size, None,
                    )?;
                    nodes.push(node);
                }

                break;
            }

            let x = self.child_h_align_offset(child_id, child_size.width, available.width);

            let node = self
                .layout_single_node_with_extent(child_id, child, x, y_cursor, child_size, None)?;
            nodes.push(node);

            y_cursor = child_bottom;
        }
        Ok(nodes)
    }

    /// XFA Spec 3.3 §8.2 — Left-to-Right Top-to-Bottom Tiled Layout (p281).
    /// §8.3 Example 8.12 (p284): when children specify hAlign, the entire row
    /// is shifted within the parent width (first child's hAlign determines row).
    fn layout_lr_tb(&self, children: &[FormNodeId], available: Size) -> Result<Vec<LayoutNode>> {
        let mut nodes = Vec::new();
        let mut x_cursor = 0.0;
        let mut y_cursor = 0.0;
        let mut row_height = 0.0_f64;
        let mut row_start = 0_usize;

        for &child_id in children {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent(child_id);

            // Wrap to next row if doesn't fit horizontally
            if x_cursor + child_size.width > available.width && x_cursor > 0.0 {
                self.shift_row_h_align(&mut nodes[row_start..], x_cursor, available.width);
                row_start = nodes.len();
                y_cursor += row_height;
                x_cursor = 0.0;
                row_height = 0.0;
            }

            let node = self.layout_single_node(child_id, child, x_cursor, y_cursor, None)?;
            nodes.push(node);

            x_cursor += child_size.width;
            row_height = row_height.max(child_size.height);
        }
        self.shift_row_h_align(&mut nodes[row_start..], x_cursor, available.width);
        Ok(nodes)
    }

    /// XFA Spec 3.3 §8.2 — Right-to-Left Top-to-Bottom Tiled Layout (p282).
    /// §8.3 (p282): default hAlign is "right" for RTL. Per-child hAlign
    /// overrides flow position (Example 8.11, p283).
    fn layout_rl_tb(&self, children: &[FormNodeId], available: Size) -> Result<Vec<LayoutNode>> {
        let mut nodes = Vec::new();
        let mut x_cursor = available.width;
        let mut y_cursor = 0.0;
        let mut row_height = 0.0_f64;

        for &child_id in children {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent(child_id);

            // Wrap to next row if doesn't fit
            if x_cursor - child_size.width < 0.0 && x_cursor < available.width {
                y_cursor += row_height;
                x_cursor = available.width;
                row_height = 0.0;
            }

            // §8.3: explicit hAlign overrides default RTL flow position
            let h_align = self.form.meta(child_id).style.h_align;
            let x = match h_align {
                Some(TextAlign::Left) => {
                    x_cursor -= child_size.width;
                    0.0
                }
                Some(TextAlign::Center) => {
                    x_cursor -= child_size.width;
                    ((available.width - child_size.width) / 2.0).max(0.0)
                }
                _ => {
                    x_cursor -= child_size.width;
                    x_cursor
                }
            };
            let node = self.layout_single_node(child_id, child, x, y_cursor, None)?;
            nodes.push(node);

            row_height = row_height.max(child_size.height);
        }
        Ok(nodes)
    }

    /// Table layout: resolve column widths from the table node, then delegate
    /// to `layout_table_rows`. This stub is still called from `layout_children`
    /// but the real table path is intercepted in `layout_single_node_with_extent`
    /// where we have access to the parent FormNode's `column_widths`.
    fn layout_table(&self, children: &[FormNodeId], available: Size) -> Result<Vec<LayoutNode>> {
        // Fallback: equal-width columns based on max cell count
        let max_cells = children
            .iter()
            .map(|&row_id| self.form.get(row_id).children.len())
            .max()
            .unwrap_or(0);
        if max_cells == 0 {
            return Ok(Vec::new());
        }
        let col_width = available.width / max_cells as f64;
        let col_widths: Vec<f64> = vec![col_width; max_cells];
        self.layout_table_rows(children, available, &col_widths)
    }

    /// XFA Spec 3.3 §8.11 — Tables (p327-332): stack rows vertically,
    /// distributing cells across resolved column widths. Per spec: first lay
    /// out cells with natural sizes, then expand cells to column width, then
    /// expand cells vertically to row height (tallest cell).
    ///
    /// TODO §8.11: rl-row (right-to-left row), non-row direct children in table.
    fn layout_table_rows(
        &self,
        children: &[FormNodeId],
        available: Size,
        col_widths: &[f64],
    ) -> Result<Vec<LayoutNode>> {
        let expanded = self.expand_occur(children);
        let mut nodes = Vec::new();
        let mut y_cursor = 0.0;

        for &row_id in &expanded {
            let row_node = self.form.get(row_id);
            let row_children = self.expand_occur(&row_node.children);

            // Layout cells within this row using column widths
            let mut cells = Vec::new();
            let mut x_cursor = 0.0;
            let mut col_idx = 0usize;
            let mut max_cell_height = 0.0_f64;

            for &cell_id in &row_children {
                if col_idx >= col_widths.len() {
                    break;
                }
                let cell = self.form.get(cell_id);
                let span = cell.col_span;

                // Calculate cell width from column widths
                let cell_width = if span == -1 {
                    // Span remaining columns
                    col_widths[col_idx..].iter().sum::<f64>()
                } else {
                    let span_count =
                        (span.max(1) as usize).min(col_widths.len().saturating_sub(col_idx));
                    col_widths[col_idx..col_idx + span_count]
                        .iter()
                        .sum::<f64>()
                };

                // Layout cell with forced width
                let cell_available = Size {
                    width: cell_width,
                    height: available.height - y_cursor,
                };
                let cell_height = self
                    .compute_extent_with_available(cell_id, Some(cell_available))
                    .height;
                let cell_extent = Size {
                    width: cell_width,
                    height: cell_height,
                };

                let cell_node = self.layout_single_node_with_extent(
                    cell_id,
                    cell,
                    x_cursor,
                    0.0,
                    cell_extent,
                    None,
                )?;
                max_cell_height = max_cell_height.max(cell_extent.height);
                cells.push(cell_node);

                x_cursor += cell_width;
                if span == -1 {
                    col_idx = col_widths.len();
                } else {
                    col_idx += span.max(1) as usize;
                }
            }

            // Equalize row height: all cells expand to tallest
            for cell in &mut cells {
                cell.rect.height = max_cell_height;
            }

            // Create row layout node
            let row_layout = LayoutNode {
                form_node: row_id,
                rect: Rect::new(0.0, y_cursor, available.width, max_cell_height),
                name: row_node.name.clone(),
                content: LayoutContent::None,
                children: cells,
                style: self.form.meta(row_id).style.clone(),
                display_items: self.form.meta(row_id).display_items.clone(),
                save_items: self.form.meta(row_id).save_items.clone(),
            };
            nodes.push(row_layout);

            y_cursor += max_cell_height;
        }
        Ok(nodes)
    }

    fn resolve_column_widths_with_override(
        &self,
        table_node: &FormNode,
        available_width: f64,
        children_override: Option<&[FormNodeId]>,
    ) -> Vec<f64> {
        let specified = &table_node.column_widths;
        let node_children = children_override.unwrap_or(&table_node.children);

        // Determine number of columns
        let max_cols_from_rows = node_children
            .iter()
            .map(|&row_id| {
                let row = self.form.get(row_id);
                row.children
                    .iter()
                    .map(|&cell_id| {
                        let cell = self.form.get(cell_id);
                        cell.col_span.max(1) as usize
                    })
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(0);

        let num_cols = specified.len().max(max_cols_from_rows);
        if num_cols == 0 {
            return vec![];
        }

        let mut widths = Vec::with_capacity(num_cols);
        for i in 0..num_cols {
            let spec_value = specified.get(i).copied().unwrap_or(-1.0);
            if spec_value >= 0.0 {
                widths.push(spec_value);
            } else {
                // Auto-size: find widest single-span cell in this column
                let mut max_w = 0.0_f64;
                for &row_id in node_children {
                    let row = self.form.get(row_id);
                    let mut col_idx = 0usize;
                    for &cell_id in &row.children {
                        let cell = self.form.get(cell_id);
                        let span = cell.col_span;
                        if col_idx == i && span == 1 {
                            let cell_extent = self.compute_extent(cell_id);
                            max_w = max_w.max(cell_extent.width);
                        }
                        col_idx += span.max(1) as usize;
                    }
                }
                widths.push(max_w);
            }
        }

        // Scale down proportionally if total exceeds available width
        let total: f64 = widths.iter().sum();
        if total > available_width && total > 0.0 {
            let scale = available_width / total;
            for w in &mut widths {
                *w *= scale;
            }
        }

        widths
    }

    /// Row layout: children fill horizontally within the row.
    /// Used for standalone Row-layout subforms (not inside a table context).
    fn layout_row(&self, children: &[FormNodeId], available: Size) -> Result<Vec<LayoutNode>> {
        let mut nodes = Vec::new();
        let mut x_cursor = 0.0;

        for &child_id in children {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent(child_id);

            let node = self.layout_single_node(child_id, child, x_cursor, 0.0, None)?;
            nodes.push(node);

            x_cursor += child_size.width;

            if x_cursor > available.width {
                break;
            }
        }
        Ok(nodes)
    }

    /// Layout a single node: compute its rect and recursively layout children.
    fn layout_single_node(
        &self,
        id: FormNodeId,
        node: &FormNode,
        x: f64,
        y: f64,
        children_override: Option<&[FormNodeId]>,
    ) -> Result<LayoutNode> {
        let extent = self.compute_extent_with_override(id, children_override);
        self.layout_single_node_with_extent(id, node, x, y, extent, children_override)
    }

    /// Layout a single node with a pre-computed extent.
    fn layout_single_node_with_extent(
        &self,
        id: FormNodeId,
        node: &FormNode,
        x: f64,
        y: f64,
        extent: Size,
        children_override: Option<&[FormNodeId]>,
    ) -> Result<LayoutNode> {
        // All non-visible presence values (hidden/invisible/inactive) produce
        // no visual content. In practice these nodes are already filtered in
        // queue_content/expand_occur, but this guard handles children_override.
        if self.form.meta(id).presence.is_layout_hidden() {
            let hidden_meta = self.form.meta(id);
            return Ok(LayoutNode {
                form_node: id,
                rect: Rect::new(x, y, extent.width, extent.height),
                name: node.name.clone(),
                content: LayoutContent::None,
                children: Vec::new(),
                style: Default::default(),
                display_items: hidden_meta.display_items.clone(),
                save_items: hidden_meta.save_items.clone(),
            });
        }

        let node_style = &self.form.meta(id).style;
        let para_margins = node_style
            .margin_left_pt
            .unwrap_or(crate::types::DEFAULT_TEXT_PADDING)
            + node_style
                .margin_right_pt
                .unwrap_or(crate::types::DEFAULT_TEXT_PADDING);

        let content = match &node.node_type {
            FormNodeType::Field { value } => {
                let meta = self.form.meta(id);
                let display_val = resolve_display_value(value, meta);
                if !display_val.is_empty() && node.children.is_empty() {
                    let border_w = node_style
                        .border_width_pt
                        .unwrap_or(node.box_model.border_width);
                    let insets_w =
                        node.box_model.margins.horizontal() + border_w * 2.0 + para_margins;
                    let max_w = (extent.width - insets_w).max(0.0);
                    let wrapped = text::wrap_text(
                        &display_val,
                        max_w,
                        &node.font,
                        node_style.text_indent_pt.unwrap_or(0.0),
                        node_style.line_height_pt,
                    );
                    LayoutContent::WrappedText {
                        lines: wrapped.lines,
                        first_line_of_para: wrapped.first_line_of_para,
                        font_size: node.font.size,
                        text_align: node.font.text_align,
                        font_family: node.font.typeface,
                        space_above_pt: node_style.space_above_pt,
                        space_below_pt: node_style.space_below_pt,
                        from_field: true,
                    }
                } else {
                    LayoutContent::Field {
                        value: display_val.to_string(),
                        field_kind: meta.field_kind,
                        font_size: node.font.size,
                        font_family: node.font.typeface,
                    }
                }
            }
            FormNodeType::Draw(DrawContent::Text(content)) => {
                if !content.is_empty() && node.children.is_empty() {
                    let border_w = node_style
                        .border_width_pt
                        .unwrap_or(node.box_model.border_width);
                    let insets_w =
                        node.box_model.margins.horizontal() + border_w * 2.0 + para_margins;
                    let max_w = (extent.width - insets_w).max(0.0);
                    let wrapped = text::wrap_text(
                        content,
                        max_w,
                        &node.font,
                        node_style.text_indent_pt.unwrap_or(0.0),
                        node_style.line_height_pt,
                    );
                    LayoutContent::WrappedText {
                        lines: wrapped.lines,
                        first_line_of_para: wrapped.first_line_of_para,
                        font_size: node.font.size,
                        text_align: node.font.text_align,
                        font_family: node.font.typeface,
                        space_above_pt: node_style.space_above_pt,
                        space_below_pt: node_style.space_below_pt,
                        from_field: false,
                    }
                } else {
                    LayoutContent::Text(content.clone())
                }
            }
            FormNodeType::Draw(dc) => LayoutContent::Draw(dc.clone()),
            FormNodeType::Image { data, mime_type } => LayoutContent::Image {
                data: data.clone(),
                mime_type: mime_type.clone(),
            },
            _ => LayoutContent::None,
        };

        let child_available = Size {
            width: node.box_model.content_width().min(extent.width),
            height: node.box_model.content_height().min(extent.height),
        };

        let node_children = children_override.unwrap_or(&node.children);

        let children = if node_children.is_empty() {
            Vec::new()
        } else if node.layout == LayoutStrategy::Table {
            // Table layout: resolve column widths from the parent node,
            // then distribute cells across rows.
            let col_widths = self.resolve_column_widths_with_override(
                node,
                child_available.width,
                children_override,
            );
            self.layout_table_rows(node_children, child_available, &col_widths)?
        } else {
            self.layout_children(node_children, child_available, node.layout)?
        };
        let mut children = children;

        if node.layout == LayoutStrategy::Positioned {
            let dx = node.box_model.margins.left;
            let mut dy = node.box_model.margins.top;
            if let Some(override_children) = children_override {
                if let Some(y_base) = override_children
                    .iter()
                    .map(|&cid| self.form.get(cid).box_model.y)
                    .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                {
                    dy -= y_base;
                }
            }
            if dx != 0.0 || dy != 0.0 {
                for child in &mut children {
                    child.rect.x += dx;
                    child.rect.y += dy;
                }
            }
        }

        Ok(LayoutNode {
            form_node: id,
            rect: Rect::new(x, y, extent.width, extent.height),
            name: node.name.clone(),
            content,
            children,
            style: self.form.meta(id).style.clone(),
            display_items: self.form.meta(id).display_items.clone(),
            save_items: self.form.meta(id).save_items.clone(),
        })
    }

    /// Compute the outer extent (total bounding box) of a form node.
    ///
    /// When `available` is provided, growable dimensions may expand to fill
    /// the available space (XFA §8: growable objects fill the parent container).
    pub fn compute_extent(&self, id: FormNodeId) -> Size {
        self.compute_extent_with_available(id, None)
    }

    /// Compute extent with optional children override.
    pub fn compute_extent_with_override(
        &self,
        id: FormNodeId,
        children_override: Option<&[FormNodeId]>,
    ) -> Size {
        self.compute_extent_with_available_and_override(id, None, children_override)
    }

    /// Compute extent with optional available-space constraint.
    ///
    /// For growable dimensions (width/height = None), the element sizes to fit
    /// its content. When `available` is given, a growable dimension expands to
    /// at least the available space (but content can make it larger, subject to
    /// max constraints).
    fn compute_extent_with_available(&self, id: FormNodeId, available: Option<Size>) -> Size {
        self.compute_extent_with_available_and_override(id, available, None)
    }

    fn compute_extent_with_available_and_override(
        &self,
        id: FormNodeId,
        available: Option<Size>,
        children_override: Option<&[FormNodeId]>,
    ) -> Size {
        let node = self.form.get(id);
        let bm = &node.box_model;
        let ext_style = &self.form.meta(id).style;

        // If explicit size is set, use it — except for TB subforms with
        // children, where we must compute the actual content height so that
        // pagination can detect overflow beyond the explicit height (#768).
        let is_tb_with_children = node.layout == LayoutStrategy::TopToBottom
            && !children_override.unwrap_or(&node.children).is_empty();
        if let (Some(w), Some(h)) = (bm.width, bm.height) {
            if !is_tb_with_children {
                return Size {
                    width: w,
                    height: h,
                };
            }
        }

        // For growable dimensions, compute from children or text content
        let mut content_size = Size::default();

        let node_children = children_override.unwrap_or(&node.children);

        if !node_children.is_empty() {
            // (#764) When children_override is set the list is already
            // occur-expanded from a prior split — re-expanding inflates the
            // extent and contributes to over-pagination.
            let expanded = if children_override.is_some() {
                node_children.to_vec()
            } else {
                self.expand_occur(node_children)
            };
            match node.layout {
                LayoutStrategy::TopToBottom => {
                    // #687: pass available so text wrapping is considered
                    for &child_id in &expanded {
                        let cs = self.compute_extent_with_available(child_id, available);
                        content_size.width = content_size.width.max(cs.width);
                        content_size.height += cs.height;
                    }
                }
                LayoutStrategy::LeftToRightTB | LayoutStrategy::Row => {
                    for &child_id in &expanded {
                        let cs = self.compute_extent_with_available(child_id, available);
                        content_size.width += cs.width;
                        content_size.height = content_size.height.max(cs.height);
                    }
                }
                LayoutStrategy::Table => {
                    let avail_w = available.map(|a| a.width).unwrap_or(f64::MAX);
                    let col_widths =
                        self.resolve_column_widths_with_override(node, avail_w, children_override);
                    let table_width: f64 = col_widths.iter().sum();
                    content_size.width = content_size.width.max(table_width);
                    // Table height = sum of row heights
                    for &row_id in &expanded {
                        let row_extent = self.compute_extent_with_available(row_id, available);
                        content_size.height += row_extent.height;
                    }
                }
                _ => {
                    // Positioned: envelope all children (occur doesn't stack in positioned).
                    // When a children_override is active (from a positioned split),
                    // compute relative height from the minimum y so that remaining
                    // children after a page break produce a sensible extent instead
                    // of the full original height (#736).
                    let y_base = if children_override.is_some() {
                        node_children
                            .iter()
                            .map(|&cid| self.form.get(cid).box_model.y)
                            .fold(f64::MAX, f64::min)
                    } else {
                        0.0
                    };
                    for &child_id in node_children {
                        let child = self.form.get(child_id);
                        let cs = self.compute_extent(child_id);
                        content_size.width = content_size.width.max(child.box_model.x + cs.width);
                        content_size.height = content_size
                            .height
                            .max(child.box_model.y - y_base + cs.height);
                    }
                }
            }
        } else {
            // Leaf node: measure text content for Draw/Field nodes
            let text_content = match &node.node_type {
                FormNodeType::Draw(DrawContent::Text(content)) => Some(content.as_str()),
                FormNodeType::Field { value } => Some(value.as_str()),
                _ => None,
            };

            if let Some(txt) = text_content {
                if !txt.is_empty() {
                    let ext_style = &self.form.meta(id).style;
                    let ext_para = ext_style
                        .margin_left_pt
                        .unwrap_or(crate::types::DEFAULT_TEXT_PADDING)
                        + ext_style
                            .margin_right_pt
                            .unwrap_or(crate::types::DEFAULT_TEXT_PADDING);
                    let border_w = ext_style.border_width_pt.unwrap_or(bm.border_width);
                    let insets_w = bm.margins.horizontal() + border_w * 2.0 + ext_para;
                    let space_above = ext_style.space_above_pt.unwrap_or(0.0);
                    let space_below = ext_style.space_below_pt.unwrap_or(0.0);
                    // If width is fixed, wrap text within that width minus insets
                    // If width is growable, measure without wrapping
                    let text_size = if let Some(w) = bm.width {
                        let max_text_width = (w - insets_w).max(0.0);
                        text::wrap_text(
                            txt,
                            max_text_width,
                            &node.font,
                            ext_style.text_indent_pt.unwrap_or(0.0),
                            ext_style.line_height_pt,
                        )
                        .size
                    } else if let Some(avail) = available {
                        let max_text_width = (avail.width - insets_w).max(0.0);
                        text::wrap_text(
                            txt,
                            max_text_width,
                            &node.font,
                            ext_style.text_indent_pt.unwrap_or(0.0),
                            ext_style.line_height_pt,
                        )
                        .size
                    } else {
                        text::measure_text(txt, &node.font)
                    };
                    content_size.width = content_size.width.max(text_size.width);
                    content_size.height = content_size
                        .height
                        .max(text_size.height + space_above + space_below);
                }
            }
        }

        // When available space is given, growable dims expand to fill it
        if let Some(avail) = available {
            if bm.width.is_none() {
                let border_w = ext_style.border_width_pt.unwrap_or(bm.border_width);
                let insets_w = bm.margins.horizontal() + border_w * 2.0;
                content_size.width = content_size.width.max(avail.width - insets_w);
            }
        }

        let mut result = bm.outer_size(content_size);

        // For TB subforms with children AND an explicit height, ensure the
        // reported height reflects actual content height (not clamped to the
        // fixed h) so layout_content_fitting() detects overflow and triggers
        // pagination.  max_height constraints on growable subforms are NOT
        // overridden — only explicit height declarations (#768).
        if is_tb_with_children && bm.height.is_some() {
            let border_w = ext_style.border_width_pt.unwrap_or(bm.border_width);
            let mut unclamped_h = content_size.height + bm.margins.vertical() + border_w * 2.0;
            if let Some(ref cap) = bm.caption {
                if matches!(
                    cap.placement,
                    crate::types::CaptionPlacement::Top | crate::types::CaptionPlacement::Bottom
                ) {
                    unclamped_h += cap.reserve.unwrap_or(0.0);
                }
            }
            result.height = result.height.max(unclamped_h);
        }

        result
    }
}

/// Find a page area by break target string. The target can match the page
/// area's `name`, its `xfa_id`, or a `pageArea[N]` index reference.
#[allow(dead_code)]
fn find_page_area_by_target(page_areas: &[PageAreaInfo], target: &str) -> Option<usize> {
    // Try matching by name first (e.g. "MP3").
    if let Some(idx) = page_areas.iter().position(|pa| pa.name == target) {
        return Some(idx);
    }
    // Try matching by xfa_id (e.g. "Page4_ID").
    if let Some(idx) = page_areas
        .iter()
        .position(|pa| pa.xfa_id.as_deref() == Some(target))
    {
        return Some(idx);
    }
    // Try matching "pageArea[N]" index reference.
    if let Some(rest) = target.strip_prefix("pageArea") {
        if let Some(idx_str) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if let Ok(idx) = idx_str.parse::<usize>() {
                if idx < page_areas.len() {
                    return Some(idx);
                }
            }
        }
        // "pageArea" without index → first.
        if rest.is_empty() && !page_areas.is_empty() {
            return Some(0);
        }
    }
    None
}

/// Get the primary (largest) content area from a page area.
fn primary_content_area(pa: &PageAreaInfo) -> &ContentArea {
    let max_area = pa
        .content_areas
        .iter()
        .map(|ca| ca.width * ca.height)
        .fold(0.0_f64, f64::max);
    pa.content_areas
        .iter()
        .find(|ca| {
            let a = ca.width * ca.height;
            a >= max_area * 0.90 || pa.content_areas.len() == 1
        })
        .unwrap_or(&pa.content_areas[0])
}

struct PageAreaInfo {
    /// Name of the page area (e.g. "MP1", "MP3") for break targeting.
    name: String,
    /// XFA id attribute (e.g. "Page1", "Page4_ID") for break targeting.
    xfa_id: Option<String>,
    content_areas: Vec<ContentArea>,
    page_width: f64,
    page_height: f64,
    /// Fixed-position nodes (e.g., page-level headers/footers) placed on every
    /// page that uses this page area.
    fixed_nodes: Vec<FormNodeId>,
    /// XFA 3.3 §8.6 / §3.1 — this pageArea was instantiated at runtime
    /// (recorded in the form-DOM packet) rather than declared once in the
    /// template.  Such instances always emit a layout page, even when the
    /// flowing body queue is exhausted, because they record an already-
    /// paginated runtime state.
    runtime_instantiated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::Occur;
    use crate::text::FontMetrics;
    use crate::types::BoxModel;

    fn make_field(tree: &mut FormTree, name: &str, w: f64, h: f64) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Field {
                value: name.to_string(),
            },
            box_model: BoxModel {
                width: Some(w),
                height: Some(h),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn make_subform(
        tree: &mut FormTree,
        name: &str,
        strategy: LayoutStrategy,
        w: Option<f64>,
        h: Option<f64>,
        children: Vec<FormNodeId>,
    ) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: w,
                height: h,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: strategy,
            children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    /// Count leaf layout nodes (fields/draws) recursively across a page.
    fn count_leaf_nodes(page: &LayoutPage) -> usize {
        fn count(nodes: &[LayoutNode]) -> usize {
            nodes
                .iter()
                .map(|n| {
                    if n.children.is_empty() {
                        1
                    } else {
                        count(&n.children)
                    }
                })
                .sum()
        }
        count(&page.nodes)
    }

    fn make_field_value(
        tree: &mut FormTree,
        name: &str,
        value: &str,
        w: f64,
        h: f64,
    ) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Field {
                value: value.to_string(),
            },
            box_model: BoxModel {
                width: Some(w),
                height: Some(h),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn make_draw_text(tree: &mut FormTree, name: &str, text: &str, w: f64, h: f64) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Draw(DrawContent::Text(text.to_string())),
            box_model: BoxModel {
                width: Some(w),
                height: Some(h),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn make_page_area(tree: &mut FormTree, name: &str, width: f64, height: f64) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                    leader: None,
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(width),
                height: Some(height),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn make_root(tree: &mut FormTree, children: Vec<FormNodeId>) -> FormNodeId {
        tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn mark_data_bound(tree: &mut FormTree, id: FormNodeId) {
        let name = tree.get(id).name.clone();
        tree.meta_mut(id).data_bind_ref = Some(format!("$.{name}"));
    }

    #[test]
    fn positioned_layout() {
        let mut tree = FormTree::new();
        let f1 = tree.add_node(FormNode {
            name: "Field1".to_string(),
            node_type: FormNodeType::Field {
                value: "A".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(20.0),
                x: 10.0,
                y: 30.0,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let f2 = tree.add_node(FormNode {
            name: "Field2".to_string(),
            node_type: FormNodeType::Field {
                value: "B".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(20.0),
                x: 10.0,
                y: 60.0,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        let page = &result.pages[0];
        assert_eq!(page.nodes.len(), 2);
        assert_eq!(page.nodes[0].rect.x, 10.0);
        assert_eq!(page.nodes[0].rect.y, 30.0);
        assert_eq!(page.nodes[1].rect.x, 10.0);
        assert_eq!(page.nodes[1].rect.y, 60.0);
    }

    #[test]
    fn positioned_children_offset_by_parent_margins() {
        use crate::types::Insets;

        let mut tree = FormTree::new();
        let child = tree.add_node(FormNode {
            name: "Child".to_string(),
            node_type: FormNodeType::Field {
                value: "Child".to_string(),
            },
            box_model: BoxModel {
                width: Some(80.0),
                height: Some(20.0),
                x: 0.0,
                y: 0.0,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let parent = tree.add_node(FormNode {
            name: "Parent".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(100.0),
                x: 50.0,
                y: 40.0,
                margins: Insets {
                    top: 15.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 10.0,
                },
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![child],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![parent],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let parent_node = &result.pages[0].nodes[0];
        assert_eq!(parent_node.rect.x, 50.0);
        assert_eq!(parent_node.rect.y, 40.0);
        assert_eq!(parent_node.children[0].rect.x, 10.0);
        assert_eq!(parent_node.children[0].rect.y, 15.0);
    }

    #[test]
    fn tb_layout() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "Field1", 200.0, 30.0);
        let f2 = make_field(&mut tree, "Field2", 200.0, 30.0);
        let f3 = make_field(&mut tree, "Field3", 200.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1, f2, f3],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        let page = &result.pages[0];
        assert_eq!(page.nodes.len(), 3);
        assert_eq!(page.nodes[0].rect.y, 0.0);
        assert_eq!(page.nodes[1].rect.y, 30.0);
        assert_eq!(page.nodes[2].rect.y, 60.0);
    }

    #[test]
    fn lr_tb_wrapping() {
        let mut tree = FormTree::new();
        // 3 fields of 250pt width in a 600pt container → 2 fit on first row, 1 wraps
        let f1 = make_field(&mut tree, "F1", 250.0, 30.0);
        let f2 = make_field(&mut tree, "F2", 250.0, 30.0);
        let f3 = make_field(&mut tree, "F3", 250.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(600.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::LeftToRightTB,
            children: vec![f1, f2, f3],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        assert_eq!(page.nodes.len(), 3);
        // First two on row 1
        assert_eq!(page.nodes[0].rect.x, 0.0);
        assert_eq!(page.nodes[0].rect.y, 0.0);
        assert_eq!(page.nodes[1].rect.x, 250.0);
        assert_eq!(page.nodes[1].rect.y, 0.0);
        // Third wraps to row 2
        assert_eq!(page.nodes[2].rect.x, 0.0);
        assert_eq!(page.nodes[2].rect.y, 30.0);
    }

    #[test]
    fn nested_subforms() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "Name", 200.0, 25.0);
        let f2 = make_field(&mut tree, "Email", 200.0, 25.0);

        let sub = make_subform(
            &mut tree,
            "PersonalInfo",
            LayoutStrategy::TopToBottom,
            Some(300.0),
            Some(100.0),
            vec![f1, f2],
        );

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![sub],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        assert_eq!(page.nodes.len(), 1);
        let subform = &page.nodes[0];
        assert_eq!(subform.name, "PersonalInfo");
        assert_eq!(subform.rect.width, 300.0);
        assert_eq!(subform.children.len(), 2);
        assert_eq!(subform.children[0].rect.y, 0.0);
        assert_eq!(subform.children[1].rect.y, 25.0);
    }

    #[test]
    fn page_area_layout() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "Field1", 200.0, 30.0);

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 36.0,
                    y: 36.0,
                    width: 540.0,
                    height: 720.0,
                    leader: None,
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![page_area, f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        let page = &result.pages[0];
        // Field should be offset by content area position (36, 36)
        assert_eq!(page.nodes[0].rect.x, 36.0);
        assert_eq!(page.nodes[0].rect.y, 36.0);
    }

    #[test]
    fn trailing_empty_pagearea_continuation_suppressed() {
        let mut tree = FormTree::new();
        let first_page = make_page_area(&mut tree, "Page1", 200.0, 80.0);
        let continuation_page = make_page_area(&mut tree, "Page2", 200.0, 100.0);
        let record = make_field_value(&mut tree, "record", "data", 180.0, 80.0);
        mark_data_bound(&mut tree, record);
        let template_only = make_draw_text(&mut tree, "template_only", "template", 180.0, 20.0);
        let root = make_root(
            &mut tree,
            vec![first_page, continuation_page, record, template_only],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        assert_eq!(count_leaf_nodes(&result.pages[0]), 1);
    }

    #[test]
    fn valid_multi_page_overflow_unchanged() {
        let mut tree = FormTree::new();
        let page_area = make_page_area(&mut tree, "Page1", 200.0, 100.0);
        let mut children = vec![page_area];
        for idx in 0..4 {
            let field = make_field_value(&mut tree, &format!("record{idx}"), "data", 180.0, 40.0);
            mark_data_bound(&mut tree, field);
            children.push(field);
        }
        let root = make_root(&mut tree, children);

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages.iter().map(count_leaf_nodes).sum::<usize>(), 4);
    }

    #[test]
    fn template_only_page_kept_when_explicitly_required() {
        let mut tree = FormTree::new();
        let first_page = make_page_area(&mut tree, "Page1", 200.0, 80.0);
        let anchored_page = make_page_area(&mut tree, "Page2", 200.0, 100.0);
        let record = make_field_value(&mut tree, "record", "data", 180.0, 80.0);
        mark_data_bound(&mut tree, record);
        let anchored_draw = make_draw_text(&mut tree, "anchored_draw", "next page", 180.0, 20.0);
        tree.meta_mut(anchored_draw).page_break_before = true;
        let root = make_root(
            &mut tree,
            vec![first_page, anchored_page, record, anchored_draw],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 2);
        assert_eq!(count_leaf_nodes(&result.pages[1]), 1);
    }

    #[test]
    fn single_page_form_remains_single_page() {
        let mut tree = FormTree::new();
        let page_area = make_page_area(&mut tree, "Page1", 200.0, 100.0);
        let record = make_field_value(&mut tree, "record", "data", 180.0, 40.0);
        mark_data_bound(&mut tree, record);
        let root = make_root(&mut tree, vec![page_area, record]);

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        assert_eq!(count_leaf_nodes(&result.pages[0]), 1);
    }

    #[test]
    fn continuation_with_remaining_body_still_emits() {
        let mut tree = FormTree::new();
        let first_page = make_page_area(&mut tree, "Page1", 200.0, 60.0);
        let continuation_page = make_page_area(&mut tree, "Page2", 200.0, 100.0);
        let first_record = make_field_value(&mut tree, "first_record", "data", 180.0, 60.0);
        let second_record = make_field_value(&mut tree, "second_record", "data", 180.0, 40.0);
        mark_data_bound(&mut tree, first_record);
        mark_data_bound(&mut tree, second_record);
        let root = make_root(
            &mut tree,
            vec![first_page, continuation_page, first_record, second_record],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 2);
        assert_eq!(count_leaf_nodes(&result.pages[1]), 1);
    }

    #[test]
    fn growable_extent() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 20.0);
        let f2 = make_field(&mut tree, "F2", 150.0, 20.0);

        // Subform with no explicit size — should grow to fit children
        let sub = make_subform(
            &mut tree,
            "Container",
            LayoutStrategy::TopToBottom,
            None,
            None,
            vec![f1, f2],
        );

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(sub);

        // Width = max child width = 150, Height = sum = 40
        assert_eq!(extent.width, 150.0);
        assert_eq!(extent.height, 40.0);
    }

    #[test]
    fn rl_tb_layout() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 30.0);
        let f2 = make_field(&mut tree, "F2", 100.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::RightToLeftTB,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();
        let page = &result.pages[0];

        // RL: first field at right edge, second to its left
        assert_eq!(page.nodes[0].rect.x, 300.0); // 400 - 100
        assert_eq!(page.nodes[1].rect.x, 200.0); // 400 - 100 - 100
    }

    // --- Dynamic sizing tests (Epic 3.5) ---

    #[test]
    fn growable_clamped_by_min() {
        // A container with tiny content but min constraints
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 50.0, 10.0);

        let sub = tree.add_node(FormNode {
            name: "Container".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                min_width: 200.0,
                min_height: 100.0,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(sub);

        // Content is 50x10 but min clamps to 200x100
        assert_eq!(extent.width, 200.0);
        assert_eq!(extent.height, 100.0);
    }

    #[test]
    fn growable_clamped_by_max() {
        // A container with large content but max constraints
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 500.0, 300.0);

        let sub = tree.add_node(FormNode {
            name: "Container".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                min_width: 0.0,
                min_height: 0.0,
                max_width: 200.0,
                max_height: 100.0,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(sub);

        // Content is 500x300 but max clamps to 200x100
        assert_eq!(extent.width, 200.0);
        assert_eq!(extent.height, 100.0);
    }

    #[test]
    fn partially_growable_width_fixed() {
        // Width fixed, height growable
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 25.0);
        let f2 = make_field(&mut tree, "F2", 100.0, 25.0);

        let sub = tree.add_node(FormNode {
            name: "Container".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(300.0),
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(sub);

        // Width fixed at 300, height grows to content (25+25=50)
        assert_eq!(extent.width, 300.0);
        assert_eq!(extent.height, 50.0);
    }

    #[test]
    fn partially_growable_height_fixed() {
        // Height fixed, width growable
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 25.0);
        let f2 = make_field(&mut tree, "F2", 150.0, 25.0);

        let sub = tree.add_node(FormNode {
            name: "Container".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(sub);

        // Height fixed at 200, width grows to max child (150)
        assert_eq!(extent.width, 150.0);
        assert_eq!(extent.height, 200.0);
    }

    #[test]
    fn growable_fills_available_width_in_tb() {
        // A growable subform inside a tb-layout parent should fill parent width
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 25.0);

        let growable_sub = tree.add_node(FormNode {
            name: "GrowableSub".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(500.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![growable_sub],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // Growable subform should fill the parent's available width (500)
        assert_eq!(page.nodes[0].rect.width, 500.0);
        // Height should be content-based (25)
        assert_eq!(page.nodes[0].rect.height, 25.0);
    }

    #[test]
    fn growable_fill_capped_by_max() {
        // A growable subform filling parent, but capped by maxW
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 25.0);

        let growable_sub = tree.add_node(FormNode {
            name: "GrowableSub".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                max_width: 300.0,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(500.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![growable_sub],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // Would fill 500, but maxW caps it to 300
        assert_eq!(page.nodes[0].rect.width, 300.0);
    }

    #[test]
    fn growable_with_margins_in_tb() {
        // Growable container with margins should fill parent minus margins
        use crate::types::Insets;
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 50.0, 20.0);

        let growable_sub = tree.add_node(FormNode {
            name: "GrowableSub".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                margins: Insets {
                    top: 5.0,
                    right: 10.0,
                    bottom: 5.0,
                    left: 10.0,
                },
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(300.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![growable_sub],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // Width: content fills 400 - margins(20) = 380, outer = 380 + 20 = 400
        assert_eq!(page.nodes[0].rect.width, 400.0);
        // Height: content 20, outer = 20 + margins(10) = 30
        assert_eq!(page.nodes[0].rect.height, 30.0);
    }

    #[test]
    fn nested_growable_containers() {
        // Nested growable containers should propagate constraints correctly
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 80.0, 20.0);

        let inner = tree.add_node(FormNode {
            name: "Inner".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                min_width: 150.0,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        // Outer container with maxW constraint
        let outer = tree.add_node(FormNode {
            name: "Outer".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                max_width: 400.0,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![inner],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(outer);

        // Inner: content 80, minW clamps to 150 → 150x20
        // Outer: child is 150, maxW is 400 → 150x20
        assert_eq!(extent.width, 150.0);
        assert_eq!(extent.height, 20.0);
    }

    #[test]
    fn min_max_in_lr_tb_layout() {
        // Min/max constraints on children in lr-tb layout
        let mut tree = FormTree::new();

        let f1 = tree.add_node(FormNode {
            name: "F1".to_string(),
            node_type: FormNodeType::Field {
                value: "A".to_string(),
            },
            box_model: BoxModel {
                width: None,
                height: Some(30.0),
                min_width: 200.0,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let f2 = make_field(&mut tree, "F2", 200.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(500.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::LeftToRightTB,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // F1 has minW=200, no content so uses 200
        assert_eq!(page.nodes[0].rect.width, 200.0);
        // F2 at x=200
        assert_eq!(page.nodes[1].rect.x, 200.0);
    }

    // --- Occur rules tests (Epic 3.6) ---

    #[test]
    fn occur_default_once() {
        // Default occur = 1, should produce exactly one instance
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();
        assert_eq!(result.pages[0].nodes.len(), 1);
    }

    #[test]
    fn occur_repeating_tb() {
        // A subform with occur(initial=3) in tb layout should produce 3 instances
        let mut tree = FormTree::new();
        let f1 = tree.add_node(FormNode {
            name: "Row".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(30.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::repeating(1, Some(10), 3),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();
        let page = &result.pages[0];

        // 3 instances stacked vertically
        assert_eq!(page.nodes.len(), 3);
        assert_eq!(page.nodes[0].rect.y, 0.0);
        assert_eq!(page.nodes[1].rect.y, 30.0);
        assert_eq!(page.nodes[2].rect.y, 60.0);
    }

    #[test]
    fn occur_repeating_lr_tb() {
        // Repeating subform in lr-tb layout
        let mut tree = FormTree::new();
        let f1 = tree.add_node(FormNode {
            name: "Cell".to_string(),
            node_type: FormNodeType::Field {
                value: "X".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(30.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::repeating(1, None, 5),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(350.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::LeftToRightTB,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();
        let page = &result.pages[0];

        // 5 instances of 100pt wide in 350pt container:
        // Row 1: 3 cells (0, 100, 200), Row 2: 2 cells (0, 100)
        assert_eq!(page.nodes.len(), 5);
        assert_eq!(page.nodes[0].rect.x, 0.0);
        assert_eq!(page.nodes[0].rect.y, 0.0);
        assert_eq!(page.nodes[1].rect.x, 100.0);
        assert_eq!(page.nodes[2].rect.x, 200.0);
        assert_eq!(page.nodes[3].rect.x, 0.0);
        assert_eq!(page.nodes[3].rect.y, 30.0);
        assert_eq!(page.nodes[4].rect.x, 100.0);
        assert_eq!(page.nodes[4].rect.y, 30.0);
    }

    #[test]
    fn occur_min_enforced() {
        // Occur with min=2, initial=2 should always produce at least 2
        let occur = Occur::repeating(2, Some(5), 2);
        assert_eq!(occur.count(), 2);
        assert!(occur.is_repeating());
    }

    #[test]
    fn occur_max_caps_initial() {
        // Occur with max=3 but initial=5 should cap at 3
        let occur = Occur::repeating(1, Some(3), 5);
        assert_eq!(occur.count(), 3);
    }

    #[test]
    fn occur_initial_raised_to_min() {
        // Occur with min=3 but initial=1 should raise to 3
        let occur = Occur::repeating(3, Some(10), 1);
        assert_eq!(occur.count(), 3);
    }

    #[test]
    fn occur_unlimited_max() {
        let occur = Occur::repeating(0, None, 5);
        assert_eq!(occur.count(), 5);
        assert!(occur.is_repeating());
    }

    #[test]
    fn occur_mixed_children() {
        // Mix of repeating and non-repeating children
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "Header", 200.0, 40.0);
        let row = tree.add_node(FormNode {
            name: "DataRow".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(25.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::repeating(1, Some(10), 4),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let footer = make_field(&mut tree, "Footer", 200.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(600.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![header, row, footer],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();
        let page = &result.pages[0];

        // Header(1) + DataRow(4) + Footer(1) = 6 nodes
        assert_eq!(page.nodes.len(), 6);
        assert_eq!(page.nodes[0].name, "Header");
        assert_eq!(page.nodes[0].rect.y, 0.0);
        // 4 data rows at y=40, 65, 90, 115
        assert_eq!(page.nodes[1].name, "DataRow");
        assert_eq!(page.nodes[1].rect.y, 40.0);
        assert_eq!(page.nodes[2].rect.y, 65.0);
        assert_eq!(page.nodes[3].rect.y, 90.0);
        assert_eq!(page.nodes[4].rect.y, 115.0);
        // Footer at y=140
        assert_eq!(page.nodes[5].name, "Footer");
        assert_eq!(page.nodes[5].rect.y, 140.0);
    }

    #[test]
    fn occur_growable_extent() {
        // A growable container with a repeating child should size to all instances
        let mut tree = FormTree::new();
        let row = tree.add_node(FormNode {
            name: "Row".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(150.0),
                height: Some(20.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::repeating(1, None, 5),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let container = tree.add_node(FormNode {
            name: "Container".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![row],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(container);

        // 5 rows of 150x20 stacked: width=150, height=100
        assert_eq!(extent.width, 150.0);
        assert_eq!(extent.height, 100.0);
    }

    // --- Pagination tests (Epic 3.7) ---

    #[test]
    fn pagination_single_page_no_overflow() {
        // Content fits on one page — no extra pages
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 200.0, 30.0);
        let f2 = make_field(&mut tree, "F2", 200.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        assert_eq!(result.pages[0].nodes.len(), 2);
    }

    #[test]
    fn pagination_overflow_creates_pages() {
        // 10 fields of 30pt each = 300pt total, page height 100pt → 3 pages
        let mut tree = FormTree::new();
        let mut fields = Vec::new();
        for i in 0..10 {
            fields.push(make_field(&mut tree, &format!("F{i}"), 200.0, 30.0));
        }

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(100.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: fields,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // 100pt fits 3 fields (0+30+30+30=90 < 100). 4th at 90+30=120 > 100.
        // Page 1: 3 fields, Page 2: 3 fields, Page 3: 3 fields, Page 4: 1 field
        assert_eq!(result.pages.len(), 4);
        assert_eq!(result.pages[0].nodes.len(), 3);
        assert_eq!(result.pages[1].nodes.len(), 3);
        assert_eq!(result.pages[2].nodes.len(), 3);
        assert_eq!(result.pages[3].nodes.len(), 1);
    }

    #[test]
    fn pagination_profile_reports_height_usage_and_overflow_target() {
        let mut tree = FormTree::new();
        let mut fields = Vec::new();
        for i in 0..4 {
            let field = make_field(&mut tree, &format!("F{i}"), 200.0, 30.0);
            tree.meta_mut(field).xfa_id = Some(format!("row_{i}"));
            fields.push(field);
        }

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(100.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: fields,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let (layout, profile) = engine.layout_with_profile(root).unwrap();

        assert_eq!(layout.pages.len(), 2);
        assert_eq!(profile.pages.len(), 2);
        assert_eq!(profile.pages[0].page_height, 100.0);
        assert_eq!(profile.pages[0].used_height, 90.0);
        assert!(profile.pages[0].overflow_to_next);
        assert_eq!(
            profile.pages[0].first_overflow_element.as_deref(),
            Some("field#row_3 (h=30.0)")
        );
        assert_eq!(profile.pages[1].used_height, 30.0);
        assert!(!profile.pages[1].overflow_to_next);
        assert!(profile.pages[1].first_overflow_element.is_none());
    }

    #[test]
    fn pagination_with_page_area() {
        // PageArea with content area, content overflows to multiple pages
        let mut tree = FormTree::new();
        let mut fields = Vec::new();
        for i in 0..6 {
            fields.push(make_field(&mut tree, &format!("F{i}"), 200.0, 50.0));
        }

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 20.0,
                    y: 20.0,
                    width: 360.0,
                    height: 160.0, // fits 3 fields of 50pt
                    leader: None,
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let mut root_children = vec![page_area];
        root_children.extend(fields);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: root_children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // 6 fields × 50pt = 300pt, content area is 160pt → 2 pages
        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].nodes.len(), 3);
        assert_eq!(result.pages[1].nodes.len(), 3);

        // Nodes should be offset by content area position (20, 20)
        assert_eq!(result.pages[0].nodes[0].rect.x, 20.0);
        assert_eq!(result.pages[0].nodes[0].rect.y, 20.0);
        assert_eq!(result.pages[0].nodes[1].rect.y, 70.0); // 20 + 50
    }

    #[test]
    fn pagination_with_occur_repeating() {
        // Repeating subform creating many instances that overflow
        let mut tree = FormTree::new();
        let row = tree.add_node(FormNode {
            name: "DataRow".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(25.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::repeating(1, None, 8),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(100.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![row],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // 8 rows × 25pt = 200pt, page 100pt → 2 pages (4+4)
        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].nodes.len(), 4);
        assert_eq!(result.pages[1].nodes.len(), 4);
    }

    #[test]
    fn pagination_oversized_item_forced() {
        // Single item taller than page — should still be placed (forced)
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "Big", 200.0, 200.0); // taller than page
        let f2 = make_field(&mut tree, "Small", 200.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(100.0), // page shorter than f1
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // Big item forced onto page 1, Small on page 2
        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].nodes[0].name, "Big");
        assert_eq!(result.pages[1].nodes[0].name, "Small");
    }

    #[test]
    fn pagination_page_dimensions_correct() {
        // All pages should have correct dimensions
        let mut tree = FormTree::new();
        let mut fields = Vec::new();
        for i in 0..5 {
            fields.push(make_field(&mut tree, &format!("F{i}"), 200.0, 50.0));
        }

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(500.0),
                height: Some(120.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: fields,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        for page in &result.pages {
            assert_eq!(page.width, 500.0);
            assert_eq!(page.height, 120.0);
        }
    }

    // --- Content splitting tests (Epic 3.8) ---

    #[test]
    fn split_subform_across_pages() {
        // A tb-subform with 6 children of 30pt each (180pt total)
        // on a page with only 100pt remaining after a header.
        // The subform should be split: some children on page 1, rest on page 2.
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "Header", 300.0, 40.0);

        let mut sub_children = Vec::new();
        for i in 0..6 {
            sub_children.push(make_field(&mut tree, &format!("Row{i}"), 300.0, 30.0));
        }

        let subform = tree.add_node(FormNode {
            name: "DataBlock".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(300.0),
                height: None, // growable
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: sub_children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(160.0), // header(40) + 120pt left → fits 4 rows
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![header, subform],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // Page 1: header + partial subform (4 rows fit in 120pt)
        // Page 2: remaining 2 rows
        assert!(result.pages.len() >= 2);
        // Page 1 has header + partial subform
        let p1 = &result.pages[0];
        assert_eq!(p1.nodes[0].name, "Header");
        assert_eq!(p1.nodes[1].name, "DataBlock");
        let split_sub = &p1.nodes[1];
        assert_eq!(split_sub.children.len(), 4); // 4 rows fit

        // Page 2 has the remaining 2 rows
        let p2 = &result.pages[1];
        assert_eq!(p2.nodes.len(), 2);
    }

    #[test]
    fn split_preserves_node_positions() {
        // Verify that split children have correct y positions within their partial container
        let mut tree = FormTree::new();
        let mut sub_children = Vec::new();
        for i in 0..4 {
            sub_children.push(make_field(&mut tree, &format!("Row{i}"), 200.0, 25.0));
        }

        let subform = tree.add_node(FormNode {
            name: "Block".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: sub_children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(60.0), // fits 2 rows of 25pt (50pt < 60pt, 75pt > 60pt)
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![subform],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // First page: partial subform with 2 children
        let split_sub = &result.pages[0].nodes[0];
        assert_eq!(split_sub.children.len(), 2);
        assert_eq!(split_sub.children[0].rect.y, 0.0);
        assert_eq!(split_sub.children[1].rect.y, 25.0);
    }

    #[test]
    fn split_recurses_into_oversized_first_child() {
        let mut tree = FormTree::new();

        let mut rows = Vec::new();
        for i in 0..6 {
            rows.push(make_field(&mut tree, &format!("Row{i}"), 300.0, 30.0));
        }

        let inner = tree.add_node(FormNode {
            name: "InnerBlock".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(300.0),
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: rows,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let outer = tree.add_node(FormNode {
            name: "OuterBlock".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(300.0),
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![inner],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(100.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![outer],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].nodes[0].name, "OuterBlock");
        assert_eq!(result.pages[0].nodes[0].children[0].name, "InnerBlock");
        assert_eq!(result.pages[0].nodes[0].children[0].children.len(), 3);
    }

    #[test]
    fn no_split_for_non_tb_layout() {
        // A positioned subform should NOT be split — goes entirely to next page
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "Header", 300.0, 80.0);

        let f1 = tree.add_node(FormNode {
            name: "Child1".to_string(),
            node_type: FormNodeType::Field {
                value: "A".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(50.0),
                x: 0.0,
                y: 0.0,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let subform = tree.add_node(FormNode {
            name: "PositionedBlock".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(100.0), // fixed size, doesn't fit after header
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned, // can't split
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(100.0), // header takes 80pt, subform needs 100pt → overflow
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![header, subform],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // Header on page 1, positioned subform on page 2 (not split)
        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].nodes[0].name, "Header");
        assert_eq!(result.pages[1].nodes[0].name, "PositionedBlock");
    }

    #[test]
    fn layout_tb_splits_on_overflow() {
        let mut tree = FormTree::new();
        let mut children = Vec::new();
        for i in 0..10 {
            children.push(make_field(&mut tree, &format!("F{i}"), 200.0, 100.0));
        }
        let parent = make_subform(
            &mut tree,
            "Parent",
            LayoutStrategy::TopToBottom,
            Some(200.0),
            None,
            children,
        );

        let engine = LayoutEngine::new(&tree);
        let parent_children = tree.get(parent).children.clone();
        let nodes = engine
            .layout_tb(
                &parent_children,
                Size {
                    width: 200.0,
                    height: 300.0,
                },
            )
            .unwrap();

        // Only three 100pt children fit in a 300pt TB container.
        assert_eq!(nodes.len(), 3);
        assert!(nodes.iter().all(|n| n.rect.y + n.rect.height <= 301.0));
    }

    #[test]
    fn can_split_checks() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 20.0);

        let tb_sub = tree.add_node(FormNode {
            name: "TB".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let pos_sub = tree.add_node(FormNode {
            name: "Pos".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let empty_sub = tree.add_node(FormNode {
            name: "Empty".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        assert!(engine.can_split(tb_sub));
        // Positioned subforms without explicit height can now be split (#736).
        assert!(engine.can_split(pos_sub));
        assert!(!engine.can_split(empty_sub));
    }

    #[test]
    fn positioned_subform_paginates_across_pages() {
        // A positioned subform with many fields should split across pages
        // when its content exceeds the page content area (#736).
        let mut tree = FormTree::new();

        // Create 10 fields at y = 0, 80, 160, ..., 720
        // Each field is 60pt tall, total extent = 780pt.
        let mut fields = Vec::new();
        for i in 0..10 {
            let f = tree.add_node(FormNode {
                name: format!("F{i}"),
                node_type: FormNodeType::Field {
                    value: format!("Value{i}"),
                },
                box_model: BoxModel {
                    width: Some(200.0),
                    height: Some(60.0),
                    x: 10.0,
                    y: i as f64 * 80.0,
                    max_width: f64::MAX,
                    max_height: f64::MAX,
                    ..Default::default()
                },
                layout: LayoutStrategy::Positioned,
                children: vec![],
                occur: Occur::once(),
                font: FontMetrics::default(),
                calculate: None,
                validate: None,
                column_widths: vec![],
                col_span: 1,
            });
            fields.push(f);
        }

        // Positioned subform with no explicit height containing all fields.
        let positioned = tree.add_node(FormNode {
            name: "PositionedBody".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: fields,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        // Page area with 400pt content height (fits ~5 fields)
        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 400.0,
                    leader: None,
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![page_area, positioned],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // 10 fields at y=0,80,...,720 with height 60, page height 400.
        // Page 1: fields at y=0..319 (y+h<=400) = F0(0+60), F1(80+60), F2(160+60),
        //         F3(240+60), F4(320+60=380) -> 5 fields
        // Page 2: fields at y=400..719, shifted -> F5(0+60), F6(80+60), F7(160+60),
        //         F8(240+60), F9(320+60=380) -> 5 fields
        assert!(
            result.pages.len() >= 2,
            "Expected at least 2 pages, got {}",
            result.pages.len()
        );

        // Page 1 should have the positioned subform with some children
        let page1_children = count_leaf_nodes(&result.pages[0]);
        let page2_children = count_leaf_nodes(&result.pages[1]);
        assert!(page1_children > 0, "Page 1 should have content");
        assert!(page2_children > 0, "Page 2 should have content");
        assert_eq!(
            page1_children + page2_children,
            10,
            "All 10 fields should be placed across pages"
        );
    }

    #[test]
    fn positioned_split_preserves_parent_margin_offset() {
        use crate::types::Insets;

        let mut tree = FormTree::new();

        let first = tree.add_node(FormNode {
            name: "First".to_string(),
            node_type: FormNodeType::Field {
                value: "First".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(40.0),
                x: 0.0,
                y: 0.0,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let second = tree.add_node(FormNode {
            name: "Second".to_string(),
            node_type: FormNodeType::Field {
                value: "Second".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(40.0),
                x: 0.0,
                y: 60.0,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let positioned = tree.add_node(FormNode {
            name: "PositionedBody".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(140.0),
                margins: Insets {
                    top: 10.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 8.0,
                },
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![first, second],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 200.0,
                    height: 70.0,
                    leader: None,
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(70.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![page_area, positioned],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].nodes[0].children.len(), 1);
        assert_eq!(result.pages[1].nodes[0].children.len(), 1);
        assert_eq!(result.pages[0].nodes[0].children[0].rect.x, 8.0);
        assert_eq!(result.pages[0].nodes[0].children[0].rect.y, 10.0);
        assert_eq!(result.pages[1].nodes[0].children[0].rect.x, 8.0);
        assert_eq!(result.pages[1].nodes[0].children[0].rect.y, 10.0);
    }

    #[test]
    fn single_positioned_child_no_overpagination() {
        // #794: A TopToBottom root with a single Positioned child whose
        // children are all Positioned and fit on one page should produce
        // exactly 1 page, not 2+.
        let mut tree = FormTree::new();

        // 5 positioned fields that fit within a 792pt page
        let mut fields = Vec::new();
        for i in 0..5 {
            let f = tree.add_node(FormNode {
                name: format!("F{i}"),
                node_type: FormNodeType::Field {
                    value: format!("Value{i}"),
                },
                box_model: BoxModel {
                    width: Some(200.0),
                    height: Some(30.0),
                    x: 36.0,
                    y: 36.0 + i as f64 * 40.0,
                    max_width: f64::MAX,
                    max_height: f64::MAX,
                    ..Default::default()
                },
                layout: LayoutStrategy::Positioned,
                children: vec![],
                occur: Occur::once(),
                font: FontMetrics::default(),
                calculate: None,
                validate: None,
                column_widths: vec![],
                col_span: 1,
            });
            fields.push(f);
        }

        // Positioned subform containing all fields — fits on one page
        let positioned = tree.add_node(FormNode {
            name: "PageSubform".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: fields,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 612.0,
                    height: 792.0,
                    leader: None,
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![page_area, positioned],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(
            result.pages.len(),
            1,
            "Single positioned child fitting on one page should produce 1 page, got {}",
            result.pages.len()
        );

        // All 5 fields should be on the single page
        let leaf_count = count_leaf_nodes(&result.pages[0]);
        assert_eq!(leaf_count, 5, "All 5 fields should be placed on page 1");
    }

    #[test]
    fn positioned_inside_tb_no_infinite_pagination() {
        // Regression test for #737: a positioned subform nested inside a
        // tb-layout parent caused infinite pagination because the
        // children_override from split_positioned_node was discarded when
        // split_tb_node re-wrapped the rest.
        //
        // Structure:
        //   Root (tb)
        //     PageArea (content height = 300)
        //     TbWrapper (tb, no height)
        //       PositionedBody (positioned, no height)
        //         8 fields at y = 0, 80, 160, ..., 560 (each 60pt tall)
        //
        // Content spans 620pt.  With 300pt pages, we expect 3 pages max.
        // Before the fix, this produced MAX_PAGES pages.
        let mut tree = FormTree::new();

        let mut fields = Vec::new();
        for i in 0..8 {
            let f = tree.add_node(FormNode {
                name: format!("F{i}"),
                node_type: FormNodeType::Field {
                    value: format!("Val{i}"),
                },
                box_model: BoxModel {
                    width: Some(200.0),
                    height: Some(60.0),
                    x: 10.0,
                    y: i as f64 * 80.0,
                    max_width: f64::MAX,
                    max_height: f64::MAX,
                    ..Default::default()
                },
                layout: LayoutStrategy::Positioned,
                children: vec![],
                occur: Occur::once(),
                font: FontMetrics::default(),
                calculate: None,
                validate: None,
                column_widths: vec![],
                col_span: 1,
            });
            fields.push(f);
        }

        let positioned = tree.add_node(FormNode {
            name: "PositionedBody".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: fields,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        // Wrap the positioned subform in a tb-layout parent — this is the
        // configuration that triggered the bug.
        let tb_wrapper = tree.add_node(FormNode {
            name: "TbWrapper".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![positioned],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 300.0,
                    leader: None,
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(300.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![page_area, tb_wrapper],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // 8 fields spanning 620pt, pages of 300pt → should be 3 pages.
        // The critical assertion: we must NOT produce hundreds of pages.
        assert!(
            result.pages.len() <= 5,
            "Expected at most 5 pages for 8 fields across 300pt pages, got {} \
             (infinite pagination bug #737)",
            result.pages.len()
        );
        assert!(
            result.pages.len() >= 2,
            "Expected at least 2 pages, got {}",
            result.pages.len()
        );

        // All 8 fields should be distributed across the pages.
        let total_leaves: usize = result.pages.iter().map(count_leaf_nodes).sum();
        assert_eq!(
            total_leaves, 8,
            "All 8 fields should appear across pages, found {}",
            total_leaves
        );
    }

    #[test]
    fn positioned_subform_with_explicit_height_not_split() {
        // A positioned subform with explicit height should NOT be split.
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 20.0);

        let positioned = tree.add_node(FormNode {
            name: "FixedBlock".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(500.0), // explicit height
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        assert!(
            !engine.can_split(positioned),
            "Positioned subform with explicit height should not be splittable"
        );
    }

    // --- Leaders & trailers tests (Epic 3.10) ---

    #[test]
    fn leader_placed_at_top() {
        // Leader (header) should appear at the top of each page
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "PageHeader", 300.0, 30.0);
        let f1 = make_field(&mut tree, "Content1", 300.0, 50.0);
        let f2 = make_field(&mut tree, "Content2", 300.0, 50.0);

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 200.0,
                    leader: Some(header),
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![page_area, f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // Header at y=0, content starts at y=30
        assert_eq!(page.nodes[0].name, "PageHeader");
        assert_eq!(page.nodes[0].rect.y, 0.0);
        // Content after header
        assert!(page.nodes.len() >= 2);
        // First content node at y=30 (after header)
        let first_content = page.nodes.iter().find(|n| n.name == "Content1").unwrap();
        assert_eq!(first_content.rect.y, 30.0);
    }

    #[test]
    fn trailer_placed_at_bottom() {
        // Trailer (footer) should appear at the bottom of the content area
        let mut tree = FormTree::new();
        let footer = make_field(&mut tree, "PageFooter", 300.0, 25.0);
        let f1 = make_field(&mut tree, "Content1", 300.0, 50.0);

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 200.0,
                    leader: None,
                    trailer: Some(footer),
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![page_area, f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // Footer at bottom: y = 200 - 25 = 175
        let footer_node = page.nodes.iter().find(|n| n.name == "PageFooter").unwrap();
        assert_eq!(footer_node.rect.y, 175.0);
    }

    #[test]
    fn leader_and_trailer_reduce_content_space() {
        // With both leader and trailer, content space is reduced
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "Header", 300.0, 30.0);
        let footer = make_field(&mut tree, "Footer", 300.0, 20.0);

        // 5 fields of 30pt each = 150pt total
        let mut fields = Vec::new();
        for i in 0..5 {
            fields.push(make_field(&mut tree, &format!("F{i}"), 300.0, 30.0));
        }

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 200.0, // 200 - 30(header) - 20(footer) = 150pt for content
                    leader: Some(header),
                    trailer: Some(footer),
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let mut root_children = vec![page_area];
        root_children.extend(fields);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: root_children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // Content space is 150pt, 5 fields × 30pt = 150pt → exactly fits on 1 page
        assert_eq!(result.pages.len(), 1);
        let page = &result.pages[0];
        // header + 5 content + footer = 7 nodes
        assert_eq!(page.nodes.len(), 7);
    }

    #[test]
    fn leader_trailer_repeated_on_overflow_pages() {
        // When content overflows, leaders/trailers should appear on each page
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "Header", 300.0, 30.0);
        let footer = make_field(&mut tree, "Footer", 300.0, 20.0);

        // 8 fields of 30pt = 240pt, available per page = 200-30-20 = 150pt
        // Page 1: 5 fields (150pt), Page 2: 3 fields (90pt)
        let mut fields = Vec::new();
        for i in 0..8 {
            fields.push(make_field(&mut tree, &format!("F{i}"), 300.0, 30.0));
        }

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 200.0,
                    leader: Some(header),
                    trailer: Some(footer),
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let mut root_children = vec![page_area];
        root_children.extend(fields);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: root_children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 2);

        // Both pages should have header and footer
        for page in &result.pages {
            let has_header = page.nodes.iter().any(|n| n.name == "Header");
            let has_footer = page.nodes.iter().any(|n| n.name == "Footer");
            assert!(has_header, "Page missing header");
            assert!(has_footer, "Page missing footer");
        }
    }

    // ── Text placement tests ──────────────────────────────────────────

    #[test]
    fn draw_node_growable_height_from_text() {
        // A Draw node with fixed width but no height should grow to fit text
        let mut tree = FormTree::new();
        let draw = tree.add_node(FormNode {
            name: "Label".to_string(),
            node_type: FormNodeType::Draw(DrawContent::Text("Hello World".to_string())),
            box_model: BoxModel {
                width: Some(200.0),
                height: None, // growable
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(), // 10pt, avg_char_width=0.5
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let root = make_subform(
            &mut tree,
            "Root",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![draw],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        let label = &page.nodes[0];
        // "Hello World" = 11 chars * 10pt * 0.5 = 55pt wide, fits in 200pt
        // 1 line * 10pt * 1.2 = 12pt tall
        assert_eq!(label.rect.height, 12.0);
    }

    #[test]
    fn draw_node_text_wraps_in_narrow_width() {
        // A Draw node with narrow width should wrap text and grow taller
        let mut tree = FormTree::new();
        let draw = tree.add_node(FormNode {
            name: "Label".to_string(),
            node_type: FormNodeType::Draw(DrawContent::Text("Hello World".to_string())),
            box_model: BoxModel {
                width: Some(40.0), // narrow: "Hello" = 25pt fits, "World" wraps
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let root = make_subform(
            &mut tree,
            "Root",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![draw],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let label = &result.pages[0].nodes[0];
        // 2 lines * 12pt = 24pt
        assert_eq!(label.rect.height, 24.0);
    }

    #[test]
    fn field_produces_wrapped_text_content() {
        let mut tree = FormTree::new();
        let field = tree.add_node(FormNode {
            name: "Name".to_string(),
            node_type: FormNodeType::Field {
                value: "John".to_string(),
            },
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(20.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let root = make_subform(
            &mut tree,
            "Root",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![field],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let node = &result.pages[0].nodes[0];
        match &node.content {
            LayoutContent::WrappedText {
                lines, font_size, ..
            } => {
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0], "John");
                assert_eq!(*font_size, 10.0);
            }
            other => panic!("Expected WrappedText, got {:?}", other),
        }
    }

    #[test]
    fn draw_growable_width_and_height_from_text() {
        // Both width and height are growable: should size to text content
        let mut tree = FormTree::new();
        let draw = tree.add_node(FormNode {
            name: "Auto".to_string(),
            node_type: FormNodeType::Draw(DrawContent::Text("Test".to_string())),
            box_model: BoxModel {
                width: None,
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let size = engine.compute_extent(draw);
        // "Test" measured with Helvetica AFM widths: T=611 e=556 s=500 t=278 = 1945/1000*10 = 19.45
        assert!((size.width - 19.45).abs() < 0.1, "width={}", size.width);
        assert_eq!(size.height, 12.0);
    }

    #[test]
    fn custom_font_size_affects_layout() {
        let mut tree = FormTree::new();
        let draw = tree.add_node(FormNode {
            name: "Big".to_string(),
            node_type: FormNodeType::Draw(DrawContent::Text("Hi".to_string())),
            box_model: BoxModel {
                width: None,
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::new(20.0), // 20pt font
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let size = engine.compute_extent(draw);
        // "Hi" measured with Helvetica AFM widths: H=722 i=222 = 944/1000*20 = 18.88
        assert!((size.width - 18.88).abs() < 0.1, "width={}", size.width);
        assert_eq!(size.height, 24.0);
    }

    // =========================================================================
    // Table Layout Tests
    // =========================================================================

    fn make_cell(tree: &mut FormTree, name: &str, w: f64, h: f64, col_span: i32) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Field {
                value: name.to_string(),
            },
            box_model: BoxModel {
                width: Some(w),
                height: Some(h),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span,
        })
    }

    fn make_row(tree: &mut FormTree, name: &str, cells: Vec<FormNodeId>) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Row,
            children: cells,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn make_table(
        tree: &mut FormTree,
        name: &str,
        column_widths: Vec<f64>,
        rows: Vec<FormNodeId>,
    ) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Table,
            children: rows,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths,
            col_span: 1,
        })
    }

    #[test]
    fn table_basic_fixed_columns() {
        let mut tree = FormTree::new();

        // 3 columns: 100, 150, 200
        let c1 = make_cell(&mut tree, "A1", 100.0, 30.0, 1);
        let c2 = make_cell(&mut tree, "A2", 150.0, 30.0, 1);
        let c3 = make_cell(&mut tree, "A3", 200.0, 30.0, 1);
        let r1 = make_row(&mut tree, "Row1", vec![c1, c2, c3]);

        let c4 = make_cell(&mut tree, "B1", 100.0, 25.0, 1);
        let c5 = make_cell(&mut tree, "B2", 150.0, 25.0, 1);
        let c6 = make_cell(&mut tree, "B3", 200.0, 25.0, 1);
        let r2 = make_row(&mut tree, "Row2", vec![c4, c5, c6]);

        let table = make_table(&mut tree, "Table", vec![100.0, 150.0, 200.0], vec![r1, r2]);

        let page_area = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page_area).unwrap();

        assert_eq!(layout.pages.len(), 1);
        let page = &layout.pages[0];
        // Table node should be on the page
        assert_eq!(page.nodes.len(), 1);
        let table_node = &page.nodes[0];
        assert_eq!(table_node.name, "Table");

        // 2 rows
        assert_eq!(table_node.children.len(), 2);
        let row1 = &table_node.children[0];
        let row2 = &table_node.children[1];

        // Row 1: 3 cells at x=0, 100, 250
        assert_eq!(row1.children.len(), 3);
        assert_eq!(row1.children[0].rect.x, 0.0);
        assert_eq!(row1.children[0].rect.width, 100.0);
        assert_eq!(row1.children[1].rect.x, 100.0);
        assert_eq!(row1.children[1].rect.width, 150.0);
        assert_eq!(row1.children[2].rect.x, 250.0);
        assert_eq!(row1.children[2].rect.width, 200.0);

        // Row 2 stacked below row 1
        assert_eq!(row2.rect.y, 30.0); // row 1 height = 30
        assert_eq!(row2.children[0].rect.x, 0.0);
    }

    #[test]
    fn table_auto_columns() {
        let mut tree = FormTree::new();

        // Auto columns: -1 means auto-size
        let c1 = make_cell(&mut tree, "A", 80.0, 20.0, 1);
        let c2 = make_cell(&mut tree, "B", 120.0, 20.0, 1);
        let r1 = make_row(&mut tree, "Row1", vec![c1, c2]);

        let c3 = make_cell(&mut tree, "C", 60.0, 20.0, 1);
        let c4 = make_cell(&mut tree, "D", 150.0, 20.0, 1);
        let r2 = make_row(&mut tree, "Row2", vec![c3, c4]);

        // Auto-size: widest in col 0 = 80, col 1 = 150
        let table = make_table(&mut tree, "Table", vec![-1.0, -1.0], vec![r1, r2]);

        let page = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page).unwrap();

        let table_node = &layout.pages[0].nodes[0];
        let row1 = &table_node.children[0];

        // Column 0 auto-sized to 80 (widest), Column 1 auto-sized to 150
        assert_eq!(row1.children[0].rect.width, 80.0);
        assert_eq!(row1.children[1].rect.width, 150.0);
        assert_eq!(row1.children[1].rect.x, 80.0);
    }

    #[test]
    fn table_col_span() {
        let mut tree = FormTree::new();

        // 3 fixed columns: 100, 100, 100
        let c1 = make_cell(&mut tree, "Span2", 200.0, 20.0, 2); // spans 2 columns
        let c2 = make_cell(&mut tree, "Single", 100.0, 20.0, 1);
        let r1 = make_row(&mut tree, "Row1", vec![c1, c2]);

        let table = make_table(&mut tree, "Table", vec![100.0, 100.0, 100.0], vec![r1]);

        let page = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page).unwrap();

        let row = &layout.pages[0].nodes[0].children[0];
        // First cell spans 2 columns: width = 100 + 100 = 200
        assert_eq!(row.children[0].rect.width, 200.0);
        assert_eq!(row.children[0].rect.x, 0.0);
        // Second cell at x=200, width=100
        assert_eq!(row.children[1].rect.x, 200.0);
        assert_eq!(row.children[1].rect.width, 100.0);
    }

    #[test]
    fn table_col_span_rest() {
        let mut tree = FormTree::new();

        // 3 fixed columns: 100, 100, 100
        let c1 = make_cell(&mut tree, "First", 100.0, 20.0, 1);
        let c2 = make_cell(&mut tree, "Rest", 200.0, 20.0, -1); // span remaining
        let r1 = make_row(&mut tree, "Row1", vec![c1, c2]);

        let table = make_table(&mut tree, "Table", vec![100.0, 100.0, 100.0], vec![r1]);

        let page = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page).unwrap();

        let row = &layout.pages[0].nodes[0].children[0];
        // First cell: width=100 at x=0
        assert_eq!(row.children[0].rect.width, 100.0);
        // Second cell: spans remaining = 100 + 100 = 200 at x=100
        assert_eq!(row.children[1].rect.x, 100.0);
        assert_eq!(row.children[1].rect.width, 200.0);
    }

    #[test]
    fn table_row_height_equalization() {
        let mut tree = FormTree::new();

        // Cells with different heights: 30, 50, 20
        let c1 = make_cell(&mut tree, "Short", 100.0, 30.0, 1);
        let c2 = make_cell(&mut tree, "Tall", 100.0, 50.0, 1);
        let c3 = make_cell(&mut tree, "Tiny", 100.0, 20.0, 1);
        let r1 = make_row(&mut tree, "Row1", vec![c1, c2, c3]);

        let table = make_table(&mut tree, "Table", vec![100.0, 100.0, 100.0], vec![r1]);

        let page = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page).unwrap();

        let row = &layout.pages[0].nodes[0].children[0];
        // All cells should have height = 50 (tallest cell)
        assert_eq!(row.children[0].rect.height, 50.0);
        assert_eq!(row.children[1].rect.height, 50.0);
        assert_eq!(row.children[2].rect.height, 50.0);
        // Row itself should be 50
        assert_eq!(row.rect.height, 50.0);
    }

    #[test]
    fn table_growable_height() {
        let mut tree = FormTree::new();

        let c1 = make_cell(&mut tree, "A", 100.0, 30.0, 1);
        let r1 = make_row(&mut tree, "Row1", vec![c1]);

        let c2 = make_cell(&mut tree, "B", 100.0, 40.0, 1);
        let r2 = make_row(&mut tree, "Row2", vec![c2]);

        // Table with no explicit height (growable)
        let table = make_table(&mut tree, "Table", vec![100.0], vec![r1, r2]);

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(table);

        // Table height = sum of row heights = 30 + 40 = 70
        assert_eq!(extent.height, 70.0);
        // Table width = column width = 100
        assert_eq!(extent.width, 100.0);
    }

    #[test]
    fn table_empty() {
        let mut tree = FormTree::new();
        let table = make_table(&mut tree, "EmptyTable", vec![100.0, 200.0], vec![]);

        let page = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page).unwrap();

        // Table exists but has no row children
        let table_node = &layout.pages[0].nodes[0];
        assert_eq!(table_node.children.len(), 0);
    }

    #[test]
    fn table_splits_across_pages() {
        let mut tree = FormTree::new();

        // Rows of 100pt height
        let mut rows = Vec::new();
        for i in 0..10 {
            let cell = make_cell(&mut tree, &format!("C{}", i), 200.0, 100.0, 1);
            let row = make_row(&mut tree, &format!("Row{}", i), vec![cell]);
            rows.push(row);
        }

        // Table with 10 rows = 1000pt total height
        let table = make_table(&mut tree, "Table", vec![200.0], rows);

        // Page area of 400pt height. Should fit 4 rows per page.
        let page_area = tree.add_node(FormNode {
            name: "PageArea".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 400.0,
                    leader: None,
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![page_area, table],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // Should have 3 pages (4 rows, 4 rows, 2 rows)
        assert_eq!(result.pages.len(), 3);
    }

    #[test]
    fn resolve_display_value_maps_save_to_display() {
        let meta = FormNodeMeta {
            field_kind: FieldKind::Dropdown,
            display_items: vec![
                "United States".to_string(),
                "United Kingdom".to_string(),
                "Canada".to_string(),
            ],
            save_items: vec!["US".to_string(), "UK".to_string(), "CA".to_string()],
            ..Default::default()
        };

        // Save value "UK" should resolve to "United Kingdom"
        assert_eq!(resolve_display_value("UK", &meta), "United Kingdom");
        // Save value "CA" should resolve to "Canada"
        assert_eq!(resolve_display_value("CA", &meta), "Canada");
        // Unknown value stays as-is
        assert_eq!(resolve_display_value("DE", &meta), "DE");
        // Empty value stays empty
        assert_eq!(resolve_display_value("", &meta), "");
    }

    #[test]
    fn resolve_display_value_no_save_items_passthrough() {
        let meta = FormNodeMeta {
            field_kind: FieldKind::Dropdown,
            display_items: vec!["Red".to_string(), "Green".to_string()],
            ..Default::default()
        };
        // No save_items — value passes through unchanged
        assert_eq!(resolve_display_value("Red", &meta), "Red");
    }

    #[test]
    fn resolve_display_value_non_dropdown_passthrough() {
        let meta = FormNodeMeta {
            field_kind: FieldKind::Text,
            save_items: vec!["US".to_string()],
            display_items: vec!["United States".to_string()],
            ..Default::default()
        };
        // Non-dropdown field: no resolution
        assert_eq!(resolve_display_value("US", &meta), "US");
    }

    #[test]
    fn resolve_display_value_numeric_edit_strips_trailing_zeros() {
        let meta = FormNodeMeta {
            field_kind: FieldKind::NumericEdit,
            ..Default::default()
        };

        assert_eq!(resolve_display_value("1.00000000", &meta), "1");
        assert_eq!(resolve_display_value("3.50", &meta), "3.5");
        assert_eq!(resolve_display_value("100.00", &meta), "100");
        assert_eq!(resolve_display_value("0.12345", &meta), "0.12345");
        assert_eq!(resolve_display_value("42", &meta), "42");
        // Non-numeric value passes through
        assert_eq!(resolve_display_value("abc", &meta), "abc");
        assert_eq!(resolve_display_value("", &meta), "");
    }

    #[test]
    fn resolve_display_value_date_time_picker_uses_iso_date_prefix() {
        let meta = FormNodeMeta {
            field_kind: FieldKind::DateTimePicker,
            ..Default::default()
        };

        assert_eq!(resolve_display_value("2026-04-12", &meta), "2026-04-12");
        assert_eq!(
            resolve_display_value("2026-04-12T13:45:00Z", &meta),
            "2026-04-12"
        );
        assert_eq!(
            resolve_display_value("2026-04-12T13:45:00+02:00", &meta),
            "2026-04-12"
        );
        // Non-ISO values pass through unchanged until picture clauses exist.
        assert_eq!(resolve_display_value("12/04/2026", &meta), "12/04/2026");
        assert_eq!(resolve_display_value("abc", &meta), "abc");
        assert_eq!(resolve_display_value("", &meta), "");
    }

    // -----------------------------------------------------------------------
    // XFA-F8-02 (#1118): estimated_heap_bytes
    // -----------------------------------------------------------------------

    #[test]
    fn estimated_heap_bytes_is_positive_for_non_empty_layout() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "Field1", 100.0, 20.0);
        let f2 = make_field(&mut tree, "Field2", 100.0, 20.0);
        let root = make_subform(
            &mut tree,
            "Root",
            LayoutStrategy::TopToBottom,
            Some(200.0),
            Some(200.0),
            vec![f1, f2],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(root).unwrap();

        // A layout with two fields must report non-zero heap usage.
        assert!(!layout.pages.is_empty(), "expected at least one page");
        let bytes = layout.estimated_heap_bytes();
        assert!(
            bytes > 0,
            "estimated_heap_bytes should be > 0 for non-empty layout"
        );
    }
}

#[cfg(test)]
mod halign_tests {
    use super::*;
    use crate::form::{FormNode, FormNodeType, FormTree, Occur};
    use crate::text::FontMetrics;
    use crate::types::{BoxModel, LayoutStrategy, TextAlign};

    fn make_field(tree: &mut FormTree, name: &str, w: f64, h: f64) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Field {
                value: name.to_string(),
            },
            box_model: BoxModel {
                width: Some(w),
                height: Some(h),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn make_subform(
        tree: &mut FormTree,
        name: &str,
        strategy: LayoutStrategy,
        w: Option<f64>,
        h: Option<f64>,
        children: Vec<FormNodeId>,
    ) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: w,
                height: h,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: strategy,
            children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    /// XFA Spec 3.3 §8.3 Example 8.13 (p284):
    /// TB parent w=10cm, child w=8cm hAlign="right" → child at x=2cm.
    #[test]
    fn tb_halign_right_offsets_child() {
        let mut tree = FormTree::new();
        let child = make_field(&mut tree, "A", 200.0, 30.0);
        tree.meta_mut(child).style.h_align = Some(TextAlign::Right);

        let parent = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(500.0),
            Some(500.0),
            vec![child],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(parent).unwrap();

        let child_node = &layout.pages[0].nodes[0];
        // child_w=200, parent_w=500 → x = 500 - 200 = 300
        assert_eq!(child_node.rect.x, 300.0);
    }

    /// hAlign="center" centers child within TB parent.
    #[test]
    fn tb_halign_center_centers_child() {
        let mut tree = FormTree::new();
        let child = make_field(&mut tree, "A", 200.0, 30.0);
        tree.meta_mut(child).style.h_align = Some(TextAlign::Center);

        let parent = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(500.0),
            Some(500.0),
            vec![child],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(parent).unwrap();

        let child_node = &layout.pages[0].nodes[0];
        // (500 - 200) / 2 = 150
        assert_eq!(child_node.rect.x, 150.0);
    }

    /// Default hAlign (left) keeps x=0 in TB layout.
    #[test]
    fn tb_halign_default_left() {
        let mut tree = FormTree::new();
        let child = make_field(&mut tree, "A", 200.0, 30.0);
        // No h_align set — defaults to left

        let parent = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(500.0),
            Some(500.0),
            vec![child],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(parent).unwrap();

        let child_node = &layout.pages[0].nodes[0];
        assert_eq!(child_node.rect.x, 0.0);
    }

    /// XFA Spec 3.3 §8.3 Example 8.12 (p284):
    /// LR-TB parent w=10, three children w=2 hAlign="right" →
    /// row right-aligned: A at x=4, B at x=6, C at x=8.
    #[test]
    fn lr_tb_halign_right_shifts_row() {
        let mut tree = FormTree::new();
        let a = make_field(&mut tree, "A", 60.0, 20.0);
        let b = make_field(&mut tree, "B", 60.0, 20.0);
        let c = make_field(&mut tree, "C", 60.0, 20.0);
        tree.meta_mut(a).style.h_align = Some(TextAlign::Right);
        tree.meta_mut(b).style.h_align = Some(TextAlign::Right);
        tree.meta_mut(c).style.h_align = Some(TextAlign::Right);

        let parent = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::LeftToRightTB,
            Some(300.0),
            Some(300.0),
            vec![a, b, c],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(parent).unwrap();
        let page = &layout.pages[0];

        // Row width = 3 * 60 = 180, parent = 300 → offset = 120
        assert_eq!(page.nodes[0].rect.x, 120.0); // A
        assert_eq!(page.nodes[1].rect.x, 180.0); // B
        assert_eq!(page.nodes[2].rect.x, 240.0); // C
    }

    /// RL-TB with explicit hAlign="left" overrides default right flow.
    #[test]
    fn rl_tb_halign_left_overrides_flow() {
        let mut tree = FormTree::new();
        let child = make_field(&mut tree, "A", 100.0, 30.0);
        tree.meta_mut(child).style.h_align = Some(TextAlign::Left);

        let parent = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::RightToLeftTB,
            Some(500.0),
            Some(500.0),
            vec![child],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(parent).unwrap();

        let child_node = &layout.pages[0].nodes[0];
        // hAlign="left" in rl-tb → x=0
        assert_eq!(child_node.rect.x, 0.0);
    }

    // -------------------------------------------------------------------
    // anchorType tests (XFA 3.3 §2.6 + App A p1510)
    // -------------------------------------------------------------------

    fn make_positioned_field(
        tree: &mut FormTree,
        name: &str,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Field {
                value: name.to_string(),
            },
            box_model: BoxModel {
                width: Some(w),
                height: Some(h),
                x,
                y,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn layout_with_anchor(anchor: crate::form::AnchorType, x: f64, y: f64, w: f64, h: f64) -> Rect {
        let mut tree = FormTree::new();
        let field = make_positioned_field(&mut tree, "F", x, y, w, h);
        tree.meta_mut(field).anchor_type = anchor;
        let root = make_subform(
            &mut tree,
            "Root",
            LayoutStrategy::Positioned,
            Some(612.0),
            Some(792.0),
            vec![field],
        );
        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();
        result.pages[0].nodes[0].rect
    }

    #[test]
    fn anchor_top_left_no_adjustment() {
        use crate::form::AnchorType;
        let r = layout_with_anchor(AnchorType::TopLeft, 100.0, 200.0, 80.0, 40.0);
        assert_eq!(r.x, 100.0);
        assert_eq!(r.y, 200.0);
        assert_eq!(r.width, 80.0);
        assert_eq!(r.height, 40.0);
    }
    #[test]
    fn anchor_top_center() {
        use crate::form::AnchorType;
        let r = layout_with_anchor(AnchorType::TopCenter, 100.0, 200.0, 80.0, 40.0);
        assert_eq!(r.x, 60.0);
        assert_eq!(r.y, 200.0);
    }
    #[test]
    fn anchor_top_right() {
        use crate::form::AnchorType;
        let r = layout_with_anchor(AnchorType::TopRight, 100.0, 200.0, 80.0, 40.0);
        assert_eq!(r.x, 20.0);
        assert_eq!(r.y, 200.0);
    }
    #[test]
    fn anchor_middle_left() {
        use crate::form::AnchorType;
        let r = layout_with_anchor(AnchorType::MiddleLeft, 100.0, 200.0, 80.0, 40.0);
        assert_eq!(r.x, 100.0);
        assert_eq!(r.y, 180.0);
    }
    #[test]
    fn anchor_middle_center() {
        use crate::form::AnchorType;
        let r = layout_with_anchor(AnchorType::MiddleCenter, 100.0, 200.0, 80.0, 40.0);
        assert_eq!(r.x, 60.0);
        assert_eq!(r.y, 180.0);
    }
    #[test]
    fn anchor_middle_right() {
        use crate::form::AnchorType;
        let r = layout_with_anchor(AnchorType::MiddleRight, 100.0, 200.0, 80.0, 40.0);
        assert_eq!(r.x, 20.0);
        assert_eq!(r.y, 180.0);
    }
    #[test]
    fn anchor_bottom_left() {
        use crate::form::AnchorType;
        let r = layout_with_anchor(AnchorType::BottomLeft, 100.0, 200.0, 80.0, 40.0);
        assert_eq!(r.x, 100.0);
        assert_eq!(r.y, 160.0);
    }
    #[test]
    fn anchor_bottom_center() {
        use crate::form::AnchorType;
        let r = layout_with_anchor(AnchorType::BottomCenter, 100.0, 200.0, 80.0, 40.0);
        assert_eq!(r.x, 60.0);
        assert_eq!(r.y, 160.0);
    }
    #[test]
    fn anchor_bottom_right() {
        use crate::form::AnchorType;
        let r = layout_with_anchor(AnchorType::BottomRight, 100.0, 200.0, 80.0, 40.0);
        assert_eq!(r.x, 20.0);
        assert_eq!(r.y, 160.0);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// #1102  XFA-F4-05: area, exclGroup, subformSet container nodes
// XFA 3.3 Appendix B — Layout Objects
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod container_node_tests {
    use super::*;
    use crate::form::{FormNode, FormNodeType, FormTree, Occur};
    use crate::text::FontMetrics;
    use crate::types::{BoxModel, LayoutStrategy};

    fn make_field(tree: &mut FormTree, name: &str, x: f64, y: f64, w: f64, h: f64) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Field {
                value: name.to_string(),
            },
            box_model: BoxModel {
                width: Some(w),
                height: Some(h),
                x,
                y,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn make_container(
        tree: &mut FormTree,
        name: &str,
        node_type: FormNodeType,
        strategy: LayoutStrategy,
        w: f64,
        h: f64,
        children: Vec<FormNodeId>,
    ) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type,
            box_model: BoxModel {
                width: Some(w),
                height: Some(h),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: strategy,
            children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    /// XFA 3.3 Appendix B — `<area>` is a positioned container.
    /// Children have absolute positions within the area. The area itself
    /// is treated exactly like a Subform with positioned layout.
    #[test]
    fn area_node_positions_children_absolutely() {
        let mut tree = FormTree::new();
        let child = make_field(&mut tree, "Child", 10.0, 20.0, 50.0, 15.0);
        let area = make_container(
            &mut tree,
            "MyArea",
            FormNodeType::Area,
            LayoutStrategy::Positioned,
            200.0,
            100.0,
            vec![child],
        );
        // Root: TB with the area as only child
        let root = make_container(
            &mut tree,
            "Root",
            FormNodeType::Subform,
            LayoutStrategy::TopToBottom,
            200.0,
            200.0,
            vec![area],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        let area_node = &result.pages[0].nodes[0];
        assert_eq!(area_node.name, "MyArea");
        assert_eq!(area_node.children.len(), 1);
        // Child positioned at (10, 20) within the area
        let child_node = &area_node.children[0];
        assert_eq!(child_node.name, "Child");
        assert_eq!(child_node.rect.x, 10.0);
        assert_eq!(child_node.rect.y, 20.0);
    }

    /// XFA 3.3 §7.2 — `<exclGroup>` lays out radio-button fields top-to-bottom.
    /// From a layout perspective, each child field is rendered normally.
    #[test]
    fn excl_group_lays_out_children_top_to_bottom() {
        let mut tree = FormTree::new();
        let opt_a = make_field(&mut tree, "OptionA", 0.0, 0.0, 100.0, 20.0);
        let opt_b = make_field(&mut tree, "OptionB", 0.0, 0.0, 100.0, 20.0);
        let excl = make_container(
            &mut tree,
            "MyGroup",
            FormNodeType::ExclGroup,
            LayoutStrategy::TopToBottom,
            200.0,
            100.0,
            vec![opt_a, opt_b],
        );
        let root = make_container(
            &mut tree,
            "Root",
            FormNodeType::Subform,
            LayoutStrategy::TopToBottom,
            200.0,
            200.0,
            vec![excl],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        let group_node = &result.pages[0].nodes[0];
        assert_eq!(group_node.name, "MyGroup");
        assert_eq!(group_node.children.len(), 2);
        // OptionA at y=0, OptionB stacked below at y=20
        assert_eq!(group_node.children[0].rect.y, 0.0);
        assert_eq!(group_node.children[1].rect.y, 20.0);
    }

    /// XFA 3.3 §7.1 — `<subformSet>` is transparent: its children appear as
    /// direct siblings of the containing subform's children.
    #[test]
    fn subform_set_is_transparent_container() {
        let mut tree = FormTree::new();
        let field_a = make_field(&mut tree, "A", 0.0, 0.0, 100.0, 20.0);
        let field_b = make_field(&mut tree, "B", 0.0, 0.0, 100.0, 20.0);
        // SubformSet wrapping two fields — should be transparent
        let set = make_container(
            &mut tree,
            "MySet",
            FormNodeType::SubformSet,
            LayoutStrategy::TopToBottom,
            200.0,
            100.0,
            vec![field_a, field_b],
        );
        let root = make_container(
            &mut tree,
            "Root",
            FormNodeType::Subform,
            LayoutStrategy::TopToBottom,
            200.0,
            200.0,
            vec![set],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        // SubformSet itself may appear as a container node; its children should be present
        let page = &result.pages[0];
        fn count_named(nodes: &[LayoutNode], name: &str) -> usize {
            nodes
                .iter()
                .map(|n| usize::from(n.name == name) + count_named(&n.children, name))
                .sum()
        }
        assert!(
            count_named(&page.nodes, "A") >= 1,
            "Field A should appear in layout"
        );
        assert!(
            count_named(&page.nodes, "B") >= 1,
            "Field B should appear in layout"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// #1103  XFA-F4-06: Keep chains and orderedOccurrence
// XFA 3.3 §8.9 Adhesion (keep) + §8.8 orderedOccurrence
// ─────────────────────────────────────────────────────────────────────────────
//
// orderedOccurrence: Repeating subform instances maintain their sequential
// order (first occurrence first, last last).  This is the default behaviour
// when `expand_occur` emits IDs in source order.  If the data-driven
// count matches, orderedOccurrence is satisfied without extra work.
// See `expand_occur` and the pagination loop in `layout_content_on_page`.
//
// Keep chains are tested below.
#[cfg(test)]
mod keep_chain_tests {
    use super::*;
    use crate::form::{FormNode, FormNodeType, FormTree, Occur};
    use crate::text::FontMetrics;
    use crate::types::{BoxModel, LayoutStrategy};

    fn make_field(tree: &mut FormTree, name: &str, w: f64, h: f64) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Field {
                value: name.to_string(),
            },
            box_model: BoxModel {
                width: Some(w),
                height: Some(h),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    /// XFA 3.3 §8.9 Adhesion — keep.next keeps a node on the same page as
    /// the following sibling.  When the heading + body together would not fit
    /// on the current page, both are pushed to the next page.
    ///
    /// Setup:
    ///   page height = 100pt
    ///   filler = 70pt (placed first, leaves 30pt)
    ///   heading = 40pt  with keep_next_content_area = true
    ///   body    = 40pt  (the kept-with node)
    ///
    /// Without keep: heading(40pt) > 30pt remaining → goes to page 2, body on page 2.
    /// With keep (chain height = 80pt > 30pt): both pushed to page 2 together.
    #[test]
    fn keep_next_pushes_heading_and_body_to_same_page() {
        let mut tree = FormTree::new();

        let filler = make_field(&mut tree, "Filler", 200.0, 70.0);
        let heading = make_field(&mut tree, "Heading", 200.0, 40.0);
        let body = make_field(&mut tree, "Body", 200.0, 40.0);

        // Mark heading as keep-with-next
        tree.meta_mut(heading).keep_next_content_area = true;

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(100.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![filler, heading, body],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // Should produce 2 pages
        assert_eq!(
            result.pages.len(),
            2,
            "expected filler on page 1, heading+body on page 2"
        );

        // Page 1: only the filler
        let p1_names: Vec<&str> = result.pages[0]
            .nodes
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert!(p1_names.contains(&"Filler"), "Filler should be on page 1");
        assert!(
            !p1_names.contains(&"Heading"),
            "Heading should NOT be on page 1"
        );
        assert!(!p1_names.contains(&"Body"), "Body should NOT be on page 1");

        // Page 2: heading and body together
        let p2_names: Vec<&str> = result.pages[1]
            .nodes
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert!(p2_names.contains(&"Heading"), "Heading should be on page 2");
        assert!(p2_names.contains(&"Body"), "Body should be on page 2");
    }
}
