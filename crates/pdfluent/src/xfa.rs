//! XFA form field model — public DTOs for the Phase-1 XFA fill API.
//!
//! XFA forms (XML Forms Architecture, used by LiveCycle/AEM government and
//! enterprise forms) store their field tree and values in XML packets, not
//! in AcroForm dictionaries. [`PdfDocument::xfa_form_model`] enumerates the
//! currently layouted fields of such a document and
//! [`PdfDocument::set_xfa_field_value`] fills them, persisting values into
//! the `datasets` packet so Adobe Acrobat/Reader reopens the form with them.
//!
//! [`PdfDocument::xfa_form_model`]: crate::PdfDocument::xfa_form_model
//! [`PdfDocument::set_xfa_field_value`]: crate::PdfDocument::set_xfa_field_value

use crate::error::Error;

/// The type of an XFA form field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum XfaFieldType {
    /// Single-line or multiline text edit.
    Text,
    /// Standalone checkbox.
    Checkbox,
    /// Exclusive radio group (one logical field per `exclGroup`).
    RadioGroup,
    /// Push button (not fillable).
    Button,
    /// Dropdown / choice list.
    Dropdown,
    /// Signature field (not fillable in this phase).
    Signature,
    /// Date/time picker (accepts text values).
    DateTime,
    /// Numeric edit (accepts text values).
    Numeric,
    /// Password edit.
    Password,
    /// Image edit (not fillable in this phase).
    Image,
    /// Barcode (not fillable).
    Barcode,
}

/// Rectangle in page space: points, top-left origin (XFA convention — `y`
/// grows downward from the top of the page).
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
    /// 0-based page index in the XFA layout.
    pub page: usize,
    /// Widget rectangle.
    pub rect: XfaRect,
    /// For radio groups: the on-value this member widget asserts.
    pub on_value: Option<String>,
}

/// A selectable option of a dropdown / choice field.
#[derive(Debug, Clone)]
pub struct XfaFieldOption {
    /// User-visible text.
    pub display: String,
    /// Persisted save value.
    pub save: String,
}

/// One logical XFA form field.
#[derive(Debug, Clone)]
pub struct XfaField {
    /// Fully-qualified dotted name (Adobe display-SOM style, `[n]` omitted
    /// when 0), e.g. `form1.applicant.name`. Accepted by
    /// [`set_xfa_field_value`](crate::PdfDocument::set_xfa_field_value).
    pub name: String,
    /// Fully-qualified SOM path with explicit indices,
    /// e.g. `form1[0].applicant[0].name[0]`. Also accepted as a field name.
    pub som_path: String,
    /// Field type.
    pub field_type: XfaFieldType,
    /// Current value (radio groups: the selected member's on-value).
    pub value: String,
    /// Effective read-only state (the field or an ancestor container sets
    /// `access="readOnly" | "protected" | "nonInteractive"`, in the template
    /// or in the saved form state).
    pub read_only: bool,
    /// Mandatory field (`<validate nullTest="error">`).
    pub required: bool,
    /// Multiline text edit.
    pub multiline: bool,
    /// Hidden / invisible / inactive presence.
    pub hidden: bool,
    /// Dropdown options; for radio groups one option per member.
    pub options: Vec<XfaFieldOption>,
    /// Checkbox on-value, when known.
    pub on_value: Option<String>,
    /// Checkbox off-value, when known.
    pub off_value: Option<String>,
    /// First page the field appears on (0-based in the XFA layout); `None`
    /// when the field is not part of the current layout (e.g. hidden, or on
    /// a master page).
    pub page: Option<usize>,
    /// Rectangle of the first widget occurrence.
    pub rect: Option<XfaRect>,
    /// All widget occurrences (radio groups: one per member).
    pub widgets: Vec<XfaWidget>,
    /// Whether a datasets node currently backs this field. Unbound fields
    /// (except `bind="none"`) get their data node created on first write.
    pub bound_to_data: bool,
    /// `<bind match="none">` — values of such fields are not persisted in
    /// the datasets packet (Adobe keeps them in the saved form state only).
    pub bind_none: bool,
}

/// The XFA form model of a document: layout page count plus the enumerated
/// fields, in document order.
#[derive(Debug, Clone)]
pub struct XfaFormModel {
    /// Number of pages in the XFA layout. May differ from
    /// [`page_count`](crate::PdfDocument::page_count) of the PDF shell —
    /// dynamic XFA PDFs often ship a 1-page "please update your reader"
    /// shell while the real form lays out to N pages.
    pub page_count: usize,
    /// The fields.
    pub fields: Vec<XfaField>,
}

/// A value to write into an XFA field.
#[derive(Debug, Clone, Copy)]
pub enum XfaFieldValue<'a> {
    /// Text-like fields (text, multiline, numeric, date/time, password,
    /// dropdown — display or save values both resolve).
    Text(&'a str),
    /// Checkbox state.
    Checkbox(bool),
    /// Radio group selection: the on-value of the member to select.
    Radio(&'a str),
}

/// Outcome of a successful [`set_xfa_field_value`] call.
///
/// [`set_xfa_field_value`]: crate::PdfDocument::set_xfa_field_value
#[derive(Debug, Clone)]
pub struct XfaSetOutcome {
    /// The normalized value written (e.g. a dropdown display value resolved
    /// to its save value, a checkbox `true` to its on-value).
    pub raw_value: String,
    /// The value was written through to the datasets packet and persists
    /// across save/reopen. `false` only for `bind="none"` fields (or
    /// unnamed/unanchorable nodes) — those values update the in-memory
    /// model and the saved form state but not the datasets.
    pub persisted_to_datasets: bool,
}

pub(crate) fn field_type_from_engine(t: pdf_engine::xfa::XfaFieldType) -> XfaFieldType {
    use pdf_engine::xfa::XfaFieldType as E;
    match t {
        E::Text => XfaFieldType::Text,
        E::Checkbox => XfaFieldType::Checkbox,
        E::RadioGroup => XfaFieldType::RadioGroup,
        E::Button => XfaFieldType::Button,
        E::Dropdown => XfaFieldType::Dropdown,
        E::Signature => XfaFieldType::Signature,
        E::DateTime => XfaFieldType::DateTime,
        E::Numeric => XfaFieldType::Numeric,
        E::Password => XfaFieldType::Password,
        E::Image => XfaFieldType::Image,
        E::Barcode => XfaFieldType::Barcode,
    }
}

pub(crate) fn field_from_engine(f: &pdf_engine::xfa::XfaFieldModel) -> XfaField {
    let rect = |r: &pdf_engine::xfa::XfaRect| XfaRect {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    };
    XfaField {
        name: f.name.clone(),
        som_path: f.som_path.clone(),
        field_type: field_type_from_engine(f.field_type),
        value: f.value.clone(),
        read_only: f.read_only,
        required: f.required,
        multiline: f.multiline,
        hidden: f.hidden,
        options: f
            .options
            .iter()
            .map(|o| XfaFieldOption {
                display: o.display.clone(),
                save: o.save.clone(),
            })
            .collect(),
        on_value: f.on_value.clone(),
        off_value: f.off_value.clone(),
        page: f.page,
        rect: f.rect.as_ref().map(rect),
        widgets: f
            .widgets
            .iter()
            .map(|w| XfaWidget {
                page: w.page,
                rect: rect(&w.rect),
                on_value: w.on_value.clone(),
            })
            .collect(),
        bound_to_data: f.bound_to_data,
        bind_none: f.bind_none,
    }
}

/// Map session errors onto the public error surface: caller-addressable
/// conditions become [`Error::Unsupported`] with a descriptive message,
/// engine faults become [`Error::Internal`].
pub(crate) fn map_xfa_err(e: pdf_engine::xfa::XfaError) -> Error {
    use pdf_engine::xfa::XfaError as X;
    match e {
        X::PacketNotFound(_) => {
            Error::Unsupported("document has no XFA form (template packet missing)".to_string())
        }
        X::FieldNotFound(name) => Error::Unsupported(format!("XFA field not found: {name}")),
        X::FieldReadOnly(name) => Error::Unsupported(format!("XFA field is read-only: {name}")),
        X::InvalidFieldValue { name, reason } => {
            Error::Unsupported(format!("invalid value for XFA field {name}: {reason}"))
        }
        other => crate::error::internal_error(other.to_string()),
    }
}
