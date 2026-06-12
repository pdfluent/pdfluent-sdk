//! Single reliable AcroForm value writeback (D-chain).
//!
//! [`apply_field_value`] is the one SDK-level operation for setting a form
//! value on a `lopdf::Document`. It owns the complete chain the PDF spec and
//! the reference implementations (pdfium, mupdf, pdf.js) require:
//!
//! 1. **`/V`** — text values via the ASCII-literal-else-UTF-16BE+BOM policy
//!    (ISO 32000-1 §7.9.2.2); button values as byte-exact `/Name` objects
//!    (§12.7.4.2.3).
//! 2. **`/AS`** — kept consistent with `/V` on *every* widget of a button
//!    field: a widget's `/AS` becomes the on-state iff that name is a key of
//!    the widget's own `/AP /N` sub-dictionary, else `/Off` (the mupdf
//!    `set_check_grp` rule).
//! 3. **`/AP`** — text and choice widgets get a regenerated `/AP /N` Form
//!    XObject (`/Tx BMC … EMC`, WinAnsi-encoded show text, AFM-measured
//!    positioning, comb/multiline/quadding support) so the fill is visible in
//!    every viewer without `/NeedAppearances` processing.
//! 4. **`/NeedAppearances`** — set `true` only as a fallback when appearance
//!    generation is not trustworthy (value not WinAnsi-representable); the
//!    stale `/AP /N` is removed in that case so no viewer shows the old
//!    value. PDF 2.0 deprecates `NeedAppearances`, so the primary path never
//!    sets it.
//!
//! Field lookup accepts fully-qualified names (`parent.kid`) and recurses
//! through `/Kids`, unlike the historical top-level-only paths. Read-only
//! fields (`/Ff` bit 1, inherited) are rejected — note this is stricter than
//! pdfium/mupdf, which only enforce read-only in their UI layers; an SDK has
//! no UI layer, so set-time enforcement is the only place the contract can
//! live.

use crate::appearance::{parse_da, DefaultAppearance};
use crate::encoding::{encode_winansi, escape_string_bytes};
use crate::metrics::StandardFace;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use std::io::Write as _;

/// Maximum `/Kids` / `/Parent` traversal depth (mirrors `parse::MAX_FIELD_DEPTH`).
const MAX_DEPTH: usize = 100;

/// Inset between the widget border and the text, in points. The reference
/// implementations converge on ~2pt at the default 1pt border width.
const TEXT_INSET: f32 = 2.0;

/// Errors surfaced by [`apply_field_value`].
#[derive(Debug, thiserror::Error)]
pub enum WritebackError {
    /// No terminal field or button group matches the fully-qualified name.
    #[error("form field '{0}' not found")]
    FieldNotFound(String),
    /// The field (or an ancestor) carries the ReadOnly flag (`/Ff` bit 1).
    #[error("form field '{0}' is read-only")]
    ReadOnly(String),
    /// The value kind does not match the field's `/FT`.
    #[error("form field '{name}' is a {actual} field; cannot apply a {requested} value")]
    WrongType {
        /// Fully-qualified field name.
        name: String,
        /// The field's actual type as found in the PDF.
        actual: &'static str,
        /// The value kind the caller tried to apply.
        requested: &'static str,
    },
    /// A radio export value that no kid widget declares, or a choice value
    /// not present in a non-editable field's `/Opt` array.
    #[error("value '{value}' is not a valid option for field '{name}'")]
    InvalidOption {
        /// Fully-qualified field name.
        name: String,
        /// The rejected value.
        value: String,
    },
    /// Structural problem in the document (missing AcroForm, shape changes).
    #[error("malformed form structure: {0}")]
    Malformed(String),
}

/// The value to write, by field family.
#[derive(Debug, Clone, Copy)]
pub enum WriteValue<'a> {
    /// Text field (`/FT /Tx`) value.
    Text(&'a str),
    /// Checkbox on/off. The on-state name is resolved from the widget's own
    /// `/AP /N` keys (first non-`Off` key), defaulting to `Yes`.
    Checkbox(bool),
    /// Radio group selection by export (on-state) name. Matched byte-exactly
    /// against each kid widget's `/AP /N` keys, with WinAnsi re-encoding of
    /// the lookup string as a fallback so Latin-1 state names (`coöperatie…`)
    /// match from a Unicode caller.
    Radio(&'a str),
    /// Choice field (`/FT /Ch`) selection.
    Choice(&'a str),
}

impl WriteValue<'_> {
    fn kind(&self) -> &'static str {
        match self {
            WriteValue::Text(_) => "text",
            WriteValue::Checkbox(_) => "checkbox",
            WriteValue::Radio(_) => "radio",
            WriteValue::Choice(_) => "choice",
        }
    }
}

/// What [`apply_field_value`] did, for callers that report or log.
#[derive(Debug, Default, Clone)]
pub struct WriteOutcome {
    /// `/AP` streams (re)generated on widgets of this field.
    pub appearances_generated: usize,
    /// Widgets whose `/AS` was updated.
    pub appearance_states_set: usize,
    /// `true` when `/NeedAppearances` was set as the encoding fallback.
    pub need_appearances_fallback: bool,
}

// ---------------------------------------------------------------------------
// Field location
// ---------------------------------------------------------------------------

/// A located field: the object id of its dictionary plus its FQN.
#[derive(Debug, Clone)]
struct Located {
    id: ObjectId,
    fqn: String,
    /// Direct kids that are themselves located (for widget enumeration).
    kids: Vec<ObjectId>,
}

fn acroform_id(doc: &Document) -> Result<ObjectId, WritebackError> {
    let catalog = doc
        .catalog()
        .map_err(|_| WritebackError::Malformed("document has no catalog".into()))?;
    match catalog.get(b"AcroForm") {
        Ok(Object::Reference(id)) => Ok(*id),
        Ok(Object::Dictionary(_)) => Err(WritebackError::Malformed(
            "inline /AcroForm dictionary; promote it first via ensure_indirect_acroform".into(),
        )),
        _ => Err(WritebackError::Malformed(
            "document has no /AcroForm dictionary".into(),
        )),
    }
}

/// Promote an inline `/AcroForm` dictionary to an indirect object.
///
/// Real-world corpora are full of documents whose `/AcroForm` lives inline in
/// the catalog (LiveCycle static shells in particular — 92% of our hybrid
/// gate corpus). Mutation paths address the AcroForm by object id, so this
/// one-time normalization runs at every writeback entry point. Acrobat
/// itself always writes indirect AcroForms, so the promoted shape is the
/// canonical one. No-op when `/AcroForm` is already indirect or absent.
fn ensure_indirect_acroform(doc: &mut Document) {
    let inline = match doc.catalog() {
        Ok(catalog) => match catalog.get(b"AcroForm") {
            Ok(Object::Dictionary(d)) => Some(d.clone()),
            _ => None,
        },
        Err(_) => None,
    };
    if let Some(dict) = inline {
        let id = doc.add_object(Object::Dictionary(dict));
        if let Ok(catalog) = doc.catalog_mut() {
            catalog.set("AcroForm", Object::Reference(id));
        }
    }
}

/// Walk `/AcroForm /Fields` (+ `/Kids`, depth- and cycle-guarded) and collect
/// every indirect field dictionary with its fully-qualified name.
fn collect_fields(doc: &Document) -> Result<Vec<Located>, WritebackError> {
    let af_id = acroform_id(doc)?;
    let af = doc
        .get_object(af_id)
        .and_then(|o| o.as_dict())
        .map_err(|_| WritebackError::Malformed("/AcroForm is not a dictionary".into()))?;
    let fields = match af.get(b"Fields") {
        Ok(Object::Array(arr)) => arr.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(arr)) => arr.clone(),
            _ => return Err(WritebackError::Malformed("/Fields is not an array".into())),
        },
        _ => return Err(WritebackError::Malformed("/AcroForm has no /Fields".into())),
    };

    let mut out = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    for entry in &fields {
        if let Object::Reference(id) = entry {
            walk_field(doc, *id, String::new(), 0, &mut visited, &mut out);
        }
    }
    Ok(out)
}

fn walk_field(
    doc: &Document,
    id: ObjectId,
    prefix: String,
    depth: usize,
    visited: &mut std::collections::BTreeSet<ObjectId>,
    out: &mut Vec<Located>,
) {
    if depth >= MAX_DEPTH || !visited.insert(id) {
        return;
    }
    let Ok(dict) = doc.get_object(id).and_then(|o| o.as_dict()) else {
        return;
    };
    let partial = dict
        .get(b"T")
        .ok()
        .and_then(|o| lopdf::decode_text_string(o).ok())
        .unwrap_or_default();
    let fqn = match (prefix.is_empty(), partial.is_empty()) {
        (true, _) => partial.clone(),
        (false, true) => prefix.clone(),
        (false, false) => format!("{prefix}.{partial}"),
    };

    let mut kid_ids = Vec::new();
    if let Ok(Object::Array(kids)) = dict.get(b"Kids") {
        for kid in kids {
            if let Object::Reference(kid_id) = kid {
                kid_ids.push(*kid_id);
                // Only kids that carry /T are nested FIELDS and get their own
                // Located entry; unnamed kids are widget annotations of THIS
                // field (§12.7.3.1 — a field node is identified by /T) and
                // must not shadow the field under the same FQN.
                let is_nested_field = doc
                    .get_object(*kid_id)
                    .and_then(|o| o.as_dict())
                    .map(|d| d.get(b"T").is_ok())
                    .unwrap_or(false);
                if is_nested_field {
                    walk_field(doc, *kid_id, fqn.clone(), depth + 1, visited, out);
                }
            }
        }
    }
    out.push(Located {
        id,
        fqn,
        kids: kid_ids,
    });
}

/// Resolve an inheritable key by walking the `/Parent` chain.
fn inherited<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<Object> {
    let mut current: Option<&Dictionary> = Some(dict);
    for _ in 0..MAX_DEPTH {
        let d = current?;
        if let Ok(v) = d.get(key) {
            return Some(v.clone());
        }
        current = match d.get(b"Parent") {
            Ok(Object::Reference(pid)) => doc.get_object(*pid).and_then(|o| o.as_dict()).ok(),
            _ => None,
        };
    }
    None
}

fn effective_flags(doc: &Document, dict: &Dictionary) -> i64 {
    match inherited(doc, dict, b"Ff") {
        Some(Object::Integer(i)) => i,
        _ => 0,
    }
}

fn effective_field_type(doc: &Document, dict: &Dictionary) -> Option<Vec<u8>> {
    match inherited(doc, dict, b"FT") {
        Some(Object::Name(n)) => Some(n),
        _ => None,
    }
}

/// The widgets of a field: kid widget annotations, or the field dict itself
/// when field and widget are merged (it carries a `/Rect`).
fn widget_ids(doc: &Document, located: &Located) -> Vec<ObjectId> {
    let mut out = Vec::new();
    for &kid in &located.kids {
        if let Ok(d) = doc.get_object(kid).and_then(|o| o.as_dict()) {
            // A kid with /T is a nested field, not a widget of this field.
            // /Rect is NOT required: spec-degenerate widgets without one
            // still carry /AP//AS state; geometry-dependent steps guard via
            // widget_rect() themselves.
            if d.get(b"T").is_err() {
                out.push(kid);
            }
        }
    }
    if out.is_empty() {
        out.push(located.id);
    }
    out
}

/// First non-`Off` key of a widget's `/AP /N` sub-dictionary, raw bytes.
fn on_state_of_widget(doc: &Document, widget: ObjectId) -> Option<Vec<u8>> {
    let dict = doc.get_object(widget).and_then(|o| o.as_dict()).ok()?;
    let n = appearance_normal_dict(doc, dict)?;
    n.iter()
        .map(|(k, _)| k.clone())
        .find(|k| k.as_slice() != b"Off")
}

/// Resolve `/AP /N` to a sub-state dictionary (following references).
fn appearance_normal_dict<'a>(doc: &'a Document, dict: &'a Dictionary) -> Option<&'a Dictionary> {
    let ap = match dict.get(b"AP").ok()? {
        Object::Reference(id) => doc.get_object(*id).and_then(|o| o.as_dict()).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };
    match ap.get(b"N").ok()? {
        Object::Reference(id) => doc.get_object(*id).and_then(|o| o.as_dict()).ok(),
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

/// Whether a widget's `/AP /N` declares `state` as a key, byte-exact.
fn widget_has_state(doc: &Document, widget: ObjectId, state: &[u8]) -> bool {
    let Ok(dict) = doc.get_object(widget).and_then(|o| o.as_dict()) else {
        return false;
    };
    match appearance_normal_dict(doc, dict) {
        Some(n) => n.iter().any(|(k, _)| k.as_slice() == state),
        None => false,
    }
}

/// Set `/AS` on a widget dictionary.
fn set_widget_as(doc: &mut Document, widget: ObjectId, state: &[u8]) -> bool {
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(widget) {
        d.set("AS", Object::Name(state.to_vec()));
        true
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Set a form field value, updating `/V`, `/AS`, `/AP` and (only as an
/// encoding fallback) `/NeedAppearances`. See the module docs for the chain.
///
/// `name` is the fully-qualified field name (`parent.kid` notation);
/// hierarchical forms are fully addressable.
pub fn apply_field_value(
    doc: &mut Document,
    name: &str,
    value: WriteValue<'_>,
) -> Result<WriteOutcome, WritebackError> {
    ensure_indirect_acroform(doc);
    let fields = collect_fields(doc)?;
    let located = fields
        .iter()
        .find(|l| l.fqn == name)
        .cloned()
        .ok_or_else(|| WritebackError::FieldNotFound(name.to_string()))?;

    let dict = doc
        .get_object(located.id)
        .and_then(|o| o.as_dict())
        .map_err(|_| WritebackError::Malformed("field object is not a dictionary".into()))?;

    // ReadOnly (Ff bit 1) — inherited, set-time enforced (see module docs).
    if effective_flags(doc, dict) & 0x1 != 0 {
        return Err(WritebackError::ReadOnly(name.to_string()));
    }

    let ft = effective_field_type(doc, dict).unwrap_or_else(|| b"Tx".to_vec());
    let flags = effective_flags(doc, dict);

    match (ft.as_slice(), value) {
        (b"Tx", WriteValue::Text(text)) => apply_text(doc, &located, text),
        (b"Btn", WriteValue::Checkbox(on)) if flags & 0x18000 == 0 => {
            apply_checkbox(doc, &located, on)
        }
        (b"Btn", WriteValue::Radio(export)) if flags & 0x8000 != 0 => {
            apply_radio(doc, &located, name, export)
        }
        (b"Ch", WriteValue::Choice(text)) => apply_choice(doc, &located, name, text, flags),
        (b"Sig", _) => Err(WritebackError::WrongType {
            name: name.to_string(),
            actual: "signature",
            requested: value.kind(),
        }),
        (actual_ft, v) => Err(WritebackError::WrongType {
            name: name.to_string(),
            actual: match actual_ft {
                b"Tx" => "text",
                b"Btn" if flags & 0x10000 != 0 => "push-button",
                b"Btn" if flags & 0x8000 != 0 => "radio",
                b"Btn" => "checkbox",
                b"Ch" => "choice",
                _ => "unknown",
            },
            requested: v.kind(),
        }),
    }
}

/// Regenerate `/AP /N` for every filled text/choice field in the document.
///
/// For documents filled by tools that only wrote `/V` (+`/NeedAppearances`),
/// this materialises trustworthy appearances so the document renders
/// correctly in viewers — including this SDK's own renderer — that do not
/// honour `/NeedAppearances`. Fields whose value is not
/// WinAnsi-representable are left for the viewer (counted in the outcome's
/// `need_appearances_fallback`).
pub fn regenerate_appearances(doc: &mut Document) -> Result<WriteOutcome, WritebackError> {
    ensure_indirect_acroform(doc);
    let fields = collect_fields(doc)?;
    let mut outcome = WriteOutcome::default();
    for located in &fields {
        let Ok(dict) = doc.get_object(located.id).and_then(|o| o.as_dict()) else {
            continue;
        };
        // Terminal fields only: a node with field-kids is a group.
        let has_field_kids = located.kids.iter().any(|&k| {
            doc.get_object(k)
                .and_then(|o| o.as_dict())
                .map(|d| d.get(b"T").is_ok())
                .unwrap_or(false)
        });
        if has_field_kids {
            continue;
        }
        let ft = effective_field_type(doc, dict).unwrap_or_default();
        if ft != b"Tx" && ft != b"Ch" {
            continue;
        }
        let value = match inherited(doc, dict, b"V") {
            Some(v @ Object::String(..)) => lopdf::decode_text_string(&v).unwrap_or_default(),
            _ => continue,
        };
        if value.is_empty() {
            continue;
        }
        match generate_text_widget_appearances(doc, located, &value) {
            Ok(n) => outcome.appearances_generated += n,
            Err(_) => outcome.need_appearances_fallback = true,
        }
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Per-type application
// ---------------------------------------------------------------------------

fn apply_text(
    doc: &mut Document,
    located: &Located,
    text: &str,
) -> Result<WriteOutcome, WritebackError> {
    // Enforce /MaxLen (inherited) by truncation, matching viewer behavior.
    let max_len = {
        let dict = field_dict(doc, located)?;
        match inherited(doc, dict, b"MaxLen") {
            Some(Object::Integer(n)) if n >= 0 => Some(n as usize),
            _ => None,
        }
    };
    let text: String = match max_len {
        Some(n) => text.chars().take(n).collect(),
        None => text.to_string(),
    };

    set_field_v(doc, located.id, lopdf::text_string(&text))?;

    let mut outcome = WriteOutcome::default();
    match generate_text_widget_appearances(doc, located, &text) {
        Ok(n) => outcome.appearances_generated = n,
        Err(NotWinAnsi) => {
            // Encoding fallback: never leave a stale appearance showing the
            // old value; defer drawing to the viewer.
            remove_widget_appearances(doc, located);
            set_need_appearances(doc, true)?;
            outcome.need_appearances_fallback = true;
        }
    }
    Ok(outcome)
}

fn apply_checkbox(
    doc: &mut Document,
    located: &Located,
    on: bool,
) -> Result<WriteOutcome, WritebackError> {
    let widgets = widget_ids(doc, located);
    let on_state = widgets
        .iter()
        .find_map(|&w| on_state_of_widget(doc, w))
        .unwrap_or_else(|| b"Yes".to_vec());
    let state: &[u8] = if on { &on_state } else { b"Off" };

    set_field_v(doc, located.id, Object::Name(state.to_vec()))?;

    let mut outcome = WriteOutcome::default();
    for &w in &widgets {
        // With a substate dict, /AS may only name an existing key; without
        // one (malformed checkbox with a bare stream /N), keep /AS consistent
        // with /V anyway — viewers ignore /AS for stream-/N appearances, and
        // appearance-regenerating processors read it (pdfium CheckControl
        // behaves the same).
        let has_substates = {
            let dict = doc.get_object(w).and_then(|o| o.as_dict()).ok();
            dict.and_then(|d| appearance_normal_dict(doc, d)).is_some()
        };
        let on_here = on && (!has_substates || widget_has_state(doc, w, state));
        let widget_state: &[u8] = if on_here { state } else { b"Off" };
        if set_widget_as(doc, w, widget_state) {
            outcome.appearance_states_set += 1;
        }
    }
    Ok(outcome)
}

fn apply_radio(
    doc: &mut Document,
    located: &Located,
    name: &str,
    export: &str,
) -> Result<WriteOutcome, WritebackError> {
    let widgets = widget_ids(doc, located);

    // Resolve the byte-exact on-state: try the caller's string as raw UTF-8
    // bytes first, then WinAnsi-encoded (Latin-1 state names like
    // `coöperatie…` are stored as single 0xF6 bytes in real forms).
    let candidates: Vec<Vec<u8>> = {
        let mut c = vec![export.as_bytes().to_vec()];
        if let Some(w) = encode_winansi(export) {
            if w != export.as_bytes() {
                c.push(w);
            }
        }
        c
    };
    // A group where no widget declares a substate dictionary is degenerate
    // (no /AP at all — fixture-grade or broken forms). There is nothing to
    // validate against or to /AS-select; accept the export verbatim so /V
    // round-trips, matching the reference implementations' programmatic
    // paths.
    let any_substates = widgets.iter().any(|&w| {
        doc.get_object(w)
            .and_then(|o| o.as_dict())
            .ok()
            .and_then(|d| appearance_normal_dict(doc, d))
            .is_some()
    });
    let chosen = if any_substates {
        candidates
            .into_iter()
            .find(|cand| widgets.iter().any(|&w| widget_has_state(doc, w, cand)))
            .ok_or_else(|| WritebackError::InvalidOption {
                name: name.to_string(),
                value: export.to_string(),
            })?
    } else {
        export.as_bytes().to_vec()
    };

    // /V (a Name) on the group head; /AS per kid widget (§12.7.4.2.3).
    set_field_v(doc, located.id, Object::Name(chosen.clone()))?;

    let mut outcome = WriteOutcome::default();
    for &w in &widgets {
        let state: &[u8] = if widget_has_state(doc, w, &chosen) {
            &chosen
        } else {
            b"Off"
        };
        if set_widget_as(doc, w, state) {
            outcome.appearance_states_set += 1;
        }
    }
    Ok(outcome)
}

fn apply_choice(
    doc: &mut Document,
    located: &Located,
    name: &str,
    text: &str,
    flags: i64,
) -> Result<WriteOutcome, WritebackError> {
    // Validate against /Opt unless the combo is editable (Edit flag bit 19).
    let editable = flags & 0x40000 != 0;
    let options = {
        let dict = field_dict(doc, located)?;
        choice_options(doc, dict)
    };
    if !editable && !options.is_empty() {
        let known = options
            .iter()
            .any(|(export, display)| export == text || display == text);
        if !known {
            return Err(WritebackError::InvalidOption {
                name: name.to_string(),
                value: text.to_string(),
            });
        }
    }

    set_field_v(doc, located.id, lopdf::text_string(text))?;
    // Stale selected-index cache is only valid against the old /V.
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(located.id) {
        d.remove(b"I");
    }

    let mut outcome = WriteOutcome::default();
    // Display text: prefer the display string of a matching export value.
    let display = options
        .iter()
        .find(|(export, _)| export == text)
        .map(|(_, d)| d.clone())
        .unwrap_or_else(|| text.to_string());
    match generate_text_widget_appearances(doc, located, &display) {
        Ok(n) => outcome.appearances_generated = n,
        Err(NotWinAnsi) => {
            remove_widget_appearances(doc, located);
            set_need_appearances(doc, true)?;
            outcome.need_appearances_fallback = true;
        }
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Shared mutation helpers
// ---------------------------------------------------------------------------

fn field_dict<'a>(doc: &'a Document, located: &Located) -> Result<&'a Dictionary, WritebackError> {
    doc.get_object(located.id)
        .and_then(|o| o.as_dict())
        .map_err(|_| WritebackError::Malformed("field object vanished".into()))
}

fn set_field_v(doc: &mut Document, id: ObjectId, value: Object) -> Result<(), WritebackError> {
    match doc.get_object_mut(id) {
        Ok(Object::Dictionary(d)) => {
            d.set("V", value);
            Ok(())
        }
        _ => Err(WritebackError::Malformed(
            "field object is not mutable".into(),
        )),
    }
}

fn set_need_appearances(doc: &mut Document, value: bool) -> Result<(), WritebackError> {
    let af_id = acroform_id(doc)?;
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(af_id) {
        d.set("NeedAppearances", Object::Boolean(value));
    }
    Ok(())
}

fn remove_widget_appearances(doc: &mut Document, located: &Located) {
    for w in widget_ids(doc, located) {
        if let Ok(Object::Dictionary(d)) = doc.get_object_mut(w) {
            d.remove(b"AP");
        }
    }
}

/// `(export, display)` pairs from `/Opt` (inherited).
fn choice_options(doc: &Document, dict: &Dictionary) -> Vec<(String, String)> {
    let Some(Object::Array(arr)) = inherited(doc, dict, b"Opt") else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|o| {
            let resolved = match o {
                Object::Reference(id) => doc.get_object(*id).ok()?,
                other => other,
            };
            match resolved {
                Object::String(..) => {
                    let s = lopdf::decode_text_string(resolved).ok()?;
                    Some((s.clone(), s))
                }
                Object::Array(pair) if pair.len() >= 2 => {
                    let export = lopdf::decode_text_string(&pair[0]).ok()?;
                    let display = lopdf::decode_text_string(&pair[1]).ok()?;
                    Some((export, display))
                }
                _ => None,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Appearance generation (text + choice widgets)
// ---------------------------------------------------------------------------

/// Marker error: the value is not WinAnsi-representable, generation skipped.
struct NotWinAnsi;

/// Generate and install `/AP /N` on every widget of a text/choice field.
/// Returns the number of widgets updated.
fn generate_text_widget_appearances(
    doc: &mut Document,
    located: &Located,
    text: &str,
) -> Result<usize, NotWinAnsi> {
    // Multiline values are encoded per line; the encoder rejects what the
    // Standard-14 WinAnsi fonts cannot show.
    let encoded_lines: Vec<Vec<u8>> = {
        let mut lines = Vec::new();
        for line in text.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            lines.push(encode_winansi(line).ok_or(NotWinAnsi)?);
        }
        lines
    };

    let Ok(dict) = doc.get_object(located.id).and_then(|o| o.as_dict()) else {
        return Ok(0);
    };
    let flags = effective_flags(doc, dict);
    let da_string = match inherited(doc, dict, b"DA") {
        Some(v @ Object::String(..)) => lopdf::decode_text_string(&v).unwrap_or_default(),
        _ => acroform_da(doc).unwrap_or_default(),
    };
    let da = parse_da(if da_string.is_empty() {
        "/Helv 0 Tf 0 g"
    } else {
        &da_string
    });
    let quadding = match inherited(doc, dict, b"Q") {
        Some(Object::Integer(1)) => 1u8,
        Some(Object::Integer(2)) => 2u8,
        _ => 0u8,
    };
    let max_len = match inherited(doc, dict, b"MaxLen") {
        Some(Object::Integer(n)) if n > 0 => Some(n as u32),
        _ => None,
    };
    let multiline = flags & 0x1000 != 0;
    let comb = flags & 0x100_0000 != 0 && max_len.is_some() && !multiline;
    let password = flags & 0x2000 != 0;

    let widgets = widget_ids(doc, located);
    let mut updated = 0;
    for &w in &widgets {
        let rect = match widget_rect(doc, w) {
            Some(r) => r,
            None => continue,
        };
        let content = build_text_appearance_content(
            &encoded_lines,
            rect,
            &da,
            quadding,
            comb,
            max_len,
            multiline,
            password,
        );
        let font_alias = da.font_name.clone().unwrap_or_else(|| "Helv".to_string());
        install_widget_appearance(doc, w, rect, content, &font_alias);
        updated += 1;
    }
    Ok(updated)
}

fn acroform_da(doc: &Document) -> Option<String> {
    let af_id = acroform_id(doc).ok()?;
    let af = doc.get_object(af_id).and_then(|o| o.as_dict()).ok()?;
    let v = af.get(b"DA").ok()?;
    lopdf::decode_text_string(v).ok()
}

fn widget_rect(doc: &Document, widget: ObjectId) -> Option<[f32; 4]> {
    let d = doc.get_object(widget).and_then(|o| o.as_dict()).ok()?;
    let Ok(Object::Array(arr)) = d.get(b"Rect") else {
        return None;
    };
    if arr.len() != 4 {
        return None;
    }
    let mut r = [0f32; 4];
    for (i, o) in arr.iter().enumerate() {
        r[i] = match o {
            Object::Integer(n) => *n as f32,
            Object::Real(f) => *f,
            _ => return None,
        };
    }
    // Normalize: PDF rects may have swapped corners.
    Some([
        r[0].min(r[2]),
        r[1].min(r[3]),
        r[0].max(r[2]),
        r[1].max(r[3]),
    ])
}

/// Build the `/Tx BMC … EMC` content stream for a text-ish widget.
#[allow(clippy::too_many_arguments)]
fn build_text_appearance_content(
    lines: &[Vec<u8>],
    rect: [f32; 4],
    da: &DefaultAppearance,
    quadding: u8,
    comb: bool,
    max_len: Option<u32>,
    multiline: bool,
    password: bool,
) -> Vec<u8> {
    let w = rect[2] - rect[0];
    let h = rect[3] - rect[1];
    let face = StandardFace::from_font_name(da.font_name.as_deref().unwrap_or("Helv"));
    let inset = TEXT_INSET;
    let inner_w = (w - 2.0 * inset).max(1.0);
    let inner_h = (h - 2.0 * inset).max(1.0);

    // Password fields render bullets/asterisks; mirror Acrobat's asterisks.
    let display_lines: Vec<Vec<u8>> = if password {
        lines
            .iter()
            .map(|l| vec![b'*'; l.iter().filter(|&&b| b != b'\r').count()])
            .collect()
    } else {
        lines.to_vec()
    };

    // Font size: explicit from DA, else auto-size (0 Tf).
    let font_size = if da.font_size > 0.0 {
        da.font_size
    } else if multiline {
        // mupdf uses a fixed 12pt for auto-sized multiline.
        12.0_f32.min(inner_h)
    } else {
        // pdf.js: min(h/LINE_FACTOR, w/textWidth), LINE_FACTOR = 1.35.
        let longest_units: u32 = display_lines
            .iter()
            .map(|l| l.iter().map(|&b| face.glyph_width(b) as u32).sum())
            .max()
            .unwrap_or(0);
        let by_height = inner_h / 1.35;
        let by_width = if longest_units > 0 {
            inner_w * 1000.0 / longest_units as f32
        } else {
            by_height
        };
        by_height.min(by_width).clamp(2.0, 144.0)
    };

    let mut buf = Vec::with_capacity(256);
    let _ = writeln!(buf, "/Tx BMC");
    buf.extend_from_slice(b"q\n");
    // Clip to the inner box so overlong values cannot paint outside the
    // widget (ISO 32000-1 §12.7.3.3: text shall be clipped to the BBox).
    let _ = writeln!(buf, "{inset} {inset} {inner_w} {inner_h} re W n");
    buf.extend_from_slice(b"BT\n");
    write_da_color(&mut buf, da);
    let font_alias = da.font_name.as_deref().unwrap_or("Helv");
    let _ = writeln!(buf, "/{font_alias} {font_size} Tf");

    if comb {
        // Comb cells span the FULL widget width (Acrobat behavior), one
        // glyph centered per cell.
        let cells = max_len.unwrap_or(1).max(1);
        let cell_w = w / cells as f32;
        let baseline = (h - font_size) / 2.0 + font_size * 0.22;
        let line = display_lines.first().cloned().unwrap_or_default();
        let mut prev_x = 0.0_f32;
        let mut prev_y = 0.0_f32;
        for (i, &byte) in line.iter().take(cells as usize).enumerate() {
            let glyph_w = face.glyph_width(byte) as f32 * font_size / 1000.0;
            let x = cell_w * i as f32 + (cell_w - glyph_w) / 2.0;
            let _ = writeln!(buf, "{} {} Td", x - prev_x, baseline - prev_y);
            prev_x = x;
            prev_y = baseline;
            let esc = escape_string_bytes(&[byte]);
            buf.extend_from_slice(b"(");
            buf.extend_from_slice(&esc);
            buf.extend_from_slice(b") Tj\n");
        }
    } else if multiline {
        let leading = font_size * 1.2;
        let _ = writeln!(buf, "{leading} TL");
        // Word-wrap each logical line to the inner width.
        let wrapped = wrap_lines(&display_lines, face, font_size, inner_w);
        let first_baseline = h - inset - font_size;
        let _ = writeln!(buf, "{inset} {first_baseline} Td");
        for (i, line) in wrapped.iter().enumerate() {
            if i > 0 {
                buf.extend_from_slice(b"T*\n");
            }
            let esc = escape_string_bytes(line);
            buf.extend_from_slice(b"(");
            buf.extend_from_slice(&esc);
            buf.extend_from_slice(b") Tj\n");
        }
    } else {
        let line = display_lines.first().cloned().unwrap_or_default();
        let text_w = face.text_width(&line, font_size);
        let x = match quadding {
            1 => inset + (inner_w - text_w) / 2.0,
            2 => inset + inner_w - text_w,
            _ => inset,
        }
        .max(inset);
        let baseline = (h - font_size) / 2.0 + font_size * 0.22;
        let _ = writeln!(buf, "{x} {baseline} Td");
        let esc = escape_string_bytes(&line);
        buf.extend_from_slice(b"(");
        buf.extend_from_slice(&esc);
        buf.extend_from_slice(b") Tj\n");
    }

    buf.extend_from_slice(b"ET\nQ\nEMC\n");
    buf
}

/// Greedy word-wrap of encoded lines to `width` points at `font_size`.
fn wrap_lines(lines: &[Vec<u8>], face: StandardFace, font_size: f32, width: f32) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for line in lines {
        if line.is_empty() {
            out.push(Vec::new());
            continue;
        }
        let mut current: Vec<u8> = Vec::new();
        for word in line.split(|&b| b == b' ') {
            let candidate_len = if current.is_empty() {
                face.text_width(word, font_size)
            } else {
                face.text_width(&current, font_size)
                    + face.glyph_width(b' ') as f32 * font_size / 1000.0
                    + face.text_width(word, font_size)
            };
            if !current.is_empty() && candidate_len > width {
                out.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(b' ');
            }
            current.extend_from_slice(word);
        }
        out.push(current);
    }
    out
}

fn write_da_color(buf: &mut Vec<u8>, da: &DefaultAppearance) {
    let op = match (da.color.len(), da.color_op.as_deref()) {
        (1, Some("g")) => "g",
        (3, Some("rg")) => "rg",
        (4, Some("k")) => "k",
        _ => {
            buf.extend_from_slice(b"0 g\n");
            return;
        }
    };
    for c in &da.color {
        let _ = write!(buf, "{c} ");
    }
    let _ = writeln!(buf, "{op}");
}

/// Wrap content in a Form XObject with proper `/Resources /Font` and install
/// it as the widget's `/AP /N`.
fn install_widget_appearance(
    doc: &mut Document,
    widget: ObjectId,
    rect: [f32; 4],
    content: Vec<u8>,
    font_alias: &str,
) {
    let w = rect[2] - rect[0];
    let h = rect[3] - rect[1];

    // Resources: ALWAYS bind the DA alias to our own self-contained
    // Standard-14 font dict with explicit /Encoding /WinAnsiEncoding,
    // shadowing the form-level /DR entry. The show-text bytes in the content
    // are WinAnsi-encoded, and real-world /DR fonts routinely carry other
    // encodings (PDFDocEncoding-style /Differences are common in
    // government forms) — referencing them would silently remap our bytes
    // (€ 0x80 → bullet). pdfium's GenerateFallbackFontDict makes the same
    // choice. The DA's face is preserved through the BaseFont mapping; DA
    // fonts outside the Standard-14 render as their nearest standard face.
    let face = StandardFace::from_font_name(font_alias);
    let font_obj = Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(face.base_font_name().as_bytes().to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    });
    let mut fonts = Dictionary::new();
    fonts.set(font_alias.as_bytes().to_vec(), font_obj);
    let resources = dictionary! {
        "Font" => Object::Dictionary(fonts),
    };
    let xobj = Stream::new(
        dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Form".to_vec()),
            "BBox" => Object::Array(vec![
                Object::Real(0.0), Object::Real(0.0), Object::Real(w), Object::Real(h),
            ]),
            "Resources" => Object::Dictionary(resources),
        },
        content,
    );
    let ap_ref = doc.add_object(Object::Stream(xobj));
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(widget) {
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(ap_ref));
        d.set("AP", Object::Dictionary(ap));
    }
}
