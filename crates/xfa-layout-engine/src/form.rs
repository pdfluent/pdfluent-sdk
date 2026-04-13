//! Form node types — the input to the layout engine.
//!
//! These represent the merged Form DOM nodes that the layout engine processes.
//! In a full implementation, these would come from xfa-dom-resolver's merge step.

use std::collections::HashMap;

use crate::text::FontMetrics;
use crate::types::{BoxModel, LayoutStrategy, TextAlign, VerticalAlign};

/// A unique identifier for a form node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormNodeId(pub usize);

/// The form tree: a node-based representation of the merged template+data.
#[derive(Debug)]
pub struct FormTree {
    pub nodes: Vec<FormNode>,
    /// Per-node metadata (parallel to `nodes`).
    pub metadata: Vec<FormNodeMeta>,
    /// Lookup table: XFA `id` attribute -> `FormNodeId`.
    pub node_ids: HashMap<String, FormNodeId>,
}

impl FormTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            metadata: Vec::new(),
            node_ids: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: FormNode) -> FormNodeId {
        let id = FormNodeId(self.nodes.len());
        self.nodes.push(node);
        self.metadata.push(FormNodeMeta::default());
        id
    }

    /// Add a node together with its metadata. If the meta has an `xfa_id`,
    /// it is registered in the `node_ids` lookup table.
    pub fn add_node_with_meta(&mut self, node: FormNode, meta: FormNodeMeta) -> FormNodeId {
        let id = FormNodeId(self.nodes.len());
        if let Some(ref xfa_id) = meta.xfa_id {
            self.node_ids.insert(xfa_id.clone(), id);
        }
        self.nodes.push(node);
        self.metadata.push(meta);
        id
    }

    pub fn get(&self, id: FormNodeId) -> &FormNode {
        &self.nodes[id.0]
    }

    pub fn get_mut(&mut self, id: FormNodeId) -> &mut FormNode {
        &mut self.nodes[id.0]
    }

    /// Access the metadata for a node.
    pub fn meta(&self, id: FormNodeId) -> &FormNodeMeta {
        &self.metadata[id.0]
    }

    /// Mutably access the metadata for a node.
    pub fn meta_mut(&mut self, id: FormNodeId) -> &mut FormNodeMeta {
        &mut self.metadata[id.0]
    }

    /// Look up a node by its XFA `id` attribute.
    pub fn find_by_xfa_id(&self, id: &str) -> Option<FormNodeId> {
        self.node_ids.get(id).copied()
    }
}

impl Default for FormTree {
    fn default() -> Self {
        Self::new()
    }
}

/// A single node in the Form DOM.
#[derive(Debug, Clone)]
pub struct FormNode {
    pub name: String,
    pub node_type: FormNodeType,
    pub box_model: BoxModel,
    pub layout: LayoutStrategy,
    pub children: Vec<FormNodeId>,
    /// Occurrence rules for repeating subforms.
    pub occur: Occur,
    /// Font metrics for text measurement (Draw/Field nodes).
    pub font: FontMetrics,
    /// FormCalc calculate script (XFA S14.3.2): runs to compute the field's value.
    pub calculate: Option<String>,
    /// FormCalc validate script: runs to validate the field's value, returns bool.
    pub validate: Option<String>,
    /// Column widths for table-layout subforms (XFA columnWidths attribute).
    /// Positive values are fixed widths in points; -1.0 means auto-size.
    /// Empty for non-table nodes.
    pub column_widths: Vec<f64>,
    /// Column span for cells inside a table row (XFA colSpan attribute).
    /// 1 = single column (default), N = span N columns, -1 = span remaining.
    pub col_span: i32,
}

/// The scripting language used by an XFA `<script>` element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScriptLanguage {
    /// FormCalc is the XFA default when `contentType` is omitted.
    #[default]
    FormCalc,
    /// JavaScript event handlers and calculations.
    JavaScript,
    /// Any other declared script language (for example VBScript).
    Other,
}

/// Script metadata collected from `<event>` / `<calculate>` elements.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventScript {
    pub script: String,
    pub language: ScriptLanguage,
    pub activity: Option<String>,
    pub event_ref: Option<String>,
    pub run_at: Option<String>,
}

impl EventScript {
    pub fn new(
        script: String,
        language: ScriptLanguage,
        activity: Option<String>,
        event_ref: Option<String>,
        run_at: Option<String>,
    ) -> Self {
        Self {
            script,
            language,
            activity,
            event_ref,
            run_at,
        }
    }

    pub fn formcalc(script: impl Into<String>, activity: Option<&str>) -> Self {
        Self::new(
            script.into(),
            ScriptLanguage::FormCalc,
            activity.map(str::to_string),
            None,
            None,
        )
    }

    pub fn javascript(script: impl Into<String>, activity: Option<&str>) -> Self {
        Self::new(
            script.into(),
            ScriptLanguage::JavaScript,
            activity.map(str::to_string),
            None,
            None,
        )
    }
}

/// Content for draw nodes (static graphic elements).
///
/// XFA Spec 3.3 §2.1 (p24) — Draw element: fixed content (boilerplate).
/// Includes text, lines, rectangles, arcs. Images are handled separately
/// via `FormNodeType::Image`.
///
/// TODO(§2.3): `circle` draw content not implemented (spec allows via arc with
///   startAngle=0, sweepAngle=360).
#[derive(Debug, Clone)]
pub enum DrawContent {
    Text(String),
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
    Rectangle {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        radius: f64,
    },
    Arc {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        start_angle: f64,
        sweep_angle: f64,
    },
}

/// The type of form node.
#[derive(Debug, Clone)]
pub enum FormNodeType {
    /// Root subform.
    Root,
    /// A page set containing page areas.
    PageSet,
    /// A page area (page template) with content areas.
    PageArea { content_areas: Vec<ContentArea> },
    /// A generic subform container.
    Subform,
    /// A form field (text field, checkbox, etc.).
    Field { value: String },
    /// A static draw element (text, image, line, etc.).
    Draw(DrawContent),
    /// A static image draw element.
    Image { data: Vec<u8>, mime_type: String },
}

/// Occurrence rules for repeating subforms (XFA S3.3 occur element).
///
/// Controls how many instances of a subform are created. The layout engine
/// expands templates based on the `initial` count, bounded by `min` and `max`.
#[derive(Debug, Clone)]
pub struct Occur {
    /// Minimum number of occurrences (default 1).
    pub min: u32,
    /// Maximum number of occurrences (-1 = unlimited). Default 1.
    /// Using `Option<u32>` where `None` means unlimited.
    pub max: Option<u32>,
    /// Initial number of occurrences (default = min).
    pub initial: u32,
}

impl Default for Occur {
    fn default() -> Self {
        Self {
            min: 1,
            max: Some(1),
            initial: 1,
        }
    }
}

impl Occur {
    /// Occur rule that means "exactly once" (the default).
    pub fn once() -> Self {
        Self::default()
    }

    /// Occur rule for a repeating subform.
    pub fn repeating(min: u32, max: Option<u32>, initial: u32) -> Self {
        let initial = initial.max(min);
        let initial = match max {
            Some(m) => initial.min(m),
            None => initial,
        };
        Self { min, max, initial }
    }

    /// How many instances should be created.
    pub fn count(&self) -> u32 {
        self.initial
    }

    /// Whether the subform can repeat (max > 1 or unlimited).
    pub fn is_repeating(&self) -> bool {
        match self.max {
            Some(m) => m > 1,
            None => true,
        }
    }
}

/// A content area within a page area.
#[derive(Debug, Clone)]
pub struct ContentArea {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Leader (header) node placed at the top of each page's content area.
    pub leader: Option<FormNodeId>,
    /// Trailer (footer) node placed at the bottom of each page's content area.
    pub trailer: Option<FormNodeId>,
}

impl Default for ContentArea {
    fn default() -> Self {
        Self {
            name: String::new(),
            x: 0.0,
            y: 0.0,
            width: 612.0,  // US Letter width in points
            height: 792.0, // US Letter height in points
            leader: None,
            trailer: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Metadata, style, and kind types
// ---------------------------------------------------------------------------

/// XFA `presence` attribute values (XFA 3.3 §2.6 p67-68).
///
/// Controls visibility and layout space allocation:
/// - `Visible` -- all phases: binding, automation, layout, rendering, interaction.
/// - `Hidden` -- binding + automation only; no layout space, no rendering.
/// - `Invisible` -- binding + automation + layout; takes up space but not visible.
/// - `Inactive` -- binding only; completely absent from form.
///
/// Note: spec defines `hidden` as "effectively absent" (no space) and `invisible`
/// as "takes space but not visible". Our `is_layout_hidden` implementation reflects this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Presence {
    #[default]
    Visible,
    Hidden,
    Invisible,
    Inactive,
}

impl Presence {
    /// True when the element should not be rendered.
    pub fn is_not_visible(self) -> bool {
        !matches!(self, Presence::Visible)
    }

    /// True when the element should not occupy layout space.
    ///
    /// XFA Spec 3.3 §2.6 (p68):
    /// - `hidden`:   no layout space, no rendering (effectively absent)
    /// - `invisible`: no layout space in Adobe (spec says "takes space",
    ///   but empirical testing shows Adobe skips it)
    /// - `inactive`:  completely absent (no binding, no space)
    ///
    /// `Hidden` was previously excluded from this predicate based on an
    /// incorrect assumption that Adobe reserves space for hidden elements.
    /// GATE #27 testing proved this wrong: hidden subforms with `<break>`
    /// elements caused 2-23x overpagination in forms with many
    /// `presence="hidden"` subforms (fixes #806).
    pub fn is_layout_hidden(self) -> bool {
        matches!(
            self,
            Presence::Hidden | Presence::Invisible | Presence::Inactive
        )
    }
}

/// Extended metadata for a form node.
///
/// Carries XFA attributes that the layout engine and dynamic scripting
/// system need but that are not part of the core `FormNode` shape.
#[derive(Debug, Clone, Default)]
pub struct FormNodeMeta {
    /// Optional XFA `id` attribute.
    pub xfa_id: Option<String>,
    /// XFA presence attribute (visible/hidden/invisible/inactive).
    pub presence: Presence,
    /// Whether a page break should be inserted before this node.
    pub page_break_before: bool,
    /// Whether a page break should be inserted after this node.
    pub page_break_after: bool,
    /// Target page area name/id for the break (e.g. "MP3", "Page4_ID").
    pub break_target: Option<String>,
    /// Whether this node targets a specific content area via
    /// `breakBefore targetType="contentArea"`.  Such nodes should be
    /// excluded from the primary content flow (they go into small
    /// decorative areas like "flatten" or "eSign").
    pub content_area_break: bool,
    /// Overflow leader reference name.
    pub overflow_leader: Option<String>,
    /// Overflow trailer reference name.
    pub overflow_trailer: Option<String>,
    /// Keep with next content area.
    pub keep_next_content_area: bool,
    /// Keep with previous content area.
    pub keep_previous_content_area: bool,
    /// Keep intact within content area.
    pub keep_intact_content_area: bool,
    /// Layout-ready script (XFA S14.3).
    pub layout_ready_script: Option<String>,
    /// Event scripts collected from `<event>` and `<calculate>` children.
    pub event_scripts: Vec<EventScript>,
    /// Explicit XFA data binding ref from `<bind ref="...">`.
    pub data_bind_ref: Option<String>,
    /// Whether the node explicitly opts out of data binding via `<bind match="none">`.
    pub data_bind_none: bool,
    /// Visual style (font, colors, borders).
    pub style: FormNodeStyle,
    /// The kind of field (text, checkbox, radio, etc.).
    pub field_kind: FieldKind,
    /// The kind of group (none or exclusive choice).
    pub group_kind: GroupKind,
    /// Item value for fields inside an exclGroup.
    pub item_value: Option<String>,
    /// Check box / radio button size in points.
    pub check_size: Option<f64>,
    /// Display items for choice list fields (XFA 3.3 §7.7).
    pub display_items: Vec<String>,
    /// Save items for choice list fields (XFA 3.3 §7.7).
    pub save_items: Vec<String>,
    /// XFA anchorType for positioned layout (XFA 3.3 §2.6, App A p1510).
    pub anchor_type: AnchorType,
}

/// XFA `anchorType` attribute (XFA 3.3 §2.6, Appendix A p1510).
///
/// Determines which point of an element is placed at the (x,y) coordinate
/// in positioned layout.  Default is `TopLeft`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnchorType {
    #[default]
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    MiddleCenter,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// The kind of group container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupKind {
    #[default]
    None,
    ExclusiveChoice,
}

/// The kind of form field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldKind {
    #[default]
    Text,
    Checkbox,
    Radio,
    Button,
    Dropdown,
    Signature,
    DateTimePicker,
    NumericEdit,
    PasswordEdit,
    ImageEdit,
    Barcode,
}

/// A span of rich text with per-span style overrides.
///
/// XFA Spec 3.3 §4.2.7 (p155) — `<exData contentType="text/html">` stores
/// XHTML content with inline CSS. Each span carries its own formatting
/// (font, color, weight, etc.) that overrides the node-level defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct RichTextSpan {
    pub text: String,
    pub font_size: Option<f64>,
    pub font_family: Option<String>,
    pub font_weight: Option<String>,
    pub font_style: Option<String>,
    pub text_color: Option<(u8, u8, u8)>,
    pub underline: bool,
    pub line_through: bool,
}

/// Visual style properties for a form node.
#[derive(Debug, Clone, PartialEq)]
pub struct FormNodeStyle {
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub font_weight: Option<String>,
    pub font_style: Option<String>,
    pub text_color: Option<(u8, u8, u8)>,
    pub bg_color: Option<(u8, u8, u8)>,
    pub border_color: Option<(u8, u8, u8)>,
    /// Per-edge border colors (top, right, bottom, left) in RGB 0-255.
    /// When set, overrides `border_color` for individual edges.
    pub border_colors: Option<[(u8, u8, u8); 4]>,
    pub border_width_pt: Option<f64>,
    /// Per-edge border widths (top, right, bottom, left) in points.
    /// When set, overrides `border_width_pt` for individual edges.
    pub border_widths: Option<[f64; 4]>,
    /// Paragraph space above in points (XFA `<para spaceAbove>`).
    pub space_above_pt: Option<f64>,
    /// Paragraph space below in points (XFA `<para spaceBelow>`).
    pub space_below_pt: Option<f64>,
    /// Paragraph left margin in points (XFA `<para marginLeft>`).
    pub margin_left_pt: Option<f64>,
    /// Paragraph right margin in points (XFA `<para marginRight>`).
    pub margin_right_pt: Option<f64>,
    /// XFA Spec 3.3 §17 "para" (p803) — lineHeight: baseline-to-baseline
    /// distance in points. When `None`, use font metrics.
    pub line_height_pt: Option<f64>,
    /// XFA Spec 3.3 §17 "para" (p803) — textIndent: indentation of the first
    /// line of each paragraph in points.
    pub text_indent_pt: Option<f64>,
    /// Margin top inset in points (XFA `<margin topInset>`).
    pub inset_top_pt: Option<f64>,
    /// Margin bottom inset in points (XFA `<margin bottomInset>`).
    pub inset_bottom_pt: Option<f64>,
    /// Margin left inset in points (XFA `<margin leftInset>`).
    pub inset_left_pt: Option<f64>,
    /// Margin right inset in points (XFA `<margin rightInset>`).
    pub inset_right_pt: Option<f64>,
    /// Vertical text alignment (XFA `<para vAlign>`).
    pub v_align: Option<VerticalAlign>,
    /// Horizontal alignment for layout positioning (XFA `<para hAlign>`, §8.3).
    pub h_align: Option<TextAlign>,
    /// Border corner radius in points (XFA `<border><corner radius>`).
    pub border_radius_pt: Option<f64>,
    /// Border edge stroke style (XFA `<border><edge stroke>`).
    pub border_style: Option<String>,
    /// Per-edge visibility: [top, right, bottom, left]. All true when absent.
    pub border_edges: [bool; 4],
    /// XFA Spec 3.3 §17 (p716) — genericFamily fallback hint.
    /// Values: serif, sansSerif, monospaced, decorative, fantasy, cursive.
    pub generic_family: Option<String>,
    /// Font horizontal scale factor (XFA `<font fontHorizontalScale>`).
    /// 1.0 = 100% (default), 0.96 = 96%, etc.
    pub font_horizontal_scale: Option<f64>,
    /// Letter spacing in points (XFA `<font letterSpacing>`).
    /// 0.0 = normal (default). Negative values tighten, positive widen.
    pub letter_spacing_pt: Option<f64>,
    /// Caption text (XFA `<caption><value><text>`).
    pub caption_text: Option<String>,
    /// Caption placement (left/right/top/bottom/inline).
    pub caption_placement: Option<String>,
    /// Caption reserve width/height in points.
    pub caption_reserve: Option<f64>,
    /// CheckButton mark style (XFA `<checkButton mark="...">`).
    /// Values: "check", "circle", "cross", "diamond", "square", "star".
    pub check_button_mark: Option<String>,
    /// CheckButton on-value (first `<items>` entry, XFA 3.3 §17.8).
    pub check_button_on_value: Option<String>,
    /// CheckButton off-value (second `<items>` entry). When omitted, the
    /// spec default is the null string.
    pub check_button_off_value: Option<String>,
    /// CheckButton neutral-value (third `<items>` entry, checkbox only).
    pub check_button_neutral_value: Option<String>,
    /// Rich text spans parsed from `<exData contentType="text/html">` XHTML.
    pub rich_text_spans: Option<Vec<RichTextSpan>>,
    /// Font underline (XFA `<font underline="1">`).
    pub underline: bool,
    /// Font line-through / strikethrough (XFA `<font lineThrough="1">`).
    pub line_through: bool,
    /// XFA format picture clause (e.g. `num{z,zzz.99}`).
    pub format_pattern: Option<String>,
}

impl Default for FormNodeStyle {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: None,
            font_weight: None,
            font_style: None,
            text_color: None,
            bg_color: None,
            border_color: None,
            border_colors: None,
            border_width_pt: None,
            border_widths: None,
            space_above_pt: None,
            space_below_pt: None,
            margin_left_pt: None,
            margin_right_pt: None,
            line_height_pt: None,
            text_indent_pt: None,
            inset_top_pt: None,
            inset_bottom_pt: None,
            inset_left_pt: None,
            inset_right_pt: None,
            v_align: None,
            h_align: None,
            border_radius_pt: None,
            border_style: None,
            border_edges: [true, true, true, true],
            generic_family: None,
            font_horizontal_scale: None,
            letter_spacing_pt: None,
            caption_text: None,
            caption_placement: None,
            caption_reserve: None,
            check_button_mark: None,
            check_button_on_value: None,
            check_button_off_value: None,
            check_button_neutral_value: None,
            rich_text_spans: None,
            underline: false,
            line_through: false,
            format_pattern: None,
        }
    }
}
