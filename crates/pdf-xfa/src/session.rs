//! `XfaSession` — parse-once interactive fill session over an XFA PDF.
//!
//! Phase 1 SDK foundation: enumerate the currently visible/layouted fields,
//! set values (with write-through to the bound data nodes), and save a PDF
//! whose datasets packet carries the filled values so Adobe Acrobat/Reader
//! reopens the form with them.
//!
//! Explicit non-goals at this phase (see the UEA gap analysis): no
//! change/click event execution, no dynamic re-layout/reflow after a value
//! write, no instanceManager add/remove, no form-packet *structural* writes.
//!
//! ```no_run
//! use pdf_xfa::session::{XfaSession, XfaWriteValue};
//!
//! let bytes = std::fs::read("form.pdf").unwrap();
//! let mut session = XfaSession::open(&bytes).unwrap();
//! println!("pages: {}", session.page_count());
//! for f in session.fields() {
//!     println!("{} = {:?}", f.name, f.value);
//! }
//! session
//!     .set_value("form1.applicant.name", XfaWriteValue::Text("Alice"))
//!     .unwrap();
//! let filled = session.save_to_bytes().unwrap();
//! std::fs::write("filled.pdf", filled).unwrap();
//! ```

use std::collections::HashMap;

use lopdf::Document;
use xfa_dom_resolver::data_dom::{DataDom, DataNode, DataNodeId};
use xfa_layout_engine::form::{
    Access, FieldKind, FormNodeId, FormNodeType, FormTree, GroupKind, Presence,
};
use xfa_layout_engine::layout::{LayoutContent, LayoutDom, LayoutEngine, LayoutNode};

use crate::datasets_writeback::{self, DatasetsEdit, FormPacketEdit};
use crate::dynamic::apply_dynamic_scripts;
use crate::error::{Result, XfaError};
use crate::extract::{extract_xfa_from_bytes, XfaPackets};
use crate::flatten::{
    apply_form_dom_presence, extract_embedded_images, inject_resolved_metrics,
    resolve_template_fonts, XfaRenderingPolicy,
};
use crate::merger::FormMerger;

/// A value to write into an XFA field.
#[derive(Debug, Clone, Copy)]
pub enum XfaWriteValue<'a> {
    /// Text-like fields: text, multiline text, numeric, date/time, password,
    /// and choice lists (pass the save value or the display value).
    Text(&'a str),
    /// Checkbox: `true` selects the on-value, `false` the off-value.
    Checkbox(bool),
    /// Radio group (exclGroup): the on-value of the member to select.
    Radio(&'a str),
}

/// Public field type, mirroring [`FieldKind`] without leaking the
/// layout-engine type into the SDK surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfaFieldType {
    /// Single-line or multiline text edit.
    Text,
    /// Standalone checkbox.
    Checkbox,
    /// Exclusive radio group (exclGroup with checkButton members).
    RadioGroup,
    /// Push button (not fillable).
    Button,
    /// Dropdown / choice list.
    Dropdown,
    /// Signature field (not fillable in Phase 1).
    Signature,
    /// Date/time picker (accepts text in Phase 1).
    DateTime,
    /// Numeric edit (accepts text in Phase 1).
    Numeric,
    /// Password edit.
    Password,
    /// Image edit (not fillable in Phase 1).
    Image,
    /// Barcode (not fillable).
    Barcode,
}

impl XfaFieldType {
    fn from_kind(kind: FieldKind) -> Self {
        match kind {
            FieldKind::Text => XfaFieldType::Text,
            FieldKind::Checkbox => XfaFieldType::Checkbox,
            FieldKind::Radio => XfaFieldType::RadioGroup,
            FieldKind::Button => XfaFieldType::Button,
            FieldKind::Dropdown => XfaFieldType::Dropdown,
            FieldKind::Signature => XfaFieldType::Signature,
            FieldKind::DateTimePicker => XfaFieldType::DateTime,
            FieldKind::NumericEdit => XfaFieldType::Numeric,
            FieldKind::PasswordEdit => XfaFieldType::Password,
            FieldKind::ImageEdit => XfaFieldType::Image,
            FieldKind::Barcode => XfaFieldType::Barcode,
        }
    }

    /// Whether [`XfaSession::set_value`] accepts writes for this type.
    pub fn is_fillable(self) -> bool {
        !matches!(
            self,
            XfaFieldType::Button
                | XfaFieldType::Signature
                | XfaFieldType::Image
                | XfaFieldType::Barcode
        )
    }
}

/// Axis-aligned rectangle in page space, points, top-left origin (XFA
/// coordinate convention: y grows downward from the top of the page).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XfaRect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// One layouted widget occurrence of a field.
#[derive(Debug, Clone)]
pub struct XfaWidget {
    /// 0-based page index in the layout result.
    pub page: usize,
    /// Widget rectangle (page space, top-left origin, points).
    pub rect: XfaRect,
    /// For radio groups: the on-value asserted by this member widget.
    pub on_value: Option<String>,
}

/// A selectable option of a dropdown / list field.
#[derive(Debug, Clone)]
pub struct XfaFieldOption {
    /// User-visible text.
    pub display: String,
    /// Persisted save value (equals `display` when the template defines no
    /// separate save items).
    pub save: String,
}

/// SDK-level model of one fillable XFA form field, derived from the merged
/// form tree and the current layout.
#[derive(Debug, Clone)]
pub struct XfaFieldModel {
    /// Fully-qualified dotted name with `[n]` indices omitted when 0
    /// (Adobe display-SOM style), e.g. `form1.applicant.name` or
    /// `form1.rows.row[2].amount`.
    pub name: String,
    /// Fully-qualified SOM path with explicit `[n]` on every segment,
    /// e.g. `form1[0].rows[0].row[2].amount[0]`.
    pub som_path: String,
    /// Field type.
    pub field_type: XfaFieldType,
    /// Current value (the form-tree raw value; radio groups report the
    /// selected member's on-value, empty when none selected).
    pub value: String,
    /// Effective read-only state: the field or an ancestor container sets
    /// `access="readOnly" | "protected" | "nonInteractive"`.
    pub read_only: bool,
    /// Mandatory field (`<validate nullTest="error">`).
    pub required: bool,
    /// Multiline text edit (`<ui><textEdit multiLine="1">`).
    pub multiline: bool,
    /// Hidden/invisible/inactive via XFA presence (template, saved form
    /// state, or script-derived).
    pub hidden: bool,
    /// Dropdown/list options. For radio groups: one option per member
    /// (display = member on-value).
    pub options: Vec<XfaFieldOption>,
    /// Checkbox on-value (`<items>` / checkButton binding), when known.
    pub on_value: Option<String>,
    /// Checkbox off-value, when known.
    pub off_value: Option<String>,
    /// First page this field appears on (0-based), `None` when the field is
    /// not part of the current layout (e.g. hidden).
    pub page: Option<usize>,
    /// Rectangle of the first widget occurrence.
    pub rect: Option<XfaRect>,
    /// All layouted widget occurrences (radio groups: one per member).
    pub widgets: Vec<XfaWidget>,
    /// Whether the field has a bound data node in the datasets DOM. Values
    /// written to unbound fields are persisted by creating the data node on
    /// demand (default binding), except for `bind match="none"` fields.
    pub bound_to_data: bool,
    /// `<bind match="none">` — the field deliberately does not participate
    /// in data binding; Adobe persists such values only in the form packet.
    pub bind_none: bool,
}

/// Outcome of a successful [`XfaSession::set_value`] call.
#[derive(Debug, Clone)]
pub struct XfaSetOutcome {
    /// The normalized value written to the form tree (e.g. the resolved
    /// save value of a dropdown, or a checkbox on/off value).
    pub raw_value: String,
    /// The value was written through to a datasets node (existing or
    /// created on demand) and will be persisted by [`XfaSession::save_to_bytes`].
    pub persisted_to_datasets: bool,
}

/// Report from a save/writeback.
#[derive(Debug, Clone, Default)]
pub struct XfaWritebackReport {
    /// Datasets packet rewritten by surgical splice (original XML preserved
    /// except for the changed values). `false` = lossless splice failed and
    /// the data section was regenerated from the data DOM instead.
    pub datasets_spliced: bool,
    /// Number of `<value>` updates applied to the saved form packet (kept in
    /// sync so Adobe's `restoreState` does not resurrect stale values).
    pub form_packet_values_updated: usize,
}

struct FieldEntry {
    /// The field node (for radio groups: the exclGroup node).
    node: FormNodeId,
    /// Member fields of a radio group, with their on-values.
    members: Vec<(FormNodeId, String)>,
    /// Effective access at enumeration time.
    read_only: bool,
    /// Path of named segments from the root, with same-name sibling indices.
    segments: Vec<(String, usize)>,
}

/// Parse-once session over a single XFA PDF.
///
/// Holds the extracted packets, the parsed datasets DOM, the merged form
/// tree, and the layout result. Field enumeration and value writes operate
/// on the cached state; no re-parse or re-layout happens per call (Phase 1:
/// value writes do not reflow the layout).
pub struct XfaSession {
    pdf_bytes: Vec<u8>,
    packets: XfaPackets,
    data_dom: DataDom,
    tree: FormTree,
    layout: LayoutDom,
    fields: Vec<FieldEntry>,
    models: Vec<XfaFieldModel>,
    by_name: HashMap<String, usize>,
    /// Data nodes whose values changed (existing nodes).
    dirty_data: Vec<DataNodeId>,
    /// Data nodes created on demand: (id, parent path at creation time).
    created_data: Vec<DataNodeId>,
    /// Form-packet value syncs to apply at save.
    form_packet_sync: Vec<FormPacketEdit>,
}

impl XfaSession {
    /// Open a session over the given PDF bytes.
    ///
    /// Mirrors the flatten pipeline's Extract → Bind → (static scripts) →
    /// saved-form-state → Layout stages, then caches everything.
    ///
    /// Returns [`XfaError::PacketNotFound`] when the PDF has no XFA template
    /// packet (i.e. it is not an XFA form).
    pub fn open(pdf_bytes: &[u8]) -> Result<XfaSession> {
        let packets = extract_xfa_from_bytes(pdf_bytes.to_vec())?;
        let template_xml = packets
            .template()
            .ok_or_else(|| XfaError::PacketNotFound("template".to_string()))?
            .to_string();

        let data_dom = match packets.datasets() {
            Some(ds) => DataDom::from_xml(ds)
                .map_err(|e| XfaError::ParseFailed(format!("datasets parse: {e}")))?,
            None => DataDom::new(),
        };

        let image_files = match Document::load_mem(pdf_bytes) {
            Ok(doc) => extract_embedded_images(&doc),
            Err(_) => HashMap::new(),
        };

        let merger = FormMerger::new(&data_dom).with_image_files(image_files);
        let (mut tree, root_id) = merger
            .merge(&template_xml)
            .map_err(|e| XfaError::ParseFailed(format!("template merge: {e}")))?;

        // Static-analysis script pass only (BestEffortStatic): presence
        // toggles and FormCalc the engine can resolve without a JS runtime.
        // Phase 1 deliberately runs no change/click events.
        let _ = apply_dynamic_scripts(&mut tree, root_id)?;

        // Saved form state (Adobe-Reader-saved PDFs): presence overrides and
        // saved field values, exactly as the flatten pipeline applies them.
        if let Some(form_xml) = packets.get_packet("form") {
            let _ = apply_form_dom_presence(
                &mut tree,
                root_id,
                form_xml,
                XfaRenderingPolicy::SavedStateFaithful,
                true,
            );
            apply_form_dom_access(&mut tree, root_id, form_xml);
        }

        // Resolved font metrics make the layout geometry match the flatten
        // output (widths drive wrapping and therefore field positions).
        let resolved = resolve_template_fonts(&template_xml, pdf_bytes);
        inject_resolved_metrics(&mut tree, &resolved);

        let layout = LayoutEngine::new(&tree)
            .layout(root_id)
            .map_err(|e| XfaError::LayoutFailed(e.to_string()))?;

        let mut session = XfaSession {
            pdf_bytes: pdf_bytes.to_vec(),
            packets,
            data_dom,
            tree,
            layout,
            fields: Vec::new(),
            models: Vec::new(),
            by_name: HashMap::new(),
            dirty_data: Vec::new(),
            created_data: Vec::new(),
            form_packet_sync: Vec::new(),
        };
        session.enumerate_fields(root_id);
        Ok(session)
    }

    /// Number of pages in the cached layout result.
    pub fn page_count(&self) -> usize {
        self.layout.pages.len()
    }

    /// Page dimensions `(width, height)` in points for a 0-based page index.
    pub fn page_size(&self, page: usize) -> Option<(f64, f64)> {
        self.layout.pages.get(page).map(|p| (p.width, p.height))
    }

    /// The enumerated field models, in document order.
    pub fn fields(&self) -> &[XfaFieldModel] {
        &self.models
    }

    /// Look up one field model by fully-qualified name. Accepts both the
    /// display name (`a.b[2].c`) and the explicit SOM path (`a[0].b[2].c[0]`).
    pub fn field(&self, name: &str) -> Option<&XfaFieldModel> {
        self.by_name.get(name).map(|&i| &self.models[i])
    }

    /// Set a field's value.
    ///
    /// Updates the form-tree raw value, writes through to the bound
    /// datasets node (creating it on demand for default-bound fields), and
    /// records the change for [`save_to_bytes`](Self::save_to_bytes).
    ///
    /// Rejects writes to read-only fields ([`XfaError::FieldReadOnly`]) and
    /// to non-fillable kinds (buttons, signatures, …). No change/click
    /// scripts run and the layout is not recomputed (Phase 1).
    pub fn set_value(&mut self, name: &str, value: XfaWriteValue<'_>) -> Result<XfaSetOutcome> {
        let idx = *self
            .by_name
            .get(name)
            .ok_or_else(|| XfaError::FieldNotFound(name.to_string()))?;

        if self.fields[idx].read_only {
            return Err(XfaError::FieldReadOnly(name.to_string()));
        }
        let field_type = self.models[idx].field_type;
        if !field_type.is_fillable() {
            return Err(XfaError::InvalidFieldValue {
                name: name.to_string(),
                reason: format!("{field_type:?} fields are not fillable"),
            });
        }

        let raw = self.normalize_value(idx, value, name)?;

        // 1. Form-tree raw value.
        self.write_tree_value(idx, &raw);

        // 2. Datasets write-through.
        let persisted = self.write_data_value(idx, &raw)?;

        // 3. Saved-form-packet value sync (recorded; spliced at save time).
        if self.packets.get_packet("form").is_some() {
            let segments = self.fields[idx].segments.clone();
            // Drop earlier edits for this field (group prefix covers the
            // radio-member edits too).
            self.form_packet_sync
                .retain(|e| !e.segments.starts_with(&segments));
            if self.fields[idx].members.is_empty() {
                self.form_packet_sync.push(FormPacketEdit {
                    segments,
                    value: raw.clone(),
                    insert_if_missing: true,
                });
            } else {
                // Radio group: the saved form DOM stores the selection on
                // the member fields — the selected member carries the
                // on-value, deselected members are cleared. Also refresh a
                // group-level <value> when the producer wrote one.
                for (member, on) in self.fields[idx].members.clone() {
                    let mut member_segments = segments.clone();
                    member_segments.push((self.tree.get(member).name.clone(), 0));
                    let selected = on == raw;
                    self.form_packet_sync.push(FormPacketEdit {
                        segments: member_segments,
                        value: if selected { raw.clone() } else { String::new() },
                        insert_if_missing: selected,
                    });
                }
                self.form_packet_sync.push(FormPacketEdit {
                    segments,
                    value: raw.clone(),
                    insert_if_missing: false,
                });
            }
        }

        // 4. Refresh the public model.
        self.models[idx].value = raw.clone();

        Ok(XfaSetOutcome {
            raw_value: raw,
            persisted_to_datasets: persisted,
        })
    }

    /// Serialize the updated datasets (and form-packet value syncs) into a
    /// copy of the original PDF and return the new bytes.
    pub fn save_to_bytes(&self) -> Result<Vec<u8>> {
        let mut doc = Document::load_mem(&self.pdf_bytes)
            .map_err(|e| XfaError::LoadFailed(format!("reload for writeback: {e}")))?;
        self.write_into_document(&mut doc)?;
        let mut out = Vec::new();
        doc.save_to(&mut out)
            .map_err(|e| XfaError::WritebackFailed(format!("PDF save: {e}")))?;
        Ok(out)
    }

    /// Apply the datasets/form-packet writeback to an already-loaded
    /// `lopdf::Document` (the SDK facade path: the caller owns the document
    /// and serializes it itself).
    pub fn write_into_document(&self, doc: &mut Document) -> Result<XfaWritebackReport> {
        let mut report = XfaWritebackReport::default();

        // Build the new datasets packet: surgical splice of the original
        // packet text, falling back to data-section regeneration.
        let edits = self.collect_datasets_edits();
        let original_datasets = self.packets.datasets();
        let new_datasets = match original_datasets {
            Some(orig) => match datasets_writeback::splice_datasets(orig, &edits) {
                Some(spliced) => {
                    report.datasets_spliced = true;
                    spliced
                }
                None => datasets_writeback::regenerate_datasets(orig, &self.data_dom),
            },
            None => datasets_writeback::regenerate_datasets("", &self.data_dom),
        };

        // Saved form packet: keep recorded field values in sync.
        let new_form = self.packets.get_packet("form").and_then(|orig| {
            if self.form_packet_sync.is_empty() {
                return None;
            }
            let (xml, updated) =
                datasets_writeback::splice_form_packet(orig, &self.form_packet_sync);
            report.form_packet_values_updated = updated;
            if updated > 0 {
                Some(xml)
            } else {
                None
            }
        });

        datasets_writeback::write_packets_into_pdf(doc, &new_datasets, new_form.as_deref())?;
        Ok(report)
    }

    /// True when any value was changed since the session opened.
    pub fn is_dirty(&self) -> bool {
        !self.dirty_data.is_empty()
            || !self.created_data.is_empty()
            || !self.form_packet_sync.is_empty()
    }

    // ── internals ──────────────────────────────────────────────────────

    fn normalize_value(&self, idx: usize, value: XfaWriteValue<'_>, name: &str) -> Result<String> {
        let model = &self.models[idx];
        match (model.field_type, value) {
            (XfaFieldType::Checkbox, XfaWriteValue::Checkbox(on)) => Ok(if on {
                model.on_value.clone().unwrap_or_else(|| "1".to_string())
            } else {
                model.off_value.clone().unwrap_or_else(|| "0".to_string())
            }),
            (XfaFieldType::Checkbox, XfaWriteValue::Text(s)) => {
                let on_v = model.on_value.clone().unwrap_or_else(|| "1".to_string());
                let off_v = model.off_value.clone().unwrap_or_else(|| "0".to_string());
                if s == on_v || s.eq_ignore_ascii_case("true") || s == "1" {
                    Ok(on_v)
                } else if s == off_v || s.eq_ignore_ascii_case("false") || s == "0" || s.is_empty()
                {
                    Ok(off_v)
                } else {
                    Err(XfaError::InvalidFieldValue {
                        name: name.to_string(),
                        reason: format!("checkbox accepts {on_v:?} or {off_v:?}, got {s:?}"),
                    })
                }
            }
            (XfaFieldType::RadioGroup, XfaWriteValue::Radio(s))
            | (XfaFieldType::RadioGroup, XfaWriteValue::Text(s)) => {
                let known = self.fields[idx].members.iter().any(|(_, on)| on == s);
                if known {
                    Ok(s.to_string())
                } else {
                    let opts: Vec<&str> = self.fields[idx]
                        .members
                        .iter()
                        .map(|(_, on)| on.as_str())
                        .collect();
                    Err(XfaError::InvalidFieldValue {
                        name: name.to_string(),
                        reason: format!("radio accepts one of {opts:?}, got {s:?}"),
                    })
                }
            }
            (XfaFieldType::Dropdown, XfaWriteValue::Text(s)) => {
                if model.options.is_empty() {
                    return Ok(s.to_string());
                }
                if let Some(opt) = model.options.iter().find(|o| o.save == s) {
                    return Ok(opt.save.clone());
                }
                if let Some(opt) = model.options.iter().find(|o| o.display == s) {
                    return Ok(opt.save.clone());
                }
                // XFA choice lists may allow free entry; the template parser
                // does not currently surface `open="..."`, so accept unknown
                // values rather than over-rejecting.
                Ok(s.to_string())
            }
            (_, XfaWriteValue::Text(s)) => Ok(s.to_string()),
            (ft, v) => Err(XfaError::InvalidFieldValue {
                name: name.to_string(),
                reason: format!("{v:?} is not assignable to a {ft:?} field"),
            }),
        }
    }

    fn write_tree_value(&mut self, idx: usize, raw: &str) {
        let entry = &self.fields[idx];
        if entry.members.is_empty() {
            let id = entry.node;
            if let FormNodeType::Field { .. } = self.tree.get(id).node_type {
                self.tree.get_mut(id).node_type = FormNodeType::Field {
                    value: raw.to_string(),
                };
            }
        } else {
            // exclGroup: the member whose on-value matches gets the value,
            // every other member is cleared (mirrors the merger's
            // apply_exclusive_choice_value semantics).
            let members = entry.members.clone();
            for (member, on_value) in members {
                let new_value = if on_value == raw {
                    raw.to_string()
                } else {
                    String::new()
                };
                if let FormNodeType::Field { .. } = self.tree.get(member).node_type {
                    self.tree.get_mut(member).node_type = FormNodeType::Field { value: new_value };
                }
            }
        }
    }

    /// Write through to the datasets DOM. Returns `true` when the value is
    /// now held by a data node (existing or created).
    fn write_data_value(&mut self, idx: usize, raw: &str) -> Result<bool> {
        let entry_node = self.fields[idx].node;
        if self.tree.meta(entry_node).data_bind_none {
            // `bind match="none"` — by design not persisted in datasets.
            return Ok(false);
        }

        if let Some(raw_id) = self.tree.meta(entry_node).bound_data_node {
            let id = DataNodeId::from_raw(raw_id);
            match self.data_dom.get(id) {
                Some(DataNode::DataValue { .. }) => {
                    self.data_dom
                        .set_value(id, raw.to_string())
                        .map_err(|e| XfaError::WritebackFailed(e.to_string()))?;
                    if !self.dirty_data.contains(&id) {
                        self.dirty_data.push(id);
                    }
                    return Ok(true);
                }
                Some(DataNode::DataGroup { .. }) => {
                    // Container-bound (e.g. an exclGroup bound to a group):
                    // fall through to create/find a value child below.
                }
                None => return Ok(false),
            }
        }

        // No directly bound value node: create one on demand under the
        // nearest bound ancestor (XFA default binding by name).
        let created = self.create_data_node_for(idx, raw)?;
        Ok(created)
    }

    fn create_data_node_for(&mut self, idx: usize, raw: &str) -> Result<bool> {
        let field_node = self.fields[idx].node;
        let data_name = self.tree.get(field_node).name.clone();
        if data_name.is_empty() {
            return Ok(false);
        }

        // Find the nearest ancestor (or the node itself when group-bound)
        // with a bound data *group*.
        let mut parent_group: Option<DataNodeId> = None;
        // The node itself may be bound to a group (exclGroup case).
        if let Some(raw_id) = self.tree.meta(field_node).bound_data_node {
            let id = DataNodeId::from_raw(raw_id);
            if matches!(self.data_dom.get(id), Some(DataNode::DataGroup { .. })) {
                // The group IS the field's data home: a value child named
                // like the field is the XFA §4.4.5 group-value convention —
                // but Designer datasets normally store the exclGroup value
                // directly on the group-named *value* node. Prefer an
                // existing value child named like the field, else write a
                // value child with the field's name.
                parent_group = Some(id);
            }
        }
        if parent_group.is_none() {
            let mut cursor = self.parent_of(field_node);
            while let Some(p) = cursor {
                if let Some(raw_id) = self.tree.meta(p).bound_data_node {
                    let id = DataNodeId::from_raw(raw_id);
                    if matches!(self.data_dom.get(id), Some(DataNode::DataGroup { .. })) {
                        parent_group = Some(id);
                        break;
                    }
                }
                cursor = self.parent_of(p);
            }
        }
        let Some(group) = parent_group else {
            return Ok(false);
        };

        // Reuse an existing same-name child value node when present.
        let existing = self
            .data_dom
            .children_by_name(group, &data_name)
            .into_iter()
            .find(|&c| matches!(self.data_dom.get(c), Some(DataNode::DataValue { .. })));
        let id = match existing {
            Some(c) => {
                self.data_dom
                    .set_value(c, raw.to_string())
                    .map_err(|e| XfaError::WritebackFailed(e.to_string()))?;
                if !self.dirty_data.contains(&c) {
                    self.dirty_data.push(c);
                }
                self.tree.meta_mut(self.fields[idx].node).bound_data_node = Some(c.as_raw());
                return Ok(true);
            }
            None => self
                .data_dom
                .create_value(group, &data_name, raw)
                .map_err(|e| XfaError::WritebackFailed(e.to_string()))?,
        };
        self.created_data.push(id);
        self.tree.meta_mut(self.fields[idx].node).bound_data_node = Some(id.as_raw());
        Ok(true)
    }

    fn parent_of(&self, node: FormNodeId) -> Option<FormNodeId> {
        // FormTree stores no parent links; derive lazily from the arena.
        for (i, n) in self.tree.nodes.iter().enumerate() {
            if n.children.contains(&node) {
                return Some(FormNodeId(i));
            }
        }
        None
    }

    fn collect_datasets_edits(&self) -> Vec<DatasetsEdit> {
        let mut edits = Vec::new();
        for &id in &self.dirty_data {
            if let Some(path) = self.data_path_of(id) {
                if let Ok(value) = self.data_dom.value(id) {
                    edits.push(DatasetsEdit::SetValue {
                        path,
                        value: value.to_string(),
                    });
                }
            }
        }
        for &id in &self.created_data {
            let Some(node) = self.data_dom.get(id) else {
                continue;
            };
            let name = node.name().to_string();
            let Some(parent) = node.parent() else {
                continue;
            };
            let Some(parent_path) = self.data_path_of(parent) else {
                continue;
            };
            if let Ok(value) = self.data_dom.value(id) {
                edits.push(DatasetsEdit::CreateValue {
                    parent_path,
                    name,
                    value: value.to_string(),
                });
            }
        }
        edits
    }

    /// Path of `(name, index-among-same-name-element-siblings)` segments
    /// from the data root down to `id`. The root itself is segment 0; the
    /// upward walk stops there (the parse may keep wrapper parents like
    /// `<xfa:datasets>` linked above the effective root).
    fn data_path_of(&self, id: DataNodeId) -> Option<Vec<(String, usize)>> {
        let root = self.data_dom.root()?;
        let mut rev = Vec::new();
        let mut cursor = Some(id);
        while let Some(c) = cursor {
            let node = self.data_dom.get(c)?;
            let name = node.name().to_string();
            let at_root = c == root;
            let parent = if at_root { None } else { node.parent() };
            let index = match parent {
                Some(p) => {
                    // Index among same-name siblings, skipping nodes the
                    // splicer cannot see as XML elements (attribute-derived
                    // metadata values).
                    let mut k = 0usize;
                    let mut found = None;
                    for &sib in self.data_dom.children(p) {
                        let Some(sn) = self.data_dom.get(sib) else {
                            continue;
                        };
                        if sn.name() != name {
                            continue;
                        }
                        if is_metadata_value(sn) {
                            continue;
                        }
                        if sib == c {
                            found = Some(k);
                            break;
                        }
                        k += 1;
                    }
                    found?
                }
                None => 0,
            };
            rev.push((name, index));
            if at_root {
                cursor = None;
            } else {
                // A node that never reaches the effective root (e.g. it
                // lives under a wrapper sibling) cannot be spliced by a
                // root-relative path.
                cursor = Some(node.parent()?);
            }
        }
        rev.reverse();
        Some(rev)
    }

    // ── enumeration ─────────────────────────────────────────────────────

    fn enumerate_fields(&mut self, root: FormNodeId) {
        // Geometry first: form node → widget occurrences.
        let mut geometry: HashMap<usize, Vec<(usize, XfaRect)>> = HashMap::new();
        for (page_idx, page) in self.layout.pages.iter().enumerate() {
            for node in &page.nodes {
                collect_geometry(node, 0.0, 0.0, page_idx, &mut geometry);
            }
        }

        let mut entries = Vec::new();
        let mut segments: Vec<(String, usize)> = Vec::new();
        let mut sibling_counters: Vec<HashMap<String, usize>> = vec![HashMap::new()];
        // The merged tree's root is a synthetic container (named "root");
        // Adobe SOM names start at the template's root subform, so walk the
        // root's children instead of the root itself.
        let root_access = self.tree.meta(root).access.unwrap_or(Access::Open);
        for &child in &self.tree.get(root).children.clone() {
            self.walk_enumerate(
                child,
                root_access,
                &mut segments,
                &mut sibling_counters,
                &mut entries,
            );
        }

        let mut models = Vec::new();
        let mut by_name: HashMap<String, usize> = HashMap::new();
        for entry in entries {
            let model = self.build_model(&entry, &geometry);
            let idx = models.len();
            // First registration wins; later duplicates stay addressable via
            // their explicit SOM path.
            by_name.entry(model.name.clone()).or_insert(idx);
            by_name.entry(model.som_path.clone()).or_insert(idx);
            models.push(model);
            self.fields.push(entry);
        }
        self.models = models;
        self.by_name = by_name;
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_enumerate(
        &self,
        node_id: FormNodeId,
        inherited_access: Access,
        segments: &mut Vec<(String, usize)>,
        sibling_counters: &mut Vec<HashMap<String, usize>>,
        out: &mut Vec<FieldEntry>,
    ) {
        let node = self.tree.get(node_id);
        let meta = self.tree.meta(node_id);
        let access = meta.access.unwrap_or(inherited_access);

        let named = !node.name.is_empty();
        if named {
            let counter = sibling_counters
                .last_mut()
                .expect("sibling counter scope present");
            let idx = *counter
                .entry(node.name.clone())
                .and_modify(|c| *c += 1)
                .or_insert(0);
            segments.push((node.name.clone(), idx));
        }

        let is_radio_group = matches!(node.node_type, FormNodeType::ExclGroup)
            || meta.group_kind == GroupKind::ExclusiveChoice;
        let is_field = matches!(node.node_type, FormNodeType::Field { .. });

        if is_radio_group {
            let mut members = Vec::new();
            for &child in &node.children {
                if let FormNodeType::Field { .. } = self.tree.get(child).node_type {
                    let cmeta = self.tree.meta(child);
                    let on = cmeta
                        .item_value
                        .clone()
                        .or_else(|| cmeta.style.check_button_on_value.clone())
                        .unwrap_or_else(|| self.tree.get(child).name.clone());
                    members.push((child, on));
                }
            }
            if !members.is_empty() {
                out.push(FieldEntry {
                    node: node_id,
                    members,
                    read_only: access.denies_writes(),
                    segments: segments.clone(),
                });
            }
        } else if is_field {
            out.push(FieldEntry {
                node: node_id,
                members: Vec::new(),
                read_only: access.denies_writes(),
                segments: segments.clone(),
            });
        }

        // Recurse into containers (not into exclGroup members or field
        // internals — members are folded into the group entry above).
        if !is_field && !is_radio_group {
            sibling_counters.push(HashMap::new());
            for &child in &node.children {
                self.walk_enumerate(child, access, segments, sibling_counters, out);
            }
            sibling_counters.pop();
        }

        if named {
            segments.pop();
        }
    }

    fn build_model(
        &self,
        entry: &FieldEntry,
        geometry: &HashMap<usize, Vec<(usize, XfaRect)>>,
    ) -> XfaFieldModel {
        let node = self.tree.get(entry.node);
        let meta = self.tree.meta(entry.node);

        let name = segments_to_display_name(&entry.segments);
        let som_path = segments_to_som_path(&entry.segments);

        let field_type = if entry.members.is_empty() {
            XfaFieldType::from_kind(meta.field_kind)
        } else {
            XfaFieldType::RadioGroup
        };

        // Value: field raw value, or selected member's on-value for groups.
        let value = if entry.members.is_empty() {
            match &node.node_type {
                FormNodeType::Field { value } => value.clone(),
                _ => String::new(),
            }
        } else {
            entry
                .members
                .iter()
                .find_map(|(m, on)| match &self.tree.get(*m).node_type {
                    FormNodeType::Field { value } if !value.is_empty() && value == on => {
                        Some(on.clone())
                    }
                    _ => None,
                })
                .unwrap_or_default()
        };

        // Options.
        let options = if !entry.members.is_empty() {
            entry
                .members
                .iter()
                .map(|(_, on)| XfaFieldOption {
                    display: on.clone(),
                    save: on.clone(),
                })
                .collect()
        } else {
            let displays = &meta.display_items;
            let saves = &meta.save_items;
            displays
                .iter()
                .enumerate()
                .map(|(i, d)| XfaFieldOption {
                    display: d.clone(),
                    save: saves.get(i).cloned().unwrap_or_else(|| d.clone()),
                })
                .collect()
        };

        let on_value = meta
            .style
            .check_button_on_value
            .clone()
            .or_else(|| meta.item_value.clone());
        let off_value = meta.style.check_button_off_value.clone();

        // Geometry: the group node's own rect plus member rects.
        let mut widgets: Vec<XfaWidget> = Vec::new();
        if let Some(occ) = geometry.get(&entry.node.0) {
            for (page, rect) in occ {
                widgets.push(XfaWidget {
                    page: *page,
                    rect: *rect,
                    on_value: None,
                });
            }
        }
        for (member, on) in &entry.members {
            if let Some(occ) = geometry.get(&member.0) {
                for (page, rect) in occ {
                    widgets.push(XfaWidget {
                        page: *page,
                        rect: *rect,
                        on_value: Some(on.clone()),
                    });
                }
            }
        }
        widgets.sort_by(|a, b| {
            a.page.cmp(&b.page).then(
                a.rect
                    .y
                    .partial_cmp(&b.rect.y)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });
        let page = widgets.first().map(|w| w.page);
        let rect = widgets.first().map(|w| w.rect);

        let bound = meta
            .bound_data_node
            .map(|raw| self.data_dom.get(DataNodeId::from_raw(raw)).is_some())
            .unwrap_or(false);

        XfaFieldModel {
            name,
            som_path,
            field_type,
            value,
            read_only: entry.read_only,
            required: meta.required,
            multiline: meta.multiline,
            hidden: meta.presence != Presence::Visible,
            options,
            on_value,
            off_value,
            page,
            rect,
            widgets,
            bound_to_data: bound,
            bind_none: meta.data_bind_none,
        }
    }
}

fn is_metadata_value(node: &DataNode) -> bool {
    match node {
        DataNode::DataValue { contains, .. } => {
            *contains == xfa_dom_resolver::data_dom::DataContains::MetaData
        }
        DataNode::DataGroup { .. } => false,
    }
}

fn segments_to_display_name(segments: &[(String, usize)]) -> String {
    segments
        .iter()
        .map(|(n, i)| {
            if *i == 0 {
                n.clone()
            } else {
                format!("{n}[{i}]")
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn segments_to_som_path(segments: &[(String, usize)]) -> String {
    segments
        .iter()
        .map(|(n, i)| format!("{n}[{i}]"))
        .collect::<Vec<_>>()
        .join(".")
}

/// Accumulate absolute widget rectangles per form node.
///
/// Layout coordinates are parent-relative (the render bridge accumulates
/// them the same way); rectangles are reported in XFA page space (top-left
/// origin, points).
fn collect_geometry(
    node: &LayoutNode,
    parent_x: f64,
    parent_y: f64,
    page_idx: usize,
    out: &mut HashMap<usize, Vec<(usize, XfaRect)>>,
) {
    let abs_x = node.rect.x + parent_x;
    let abs_y = node.rect.y + parent_y;

    let record = matches!(node.content, LayoutContent::Field { .. })
        || matches!(
            node.content,
            LayoutContent::WrappedText {
                from_field: true,
                ..
            }
        );
    if record {
        out.entry(node.form_node.0).or_default().push((
            page_idx,
            XfaRect {
                x: abs_x,
                y: abs_y,
                width: node.rect.width,
                height: node.rect.height,
            },
        ));
    }

    for child in &node.children {
        collect_geometry(child, abs_x, abs_y, page_idx, out);
    }
}

/// Apply `access="…"` attributes from the saved form packet onto matching
/// form-tree nodes (session-only pass; the flatten pipeline ignores access).
///
/// Adobe Reader records interactive locks (e.g. fields disabled after
/// submission) as `access="readOnly"` in the form packet. Without this pass
/// the session would let callers write fields Acrobat refuses to edit.
fn apply_form_dom_access(tree: &mut FormTree, root_id: FormNodeId, form_xml: &str) {
    let Ok(doc) = roxmltree::Document::parse(form_xml) else {
        return;
    };

    fn matches_node(tree: &FormTree, fid: FormNodeId, tag: &str, name: &str) -> bool {
        let node = tree.get(fid);
        match (tag, &node.node_type) {
            ("subform", FormNodeType::Subform | FormNodeType::Area) => node.name == name,
            ("exclGroup", FormNodeType::ExclGroup) => node.name == name,
            // Some producers serialize exclGroups as <field> in the form DOM.
            ("field", FormNodeType::Field { .. } | FormNodeType::ExclGroup) => node.name == name,
            _ => false,
        }
    }

    fn walk(tree: &mut FormTree, fid: FormNodeId, xml: roxmltree::Node<'_, '_>) {
        if let Some(access) = xml.attribute("access") {
            tree.meta_mut(fid).access = Some(Access::parse(access));
        }

        let xml_children: Vec<roxmltree::Node<'_, '_>> = xml
            .children()
            .filter(|c| {
                c.is_element() && matches!(c.tag_name().name(), "subform" | "field" | "exclGroup")
            })
            .collect();
        if xml_children.is_empty() {
            return;
        }

        let tree_children = tree.get(fid).children.clone();
        let mut used = vec![false; tree_children.len()];
        for xc in xml_children {
            let tag = xc.tag_name().name();
            let name = xc.attribute("name").unwrap_or("");
            if name.is_empty() {
                continue;
            }
            for (i, &cid) in tree_children.iter().enumerate() {
                if used[i] || !matches_node(tree, cid, tag, name) {
                    continue;
                }
                used[i] = true;
                walk(tree, cid, xc);
                break;
            }
        }
    }

    // The form packet root is <form> (sometimes namespaced); its element
    // children mirror the template root subform's children. The template
    // FormTree root is the <template> wrapper whose first child is the root
    // subform — align by descending one level when the names don't match.
    let Some(form_root) = doc.root().children().find(|c| c.is_element()) else {
        return;
    };
    // Try matching the root subform: the form packet's first subform child
    // corresponds to the tree root's subform child of the same name.
    let root_children = tree.get(root_id).children.clone();
    let packet_subforms: Vec<roxmltree::Node<'_, '_>> = form_root
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "subform")
        .collect();
    for xc in packet_subforms {
        let name = xc.attribute("name").unwrap_or("");
        for &cid in &root_children {
            if matches_node(tree, cid, "subform", name) {
                walk(tree, cid, xc);
                break;
            }
        }
    }
}
