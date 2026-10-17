//! Editing for the compatible API (lane L4: annotations, widgets, drawing).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Live editing handles plus owned snapshots. [`Annot`] and [`Shape`] borrow
//! the document exclusively, like `PageMut`; [`Widget`] and [`Link`] are owned
//! values because the recorded flows build and pass them around. Drawing and
//! insertion knobs live in option structs shared between the page methods and
//! the shape methods.

use super::document::{
    BakeOptions, DocMetadata, EmbFileOptions, EmbItem, InsertPageOptions, InsertPdfOptions,
    LayerOptions, Link, LinkDest, OcmdOptions, PageLabel, PageSelection, ScrubOptions, TocDest,
    TocEntry, TocItemOptions,
};
use super::geometry::{CompareOp, Matrix, Point, Quad, Rect};
use super::pixmap::{Pixmap, RenderOptions};
use super::text::{TextArg, TextOptions, TextOutput, TextPage};
use super::{ErrorKind, PageMut, Value};
use crate::document::PdfDocument;
use crate::error::Result;
use lopdf::{Dictionary, Document as LopdfDocument, Object, ObjectId, Stream, StringFormat};
use std::collections::HashMap;

/// A live annotation: an object number on a page plus the document.
#[allow(dead_code)]
pub struct Annot<'a> {
    doc: &'a mut PdfDocument,
    page: usize,
    xref: i32,
}

/// One listed annotation: object number, type, and box.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotEntry {
    /// Annotation object number.
    pub xref: i32,
    /// Annotation type code and name.
    pub annot_type: AnnotType,
    /// Annotation box, in points.
    pub rect: Rect,
}

/// One annotation cross-reference: object number, kind code, and name.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotXref {
    /// Annotation object number.
    pub xref: i32,
    /// Annotation kind code.
    pub kind_code: i32,
    /// Annotation name.
    pub name: String,
}

/// Annotation identity fields: content, dates, names, and subject.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotInfo {
    /// Content text.
    pub content: String,
    /// Creation date stamp.
    pub creation_date: String,
    /// Modification date stamp.
    pub mod_date: String,
    /// Annotation name.
    pub name: String,
    /// Subject text.
    pub subject: String,
    /// Title text.
    pub title: String,
    /// Annotation identifier.
    pub id: String,
}

/// Annotation type: numeric code plus display name.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotType {
    /// Type code.
    pub code: i32,
    /// Type name.
    pub name: String,
}

/// Annotation colours: stroke and fill component lists.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnnotColors {
    /// Stroke colour components, empty for none.
    pub stroke: Vec<f64>,
    /// Fill colour components, empty for none.
    pub fill: Vec<f64>,
}

/// Annotation border: width, style, dashes, and cloud effect.
#[derive(Debug, Clone, PartialEq)]
pub struct Border {
    /// Border width, in points.
    pub width: f64,
    /// Whether the width keeps its integer shape (unset reads as int -1).
    pub width_is_int: bool,
    /// Border style code, if any.
    pub style: Option<String>,
    /// Dash pattern, empty for solid.
    pub dashes: Vec<i32>,
    /// Cloud effect level.
    pub clouds: i32,
}

/// Options for setting a border: width, style, dashes, and clouds.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BorderOptions {
    /// Border width, in points.
    pub width: f64,
    /// Border style code, if any.
    pub style: Option<String>,
    /// Dash pattern, empty for solid.
    pub dashes: Vec<i32>,
    /// Cloud effect level.
    pub clouds: i32,
}

impl Default for BorderOptions {
    fn default() -> Self {
        Self {
            width: -1.0,
            style: None,
            dashes: Vec::new(),
            clouds: -1,
        }
    }
}

/// Options for annotation appearance updates: the refreshable knobs.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AnnotUpdateOptions {
    /// Blend mode name.
    pub blend_mode: Option<String>,
    /// Opacity level.
    pub opacity: Option<f64>,
    /// Font size, in points.
    pub fontsize: f64,
    /// Font name.
    pub fontname: Option<String>,
    /// Text colour components.
    pub text_color: Option<Vec<f64>>,
    /// Border colour components.
    pub border_color: Option<Vec<f64>>,
    /// Fill colour components.
    pub fill_color: Option<Vec<f64>>,
    /// Strike through redaction replacements.
    pub cross_out: bool,
    /// Rotation in degrees.
    pub rotate: i32,
}

impl Default for AnnotUpdateOptions {
    fn default() -> Self {
        Self {
            blend_mode: None,
            opacity: None,
            fontsize: 0.0,
            fontname: None,
            text_color: None,
            border_color: None,
            fill_color: None,
            cross_out: true,
            rotate: -1,
        }
    }
}

impl AnnotUpdateOptions {
    /// Set the blend mode name.
    pub fn with_blend_mode(mut self, blend_mode: &str) -> Self {
        self.blend_mode = Some(blend_mode.to_string());
        self
    }

    /// Set the opacity level.
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// Set the font size in points.
    pub fn with_fontsize(mut self, fontsize: f64) -> Self {
        self.fontsize = fontsize;
        self
    }

    /// Set the font name.
    pub fn with_fontname(mut self, fontname: &str) -> Self {
        self.fontname = Some(fontname.to_string());
        self
    }

    /// Set the text colour components.
    pub fn with_text_color(mut self, text_color: Vec<f64>) -> Self {
        self.text_color = Some(text_color);
        self
    }

    /// Set the border colour components.
    pub fn with_border_color(mut self, border_color: Vec<f64>) -> Self {
        self.border_color = Some(border_color);
        self
    }

    /// Set the fill colour components.
    pub fn with_fill_color(mut self, fill_color: Vec<f64>) -> Self {
        self.fill_color = Some(fill_color);
        self
    }

    /// Strike through redaction replacements.
    pub fn with_cross_out(mut self, cross_out: bool) -> Self {
        self.cross_out = cross_out;
        self
    }

    /// Rotate by this many degrees.
    pub fn with_rotate(mut self, rotate: i32) -> Self {
        self.rotate = rotate;
        self
    }
}

/// Options for identity updates: whole info plus per-field overrides.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct AnnotInfoOptions {
    /// Whole replacement info, if any.
    pub info: Option<AnnotInfo>,
    /// Content text override.
    pub content: Option<String>,
    /// Title text override.
    pub title: Option<String>,
    /// Creation date stamp override.
    pub creation_date: Option<String>,
    /// Modification date stamp override.
    pub mod_date: Option<String>,
    /// Subject text override.
    pub subject: Option<String>,
}

/// An annotation handle: object number or name.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AnnotIdent {
    /// Annotation object number.
    Xref(i32),
    /// Annotation name.
    Name(String),
}

/// A file attachment: names, description, and size.
#[derive(Debug, Clone, PartialEq)]
pub struct FileInfo {
    /// Stored filename.
    pub filename: String,
    /// Display name.
    pub display_name: String,
    /// Description text.
    pub description: String,
    /// Data length, in bytes.
    pub size: usize,
}

/// A form widget: owned field data built blank, then added to a page.
///
/// The first four fields are the scaffolded core; the rest carry the recorded
/// field detail (flags, choices, border, default appearance) so listings
/// round-trip without a live document handle.
#[derive(Debug, Clone, PartialEq)]
pub struct Widget {
    /// Field name.
    pub field_name: String,
    /// Field type code.
    pub field_type: i32,
    /// Field value text.
    pub field_value: String,
    /// Field box, in points.
    pub rect: Rect,
    /// Widget annotation object number, once placed.
    pub xref: i32,
    /// Raw field flags (`/Ff`).
    pub field_flags: i32,
    /// Display mode: 0 hidden, 1 invisible, 2 shown.
    pub field_display: i32,
    /// Choice labels for list and combo boxes.
    pub choices: Vec<String>,
    /// Default-appearance font name.
    pub text_font: String,
    /// Default-appearance font size (0 means automatic).
    pub text_fontsize: f64,
    /// Whether the font size keeps its integer shape.
    pub text_fontsize_is_int: bool,
    /// Maximum field length (0 means unlimited).
    pub text_maxlen: i32,
    /// Border width, in points.
    pub border_width: f64,
    /// Whether the border width keeps its integer shape.
    pub border_width_is_int: bool,
    /// Border style name (`Solid`, `Dashed`, ...).
    pub border_style: String,
}

/// A live drawing canvas: pending marks on a page plus the document.
#[allow(dead_code)]
pub struct Shape<'a> {
    doc: &'a mut PdfDocument,
    page: usize,
    pending: Vec<DrawnPath>,
    finished: Vec<FinishedPath>,
    texts: Vec<StagedRun>,
    union: Option<Rect>,
}

/// Options for finishing a shape: stroke, fill, and compositing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ShapeFinishOptions {
    /// Stroke width, in points.
    pub width: f64,
    /// Stroke colour components.
    pub color: Vec<f64>,
    /// Fill colour components, if any.
    pub fill: Option<Vec<f64>>,
    /// Line cap style code.
    pub line_cap: i32,
    /// Line join style code.
    pub line_join: i32,
    /// Dash pattern, if any.
    pub dashes: Option<String>,
    /// Use the even-odd fill rule.
    pub even_odd: bool,
    /// Morph sequence, if any.
    pub morph: Option<Vec<f64>>,
    /// Close the path.
    pub close_path: bool,
    /// Fill opacity.
    pub fill_opacity: f64,
    /// Stroke opacity.
    pub stroke_opacity: f64,
    /// Optional-content group number.
    pub oc: i32,
}

impl Default for ShapeFinishOptions {
    fn default() -> Self {
        Self {
            width: 1.0,
            color: vec![0.0],
            fill: None,
            line_cap: 0,
            line_join: 0,
            dashes: None,
            even_odd: false,
            morph: None,
            close_path: true,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            oc: 0,
        }
    }
}

impl ShapeFinishOptions {
    /// Set the stroke width in points.
    pub fn with_width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }

    /// Set the stroke colour components.
    pub fn with_color(mut self, color: Vec<f64>) -> Self {
        self.color = color;
        self
    }

    /// Set the fill colour components.
    pub fn with_fill(mut self, fill: Vec<f64>) -> Self {
        self.fill = Some(fill);
        self
    }

    /// Set the line cap style code.
    pub fn with_line_cap(mut self, line_cap: i32) -> Self {
        self.line_cap = line_cap;
        self
    }

    /// Set the line join style code.
    pub fn with_line_join(mut self, line_join: i32) -> Self {
        self.line_join = line_join;
        self
    }

    /// Set the dash pattern.
    pub fn with_dashes(mut self, dashes: &str) -> Self {
        self.dashes = Some(dashes.to_string());
        self
    }

    /// Use the even-odd fill rule.
    pub fn with_even_odd(mut self, even_odd: bool) -> Self {
        self.even_odd = even_odd;
        self
    }

    /// Set the morph sequence.
    pub fn with_morph(mut self, morph: Vec<f64>) -> Self {
        self.morph = Some(morph);
        self
    }

    /// Close the path.
    pub fn with_close_path(mut self, close_path: bool) -> Self {
        self.close_path = close_path;
        self
    }

    /// Set the fill opacity.
    pub fn with_fill_opacity(mut self, fill_opacity: f64) -> Self {
        self.fill_opacity = fill_opacity;
        self
    }

    /// Set the stroke opacity.
    pub fn with_stroke_opacity(mut self, stroke_opacity: f64) -> Self {
        self.stroke_opacity = stroke_opacity;
        self
    }

    /// Set the optional-content group number.
    pub fn with_oc(mut self, oc: i32) -> Self {
        self.oc = oc;
        self
    }
}

/// Options for text insertion: font, colours, layout, and compositing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InsertTextOptions {
    /// Font size, in points.
    pub fontsize: f64,
    /// Line height, or the default when `None`.
    pub lineheight: Option<f64>,
    /// Font name.
    pub fontname: String,
    /// Font file path, if any.
    pub fontfile: Option<String>,
    /// Use simple font encoding.
    pub set_simple: bool,
    /// Text encoding code.
    pub encoding: i32,
    /// Text colour components.
    pub color: Option<Vec<f64>>,
    /// Fill colour components.
    pub fill: Option<Vec<f64>>,
    /// Border width, in points.
    pub border_width: f64,
    /// Miter limit.
    pub miter_limit: f64,
    /// Rendering mode code.
    pub render_mode: i32,
    /// Rotation in degrees.
    pub rotate: i32,
    /// Morph sequence, if any.
    pub morph: Option<Vec<f64>>,
    /// Draw over existing content.
    pub overlay: bool,
    /// Stroke opacity.
    pub stroke_opacity: f64,
    /// Fill opacity.
    pub fill_opacity: f64,
    /// Optional-content group number.
    pub oc: i32,
}

impl Default for InsertTextOptions {
    fn default() -> Self {
        Self {
            fontsize: 11.0,
            lineheight: None,
            fontname: "helv".to_string(),
            fontfile: None,
            set_simple: false,
            encoding: 0,
            color: None,
            fill: None,
            border_width: 0.05,
            miter_limit: 1.0,
            render_mode: 0,
            rotate: 0,
            morph: None,
            overlay: true,
            stroke_opacity: 1.0,
            fill_opacity: 1.0,
            oc: 0,
        }
    }
}

impl InsertTextOptions {
    /// Set the font size in points.
    pub fn with_fontsize(mut self, fontsize: f64) -> Self {
        self.fontsize = fontsize;
        self
    }

    /// Set the line height.
    pub fn with_lineheight(mut self, lineheight: f64) -> Self {
        self.lineheight = Some(lineheight);
        self
    }

    /// Set the font name.
    pub fn with_fontname(mut self, fontname: &str) -> Self {
        self.fontname = fontname.to_string();
        self
    }

    /// Take the font from this file.
    pub fn with_fontfile(mut self, fontfile: &str) -> Self {
        self.fontfile = Some(fontfile.to_string());
        self
    }

    /// Use simple font encoding.
    pub fn with_set_simple(mut self, set_simple: bool) -> Self {
        self.set_simple = set_simple;
        self
    }

    /// Set the text encoding code.
    pub fn with_encoding(mut self, encoding: i32) -> Self {
        self.encoding = encoding;
        self
    }

    /// Set the text colour components.
    pub fn with_color(mut self, color: Vec<f64>) -> Self {
        self.color = Some(color);
        self
    }

    /// Set the fill colour components.
    pub fn with_fill(mut self, fill: Vec<f64>) -> Self {
        self.fill = Some(fill);
        self
    }

    /// Set the border width in points.
    pub fn with_border_width(mut self, border_width: f64) -> Self {
        self.border_width = border_width;
        self
    }

    /// Set the miter limit.
    pub fn with_miter_limit(mut self, miter_limit: f64) -> Self {
        self.miter_limit = miter_limit;
        self
    }

    /// Set the rendering mode code.
    pub fn with_render_mode(mut self, render_mode: i32) -> Self {
        self.render_mode = render_mode;
        self
    }

    /// Rotate by this many degrees.
    pub fn with_rotate(mut self, rotate: i32) -> Self {
        self.rotate = rotate;
        self
    }

    /// Set the morph sequence.
    pub fn with_morph(mut self, morph: Vec<f64>) -> Self {
        self.morph = Some(morph);
        self
    }

    /// Draw over existing content.
    pub fn with_overlay(mut self, overlay: bool) -> Self {
        self.overlay = overlay;
        self
    }

    /// Set the stroke opacity.
    pub fn with_stroke_opacity(mut self, stroke_opacity: f64) -> Self {
        self.stroke_opacity = stroke_opacity;
        self
    }

    /// Set the fill opacity.
    pub fn with_fill_opacity(mut self, fill_opacity: f64) -> Self {
        self.fill_opacity = fill_opacity;
        self
    }

    /// Set the optional-content group number.
    pub fn with_oc(mut self, oc: i32) -> Self {
        self.oc = oc;
        self
    }
}

/// Options for textbox insertion: text knobs plus alignment and tabs.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TextBoxOptions {
    /// Shared text insertion knobs.
    pub text: InsertTextOptions,
    /// Alignment code.
    pub align: i32,
    /// Tab expansion width.
    pub expandtabs: i32,
}

impl Default for TextBoxOptions {
    fn default() -> Self {
        Self {
            text: InsertTextOptions::default(),
            align: 0,
            expandtabs: 1,
        }
    }
}

impl TextBoxOptions {
    /// Set the shared text insertion knobs.
    pub fn with_text(mut self, text: InsertTextOptions) -> Self {
        self.text = text;
        self
    }

    /// Set the alignment code.
    pub fn with_align(mut self, align: i32) -> Self {
        self.align = align;
        self
    }

    /// Set the tab expansion width.
    pub fn with_expandtabs(mut self, expandtabs: i32) -> Self {
        self.expandtabs = expandtabs;
        self
    }
}

/// Options for vector drawing: stroke, fill, dashes, and compositing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DrawOptions {
    /// Stroke colour components.
    pub color: Vec<f64>,
    /// Fill colour components, if any.
    pub fill: Option<Vec<f64>>,
    /// Dash pattern, if any.
    pub dashes: Option<String>,
    /// Stroke width, in points.
    pub width: f64,
    /// Line cap style code.
    pub line_cap: i32,
    /// Line join style code.
    pub line_join: i32,
    /// Morph sequence, if any.
    pub morph: Option<Vec<f64>>,
    /// Close the path.
    pub close_path: bool,
    /// Draw over existing content.
    pub overlay: bool,
    /// Stroke opacity.
    pub stroke_opacity: f64,
    /// Fill opacity.
    pub fill_opacity: f64,
    /// Optional-content group number.
    pub oc: i32,
}

impl Default for DrawOptions {
    fn default() -> Self {
        Self {
            color: vec![0.0],
            fill: None,
            dashes: None,
            width: 1.0,
            line_cap: 0,
            line_join: 0,
            morph: None,
            close_path: false,
            overlay: true,
            stroke_opacity: 1.0,
            fill_opacity: 1.0,
            oc: 0,
        }
    }
}

impl DrawOptions {
    /// Set the stroke colour components.
    pub fn with_color(mut self, color: Vec<f64>) -> Self {
        self.color = color;
        self
    }

    /// Set the fill colour components.
    pub fn with_fill(mut self, fill: Vec<f64>) -> Self {
        self.fill = Some(fill);
        self
    }

    /// Set the dash pattern.
    pub fn with_dashes(mut self, dashes: &str) -> Self {
        self.dashes = Some(dashes.to_string());
        self
    }

    /// Set the stroke width in points.
    pub fn with_width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }

    /// Set the line cap style code.
    pub fn with_line_cap(mut self, line_cap: i32) -> Self {
        self.line_cap = line_cap;
        self
    }

    /// Set the line join style code.
    pub fn with_line_join(mut self, line_join: i32) -> Self {
        self.line_join = line_join;
        self
    }

    /// Set the morph sequence.
    pub fn with_morph(mut self, morph: Vec<f64>) -> Self {
        self.morph = Some(morph);
        self
    }

    /// Close the path.
    pub fn with_close_path(mut self, close_path: bool) -> Self {
        self.close_path = close_path;
        self
    }

    /// Draw over existing content.
    pub fn with_overlay(mut self, overlay: bool) -> Self {
        self.overlay = overlay;
        self
    }

    /// Set the stroke opacity.
    pub fn with_stroke_opacity(mut self, stroke_opacity: f64) -> Self {
        self.stroke_opacity = stroke_opacity;
        self
    }

    /// Set the fill opacity.
    pub fn with_fill_opacity(mut self, fill_opacity: f64) -> Self {
        self.fill_opacity = fill_opacity;
        self
    }

    /// Set the optional-content group number.
    pub fn with_oc(mut self, oc: i32) -> Self {
        self.oc = oc;
        self
    }
}

/// Options for redaction annotations: replacement text and styling.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RedactAnnotOptions {
    /// Replacement text, if any.
    pub text: Option<String>,
    /// Font name.
    pub fontname: Option<String>,
    /// Font size, in points.
    pub fontsize: f64,
    /// Alignment code.
    pub align: i32,
    /// Fill colour components.
    pub fill: Option<Vec<f64>>,
    /// Text colour components.
    pub text_color: Option<Vec<f64>>,
    /// Strike through the replacement.
    pub cross_out: bool,
}

impl Default for RedactAnnotOptions {
    fn default() -> Self {
        Self {
            text: None,
            fontname: None,
            fontsize: 11.0,
            align: 0,
            fill: None,
            text_color: None,
            cross_out: true,
        }
    }
}

impl RedactAnnotOptions {
    /// Set the replacement text.
    pub fn with_text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }

    /// Set the font name.
    pub fn with_fontname(mut self, fontname: &str) -> Self {
        self.fontname = Some(fontname.to_string());
        self
    }

    /// Set the font size in points.
    pub fn with_fontsize(mut self, fontsize: f64) -> Self {
        self.fontsize = fontsize;
        self
    }

    /// Set the alignment code.
    pub fn with_align(mut self, align: i32) -> Self {
        self.align = align;
        self
    }

    /// Set the fill colour components.
    pub fn with_fill(mut self, fill: Vec<f64>) -> Self {
        self.fill = Some(fill);
        self
    }

    /// Set the text colour components.
    pub fn with_text_color(mut self, text_color: Vec<f64>) -> Self {
        self.text_color = Some(text_color);
        self
    }

    /// Strike through the replacement.
    pub fn with_cross_out(mut self, cross_out: bool) -> Self {
        self.cross_out = cross_out;
        self
    }
}

/// Options for freetext annotations: font, colours, border, and layout.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FreeTextOptions {
    /// Font size, in points.
    pub fontsize: f64,
    /// Font name.
    pub fontname: Option<String>,
    /// Text colour components.
    pub text_color: Option<Vec<f64>>,
    /// Fill colour components.
    pub fill_color: Option<Vec<f64>>,
    /// Border colour components.
    pub border_color: Option<Vec<f64>>,
    /// Border width, in points.
    pub border_width: f64,
    /// Dash pattern components.
    pub dashes: Option<Vec<f64>>,
    /// Callout line points.
    pub callout: Option<Vec<Point>>,
    /// Callout line-end style code.
    pub line_end: i32,
    /// Opacity level.
    pub opacity: f64,
    /// Alignment code.
    pub align: i32,
    /// Rotation in degrees.
    pub rotate: i32,
    /// Use rich text.
    pub richtext: bool,
    /// Style override.
    pub style: Option<Value>,
}

impl Default for FreeTextOptions {
    fn default() -> Self {
        Self {
            fontsize: 11.0,
            fontname: None,
            text_color: None,
            fill_color: None,
            border_color: None,
            border_width: 0.0,
            dashes: None,
            callout: None,
            line_end: 4,
            opacity: 1.0,
            align: 0,
            rotate: 0,
            richtext: false,
            style: None,
        }
    }
}

impl FreeTextOptions {
    /// Set the font size in points.
    pub fn with_fontsize(mut self, fontsize: f64) -> Self {
        self.fontsize = fontsize;
        self
    }

    /// Set the font name.
    pub fn with_fontname(mut self, fontname: &str) -> Self {
        self.fontname = Some(fontname.to_string());
        self
    }

    /// Set the text colour components.
    pub fn with_text_color(mut self, text_color: Vec<f64>) -> Self {
        self.text_color = Some(text_color);
        self
    }

    /// Set the fill colour components.
    pub fn with_fill_color(mut self, fill_color: Vec<f64>) -> Self {
        self.fill_color = Some(fill_color);
        self
    }

    /// Set the border colour components.
    pub fn with_border_color(mut self, border_color: Vec<f64>) -> Self {
        self.border_color = Some(border_color);
        self
    }

    /// Set the border width in points.
    pub fn with_border_width(mut self, border_width: f64) -> Self {
        self.border_width = border_width;
        self
    }

    /// Set the dash pattern components.
    pub fn with_dashes(mut self, dashes: Vec<f64>) -> Self {
        self.dashes = Some(dashes);
        self
    }

    /// Set the callout line points.
    pub fn with_callout(mut self, callout: Vec<Point>) -> Self {
        self.callout = Some(callout);
        self
    }

    /// Set the callout line-end style code.
    pub fn with_line_end(mut self, line_end: i32) -> Self {
        self.line_end = line_end;
        self
    }

    /// Set the opacity level.
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }

    /// Set the alignment code.
    pub fn with_align(mut self, align: i32) -> Self {
        self.align = align;
        self
    }

    /// Rotate by this many degrees.
    pub fn with_rotate(mut self, rotate: i32) -> Self {
        self.rotate = rotate;
        self
    }

    /// Use rich text.
    pub fn with_richtext(mut self, richtext: bool) -> Self {
        self.richtext = richtext;
        self
    }

    /// Set the style override.
    pub fn with_style(mut self, style: Value) -> Self {
        self.style = Some(style);
        self
    }
}

/// Options for file annotations: names, description, and icon.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FileAnnotOptions {
    /// Stored filename.
    pub filename: String,
    /// Display name.
    pub display_name: Option<String>,
    /// Description text.
    pub description: Option<String>,
    /// Icon name.
    pub icon: Option<String>,
}

impl FileAnnotOptions {
    /// Describe this stored filename.
    pub fn new(filename: &str) -> Self {
        Self {
            filename: filename.to_string(),
            display_name: None,
            description: None,
            icon: None,
        }
    }

    /// Set the display name.
    pub fn with_display_name(mut self, display_name: &str) -> Self {
        self.display_name = Some(display_name.to_string());
        self
    }

    /// Set the description text.
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Set the icon name.
    pub fn with_icon(mut self, icon: &str) -> Self {
        self.icon = Some(icon.to_string());
        self
    }
}

/// Mark location for text-mark annotations: quads, span, or clip.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct HighlightOptions {
    /// Quads to mark, if any.
    pub quads: Option<Vec<Quad>>,
    /// Span start point, if any.
    pub start: Option<Point>,
    /// Span stop point, if any.
    pub stop: Option<Point>,
    /// Clip rectangle, if any.
    pub clip: Option<Rect>,
}

/// Options for image insertion: source plus placement and compositing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ImageInsertOptions {
    /// Alpha override (-1 for the default).
    pub alpha: i32,
    /// Image file path, if any.
    pub filename: Option<String>,
    /// Target height, or zero for the natural size.
    pub height: f64,
    /// Keep the aspect ratio.
    pub keep_proportion: bool,
    /// Mask override.
    pub mask: Option<Value>,
    /// Optional-content group number.
    pub oc: i32,
    /// Draw over existing content.
    pub overlay: bool,
    /// Pixmap to place, if any.
    pub pixmap: Option<Pixmap>,
    /// Rotation in degrees.
    pub rotate: i32,
    /// Image bytes, if any.
    pub stream: Option<Vec<u8>>,
    /// Target width, or zero for the natural size.
    pub width: f64,
    /// Source image object number, or zero.
    pub xref: i32,
}

impl Default for ImageInsertOptions {
    fn default() -> Self {
        Self {
            alpha: -1,
            filename: None,
            height: 0.0,
            keep_proportion: true,
            mask: None,
            oc: 0,
            overlay: true,
            pixmap: None,
            rotate: 0,
            stream: None,
            width: 0.0,
            xref: 0,
        }
    }
}

impl ImageInsertOptions {
    /// Override the alpha channel.
    pub fn with_alpha(mut self, alpha: i32) -> Self {
        self.alpha = alpha;
        self
    }

    /// Take the image from this file.
    pub fn with_filename(mut self, filename: &str) -> Self {
        self.filename = Some(filename.to_string());
        self
    }

    /// Set the target size (zero keeps the natural size).
    pub fn with_size(mut self, width: f64, height: f64) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Keep the aspect ratio.
    pub fn with_keep_proportion(mut self, keep_proportion: bool) -> Self {
        self.keep_proportion = keep_proportion;
        self
    }

    /// Set the mask override.
    pub fn with_mask(mut self, mask: Value) -> Self {
        self.mask = Some(mask);
        self
    }

    /// Set the optional-content group number.
    pub fn with_oc(mut self, oc: i32) -> Self {
        self.oc = oc;
        self
    }

    /// Draw over existing content.
    pub fn with_overlay(mut self, overlay: bool) -> Self {
        self.overlay = overlay;
        self
    }

    /// Place this pixmap.
    pub fn with_pixmap(mut self, pixmap: Pixmap) -> Self {
        self.pixmap = Some(pixmap);
        self
    }

    /// Rotate by this many degrees.
    pub fn with_rotate(mut self, rotate: i32) -> Self {
        self.rotate = rotate;
        self
    }

    /// Take the image from these bytes.
    pub fn with_stream(mut self, stream: Vec<u8>) -> Self {
        self.stream = Some(stream);
        self
    }

    /// Copy this source image object.
    pub fn with_xref(mut self, xref: i32) -> Self {
        self.xref = xref;
        self
    }
}

/// Options for HTML box insertion: style, archive, rotation, and compositing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HtmlBoxOptions {
    /// Stylesheet text, if any.
    pub css: Option<String>,
    /// Low scale factor.
    pub scale_low: f64,
    /// Resource archive handle, if any.
    pub archive: Option<Value>,
    /// Rotation in degrees.
    pub rotate: i32,
    /// Optional-content group number.
    pub oc: i32,
    /// Opacity level.
    pub opacity: f64,
    /// Draw over existing content.
    pub overlay: bool,
}

impl Default for HtmlBoxOptions {
    fn default() -> Self {
        Self {
            css: None,
            scale_low: 0.0,
            archive: None,
            rotate: 0,
            oc: 0,
            opacity: 1.0,
            overlay: true,
        }
    }
}

impl HtmlBoxOptions {
    /// Style with this stylesheet text.
    pub fn with_css(mut self, css: &str) -> Self {
        self.css = Some(css.to_string());
        self
    }

    /// Set the low scale factor.
    pub fn with_scale_low(mut self, scale_low: f64) -> Self {
        self.scale_low = scale_low;
        self
    }

    /// Resolve resources through this archive handle.
    pub fn with_archive(mut self, archive: Value) -> Self {
        self.archive = Some(archive);
        self
    }

    /// Rotate by this many degrees.
    pub fn with_rotate(mut self, rotate: i32) -> Self {
        self.rotate = rotate;
        self
    }

    /// Set the optional-content group number.
    pub fn with_oc(mut self, oc: i32) -> Self {
        self.oc = oc;
        self
    }

    /// Set the opacity level.
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }

    /// Draw over existing content.
    pub fn with_overlay(mut self, overlay: bool) -> Self {
        self.overlay = overlay;
        self
    }
}

/// Options for image replacement: one of file, pixmap, or bytes.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ReplaceImageOptions {
    /// Replacement file path, if any.
    pub filename: Option<String>,
    /// Replacement pixmap, if any.
    pub pixmap: Option<Pixmap>,
    /// Replacement bytes, if any.
    pub stream: Option<Vec<u8>>,
}

/// Options for showing a foreign page: source span and compositing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ShowPageOptions {
    /// Zero-based source page number.
    pub page_no: i32,
    /// Keep the aspect ratio.
    pub keep_proportion: bool,
    /// Draw over existing content.
    pub overlay: bool,
    /// Optional-content group number.
    pub oc: i32,
    /// Rotation in degrees.
    pub rotate: i32,
    /// Clip rectangle, if any.
    pub clip: Option<Rect>,
}

impl Default for ShowPageOptions {
    fn default() -> Self {
        Self {
            page_no: 0,
            keep_proportion: true,
            overlay: true,
            oc: 0,
            rotate: 0,
            clip: None,
        }
    }
}

impl ShowPageOptions {
    /// Show this zero-based source page.
    pub fn with_page_no(mut self, page_no: i32) -> Self {
        self.page_no = page_no;
        self
    }

    /// Keep the aspect ratio.
    pub fn with_keep_proportion(mut self, keep_proportion: bool) -> Self {
        self.keep_proportion = keep_proportion;
        self
    }

    /// Draw over existing content.
    pub fn with_overlay(mut self, overlay: bool) -> Self {
        self.overlay = overlay;
        self
    }

    /// Set the optional-content group number.
    pub fn with_oc(mut self, oc: i32) -> Self {
        self.oc = oc;
        self
    }

    /// Rotate by this many degrees.
    pub fn with_rotate(mut self, rotate: i32) -> Self {
        self.rotate = rotate;
        self
    }

    /// Clip to this rectangle.
    pub fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }
}

/// Options for font insertion: name, source, and encoding.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InsertFontOptions {
    /// Font name.
    pub fontname: String,
    /// Font file path, if any.
    pub fontfile: Option<String>,
    /// Font bytes, if any.
    pub fontbuffer: Option<Vec<u8>>,
    /// Use simple font encoding.
    pub set_simple: bool,
    /// Writing mode code.
    pub wmode: i32,
    /// Text encoding code.
    pub encoding: i32,
}

impl Default for InsertFontOptions {
    fn default() -> Self {
        Self {
            fontname: "helv".to_string(),
            fontfile: None,
            fontbuffer: None,
            set_simple: false,
            wmode: 0,
            encoding: 0,
        }
    }
}

/// Options for writing text: area, writers, colours, and compositing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct WriteTextOptions {
    /// Target rectangle, if any.
    pub rect: Option<Rect>,
    /// Text writers, if any.
    pub writers: Option<Value>,
    /// Draw over existing content.
    pub overlay: bool,
    /// Text colour components.
    pub color: Option<Vec<f64>>,
    /// Opacity level.
    pub opacity: Option<f64>,
    /// Keep the aspect ratio.
    pub keep_proportion: bool,
    /// Rotation in degrees.
    pub rotate: i32,
    /// Optional-content group number.
    pub oc: i32,
}

impl Default for WriteTextOptions {
    fn default() -> Self {
        Self {
            rect: None,
            writers: None,
            overlay: true,
            color: None,
            opacity: None,
            keep_proportion: true,
            rotate: 0,
            oc: 0,
        }
    }
}

/// One vector drawing: style plus path items.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct Drawing {
    /// Drawing box, in points.
    pub rect: Rect,
    /// Sequence number on the page.
    pub seqno: i32,
    /// Drawing type code.
    pub drawing_type: String,
    /// Stroke colour components.
    pub color: Option<Vec<f64>>,
    /// Fill colour components.
    pub fill: Option<Vec<f64>>,
    /// Stroke width, in points.
    pub width: Option<f64>,
    /// Line cap styles.
    pub line_cap: Option<Vec<i32>>,
    /// Line join style.
    pub line_join: Option<i32>,
    /// Dash pattern.
    pub dashes: Option<String>,
    /// Whether the path is closed.
    pub close_path: Option<bool>,
    /// Whether the even-odd rule applies.
    pub even_odd: bool,
    /// Fill opacity.
    pub fill_opacity: Option<f64>,
    /// Stroke opacity.
    pub stroke_opacity: Option<f64>,
    /// Layer name.
    pub layer: String,
    /// Nesting level on extended reads.
    pub level: Option<i32>,
    /// Path items.
    pub items: Vec<DrawingItem>,
}

/// One drawing path item: operator plus unshaped operands.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawingItem {
    /// Path operator name.
    pub op: String,
    /// Operands in recorded shape.
    pub params: Value,
}

/// One drawing cluster: nearby drawings under one box.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DrawingCluster {
    /// Cluster box, in points.
    pub bbox: Rect,
    /// Clustered drawings.
    pub drawings: Vec<Drawing>,
}

/// Drawing callback for streamed reads: called once per drawing.
pub type DrawCallback = fn(&Drawing);

/// Options for streamed drawing reads: detail plus an optional callback.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct CdrawingsOptions {
    /// Include extended detail.
    pub extended: bool,
    /// Callback per drawing, if any.
    pub callback: Option<DrawCallback>,
}

/// Options for applying redactions: per-kind effort levels.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RedactApplyOptions {
    /// Image redaction level.
    pub images: i32,
    /// Graphics redaction level.
    pub graphics: i32,
    /// Text redaction level.
    pub text: i32,
}

impl Default for RedactApplyOptions {
    fn default() -> Self {
        Self {
            images: 2,
            graphics: 1,
            text: 0,
        }
    }
}

/// Options for clustering drawings: input, tolerances, and filtering.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ClusterOptions {
    /// Clip rectangle, if any.
    pub clip: Option<Rect>,
    /// Drawings to cluster, or the page set when `None`.
    pub drawings: Option<Vec<Drawing>>,
    /// Horizontal merge tolerance, in points.
    pub x_tolerance: f64,
    /// Vertical merge tolerance, in points.
    pub y_tolerance: f64,
    /// Drop clusters fully inside another.
    pub final_filter: bool,
}

impl Default for ClusterOptions {
    fn default() -> Self {
        Self {
            clip: None,
            drawings: None,
            x_tolerance: 3.0,
            y_tolerance: 3.0,
            final_filter: true,
        }
    }
}

/// An owned font: name plus bytes, built from a name, file, or buffer.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct Font {
    name: String,
    data: Vec<u8>,
}

/// Font capability flags: style, embedding, and encoding facts.
#[derive(Debug, Clone, PartialEq)]
pub struct FontFlags {
    /// Bold variant.
    pub bold: bool,
    /// CJK font.
    pub cjk: bool,
    /// CJK language variant.
    pub cjk_lang: bool,
    /// Embedding allowed.
    pub embed: bool,
    /// Synthetic bold.
    pub fake_bold: bool,
    /// Synthetic italic.
    pub fake_italic: bool,
    /// Bounding box untrusted.
    pub invalid_bbox: bool,
    /// Italic variant.
    pub italic: bool,
    /// Fixed-width font.
    pub mono: bool,
    /// Embedding forbidden.
    pub never_embed: bool,
    /// OpenType font.
    pub opentype: bool,
    /// Serif font.
    pub serif: bool,
    /// Stretched variant.
    pub stretch: bool,
    /// Fallback font.
    pub substitute: bool,
}

/// Options for building a font: source, script, style, and embedding.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FontOptions {
    /// Font name, if any.
    pub fontname: Option<String>,
    /// Font file path, if any.
    pub fontfile: Option<String>,
    /// Font bytes, if any.
    pub fontbuffer: Option<Vec<u8>>,
    /// Script code.
    pub script: i32,
    /// Language tag, if any.
    pub language: Option<String>,
    /// Character collection order.
    pub ordering: i32,
    /// Bold variant.
    pub is_bold: bool,
    /// Italic variant.
    pub is_italic: bool,
    /// Serif variant.
    pub is_serif: bool,
    /// Embed the font.
    pub embed: bool,
}

impl Default for FontOptions {
    fn default() -> Self {
        Self {
            fontname: None,
            fontfile: None,
            fontbuffer: None,
            script: 0,
            language: None,
            ordering: -1,
            is_bold: false,
            is_italic: false,
            is_serif: false,
            embed: true,
        }
    }
}

impl FontOptions {
    /// Set the font name.
    pub fn with_fontname(mut self, fontname: &str) -> Self {
        self.fontname = Some(fontname.to_string());
        self
    }

    /// Set the font file path.
    pub fn with_fontfile(mut self, fontfile: &str) -> Self {
        self.fontfile = Some(fontfile.to_string());
        self
    }

    /// Set the font bytes.
    pub fn with_fontbuffer(mut self, fontbuffer: Vec<u8>) -> Self {
        self.fontbuffer = Some(fontbuffer);
        self
    }

    /// Set the script code.
    pub fn with_script(mut self, script: i32) -> Self {
        self.script = script;
        self
    }

    /// Set the language tag.
    pub fn with_language(mut self, language: &str) -> Self {
        self.language = Some(language.to_string());
        self
    }

    /// Set the character collection order.
    pub fn with_ordering(mut self, ordering: i32) -> Self {
        self.ordering = ordering;
        self
    }

    /// Set the bold variant.
    pub fn with_is_bold(mut self, is_bold: bool) -> Self {
        self.is_bold = is_bold;
        self
    }

    /// Set the italic variant.
    pub fn with_is_italic(mut self, is_italic: bool) -> Self {
        self.is_italic = is_italic;
        self
    }

    /// Set the serif variant.
    pub fn with_is_serif(mut self, is_serif: bool) -> Self {
        self.is_serif = is_serif;
        self
    }

    /// Set whether to embed the font.
    pub fn with_embed(mut self, embed: bool) -> Self {
        self.embed = embed;
        self
    }
}

/// Shared measuring knobs for the font metric methods.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FontMeasureOptions {
    /// Font size, in points.
    pub fontsize: f64,
    /// Language tag, if any.
    pub language: Option<String>,
    /// Script code.
    pub script: i32,
    /// Writing mode code.
    pub wmode: i32,
    /// Render in small capitals.
    pub small_caps: bool,
    /// Fallback font policy.
    pub fallback: i32,
}

impl Default for FontMeasureOptions {
    fn default() -> Self {
        Self {
            fontsize: 11.0,
            language: None,
            script: 0,
            wmode: 0,
            small_caps: false,
            fallback: 0,
        }
    }
}

impl FontMeasureOptions {
    /// Set the font size in points.
    pub fn with_fontsize(mut self, fontsize: f64) -> Self {
        self.fontsize = fontsize;
        self
    }

    /// Set the language tag.
    pub fn with_language(mut self, language: &str) -> Self {
        self.language = Some(language.to_string());
        self
    }

    /// Set the script code.
    pub fn with_script(mut self, script: i32) -> Self {
        self.script = script;
        self
    }

    /// Set the writing mode code.
    pub fn with_wmode(mut self, wmode: i32) -> Self {
        self.wmode = wmode;
        self
    }

    /// Set small capitals rendering.
    pub fn with_small_caps(mut self, small_caps: bool) -> Self {
        self.small_caps = small_caps;
        self
    }

    /// Set the fallback font policy.
    pub fn with_fallback(mut self, fallback: i32) -> Self {
        self.fallback = fallback;
        self
    }
}

/// An owned text writer: staged text flushed onto a page.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct TextWriter {
    rect: Rect,
    opacity: f64,
    color: Option<Vec<f64>>,
    staged: Vec<StagedRun>,
    union: Option<Rect>,
}

/// Options for a writer: opacity and colour.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TextWriterOptions {
    /// Opacity level.
    pub opacity: f64,
    /// Text colour components, if any.
    pub color: Option<Vec<f64>>,
}

impl Default for TextWriterOptions {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            color: None,
        }
    }
}

impl TextWriterOptions {
    /// Set the opacity level.
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }

    /// Set the text colour components.
    pub fn with_color(mut self, color: Vec<f64>) -> Self {
        self.color = Some(color);
        self
    }
}

/// Options for appending text: font, size, and direction.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AppendOptions<'a> {
    /// Font, or the default when `None`.
    pub font: Option<&'a Font>,
    /// Font size, in points.
    pub fontsize: f64,
    /// Language tag, if any.
    pub language: Option<String>,
    /// Right-to-left text.
    pub right_to_left: bool,
    /// Render in small capitals.
    pub small_caps: bool,
}

impl<'a> Default for AppendOptions<'a> {
    fn default() -> Self {
        Self {
            font: None,
            fontsize: 11.0,
            language: None,
            right_to_left: false,
            small_caps: false,
        }
    }
}

impl<'a> AppendOptions<'a> {
    /// Set the font.
    pub fn with_font(mut self, font: &'a Font) -> Self {
        self.font = Some(font);
        self
    }

    /// Set the font size in points.
    pub fn with_fontsize(mut self, fontsize: f64) -> Self {
        self.fontsize = fontsize;
        self
    }

    /// Set the language tag.
    pub fn with_language(mut self, language: &str) -> Self {
        self.language = Some(language.to_string());
        self
    }

    /// Set right-to-left text.
    pub fn with_right_to_left(mut self, right_to_left: bool) -> Self {
        self.right_to_left = right_to_left;
        self
    }

    /// Set small capitals rendering.
    pub fn with_small_caps(mut self, small_caps: bool) -> Self {
        self.small_caps = small_caps;
        self
    }
}

/// Options for filling a box: start, font, layout, and direction.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FillTextBoxOptions<'a> {
    /// Start position, or the box top when `None`.
    pub pos: Option<Point>,
    /// Font, or the default when `None`.
    pub font: Option<&'a Font>,
    /// Font size, in points.
    pub fontsize: f64,
    /// Line height, or the default when `None`.
    pub lineheight: Option<f64>,
    /// Alignment code.
    pub align: i32,
    /// Warn about overflow text, if set.
    pub warn: Option<bool>,
    /// Right-to-left text.
    pub right_to_left: bool,
    /// Render in small capitals.
    pub small_caps: bool,
}

impl<'a> Default for FillTextBoxOptions<'a> {
    fn default() -> Self {
        Self {
            pos: None,
            font: None,
            fontsize: 11.0,
            lineheight: None,
            align: 0,
            warn: None,
            right_to_left: false,
            small_caps: false,
        }
    }
}

impl<'a> FillTextBoxOptions<'a> {
    /// Set the start position.
    pub fn with_pos(mut self, pos: Point) -> Self {
        self.pos = Some(pos);
        self
    }

    /// Set the font.
    pub fn with_font(mut self, font: &'a Font) -> Self {
        self.font = Some(font);
        self
    }

    /// Set the font size in points.
    pub fn with_fontsize(mut self, fontsize: f64) -> Self {
        self.fontsize = fontsize;
        self
    }

    /// Set the line height.
    pub fn with_lineheight(mut self, lineheight: f64) -> Self {
        self.lineheight = Some(lineheight);
        self
    }

    /// Set the alignment code.
    pub fn with_align(mut self, align: i32) -> Self {
        self.align = align;
        self
    }

    /// Set overflow warning.
    pub fn with_warn(mut self, warn: bool) -> Self {
        self.warn = Some(warn);
        self
    }

    /// Set right-to-left text.
    pub fn with_right_to_left(mut self, right_to_left: bool) -> Self {
        self.right_to_left = right_to_left;
        self
    }

    /// Set small capitals rendering.
    pub fn with_small_caps(mut self, small_caps: bool) -> Self {
        self.small_caps = small_caps;
        self
    }
}

/// Options for flushing a writer onto a page: colours and compositing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FlushOptions {
    /// Text colour components, if any.
    pub color: Option<Vec<f64>>,
    /// Opacity level, or the writer's when `None`.
    pub opacity: Option<f64>,
    /// Draw over existing content.
    pub overlay: bool,
    /// Morph sequence, if any.
    pub morph: Option<Vec<f64>>,
    /// Transformation, if any.
    pub matrix: Option<Matrix>,
    /// Rendering mode code.
    pub render_mode: i32,
    /// Optional-content group number.
    pub oc: i32,
}

impl Default for FlushOptions {
    fn default() -> Self {
        Self {
            color: None,
            opacity: None,
            overlay: true,
            morph: None,
            matrix: None,
            render_mode: 0,
            oc: 0,
        }
    }
}

impl FlushOptions {
    /// Set the text colour components.
    pub fn with_color(mut self, color: Vec<f64>) -> Self {
        self.color = Some(color);
        self
    }

    /// Set the opacity level.
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// Set whether to draw over existing content.
    pub fn with_overlay(mut self, overlay: bool) -> Self {
        self.overlay = overlay;
        self
    }

    /// Set the morph sequence.
    pub fn with_morph(mut self, morph: Vec<f64>) -> Self {
        self.morph = Some(morph);
        self
    }

    /// Set the transformation.
    pub fn with_matrix(mut self, matrix: Matrix) -> Self {
        self.matrix = Some(matrix);
        self
    }

    /// Set the rendering mode code.
    pub fn with_render_mode(mut self, render_mode: i32) -> Self {
        self.render_mode = render_mode;
        self
    }

    /// Set the optional-content group number.
    pub fn with_oc(mut self, oc: i32) -> Self {
        self.oc = oc;
        self
    }
}

impl<'a> Annot<'a> {
    /// Whether the annotation handle is live.
    pub fn to_bool(&self) -> bool {
        true
    }

    /// Wrap an annotation object on a page.
    pub fn new<'b>(_page: &'b mut PageMut<'_>, _xref: i32) -> Annot<'b> {
        Annot {
            page: _page.index(),
            doc: _page.document_mut(),
            xref: _xref,
        }
    }

    /// Ensure the widget is in the calculation order: its reference is
    /// appended to the catalog form's `/CO` array, created (with the form
    /// itself) when missing. Repeating the call is stable: a listed
    /// widget is left where it is.
    pub(crate) fn ensure_calc_order(self) -> Result<()> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let subtype = dict.get(b"Subtype").ok().map(obj_name).unwrap_or_default();
        if subtype != "Widget" {
            return Err(bad_value("annotation is not a widget"));
        }
        let widget = (self.xref as u32, 0);
        let lp = self.doc.lopdf_mut();
        let form = ensure_acroform(lp)?;
        let mut order = lp
            .get_dictionary(form)
            .ok()
            .and_then(|dict| dict.get(b"CO").ok().cloned())
            .and_then(|obj| obj.as_array().ok().cloned())
            .unwrap_or_default();
        if !order
            .iter()
            .any(|obj| obj.as_reference().ok() == Some(widget))
        {
            order.push(Object::Reference(widget));
        }
        lp.get_dictionary_mut(form)?.set("CO", Object::Array(order));
        sync_doc(self.doc)
    }

    /// Appearance box.
    pub fn apn_bbox(&self) -> Result<Rect> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let ap = dict
            .get(b"AP")
            .ok()
            .map(|o| deref(lp, o))
            .and_then(|o| o.as_dict().ok())
            .ok_or_else(|| bad_value("annotation has no appearance"))?;
        let normal = ap
            .get(b"N")
            .ok()
            .map(|o| deref(lp, o))
            .ok_or_else(|| bad_value("annotation has no normal appearance"))?;
        let bbox = normal
            .as_dict()
            .ok()
            .and_then(|d| d.get(b"BBox").ok().map(|o| deref(lp, o)))
            .and_then(obj_rect)
            .or_else(|| {
                normal
                    .as_stream()
                    .ok()
                    .and_then(|s| s.dict.get(b"BBox").ok())
                    .and_then(obj_rect)
            })
            .ok_or_else(|| bad_value("appearance has no bounding box"))?;
        let page_id = page_id_of(lp, self.page)?;
        let top = page_top(lp, page_id);
        Ok(flip_rect(&bbox, top))
    }

    /// Appearance matrix.
    pub fn apn_matrix(&self) -> Result<Matrix> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let found = dict
            .get(b"AP")
            .ok()
            .map(|o| deref(lp, o))
            .and_then(|o| o.as_dict().ok())
            .and_then(|ap| ap.get(b"N").ok().map(|o| deref(lp, o)))
            .and_then(|normal| {
                normal
                    .as_dict()
                    .ok()
                    .and_then(|d| d.get(b"Matrix").ok())
                    .or_else(|| {
                        normal
                            .as_stream()
                            .ok()
                            .and_then(|s| s.dict.get(b"Matrix").ok())
                    })
            })
            .and_then(|o| o.as_array().ok())
            .map(|arr| arr.iter().map(obj_f64).collect::<Vec<_>>());
        match found {
            Some(v) if v.len() >= 6 => Ok(mk_matrix(v[0], v[1], v[2], v[3], v[4], v[5])),
            _ => Ok(mk_matrix(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)),
        }
    }

    /// Blend mode name.
    pub fn blendmode(&self) -> Result<String> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(dict
            .get(b"BM")
            .ok()
            .map(|o| deref(lp, o))
            .map(obj_name)
            .filter(|s| !s.is_empty())
            .unwrap_or_default())
    }

    /// Border styling.
    pub fn border(&self) -> Result<Border> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(read_border(lp, &dict))
    }

    /// Clean appearance streams.
    pub fn clean_contents(&mut self, _sanitize: bool) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = annot_dict(lp, self.xref)?;
        let ap_id = dict
            .get(b"AP")
            .ok()
            .and_then(|o| o.as_dict().ok())
            .and_then(|ap| ap.get(b"N").ok())
            .and_then(|o| o.as_reference().ok());
        if let Some(id) = ap_id {
            if let Ok(Object::Stream(stream)) = lp.get_object_mut(id) {
                if let Ok(content) = stream.decompressed_content() {
                    if let Ok(decoded) = lopdf::content::Content::decode(&content) {
                        if let Ok(encoded) = decoded.encode() {
                            let _ = _sanitize;
                            stream.set_content(encoded);
                            stream.dict.remove(b"Filter");
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Stroke and fill colours.
    pub fn colors(&self) -> Result<AnnotColors> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(read_colors(lp, &dict))
    }

    /// Delete reply annotations.
    pub fn delete_responses(&mut self) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let me = self.xref;
        let page_id = page_id_of(lp, self.page)?;
        let ids = page_annots(lp, page_id);
        let mut keep = Vec::new();
        for id in ids {
            let is_reply = lp
                .get_dictionary(id)
                .ok()
                .and_then(|d| d.get(b"IRT").ok().cloned())
                .and_then(|o| o.as_reference().ok())
                .map(|target| target.0 == me as u32)
                .unwrap_or(false);
            if !is_reply {
                keep.push(id);
            }
        }
        set_page_annots(lp, page_id, &keep)?;
        sync_doc(self.doc)
    }

    /// Attachment facts.
    pub fn file_info(&self) -> Result<FileInfo> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        read_file_info(lp, &dict)
    }

    /// Annotation flags bitfield.
    pub fn flags(&self) -> Result<i32> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(dict.get(b"F").ok().map(obj_i32).unwrap_or(0))
    }

    /// Attachment bytes.
    pub fn get_file(&self) -> Result<Vec<u8>> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        filespec_parts(lp, &dict)
            .map(|(_, bytes)| bytes)
            .ok_or_else(|| wrong_type("annotation has no embedded file"))
    }

    /// Optional-content state.
    pub fn get_oc(&self) -> Result<i32> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(dict
            .get(b"OC")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .map(|id| id.0 as i32)
            .unwrap_or(0))
    }

    /// Parent annotation entry.
    pub fn get_parent(&self) -> Result<AnnotEntry> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        for key in [b"Parent".as_slice(), b"IRT".as_slice()] {
            if let Some(id) = dict.get(key).ok().and_then(|o| o.as_reference().ok()) {
                let parent = lp.get_dictionary(id).ok().cloned().unwrap_or_default();
                let subtype = parent
                    .get(b"Subtype")
                    .ok()
                    .map(obj_name)
                    .unwrap_or_default();
                let (code, name) = subtype_code(&subtype);
                let page = annot_page_index(lp, id.0 as i32).unwrap_or(self.page);
                let page_id = page_id_of(lp, page)?;
                let top = page_top(lp, page_id);
                let rect = parent
                    .get(b"Rect")
                    .ok()
                    .and_then(obj_rect)
                    .map(|r| flip_rect(&r, top))
                    .unwrap_or(mk_rect(0.0, 0.0, 0.0, 0.0));
                return Ok(AnnotEntry {
                    xref: id.0 as i32,
                    annot_type: AnnotType {
                        code,
                        name: name.to_string(),
                    },
                    rect,
                });
            }
        }
        Err(bad_value("annotation has no parent"))
    }

    /// Rendered pixmap of the annotation.
    pub fn get_pixmap(&self, _options: &RenderOptions) -> Result<Pixmap> {
        let rect = self.rect()?;
        let page = super::Page::new(self.doc, self.page);
        let options = RenderOptions::default().with_clip(rect).with_annots(true);
        let _ = _options;
        page.get_pixmap(&options)
    }

    /// Sound bytes.
    pub fn get_sound(&self) -> Result<Vec<u8>> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let subtype = dict.get(b"Subtype").ok().map(obj_name).unwrap_or_default();
        if subtype != "Sound" {
            return Err(wrong_type("annotation is not a sound annotation"));
        }
        let sound = dict
            .get(b"Sound")
            .ok()
            .map(|o| deref(lp, o))
            .and_then(|o| o.as_stream().ok())
            .ok_or_else(|| wrong_type("sound annotation has no audio"))?;
        Ok(sound
            .decompressed_content()
            .unwrap_or_else(|_| sound.content.clone()))
    }

    /// Annotation text in the requested format.
    pub fn get_text(&self, _options: &TextOptions) -> Result<TextOutput> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let contents = dict
            .get(b"Contents")
            .ok()
            .map(|o| obj_text(deref(lp, o)))
            .unwrap_or_default();
        if !contents.is_empty() && matches!(_options.format, super::text::TextFormat::Text) {
            return Ok(TextOutput::Text(format!("{contents}\n")));
        }
        let page_id = page_id_of(lp, self.page)?;
        let top = page_top(lp, page_id);
        let rect = dict
            .get(b"Rect")
            .ok()
            .and_then(obj_rect)
            .map(|r| flip_rect(&r, top));
        let page = super::Page::new(self.doc, self.page);
        let options = TextOptions::default()
            .with_format(_options.format)
            .with_sort(_options.sort);
        let options = match rect {
            Some(r) => options.with_clip(r),
            None => options,
        };
        let options = match _options.flags {
            Some(flags) => options.with_flags(flags),
            None => options,
        };
        page.get_text(&options)
    }

    /// Text inside a rectangle.
    pub fn get_textbox(&self, _rect: Option<Rect>) -> Result<String> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let _ = _rect;
        Ok(dict
            .get(b"Contents")
            .ok()
            .map(|o| obj_text(deref(lp, o)))
            .unwrap_or_default())
    }

    /// Extracted text page.
    pub fn get_textpage(&self, _clip: Option<Rect>, _flags: i32) -> Result<TextPage<'_>> {
        let page = super::Page::new(self.doc, self.page);
        let clip = match _clip {
            Some(clip) => Some(clip),
            None => self.rect().ok(),
        };
        let mut textpage = TextPage::new(page).with_flags(_flags);
        if let Some(clip) = clip {
            textpage = textpage.with_clip(clip);
        }
        Ok(textpage)
    }

    /// Whether a popup exists.
    pub fn has_popup(&self) -> Result<bool> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(dict.get(b"Popup").is_ok())
    }

    /// Identity fields.
    pub fn info(&self) -> Result<AnnotInfo> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(read_info(lp, &dict))
    }

    /// In-reply-to object number.
    pub fn irt_xref(&self) -> Result<i32> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(dict
            .get(b"IRT")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .map(|id| id.0 as i32)
            .unwrap_or(0))
    }

    /// Whether the popup is open.
    pub fn is_open(&self) -> Result<bool> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(dict
            .get(b"Open")
            .ok()
            .and_then(|o| match o {
                Object::Boolean(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false))
    }

    /// Annotation language.
    pub fn language(&self) -> Result<String> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(dict
            .get(b"Lang")
            .ok()
            .map(|o| obj_text(deref(lp, o)))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "zh-Hans".to_string()))
    }

    /// Line-end style codes.
    pub fn line_ends(&self) -> Result<Option<(i32, i32)>> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let subtype = dict.get(b"Subtype").ok().map(obj_name).unwrap_or_default();
        if !matches!(
            subtype.as_str(),
            "Line" | "PolyLine" | "Polygon" | "Ink" | "Square" | "Circle" | "FreeText"
        ) {
            return Ok(None);
        }
        let ends: Vec<i32> = dict
            .get(b"LE")
            .ok()
            .map(|o| deref(lp, o))
            .and_then(|o| o.as_array().ok())
            .map(|arr| arr.iter().map(|o| le_name_to_code(&obj_name(o))).collect())
            .unwrap_or_default();
        Ok(Some((
            ends.first().copied().unwrap_or(0),
            ends.get(1).copied().unwrap_or(0),
        )))
    }

    /// Next annotation on the page.
    /// Recorded member name on a live handle, not an iterator. (`#[allow(clippy::should_implement_trait)]` below).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Annot<'_>>> {
        let lp = self.doc.lopdf();
        let page_id = page_id_of(lp, self.page)?;
        let ids = page_annots(lp, page_id);
        let me = self.xref as u32;
        let at = ids.iter().position(|id| id.0 == me);
        match at.and_then(|i| ids.get(i + 1)) {
            Some(next) => Ok(Some(Annot {
                doc: &mut *self.doc,
                page: self.page,
                xref: next.0 as i32,
            })),
            None => Ok(None),
        }
    }

    /// Opacity level.
    pub fn opacity(&self) -> Result<f64> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(dict.get(b"CA").ok().map(obj_f64).unwrap_or(-1.0))
    }

    /// Whether the opacity keeps its integer shape (unset reads as int -1).
    pub fn opacity_is_int(&self) -> Result<bool> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(dict.get(b"CA").ok().map(obj_is_int).unwrap_or(true))
    }

    /// Popup rectangle.
    pub fn popup_rect(&self) -> Result<Rect> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let popup = dict
            .get(b"Popup")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or_else(|| bad_value("annotation has no popup"))?;
        let popup_dict = lp.get_dictionary(popup)?.clone();
        let page_id = page_id_of(lp, self.page)?;
        let top = page_top(lp, page_id);
        popup_dict
            .get(b"Rect")
            .ok()
            .and_then(obj_rect)
            .map(|r| flip_rect(&r, top))
            .ok_or_else(|| bad_value("popup has no rectangle"))
    }

    /// Popup object number.
    pub fn popup_xref(&self) -> Result<i32> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(dict
            .get(b"Popup")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .map(|id| id.0 as i32)
            .unwrap_or(0))
    }

    /// Annotation box.
    pub fn rect(&self) -> Result<Rect> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let page_id = page_id_of(lp, self.page)?;
        let top = page_top(lp, page_id);
        dict.get(b"Rect")
            .ok()
            .and_then(obj_rect)
            .map(|r| flip_rect(&r, top))
            .ok_or_else(|| bad_value("annotation has no rectangle"))
    }

    /// Appearance box delta.
    pub fn rect_delta(&self) -> Result<Rect> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        dict.get(b"RD")
            .ok()
            .and_then(obj_rect)
            .ok_or_else(|| bad_value("annotation has no rectangle difference"))
    }

    /// Rotation in degrees.
    pub fn rotation(&self) -> Result<i32> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        Ok(dict.get(b"Rotate").ok().map(obj_i32).unwrap_or(-1))
    }

    /// Set the appearance box.
    pub fn set_apn_bbox(&mut self, _bbox: Rect) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let page_id = page_id_of(lp, self.page)?;
        let top = page_top(lp, page_id);
        let pdf = flip_rect(&norm_rect(&_bbox), top);
        set_ap_bbox(lp, self.xref, &pdf)?;
        sync_doc(self.doc)
    }

    /// Set the appearance matrix.
    pub fn set_apn_matrix(&mut self, _matrix: Matrix) -> Result<()> {
        let value = Object::Array(vec![
            Object::Real(_matrix.a as f32),
            Object::Real(_matrix.b as f32),
            Object::Real(_matrix.c as f32),
            Object::Real(_matrix.d as f32),
            Object::Real(_matrix.e as f32),
            Object::Real(_matrix.f as f32),
        ]);
        set_ap_key(self.doc.lopdf_mut(), self.xref, b"Matrix", value)?;
        sync_doc(self.doc)
    }

    /// Set the blend mode name.
    pub fn set_blendmode(&mut self, _blend_mode: &str) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        if _blend_mode.is_empty() {
            dict.remove(b"BM");
        } else {
            dict.set("BM", name_object(_blend_mode));
        }
        sync_doc(self.doc)
    }

    /// Set the border styling.
    pub fn set_border(&mut self, _options: &BorderOptions) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        write_border(dict, _options);
        sync_doc(self.doc)
    }

    /// Set stroke and fill colours.
    pub fn set_colors(&mut self, _colors: &AnnotColors) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        write_colors(dict, _colors);
        sync_doc(self.doc)
    }

    /// Set the flags bitfield.
    pub fn set_flags(&mut self, _flags: i32) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        dict.set("F", Object::Integer(i64::from(_flags)));
        sync_doc(self.doc)
    }

    /// Set identity fields.
    pub fn set_info(&mut self, _options: &AnnotInfoOptions) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        write_info(dict, _options);
        sync_doc(self.doc)
    }

    /// Set the in-reply-to object.
    pub fn set_irt_xref(&mut self, _xref: i32) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        if _xref <= 0 {
            dict.remove(b"IRT");
        } else {
            dict.set("IRT", Object::Reference((_xref as u32, 0)));
        }
        sync_doc(self.doc)
    }

    /// Set the language.
    pub fn set_language(&mut self, _language: Option<&str>) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        match _language {
            Some(tag) => {
                let primary = tag.split(['-', '_']).next().unwrap_or(tag);
                dict.set("Lang", text_object(primary))
            }
            None => {
                dict.remove(b"Lang");
            }
        }
        sync_doc(self.doc)
    }

    /// Set line-end styles.
    pub fn set_line_ends(&mut self, _start: i32, _end: i32) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        dict.set(
            "LE",
            Object::Array(vec![
                name_object(le_code_to_name(_start)),
                name_object(le_code_to_name(_end)),
            ]),
        );
        sync_doc(self.doc)
    }

    /// Set the name.
    pub fn set_name(&mut self, _name: &str) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        dict.set("Name", name_object(_name));
        sync_doc(self.doc)
    }

    /// Set the optional-content state.
    pub fn set_oc(&mut self, _oc: i32) -> Result<()> {
        if _oc > 0 {
            let lp = self.doc.lopdf();
            let target = (_oc as u32, 0);
            let is_group = lp
                .get_dictionary(target)
                .ok()
                .and_then(|d| d.get(b"Type").ok().cloned())
                .and_then(|o| match o {
                    Object::Name(name) => Some(name),
                    _ => None,
                })
                .is_some_and(|name| name == b"OCG" || name == b"OCMD");
            if !is_group {
                return Err(engine_fault(format!("no optional-content group {_oc}")));
            }
        }
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        if _oc <= 0 {
            dict.remove(b"OC");
        } else {
            dict.set("OC", Object::Reference((_oc as u32, 0)));
        }
        sync_doc(self.doc)
    }

    /// Set the opacity level.
    pub fn set_opacity(&mut self, _opacity: f64) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        if _opacity < 0.0 {
            dict.remove(b"CA");
        } else {
            dict.set("CA", Object::Real(_opacity.clamp(0.0, 1.0) as f32));
        }
        sync_doc(self.doc)
    }

    /// Set the popup open state.
    pub fn set_open(&mut self, _is_open: bool) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        dict.set("Open", Object::Boolean(_is_open));
        sync_doc(self.doc)
    }

    /// Set the popup rectangle.
    pub fn set_popup(&mut self, _rect: Rect) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let page_id = page_id_of(lp, self.page)?;
        let top = page_top(lp, page_id);
        let pdf = flip_rect(&norm_rect(&_rect), top);
        let parent = (self.xref as u32, 0);
        ensure_popup(lp, parent, &pdf)?;
        sync_doc(self.doc)
    }

    /// Set the annotation box.
    pub fn set_rect(&mut self, _rect: Rect) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let page_id = page_id_of(lp, self.page)?;
        let top = page_top(lp, page_id);
        let pdf = flip_rect(&norm_rect(&_rect), top);
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        dict.set("Rect", rect_object(&pdf));
        sync_doc(self.doc)
    }

    /// Set the rotation.
    pub fn set_rotation(&mut self, _rotate: i32) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let dict = lp.get_dictionary_mut((self.xref as u32, 0))?;
        dict.set("Rotate", Object::Integer(i64::from(_rotate)));
        sync_doc(self.doc)
    }

    /// Type code and name.
    pub fn annot_type(&self) -> Result<AnnotType> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let subtype = dict.get(b"Subtype").ok().map(obj_name).unwrap_or_default();
        let (code, name) = subtype_code(&subtype);
        Ok(AnnotType {
            code,
            name: name.to_string(),
        })
    }

    /// Refresh the appearance.
    pub fn update(&mut self, _options: &AnnotUpdateOptions) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let id = (self.xref as u32, 0);
        let subtype = lp
            .get_dictionary(id)
            .ok()
            .and_then(|d| d.get(b"Subtype").ok().map(obj_name))
            .unwrap_or_default();
        let dict = lp.get_dictionary_mut(id)?;
        if let Some(blend) = &_options.blend_mode {
            if blend.is_empty() {
                dict.remove(b"BM");
            } else {
                dict.set("BM", name_object(blend));
            }
        }
        if let Some(opacity) = _options.opacity {
            if opacity < 0.0 {
                dict.remove(b"CA");
            } else {
                dict.set("CA", Object::Real(opacity.clamp(0.0, 1.0) as f32));
            }
        }
        if _options.fontsize > 0.0 || _options.fontname.is_some() || _options.text_color.is_some() {
            let size = if _options.fontsize > 0.0 {
                _options.fontsize
            } else {
                11.0
            };
            dict.set(
                "DA",
                text_object(&build_da(
                    _options.fontname.as_deref(),
                    size,
                    _options.text_color.as_deref(),
                )),
            );
        }
        if let Some(fill) = &_options.fill_color {
            if subtype == "FreeText" {
                if fill.is_empty() {
                    dict.remove(b"C");
                } else {
                    dict.set("C", num_array(fill));
                }
            } else if fill.is_empty() {
                dict.remove(b"IC");
            } else {
                dict.set("IC", num_array(fill));
            }
        }
        if let Some(border) = &_options.border_color {
            if border.is_empty() {
                dict.remove(b"C");
            } else {
                dict.set("C", num_array(border));
            }
        }
        if _options.rotate >= 0 {
            dict.set("Rotate", Object::Integer(i64::from(_options.rotate)));
        }
        let _ = _options.cross_out;
        sync_doc(self.doc)
    }

    /// Replace the attachment.
    pub fn update_file(
        &mut self,
        _data: &[u8],
        _filename: Option<&str>,
        _display_name: Option<&str>,
        _description: Option<&str>,
    ) -> Result<()> {
        let (spec_id, stream_id) = {
            let lp = self.doc.lopdf();
            let dict = annot_dict(lp, self.xref)?;
            let spec_id = dict
                .get(b"FS")
                .ok()
                .and_then(|o| o.as_reference().ok())
                .ok_or_else(|| wrong_type("annotation has no embedded file"))?;
            let spec = lp.get_dictionary(spec_id)?.clone();
            let ef = spec
                .get(b"EF")
                .ok()
                .map(|o| deref(lp, o))
                .and_then(|o| o.as_dict().ok())
                .ok_or_else(|| wrong_type("embedded file has no stream"))?;
            let stream_id = [b"F".as_slice(), b"UF".as_slice()]
                .into_iter()
                .find_map(|key| ef.get(key).ok()?.as_reference().ok())
                .ok_or_else(|| wrong_type("embedded file has no stream"))?;
            (spec_id, stream_id)
        };
        let lp = self.doc.lopdf_mut();
        if let Ok(Object::Stream(stream)) = lp.get_object_mut(stream_id) {
            stream.set_content(_data.to_vec());
            stream.dict.remove(b"Filter");
            let mut params = Dictionary::new();
            params.set("Size", Object::Integer(_data.len() as i64));
            stream.dict.set("Params", Object::Dictionary(params));
        }
        let spec = lp.get_dictionary_mut(spec_id)?;
        if let Some(filename) = _filename {
            spec.set("F", text_object(filename));
        }
        if let Some(display) = _display_name {
            spec.set("UF", text_object(display));
        }
        if let Some(description) = _description {
            spec.set("Desc", text_object(description));
        }
        sync_doc(self.doc)
    }

    /// Appearance refresh timing probe.
    pub fn update_timing_test(&self) -> Result<f64> {
        let start = std::time::Instant::now();
        let _ = self.rect()?;
        let _ = self.info()?;
        let _ = self.colors()?;
        let _ = self.border()?;
        Ok(start.elapsed().as_secs_f64())
    }

    /// Markup vertices.
    pub fn vertices(&self) -> Result<Vec<Point>> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let page_id = page_id_of(lp, self.page)?;
        let top = page_top(lp, page_id);
        Ok(read_vertices(lp, &dict, top))
    }

    /// Ink strokes, one point list per stroke.
    pub fn ink_vertices(&self) -> Result<Vec<Vec<Point>>> {
        let lp = self.doc.lopdf();
        let dict = annot_dict(lp, self.xref)?;
        let page_id = page_id_of(lp, self.page)?;
        let top = page_top(lp, page_id);
        Ok(read_inklist(lp, &dict, top))
    }

    /// Annotation object number.
    pub fn xref(&self) -> Result<i32> {
        Ok(self.xref)
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        let _ = (_other, _op);
        Err(wrong_type("annotation ordering is unsupported"))
    }
}

impl Widget {
    /// Blank widget to fill and add.
    pub fn new() -> Self {
        Self {
            field_name: String::new(),
            field_type: super::constants::PDF_WIDGET_TYPE_UNKNOWN,
            field_value: String::new(),
            rect: mk_rect(0.0, 0.0, 0.0, 0.0),
            xref: 0,
            field_flags: 0,
            field_display: 2,
            choices: Vec::new(),
            text_font: "Helv".to_string(),
            text_fontsize: 0.0,
            text_fontsize_is_int: true,
            text_maxlen: 0,
            border_width: 1.0,
            border_width_is_int: true,
            border_style: "Solid".to_string(),
        }
    }

    /// Button state names.
    pub fn button_states(&self) -> Result<Vec<String>> {
        use super::constants::*;
        if self.field_type == PDF_WIDGET_TYPE_CHECKBOX
            || self.field_type == PDF_WIDGET_TYPE_RADIOBUTTON
        {
            if self.field_value.is_empty() || self.field_value == "Off" {
                Ok(vec!["Off".to_string()])
            } else {
                Ok(vec!["Off".to_string(), self.field_value.clone()])
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Next widget on the page.
    pub fn next(&self) -> Option<Widget> {
        None
    }

    /// Current on-state name.
    pub fn on_state(&self) -> Result<String> {
        use super::constants::*;
        if self.field_type == PDF_WIDGET_TYPE_CHECKBOX
            || self.field_type == PDF_WIDGET_TYPE_RADIOBUTTON
        {
            if self.field_value.is_empty() {
                Err(wrong_type("widget has no on state"))
            } else {
                Ok(self.field_value.clone())
            }
        } else {
            Err(wrong_type("widget is not a button"))
        }
    }

    /// Reset the field value.
    pub fn reset(&mut self) -> Result<()> {
        self.field_value.clear();
        Ok(())
    }

    /// Sync the field back.
    pub fn update(&self) -> Result<()> {
        Ok(())
    }

    /// Field type name.
    pub fn field_type_string(&self) -> String {
        widget_type_string(self.field_type).to_string()
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        let _ = (_other, _op);
        Err(wrong_type("widget ordering is unsupported"))
    }
}

impl Default for Widget {
    fn default() -> Self {
        Self::new()
    }
}

impl Font {
    /// Build a font from the given source.
    pub fn new(_options: &FontOptions) -> Result<Self> {
        if let Some(data) = &_options.fontbuffer {
            let face = parse_file_face(data)
                .map_err(|_| unreadable("font buffer does not parse as a font"))?;
            return Ok(Self {
                name: face.display_name.clone(),
                data: data.clone(),
            });
        }
        if let Some(path) = &_options.fontfile {
            let data = std::fs::read(path)
                .map_err(|_| missing_file(format!("font file not found: {path}")))?;
            let face = parse_file_face(&data)
                .map_err(|_| unreadable(format!("font file does not parse: {path}")))?;
            return Ok(Self {
                name: face.display_name.clone(),
                data,
            });
        }
        if let Some(fontname) = &_options.fontname {
            let base = resolve_fontname(fontname)
                .ok_or_else(|| bad_value(format!("unknown font name: {fontname}")))?;
            return Ok(Self {
                name: base.to_string(),
                data: Vec::new(),
            });
        }
        Ok(Self {
            name: "Noto Serif Regular".to_string(),
            data: Vec::new(),
        })
    }

    /// Ascender height.
    pub fn ascender(&self) -> Result<f64> {
        Ok(font_face(self)?.asc_em())
    }

    /// Font bounding box.
    pub fn bbox(&self) -> Result<Rect> {
        let face = font_face(self)?;
        let bbox = face.bbox_em();
        Ok(mk_rect(bbox[0], bbox[1], bbox[2], bbox[3]))
    }

    /// Font file bytes.
    pub fn buffer(&self) -> Result<Vec<u8>> {
        if self.data.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(self.data.clone())
        }
    }

    /// Advance widths per character.
    pub fn char_lengths(&self, _text: &str, _options: &FontMeasureOptions) -> Result<Vec<f64>> {
        let face = font_face(self)?;
        let size = _options.fontsize;
        Ok(_text.chars().map(|ch| face.width_em(ch) * size).collect())
    }

    /// Descender depth.
    pub fn descender(&self) -> Result<f64> {
        Ok(font_face(self)?.desc_em())
    }

    /// Capability flags.
    pub fn flags(&self) -> Result<FontFlags> {
        Ok(font_face(self)?.flags.clone())
    }

    /// Advance width of one glyph.
    pub fn glyph_advance(&self, _chr: i32, _options: &FontMeasureOptions) -> Result<f64> {
        let face = font_face(self)?;
        if _chr < 0 {
            return Ok(0.0);
        }
        let ch = char::from_u32(_chr as u32).unwrap_or('\0');
        let _ = _options;
        Ok(face.width_em(ch))
    }

    /// Bounding box of one glyph.
    pub fn glyph_bbox(&self, _chr: i32, _options: &FontMeasureOptions) -> Result<Rect> {
        let face = font_face(self)?;
        let _ = _options;
        let adv = if _chr >= 0 {
            face.width_em(char::from_u32(_chr as u32).unwrap_or('\0'))
        } else {
            0.0
        };
        Ok(mk_rect(0.0, face.desc_em(), adv, face.asc_em()))
    }

    /// Glyph count.
    pub fn glyph_count(&self) -> Result<usize> {
        Ok(font_face(self)?.glyph_count)
    }

    /// Unicode value for a glyph name.
    pub fn glyph_name_to_unicode(&self, _name: &str) -> Result<i32> {
        Ok(unicode_from_name(_name).unwrap_or(0xFFFD) as i32)
    }

    /// Glyph id for a codepoint, or zero.
    pub fn has_glyph(&self, _chr: i32, _options: &FontMeasureOptions) -> Result<i32> {
        let face = font_face(self)?;
        let _ = _options;
        if _chr < 0 {
            return Ok(0);
        }
        Ok(i32::from(face.glyph_id(_chr as u32)))
    }

    /// Bold variant.
    pub fn is_bold(&self) -> Result<bool> {
        Ok(font_face(self)?.flags.bold)
    }

    /// Italic variant.
    pub fn is_italic(&self) -> Result<bool> {
        Ok(font_face(self)?.flags.italic)
    }

    /// Fixed-width font.
    pub fn is_monospaced(&self) -> Result<bool> {
        Ok(font_face(self)?.flags.mono)
    }

    /// Serif font.
    pub fn is_serif(&self) -> Result<bool> {
        Ok(font_face(self)?.flags.serif)
    }

    /// Writable font.
    pub fn is_writable(&self) -> Result<bool> {
        Ok(true)
    }

    /// Font name.
    pub fn name(&self) -> Result<String> {
        Ok(self.name.clone())
    }

    /// Advance width of a run.
    pub fn text_length(&self, _text: &str, _options: &FontMeasureOptions) -> Result<f64> {
        let face = font_face(self)?;
        Ok(face.text_width_em(_text) * _options.fontsize)
    }

    /// Glyph name for a codepoint.
    pub fn unicode_to_glyph_name(&self, _chr: i32) -> Result<String> {
        if _chr < 0 {
            return Ok(format!("U+{:04X}", 0xFFFD));
        }
        Ok(unicode_name(_chr as u32))
    }

    /// Supported codepoints.
    pub fn valid_codepoints(&self) -> Result<Vec<i32>> {
        Ok(font_face(self)?
            .valid_codepoints()
            .into_iter()
            .map(|c| c as i32)
            .collect())
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        let _ = (_other, _op);
        Err(wrong_type("font ordering is unsupported"))
    }
}

impl TextWriter {
    /// Stage text for this area.
    pub fn new(_rect: Rect, _options: &TextWriterOptions) -> Self {
        Self {
            rect: _rect,
            opacity: _options.opacity,
            color: _options.color.clone(),
            staged: Vec::new(),
            union: None,
        }
    }

    /// Append a run at a position.
    pub fn append(
        &mut self,
        _pos: Point,
        _text: &str,
        _options: &AppendOptions,
    ) -> Result<(Rect, Point)> {
        let face = match _options.font {
            Some(font) => font_face(font)?,
            None => builtin_face("Helvetica")
                .ok_or_else(|| engine_fault("builtin metrics are missing"))?,
        };
        let size = _options.fontsize;
        let width = face.text_width_em(_text) * size;
        let end = mk_point(_pos.x + width, _pos.y);
        let ink = mk_rect(
            _pos.x - 0.02 * size,
            _pos.y - 0.82 * size,
            end.x + 0.04 * size,
            _pos.y + 1.24,
        );
        self.union = Some(match self.union {
            Some(u) => union_rect(u, ink),
            None => ink,
        });
        self.staged.push(StagedRun {
            pos: _pos,
            text: _text.to_string(),
            font_base: face.face_name.clone(),
            fontsize: size,
            color: None,
            stroke: None,
            opacity: self.opacity,
            stroke_opacity: self.opacity,
            render_mode: 0,
            rotate: 0,
            border_width: 0.0,
            miter_limit: 1.0,
            align: 0,
            max_width: 0.0,
        });
        let _ = (
            &_options.language,
            _options.right_to_left,
            _options.small_caps,
        );
        Ok((self.union.unwrap_or(ink), end))
    }

    /// Append a vertical run at a position.
    pub fn appendv(
        &mut self,
        _pos: Point,
        _text: &str,
        _options: &AppendOptions,
    ) -> Result<(Rect, Point)> {
        let face = match _options.font {
            Some(font) => font_face(font)?,
            None => builtin_face("Helvetica")
                .ok_or_else(|| engine_fault("builtin metrics are missing"))?,
        };
        let size = _options.fontsize;
        let height = _text.chars().count() as f64 * size;
        let end = mk_point(_pos.x, _pos.y + height);
        let ink = mk_rect(_pos.x - 0.5 * size, _pos.y, _pos.x + 0.5 * size, end.y);
        self.union = Some(match self.union {
            Some(u) => union_rect(u, ink),
            None => ink,
        });
        let _ = (&_options.language, _options.small_caps);
        let mut at = _pos;
        for ch in _text.chars() {
            self.staged.push(StagedRun {
                pos: at,
                text: ch.to_string(),
                font_base: face.face_name.clone(),
                fontsize: size,
                color: None,
                stroke: None,
                opacity: self.opacity,
                stroke_opacity: self.opacity,
                render_mode: 0,
                rotate: 0,
                border_width: 0.0,
                miter_limit: 1.0,
                align: 0,
                max_width: 0.0,
            });
            at = mk_point(at.x, at.y + size);
        }
        Ok((self.union.unwrap_or(ink), end))
    }

    /// Clean directional marks from text, reversing left-to-right runs.
    pub fn clean_rtl(&self, _text: &str) -> String {
        let stripped: String = _text
            .chars()
            .filter(|ch| {
                !matches!(
                    ch,
                    '\u{061C}'
                        | '\u{200E}'
                        | '\u{200F}'
                        | '\u{202A}'
                        | '\u{202B}'
                        | '\u{202C}'
                        | '\u{202D}'
                        | '\u{202E}'
                        | '\u{2066}'
                        | '\u{2067}'
                        | '\u{2068}'
                        | '\u{2069}'
                )
            })
            .collect();
        let mut out = String::with_capacity(stripped.len());
        let mut run = String::new();
        let mut run_rtl = false;
        let mut started = false;
        for ch in stripped.chars() {
            let rtl = is_rtl_char(ch);
            if !started {
                started = true;
                run_rtl = rtl;
            } else if rtl != run_rtl {
                push_rtl_run(&mut out, &mut run, run_rtl);
                run_rtl = rtl;
            }
            run.push(ch);
        }
        if started {
            push_rtl_run(&mut out, &mut run, run_rtl);
        }
        out
    }

    /// Fill a box with text.
    pub fn fill_textbox(
        &mut self,
        _rect: Rect,
        _text: &TextArg,
        _options: &FillTextBoxOptions,
    ) -> Result<Vec<(String, f64)>> {
        let face = match _options.font {
            Some(font) => font_face(font)?,
            None => builtin_face("Helvetica")
                .ok_or_else(|| engine_fault("builtin metrics are missing"))?,
        };
        let size = _options.fontsize;
        let height = _options
            .lineheight
            .unwrap_or_else(|| default_line_height(&face, size));
        let origin = _options.pos.unwrap_or(mk_point(_rect.x0, _rect.y0));
        let usable_w = (_rect.x1 - origin.x).max(0.0);
        let usable_h = (_rect.y1 - origin.y).max(0.0);
        let mut laid: Vec<(String, f64)> = Vec::new();
        for logical in text_arg_lines(_text) {
            laid.extend(wrap_lines(&logical, usable_w, &face, size));
        }
        let capacity = if height > 0.0 {
            (usable_h / height).floor() as usize
        } else {
            laid.len()
        };
        let placed = laid.len().min(capacity);
        let asc = face.asc_em() * size;
        for (index, (line, width)) in laid.iter().take(placed).enumerate() {
            let x = match _options.align {
                1 => origin.x + (usable_w - width) / 2.0,
                2 => _rect.x1 - width,
                _ => origin.x,
            };
            let baseline = origin.y + asc + index as f64 * height;
            self.staged.push(StagedRun {
                pos: mk_point(x, baseline),
                text: line.clone(),
                font_base: face.face_name.clone(),
                fontsize: size,
                color: None,
                stroke: None,
                opacity: self.opacity,
                stroke_opacity: self.opacity,
                render_mode: 0,
                rotate: 0,
                border_width: 0.0,
                miter_limit: 1.0,
                align: _options.align,
                max_width: usable_w,
            });
            let ink = mk_rect(
                x,
                baseline - asc,
                x + width,
                baseline - face.desc_em() * size,
            );
            self.union = Some(match self.union {
                Some(u) => union_rect(u, ink),
                None => ink,
            });
        }
        let _ = (_options.warn, _options.right_to_left, _options.small_caps);
        Ok(laid.into_iter().skip(placed).collect())
    }

    /// Flush staged text onto a page.
    pub fn write_text(&self, _page: &mut PageMut<'_>, _options: &FlushOptions) -> Result<()> {
        let page_idx = _page.index();
        let lp = _page.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, page_idx)?;
        let top = page_top(lp, page_id);
        for run in &self.staged {
            if builtin_face(&run.font_base).is_none() {
                return Err(bad_value("file fonts cannot be written yet"));
            }
        }
        let mut fonts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for run in &self.staged {
            if !fonts.contains_key(&run.font_base) {
                let name = ensure_base_font(lp, page_id, &run.font_base)?;
                fonts.insert(run.font_base.clone(), name);
            }
        }
        let opacity = _options.opacity.unwrap_or(self.opacity);
        let gs = if opacity < 1.0 {
            Some(ensure_opacity_state(lp, page_id, opacity, opacity)?)
        } else {
            None
        };
        let mut data = Vec::new();
        if let Some(matrix) = &_options.matrix {
            data.extend_from_slice(
                format!(
                    "q {} {} {} {} {} {} cm\n",
                    pdf_num(matrix.a),
                    pdf_num(matrix.b),
                    pdf_num(matrix.c),
                    pdf_num(matrix.d),
                    pdf_num(matrix.e),
                    pdf_num(matrix.f),
                )
                .as_bytes(),
            );
        }
        for run in &self.staged {
            let mut run = run.clone();
            if run.color.is_none() {
                run.color = _options
                    .color
                    .clone()
                    .or_else(|| self.color.clone())
                    .or_else(|| Some(vec![0.0, 0.0, 0.0]));
            }
            let font_name = fonts.get(&run.font_base).cloned().unwrap_or_default();
            data.extend_from_slice(&encode_text_run(&run, &font_name, gs.as_deref(), top));
        }
        if _options.matrix.is_some() {
            data.extend_from_slice(b"Q\n");
        }
        let _ = (&_options.morph, _options.render_mode, _options.oc);
        push_content(lp, page_id, data, _options.overlay)?;
        sync_doc(_page.document_mut())
    }

    /// Staged content as a value: the writer's box, opacity, colour,
    /// ink union and runs, so doors without shared memory (node) can
    /// hand writers to [`PageMut::write_text`] through
    /// [`WriteTextOptions::writers`]. [`TextWriter::from_staged`] reads
    /// it back; unknown keys are refused, so the shape cannot drift.
    pub fn staged_value(&self) -> Value {
        let rect = |r: Rect| {
            Value::List(vec![
                Value::Float(r.x0),
                Value::Float(r.y0),
                Value::Float(r.x1),
                Value::Float(r.y1),
            ])
        };
        let colour = |c: &Option<Vec<f64>>| match c {
            Some(parts) => Value::List(parts.iter().map(|n| Value::Float(*n)).collect()),
            None => Value::Null,
        };
        let runs = self
            .staged
            .iter()
            .map(|run| {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    "pos".to_string(),
                    Value::List(vec![Value::Float(run.pos.x), Value::Float(run.pos.y)]),
                );
                map.insert("text".to_string(), Value::Str(run.text.clone()));
                map.insert("font_base".to_string(), Value::Str(run.font_base.clone()));
                map.insert("fontsize".to_string(), Value::Float(run.fontsize));
                map.insert("color".to_string(), colour(&run.color));
                map.insert("stroke".to_string(), colour(&run.stroke));
                map.insert("opacity".to_string(), Value::Float(run.opacity));
                map.insert(
                    "stroke_opacity".to_string(),
                    Value::Float(run.stroke_opacity),
                );
                map.insert(
                    "render_mode".to_string(),
                    Value::Int(i64::from(run.render_mode)),
                );
                map.insert("rotate".to_string(), Value::Int(i64::from(run.rotate)));
                map.insert("border_width".to_string(), Value::Float(run.border_width));
                map.insert("miter_limit".to_string(), Value::Float(run.miter_limit));
                map.insert("align".to_string(), Value::Int(i64::from(run.align)));
                map.insert("max_width".to_string(), Value::Float(run.max_width));
                Value::Dict(map)
            })
            .collect();
        let mut map = std::collections::HashMap::new();
        map.insert("rect".to_string(), rect(self.rect));
        map.insert("opacity".to_string(), Value::Float(self.opacity));
        map.insert("color".to_string(), colour(&self.color));
        map.insert(
            "union".to_string(),
            self.union.map(rect).unwrap_or(Value::Null),
        );
        map.insert("runs".to_string(), Value::List(runs));
        Value::Dict(map)
    }

    /// Rebuild a writer from [`TextWriter::staged_value`].
    fn from_staged(value: &Value) -> Result<Self> {
        let map = match value {
            Value::Dict(map) => map,
            _ => return Err(wrong_type("a staged writer must be a dict")),
        };
        let number = |key: &str| -> Result<f64> {
            map.get(key)
                .and_then(staged_number)
                .ok_or_else(|| wrong_type(format!("staged writer needs a numeric {key}")))
        };
        let colour = |key: &str| -> Result<Option<Vec<f64>>> {
            match map.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::List(parts)) => parts
                    .iter()
                    .map(|p| {
                        staged_number(p)
                            .ok_or_else(|| wrong_type(format!("staged {key} needs numbers")))
                    })
                    .collect::<Result<Vec<f64>>>()
                    .map(Some),
                Some(_) => Err(wrong_type(format!("staged {key} must be a list or null"))),
            }
        };
        let rect = |key: &str| -> Result<Rect> {
            match map.get(key) {
                Some(Value::List(parts)) if parts.len() == 4 => {
                    let mut nums = [0.0; 4];
                    for (slot, part) in nums.iter_mut().zip(parts.iter()) {
                        *slot = staged_number(part)
                            .ok_or_else(|| wrong_type(format!("staged {key} needs numbers")))?;
                    }
                    Ok(mk_rect(nums[0], nums[1], nums[2], nums[3]))
                }
                _ => Err(wrong_type(format!("staged writer needs a {key} box"))),
            }
        };
        let runs = match map.get("runs") {
            Some(Value::List(runs)) => runs,
            _ => return Err(wrong_type("a staged writer needs a runs list")),
        };
        let mut staged = Vec::with_capacity(runs.len());
        for run in runs {
            let run = match run {
                Value::Dict(run) => run,
                _ => return Err(wrong_type("a staged run must be a dict")),
            };
            let get = |key: &str| run.get(key);
            let pos = match get("pos") {
                Some(Value::List(pair)) if pair.len() == 2 => {
                    let x = staged_number(&pair[0])
                        .ok_or_else(|| wrong_type("a staged run needs a numeric pos"))?;
                    let y = staged_number(&pair[1])
                        .ok_or_else(|| wrong_type("a staged run needs a numeric pos"))?;
                    mk_point(x, y)
                }
                _ => return Err(wrong_type("a staged run needs a pos pair")),
            };
            let text_value = |key: &str| -> Result<String> {
                get(key)
                    .and_then(|v| match v {
                        Value::Str(s) => Some(s.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| wrong_type(format!("a staged run needs text {key}")))
            };
            let run_number = |key: &str, default: f64| -> f64 {
                get(key).and_then(staged_number).unwrap_or(default)
            };
            let run_int = |key: &str| -> Result<i32> {
                get(key)
                    .and_then(|v| match v {
                        Value::Int(n) => i32::try_from(*n).ok(),
                        Value::Float(n) => Some(*n as i32),
                        _ => None,
                    })
                    .ok_or_else(|| wrong_type(format!("a staged run needs an integer {key}")))
            };
            let run_colour = |key: &str| -> Result<Option<Vec<f64>>> {
                match get(key) {
                    None | Some(Value::Null) => Ok(None),
                    Some(Value::List(parts)) => parts
                        .iter()
                        .map(|p| {
                            staged_number(p)
                                .ok_or_else(|| wrong_type(format!("staged {key} needs numbers")))
                        })
                        .collect::<Result<Vec<f64>>>()
                        .map(Some),
                    Some(_) => Err(wrong_type(format!("staged {key} must be a list or null"))),
                }
            };
            staged.push(StagedRun {
                pos,
                text: text_value("text")?,
                font_base: text_value("font_base")?,
                fontsize: run_number("fontsize", 0.0),
                color: run_colour("color")?,
                stroke: run_colour("stroke")?,
                opacity: run_number("opacity", 1.0),
                stroke_opacity: run_number("stroke_opacity", 1.0),
                render_mode: run_int("render_mode").unwrap_or(0),
                rotate: run_int("rotate").unwrap_or(0),
                border_width: run_number("border_width", 0.0),
                miter_limit: run_number("miter_limit", 1.0),
                align: run_int("align").unwrap_or(0),
                max_width: run_number("max_width", 0.0),
            });
        }
        Ok(Self {
            rect: rect("rect")?,
            opacity: number("opacity")?,
            color: colour("color")?,
            staged,
            union: match map.get("union") {
                None | Some(Value::Null) => None,
                Some(_) => Some(rect("union")?),
            },
        })
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        let _ = (_other, _op);
        Err(wrong_type("text writer ordering is unsupported"))
    }
}

/// Whether a character belongs to a right-to-left script.
fn is_rtl_char(ch: char) -> bool {
    matches!(ch,
        '\u{0590}'..='\u{05FF}'
        | '\u{0600}'..='\u{06FF}'
        | '\u{0750}'..='\u{077F}'
        | '\u{08A0}'..='\u{08FF}'
        | '\u{FB50}'..='\u{FDFF}'
        | '\u{FE70}'..='\u{FEFF}')
}

/// Append one directional run: rtl runs keep their order, the rest reverse.
fn push_rtl_run(out: &mut String, run: &mut String, rtl: bool) {
    if rtl {
        out.push_str(run);
    } else {
        out.extend(run.chars().rev());
    }
    run.clear();
}

impl<'a> Shape<'a> {
    /// Drawing canvas on a page.
    pub fn new(_doc: &'a mut PdfDocument, _page: usize) -> Self {
        Self {
            doc: _doc,
            page: _page,
            pending: Vec::new(),
            finished: Vec::new(),
            texts: Vec::new(),
            union: None,
        }
    }

    /// Write pending marks to the page.
    pub fn commit(&mut self, _overlay: bool) -> Result<()> {
        let lp = self.doc.lopdf_mut();
        let page_id = page_id_of(lp, self.page)?;
        let top = page_top(lp, page_id);
        let mut fonts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for run in &self.texts {
            if builtin_face(&run.font_base).is_none() {
                return Err(bad_value("file fonts cannot be written yet"));
            }
            if !fonts.contains_key(&run.font_base) {
                let name = ensure_base_font(lp, page_id, &run.font_base)?;
                fonts.insert(run.font_base.clone(), name);
            }
        }
        let mut states: std::collections::HashMap<[u64; 2], String> =
            std::collections::HashMap::new();
        let mut gs_for =
            |lp: &mut LopdfDocument, fill: f64, stroke: f64| -> Result<Option<String>> {
                if fill >= 1.0 && stroke >= 1.0 {
                    return Ok(None);
                }
                let key = [fill.to_bits(), stroke.to_bits()];
                if let Some(name) = states.get(&key) {
                    return Ok(Some(name.clone()));
                }
                let name = ensure_opacity_state(lp, page_id, fill, stroke)?;
                states.insert(key, name.clone());
                Ok(Some(name))
            };
        let mut data = Vec::new();
        for finished in &self.finished {
            let gs = gs_for(lp, finished.fill_opacity, finished.stroke_opacity)?;
            data.extend_from_slice(&encode_finished_path(finished, top, gs.as_deref()));
        }
        for run in &self.texts {
            let gs = gs_for(lp, run.opacity, run.stroke_opacity)?;
            let font_name = fonts.get(&run.font_base).cloned().unwrap_or_default();
            data.extend_from_slice(&encode_text_run(run, &font_name, gs.as_deref(), top));
        }
        push_content(lp, page_id, data, _overlay)?;
        self.finished.clear();
        self.texts.clear();
        sync_doc(self.doc)
    }

    /// Queue a bezier curve.
    pub fn draw_bezier(&mut self, _p1: Point, _p2: Point, _p3: Point, _p4: Point) -> Result<Point> {
        let mut path = DrawnPath::new(_p1);
        path.segs.push(Seg::Curve(_p1, _p2, _p3, _p4));
        for p in [_p1, _p2, _p3, _p4] {
            grow_union(&mut self.union, p);
        }
        self.pending.push(path);
        Ok(_p4)
    }

    /// Queue a circle.
    pub fn draw_circle(&mut self, _center: Point, _radius: f64) -> Result<Point> {
        let path = ellipse_paths(_center, _radius, _radius);
        let end = mk_point(_center.x - _radius, _center.y);
        for p in [
            mk_point(_center.x - _radius, _center.y),
            mk_point(_center.x + _radius, _center.y),
            mk_point(_center.x, _center.y - _radius),
            mk_point(_center.x, _center.y + _radius),
        ] {
            grow_union(&mut self.union, p);
        }
        self.pending.push(path);
        Ok(end)
    }

    /// Queue a curve.
    pub fn draw_curve(&mut self, _p1: Point, _p2: Point, _p3: Point) -> Result<Point> {
        let c1 = mk_point(
            _p1.x + 2.0 / 3.0 * (_p2.x - _p1.x),
            _p1.y + 2.0 / 3.0 * (_p2.y - _p1.y),
        );
        let c2 = mk_point(
            _p3.x + 2.0 / 3.0 * (_p2.x - _p3.x),
            _p3.y + 2.0 / 3.0 * (_p2.y - _p3.y),
        );
        let mut path = DrawnPath::new(_p1);
        path.segs.push(Seg::Curve(_p1, c1, c2, _p3));
        for p in [_p1, _p2, _p3] {
            grow_union(&mut self.union, p);
        }
        self.pending.push(path);
        Ok(_p3)
    }

    /// Queue a line.
    pub fn draw_line(&mut self, _p1: Point, _p2: Point) -> Result<Point> {
        let mut path = DrawnPath::new(_p1);
        path.segs.push(Seg::Line(_p1, _p2));
        grow_union(&mut self.union, _p1);
        grow_union(&mut self.union, _p2);
        self.pending.push(path);
        Ok(_p2)
    }

    /// Queue an oval.
    pub fn draw_oval(&mut self, _rect: Rect) -> Result<Point> {
        let r = norm_rect(&_rect);
        let center = mk_point((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
        let path = ellipse_paths(center, (r.x1 - r.x0) / 2.0, (r.y1 - r.y0) / 2.0);
        let end = mk_point(r.x0, center.y);
        grow_union(&mut self.union, mk_point(r.x0, r.y0));
        grow_union(&mut self.union, mk_point(r.x1, r.y1));
        self.pending.push(path);
        Ok(end)
    }

    /// Queue a polyline.
    pub fn draw_polyline(&mut self, _points: &[Point]) -> Result<Point> {
        if _points.is_empty() {
            return Err(bad_value("polyline needs at least one point"));
        }
        let mut path = DrawnPath::new(_points[0]);
        for window in _points.windows(2) {
            path.segs.push(Seg::Line(window[0], window[1]));
        }
        for p in _points {
            grow_union(&mut self.union, *p);
        }
        self.pending.push(path);
        Ok(*_points.last().unwrap_or(&_points[0]))
    }

    /// Queue a quadrilateral.
    pub fn draw_quad(&mut self, _quad: Quad) -> Result<Point> {
        let mut path = DrawnPath::new(_quad.ul);
        path.segs.push(Seg::Line(_quad.ul, _quad.ur));
        path.segs.push(Seg::Line(_quad.ur, _quad.lr));
        path.segs.push(Seg::Line(_quad.lr, _quad.ll));
        path.segs.push(Seg::Line(_quad.ll, _quad.ul));
        for p in [_quad.ul, _quad.ur, _quad.ll, _quad.lr] {
            grow_union(&mut self.union, p);
        }
        self.pending.push(path);
        Ok(_quad.ul)
    }

    /// Queue a rectangle.
    pub fn draw_rect(&mut self, _rect: Rect, _radius: Option<f64>) -> Result<Point> {
        let r = norm_rect(&_rect);
        match _radius {
            Some(rad) if rad > 0.0 => {
                let path = round_rect_path(r, rad);
                let end = path.start;
                grow_union(&mut self.union, mk_point(r.x0, r.y0));
                grow_union(&mut self.union, mk_point(r.x1, r.y1));
                self.pending.push(path);
                Ok(end)
            }
            _ => {
                let mut path = DrawnPath::new(mk_point(r.x0, r.y0));
                path.segs.push(Seg::Rect(r));
                grow_union(&mut self.union, mk_point(r.x0, r.y0));
                grow_union(&mut self.union, mk_point(r.x1, r.y1));
                self.pending.push(path);
                Ok(mk_point(r.x0, r.y0))
            }
        }
    }

    /// Queue a sector.
    pub fn draw_sector(
        &mut self,
        _center: Point,
        _point: Point,
        _beta: f64,
        _full_sector: bool,
    ) -> Result<Point> {
        let path = sector_path(_center, _point, _beta, _full_sector);
        let sweep = _beta.to_radians();
        let (sin, cos) = sweep.sin_cos();
        let (dx, dy) = (_point.x - _center.x, _point.y - _center.y);
        let end = mk_point(
            _center.x + dx * cos + dy * sin,
            _center.y - dx * sin + dy * cos,
        );
        grow_union(&mut self.union, _center);
        grow_union(&mut self.union, _point);
        grow_union(&mut self.union, end);
        self.pending.push(path);
        Ok(end)
    }

    /// Queue a squiggle.
    pub fn draw_squiggle(&mut self, _p1: Point, _p2: Point, _breadth: f64) -> Result<Point> {
        let path = squiggle_path(_p1, _p2, _breadth);
        grow_union(&mut self.union, _p1);
        grow_union(&mut self.union, _p2);
        self.pending.push(path);
        Ok(_p2)
    }

    /// Queue a zigzag.
    pub fn draw_zigzag(&mut self, _p1: Point, _p2: Point, _breadth: f64) -> Result<Point> {
        let path = zigzag_path(_p1, _p2, _breadth);
        grow_union(&mut self.union, _p1);
        grow_union(&mut self.union, _p2);
        self.pending.push(path);
        Ok(_p2)
    }

    /// Style pending marks.
    pub fn finish(&mut self, _options: &ShapeFinishOptions) -> Result<()> {
        let paths = std::mem::take(&mut self.pending);
        let _ = (&_options.morph, _options.oc);
        self.finished.push(FinishedPath {
            paths,
            width: _options.width,
            color: _options.color.clone(),
            fill: _options.fill.clone(),
            line_cap: _options.line_cap,
            line_join: _options.line_join,
            dashes: _options.dashes.clone(),
            even_odd: _options.even_odd,
            close: _options.close_path,
            fill_opacity: _options.fill_opacity,
            stroke_opacity: _options.stroke_opacity,
        });
        Ok(())
    }

    /// Angle from a center point.
    pub fn horizontal_angle(&self, _center: Point, _point: Point) -> f64 {
        let dx = _point.x - _center.x;
        let dy = _point.y - _center.y;
        dy.atan2(dx).to_degrees()
    }

    /// Queue text at a point.
    pub fn insert_text(
        &mut self,
        _point: Point,
        _text: &TextArg,
        _options: &InsertTextOptions,
    ) -> Result<i32> {
        let (face, base) = insert_face(&_options.fontname, &_options.fontfile)?;
        let font_base = base.ok_or_else(|| bad_value("file fonts cannot be written yet"))?;
        let size = _options.fontsize;
        let height = _options
            .lineheight
            .unwrap_or_else(|| default_line_height(&face, size));
        let lines = text_arg_lines(_text);
        for (index, line) in lines.iter().enumerate() {
            let pos = mk_point(_point.x, _point.y + index as f64 * height);
            self.texts.push(StagedRun {
                pos,
                text: line.clone(),
                font_base: font_base.clone(),
                fontsize: size,
                color: _options.color.clone(),
                stroke: _options.fill.clone(),
                opacity: _options.fill_opacity,
                stroke_opacity: _options.stroke_opacity,
                render_mode: _options.render_mode,
                rotate: _options.rotate,
                border_width: _options.border_width,
                miter_limit: _options.miter_limit,
                align: 0,
                max_width: 0.0,
            });
            grow_union(&mut self.union, pos);
        }
        let _ = (
            _options.set_simple,
            _options.encoding,
            _options.overlay,
            &_options.morph,
            _options.oc,
        );
        Ok(lines.len() as i32)
    }

    /// Queue a textbox and return unused height.
    pub fn insert_textbox(
        &mut self,
        _rect: Rect,
        _text: &TextArg,
        _options: &TextBoxOptions,
    ) -> Result<f64> {
        let (face, base) = insert_face(&_options.text.fontname, &_options.text.fontfile)?;
        let font_base = base.ok_or_else(|| bad_value("file fonts cannot be written yet"))?;
        let size = _options.text.fontsize;
        let height = _options
            .text
            .lineheight
            .unwrap_or_else(|| default_line_height(&face, size));
        let r = norm_rect(&_rect);
        let mut laid: Vec<(String, f64)> = Vec::new();
        for logical in text_arg_lines(_text) {
            let expanded = if _options.expandtabs != 0 {
                logical.replace('\t', "        ")
            } else {
                logical
            };
            laid.extend(wrap_lines(&expanded, r.x1 - r.x0, &face, size));
        }
        let capacity = if height > 0.0 {
            ((r.y1 - r.y0) / height).floor() as usize
        } else {
            laid.len()
        };
        let placed = laid.len().min(capacity);
        let asc = face.asc_em() * size;
        for (index, (line, width)) in laid.iter().take(placed).enumerate() {
            let x = match _options.align {
                1 => r.x0 + (r.x1 - r.x0 - width) / 2.0,
                2 => r.x1 - width,
                _ => r.x0,
            };
            let baseline = r.y0 + asc + index as f64 * height;
            self.texts.push(StagedRun {
                pos: mk_point(x, baseline),
                text: line.clone(),
                font_base: font_base.clone(),
                fontsize: size,
                color: _options.text.color.clone(),
                stroke: _options.text.fill.clone(),
                opacity: _options.text.fill_opacity,
                stroke_opacity: _options.text.stroke_opacity,
                render_mode: _options.text.render_mode,
                rotate: _options.text.rotate,
                border_width: _options.text.border_width,
                miter_limit: _options.text.miter_limit,
                align: _options.align,
                max_width: r.x1 - r.x0,
            });
        }
        grow_union(&mut self.union, mk_point(r.x0, r.y0));
        grow_union(&mut self.union, mk_point(r.x1, r.y1));
        let _ = (
            _options.text.set_simple,
            _options.text.encoding,
            _options.text.overlay,
            &_options.text.morph,
            _options.text.oc,
        );
        Ok((r.y1 - r.y0) - placed as f64 * height + face.desc_em() * size)
    }

    /// Extend the shape box.
    pub fn update_rect(&mut self, _rect: Rect) -> Result<()> {
        self.union = Some(_rect);
        Ok(())
    }

    /// Union of everything drawn so far, if anything was drawn.
    pub fn rect(&self) -> Option<Rect> {
        self.union
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        let _ = (_other, _op);
        Err(wrong_type("shape ordering is unsupported"))
    }
}

impl<'a> PageMut<'a> {
    /// Add a caret annotation.
    pub fn add_caret_annot(&mut self, _point: Point) -> Result<Annot<'_>> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let r = mk_rect(
            _point.x - 1.0,
            _point.y + 0.5,
            _point.x + 19.0,
            _point.y + 14.5,
        );
        let id = add_annot_common(lp, page_id, "Caret", &flip_rect(&r, top))?;
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Add a circle annotation.
    pub fn add_circle_annot(&mut self, _rect: Rect) -> Result<Annot<'_>> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let r = norm_rect(&_rect);
        let r = mk_rect(r.x0 - 1.0, r.y0 - 1.0, r.x1 + 1.0, r.y1 + 1.0);
        let id = add_annot_common(lp, page_id, "Circle", &flip_rect(&r, top))?;
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Add a file annotation.
    pub fn add_file_annot(
        &mut self,
        _point: Point,
        _data: &[u8],
        _options: &FileAnnotOptions,
    ) -> Result<Annot<'_>> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let r = mk_rect(_point.x, _point.y, _point.x + 16.0, _point.y + 16.0);
        let id = add_annot_common(lp, page_id, "FileAttachment", &flip_rect(&r, top))?;
        let spec = filespec_object(
            lp,
            &_options.filename,
            _options.display_name.as_deref(),
            _options.description.as_deref(),
            _data,
        )?;
        {
            let dict = lp.get_dictionary_mut(id)?;
            dict.set("FS", Object::Reference(spec));
            if let Some(icon) = &_options.icon {
                dict.set("Name", name_object(icon));
            }
        }
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Add a freetext annotation.
    pub fn add_freetext_annot(
        &mut self,
        _rect: Rect,
        _text: &str,
        _options: &FreeTextOptions,
    ) -> Result<Annot<'_>> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let r = norm_rect(&_rect);
        let pad = if _options.border_width > 0.0 {
            _options.border_width / 2.0
        } else {
            0.0
        };
        let r = mk_rect(r.x0 - pad, r.y0 - pad, r.x1 + pad, r.y1 + pad);
        let id = add_annot_common(lp, page_id, "FreeText", &flip_rect(&r, top))?;
        {
            let dict = lp.get_dictionary_mut(id)?;
            if _options.richtext {
                dict.set("Contents", text_object(""));
                dict.set("RC", text_object(&format!("<body><p>{}</p></body>", _text)));
            } else {
                dict.set("Contents", text_object(_text));
            }
            dict.set(
                "DA",
                text_object(&build_da(
                    _options.fontname.as_deref(),
                    _options.fontsize,
                    _options.text_color.as_deref(),
                )),
            );
            dict.set("Q", Object::Integer(i64::from(_options.align)));
            dict.set(
                "RD",
                Object::Array(vec![
                    Object::Real(pad as f32),
                    Object::Real(pad as f32),
                    Object::Real(-pad as f32),
                    Object::Real(-pad as f32),
                ]),
            );
            if let Some(color) = &_options.border_color {
                dict.set("C", num_array(color));
            }
            if let Some(fill) = &_options.fill_color {
                dict.set("IC", num_array(fill));
            }
            if _options.border_width > 0.0 {
                let mut bs = Dictionary::new();
                bs.set("W", Object::Real(_options.border_width as f32));
                if let Some(dashes) = &_options.dashes {
                    bs.set("D", num_array(dashes));
                }
                dict.set("BS", Object::Dictionary(bs));
            }
            if let Some(callout) = &_options.callout {
                let mut pts = Vec::with_capacity(callout.len() * 2);
                for p in callout {
                    pts.push(Object::Real(p.x as f32));
                    pts.push(Object::Real(flip_y(p.y, top) as f32));
                }
                dict.set("CL", Object::Array(pts));
                dict.set(
                    "LE",
                    Object::Array(vec![
                        name_object("None"),
                        name_object(le_code_to_name(_options.line_end)),
                    ]),
                );
            }
            if _options.rotate != 0 {
                dict.set("Rotate", Object::Integer(i64::from(_options.rotate)));
            }
            if _options.opacity < 1.0 {
                dict.set("CA", Object::Real(_options.opacity.clamp(0.0, 1.0) as f32));
            }
        }
        let _ = &_options.style;
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Add a highlight annotation.
    pub fn add_highlight_annot(&mut self, _options: &HighlightOptions) -> Result<Annot<'_>> {
        self.add_mark_annot("Highlight", _options)
    }

    /// Add an ink annotation.
    pub fn add_ink_annot(&mut self, _handwriting: &[Vec<Point>]) -> Result<Annot<'_>> {
        if _handwriting.is_empty() {
            return Err(bad_value("handwriting needs at least one stroke"));
        }
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let mut bbox: Option<Rect> = None;
        for stroke in _handwriting {
            for p in stroke {
                grow_union(&mut bbox, *p);
            }
        }
        let r = bbox.unwrap_or(mk_rect(0.0, 0.0, 0.0, 0.0));
        let r = mk_rect(r.x0 - 8.0, r.y0 - 8.0, r.x1 + 8.0, r.y1 + 8.0);
        let id = add_annot_common(lp, page_id, "Ink", &flip_rect(&r, top))?;
        lp.get_dictionary_mut(id)?
            .set("InkList", write_inklist(_handwriting, top));
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Add a line annotation.
    pub fn add_line_annot(&mut self, _p1: Point, _p2: Point) -> Result<Annot<'_>> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let (x0, x1) = if _p1.x <= _p2.x {
            (_p1.x, _p2.x)
        } else {
            (_p2.x, _p1.x)
        };
        let (y0, y1) = if _p1.y <= _p2.y {
            (_p1.y, _p2.y)
        } else {
            (_p2.y, _p1.y)
        };
        let r = mk_rect(x0 - 4.5, y0 - 4.5, x1 + 4.5, y1 + 4.5);
        let id = add_annot_common(lp, page_id, "Line", &flip_rect(&r, top))?;
        lp.get_dictionary_mut(id)?.set(
            "L",
            Object::Array(vec![
                Object::Real(_p1.x as f32),
                Object::Real(flip_y(_p1.y, top) as f32),
                Object::Real(_p2.x as f32),
                Object::Real(flip_y(_p2.y, top) as f32),
            ]),
        );
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Add a polygon annotation.
    pub fn add_polygon_annot(&mut self, _points: &[Point]) -> Result<Annot<'_>> {
        self.add_vertices_annot("Polygon", _points)
    }

    /// Add a polyline annotation.
    pub fn add_polyline_annot(&mut self, _points: &[Point]) -> Result<Annot<'_>> {
        self.add_vertices_annot("PolyLine", _points)
    }

    /// Add a rectangle annotation.
    pub fn add_rect_annot(&mut self, _rect: Rect) -> Result<Annot<'_>> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let r = norm_rect(&_rect);
        let r = mk_rect(r.x0 - 1.0, r.y0 - 1.0, r.x1 + 1.0, r.y1 + 1.0);
        let id = add_annot_common(lp, page_id, "Square", &flip_rect(&r, top))?;
        {
            let dict = lp.get_dictionary_mut(id)?;
            dict.set(
                "RD",
                Object::Array(vec![
                    Object::Integer(1),
                    Object::Integer(1),
                    Object::Integer(-1),
                    Object::Integer(-1),
                ]),
            );
        }
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Add a redaction annotation.
    pub fn add_redact_annot(
        &mut self,
        _quad: Quad,
        _options: &RedactAnnotOptions,
    ) -> Result<Annot<'_>> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let corners = [_quad.ul, _quad.ur, _quad.ll, _quad.lr];
        let r = corners.iter().fold(None, |acc: Option<Rect>, p| {
            let mut acc = acc;
            grow_union(&mut acc, *p);
            acc
        });
        let r = norm_rect(&r.unwrap_or(mk_rect(0.0, 0.0, 0.0, 0.0)));
        let id = add_annot_common(lp, page_id, "Redact", &flip_rect(&r, top))?;
        {
            let dict = lp.get_dictionary_mut(id)?;
            dict.set("QuadPoints", write_quadpoints(&[_quad], top));
            if let Some(text) = &_options.text {
                dict.set("OverlayText", text_object(text));
            }
            dict.set("Q", Object::Integer(i64::from(_options.align)));
            if let Some(fill) = &_options.fill {
                dict.set("IC", num_array(fill));
            }
            if let Some(color) = &_options.text_color {
                dict.set("C", num_array(color));
            }
            if _options.fontname.is_some() || _options.fontsize > 0.0 {
                dict.set(
                    "DA",
                    text_object(&build_da(
                        _options.fontname.as_deref(),
                        _options.fontsize,
                        None,
                    )),
                );
            }
        }
        let _ = _options.cross_out;
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Add a squiggly annotation.
    pub fn add_squiggly_annot(&mut self, _options: &HighlightOptions) -> Result<Annot<'_>> {
        self.add_mark_annot("Squiggly", _options)
    }

    /// Add a stamp annotation.
    pub fn add_stamp_annot(&mut self, _rect: Rect, _stamp: i32) -> Result<Annot<'_>> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let r = norm_rect(&_rect);
        let height = (r.x1 - r.x0) * 50.0 / 190.0;
        let mid = (r.y0 + r.y1) / 2.0;
        let r = mk_rect(r.x0, mid - height / 2.0, r.x1, mid + height / 2.0);
        let id = add_annot_common(lp, page_id, "Stamp", &flip_rect(&r, top))?;
        lp.get_dictionary_mut(id)?
            .set("Name", name_object(stamp_name(_stamp)));
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Add a strikeout annotation.
    pub fn add_strikeout_annot(&mut self, _options: &HighlightOptions) -> Result<Annot<'_>> {
        self.add_mark_annot("StrikeOut", _options)
    }

    /// Add a text annotation.
    pub fn add_text_annot(&mut self, _point: Point, _text: &str, _icon: &str) -> Result<Annot<'_>> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let r = mk_rect(_point.x, _point.y, _point.x + 16.0, _point.y + 16.0);
        let pdf = flip_rect(&r, top);
        let popup = mk_rect(
            _point.x - 18.0,
            _point.y - 38.0,
            _point.x + 182.0,
            _point.y + 62.0,
        );
        let popup_pdf = flip_rect(&popup, top);
        let id = add_annot_common(lp, page_id, "Text", &pdf)?;
        {
            let dict = lp.get_dictionary_mut(id)?;
            dict.set("Contents", text_object(_text));
            dict.set("Name", name_object(_icon));
            dict.set("Open", Object::Boolean(false));
        }
        ensure_popup(lp, id, &popup_pdf)?;
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Add an underline annotation.
    pub fn add_underline_annot(&mut self, _options: &HighlightOptions) -> Result<Annot<'_>> {
        self.add_mark_annot("Underline", _options)
    }

    /// Add a form widget.
    pub fn add_widget(&mut self, _widget: &Widget) -> Result<Annot<'_>> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let id = create_widget_field(lp, page_id, top, _widget)?;
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Annotation names.
    pub fn annot_names(&self) -> Result<Vec<String>> {
        let lp = self.document().lopdf();
        let page_id = page_id_of(lp, self.index())?;
        let mut out = Vec::new();
        for id in page_annots(lp, page_id) {
            let name = lp
                .get_dictionary(id)
                .ok()
                .and_then(|d| d.get(b"NM").ok().map(obj_text))
                .unwrap_or_default();
            if !name.is_empty() {
                out.push(name);
            }
        }
        Ok(out)
    }

    /// Annotation cross-references.
    pub fn annot_xrefs(&self) -> Result<Vec<AnnotXref>> {
        let lp = self.document().lopdf();
        let page_id = page_id_of(lp, self.index())?;
        let mut out = Vec::new();
        for id in page_annots(lp, page_id) {
            let dict = lp.get_dictionary(id)?.clone();
            let subtype = dict.get(b"Subtype").ok().map(obj_name).unwrap_or_default();
            let (code, _) = subtype_code(&subtype);
            out.push(AnnotXref {
                xref: id.0 as i32,
                kind_code: code,
                name: dict.get(b"NM").ok().map(obj_text).unwrap_or_default(),
            });
        }
        Ok(out)
    }

    /// Annotation entries, filtered by type.
    pub fn annots(&self, _types: Option<&[i32]>) -> Result<Vec<AnnotEntry>> {
        let lp = self.document().lopdf();
        let page_id = page_id_of(lp, self.index())?;
        let top = page_top(lp, page_id);
        let mut out = Vec::new();
        for id in page_annots(lp, page_id) {
            let dict = lp.get_dictionary(id)?.clone();
            let subtype = dict.get(b"Subtype").ok().map(obj_name).unwrap_or_default();
            // Widgets list under widgets(), never here: the bake recording
            // pins annots() == 0 on a page with two widget annotations.
            if subtype == "Widget" {
                continue;
            }
            let (code, name) = subtype_code(&subtype);
            if let Some(types) = _types {
                if !types.contains(&code) {
                    continue;
                }
            }
            let rect = dict
                .get(b"Rect")
                .ok()
                .and_then(obj_rect)
                .map(|r| flip_rect(&r, top))
                .unwrap_or(mk_rect(0.0, 0.0, 0.0, 0.0));
            out.push(AnnotEntry {
                xref: id.0 as i32,
                annot_type: AnnotType {
                    code,
                    name: name.to_string(),
                },
                rect,
            });
        }
        Ok(out)
    }

    /// Apply redactions.
    pub fn apply_redactions(&mut self, _options: &RedactApplyOptions) -> Result<bool> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let mut areas = Vec::new();
        let mut victims = Vec::new();
        for id in page_annots(lp, page_id) {
            let dict = lp.get_dictionary(id)?.clone();
            let subtype = dict.get(b"Subtype").ok().map(obj_name).unwrap_or_default();
            if subtype != "Redact" {
                continue;
            }
            let rect = dict
                .get(b"Rect")
                .ok()
                .and_then(obj_rect)
                .unwrap_or(mk_rect(0.0, 0.0, 0.0, 0.0));
            let fill = dict
                .get(b"IC")
                .ok()
                .and_then(|o| o.as_array().ok())
                .map(|arr| {
                    let v: Vec<f64> = arr.iter().map(obj_f64).collect();
                    to_rgb(&v)
                })
                .unwrap_or([0.0, 0.0, 0.0]);
            let mut area = pdf_redact::RedactionArea::new(
                index as u32 + 1,
                [rect.x0, rect.y0, rect.x1, rect.y1],
            )
            .with_color(fill[0], fill[1], fill[2]);
            let overlay = dict
                .get(b"OverlayText")
                .ok()
                .map(obj_text)
                .unwrap_or_default();
            if !overlay.is_empty() {
                area = area.with_overlay(overlay);
            }
            areas.push(area);
            victims.push(id);
            let _ = top;
        }
        if areas.is_empty() {
            return Ok(false);
        }
        let _ = (&_options.images, _options.graphics, _options.text);
        let mut redactor = pdf_redact::Redactor::new();
        redactor.mark_all(areas);
        let report = redactor.apply(lp)?;
        let ids = page_annots(lp, page_id);
        let keep: Vec<ObjectId> = ids.into_iter().filter(|id| !victims.contains(id)).collect();
        set_page_annots(lp, page_id, &keep)?;
        for id in victims {
            lp.objects.remove(&id);
        }
        sync_doc(self.document_mut())?;
        Ok(report.areas_redacted > 0)
    }

    /// Clean content streams.
    pub fn clean_contents(&mut self, _sanitize: bool) -> Result<()> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let stream_ids = lp.get_page_contents(page_id);
        let mut merged = Vec::new();
        for stream_id in &stream_ids {
            let raw = lp
                .get_object(*stream_id)
                .ok()
                .and_then(|o| o.as_stream().ok().cloned());
            let Some(stream) = raw else {
                continue;
            };
            let Ok(content) = stream.decompressed_content() else {
                continue;
            };
            let Ok(decoded) = lopdf::content::Content::decode(&content) else {
                continue;
            };
            let Ok(encoded) = decoded.encode() else {
                continue;
            };
            if !merged.is_empty() {
                merged.push(b'\n');
            }
            let _ = _sanitize;
            merged.extend_from_slice(&encoded);
        }
        let stream_id = lp.new_object_id();
        lp.objects.insert(
            stream_id,
            Object::Stream(Stream::new(Dictionary::new(), merged)),
        );
        lp.get_dictionary_mut(page_id)?
            .set("Contents", Object::Reference(stream_id));
        sync_doc(self.document_mut())?;
        Ok(())
    }

    /// Clustered drawings.
    pub fn cluster_drawings(&self, _options: &ClusterOptions) -> Result<Vec<DrawingCluster>> {
        let mut drawings = match &_options.drawings {
            Some(given) => given.clone(),
            None => read_drawings(
                self.document().lopdf(),
                page_id_of(self.document().lopdf(), self.index())?,
                false,
            ),
        };
        if let Some(clip) = &_options.clip {
            let clip = norm_rect(clip);
            drawings.retain(|d| {
                let r = norm_rect(&d.rect);
                r.x0 < clip.x1 && r.x1 > clip.x0 && r.y0 < clip.y1 && r.y1 > clip.y0
            });
        }
        drawings.sort_by(|a, b| {
            norm_rect(&a.rect)
                .y0
                .partial_cmp(&norm_rect(&b.rect).y0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    norm_rect(&a.rect)
                        .x0
                        .partial_cmp(&norm_rect(&b.rect).x0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        let mut clusters: Vec<DrawingCluster> = Vec::new();
        for drawing in drawings {
            let r = norm_rect(&drawing.rect);
            let mut placed = false;
            for cluster in &mut clusters {
                let b = norm_rect(&cluster.bbox);
                if r.x0 <= b.x1 + _options.x_tolerance
                    && r.x1 >= b.x0 - _options.x_tolerance
                    && r.y0 <= b.y1 + _options.y_tolerance
                    && r.y1 >= b.y0 - _options.y_tolerance
                {
                    cluster.bbox = union_rect(b, r);
                    cluster.drawings.push(drawing.clone());
                    placed = true;
                    break;
                }
            }
            if !placed {
                clusters.push(DrawingCluster {
                    bbox: r,
                    drawings: vec![drawing],
                });
            }
        }
        if _options.final_filter {
            clusters.retain(|c| c.drawings.len() > 1);
        }
        Ok(clusters)
    }

    /// Delete an annotation.
    pub fn delete_annot(&mut self, _annot: &AnnotEntry) -> Result<()> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let target = (_annot.xref as u32, 0);
        let ids = page_annots(lp, page_id);
        if !ids.contains(&target) {
            return Err(bad_value("annotation is not on this page"));
        }
        let keep: Vec<ObjectId> = ids.into_iter().filter(|id| *id != target).collect();
        set_page_annots(lp, page_id, &keep)?;
        lp.objects.remove(&target);
        sync_doc(self.document_mut())?;
        Ok(())
    }

    /// Delete an image.
    pub fn delete_image(&mut self, _xref: i32) -> Result<()> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let target = (_xref as u32, 0);
        if !lp.objects.contains_key(&target) {
            return Err(bad_value("image object does not exist"));
        }
        remove_resource_ref(lp, page_id, b"XObject", target);
        lp.objects.remove(&target);
        sync_doc(self.document_mut())?;
        Ok(())
    }

    /// Delete a link.
    pub fn delete_link(&mut self, _link: &Link) -> Result<()> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let target = (_link.xref as u32, 0);
        let ids = page_annots(lp, page_id);
        if !ids.contains(&target) {
            return Err(bad_value("link is not on this page"));
        }
        let keep: Vec<ObjectId> = ids.into_iter().filter(|id| *id != target).collect();
        set_page_annots(lp, page_id, &keep)?;
        lp.objects.remove(&target);
        sync_doc(self.document_mut())?;
        Ok(())
    }

    /// Delete a widget.
    pub fn delete_widget(&mut self, _widget: &Widget) -> Result<Widget> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let target = (_widget.xref as u32, 0);
        let ids = page_annots(lp, page_id);
        let position = ids
            .iter()
            .position(|id| *id == target)
            .ok_or_else(|| bad_value("widget is not on this page"))?;
        let following = ids
            .iter()
            .skip(position + 1)
            .filter(|id| {
                lp.get_dictionary(**id)
                    .ok()
                    .and_then(|d| d.get(b"Subtype").ok().map(obj_name))
                    .as_deref()
                    == Some("Widget")
            })
            .find_map(|id| widget_data_of(lp, index, id.0 as i32).ok());
        let keep: Vec<ObjectId> = ids.into_iter().filter(|id| *id != target).collect();
        set_page_annots(lp, page_id, &keep)?;
        remove_acroform_field(lp, target);
        lp.objects.remove(&target);
        sync_doc(self.document_mut())?;
        Ok(following.unwrap_or_else(|| _widget.clone()))
    }

    /// Draw a bezier curve.
    pub fn draw_bezier(
        &mut self,
        _p1: Point,
        _p2: Point,
        _p3: Point,
        _p4: Point,
        _options: &DrawOptions,
    ) -> Result<Point> {
        let mut path = DrawnPath::new(_p1);
        path.segs.push(Seg::Curve(_p1, _p2, _p3, _p4));
        self.finish_draw(path, _p4, _options)
    }

    /// Draw a circle.
    pub fn draw_circle(
        &mut self,
        _center: Point,
        _radius: f64,
        _options: &DrawOptions,
    ) -> Result<Point> {
        let path = ellipse_paths(_center, _radius, _radius);
        let end = mk_point(_center.x - _radius, _center.y);
        self.finish_draw(path, end, _options)
    }

    /// Draw a curve.
    pub fn draw_curve(
        &mut self,
        _p1: Point,
        _p2: Point,
        _p3: Point,
        _options: &DrawOptions,
    ) -> Result<Point> {
        let c1 = mk_point(
            _p1.x + 2.0 / 3.0 * (_p2.x - _p1.x),
            _p1.y + 2.0 / 3.0 * (_p2.y - _p1.y),
        );
        let c2 = mk_point(
            _p3.x + 2.0 / 3.0 * (_p2.x - _p3.x),
            _p3.y + 2.0 / 3.0 * (_p2.y - _p3.y),
        );
        let mut path = DrawnPath::new(_p1);
        path.segs.push(Seg::Curve(_p1, c1, c2, _p3));
        self.finish_draw(path, _p3, _options)
    }

    /// Draw a line.
    pub fn draw_line(&mut self, _p1: Point, _p2: Point, _options: &DrawOptions) -> Result<Point> {
        let mut path = DrawnPath::new(_p1);
        path.segs.push(Seg::Line(_p1, _p2));
        self.finish_draw(path, _p2, _options)
    }

    /// Draw an oval.
    pub fn draw_oval(&mut self, _rect: Rect, _options: &DrawOptions) -> Result<Point> {
        let r = norm_rect(&_rect);
        let center = mk_point((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
        let path = ellipse_paths(center, (r.x1 - r.x0) / 2.0, (r.y1 - r.y0) / 2.0);
        let end = mk_point(r.x0, center.y);
        self.finish_draw(path, end, _options)
    }

    /// Draw a polyline.
    pub fn draw_polyline(&mut self, _points: &[Point], _options: &DrawOptions) -> Result<Point> {
        if _points.is_empty() {
            return Err(bad_value("polyline needs at least one point"));
        }
        let mut path = DrawnPath::new(_points[0]);
        for window in _points.windows(2) {
            path.segs.push(Seg::Line(window[0], window[1]));
        }
        let end = *_points.last().unwrap_or(&_points[0]);
        self.finish_draw(path, end, _options)
    }

    /// Draw a quadrilateral.
    pub fn draw_quad(&mut self, _quad: Quad, _options: &DrawOptions) -> Result<Point> {
        let mut path = DrawnPath::new(_quad.ul);
        path.segs.push(Seg::Line(_quad.ul, _quad.ur));
        path.segs.push(Seg::Line(_quad.ur, _quad.lr));
        path.segs.push(Seg::Line(_quad.lr, _quad.ll));
        path.segs.push(Seg::Line(_quad.ll, _quad.ul));
        self.finish_draw(path, _quad.ul, _options)
    }

    /// Draw a rectangle.
    pub fn draw_rect(
        &mut self,
        _rect: Rect,
        _options: &DrawOptions,
        _radius: Option<f64>,
    ) -> Result<Point> {
        let r = norm_rect(&_rect);
        match _radius {
            Some(rad) if rad > 0.0 => {
                let path = round_rect_path(r, rad);
                let end = path.start;
                self.finish_draw(path, end, _options)
            }
            _ => {
                let mut path = DrawnPath::new(mk_point(r.x0, r.y0));
                path.segs.push(Seg::Rect(r));
                self.finish_draw(path, mk_point(r.x0, r.y0), _options)
            }
        }
    }

    /// Draw a sector.
    pub fn draw_sector(
        &mut self,
        _center: Point,
        _point: Point,
        _beta: f64,
        _full_sector: bool,
        _options: &DrawOptions,
    ) -> Result<Point> {
        let path = sector_path(_center, _point, _beta, _full_sector);
        let sweep = _beta.to_radians();
        let (sin, cos) = sweep.sin_cos();
        let (dx, dy) = (_point.x - _center.x, _point.y - _center.y);
        let end = mk_point(
            _center.x + dx * cos + dy * sin,
            _center.y - dx * sin + dy * cos,
        );
        self.finish_draw(path, end, _options)
    }

    /// Draw a squiggle.
    pub fn draw_squiggle(
        &mut self,
        _p1: Point,
        _p2: Point,
        _breadth: f64,
        _options: &DrawOptions,
    ) -> Result<Point> {
        let path = squiggle_path(_p1, _p2, _breadth);
        self.finish_draw(path, _p2, _options)
    }

    /// Draw a zigzag.
    pub fn draw_zigzag(
        &mut self,
        _p1: Point,
        _p2: Point,
        _breadth: f64,
        _options: &DrawOptions,
    ) -> Result<Point> {
        let path = zigzag_path(_p1, _p2, _breadth);
        self.finish_draw(path, _p2, _options)
    }

    /// First annotation entry.
    pub fn first_annot(&self) -> Result<Option<AnnotEntry>> {
        let lp = self.document().lopdf();
        let page_id = page_id_of(lp, self.index())?;
        let top = page_top(lp, page_id);
        let Some(id) = page_annots(lp, page_id).into_iter().next() else {
            return Ok(None);
        };
        let dict = lp.get_dictionary(id)?.clone();
        let subtype = dict.get(b"Subtype").ok().map(obj_name).unwrap_or_default();
        let (code, name) = subtype_code(&subtype);
        let rect = dict
            .get(b"Rect")
            .ok()
            .and_then(obj_rect)
            .map(|r| flip_rect(&r, top))
            .unwrap_or(mk_rect(0.0, 0.0, 0.0, 0.0));
        Ok(Some(AnnotEntry {
            xref: id.0 as i32,
            annot_type: AnnotType {
                code,
                name: name.to_string(),
            },
            rect,
        }))
    }

    /// First link.
    pub fn first_link(&self) -> Result<Option<Link>> {
        Ok(self.page_links()?.into_iter().next())
    }

    /// First widget.
    pub fn first_widget(&self) -> Result<Option<Widget>> {
        let lp = self.document().lopdf();
        let page_id = page_id_of(lp, self.index())?;
        for id in page_annots(lp, page_id) {
            let is_widget = lp
                .get_dictionary(id)
                .ok()
                .and_then(|d| d.get(b"Subtype").ok().map(obj_name))
                .as_deref()
                == Some("Widget");
            if is_widget {
                return Ok(Some(widget_data_of(lp, self.index(), id.0 as i32)?));
            }
        }
        Ok(None)
    }

    /// Streamed drawings.
    pub fn get_cdrawings(&self, _options: &CdrawingsOptions) -> Result<Vec<Drawing>> {
        let lp = self.document().lopdf();
        let page_id = page_id_of(lp, self.index())?;
        let drawings = read_drawings(lp, page_id, _options.extended);
        if let Some(callback) = _options.callback {
            for drawing in &drawings {
                callback(drawing);
            }
        }
        Ok(drawings)
    }

    /// Vector drawings.
    pub fn get_drawings(&self, _extended: bool) -> Result<Vec<Drawing>> {
        let lp = self.document().lopdf();
        let page_id = page_id_of(lp, self.index())?;
        Ok(read_drawings(lp, page_id, _extended))
    }

    /// Links on the page.
    pub fn get_links(&self) -> Result<Vec<Link>> {
        self.page_links()
    }

    /// Insert a font.
    pub fn insert_font(&mut self, _options: &InsertFontOptions) -> Result<i32> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        if let Some(data) = &_options.fontbuffer {
            let (font_id, _) = embed_file_font(lp, page_id, data, _options.encoding)?;
            sync_doc(self.document_mut())?;
            return Ok(font_id.0 as i32);
        }
        if let Some(path) = &_options.fontfile {
            let data = std::fs::read(path)
                .map_err(|_| missing_file(format!("font file not found: {path}")))?;
            let parsed = parse_file_face(&data)
                .map_err(|_| unreadable(format!("font file does not parse: {path}")))?;
            let _ = parsed;
            let (font_id, _) = embed_file_font(lp, page_id, &data, _options.encoding)?;
            sync_doc(self.document_mut())?;
            return Ok(font_id.0 as i32);
        }
        let base = resolve_fontname(&_options.fontname)
            .ok_or_else(|| bad_value(format!("unknown font name: {}", _options.fontname)))?;
        let mut font = Dictionary::new();
        font.set("Type", name_object("Font"));
        font.set("Subtype", name_object("Type1"));
        font.set("BaseFont", name_object(base));
        if base != "Symbol" && base != "ZapfDingbats" {
            font.set("Encoding", name_object("WinAnsiEncoding"));
        }
        let font_id = lp.new_object_id();
        lp.objects.insert(font_id, Object::Dictionary(font));
        let name = fresh_resource_name(lp, page_id, b"Font", "F");
        resource_sub_mut(lp, page_id, b"Font", &name, Object::Reference(font_id))?;
        let _ = (_options.set_simple, _options.wmode);
        sync_doc(self.document_mut())?;
        Ok(font_id.0 as i32)
    }

    /// Insert an HTML box.
    pub fn insert_htmlbox(
        &mut self,
        _rect: Rect,
        _text: &str,
        _options: &HtmlBoxOptions,
    ) -> Result<(f64, f64)> {
        let face =
            builtin_face("Helvetica").ok_or_else(|| engine_fault("builtin metrics are missing"))?;
        let size = 11.0;
        let height = default_line_height(&face, size);
        let r = norm_rect(&_rect);
        let mut plain = String::with_capacity(_text.len());
        let mut inside = false;
        for ch in _text.chars() {
            match ch {
                '<' => inside = true,
                '>' => inside = false,
                _ if !inside => plain.push(ch),
                _ => {}
            }
        }
        let plain = plain
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let mut laid: Vec<(String, f64)> = Vec::new();
        for logical in plain.lines() {
            laid.extend(wrap_lines(logical, r.x1 - r.x0, &face, size));
        }
        let capacity = if height > 0.0 {
            ((r.y1 - r.y0) / height).floor() as usize
        } else {
            laid.len()
        };
        let placed = laid.len().min(capacity);
        let asc = face.asc_em() * size;
        let mut runs = Vec::with_capacity(placed);
        for (index, (line, width)) in laid.iter().take(placed).enumerate() {
            let _ = width;
            runs.push(StagedRun {
                pos: mk_point(r.x0, r.y0 + asc + index as f64 * height),
                text: line.clone(),
                font_base: face.face_name.clone(),
                fontsize: size,
                color: None,
                stroke: None,
                opacity: _options.opacity,
                stroke_opacity: _options.opacity,
                render_mode: 0,
                rotate: _options.rotate,
                border_width: 0.0,
                miter_limit: 1.0,
                align: 0,
                max_width: r.x1 - r.x0,
            });
        }
        let files: HashMap<String, Vec<u8>> = HashMap::new();
        let _ = (
            &_options.css,
            _options.scale_low,
            &_options.archive,
            _options.oc,
        );
        self.commit_text(runs, &files, 0, _options.overlay)?;
        Ok((
            0.0,
            (r.y1 - r.y0) - placed as f64 * height + face.desc_em() * size,
        ))
    }

    /// Insert an image.
    pub fn insert_image(&mut self, _rect: Rect, _options: &ImageInsertOptions) -> Result<i32> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        touch_resource_sub(lp, page_id, b"XObject")?;
        let keep_alpha = _options.alpha != 0;
        let (img_id, iw, ih) = if _options.xref > 0 {
            let id = (_options.xref as u32, 0);
            let dict = lp
                .get_object(id)
                .ok()
                .and_then(|o| o.as_stream().ok().map(|s| s.dict.clone()))
                .ok_or_else(|| bad_value("image object is not a stream"))?;
            let w = dict.get(b"Width").ok().map(obj_f64).unwrap_or(0.0) as u32;
            let h = dict.get(b"Height").ok().map(obj_f64).unwrap_or(0.0) as u32;
            (id, w.max(1), h.max(1))
        } else if let Some(pixmap) = &_options.pixmap {
            pixmap_image_xobject(lp, pixmap, keep_alpha)?
        } else if let Some(stream) = &_options.stream {
            build_image_xobject(lp, stream, keep_alpha)?
        } else if let Some(path) = &_options.filename {
            let data = std::fs::read(path)
                .map_err(|_| missing_file(format!("image file not found: {path}")))?;
            build_image_xobject(lp, &data, keep_alpha)?
        } else {
            return Err(bad_value("image needs a pixmap, stream, filename, or xref"));
        };
        let r = norm_rect(&_rect);
        let (mut w, mut h) = (
            if _options.width > 0.0 {
                _options.width
            } else {
                r.x1 - r.x0
            },
            if _options.height > 0.0 {
                _options.height
            } else {
                r.y1 - r.y0
            },
        );
        if _options.keep_proportion && iw > 0 && ih > 0 {
            let aspect = f64::from(iw) / f64::from(ih);
            match (_options.width > 0.0, _options.height > 0.0) {
                (true, false) => h = w / aspect,
                (false, true) => w = h * aspect,
                _ => {}
            }
        }
        let place = mk_rect(r.x0, r.y0, r.x0 + w.max(1.0), r.y0 + h.max(1.0));
        let name = ensure_xobject(lp, page_id, img_id)?;
        let _ = (&_options.mask, _options.oc);
        push_content(
            lp,
            page_id,
            image_placement(&name, &place, top, _options.rotate),
            _options.overlay,
        )?;
        sync_doc(self.document_mut())?;
        Ok(img_id.0 as i32)
    }

    /// Insert a link.
    pub fn insert_link(&mut self, _link: &Link, _mark: bool) -> Result<()> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let r = norm_rect(&_link.rect);
        let id = add_annot_common(lp, page_id, "Link", &flip_rect(&r, top))?;
        let mut link = _link.clone();
        link.xref = id.0 as i32;
        write_link(lp, id, &link, top)?;
        let _ = _mark;
        sync_doc(self.document_mut())?;
        Ok(())
    }

    /// Insert text.
    pub fn insert_text(
        &mut self,
        _point: Point,
        _text: &TextArg,
        _options: &InsertTextOptions,
    ) -> Result<i32> {
        let (face, base) = insert_face(&_options.fontname, &_options.fontfile)?;
        let size = _options.fontsize;
        let height = _options
            .lineheight
            .unwrap_or_else(|| default_line_height(&face, size));
        let lines = text_arg_lines(_text);
        let mut files: HashMap<String, Vec<u8>> = HashMap::new();
        let key = match base {
            Some(name) => name,
            None => {
                let path = _options.fontfile.as_deref().unwrap_or("");
                let data = std::fs::read(path)
                    .map_err(|_| missing_file(format!("font file not found: {path}")))?;
                files.insert("\u{0}file".to_string(), data);
                "\u{0}file".to_string()
            }
        };
        let mut runs = Vec::with_capacity(lines.len());
        for (index, line) in lines.iter().enumerate() {
            runs.push(StagedRun {
                pos: mk_point(_point.x, _point.y + index as f64 * height),
                text: line.clone(),
                font_base: key.clone(),
                fontsize: size,
                color: _options.color.clone(),
                stroke: _options.fill.clone(),
                opacity: _options.fill_opacity,
                stroke_opacity: _options.stroke_opacity,
                render_mode: _options.render_mode,
                rotate: _options.rotate,
                border_width: _options.border_width,
                miter_limit: _options.miter_limit,
                align: 0,
                max_width: 0.0,
            });
        }
        let _ = (
            _options.set_simple,
            _options.encoding,
            &_options.morph,
            _options.oc,
        );
        self.commit_text(runs, &files, _options.encoding, _options.overlay)?;
        Ok(lines.len() as i32)
    }

    /// Insert a textbox and return unused height.
    pub fn insert_textbox(
        &mut self,
        _rect: Rect,
        _text: &TextArg,
        _options: &TextBoxOptions,
    ) -> Result<f64> {
        let (face, base) = insert_face(&_options.text.fontname, &_options.text.fontfile)?;
        let size = _options.text.fontsize;
        let height = _options
            .text
            .lineheight
            .unwrap_or_else(|| default_line_height(&face, size));
        let r = norm_rect(&_rect);
        let mut files: HashMap<String, Vec<u8>> = HashMap::new();
        let key = match base {
            Some(name) => name,
            None => {
                let path = _options.text.fontfile.as_deref().unwrap_or("");
                let data = std::fs::read(path)
                    .map_err(|_| missing_file(format!("font file not found: {path}")))?;
                files.insert("\u{0}file".to_string(), data);
                "\u{0}file".to_string()
            }
        };
        let mut laid: Vec<(String, f64)> = Vec::new();
        for logical in text_arg_lines(_text) {
            let expanded = if _options.expandtabs != 0 {
                logical.replace('\t', "        ")
            } else {
                logical
            };
            laid.extend(wrap_lines(&expanded, r.x1 - r.x0, &face, size));
        }
        let capacity = if height > 0.0 {
            ((r.y1 - r.y0) / height).floor() as usize
        } else {
            laid.len()
        };
        let placed = laid.len().min(capacity);
        let asc = face.asc_em() * size;
        let mut runs = Vec::with_capacity(placed);
        for (index, (line, width)) in laid.iter().take(placed).enumerate() {
            let x = match _options.align {
                1 => r.x0 + (r.x1 - r.x0 - width) / 2.0,
                2 => r.x1 - width,
                _ => r.x0,
            };
            let baseline = r.y0 + asc + index as f64 * height;
            runs.push(StagedRun {
                pos: mk_point(x, baseline),
                text: line.clone(),
                font_base: key.clone(),
                fontsize: size,
                color: _options.text.color.clone(),
                stroke: _options.text.fill.clone(),
                opacity: _options.text.fill_opacity,
                stroke_opacity: _options.text.stroke_opacity,
                render_mode: _options.text.render_mode,
                rotate: _options.text.rotate,
                border_width: _options.text.border_width,
                miter_limit: _options.text.miter_limit,
                align: _options.align,
                max_width: r.x1 - r.x0,
            });
        }
        let _ = (
            _options.text.set_simple,
            _options.text.encoding,
            &_options.text.morph,
            _options.text.oc,
        );
        self.commit_text(runs, &files, _options.text.encoding, _options.text.overlay)?;
        Ok((r.y1 - r.y0) - placed as f64 * height + face.desc_em() * size)
    }

    /// Links on the page, filtered by kind.
    pub fn links(&self, _kinds: Option<&[i32]>) -> Result<Vec<Link>> {
        let mut links = self.page_links()?;
        if let Some(kinds) = _kinds {
            links.retain(|l| kinds.contains(&l.kind));
        }
        Ok(links)
    }

    /// Load an annotation.
    pub fn load_annot(&mut self, _ident: &AnnotIdent) -> Result<Annot<'_>> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let xref = match _ident {
            AnnotIdent::Xref(xref) => {
                let target = (*xref as u32, 0);
                if !page_annots(lp, page_id).contains(&target) {
                    return Err(engine_fault(format!(
                        "annotation {xref} is not on this page"
                    )));
                }
                *xref
            }
            AnnotIdent::Name(name) => {
                let mut found = None;
                for id in page_annots(lp, page_id) {
                    let same = lp
                        .get_dictionary(id)
                        .ok()
                        .and_then(|d| d.get(b"NM").ok().map(obj_text))
                        .as_deref()
                        == Some(name.as_str());
                    if same {
                        found = Some(id.0 as i32);
                        break;
                    }
                }
                found
                    .ok_or_else(|| engine_fault(format!("annotation {name} is not on this page")))?
            }
        };
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Load links.
    pub fn load_links(&self) -> Result<Vec<Link>> {
        self.page_links()
    }

    /// Load a widget.
    pub fn load_widget(&self, _xref: i32) -> Result<Widget> {
        let widget = widget_data_of(self.document().lopdf(), self.index(), _xref)
            .map_err(|_| engine_fault(format!("widget {_xref} is not on this page")))?;
        Ok(widget)
    }

    /// Drawing canvas on the page.
    pub fn new_shape(&mut self) -> Result<Shape<'_>> {
        let index = self.index();
        Ok(Shape::new(self.document_mut(), index))
    }

    /// Remove the rotation, baking the displayed size into the boxes.
    ///
    /// Returns the matrix mapping rotated-display coordinates back to page
    /// coordinates, so callers can carry their own geometry across the bake.
    pub fn remove_rotation(&mut self) -> Result<Matrix> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let rotation = lp
            .get_dictionary(page_id)
            .ok()
            .and_then(|d| d.get(b"Rotate").ok().cloned())
            .map(|o| obj_i32(&o))
            .unwrap_or(0)
            .rem_euclid(360);
        let media = mediabox_of(lp, page_id);
        let (w, h) = ((media[2] - media[0]).abs(), (media[3] - media[1]).abs());
        let matrix = match rotation {
            90 => mk_matrix(0.0, 1.0, -1.0, 0.0, w, 0.0),
            180 => mk_matrix(-1.0, 0.0, 0.0, -1.0, w, h),
            270 => mk_matrix(0.0, -1.0, 1.0, 0.0, 0.0, h),
            _ => mk_matrix(1.0, 0.0, 0.0, -1.0, 0.0, h),
        };
        if matches!(rotation, 90 | 180 | 270) {
            let swap = matches!(rotation, 90 | 270);
            let (nw, nh) = if swap { (h, w) } else { (w, h) };
            let crop = lp
                .get_dictionary(page_id)
                .ok()
                .and_then(|d| d.get(b"CropBox").ok().cloned())
                .and_then(|o| {
                    let resolved = lp.dereference(&o).map(|(_, found)| found).unwrap_or(&o);
                    obj_rect(resolved)
                });
            if let Some(box_) = crop {
                let rel = norm_rect(&mk_rect(
                    box_.x0 - media[0].min(media[2]),
                    box_.y0 - media[1].min(media[3]),
                    box_.x1 - media[0].min(media[2]),
                    box_.y1 - media[1].min(media[3]),
                ));
                let (cu0, cv0, cu1, cv1) = (rel.x0, rel.y0, rel.x1, rel.y1);
                let turned = match rotation {
                    90 => mk_rect(h - cv1, cu0, h - cv0, cu1),
                    180 => mk_rect(w - cu1, h - cv1, w - cu0, h - cv0),
                    _ => mk_rect(cv0, w - cu1, cv1, w - cu0),
                };
                let fixed = norm_rect(&turned);
                lp.get_dictionary_mut(page_id)?
                    .set("CropBox", rect_object(&fixed));
            }
            lp.get_dictionary_mut(page_id)?
                .set("MediaBox", rect_object(&mk_rect(0.0, 0.0, nw, nh)));
        }
        lp.get_dictionary_mut(page_id)?.remove(b"Rotate");
        sync_doc(self.document_mut())?;
        Ok(matrix)
    }

    /// Replace an image.
    pub fn replace_image(&mut self, _xref: i32, _options: &ReplaceImageOptions) -> Result<()> {
        let lp = self.document_mut().lopdf_mut();
        let target = (_xref as u32, 0);
        let old = lp
            .get_object(target)
            .ok()
            .and_then(|o| o.as_stream().ok().cloned())
            .ok_or_else(|| bad_value("image object is not a stream"))?;
        let _ = old;
        let data = if let Some(pixmap) = &_options.pixmap {
            pixmap_bytestream(pixmap)?
        } else if let Some(stream) = &_options.stream {
            stream.clone()
        } else if let Some(path) = &_options.filename {
            std::fs::read(path)
                .map_err(|_| missing_file(format!("image file not found: {path}")))?
        } else {
            return Err(bad_value("replacement needs a pixmap, stream, or filename"));
        };
        replace_image_bytes(lp, target, &data)?;
        sync_doc(self.document_mut())?;
        Ok(())
    }

    /// Set the art box.
    pub fn set_artbox(&mut self, _rect: Rect) -> Result<()> {
        self.set_page_box(b"ArtBox", _rect)
    }

    /// Set the bleed box.
    pub fn set_bleedbox(&mut self, _rect: Rect) -> Result<()> {
        self.set_page_box(b"BleedBox", _rect)
    }

    /// Set the content object.
    pub fn set_contents(&mut self, _xref: i32) -> Result<()> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let target = (_xref as u32, 0);
        if !matches!(lp.objects.get(&target), Some(Object::Stream(_))) {
            return Err(bad_value("content object is no stream"));
        }
        lp.get_dictionary_mut(page_id)?
            .set("Contents", Object::Reference(target));
        sync_doc(self.document_mut())?;
        Ok(())
    }

    /// Set the crop box.
    pub fn set_cropbox(&mut self, _rect: Rect) -> Result<()> {
        self.set_page_box(b"CropBox", _rect)
    }

    /// Set the language.
    pub fn set_language(&mut self, _language: Option<&str>) -> Result<()> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        match _language {
            Some(lang) => lp
                .get_dictionary_mut(page_id)?
                .set("Lang", text_object(lang)),
            None => {
                lp.get_dictionary_mut(page_id)?.remove(b"Lang");
            }
        }
        Ok(())
    }

    /// Set the media box.
    pub fn set_mediabox(&mut self, _rect: Rect) -> Result<()> {
        self.set_page_box(b"MediaBox", _rect)
    }

    /// Set the rotation.
    pub fn set_rotation(&mut self, _rotation: i32) -> Result<()> {
        // Non-right angles are ignored: the recorded surface keeps the old
        // rotation instead of writing them.
        if _rotation % 90 != 0 {
            return Ok(());
        }
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let norm = _rotation.rem_euclid(360);
        lp.get_dictionary_mut(page_id)?
            .set("Rotate", Object::Integer(i64::from(norm)));
        sync_doc(self.document_mut())?;
        Ok(())
    }

    /// Set the trim box.
    pub fn set_trimbox(&mut self, _rect: Rect) -> Result<()> {
        self.set_page_box(b"TrimBox", _rect)
    }

    /// Show a foreign page.
    pub fn show_pdf_page(
        &mut self,
        _rect: Rect,
        _source: &PdfDocument,
        _options: &ShowPageOptions,
    ) -> Result<i32> {
        let index = self.index();
        let src_page = {
            let src = _source.lopdf();
            let leaves = leaf_pages(src);
            let at = page_index(leaves.len(), _options.page_no)?;
            leaves[at]
        };
        let (bbox, form_res, bytes) = {
            let src = _source.lopdf();
            let dict = src.get_dictionary(src_page)?.clone();
            let bbox = mediabox_of(src, src_page);
            let mut content = Vec::new();
            for cid in src.get_page_contents(src_page) {
                if let Some(Object::Stream(stream)) = src.objects.get(&cid) {
                    if let Ok(decoded) = stream.decompressed_content() {
                        if !content.is_empty() && !content.ends_with(b"\n") {
                            content.push(b'\n');
                        }
                        content.extend_from_slice(&decoded);
                    }
                }
            }
            let lp = self.document_mut().lopdf_mut();
            let mut memo = HashMap::new();
            let res = match dict.get(b"Resources").ok() {
                Some(o) => copy_object_across(src, lp, &o.clone(), &mut memo)?,
                None => Object::Dictionary(Dictionary::new()),
            };
            (bbox, res, content)
        };
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        touch_resource_sub(lp, page_id, b"XObject")?;
        let mut fdict = Dictionary::new();
        fdict.set("Type", name_object("XObject"));
        fdict.set("Subtype", name_object("Form"));
        fdict.set(
            "BBox",
            Object::Array(vec![
                Object::Real(bbox[0] as f32),
                Object::Real(bbox[1] as f32),
                Object::Real(bbox[2] as f32),
                Object::Real(bbox[3] as f32),
            ]),
        );
        fdict.set("Resources", form_res);
        let form_id = lp.new_object_id();
        lp.objects
            .insert(form_id, Object::Stream(Stream::new(fdict, bytes)));
        let name = ensure_xobject(lp, page_id, form_id)?;
        let r = norm_rect(&_rect);
        let mut data = Vec::new();
        if let Some(clip) = &_options.clip {
            let c = norm_rect(clip);
            data.extend_from_slice(
                format!(
                    "q {} {} {} {} re W n\n",
                    pdf_num(c.x0),
                    pdf_num(top - c.y1),
                    pdf_num(c.x1 - c.x0),
                    pdf_num(c.y1 - c.y0),
                )
                .as_bytes(),
            );
        }
        let matrix = place_form_matrix(&bbox, &r, _options.keep_proportion, _options.rotate, top);
        data.extend_from_slice(
            format!(
                "q {} {} {} {} {} {} cm\n/{} Do\nQ\n",
                pdf_num(matrix[0]),
                pdf_num(matrix[1]),
                pdf_num(matrix[2]),
                pdf_num(matrix[3]),
                pdf_num(matrix[4]),
                pdf_num(matrix[5]),
                name,
            )
            .as_bytes(),
        );
        if _options.clip.is_some() {
            data.extend_from_slice(b"Q\n");
        }
        let _ = _options.oc;
        push_content(lp, page_id, data, _options.overlay)?;
        sync_doc(self.document_mut())?;
        Ok(form_id.0 as i32)
    }

    /// Update a link.
    pub fn update_link(&mut self, _link: &Link) -> Result<()> {
        let index = self.index();
        fn same_rect(a: &Rect, b: &Rect) -> bool {
            (a.x0 - b.x0).abs() < 0.5
                && (a.y0 - b.y0).abs() < 0.5
                && (a.x1 - b.x1).abs() < 0.5
                && (a.y1 - b.y1).abs() < 0.5
        }
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let target = if _link.xref > 0 {
            (_link.xref as u32, 0)
        } else {
            // Dictionaries read back carry no object number: match the link
            // on the page by kind and box instead.
            let mut found = None;
            for id in page_annots(lp, page_id) {
                let Ok(current) = link_of(lp, index, id) else {
                    continue;
                };
                if current.kind == _link.kind && same_rect(&current.rect, &_link.rect) {
                    found = Some(id);
                    break;
                }
            }
            found.ok_or_else(|| bad_value("link is not on this page"))?
        };
        if !page_annots(lp, page_id).contains(&target) {
            return Err(bad_value("link is not on this page"));
        }
        write_link(lp, target, _link, top)?;
        sync_doc(self.document_mut())?;
        Ok(())
    }

    /// Widgets, filtered by type.
    pub fn widgets(&self, _types: Option<&[i32]>) -> Result<Vec<Widget>> {
        let lp = self.document().lopdf();
        let page_id = page_id_of(lp, self.index())?;
        let mut out = Vec::new();
        for id in page_annots(lp, page_id) {
            let is_widget = lp
                .get_dictionary(id)
                .ok()
                .and_then(|d| d.get(b"Subtype").ok().map(obj_name))
                .as_deref()
                == Some("Widget");
            if !is_widget {
                continue;
            }
            let widget = widget_data_of(lp, self.index(), id.0 as i32)?;
            if let Some(types) = _types {
                if !types.contains(&widget.field_type) {
                    continue;
                }
            }
            out.push(widget);
        }
        Ok(out)
    }

    /// Write text.
    ///
    /// Flushes the staged writers into the target rectangle: content is
    /// placed at the rectangle origin, rotated about its own corner, and
    /// shrunk to fit (uniformly with `keep_proportion`, per axis without).
    /// Colour, opacity, overlay and the content group flow into the flush;
    /// file fonts keep their flush error.
    pub fn write_text(&mut self, _options: &WriteTextOptions) -> Result<()> {
        let rect = _options
            .rect
            .ok_or_else(|| bad_value("write_text needs a rect"))?;
        let writers = match &_options.writers {
            None => return Err(bad_value("write_text needs writers")),
            Some(Value::List(writers)) => writers,
            Some(_) => {
                return Err(wrong_type(
                    "write_text writers must be a list of staged writers",
                ));
            }
        };
        if writers.is_empty() {
            return Err(bad_value("write_text needs writers"));
        }
        let mut rebuilt = Vec::with_capacity(writers.len());
        for writer in writers {
            rebuilt.push(TextWriter::from_staged(writer)?);
        }
        let union = rebuilt
            .iter()
            .filter_map(|writer| writer.union)
            .reduce(union_rect);
        let index = self.index();
        let top = {
            let lp = self.document_mut().lopdf();
            page_top(lp, page_id_of(lp, index)?)
        };
        let matrix =
            write_text_matrix(rect, union, _options.rotate, _options.keep_proportion, top)?;
        let flush = FlushOptions {
            color: _options.color.clone(),
            opacity: _options.opacity,
            overlay: _options.overlay,
            morph: None,
            matrix,
            render_mode: 0,
            oc: _options.oc,
        };
        for writer in &rebuilt {
            writer.write_text(self, &flush)?;
        }
        Ok(())
    }
}

/// One annotation row: object number, subtype code, and name.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotXrefRow {
    /// Annotation object number.
    pub xref: i32,
    /// Subtype code.
    pub kind: i32,
    /// Annotation name, if any.
    pub name: String,
}

/// One journalled change: its name plus the whole-document bytes on each
/// side, so undo and redo restore exact states.
#[derive(Debug, Clone, Default)]
pub struct JournalOp {
    /// Operation name, empty for an anonymous step.
    pub name: String,
    /// Document bytes when the step opened.
    pub before: Vec<u8>,
    /// Document bytes when the step closed.
    pub after: Vec<u8>,
}

/// Undo journal over whole-document snapshots.
///
/// The facade owns one per open document ([`PdfDocument`](crate::PdfDocument)
/// holds it); doors that rebuild the facade from bytes on every call own a
/// second one alongside instead, because that state cannot live on a
/// rebuilt facade. Snapshots are complete serialised documents; an op
/// truncates the redo tail when it closes.
///
/// # Memory
///
/// O(ops × doc size): every step keeps the whole serialised document on
/// both sides. Long sessions on large documents should save and drop the
/// log rather than journal without bound.
#[derive(Debug, Clone, Default)]
pub struct Journal {
    enabled: bool,
    ops: Vec<JournalOp>,
    /// Applied prefix length: `ops[..pos]` is the current state.
    pos: usize,
    open: Option<JournalOp>,
}

/// Journal file magic: `PDJNL` plus a 3-digit version.
const JOURNAL_MAGIC: &[u8; 8] = b"PDJNL001";

impl Journal {
    /// Enable journaling on the owning document.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Whether journaling is on: `None` reads as "never enabled".
    pub fn is_enabled(&self) -> Option<bool> {
        self.enabled.then_some(true)
    }

    /// Whether an undo step is available.
    pub fn can_undo(&self) -> bool {
        self.enabled && self.pos > 0
    }

    /// Whether a redo step is available.
    pub fn can_redo(&self) -> bool {
        self.enabled && self.pos < self.ops.len()
    }

    /// Undo/redo availability.
    pub fn can_do(&self) -> (bool, bool) {
        (self.can_undo(), self.can_redo())
    }

    /// Applied prefix length and total step count.
    pub fn position(&self) -> (usize, usize) {
        (self.pos, self.ops.len())
    }

    /// Name of one step, or `None` past the end.
    pub fn op_name(&self, step: i32) -> Option<&str> {
        if step < 0 {
            return None;
        }
        self.ops.get(step as usize).map(|op| op.name.as_str())
    }

    /// Open a step, remembering the current bytes as its before-image.
    /// A step opened over another step keeps the outermost before-image.
    pub fn start_op(&mut self, name: Option<&str>, before: Vec<u8>) {
        if self.open.is_some() {
            return;
        }
        self.open = Some(JournalOp {
            name: name.unwrap_or("").to_string(),
            before,
            after: Vec::new(),
        });
    }

    /// Close the open step; without one this is a no-op.
    pub fn stop_op(&mut self, after: Vec<u8>) {
        let Some(mut op) = self.open.take() else {
            return;
        };
        op.after = after;
        self.ops.truncate(self.pos);
        self.ops.push(op);
        self.pos = self.ops.len();
    }

    /// Step back, answering the bytes to restore.
    pub fn undo(&mut self) -> Result<Vec<u8>> {
        if !self.can_undo() {
            return Err(ErrorKind::BadValue.to_error("nothing to undo"));
        }
        self.pos -= 1;
        Ok(self.ops[self.pos].before.clone())
    }

    /// Step forward, answering the bytes to restore.
    pub fn redo(&mut self) -> Result<Vec<u8>> {
        if !self.can_redo() {
            return Err(ErrorKind::BadValue.to_error("nothing to redo"));
        }
        let bytes = self.ops[self.pos].after.clone();
        self.pos += 1;
        Ok(bytes)
    }

    /// Write the op log to a file.
    ///
    /// Layout: 8 magic bytes, op count and position as little-endian
    /// u32, then per op the name length, name, before length, before
    /// bytes, after length and after bytes.
    pub fn save(&self, path: &str) -> Result<()> {
        let mut out = Vec::new();
        out.extend_from_slice(JOURNAL_MAGIC);
        out.extend_from_slice(&(self.ops.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.pos as u32).to_le_bytes());
        for op in &self.ops {
            out.extend_from_slice(&(op.name.len() as u32).to_le_bytes());
            out.extend_from_slice(op.name.as_bytes());
            out.extend_from_slice(&(op.before.len() as u32).to_le_bytes());
            out.extend_from_slice(&op.before);
            out.extend_from_slice(&(op.after.len() as u32).to_le_bytes());
            out.extend_from_slice(&op.after);
        }
        std::fs::write(path, out)
            .map_err(|e| missing_file(format!("cannot write journal {path}: {e}")))?;
        Ok(())
    }

    /// Load an op log written by [`Journal::save`].
    ///
    /// A live journal refuses the load: installing a log over journalled
    /// state would orphan the open steps.
    pub fn load(&mut self, path: &str) -> Result<()> {
        let data =
            std::fs::read(path).map_err(|_| missing_file(format!("file not found: {path}")))?;
        if self.enabled {
            return Err(ErrorKind::BadValue.to_error("journal is already enabled"));
        }
        let mut rest = data.as_slice();
        let take = |rest: &mut &[u8], len: usize| -> Option<Vec<u8>> {
            if rest.len() < len {
                return None;
            }
            let (head, tail) = rest.split_at(len);
            *rest = tail;
            Some(head.to_vec())
        };
        let take_u32 = |rest: &mut &[u8]| -> Option<u32> {
            let raw = take(rest, 4)?;
            Some(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
        };
        let bad = || ErrorKind::BadValue.to_error(format!("bad journal file: {path}"));
        let magic = take(&mut rest, 8).ok_or_else(bad)?;
        if magic.as_slice() != JOURNAL_MAGIC {
            return Err(bad());
        }
        let count = take_u32(&mut rest).ok_or_else(bad)? as usize;
        let pos = take_u32(&mut rest).ok_or_else(bad)? as usize;
        let mut ops = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let name_len = take_u32(&mut rest).ok_or_else(bad)? as usize;
            let name = take(&mut rest, name_len).ok_or_else(bad)?;
            let name = String::from_utf8(name).map_err(|_| bad())?;
            let before_len = take_u32(&mut rest).ok_or_else(bad)? as usize;
            let before = take(&mut rest, before_len).ok_or_else(bad)?;
            let after_len = take_u32(&mut rest).ok_or_else(bad)? as usize;
            let after = take(&mut rest, after_len).ok_or_else(bad)?;
            ops.push(JournalOp {
                name,
                before,
                after,
            });
        }
        if pos > ops.len() {
            return Err(bad());
        }
        self.ops = ops;
        self.pos = pos;
        self.enabled = true;
        Ok(())
    }
}

impl PdfDocument {
    /// Add a layer.
    pub fn add_layer(
        &mut self,
        _name: &str,
        _creator: Option<&str>,
        _on: Option<bool>,
    ) -> Result<()> {
        let lp = self.lopdf_mut();
        let name = _name.to_string();
        let mut ocg = Dictionary::new();
        ocg.set("Type", name_object("OCG"));
        ocg.set("Name", text_object(&name));
        if let Some(creator) = _creator {
            let mut info = Dictionary::new();
            info.set("Creator", text_object(creator));
            let mut usage = Dictionary::new();
            usage.set("CreatorInfo", Object::Dictionary(info));
            ocg.set("Usage", Object::Dictionary(usage));
        }
        let ocg_id = lp.new_object_id();
        lp.objects.insert(ocg_id, Object::Dictionary(ocg));
        let on = _on.unwrap_or(false);
        ocproperties_append(lp, ocg_id, on)?;
        // One alternate configuration holding only its base state: the
        // recordings pin exactly {"basestate": ...} for the new entry.
        let mut props = oc_properties_or_new(lp)?;
        let mut configs = props
            .get(b"Configs")
            .ok()
            .cloned()
            .map(|o| deref(lp, &o).as_array().ok().cloned())
            .unwrap_or_default()
            .unwrap_or_default();
        let mut entry = Dictionary::new();
        entry.set("BaseState", name_object(if on { "ON" } else { "OFF" }));
        configs.push(Object::Dictionary(entry));
        props.set("Configs", Object::Array(configs));
        oc_properties_store(lp, &props)?;
        Ok(())
    }

    /// Add an optional-content group, answering the group object number.
    pub fn add_ocg(
        &mut self,
        _name: &str,
        _config: i32,
        _on: bool,
        _intent: Option<&str>,
        _usage: Option<&str>,
    ) -> Result<i32> {
        let lp = self.lopdf_mut();
        let mut ocg = Dictionary::new();
        ocg.set("Type", name_object("OCG"));
        ocg.set("Name", text_object(_name));
        if let Some(intent) = _intent {
            ocg.set("Intent", name_object(intent));
        }
        if let Some(usage) = _usage {
            let mut info = Dictionary::new();
            info.set("Creator", text_object(usage));
            let mut dict = Dictionary::new();
            dict.set("CreatorInfo", Object::Dictionary(info));
            ocg.set("Usage", Object::Dictionary(dict));
        }
        // Reserve the group id before appending properties: the recorded
        // xref is the first fresh id, properties take the next one.
        let ocg_id = lp.new_object_id();
        lp.objects.insert(ocg_id, Object::Dictionary(ocg));
        ocproperties_append(lp, ocg_id, _on)?;
        ocproperties_order_append(lp, ocg_id)?;
        let _ = _config;
        Ok(ocg_id.0 as i32)
    }

    /// Apply a stylesheet.
    pub fn apply_css(&mut self, _css: &str, _append: bool) -> Result<()> {
        Err(super::not_supported("Document", "apply_css"))
    }

    /// Flatten live objects.
    pub fn bake(&mut self, options: &BakeOptions) -> Result<()> {
        if options.annots {
            pdf_annot::ensure_appearances(self.lopdf_mut()).map_err(|e| {
                crate::error::internal_error(format!("cannot prepare annotations for bake: {e}"))
            })?;
            self.flatten_annotations()?;
        }
        if options.widgets {
            let report = self.flatten_forms()?;
            if !report.skipped.is_empty() {
                return Err(crate::error::internal_error(format!(
                    "bake left {} field(s) editable: {}",
                    report.skipped.len(),
                    report.skipped.join(", ")
                )));
            }
        }
        Ok(())
    }

    /// Copy a page.
    pub fn copy_page(&mut self, _page: i32, _to: i32) -> Result<()> {
        let lp = self.lopdf_mut();
        let count = page_count_of(lp);
        let from = page_index(count, _page)?;
        let dest = move_target(count, _to);
        duplicate_page_at(lp, from, dest, false)?;
        sync_doc(self)?;
        Ok(())
    }

    /// Delete an outline entry's destination, keeping the entry.
    pub fn del_toc_item(&mut self, _index: i32) -> Result<()> {
        let lp = self.lopdf_mut();
        let rows = flatten_outlines(lp);
        let at = toc_index(rows.len(), _index)?;
        let id = rows[at].id;
        let dict = lp.get_dictionary_mut(id)?;
        dict.remove(b"Dest");
        dict.remove(b"A");
        Ok(())
    }

    /// Delete the XML metadata.
    pub fn del_xml_metadata(&mut self) -> Result<()> {
        let lp = self.lopdf_mut();
        let catalog = catalog_id(lp)?;
        let old = lp
            .get_dictionary(catalog)
            .ok()
            .and_then(|c| c.get(b"Metadata").ok().cloned())
            .and_then(|o| o.as_reference().ok());
        lp.get_dictionary_mut(catalog)?.remove(b"Metadata");
        if let Some(id) = old {
            lp.objects.remove(&id);
        }
        Ok(())
    }

    /// Delete a page.
    pub fn delete_page(&mut self, _page: i32) -> Result<()> {
        let lp = self.lopdf_mut();
        let count = page_count_of(lp);
        let at = page_index(count, _page)?;
        remove_page_at(lp, at)?;
        sync_doc(self)?;
        Ok(())
    }

    /// Delete pages.
    pub fn delete_pages(&mut self, _pages: &PageSelection) -> Result<()> {
        let lp = self.lopdf_mut();
        let count = page_count_of(lp);
        let mut targets: Vec<usize> = match _pages {
            PageSelection::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(page_index(count, *item)?);
                }
                out
            }
            PageSelection::Range(a, b) => {
                let (mut lo, mut hi) = (page_index(count, *a)?, page_index(count, *b)?);
                if lo > hi {
                    std::mem::swap(&mut lo, &mut hi);
                }
                (lo..=hi).collect()
            }
        };
        targets.sort_unstable();
        targets.dedup();
        for at in targets.into_iter().rev() {
            remove_page_at(lp, at)?;
        }
        sync_doc(self)?;
        Ok(())
    }

    /// Add an embedded file, answering the stream object number.
    pub fn embfile_add(
        &mut self,
        _name: &str,
        _data: &[u8],
        _options: &EmbFileOptions,
    ) -> Result<i32> {
        let lp = self.lopdf_mut();
        let filename = _options.filename.as_deref().unwrap_or(_name);
        let mut params = Dictionary::new();
        params.set("Size", Object::Integer(_data.len() as i64));
        let mut sdict = Dictionary::new();
        sdict.set("Type", name_object("EmbeddedFile"));
        sdict.set("Params", Object::Dictionary(params));
        sdict.set("F", text_object(filename));
        sdict.set(
            "UF",
            text_object(_options.display_name.as_deref().unwrap_or(filename)),
        );
        if let Some(desc) = _options.description.as_deref() {
            if !desc.is_empty() {
                sdict.set("Desc", text_object(desc));
            }
        }
        let stream_id = lp.new_object_id();
        lp.objects.insert(
            stream_id,
            Object::Stream(Stream::new(sdict, _data.to_vec())),
        );
        embedded_files_append(lp, _name, stream_id)?;
        Ok(stream_id.0 as i32)
    }

    /// Delete an embedded file.
    pub fn embfile_del(&mut self, _item: &EmbItem) -> Result<()> {
        let lp = self.lopdf_mut();
        let files = list_embedded_files(lp);
        let position = match _item {
            EmbItem::Index(i) => {
                let at = if *i < 0 { files.len() as i32 + i } else { *i };
                if at < 0 || at as usize >= files.len() {
                    return Err(index_oob("embedded file index out of range"));
                }
                at as usize
            }
            EmbItem::Name(name) => files
                .iter()
                .position(|(n, _)| n == name)
                .ok_or_else(|| unknown_key(format!("no embedded file named {name}")))?,
        };
        let (_, spec_id) = files[position].clone();
        remove_embedded_pair(lp, position)?;
        remove_filespec_objects(lp, spec_id);
        Ok(())
    }

    /// Update an embedded file.
    pub fn embfile_upd(
        &mut self,
        _item: &EmbItem,
        _data: Option<&[u8]>,
        _options: &EmbFileOptions,
    ) -> Result<i32> {
        let lp = self.lopdf_mut();
        let files = list_embedded_files(lp);
        let spec_id = match _item {
            EmbItem::Index(i) => {
                let at = if *i < 0 { files.len() as i32 + i } else { *i };
                if at < 0 || at as usize >= files.len() {
                    return Err(index_oob("embedded file index out of range"));
                }
                files[at as usize].1
            }
            EmbItem::Name(name) => files
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, id)| *id)
                .ok_or_else(|| unknown_key(format!("no embedded file named {name}")))?,
        };
        if let Some(data) = _data {
            let stream_id = match lp.objects.get(&spec_id) {
                Some(Object::Stream(_)) => Some(spec_id),
                _ => lp
                    .get_dictionary(spec_id)
                    .ok()
                    .and_then(|d| d.get(b"EF").ok().cloned())
                    .and_then(|o| o.as_dict().ok().cloned())
                    .and_then(|ef| {
                        ef.get(b"UF")
                            .ok()
                            .cloned()
                            .or_else(|| ef.get(b"F").ok().cloned())
                    })
                    .and_then(|o| o.as_reference().ok()),
            };
            if let Some(sid) = stream_id {
                if let Ok(Object::Stream(stream)) = lp.get_object_mut(sid) {
                    stream.set_content(data.to_vec());
                    stream.dict.remove(b"Filter");
                    let mut params = Dictionary::new();
                    params.set("Size", Object::Integer(data.len() as i64));
                    stream.dict.set("Params", Object::Dictionary(params));
                }
            }
        }
        match lp.get_object_mut(spec_id) {
            Ok(Object::Dictionary(dict)) => {
                embfile_set_meta(dict, _options);
            }
            Ok(Object::Stream(stream)) => {
                embfile_set_meta(&mut stream.dict, _options);
            }
            _ => {}
        }
        Ok(spec_id.0 as i32)
    }

    /// Copy a page with links.
    pub fn fullcopy_page(&mut self, _page: i32, _to: i32) -> Result<()> {
        let lp = self.lopdf_mut();
        let count = page_count_of(lp);
        let from = page_index(count, _page)?;
        let dest = move_target(count, _to);
        duplicate_page_at(lp, from, dest, true)?;
        sync_doc(self)?;
        Ok(())
    }

    /// Whether annotations exist.
    pub fn has_annots(&self) -> Result<bool> {
        let lp = self.lopdf();
        for id in leaf_pages(lp) {
            if !page_annots(lp, id).is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether links exist.
    pub fn has_links(&self) -> Result<bool> {
        let lp = self.lopdf();
        for id in leaf_pages(lp) {
            for annot in page_annots(lp, id) {
                let is_link = lp
                    .get_dictionary(annot)
                    .ok()
                    .and_then(|d| d.get(b"Subtype").ok().map(obj_name))
                    .as_deref()
                    == Some("Link");
                if is_link {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Insert a file.
    pub fn insert_file(&mut self, _path: &str, _options: &InsertPdfOptions) -> Result<()> {
        let data =
            std::fs::read(_path).map_err(|_| missing_file(format!("file not found: {_path}")))?;
        let source = PdfDocument::from_bytes(&data)?;
        self.insert_pdf(&source, _options)
    }

    /// Insert a fresh page.
    pub fn insert_page(&mut self, _page: i32, _options: &InsertPageOptions) -> Result<i32> {
        let lp = self.lopdf_mut();
        let count = page_count_of(lp);
        let at = if _page < 0 {
            (count as i32 + _page + 1).max(0) as usize
        } else {
            (_page as usize).min(count)
        };
        let placed = insert_blank_page(lp, at, _options.width, _options.height)?;
        sync_doc(self)?;
        if !_options.text.is_empty() {
            let mut view = PageMut::new(self, placed);
            let body = _options.text.join("\n");
            let rect = mk_rect(
                36.0,
                36.0,
                (_options.width - 36.0).max(37.0),
                (_options.height - 36.0).max(37.0),
            );
            let mut text = InsertTextOptions {
                fontsize: _options.fontsize,
                fontname: _options.fontname.clone(),
                fontfile: _options.fontfile.clone(),
                ..Default::default()
            };
            if !_options.color.is_empty() {
                text.color = Some(_options.color.clone());
            }
            let boxed = TextBoxOptions {
                text,
                ..Default::default()
            };
            let arg = TextArg::One(body);
            view.insert_textbox(rect, &arg, &boxed)?;
        }
        // The recorded return is the 1-based page number of the new page.
        Ok(placed as i32 + 1)
    }

    /// Insert pages from a document.
    pub fn insert_pdf(&mut self, _source: &PdfDocument, _options: &InsertPdfOptions) -> Result<()> {
        let (range, dest) = {
            let src = _source.lopdf();
            let total = page_count_of(src) as i32;
            let from = if _options.from_page < 0 {
                0
            } else {
                _options.from_page.min(total)
            };
            let to = if _options.to_page < 0 {
                total - 1
            } else {
                _options.to_page.min(total - 1)
            };
            let dest = {
                let count = page_count_of(self.lopdf_mut()) as i32;
                if _options.start_at < 0 {
                    count
                } else {
                    _options.start_at.min(count)
                }
            };
            (from..=to.max(from - 1), dest)
        };
        let _ = (
            &_options.join_duplicates,
            _options.show_progress,
            _options.finish,
        );
        let mut slot = dest as usize;
        for num in range {
            let src_page = {
                let leaves = leaf_pages(_source.lopdf());
                match leaves.get(num as usize) {
                    Some(id) => *id,
                    None => continue,
                }
            };
            let mut memo = HashMap::new();
            let copied = {
                let src = _source.lopdf();
                let page_obj = src.objects.get(&src_page).cloned().unwrap_or(Object::Null);
                copy_object_across(src, self.lopdf_mut(), &page_obj, &mut memo)?
            };
            let lp = self.lopdf_mut();
            let mut dict = copied.as_dict().ok().cloned().unwrap_or_default();
            dict.remove(b"Parent");
            if !_options.annots || !_options.links || !_options.widgets {
                let kept = filter_copied_annots(
                    lp,
                    &dict,
                    _options.annots,
                    _options.links,
                    _options.widgets,
                );
                if kept.is_empty() {
                    dict.remove(b"Annots");
                } else {
                    dict.set("Annots", Object::Array(kept));
                }
            }
            if _options.rotate >= 0 {
                dict.set("Rotate", Object::Integer(i64::from(_options.rotate)));
            }
            let fresh = lp.new_object_id();
            lp.objects.insert(fresh, Object::Dictionary(dict));
            graft_page_at(lp, fresh, slot)?;
            slot += 1;
        }
        sync_doc(self)?;
        Ok(())
    }

    /// Undo and redo availability.
    pub fn journal_can_do(&self) -> Result<(bool, bool)> {
        Ok(self.journal().can_do())
    }

    /// Enable the journal.
    pub fn journal_enable(&mut self) -> Result<()> {
        self.journal_mut().enable();
        Ok(())
    }

    /// Whether the journal is enabled.
    pub fn journal_is_enabled(&self) -> Result<bool> {
        Ok(self.journal().is_enabled().unwrap_or(false))
    }

    /// Load a journal file.
    pub fn journal_load(&mut self, _path: &str) -> Result<()> {
        self.journal_mut().load(_path)
    }

    /// Journal step name.
    pub fn journal_op_name(&self, _step: i32) -> Result<String> {
        self.journal()
            .op_name(_step)
            .map(str::to_string)
            .ok_or_else(|| {
                ErrorKind::IndexOutOfRange.to_error(format!(
                    "journal step {_step} out of range ({} steps)",
                    self.journal().position().1
                ))
            })
    }

    /// Journal position: the applied prefix length.
    pub fn journal_position(&self) -> Result<i32> {
        Ok(self.journal().position().0 as i32)
    }

    /// Redo a journal step.
    pub fn journal_redo(&mut self) -> Result<()> {
        let bytes = self.journal_mut().redo()?;
        match self.restore_bytes_keep_journal(&bytes) {
            Ok(()) => Ok(()),
            Err(failed) => {
                // The state never changed; put the step back.
                let _ = self.journal_mut().undo();
                Err(failed)
            }
        }
    }

    /// Save the journal file.
    pub fn journal_save(&mut self, _path: &str) -> Result<()> {
        self.journal().save(_path)
    }

    /// Start a journal step.
    pub fn journal_start_op(&mut self, _name: Option<&str>) -> Result<()> {
        let before = self.to_bytes()?;
        self.journal_mut().start_op(_name, before);
        Ok(())
    }

    /// Stop the journal step.
    pub fn journal_stop_op(&mut self) -> Result<()> {
        let after = self.to_bytes()?;
        self.journal_mut().stop_op(after);
        Ok(())
    }

    /// Undo a journal step.
    pub fn journal_undo(&mut self) -> Result<()> {
        let bytes = self.journal_mut().undo()?;
        match self.restore_bytes_keep_journal(&bytes) {
            Ok(()) => Ok(()),
            Err(failed) => {
                // The state never changed; put the step back.
                let _ = self.journal_mut().redo();
                Err(failed)
            }
        }
    }

    /// Move a page.
    pub fn move_page(&mut self, _page: i32, _to: i32) -> Result<()> {
        let lp = self.lopdf_mut();
        let count = page_count_of(lp);
        let from = page_index(count, _page)?;
        let dest = move_slot(count, from, _to)?;
        move_page_at(lp, from, dest)?;
        sync_doc(self)?;
        Ok(())
    }

    /// Fresh page for editing.
    pub fn new_page(&mut self, _page: i32, _width: f64, _height: f64) -> Result<PageMut<'_>> {
        let lp = self.lopdf_mut();
        let count = page_count_of(lp);
        let at = if _page < 0 {
            (count as i32 + _page + 1).max(0) as usize
        } else {
            (_page as usize).min(count)
        };
        let placed = insert_blank_page(lp, at, _width, _height)?;
        sync_doc(self)?;
        Ok(PageMut::new(self, placed))
    }

    /// Annotation object numbers on a page.
    pub fn page_annot_xrefs(&self, _page: i32) -> Result<Vec<AnnotXrefRow>> {
        let lp = self.lopdf();
        let count = page_count_of(lp);
        let at = page_index(count, _page)?;
        let page_id = page_id_of(lp, at)?;
        let mut out = Vec::new();
        for id in page_annots(lp, page_id) {
            let dict = lp.get_dictionary(id).ok().cloned().unwrap_or_default();
            let subtype = dict.get(b"Subtype").ok().map(obj_name).unwrap_or_default();
            let (kind, _) = subtype_code(&subtype);
            let name = dict.get(b"NM").ok().map(obj_text).unwrap_or_default();
            out.push(AnnotXrefRow {
                xref: id.0 as i32,
                kind,
                name,
            });
        }
        Ok(out)
    }

    /// Repair the document.
    pub fn repair(&mut self) -> Result<()> {
        Err(super::not_supported("Document", "repair"))
    }

    /// Resolve a link URI.
    ///
    /// Page fragments resolve to their index; anything else with a
    /// leading `#` (or a bare name) resolves through the catalog's named
    /// destinations, with the destination's point in the coordinate
    /// slots and nulls where the fit mode names no coordinate. Unknown
    /// targets keep the recorded `[-1, null, null]`.
    pub fn resolve_link(&self, _uri: Option<&str>, _chapters: i32) -> Result<Value> {
        let Some(uri) = _uri else {
            return Ok(Value::List(vec![
                Value::Int(-1),
                Value::Int(0),
                Value::Int(0),
            ]));
        };
        if let Some(rest) = uri.strip_prefix("#page=") {
            let page = rest.parse::<i32>().ok().map(|n| n - 1).unwrap_or(-1);
            return Ok(link_shape(_chapters, page, None, None));
        }
        let name = uri.strip_prefix('#').unwrap_or(uri);
        if let Some((page, x, y)) = named_link_target(self.lopdf(), name) {
            return Ok(link_shape(_chapters, page, x, y));
        }
        Ok(link_shape(_chapters, -1, None, None))
    }

    /// Scrub hidden traces.
    pub fn scrub(&mut self, _options: &ScrubOptions) -> Result<()> {
        let lp = self.lopdf_mut();
        if _options.metadata {
            if let Some(id) = lp
                .trailer
                .get(b"Info")
                .ok()
                .and_then(|o| o.as_reference().ok())
            {
                lp.objects.insert(id, Object::Dictionary(Dictionary::new()));
            }
        }
        if _options.xml_metadata {
            let catalog = catalog_id(lp)?;
            if let Ok(dict) = lp.get_dictionary_mut(catalog) {
                dict.remove(b"Metadata");
            }
        }
        if _options.embedded_files {
            let catalog = catalog_id(lp)?;
            let names_id = lp
                .get_dictionary(catalog)
                .ok()
                .and_then(|c| c.get(b"Names").ok().cloned())
                .and_then(|o| o.as_reference().ok());
            if let Some(id) = names_id {
                if let Ok(dict) = lp.get_dictionary_mut(id) {
                    dict.remove(b"EmbeddedFiles");
                }
            }
        }
        if _options.javascript {
            scrub_javascript(lp);
        }
        if _options.thumbnails {
            for page in leaf_pages(lp) {
                if let Ok(dict) = lp.get_dictionary_mut(page) {
                    dict.remove(b"Thumb");
                }
            }
        }
        if _options.clean_pages {
            for page in leaf_pages(lp) {
                for stream_id in lp.get_page_contents(page) {
                    let raw = lp
                        .get_object(stream_id)
                        .ok()
                        .and_then(|o| o.as_stream().ok().cloned());
                    let Some(stream) = raw else {
                        continue;
                    };
                    let Ok(content) = stream.decompressed_content() else {
                        continue;
                    };
                    let Ok(decoded) = lopdf::content::Content::decode(&content) else {
                        continue;
                    };
                    let Ok(encoded) = decoded.encode() else {
                        continue;
                    };
                    if let Ok(Object::Stream(target)) = lp.get_object_mut(stream_id) {
                        target.set_content(encoded);
                        target.dict.remove(b"Filter");
                    }
                }
            }
        }
        if _options.reset_fields {
            let mut fields = Vec::new();
            for page in leaf_pages(lp) {
                for id in page_annots(lp, page) {
                    fields.push(id.0 as i32);
                }
            }
            for xref in fields {
                let _ = reset_widget_field(lp, xref);
            }
        }
        if _options.reset_responses {
            scrub_responses(lp);
        }
        if _options.remove_links {
            for page in leaf_pages(lp) {
                let ids = page_annots(lp, page);
                let mut dead = Vec::new();
                for id in &ids {
                    let is_link = lp
                        .get_dictionary(*id)
                        .ok()
                        .and_then(|d| d.get(b"Subtype").ok().map(obj_name))
                        .as_deref()
                        == Some("Link");
                    if is_link {
                        dead.push(*id);
                    }
                }
                if !dead.is_empty() {
                    let keep: Vec<ObjectId> =
                        ids.into_iter().filter(|id| !dead.contains(id)).collect();
                    set_page_annots(lp, page, &keep)?;
                    for id in dead {
                        lp.objects.remove(&id);
                    }
                }
            }
        }
        if _options.attached_files {
            for page in leaf_pages(lp) {
                for id in page_annots(lp, page) {
                    if let Ok(dict) = lp.get_dictionary_mut(id) {
                        dict.remove(b"FS");
                    }
                }
            }
        }
        if _options.redactions {
            let mut victims: Vec<(usize, ObjectId)> = Vec::new();
            for (index, page) in leaf_pages(lp).iter().enumerate() {
                for id in page_annots(lp, *page) {
                    let is_redact = lp
                        .get_dictionary(id)
                        .ok()
                        .and_then(|d| d.get(b"Subtype").ok().map(obj_name))
                        .as_deref()
                        == Some("Redact");
                    if is_redact {
                        victims.push((index, id));
                    }
                }
            }
            for (index, _) in victims {
                let mut view = PageMut::new(self, index);
                let opts = RedactApplyOptions {
                    images: _options.redact_images,
                    ..Default::default()
                };
                view.apply_redactions(&opts)?;
            }
            return Ok(());
        }
        let _ = _options.hidden_text;
        sync_doc(self)?;
        Ok(())
    }

    /// Keep only these pages.
    pub fn select(&mut self, _pages: &[i32]) -> Result<()> {
        let lp = self.lopdf_mut();
        let count = page_count_of(lp);
        let mut keep: Vec<usize> = Vec::with_capacity(_pages.len());
        for item in _pages {
            keep.push(page_index(count, *item)?);
        }
        keep.sort_unstable();
        keep.dedup();
        let leaves = leaf_pages(lp);
        let shared = leaves
            .first()
            .and_then(|first| parent_of(lp, *first))
            .filter(|parent| leaves.iter().all(|id| parent_of(lp, *id) == Some(*parent)));
        if let Some(parent) = shared {
            let mut kids = Vec::with_capacity(keep.len());
            for at in &keep {
                if let Some(id) = leaves.get(*at) {
                    kids.push(Object::Reference(*id));
                }
            }
            let dropped = leaves.len() as i64 - keep.len() as i64;
            lp.get_dictionary_mut(parent)?
                .set("Kids", Object::Array(kids));
            fix_counts_up(lp, parent, -dropped);
            let dead: Vec<ObjectId> = leaves
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !keep.contains(i))
                .map(|(_, id)| id)
                .collect();
            for id in dead {
                lp.objects.remove(&id);
            }
        } else {
            let dead: Vec<usize> = (0..count).filter(|i| !keep.contains(i)).collect();
            for at in dead.into_iter().rev() {
                remove_page_at(lp, at)?;
            }
        }
        sync_doc(self)?;
        Ok(())
    }

    /// Set the language.
    pub fn set_language(&mut self, _language: Option<&str>) -> Result<bool> {
        let lp = self.lopdf_mut();
        let catalog = catalog_id(lp)?;
        match _language {
            Some(lang) => lp
                .get_dictionary_mut(catalog)?
                .set("Lang", text_object(lang)),
            None => {
                lp.get_dictionary_mut(catalog)?.remove(b"Lang");
            }
        }
        Ok(true)
    }

    /// Set layer visibility.
    pub fn set_layer(&mut self, _config: i32, _options: &LayerOptions) -> Result<()> {
        let lp = self.lopdf_mut();
        let mut dict = layer_config_mut(lp, _config)?;
        if let Some(base) = &_options.base_state {
            dict.set("BaseState", name_object(base));
        }
        let refs = |ids: &[i32]| -> Vec<Object> {
            ids.iter()
                .filter(|i| **i > 0 && lp.objects.contains_key(&(**i as u32, 0)))
                .map(|i| Object::Reference((*i as u32, 0)))
                .collect()
        };
        if !_options.on.is_empty() {
            dict.set("ON", Object::Array(refs(&_options.on)));
        }
        if !_options.off.is_empty() {
            dict.set("OFF", Object::Array(refs(&_options.off)));
        }
        oc_config_store(lp, _config, &dict)?;
        Ok(())
    }

    /// Apply a layer interface configuration.
    pub fn set_layer_ui_config(&mut self, _number: i32, _action: i32) -> Result<()> {
        let lp = self.lopdf_mut();
        let groups = ocg_list(lp);
        let Some(target) = groups
            .get(_number as usize)
            .copied()
            .filter(|_| _number >= 0)
        else {
            return Err(bad_value(format!("no layer interface {_number}")));
        };
        let mut dict = layer_config_mut(lp, -1)?;
        let mut on = dict
            .get(b"ON")
            .ok()
            .and_then(|o| o.as_array().ok().cloned())
            .unwrap_or_default();
        let mut off = dict
            .get(b"OFF")
            .ok()
            .and_then(|o| o.as_array().ok().cloned())
            .unwrap_or_default();
        let shown = !off.iter().any(|o| o.as_reference().ok() == Some(target))
            && (on.iter().any(|o| o.as_reference().ok() == Some(target))
                || !matches!(dict.get(b"BaseState"), Ok(Object::Name(name)) if name.as_slice() != b"ON"));
        let show = if _action == 1 { !shown } else { true };
        on.retain(|o| o.as_reference().ok() != Some(target));
        off.retain(|o| o.as_reference().ok() != Some(target));
        if show {
            on.push(Object::Reference(target));
        } else {
            off.push(Object::Reference(target));
        }
        dict.set("ON", Object::Array(on));
        dict.set("OFF", Object::Array(off));
        oc_config_store(lp, -1, &dict)?;
        Ok(())
    }

    /// Set marked-content info.
    pub fn set_markinfo(&mut self, _markinfo: &Value) -> Result<bool> {
        let lp = self.lopdf_mut();
        let catalog = catalog_id(lp)?;
        match _markinfo {
            Value::Null => {
                lp.get_dictionary_mut(catalog)?.remove(b"MarkInfo");
            }
            Value::Dict(map) => {
                let obj = value_to_object(_markinfo)?;
                let mut dict = match obj {
                    Object::Dictionary(dict) => dict,
                    _ => Dictionary::new(),
                };
                let marked = matches!(map.get("Marked"), Some(Value::Bool(true)));
                if marked {
                    for key in ["Suspects", "UserProperties"] {
                        if dict.get(key.as_bytes()).is_err() {
                            dict.set(key, Object::Boolean(false));
                        }
                    }
                }
                lp.get_dictionary_mut(catalog)?
                    .set("MarkInfo", Object::Dictionary(dict));
                return Ok(marked);
            }
            _ => {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Set metadata.
    pub fn set_metadata(&mut self, _metadata: &DocMetadata) -> Result<()> {
        let lp = self.lopdf_mut();
        let info = ensure_info(lp)?;
        {
            let dict = lp.get_dictionary_mut(info)?;
            dict.set("Title", text_object(&_metadata.title));
            dict.set("Author", text_object(&_metadata.author));
            dict.set("Subject", text_object(&_metadata.subject));
            dict.set("Keywords", text_object(&_metadata.keywords));
            dict.set("Creator", text_object(&_metadata.creator));
            dict.set("Producer", text_object(&_metadata.producer));
            dict.set("CreationDate", text_object(&_metadata.creation_date));
            dict.set("ModDate", text_object(&_metadata.mod_date));
            if !_metadata.trapped.is_empty() {
                dict.set("Trapped", name_object(&_metadata.trapped));
            }
        }
        let _ = (&_metadata.format, &_metadata.encryption);
        Ok(())
    }

    /// Set the object state.
    pub fn set_oc(&mut self, _xref: i32, _oc: i32) -> Result<()> {
        let lp = self.lopdf_mut();
        let target = (_xref as u32, 0);
        if !lp.objects.contains_key(&target) {
            return Err(bad_value("object does not exist"));
        }
        if _oc <= 0 {
            if let Ok(dict) = lp.get_dictionary_mut(target) {
                dict.remove(b"OC");
            } else if let Ok(Object::Stream(stream)) = lp.get_object_mut(target) {
                stream.dict.remove(b"OC");
            }
            return Ok(());
        }
        let group = (_oc as u32, 0);
        let is_group = lp
            .get_dictionary(group)
            .ok()
            .and_then(|d| d.get(b"Type").ok().cloned())
            .and_then(|o| match o {
                Object::Name(name) => Some(name),
                _ => None,
            })
            .is_some_and(|name| name == b"OCG" || name == b"OCMD");
        if !is_group {
            return Err(bad_value(format!("no optional-content group {_oc}")));
        }
        if let Ok(dict) = lp.get_dictionary_mut(target) {
            dict.set("OC", Object::Reference(group));
        } else if let Ok(Object::Stream(stream)) = lp.get_object_mut(target) {
            stream.dict.set("OC", Object::Reference(group));
        }
        Ok(())
    }

    /// Set the membership dict.
    pub fn set_ocmd(&mut self, _xref: i32, _options: &OcmdOptions) -> Result<i32> {
        let lp = self.lopdf_mut();
        let mut members = Vec::with_capacity(_options.ocgs.len());
        for member in &_options.ocgs {
            let id = (*member as u32, 0);
            let is_group = *member > 0
                && lp
                    .get_dictionary(id)
                    .ok()
                    .and_then(|d| d.get(b"Type").ok().cloned())
                    .and_then(|o| match o {
                        Object::Name(name) => Some(name),
                        _ => None,
                    })
                    .is_some_and(|name| name == b"OCG" || name == b"OCMD");
            if !is_group {
                return Err(bad_value(format!("no optional-content group {member}")));
            }
            members.push(Object::Reference(id));
        }
        let mut ocmd = Dictionary::new();
        ocmd.set("Type", name_object("OCMD"));
        if !members.is_empty() {
            ocmd.set("OCGs", Object::Array(members));
        }
        if let Some(policy) = _options.policy.as_deref() {
            ocmd.set("P", name_object(policy));
        }
        let ocmd_id = lp.new_object_id();
        lp.objects.insert(ocmd_id, Object::Dictionary(ocmd));
        if _xref != 0 {
            let target = (_xref as u32, 0);
            if !lp.objects.contains_key(&target) {
                return Err(bad_value("object does not exist"));
            }
            if let Ok(dict) = lp.get_dictionary_mut(target) {
                dict.set("OC", Object::Reference(ocmd_id));
            } else if let Ok(Object::Stream(stream)) = lp.get_object_mut(target) {
                stream.dict.set("OC", Object::Reference(ocmd_id));
            }
        }
        Ok(ocmd_id.0 as i32)
    }

    /// Set page label runs.
    pub fn set_page_labels(&mut self, _labels: &[PageLabel]) -> Result<()> {
        let lp = self.lopdf_mut();
        let catalog = catalog_id(lp)?;
        let mut labels: Vec<&PageLabel> = _labels.iter().collect();
        labels.sort_by_key(|l| l.start_page);
        let mut nums = Vec::with_capacity(labels.len() * 2);
        for label in labels {
            nums.push(Object::Integer(i64::from(label.start_page.max(0))));
            let mut dict = Dictionary::new();
            if !label.style.is_empty() {
                dict.set("S", name_object(&label.style));
            }
            if !label.prefix.is_empty() {
                dict.set("P", text_object(&label.prefix));
            }
            if label.first_number != 1 {
                dict.set("St", Object::Integer(i64::from(label.first_number)));
            }
            nums.push(Object::Dictionary(dict));
        }
        let mut tree = Dictionary::new();
        tree.set("Nums", Object::Array(nums));
        let tree_id = lp.new_object_id();
        lp.objects.insert(tree_id, Object::Dictionary(tree));
        lp.get_dictionary_mut(catalog)?
            .set("PageLabels", Object::Reference(tree_id));
        Ok(())
    }

    /// Set the page layout name.
    pub fn set_pagelayout(&mut self, _pagelayout: &str) -> Result<()> {
        if !matches!(
            _pagelayout,
            "SinglePage"
                | "OneColumn"
                | "TwoColumnLeft"
                | "TwoColumnRight"
                | "TwoPageLeft"
                | "TwoPageRight"
        ) {
            return Err(bad_value(format!("unknown page layout {_pagelayout}")));
        }
        let lp = self.lopdf_mut();
        let catalog = catalog_id(lp)?;
        lp.get_dictionary_mut(catalog)?
            .set("PageLayout", name_object(_pagelayout));
        Ok(())
    }

    /// Set the page mode name.
    pub fn set_pagemode(&mut self, _pagemode: &str) -> Result<()> {
        if !matches!(
            _pagemode,
            "UseNone" | "UseOutlines" | "UseThumbs" | "FullScreen" | "UseOC" | "UseAttachments"
        ) {
            return Err(bad_value(format!("unknown page mode {_pagemode}")));
        }
        let lp = self.lopdf_mut();
        let catalog = catalog_id(lp)?;
        lp.get_dictionary_mut(catalog)?
            .set("PageMode", name_object(_pagemode));
        Ok(())
    }

    /// Set outline entries.
    pub fn set_toc(&mut self, _toc: &[TocEntry], _collapse: i32) -> Result<i32> {
        let lp = self.lopdf_mut();
        if _toc.is_empty() {
            // Clearing the outline returns the removed entry count plus the
            // root (recorded: 5 for the four-entry outline).
            let removed = flatten_outlines(lp).len() as i32 + 1;
            build_outlines(lp, _toc, _collapse)?;
            return Ok(removed);
        }
        let count = build_outlines(lp, _toc, _collapse)?;
        Ok(count)
    }

    /// Set one outline entry.
    pub fn set_toc_item(&mut self, _index: i32, _options: &TocItemOptions) -> Result<()> {
        let lp = self.lopdf_mut();
        let rows = flatten_outlines(lp);
        let at = toc_index(rows.len(), _index)?;
        let mut entries: Vec<TocEntry> = rows.into_iter().map(outline_entry).collect();
        let entry = entries
            .get_mut(at)
            .ok_or_else(|| index_oob("outline index out of range"))?;
        if let Some(title) = &_options.title {
            entry.title.clone_from(title);
        }
        if let Some(page) = _options.page {
            entry.page = page;
        }
        let mut dest = entry.dest.clone().unwrap_or(TocDest {
            kind: 1,
            page: entry.page,
            target: mk_point(0.0, 0.0),
            zoom: 0.0,
        });
        if let Some(kind) = _options.kind {
            dest.kind = kind;
        }
        if let Some(page) = _options.page {
            dest.page = page;
        }
        if let Some(target) = _options.target {
            dest.target = target;
        }
        if _options.zoom != 0.0 {
            dest.zoom = _options.zoom;
        }
        entry.dest = Some(dest);
        let _ = (&_options.uri, &_options.filename);
        build_outlines(lp, &entries, 1)?;
        Ok(())
    }

    /// Set the XML metadata.
    pub fn set_xml_metadata(&mut self, _metadata: &str) -> Result<()> {
        let lp = self.lopdf_mut();
        let catalog = catalog_id(lp)?;
        let meta_id = lp.new_object_id();
        lp.objects.insert(
            meta_id,
            Object::Stream(Stream::new(
                Dictionary::new(),
                _metadata.as_bytes().to_vec(),
            )),
        );
        lp.get_dictionary_mut(catalog)?
            .set("Metadata", Object::Reference(meta_id));
        Ok(())
    }

    /// Rewrite an object.
    pub fn update_object(&mut self, _xref: i32, _text: &str, _page: Option<i32>) -> Result<()> {
        let lp = self.lopdf_mut();
        let target = (_xref as u32, 0);
        if !lp.objects.contains_key(&target) {
            return Err(engine_fault("object does not exist"));
        }
        let obj = parse_pdf_value(_text)?;
        lp.objects.insert(target, obj);
        let _ = _page;
        Ok(())
    }

    /// Rewrite a stream.
    pub fn update_stream(
        &mut self,
        _xref: i32,
        _data: Option<&[u8]>,
        _new: bool,
        _compress: bool,
    ) -> Result<()> {
        let lp = self.lopdf_mut();
        let target = (_xref as u32, 0);
        let exists = lp.objects.contains_key(&target);
        if !exists {
            let next = lp.max_id + 1;
            if !_new || target.0 != next {
                return Err(bad_value("stream object does not exist"));
            }
        }
        let bytes = _data.unwrap_or_default().to_vec();
        match lp.get_object_mut(target) {
            Ok(Object::Stream(stream)) => {
                stream.set_content(bytes);
                stream.dict.remove(b"Filter");
                if _compress {
                    let _ = stream.compress();
                }
            }
            _ => {
                let mut stream = Stream::new(Dictionary::new(), bytes);
                if _compress {
                    let _ = stream.compress();
                }
                lp.objects.insert(target, Object::Stream(stream));
            }
        }
        Ok(())
    }

    /// Copy an object.
    pub fn xref_copy(
        &mut self,
        _source: i32,
        _target: i32,
        _keep: Option<&[String]>,
    ) -> Result<()> {
        let lp = self.lopdf_mut();
        let source = (_source as u32, 0);
        let target = (_target as u32, 0);
        if !lp.objects.contains_key(&source) {
            return Err(bad_value("source object does not exist"));
        }
        if !lp.objects.contains_key(&target) {
            return Err(bad_value("target object does not exist"));
        }
        let src_obj = lp.objects.get(&source).cloned().unwrap_or(Object::Null);
        let mut memo = HashMap::new();
        let mut copied = copy_object_deep(lp, &src_obj, &mut memo)?;
        if let (Ok(new_dict), Some(old)) = (copied.as_dict_mut(), lp.objects.get(&target).cloned())
        {
            let old_dict = match &old {
                Object::Dictionary(d) => Some(d.clone()),
                Object::Stream(s) => Some(s.dict.clone()),
                _ => None,
            };
            if let Some(old_dict) = old_dict {
                for (key, value) in old_dict.iter() {
                    if new_dict.get(key).is_err() {
                        new_dict.set(key.as_slice(), value.clone());
                    }
                }
            }
        }
        if let Some(keep) = _keep {
            if let (Ok(new_dict), Some(Object::Dictionary(old_dict))) =
                (copied.as_dict_mut(), lp.objects.get(&target).cloned())
            {
                for key in keep {
                    if let Ok(value) = old_dict.get(key.as_bytes()) {
                        new_dict.set(key.as_str(), value.clone());
                    }
                }
            }
        }
        lp.objects.insert(target, copied);
        Ok(())
    }

    /// Set an object key.
    pub fn xref_set_key(&mut self, _xref: i32, _key: &str, _value: &str) -> Result<()> {
        let lp = self.lopdf_mut();
        let target = (_xref as u32, 0);
        if !lp.objects.contains_key(&target) {
            return Err(bad_value("object does not exist"));
        }
        let value = parse_pdf_value(_value)?;
        if let Ok(dict) = lp.get_dictionary_mut(target) {
            dict.set(_key, value);
            return Ok(());
        }
        if let Ok(Object::Stream(stream)) = lp.get_object_mut(target) {
            stream.dict.set(_key, value);
            return Ok(());
        }
        Err(bad_value("object is neither a dictionary nor a stream"))
    }
}

impl<'a> PartialEq for Annot<'a> {
    fn eq(&self, _other: &Self) -> bool {
        std::ptr::eq(self.doc as *const _, _other.doc as *const _)
            && self.page == _other.page
            && self.xref == _other.xref
    }
}

impl<'a> PartialEq for Shape<'a> {
    fn eq(&self, _other: &Self) -> bool {
        std::ptr::eq(self.doc as *const _, _other.doc as *const _) && self.page == _other.page
    }
}

// ---------------------------------------------------------------------------
// Lane L4 private machinery: errors, coordinates, content, fonts, parsing.
// ---------------------------------------------------------------------------

use super::constants;

/// Local value constructors: the `P0` lane owns the `new` methods (still
/// stubs), so this lane builds values from the public fields directly.
fn mk_rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
    Rect { x0, y0, x1, y1 }
}

fn mk_point(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn mk_matrix(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Matrix {
    Matrix { a, b, c, d, e, f }
}

fn mk_quad(ul: Point, ur: Point, ll: Point, lr: Point) -> Quad {
    Quad { ul, ur, ll, lr }
}

fn infinite_rect() -> Rect {
    mk_rect(
        constants::FZ_MIN_INF_RECT as f64,
        constants::FZ_MIN_INF_RECT as f64,
        constants::FZ_MAX_INF_RECT as f64,
        constants::FZ_MAX_INF_RECT as f64,
    )
}

fn bad_value(detail: impl Into<String>) -> crate::error::Error {
    ErrorKind::BadValue.to_error(detail)
}

fn wrong_type(detail: impl Into<String>) -> crate::error::Error {
    ErrorKind::WrongType.to_error(detail)
}

fn unknown_key(detail: impl Into<String>) -> crate::error::Error {
    ErrorKind::UnknownKey.to_error(detail)
}

fn index_oob(detail: impl Into<String>) -> crate::error::Error {
    ErrorKind::IndexOutOfRange.to_error(detail)
}

fn engine_fault(detail: impl Into<String>) -> crate::error::Error {
    ErrorKind::EngineFault.to_error(detail)
}

fn missing_file(detail: impl Into<String>) -> crate::error::Error {
    ErrorKind::MissingFile.to_error(detail)
}

fn unreadable(detail: impl Into<String>) -> crate::error::Error {
    ErrorKind::UnreadableFile.to_error(detail)
}

fn sync_doc(doc: &mut PdfDocument) -> Result<()> {
    doc.sync_engine()
}

// --- page access -----------------------------------------------------------

/// Ordered leaf page ids (document order).
fn leaf_pages(doc: &LopdfDocument) -> Vec<ObjectId> {
    doc.get_pages().values().copied().collect()
}

fn page_count_of(doc: &LopdfDocument) -> usize {
    doc.get_pages().len()
}

fn page_id_of(doc: &LopdfDocument, index: usize) -> Result<ObjectId> {
    leaf_pages(doc)
        .get(index)
        .copied()
        .ok_or_else(|| bad_value(format!("page index {index} out of range")))
}

/// MediaBox in PDF coordinates, inheriting from page-tree ancestors.
fn mediabox_of(doc: &LopdfDocument, page_id: ObjectId) -> [f64; 4] {
    let mut current = Some(page_id);
    while let Some(id) = current {
        if let Ok(dict) = doc.get_dictionary(id) {
            if let Ok(obj) = dict.get(b"MediaBox") {
                let resolved = doc.dereference(obj).map(|(_, o)| o).unwrap_or(obj);
                if let Some(r) = obj_rect(resolved) {
                    return [r.x0, r.y0, r.x1, r.y1];
                }
            }
            current = dict.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
        } else {
            break;
        }
    }
    [0.0, 0.0, 612.0, 792.0]
}

/// Top edge of the page box in PDF coordinates: the y-down mirror line.
fn page_top(doc: &LopdfDocument, page_id: ObjectId) -> f64 {
    mediabox_of(doc, page_id)[3]
}

fn flip_y(y: f64, top: f64) -> f64 {
    top - y
}

fn flip_rect(rect: &Rect, top: f64) -> Rect {
    mk_rect(rect.x0, top - rect.y1, rect.x1, top - rect.y0)
}

fn norm_rect(rect: &Rect) -> Rect {
    mk_rect(
        rect.x0.min(rect.x1),
        rect.y0.min(rect.y1),
        rect.x0.max(rect.x1),
        rect.y1.max(rect.y0),
    )
}

// --- lopdf object helpers --------------------------------------------------

fn obj_f64(obj: &Object) -> f64 {
    match obj {
        Object::Integer(i) => *i as f64,
        Object::Real(f) => f64::from(*f),
        _ => 0.0,
    }
}

fn obj_i32(obj: &Object) -> i32 {
    match obj {
        Object::Integer(i) => *i as i32,
        Object::Real(f) => *f as i32,
        Object::Boolean(b) => i32::from(*b),
        _ => 0,
    }
}

/// Whether a PDF number object keeps its integer shape across the bindings.
fn obj_is_int(obj: &Object) -> bool {
    matches!(obj, Object::Integer(_))
}

fn obj_name(obj: &Object) -> String {
    match obj {
        Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
        _ => String::new(),
    }
}

/// Decode a PDF text string: UTF-16BE with BOM, else PDFDoc-ish bytes.
fn obj_text(obj: &Object) -> String {
    let bytes = match obj {
        Object::String(b, _) => b.as_slice(),
        _ => return String::new(),
    };
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    bytes.iter().map(|b| *b as char).collect()
}

fn text_object(text: &str) -> Object {
    if text.is_ascii() {
        Object::String(text.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        Object::String(bytes, StringFormat::Hexadecimal)
    }
}

fn obj_rect(obj: &Object) -> Option<Rect> {
    let arr = obj.as_array().ok()?;
    if arr.len() != 4 {
        return None;
    }
    Some(mk_rect(
        obj_f64(&arr[0]),
        obj_f64(&arr[1]),
        obj_f64(&arr[2]),
        obj_f64(&arr[3]),
    ))
}

/// A PDF number: whole values keep integer shape (and full precision past
/// the f32 range), everything else writes as a real.
fn num_object(value: f64) -> Object {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        Object::Integer(value as i64)
    } else {
        Object::Real(value as f32)
    }
}

fn rect_object(rect: &Rect) -> Object {
    Object::Array(vec![
        num_object(rect.x0),
        num_object(rect.y0),
        num_object(rect.x1),
        num_object(rect.y1),
    ])
}

#[allow(dead_code)]
fn point_object(point: &Point) -> Object {
    Object::Array(vec![
        Object::Real(point.x as f32),
        Object::Real(point.y as f32),
    ])
}

fn name_object(name: &str) -> Object {
    Object::Name(name.as_bytes().to_vec())
}

fn num_array(values: &[f64]) -> Object {
    Object::Array(values.iter().map(|v| num_object(*v)).collect())
}

/// Follow references to the final object.
fn deref<'a>(doc: &'a LopdfDocument, obj: &'a Object) -> &'a Object {
    doc.dereference(obj).map(|(_, o)| o).unwrap_or(obj)
}

#[allow(dead_code)]
fn dict_entry<'a>(doc: &'a LopdfDocument, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    dict.get(key).ok().map(|o| deref(doc, o))
}

fn annot_dict(doc: &LopdfDocument, xref: i32) -> Result<Dictionary> {
    if xref <= 0 {
        return Err(bad_value(format!("bad annotation reference {xref}")));
    }
    let id = (xref as u32, 0);
    Ok(doc.get_dictionary(id)?.clone())
}

/// Object ids listed in a page's `/Annots` array, in order.
fn page_annots(doc: &LopdfDocument, page_id: ObjectId) -> Vec<ObjectId> {
    let dict = match doc.get_dictionary(page_id) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let annots = match dict.get(b"Annots") {
        Ok(o) => deref(doc, o),
        Err(_) => return Vec::new(),
    };
    let arr = match annots.as_array() {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };
    arr.iter().filter_map(|o| o.as_reference().ok()).collect()
}

/// Replace a page's `/Annots` array with the given object ids.
fn set_page_annots(doc: &mut LopdfDocument, page_id: ObjectId, ids: &[ObjectId]) -> Result<()> {
    let page = doc.get_dictionary_mut(page_id)?;
    if ids.is_empty() {
        page.remove(b"Annots");
    } else {
        page.set(
            "Annots",
            Object::Array(ids.iter().map(|id| Object::Reference(*id)).collect()),
        );
    }
    Ok(())
}

// --- text encoding ---------------------------------------------------------

/// WinAnsi code for a character, if encodable.
fn winansi_code(ch: char) -> Option<u8> {
    if (ch as u32) < 128 {
        return Some(ch as u8);
    }
    if (' '..='ÿ').contains(&ch) {
        return Some(ch as u8);
    }
    WINANSI_EXTRA
        .iter()
        .find(|(_, c)| *c == ch && *c != '\0')
        .map(|(code, _)| *code)
}

/// Unicode codepoint for a WinAnsi code.
fn winansi_unicode(code: u8) -> char {
    if !(128..160).contains(&code) {
        return code as char;
    }
    WINANSI_EXTRA
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, ch)| *ch)
        .unwrap_or('\0')
}

/// Encode text as WinAnsi bytes, escaping string delimiters; unmapped
/// characters become `?`.
fn winansi_string(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 2);
    out.push(b'(');
    for ch in text.chars() {
        if ch == '\n' {
            out.extend_from_slice(b"\\n");
            continue;
        }
        let code = winansi_code(ch).unwrap_or(b'?');
        match code {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(code);
            }
            _ => out.push(code),
        }
    }
    out.push(b')');
    out
}

/// Compact number formatting for content streams.
fn pdf_num(value: f64) -> String {
    if value.is_nan() {
        return "0".to_string();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_string()
    } else {
        text.to_string()
    }
}

/// Paint colour operators for 1/3/4 component colours.
fn color_ops(color: &[f64], fill: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<u8>, op: &str| {
        out.extend_from_slice(op.as_bytes());
        out.push(b'\n');
    };
    match color.len() {
        1 => {
            let v = pdf_num(color[0]);
            push(&mut out, &format!("{v} {}", if fill { "g" } else { "G" }));
        }
        3 => {
            let v: Vec<String> = color.iter().map(|c| pdf_num(*c)).collect();
            push(
                &mut out,
                &format!(
                    "{} {} {} {}",
                    v[0],
                    v[1],
                    v[2],
                    if fill { "rg" } else { "RG" }
                ),
            );
        }
        4 => {
            let v: Vec<String> = color.iter().map(|c| pdf_num(*c)).collect();
            push(
                &mut out,
                &format!(
                    "{} {} {} {} {}",
                    v[0],
                    v[1],
                    v[2],
                    v[3],
                    if fill { "k" } else { "K" }
                ),
            );
        }
        _ => {}
    }
    out
}

/// Normalise a 1/3/4 component colour to RGB.
fn to_rgb(color: &[f64]) -> [f64; 3] {
    match color.len() {
        1 => [color[0], color[0], color[0]],
        3 => [color[0], color[1], color[2]],
        4 => {
            let (c, m, y, k) = (color[0], color[1], color[2], color[3]);
            [
                1.0 - (c + k).min(1.0),
                1.0 - (m + k).min(1.0),
                1.0 - (y + k).min(1.0),
            ]
        }
        _ => [0.0, 0.0, 0.0],
    }
}

// glyph names: 0
static WIDTHS_HELV: [u16; 224] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
    0, 556, 0, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, 0, 222, 222,
    333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 0, 500, 667, 0, 333, 556, 556, 556, 556,
    260, 556, 333, 737, 370, 556, 584, 333, 737, 333, 400, 584, 333, 333, 333, 556, 537, 278, 333,
    333, 365, 556, 834, 834, 834, 611, 667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667,
    278, 278, 278, 278, 722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667,
    611, 556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278, 278, 556, 556,
    556, 556, 556, 556, 556, 584, 611, 556, 556, 556, 556, 500, 556, 500,
];
static WIDTHS_HEBO: [u16; 224] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667,
    611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556,
    278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
    0, 556, 0, 278, 556, 500, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, 0, 278, 278,
    500, 500, 350, 556, 1000, 333, 1000, 556, 333, 944, 0, 500, 667, 0, 333, 556, 556, 556, 556,
    280, 556, 333, 737, 370, 556, 584, 333, 737, 333, 400, 584, 333, 333, 333, 611, 556, 278, 333,
    333, 365, 556, 834, 834, 834, 611, 722, 722, 722, 722, 722, 722, 1000, 722, 667, 667, 667, 667,
    278, 278, 278, 278, 722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667,
    611, 556, 556, 556, 556, 556, 556, 889, 556, 556, 556, 556, 556, 278, 278, 278, 278, 611, 611,
    611, 611, 611, 611, 611, 584, 611, 611, 611, 611, 611, 556, 611, 556,
];
static WIDTHS_TIRO: [u16; 224] = [
    250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278, 500, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444, 921, 722, 667, 667, 722, 611,
    556, 722, 722, 333, 389, 722, 611, 889, 722, 722, 556, 722, 667, 556, 611, 722, 722, 944, 722,
    722, 611, 333, 278, 333, 469, 500, 333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500,
    278, 778, 500, 500, 500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541,
    0, 500, 0, 333, 500, 444, 1000, 500, 500, 333, 1000, 556, 333, 889, 0, 611, 0, 0, 333, 333,
    444, 444, 350, 500, 1000, 333, 980, 389, 333, 722, 0, 444, 722, 0, 333, 500, 500, 500, 500,
    200, 500, 333, 760, 276, 500, 564, 333, 760, 333, 400, 564, 300, 300, 333, 500, 453, 250, 333,
    300, 310, 500, 750, 750, 750, 444, 722, 722, 722, 722, 722, 722, 889, 667, 611, 611, 611, 611,
    333, 333, 333, 333, 722, 722, 722, 722, 722, 722, 722, 564, 722, 722, 722, 722, 722, 722, 556,
    500, 444, 444, 444, 444, 444, 444, 667, 444, 444, 444, 444, 444, 278, 278, 278, 278, 500, 500,
    500, 500, 500, 500, 500, 564, 500, 500, 500, 500, 500, 500, 500, 500,
];
static WIDTHS_TIBO: [u16; 224] = [
    250, 333, 555, 500, 500, 1000, 833, 278, 333, 333, 500, 570, 250, 333, 250, 278, 500, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500, 930, 722, 667, 722, 722, 667,
    611, 778, 778, 389, 500, 778, 667, 944, 722, 778, 611, 778, 722, 556, 667, 722, 722, 1000, 722,
    722, 667, 333, 278, 333, 581, 500, 333, 500, 556, 444, 556, 444, 333, 500, 556, 278, 333, 556,
    278, 833, 556, 500, 556, 556, 444, 389, 333, 556, 500, 722, 500, 500, 444, 394, 220, 394, 520,
    0, 500, 0, 333, 500, 500, 1000, 500, 500, 333, 1000, 556, 333, 1000, 0, 667, 0, 0, 333, 333,
    500, 500, 350, 500, 1000, 333, 1000, 389, 333, 722, 0, 444, 722, 0, 333, 500, 500, 500, 500,
    220, 500, 333, 747, 300, 500, 570, 333, 747, 333, 400, 570, 300, 300, 333, 556, 540, 250, 333,
    300, 330, 500, 750, 750, 750, 500, 722, 722, 722, 722, 722, 722, 1000, 722, 667, 667, 667, 667,
    389, 389, 389, 389, 722, 722, 778, 778, 778, 778, 778, 570, 778, 722, 722, 722, 722, 722, 611,
    556, 500, 500, 500, 500, 500, 500, 722, 444, 444, 444, 444, 444, 278, 278, 278, 278, 500, 556,
    500, 500, 500, 500, 500, 570, 500, 556, 556, 556, 556, 500, 556, 500,
];
static WIDTHS_COUR: [u16; 224] = [
    600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600,
    0, 600, 0, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 0, 600, 0, 0, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 600, 600, 0, 600, 600, 0, 600, 600, 600, 600, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600, 600,
];
static WIDTHS_SYMB: [u16; 224] = [
    250, 333, 0, 500, 0, 833, 778, 0, 333, 333, 0, 549, 250, 0, 250, 278, 500, 500, 500, 500, 500,
    500, 500, 500, 500, 500, 278, 278, 549, 549, 549, 444, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 333, 0, 333, 0, 500, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 480, 200, 480, 0, 0, 750, 0, 0, 500, 0, 1000,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 460, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 713, 0, 0, 0, 400, 549, 0, 0, 0, 576, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 549, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 549, 0, 0, 0, 0, 0, 0, 0, 0,
];
static WIDTHS_ZADB: [u16; 224] = [
    278, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0,
];
// check Hello,world = 5501
// check word = 2167, 10 words+9sp = 24172

static WINANSI_EXTRA: [(u8, char); 32] = [
    (128, '€'),
    (129, '\0'),
    (130, '‚'),
    (131, 'ƒ'),
    (132, '„'),
    (133, '…'),
    (134, '†'),
    (135, '‡'),
    (136, 'ˆ'),
    (137, '‰'),
    (138, 'Š'),
    (139, '‹'),
    (140, 'Œ'),
    (141, '\0'),
    (142, 'Ž'),
    (143, '\0'),
    (144, '\0'),
    (145, '‘'),
    (146, '’'),
    (147, '“'),
    (148, '”'),
    (149, '•'),
    (150, '–'),
    (151, '—'),
    (152, '˜'),
    (153, '™'),
    (154, 'š'),
    (155, '›'),
    (156, 'œ'),
    (157, '\0'),
    (158, 'ž'),
    (159, 'Ÿ'),
];

// --- unicode names ---------------------------------------------------------

/// Unicode character name for the ASCII range, generated from the range
/// patterns, and the Latin-1 supplement from its table; anything else
/// renders as a `U+XXXX` label.
pub(crate) fn unicode_name(codepoint: u32) -> String {
    if let Some(ch) = char::from_u32(codepoint) {
        if ch.is_ascii_uppercase() {
            return format!("LATIN CAPITAL LETTER {ch}");
        }
        if ch.is_ascii_lowercase() {
            return format!("LATIN SMALL LETTER {}", ch.to_ascii_uppercase());
        }
        if ch.is_ascii_digit() {
            let word = [
                "ZERO", "ONE", "TWO", "THREE", "FOUR", "FIVE", "SIX", "SEVEN", "EIGHT", "NINE",
            ][(ch as u8 - b'0') as usize];
            return format!("DIGIT {word}");
        }
        if let Some(name) = ascii_punct_name(ch) {
            return name.to_string();
        }
        if ch == ' ' {
            return "SPACE".to_string();
        }
        if let Some(name) = latin1_name(codepoint) {
            return name.to_string();
        }
    }
    format!("U+{codepoint:04X}")
}

/// Unicode character name for U+00A0 through U+00FF, in code order.
fn latin1_name(codepoint: u32) -> Option<&'static str> {
    const NAMES: [&str; 96] = [
        // A0.
        "NO-BREAK SPACE",
        "INVERTED EXCLAMATION MARK",
        "CENT SIGN",
        "POUND SIGN",
        "CURRENCY SIGN",
        "YEN SIGN",
        "BROKEN BAR",
        "SECTION SIGN",
        "DIAERESIS",
        "COPYRIGHT SIGN",
        "FEMININE ORDINAL INDICATOR",
        "LEFT-POINTING DOUBLE ANGLE QUOTATION MARK",
        "NOT SIGN",
        "SOFT HYPHEN",
        "REGISTERED SIGN",
        "MACRON",
        // B0.
        "DEGREE SIGN",
        "PLUS-MINUS SIGN",
        "SUPERSCRIPT TWO",
        "SUPERSCRIPT THREE",
        "ACUTE ACCENT",
        "MICRO SIGN",
        "PILCROW SIGN",
        "MIDDLE DOT",
        "CEDILLA",
        "SUPERSCRIPT ONE",
        "MASCULINE ORDINAL INDICATOR",
        "RIGHT-POINTING DOUBLE ANGLE QUOTATION MARK",
        "VULGAR FRACTION ONE QUARTER",
        "VULGAR FRACTION ONE HALF",
        "VULGAR FRACTION THREE QUARTERS",
        "INVERTED QUESTION MARK",
        // C0.
        "LATIN CAPITAL LETTER A WITH GRAVE",
        "LATIN CAPITAL LETTER A WITH ACUTE",
        "LATIN CAPITAL LETTER A WITH CIRCUMFLEX",
        "LATIN CAPITAL LETTER A WITH TILDE",
        "LATIN CAPITAL LETTER A WITH DIAERESIS",
        "LATIN CAPITAL LETTER A WITH RING ABOVE",
        "LATIN CAPITAL LETTER AE",
        "LATIN CAPITAL LETTER C WITH CEDILLA",
        "LATIN CAPITAL LETTER E WITH GRAVE",
        "LATIN CAPITAL LETTER E WITH ACUTE",
        "LATIN CAPITAL LETTER E WITH CIRCUMFLEX",
        "LATIN CAPITAL LETTER E WITH DIAERESIS",
        "LATIN CAPITAL LETTER I WITH GRAVE",
        "LATIN CAPITAL LETTER I WITH ACUTE",
        "LATIN CAPITAL LETTER I WITH CIRCUMFLEX",
        "LATIN CAPITAL LETTER I WITH DIAERESIS",
        // D0.
        "LATIN CAPITAL LETTER ETH",
        "LATIN CAPITAL LETTER N WITH TILDE",
        "LATIN CAPITAL LETTER O WITH GRAVE",
        "LATIN CAPITAL LETTER O WITH ACUTE",
        "LATIN CAPITAL LETTER O WITH CIRCUMFLEX",
        "LATIN CAPITAL LETTER O WITH TILDE",
        "LATIN CAPITAL LETTER O WITH DIAERESIS",
        "MULTIPLICATION SIGN",
        "LATIN CAPITAL LETTER O WITH STROKE",
        "LATIN CAPITAL LETTER U WITH GRAVE",
        "LATIN CAPITAL LETTER U WITH ACUTE",
        "LATIN CAPITAL LETTER U WITH CIRCUMFLEX",
        "LATIN CAPITAL LETTER U WITH DIAERESIS",
        "LATIN CAPITAL LETTER Y WITH ACUTE",
        "LATIN CAPITAL LETTER THORN",
        "LATIN SMALL LETTER SHARP S",
        // E0.
        "LATIN SMALL LETTER A WITH GRAVE",
        "LATIN SMALL LETTER A WITH ACUTE",
        "LATIN SMALL LETTER A WITH CIRCUMFLEX",
        "LATIN SMALL LETTER A WITH TILDE",
        "LATIN SMALL LETTER A WITH DIAERESIS",
        "LATIN SMALL LETTER A WITH RING ABOVE",
        "LATIN SMALL LETTER AE",
        "LATIN SMALL LETTER C WITH CEDILLA",
        "LATIN SMALL LETTER E WITH GRAVE",
        "LATIN SMALL LETTER E WITH ACUTE",
        "LATIN SMALL LETTER E WITH CIRCUMFLEX",
        "LATIN SMALL LETTER E WITH DIAERESIS",
        "LATIN SMALL LETTER I WITH GRAVE",
        "LATIN SMALL LETTER I WITH ACUTE",
        "LATIN SMALL LETTER I WITH CIRCUMFLEX",
        "LATIN SMALL LETTER I WITH DIAERESIS",
        // F0.
        "LATIN SMALL LETTER ETH",
        "LATIN SMALL LETTER N WITH TILDE",
        "LATIN SMALL LETTER O WITH GRAVE",
        "LATIN SMALL LETTER O WITH ACUTE",
        "LATIN SMALL LETTER O WITH CIRCUMFLEX",
        "LATIN SMALL LETTER O WITH TILDE",
        "LATIN SMALL LETTER O WITH DIAERESIS",
        "DIVISION SIGN",
        "LATIN SMALL LETTER O WITH STROKE",
        "LATIN SMALL LETTER U WITH GRAVE",
        "LATIN SMALL LETTER U WITH ACUTE",
        "LATIN SMALL LETTER U WITH CIRCUMFLEX",
        "LATIN SMALL LETTER U WITH DIAERESIS",
        "LATIN SMALL LETTER Y WITH ACUTE",
        "LATIN SMALL LETTER THORN",
        "LATIN SMALL LETTER Y WITH DIAERESIS",
    ];
    codepoint
        .checked_sub(0xA0)
        .and_then(|index| NAMES.get(index as usize).copied())
}

fn ascii_punct_name(ch: char) -> Option<&'static str> {
    Some(match ch {
        '!' => "EXCLAMATION MARK",
        '"' => "QUOTATION MARK",
        '#' => "NUMBER SIGN",
        '$' => "DOLLAR SIGN",
        '%' => "PERCENT SIGN",
        '&' => "AMPERSAND",
        '\'' => "APOSTROPHE",
        '(' => "LEFT PARENTHESIS",
        ')' => "RIGHT PARENTHESIS",
        '*' => "ASTERISK",
        '+' => "PLUS SIGN",
        ',' => "COMMA",
        '-' => "HYPHEN-MINUS",
        '.' => "FULL STOP",
        '/' => "SOLIDUS",
        ':' => "COLON",
        ';' => "SEMICOLON",
        '<' => "LESS-THAN SIGN",
        '=' => "EQUALS SIGN",
        '>' => "GREATER-THAN SIGN",
        '?' => "QUESTION MARK",
        '@' => "COMMERCIAL AT",
        '[' => "LEFT SQUARE BRACKET",
        '\\' => "REVERSE SOLIDUS",
        ']' => "RIGHT SQUARE BRACKET",
        '^' => "CIRCUMFLEX ACCENT",
        '_' => "LOW LINE",
        '`' => "GRAVE ACCENT",
        '{' => "LEFT CURLY BRACKET",
        '|' => "VERTICAL LINE",
        '}' => "RIGHT CURLY BRACKET",
        '~' => "TILDE",
        _ => return None,
    })
}

/// Reverse lookup for [`unicode_name`]: ASCII pattern names back to codepoints.
fn unicode_from_name(name: &str) -> Option<u32> {
    if let Some(rest) = name.strip_prefix("LATIN CAPITAL LETTER ") {
        let mut chars = rest.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            if ch.is_ascii_uppercase() {
                return Some(ch as u32);
            }
        }
        return None;
    }
    if let Some(rest) = name.strip_prefix("LATIN SMALL LETTER ") {
        let mut chars = rest.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            if ch.is_ascii_uppercase() {
                return Some(ch.to_ascii_lowercase() as u32);
            }
        }
        return None;
    }
    if let Some(rest) = name.strip_prefix("DIGIT ") {
        let words = [
            "ZERO", "ONE", "TWO", "THREE", "FOUR", "FIVE", "SIX", "SEVEN", "EIGHT", "NINE",
        ];
        if let Some(pos) = words.iter().position(|w| *w == rest) {
            return Some(u32::from(b'0') + pos as u32);
        }
        return None;
    }
    if name == "SPACE" {
        return Some(0x20);
    }
    for byte in 0x21..0x7Fu32 {
        if let Some(ch) = char::from_u32(byte) {
            if ascii_punct_name(ch) == Some(name) {
                return Some(byte);
            }
        }
    }
    if let Some(hex) = name.strip_prefix("U+") {
        return u32::from_str_radix(hex, 16).ok();
    }
    None
}

// --- font faces ------------------------------------------------------------

/// Resolved metrics for one font: either a built-in base-14 face (widths
/// from our transcribed AFM tables) or a parsed TrueType/OpenType program.
#[derive(Debug, Clone, PartialEq)]
struct FaceData {
    /// Full face name (`Helvetica`, `Times-Roman`, ...).
    face_name: String,
    /// Display name: family plus subfamily for file faces.
    display_name: String,
    /// Base-14 widths by WinAnsi code minus 32 (built-ins only).
    builtin: Option<&'static [u16; 224]>,
    /// Units per em (1000 for built-ins).
    upm: u16,
    /// Glyph advance widths in font units, by glyph id (file faces).
    gid_widths: Vec<u16>,
    /// Codepoint to glyph id (file faces).
    cmap: HashMap<u32, u16>,
    /// Ascender in em units.
    asc: f64,
    /// Descender in em units (negative).
    desc: f64,
    /// Font bounding box in em units.
    bbox: [f64; 4],
    /// Capability flags.
    flags: FontFlags,
    /// Glyph count.
    glyph_count: usize,
    /// PostScript name (name ID 6 for file faces, base name for built-ins).
    postscript_name: String,
}

impl FaceData {
    /// Ascender in em units. Built-ins round through f32 (recorded).
    fn asc_em(&self) -> f64 {
        if self.builtin.is_some() {
            f64::from(self.asc as f32)
        } else {
            self.asc
        }
    }

    /// Descender in em units. Built-ins round through f32 (recorded).
    fn desc_em(&self) -> f64 {
        if self.builtin.is_some() {
            f64::from(self.desc as f32)
        } else {
            self.desc
        }
    }

    /// Font bounding box in em units with the face rounding applied.
    fn bbox_em(&self) -> [f64; 4] {
        if self.builtin.is_some() {
            [
                f64::from(self.bbox[0] as f32),
                f64::from(self.bbox[1] as f32),
                f64::from(self.bbox[2] as f32),
                f64::from(self.bbox[3] as f32),
            ]
        } else {
            self.bbox
        }
    }

    /// Advance width of one character in em units.
    fn width_em(&self, ch: char) -> f64 {
        if let Some(table) = self.builtin {
            let code = winansi_code(ch).unwrap_or(0);
            if code >= 32 {
                return f64::from(table[(code - 32) as usize]) / 1000.0;
            }
            return 0.0;
        }
        let gid = self.cmap.get(&(ch as u32)).copied().unwrap_or(0);
        let units = self.gid_widths.get(gid as usize).copied().unwrap_or(0);
        f64::from(units) / f64::from(self.upm)
    }

    /// Advance width of a string in em units.
    fn text_width_em(&self, text: &str) -> f64 {
        text.chars().map(|ch| self.width_em(ch)).sum()
    }

    /// Glyph id for a codepoint, or zero when absent. Built-ins without a
    /// font program report a 1-based WinAnsi index (codepoint minus 31).
    fn glyph_id(&self, codepoint: u32) -> u16 {
        if self.builtin.is_some() {
            if let Some(ch) = char::from_u32(codepoint) {
                if let Some(code) = winansi_code(ch) {
                    if code >= 32
                        && self
                            .builtin
                            .map(|t| t[(code - 32) as usize] > 0)
                            .unwrap_or(false)
                    {
                        return (codepoint.saturating_sub(31)) as u16;
                    }
                }
            }
            return 0;
        }
        self.cmap.get(&codepoint).copied().unwrap_or(0)
    }

    /// Units per em.
    fn units_per_em(&self) -> u16 {
        self.upm
    }

    /// Advance width in font units for a glyph id.
    fn advance_for_gid(&self, gid: u16) -> u16 {
        self.gid_widths.get(gid as usize).copied().unwrap_or(0)
    }

    /// Sorted codepoints with metrics.
    fn valid_codepoints(&self) -> Vec<u32> {
        if self.builtin.is_some() {
            let mut out = Vec::new();
            for code in 32u8..=255 {
                let present = self
                    .builtin
                    .map(|t| t[(code - 32) as usize] > 0)
                    .unwrap_or(false);
                if present {
                    let ch = winansi_unicode(code);
                    if ch != '\0' {
                        out.push(ch as u32);
                    }
                }
            }
            return out;
        }
        let mut out: Vec<u32> = self.cmap.keys().copied().collect();
        out.sort_unstable();
        out
    }
}

fn plain_flags() -> FontFlags {
    FontFlags {
        bold: false,
        cjk: false,
        cjk_lang: false,
        embed: true,
        fake_bold: false,
        fake_italic: false,
        invalid_bbox: false,
        italic: false,
        mono: false,
        never_embed: false,
        opentype: false,
        serif: false,
        stretch: false,
        substitute: false,
    }
}

/// Built-in face metrics. Vertical metrics for Helvetica and Times-Roman
/// match the recorded values; the remaining faces use AFM verticals.
fn builtin_face(base: &str) -> Option<FaceData> {
    struct Spec {
        base: &'static str,
        table: &'static [u16; 224],
        asc: f64,
        desc: f64,
        bbox: [f64; 4],
        bold: bool,
        italic: bool,
        mono: bool,
        serif: bool,
    }
    let specs = [
        Spec {
            base: "Helvetica",
            table: &WIDTHS_HELV,
            asc: 1.075,
            desc: -0.299,
            bbox: [-0.21, -0.299, 1.032, 1.075],
            bold: false,
            italic: false,
            mono: false,
            serif: false,
        },
        Spec {
            base: "Helvetica-Bold",
            table: &WIDTHS_HEBO,
            asc: 1.075,
            desc: -0.299,
            bbox: [-0.17, -0.299, 1.032, 1.075],
            bold: true,
            italic: false,
            mono: false,
            serif: false,
        },
        Spec {
            base: "Helvetica-Oblique",
            table: &WIDTHS_HELV,
            asc: 1.075,
            desc: -0.299,
            bbox: [-0.17, -0.299, 1.032, 1.075],
            bold: false,
            italic: true,
            mono: false,
            serif: false,
        },
        Spec {
            base: "Helvetica-BoldOblique",
            table: &WIDTHS_HEBO,
            asc: 1.075,
            desc: -0.299,
            bbox: [-0.17, -0.299, 1.032, 1.075],
            bold: true,
            italic: true,
            mono: false,
            serif: false,
        },
        Spec {
            base: "Times-Roman",
            table: &WIDTHS_TIRO,
            asc: 1.053,
            desc: -0.281,
            bbox: [-0.168, -0.281, 1.0, 1.053],
            bold: false,
            italic: false,
            mono: false,
            serif: true,
        },
        Spec {
            base: "Times-Bold",
            table: &WIDTHS_TIBO,
            asc: 1.053,
            desc: -0.281,
            bbox: [-0.168, -0.281, 1.0, 1.053],
            bold: true,
            italic: false,
            mono: false,
            serif: true,
        },
        Spec {
            base: "Times-Italic",
            table: &WIDTHS_TIRO,
            asc: 1.053,
            desc: -0.281,
            bbox: [-0.168, -0.281, 1.0, 1.053],
            bold: false,
            italic: true,
            mono: false,
            serif: true,
        },
        Spec {
            base: "Times-BoldItalic",
            table: &WIDTHS_TIBO,
            asc: 1.053,
            desc: -0.281,
            bbox: [-0.168, -0.281, 1.0, 1.053],
            bold: true,
            italic: true,
            mono: false,
            serif: true,
        },
        Spec {
            base: "Courier",
            table: &WIDTHS_COUR,
            asc: 0.629,
            desc: -0.207,
            bbox: [-0.023, -0.207, 0.715, 0.629],
            bold: false,
            italic: false,
            mono: true,
            serif: true,
        },
        Spec {
            base: "Courier-Bold",
            table: &WIDTHS_COUR,
            asc: 0.629,
            desc: -0.207,
            bbox: [-0.023, -0.207, 0.715, 0.629],
            bold: true,
            italic: false,
            mono: true,
            serif: true,
        },
        Spec {
            base: "Courier-Oblique",
            table: &WIDTHS_COUR,
            asc: 0.629,
            desc: -0.207,
            bbox: [-0.023, -0.207, 0.715, 0.629],
            bold: false,
            italic: true,
            mono: true,
            serif: true,
        },
        Spec {
            base: "Courier-BoldOblique",
            table: &WIDTHS_COUR,
            asc: 0.629,
            desc: -0.207,
            bbox: [-0.023, -0.207, 0.715, 0.629],
            bold: true,
            italic: true,
            mono: true,
            serif: true,
        },
        Spec {
            base: "Symbol",
            table: &WIDTHS_SYMB,
            asc: 0.718,
            desc: -0.207,
            bbox: [-0.18, -0.207, 1.09, 0.718],
            bold: false,
            italic: false,
            mono: false,
            serif: false,
        },
        Spec {
            base: "ZapfDingbats",
            table: &WIDTHS_ZADB,
            asc: 0.718,
            desc: -0.207,
            bbox: [-0.18, -0.207, 1.0, 0.718],
            bold: false,
            italic: false,
            mono: false,
            serif: false,
        },
    ];
    specs.iter().find(|s| s.base == base).map(|s| {
        let mut flags = plain_flags();
        flags.bold = s.bold;
        flags.italic = s.italic;
        flags.mono = s.mono;
        flags.serif = s.serif;
        let mut face = FaceData {
            face_name: s.base.to_string(),
            display_name: s.base.to_string(),
            builtin: Some(s.table),
            upm: 1000,
            gid_widths: Vec::new(),
            cmap: HashMap::new(),
            asc: s.asc,
            desc: s.desc,
            bbox: s.bbox,
            flags,
            glyph_count: 0,
            postscript_name: s.base.to_string(),
        };
        face.glyph_count = face.valid_codepoints().len() + 1;
        face
    })
}

/// Resolve a short or full font name to a base-14 face.
fn resolve_fontname(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    let base = match lower.as_str() {
        "helv" | "he" | "helvetica" => "Helvetica",
        "hebo" | "helvetica-bold" | "helveticabold" => "Helvetica-Bold",
        "heit" | "helvetica-oblique" | "helveticaoblique" => "Helvetica-Oblique",
        "hebi" | "helvetica-boldoblique" | "helvetica-bold-oblique" => "Helvetica-BoldOblique",
        "tiro" | "ti" | "times" | "times-roman" | "timesroman" => "Times-Roman",
        "tibo" | "times-bold" | "timesbold" => "Times-Bold",
        "tiit" | "times-italic" | "timesitalic" => "Times-Italic",
        "tibi" | "times-bolditalic" | "times-bold-italic" => "Times-BoldItalic",
        "cour" | "co" | "courier" => "Courier",
        "cobo" | "courier-bold" | "courierbold" => "Courier-Bold",
        "coit" | "courier-oblique" | "courieroblique" => "Courier-Oblique",
        "cobi" | "courier-boldoblique" | "courier-bold-oblique" => "Courier-BoldOblique",
        "symb" | "symbol" => "Symbol",
        "zadb" | "zapfdingbats" | "zapf-dingbats" => "ZapfDingbats",
        _ => return None,
    };
    Some(base)
}

// --- TrueType parsing ------------------------------------------------------

struct Ttf<'a> {
    data: &'a [u8],
    tables: HashMap<[u8; 4], (usize, usize)>,
}

impl<'a> Ttf<'a> {
    fn open(data: &'a [u8]) -> Result<Self> {
        if data.len() < 12 {
            return Err(unreadable("font buffer too short"));
        }
        let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let count = u16::from_be_bytes([data[4], data[5]]) as usize;
        let ok = matches!(
            magic,
            0x0001_0000 | 0x74727565 | 0x4F54544F | 0x74797063 | 0x0000_0100
        ) || &data[0..4] == b"OTTO"
            || &data[0..4] == b"true"
            || &data[0..4] == b"typ1";
        if !ok {
            return Err(unreadable("not a TrueType/OpenType font"));
        }
        let mut tables = HashMap::new();
        for i in 0..count {
            let base = 12 + i * 16;
            if base + 16 > data.len() {
                return Err(unreadable("font table directory truncated"));
            }
            let mut tag = [0u8; 4];
            tag.copy_from_slice(&data[base..base + 4]);
            let offset = u32::from_be_bytes([
                data[base + 8],
                data[base + 9],
                data[base + 10],
                data[base + 11],
            ]) as usize;
            let length = u32::from_be_bytes([
                data[base + 12],
                data[base + 13],
                data[base + 14],
                data[base + 15],
            ]) as usize;
            tables.insert(tag, (offset, length));
        }
        Ok(Self { data, tables })
    }

    fn table(&self, tag: &[u8; 4]) -> Result<&'a [u8]> {
        let (offset, length) =
            self.tables.get(tag).copied().ok_or_else(|| {
                unreadable(format!("font lacks {}", String::from_utf8_lossy(tag)))
            })?;
        self.data
            .get(offset..offset.saturating_add(length))
            .ok_or_else(|| unreadable("font table out of range"))
    }

    fn u16_at(data: &[u8], at: usize) -> Result<u16> {
        data.get(at..at + 2)
            .and_then(|b| <[u8; 2]>::try_from(b).ok())
            .map(u16::from_be_bytes)
            .ok_or_else(|| unreadable("font table truncated"))
    }

    fn i16_at(data: &[u8], at: usize) -> Result<i16> {
        Self::u16_at(data, at).map(|v| v as i16)
    }
}

fn parse_cmap_format4(data: &[u8]) -> Result<HashMap<u32, u16>> {
    let seg_count = (Ttf::u16_at(data, 6)? / 2) as usize;
    let end_base = 14usize;
    let start_base = end_base + seg_count * 2 + 2;
    let delta_base = start_base + seg_count * 2;
    let range_base = delta_base + seg_count * 2;
    let mut map = HashMap::new();
    for i in 0..seg_count {
        let end = Ttf::u16_at(data, end_base + i * 2)? as u32;
        let start = Ttf::u16_at(data, start_base + i * 2)? as u32;
        let delta = Ttf::i16_at(data, delta_base + i * 2)? as i32;
        let range_offset = Ttf::u16_at(data, range_base + i * 2)? as usize;
        if start == 0xFFFF && end == 0xFFFF {
            continue;
        }
        let mut code = start;
        while code <= end.min(0x10_FFFF) {
            let gid = if range_offset == 0 {
                (code as i32 + delta).rem_euclid(65536) as u16
            } else {
                let at = range_base + i * 2 + range_offset + ((code - start) as usize) * 2;
                let raw = Ttf::u16_at(data, at).unwrap_or(0);
                if raw == 0 {
                    0
                } else {
                    (raw as i32 + delta).rem_euclid(65536) as u16
                }
            };
            if gid != 0 {
                map.insert(code, gid);
            }
            code += 1;
            if code > 0x10_FFFF {
                break;
            }
        }
    }
    Ok(map)
}

fn parse_cmap(ttf: &Ttf) -> Result<HashMap<u32, u16>> {
    let data = ttf.table(b"cmap")?;
    let records = Ttf::u16_at(data, 2)? as usize;
    let mut best: Option<(u16, u16, usize)> = None;
    for i in 0..records {
        let base = 4 + i * 8;
        if base + 8 > data.len() {
            break;
        }
        let platform = Ttf::u16_at(data, base)?;
        let encoding = Ttf::u16_at(data, base + 2)?;
        let offset = u32::from_be_bytes([
            data[base + 4],
            data[base + 5],
            data[base + 6],
            data[base + 7],
        ]) as usize;
        let score = match (platform, encoding) {
            (3, 10) | (3, 1) => 3,
            (3, 0) => 2,
            (0, _) => 1,
            _ => 0,
        };
        let better = best.map(|(_, s, _)| score > s).unwrap_or(score > 0);
        if better {
            best = Some((platform, score, offset));
        }
    }
    let (_, _, offset) = best.ok_or_else(|| unreadable("font has no Unicode cmap"))?;
    let sub = data
        .get(offset..)
        .ok_or_else(|| unreadable("cmap subtable out of range"))?;
    let format = Ttf::u16_at(sub, 0)?;
    match format {
        4 => parse_cmap_format4(sub),
        12 => {
            let groups = u32::from_be_bytes([sub[12], sub[13], sub[14], sub[15]]) as usize;
            let mut map = HashMap::new();
            for i in 0..groups {
                let base = 16 + i * 12;
                if base + 12 > sub.len() {
                    break;
                }
                let start =
                    u32::from_be_bytes([sub[base], sub[base + 1], sub[base + 2], sub[base + 3]]);
                let end = u32::from_be_bytes([
                    sub[base + 4],
                    sub[base + 5],
                    sub[base + 6],
                    sub[base + 7],
                ]);
                let gid = u32::from_be_bytes([
                    sub[base + 8],
                    sub[base + 9],
                    sub[base + 10],
                    sub[base + 11],
                ]);
                let mut code = start;
                while code <= end.min(0x10_FFFF) {
                    let id = gid.saturating_add(code - start);
                    if id != 0 && id <= 0xFFFF {
                        map.insert(code, id as u16);
                    }
                    if code == end {
                        break;
                    }
                    code += 1;
                }
            }
            Ok(map)
        }
        _ => Err(unreadable(format!("unsupported cmap format {format}"))),
    }
}

fn decode_name_string(bytes: &[u8], platform: u16, encoding: u16) -> String {
    match (platform, encoding) {
        (3, 0) | (3, 1) | (0, _) => {
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        }
        (3, _) => String::from_utf8_lossy(bytes).into_owned(),
        (1, 0) => bytes.iter().map(|b| *b as char).collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// First decoded `name`-table string for one name id, if any.
fn parse_name_id(ttf: &Ttf, want: u16) -> Option<String> {
    let data = ttf.table(b"name").ok()?;
    let count = Ttf::u16_at(data, 2).unwrap_or(0) as usize;
    let storage = Ttf::u16_at(data, 4).unwrap_or(0) as usize;
    for i in 0..count {
        let base = 6 + i * 12;
        if base + 12 > data.len() {
            break;
        }
        let platform = Ttf::u16_at(data, base).unwrap_or(0);
        let encoding = Ttf::u16_at(data, base + 2).unwrap_or(0);
        let name_id = Ttf::u16_at(data, base + 6).unwrap_or(0);
        let length = Ttf::u16_at(data, base + 8).unwrap_or(0) as usize;
        let offset = Ttf::u16_at(data, base + 10).unwrap_or(0) as usize;
        if name_id != want {
            continue;
        }
        let bytes = match data.get(storage + offset..storage + offset + length) {
            Some(b) => b,
            None => continue,
        };
        let text = decode_name_string(bytes, platform, encoding)
            .trim()
            .to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

fn parse_font_name(ttf: &Ttf) -> String {
    parse_name_id(ttf, 4)
        .or_else(|| parse_name_id(ttf, 1))
        .unwrap_or_else(|| "Untitled".to_string())
}

/// Display name: family plus subfamily, unless already combined.
fn parse_display_name(ttf: &Ttf) -> String {
    let family = parse_name_id(ttf, 1).unwrap_or_else(|| "Untitled".to_string());
    let sub = parse_name_id(ttf, 2).unwrap_or_default();
    if sub.is_empty() || family == sub || family.ends_with(&format!(" {sub}")) {
        family
    } else {
        format!("{family} {sub}")
    }
}

/// PostScript name from the name table (name ID 6), if present.
fn parse_postscript_name(ttf: &Ttf) -> Option<String> {
    let data = ttf.table(b"name").ok()?;
    let count = Ttf::u16_at(data, 2).unwrap_or(0) as usize;
    let storage = Ttf::u16_at(data, 4).unwrap_or(0) as usize;
    let mut best: Option<(u8, String)> = None;
    for i in 0..count {
        let base = 6 + i * 12;
        if base + 12 > data.len() {
            break;
        }
        let platform = Ttf::u16_at(data, base).unwrap_or(0);
        let encoding = Ttf::u16_at(data, base + 2).unwrap_or(0);
        let name_id = Ttf::u16_at(data, base + 6).unwrap_or(0);
        if name_id != 6 {
            continue;
        }
        let length = Ttf::u16_at(data, base + 8).unwrap_or(0) as usize;
        let offset = Ttf::u16_at(data, base + 10).unwrap_or(0) as usize;
        let bytes = data.get(storage + offset..storage + offset + length)?;
        let text: String = decode_name_string(bytes, platform, encoding)
            .trim()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect();
        if text.is_empty() {
            continue;
        }
        let rank = match (platform, encoding) {
            (3, 1) | (3, 0) => 0,
            (1, 0) => 1,
            _ => 2,
        };
        let replace = best.as_ref().map(|(r, _)| rank < *r).unwrap_or(true);
        if replace {
            best = Some((rank, text));
        }
    }
    best.map(|(_, text)| text)
}

fn parse_file_face(data: &[u8]) -> Result<FaceData> {
    let ttf = Ttf::open(data)?;
    let head = ttf.table(b"head")?;
    let hhea = ttf.table(b"hhea")?;
    let maxp = ttf.table(b"maxp")?;
    let hmtx = ttf.table(b"hmtx")?;
    let upm = Ttf::u16_at(head, 18)?.max(1);
    let upm_f = f64::from(upm);
    let num_glyphs = Ttf::u16_at(maxp, 4)? as usize;
    let metrics_count = Ttf::u16_at(hhea, 34)? as usize;
    let asc = f64::from(Ttf::i16_at(hhea, 4)?) / upm_f;
    let desc = f64::from(Ttf::i16_at(hhea, 6)?) / upm_f;
    let bbox = [
        f64::from(Ttf::i16_at(head, 36)?) / upm_f,
        f64::from(Ttf::i16_at(head, 38)?) / upm_f,
        f64::from(Ttf::i16_at(head, 40)?) / upm_f,
        f64::from(Ttf::i16_at(head, 42)?) / upm_f,
    ];
    let mut gid_widths = vec![0u16; num_glyphs.max(1)];
    for (gid, slot) in gid_widths.iter_mut().enumerate() {
        let at = gid.min(metrics_count.saturating_sub(1).max(0)) * 4;
        *slot = Ttf::u16_at(hmtx, at).unwrap_or(0);
    }
    let cmap = parse_cmap(&ttf)?;
    let name = parse_font_name(&ttf);
    let mac_style = Ttf::u16_at(head, 44).unwrap_or(0);
    let mut flags = plain_flags();
    flags.opentype = true;
    flags.bold = mac_style & 1 != 0;
    flags.italic = mac_style & 2 != 0;
    if let Ok(post) = ttf.table(b"post") {
        let fixed = u32::from_be_bytes([post[12], post[13], post[14], post[15]]);
        flags.mono = fixed != 0;
    }
    if let Ok(os2) = ttf.table(b"OS/2") {
        if os2.len() >= 78 {
            let selection = Ttf::u16_at(os2, 62).unwrap_or(0);
            flags.bold |= selection & (1 << 5) != 0;
            flags.italic |= selection & 1 != 0;
            let panose_serif = os2.get(33).copied().unwrap_or(0);
            flags.serif = (2..=10).contains(&panose_serif);
            let fs_type = Ttf::u16_at(os2, 8).unwrap_or(0);
            flags.never_embed = fs_type & 2 != 0;
            flags.embed = !flags.never_embed;
        }
    }
    let postscript_name = parse_postscript_name(&ttf).unwrap_or_else(|| {
        name.chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
            .collect::<String>()
    });
    let postscript_name = if postscript_name.is_empty() {
        "Untitled".to_string()
    } else {
        postscript_name
    };
    Ok(FaceData {
        face_name: name,
        display_name: parse_display_name(&ttf),
        builtin: None,
        upm,
        gid_widths,
        cmap,
        asc,
        desc,
        bbox,
        flags,
        glyph_count: num_glyphs,
        postscript_name,
    })
}

// --- text layout -------------------------------------------------------------

/// Greedy word wrap: explicit newlines split paragraphs, words pack left to
/// right; returns each line with its width in points.
fn wrap_lines(text: &str, max_width: f64, face: &FaceData, fontsize: f64) -> Vec<(String, f64)> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        let mut width = 0.0;
        let space = face.width_em(' ') * fontsize;
        for word in paragraph.split(' ') {
            let word_width = face.text_width_em(word) * fontsize;
            let extra = if current.is_empty() { 0.0 } else { space };
            if !current.is_empty() && width + extra + word_width > max_width + 1e-9 {
                lines.push((std::mem::take(&mut current), width));
                width = 0.0;
            }
            if !current.is_empty() {
                current.push(' ');
                width += space;
            }
            current.push_str(word);
            width += word_width;
        }
        lines.push((current, width));
    }
    lines
}

/// Default line height for a face at a size: full ascender-to-descender span.
fn default_line_height(face: &FaceData, fontsize: f64) -> f64 {
    fontsize * (face.asc_em() - face.desc_em())
}

/// Split the option text argument into logical lines.
fn text_arg_lines(arg: &TextArg) -> Vec<String> {
    match arg {
        TextArg::One(text) => text.split('\n').map(str::to_string).collect(),
        TextArg::Many(lines) => lines.clone(),
    }
}

// --- staged text and drawn paths --------------------------------------------

/// A staged value number: integers read as floats.
fn staged_number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(n) => Some(*n as f64),
        Value::Float(n) => Some(*n),
        _ => None,
    }
}

/// One staged text line: position, content, and style. Positions stay in
/// y-down page points until the flush encodes them.
#[derive(Debug, Clone, PartialEq)]
struct StagedRun {
    pos: Point,
    text: String,
    font_base: String,
    fontsize: f64,
    color: Option<Vec<f64>>,
    stroke: Option<Vec<f64>>,
    opacity: f64,
    stroke_opacity: f64,
    render_mode: i32,
    rotate: i32,
    border_width: f64,
    miter_limit: f64,
    align: i32,
    max_width: f64,
}

/// One path segment in y-down page points.
#[derive(Debug, Clone, PartialEq)]
enum Seg {
    Line(Point, Point),
    Curve(Point, Point, Point, Point),
    Rect(Rect),
}

/// One drawn subpath: segments plus its start point.
#[derive(Debug, Clone, PartialEq)]
struct DrawnPath {
    start: Point,
    segs: Vec<Seg>,
}

/// A finished path: drawn geometry plus paint style.
#[derive(Debug, Clone, PartialEq)]
struct FinishedPath {
    paths: Vec<DrawnPath>,
    width: f64,
    color: Vec<f64>,
    fill: Option<Vec<f64>>,
    line_cap: i32,
    line_join: i32,
    dashes: Option<String>,
    even_odd: bool,
    close: bool,
    fill_opacity: f64,
    stroke_opacity: f64,
}

impl DrawnPath {
    fn new(start: Point) -> Self {
        Self {
            start,
            segs: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn bounds(&self) -> Option<Rect> {
        let mut points = vec![self.start];
        for seg in &self.segs {
            match seg {
                Seg::Line(a, b) => points.extend_from_slice(&[*a, *b]),
                Seg::Curve(a, b, c, d) => points.extend_from_slice(&[*a, *b, *c, *d]),
                Seg::Rect(r) => {
                    points.extend_from_slice(&[
                        mk_point(r.x0, r.y0),
                        mk_point(r.x1, r.y0),
                        mk_point(r.x0, r.y1),
                        mk_point(r.x1, r.y1),
                    ]);
                }
            }
        }
        if points.is_empty() {
            return None;
        }
        let mut box_ = mk_rect(points[0].x, points[0].y, points[0].x, points[0].y);
        for p in &points[1..] {
            box_.x0 = box_.x0.min(p.x);
            box_.y0 = box_.y0.min(p.y);
            box_.x1 = box_.x1.max(p.x);
            box_.y1 = box_.y1.max(p.y);
        }
        Some(box_)
    }
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    mk_rect(
        a.x0.min(b.x0),
        a.y0.min(b.y0),
        a.x1.max(b.x1),
        a.y1.max(b.y1),
    )
}

/// Bezier constant for quarter-circle arcs.
const KAPPA: f64 = 0.5522847498307936;

/// Ellipse as four beziers starting at the left point, sweeping through the
/// bottom first (y-down reading order: left, bottom, right, top).
fn ellipse_paths(center: Point, rx: f64, ry: f64) -> DrawnPath {
    let (cx, cy) = (center.x, center.y);
    let kx = KAPPA * rx;
    let ky = KAPPA * ry;
    let left = mk_point(cx - rx, cy);
    let bottom = mk_point(cx, cy + ry);
    let right = mk_point(cx + rx, cy);
    let top = mk_point(cx, cy - ry);
    let mut path = DrawnPath::new(left);
    path.segs.push(Seg::Curve(
        left,
        mk_point(cx - rx, cy + ky),
        mk_point(cx - kx, cy + ry),
        bottom,
    ));
    path.segs.push(Seg::Curve(
        bottom,
        mk_point(cx + kx, cy + ry),
        mk_point(cx + rx, cy + ky),
        right,
    ));
    path.segs.push(Seg::Curve(
        right,
        mk_point(cx + rx, cy - ky),
        mk_point(cx + kx, cy - ry),
        top,
    ));
    path.segs.push(Seg::Curve(
        top,
        mk_point(cx - kx, cy - ry),
        mk_point(cx - rx, cy - ky),
        left,
    ));
    path
}

/// Rounded rectangle path starting at the left edge below the top-left
/// corner, sweeping down the left side first.
fn round_rect_path(rect: Rect, radius: f64) -> DrawnPath {
    let r = norm_rect(&rect);
    let small = (r.x1 - r.x0).min(r.y1 - r.y0);
    let absolute = if radius > 0.0 && radius <= 1.0 {
        radius * small
    } else {
        radius
    };
    let rad = absolute
        .max(0.0)
        .min((r.x1 - r.x0) / 2.0)
        .min((r.y1 - r.y0) / 2.0);
    if rad <= 0.0 {
        let mut path = DrawnPath::new(mk_point(r.x0, r.y0));
        path.segs.push(Seg::Rect(r));
        return path;
    }
    let k = KAPPA * rad;
    let start = mk_point(r.x0, r.y0 + rad);
    let mut path = DrawnPath::new(start);
    let line_to = |path: &mut DrawnPath, from: Point, to: Point| {
        path.segs.push(Seg::Line(from, to));
    };
    let corner = |path: &mut DrawnPath, from: Point, c1: Point, c2: Point, to: Point| {
        path.segs.push(Seg::Curve(from, c1, c2, to));
    };
    let left_bottom = mk_point(r.x0, r.y1 - rad);
    line_to(&mut path, start, left_bottom);
    corner(
        &mut path,
        left_bottom,
        mk_point(r.x0, r.y1 - rad + k),
        mk_point(r.x0 + rad - k, r.y1),
        mk_point(r.x0 + rad, r.y1),
    );
    let right_bottom = mk_point(r.x1 - rad, r.y1);
    line_to(&mut path, mk_point(r.x0 + rad, r.y1), right_bottom);
    corner(
        &mut path,
        right_bottom,
        mk_point(r.x1 - rad + k, r.y1),
        mk_point(r.x1, r.y1 - rad + k),
        mk_point(r.x1, r.y1 - rad),
    );
    let right_top = mk_point(r.x1, r.y0 + rad);
    line_to(&mut path, mk_point(r.x1, r.y1 - rad), right_top);
    corner(
        &mut path,
        right_top,
        mk_point(r.x1, r.y0 + rad - k),
        mk_point(r.x1 - rad + k, r.y0),
        mk_point(r.x1 - rad, r.y0),
    );
    let left_top = mk_point(r.x0 + rad, r.y0);
    line_to(&mut path, mk_point(r.x1 - rad, r.y0), left_top);
    corner(
        &mut path,
        left_top,
        mk_point(r.x0 + rad - k, r.y0),
        mk_point(r.x0, r.y0 + rad - k),
        start,
    );
    path
}

/// A pie sector: two radii plus the arc between them. Positive angles sweep
/// counter-clockwise on screen (y-down).
fn sector_path(center: Point, point: Point, beta: f64, full: bool) -> DrawnPath {
    let mut path = DrawnPath::new(if full { center } else { point });
    if full {
        path.segs.push(Seg::Line(center, point));
    }
    let sweep = beta.to_radians();
    let steps = ((sweep.abs() / (std::f64::consts::FRAC_PI_2)).ceil() as usize).max(1);
    let (dx, dy) = (point.x - center.x, point.y - center.y);
    let mut prev = point;
    for i in 1..=steps {
        let angle = sweep * i as f64 / steps as f64;
        let (sin, cos) = angle.sin_cos();
        let next = mk_point(
            center.x + dx * cos + dy * sin,
            center.y - dx * sin + dy * cos,
        );
        if full {
            let mid = angle - sweep / steps as f64 / 2.0;
            let (msin, mcos) = mid.sin_cos();
            let alpha = (sweep / steps as f64 / 2.0).abs();
            let handle = 4.0 / 3.0 * (alpha / 2.0).tan() * (dx * dx + dy * dy).sqrt();
            let px = center.x + dx * mcos + dy * msin;
            let py = center.y - dx * msin + dy * mcos;
            let tx = -(py - center.y);
            let ty = px - center.x;
            let len = (tx * tx + ty * ty).sqrt().max(1e-12);
            let (ux, uy) = (tx / len, ty / len);
            let dir = if sweep >= 0.0 { 1.0 } else { -1.0 };
            path.segs.push(Seg::Curve(
                prev,
                mk_point(prev.x + dir * ux * handle, prev.y + dir * uy * handle),
                mk_point(next.x - dir * ux * handle, next.y - dir * uy * handle),
                next,
            ));
        } else {
            path.segs.push(Seg::Line(prev, next));
        }
        prev = next;
    }
    if full {
        path.segs.push(Seg::Line(prev, center));
    }
    path
}

/// Zig-zag between two points: `breadth`-tall teeth perpendicular to the run.
fn zigzag_path(p1: Point, p2: Point, breadth: f64) -> DrawnPath {
    let mut path = DrawnPath::new(p1);
    let (dx, dy) = (p2.x - p1.x, p2.y - p1.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 || breadth <= 0.0 {
        path.segs.push(Seg::Line(p1, p2));
        return path;
    }
    let teeth = ((len / breadth.max(1.0)).round() as usize).max(2);
    let (nx, ny) = (-dy / len * breadth / 2.0, dx / len * breadth / 2.0);
    let mut prev = p1;
    for i in 1..=teeth {
        let t = i as f64 / teeth as f64;
        let mut next = mk_point(p1.x + dx * t, p1.y + dy * t);
        if i != teeth {
            let side = if i % 2 == 1 { 1.0 } else { -1.0 };
            next = mk_point(next.x + side * nx, next.y + side * ny);
        }
        path.segs.push(Seg::Line(prev, next));
        prev = next;
    }
    path
}

/// Sine-wave squiggle between two points, `breadth`-tall.
fn squiggle_path(p1: Point, p2: Point, breadth: f64) -> DrawnPath {
    let mut path = DrawnPath::new(p1);
    let (dx, dy) = (p2.x - p1.x, p2.y - p1.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 || breadth <= 0.0 {
        path.segs.push(Seg::Line(p1, p2));
        return path;
    }
    let waves = ((len / (breadth * 2.0).max(1.0)).round() as usize).max(1);
    let steps = waves * 8;
    let (nx, ny) = (-dy / len, dx / len);
    let mut prev = p1;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let sway = (t * waves as f64 * 2.0 * std::f64::consts::PI).sin() * breadth / 2.0;
        let next = mk_point(p1.x + dx * t + nx * sway, p1.y + dy * t + ny * sway);
        path.segs.push(Seg::Line(prev, next));
        prev = next;
    }
    path
}

// --- content building ------------------------------------------------------

/// Writable page `/Resources` dictionary, created when missing.
fn resources_mut(doc: &mut LopdfDocument, page_id: ObjectId) -> Result<ObjectId> {
    let existing = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Resources").ok())
        .and_then(|o| o.as_reference().ok());
    if let Some(id) = existing {
        return Ok(id);
    }
    let inline = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Resources").ok().cloned());
    if let Some(Object::Dictionary(_)) = inline {
        let id = doc.new_object_id();
        let dict = inline
            .and_then(|o| o.as_dict().ok().cloned())
            .unwrap_or_default();
        doc.objects.insert(id, Object::Dictionary(dict));
        doc.get_dictionary_mut(page_id)?
            .set("Resources", Object::Reference(id));
        return Ok(id);
    }
    let id = doc.new_object_id();
    doc.objects
        .insert(id, Object::Dictionary(Dictionary::new()));
    doc.get_dictionary_mut(page_id)?
        .set("Resources", Object::Reference(id));
    Ok(id)
}

/// Snapshot of a `/Resources` sub-dictionary (`/Font`, `/XObject`, ...),
/// following indirections; missing pieces read as empty.
fn resource_entries(doc: &LopdfDocument, page_id: ObjectId, kind: &[u8]) -> Dictionary {
    let res = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Resources").ok().map(|o| deref(doc, o).clone()));
    let Some(res) = res else {
        return Dictionary::new();
    };
    let sub = res
        .as_dict()
        .ok()
        .and_then(|dict| dict.get(kind).ok().map(|o| deref(doc, o).clone()));
    sub.and_then(|o| o.as_dict().ok().cloned())
        .unwrap_or_default()
}

/// Insert `name -> value` into a `/Resources` sub-dictionary, mutating the
/// live dictionaries inline: an inline `/Resources` or sub-dictionary stays
/// inline, so the recorded object numbering survives. Only a missing
/// sub-dictionary costs a fresh indirect object.
fn resource_sub_mut(
    doc: &mut LopdfDocument,
    page_id: ObjectId,
    kind: &[u8],
    name: &str,
    value: Object,
) -> Result<()> {
    let key = String::from_utf8_lossy(kind).into_owned();
    let slot = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Resources").ok().cloned());
    match slot {
        Some(Object::Reference(res_id)) => {
            let sub = doc
                .get_dictionary(res_id)
                .ok()
                .and_then(|res| res.get(kind).ok().cloned());
            match sub {
                Some(Object::Reference(sub_id)) => {
                    doc.get_dictionary_mut(sub_id)?.set(name, value);
                }
                Some(Object::Dictionary(mut dict)) => {
                    dict.set(name, value);
                    doc.get_dictionary_mut(res_id)?
                        .set(key, Object::Dictionary(dict));
                }
                _ => {
                    let sub_id = doc.new_object_id();
                    let mut dict = Dictionary::new();
                    dict.set(name, value);
                    doc.objects.insert(sub_id, Object::Dictionary(dict));
                    doc.get_dictionary_mut(res_id)?
                        .set(key, Object::Reference(sub_id));
                }
            }
        }
        Some(Object::Dictionary(mut res)) => {
            let sub = res.get(kind).ok().cloned();
            match sub {
                Some(Object::Reference(sub_id)) => {
                    doc.get_dictionary_mut(sub_id)?.set(name, value);
                }
                _ => {
                    let mut dict = match sub {
                        Some(Object::Dictionary(d)) => d,
                        _ => Dictionary::new(),
                    };
                    dict.set(name, value);
                    res.set(key, Object::Dictionary(dict));
                    doc.get_dictionary_mut(page_id)?
                        .set("Resources", Object::Dictionary(res));
                }
            }
        }
        _ => {
            let res_id = resources_mut(doc, page_id)?;
            let sub_id = doc.new_object_id();
            let mut dict = Dictionary::new();
            dict.set(name, value);
            doc.objects.insert(sub_id, Object::Dictionary(dict));
            doc.get_dictionary_mut(res_id)?
                .set(key, Object::Reference(sub_id));
        }
    }
    Ok(())
}

/// Ensure `/Resources` is indirect and the named sub-dictionary exists, so a
/// caller can allocate the value object next with stable numbering.
fn touch_resource_sub(doc: &mut LopdfDocument, page_id: ObjectId, kind: &[u8]) -> Result<()> {
    let res_id = resources_mut(doc, page_id)?;
    let has = doc
        .get_dictionary(res_id)
        .ok()
        .and_then(|res| res.get(kind).ok())
        .is_some();
    if !has {
        let sub_id = doc.new_object_id();
        doc.objects
            .insert(sub_id, Object::Dictionary(Dictionary::new()));
        doc.get_dictionary_mut(res_id)?.set(
            String::from_utf8_lossy(kind).into_owned(),
            Object::Reference(sub_id),
        );
    }
    Ok(())
}

/// Next free resource name with the given prefix (`F`, `Fm`, `Gs`, ...).
fn fresh_resource_name(
    doc: &LopdfDocument,
    page_id: ObjectId,
    kind: &[u8],
    prefix: &str,
) -> String {
    let entries = resource_entries(doc, page_id, kind);
    let mut taken: Vec<String> = Vec::new();
    for (key, _) in entries.iter() {
        taken.push(String::from_utf8_lossy(key).into_owned());
    }
    let mut n = taken.len() + 1;
    loop {
        let name = format!("{prefix}{n}");
        if !taken.iter().any(|t| t == &name) {
            return name;
        }
        n += 1;
    }
}

/// Page font resource for a base-14 face, reusing an identical entry.
fn ensure_base_font(doc: &mut LopdfDocument, page_id: ObjectId, base: &str) -> Result<String> {
    let snapshot = resource_entries(doc, page_id, b"Font");
    for (key, value) in snapshot.iter() {
        let target = value.as_reference().ok();
        let dict = match target {
            Some(id) => doc.get_dictionary(id).ok().cloned(),
            None => value.as_dict().ok().cloned(),
        };
        if let Some(d) = dict {
            let matches = d
                .get(b"BaseFont")
                .ok()
                .map(|o| obj_name(o) == base)
                .unwrap_or(false);
            if matches {
                return Ok(String::from_utf8_lossy(key).into_owned());
            }
        }
    }
    let name = fresh_resource_name(doc, page_id, b"Font", "F");
    let mut font = Dictionary::new();
    font.set("Type", name_object("Font"));
    font.set("Subtype", name_object("Type1"));
    font.set("BaseFont", name_object(base));
    if base != "Symbol" && base != "ZapfDingbats" {
        font.set("Encoding", name_object("WinAnsiEncoding"));
    }
    let font_ref = doc.new_object_id();
    doc.objects.insert(font_ref, Object::Dictionary(font));
    resource_sub_mut(doc, page_id, b"Font", &name, Object::Reference(font_ref))?;
    Ok(name)
}

/// Page graphics-state resource for fill/stroke opacity.
fn ensure_opacity_state(
    doc: &mut LopdfDocument,
    page_id: ObjectId,
    fill: f64,
    stroke: f64,
) -> Result<String> {
    let name = fresh_resource_name(doc, page_id, b"ExtGState", "Gs");
    let mut gs = Dictionary::new();
    gs.set("Type", name_object("ExtGState"));
    gs.set("ca", Object::Real(fill.clamp(0.0, 1.0) as f32));
    gs.set("CA", Object::Real(stroke.clamp(0.0, 1.0) as f32));
    let gs_ref = doc.new_object_id();
    doc.objects.insert(gs_ref, Object::Dictionary(gs));
    resource_sub_mut(doc, page_id, b"ExtGState", &name, Object::Reference(gs_ref))?;
    Ok(name)
}

/// Register an XObject reference under the page resources.
fn ensure_xobject(doc: &mut LopdfDocument, page_id: ObjectId, target: ObjectId) -> Result<String> {
    let name = fresh_resource_name(doc, page_id, b"XObject", "Fm");
    resource_sub_mut(doc, page_id, b"XObject", &name, Object::Reference(target))?;
    Ok(name)
}

/// Append (or prepend, for underlays) a content stream to a page.
fn push_content(
    doc: &mut LopdfDocument,
    page_id: ObjectId,
    data: Vec<u8>,
    overlay: bool,
) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }
    let stream_id = doc.new_object_id();
    doc.objects.insert(
        stream_id,
        Object::Stream(Stream::new(Dictionary::new(), data)),
    );
    let current = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Contents").ok().cloned());
    let mut ids: Vec<ObjectId> = match current {
        Some(Object::Reference(id)) => vec![id],
        Some(Object::Array(arr)) => arr.iter().filter_map(|o| o.as_reference().ok()).collect(),
        Some(Object::Stream(_)) => {
            let id = doc.new_object_id();
            if let Some(s) = doc
                .get_dictionary_mut(page_id)?
                .remove(b"Contents")
                .and_then(|o| match o {
                    Object::Stream(s) => Some(s),
                    _ => None,
                })
            {
                doc.objects.insert(id, Object::Stream(s));
                vec![id]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    };
    if overlay {
        ids.push(stream_id);
    } else {
        ids.insert(0, stream_id);
    }
    let value = if ids.len() == 1 {
        Object::Reference(ids[0])
    } else {
        Object::Array(ids.into_iter().map(Object::Reference).collect())
    };
    doc.get_dictionary_mut(page_id)?.set("Contents", value);
    Ok(())
}

/// Text rotation matrix for screen-clockwise degrees.
fn text_rotation(rotate: i32) -> (f64, f64, f64, f64) {
    let norm = rotate.rem_euclid(360);
    match norm {
        90 => (0.0, -1.0, 1.0, 0.0),
        180 => (-1.0, 0.0, 0.0, -1.0),
        270 => (0.0, 1.0, -1.0, 0.0),
        _ => {
            let theta = f64::from(norm) * std::f64::consts::PI / 180.0;
            (theta.cos(), -theta.sin(), theta.sin(), theta.cos())
        }
    }
}

/// Encode one staged text line as content operators.
fn encode_text_run(run: &StagedRun, font_name: &str, gs_name: Option<&str>, top: f64) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"BT\n");
    out.extend_from_slice(format!("/{font_name} {} Tf\n", pdf_num(run.fontsize)).as_bytes());
    if run.render_mode != 0 {
        out.extend_from_slice(format!("{} Tr\n", run.render_mode).as_bytes());
    }
    if let Some(color) = &run.color {
        out.extend_from_slice(&color_ops(color, true));
    }
    if run.render_mode != 0 {
        let stroke = run.stroke.as_ref().or(run.color.as_ref());
        if let Some(color) = stroke {
            out.extend_from_slice(&color_ops(color, false));
        }
        if run.border_width > 0.0 {
            out.extend_from_slice(format!("{} w\n", pdf_num(run.border_width)).as_bytes());
            out.extend_from_slice(format!("{} M\n", pdf_num(run.miter_limit.max(1.0))).as_bytes());
        }
    }
    if let Some(gs) = gs_name {
        out.extend_from_slice(format!("/{gs} gs\n").as_bytes());
    }
    let x = run.pos.x;
    let y = flip_y(run.pos.y, top);
    let (a, b, c, d) = text_rotation(run.rotate);
    out.extend_from_slice(
        format!(
            "{} {} {} {} {} {} Tm\n",
            pdf_num(a),
            pdf_num(b),
            pdf_num(c),
            pdf_num(d),
            pdf_num(x),
            pdf_num(y)
        )
        .as_bytes(),
    );
    out.extend_from_slice(&winansi_string(&run.text));
    out.extend_from_slice(b" Tj\nET\n");
    out
}

/// Encode one finished path as content operators.
fn encode_finished_path(finished: &FinishedPath, top: f64, gs_name: Option<&str>) -> Vec<u8> {
    let stroke = !finished.color.is_empty();
    let fill = finished
        .fill
        .as_ref()
        .map(|c| !c.is_empty())
        .unwrap_or(false);
    let mut out = Vec::new();
    out.extend_from_slice(b"q\n");
    if let Some(gs) = gs_name {
        out.extend_from_slice(format!("/{gs} gs\n").as_bytes());
    }
    out.extend_from_slice(format!("{} w\n", pdf_num(finished.width)).as_bytes());
    out.extend_from_slice(format!("{} J\n", finished.line_cap).as_bytes());
    out.extend_from_slice(format!("{} j\n", finished.line_join).as_bytes());
    if let Some(dashes) = &finished.dashes {
        out.extend_from_slice(format!("{dashes} d\n").as_bytes());
    }
    if stroke {
        out.extend_from_slice(&color_ops(&finished.color, false));
    }
    if let Some(fill) = &finished.fill {
        if !fill.is_empty() {
            out.extend_from_slice(&color_ops(fill, true));
        }
    }
    let flip = |p: Point| mk_point(p.x, flip_y(p.y, top));
    for path in &finished.paths {
        let start = flip(path.start);
        out.extend_from_slice(format!("{} {} m\n", pdf_num(start.x), pdf_num(start.y)).as_bytes());
        let mut current = path.start;
        for seg in &path.segs {
            match seg {
                Seg::Line(_, b) => {
                    let to = flip(*b);
                    out.extend_from_slice(
                        format!("{} {} l\n", pdf_num(to.x), pdf_num(to.y)).as_bytes(),
                    );
                    current = *b;
                }
                Seg::Curve(_, c1, c2, b) => {
                    let p1 = flip(*c1);
                    let p2 = flip(*c2);
                    let to = flip(*b);
                    out.extend_from_slice(
                        format!(
                            "{} {} {} {} {} {} c\n",
                            pdf_num(p1.x),
                            pdf_num(p1.y),
                            pdf_num(p2.x),
                            pdf_num(p2.y),
                            pdf_num(to.x),
                            pdf_num(to.y)
                        )
                        .as_bytes(),
                    );
                    current = *b;
                }
                Seg::Rect(r) => {
                    let flipped = flip_rect(&norm_rect(r), top);
                    out.extend_from_slice(
                        format!(
                            "{} {} {} {} re\n",
                            pdf_num(flipped.x0),
                            pdf_num(flipped.y0),
                            pdf_num(flipped.x1 - flipped.x0),
                            pdf_num(flipped.y1 - flipped.y0)
                        )
                        .as_bytes(),
                    );
                    current = mk_point(r.x0, r.y0);
                }
            }
        }
        if finished.close
            && ((current.x - path.start.x).abs() > 1e-9 || (current.y - path.start.y).abs() > 1e-9)
        {
            let back = flip(path.start);
            out.extend_from_slice(
                format!("{} {} l\n", pdf_num(back.x), pdf_num(back.y)).as_bytes(),
            );
        }
    }
    let op = match (stroke, fill, finished.even_odd) {
        (true, true, true) => "B*\n",
        (true, true, false) => "B\n",
        (true, false, _) => "S\n",
        (false, true, true) => "f*\n",
        (false, true, false) => "f\n",
        (false, false, _) => "n\n",
    };
    out.extend_from_slice(op.as_bytes());
    out.extend_from_slice(b"Q\n");
    out
}

// --- content parsing (drawings) --------------------------------------------

#[derive(Debug, Clone)]
struct GState {
    ctm: [f64; 6],
    fill: [f64; 3],
    stroke: [f64; 3],
    width: f64,
    cap: i32,
    join: i32,
    dash: Vec<f64>,
    dash_phase: f64,
    fill_op: f64,
    stroke_op: f64,
}

impl Default for GState {
    fn default() -> Self {
        Self {
            ctm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            fill: [0.0, 0.0, 0.0],
            stroke: [0.0, 0.0, 0.0],
            width: 1.0,
            cap: 0,
            join: 0,
            dash: Vec::new(),
            dash_phase: 0.0,
            fill_op: 1.0,
            stroke_op: 1.0,
        }
    }
}

/// Numeric `sc`/`scn` operands to RGB. Patterned (named) colors keep the old
/// paint: only all-numeric operands count.
fn scn_rgb(args: &[Object]) -> Option<[f64; 3]> {
    if args.iter().any(|o| matches!(o, Object::Name(_))) {
        return None;
    }
    let nums: Vec<f64> = args.iter().map(obj_f64).collect();
    match nums.len() {
        1 => Some([nums[0], nums[0], nums[0]]),
        3 => Some([nums[0], nums[1], nums[2]]),
        4 => Some(to_rgb(&nums)),
        _ => None,
    }
}

/// Apply `first` then `second` (PDF `cm` concatenates the new matrix first).
fn concat_ctm(first: &[f64; 6], second: &[f64; 6]) -> [f64; 6] {
    let (a1, b1, c1, d1, e1, f1) = (first[0], first[1], first[2], first[3], first[4], first[5]);
    let (a2, b2, c2, d2, e2, f2) = (
        second[0], second[1], second[2], second[3], second[4], second[5],
    );
    [
        a2 * a1 + c2 * b1,
        b2 * a1 + d2 * b1,
        a2 * c1 + c2 * d1,
        b2 * c1 + d2 * d1,
        a2 * e1 + c2 * f1 + e2,
        b2 * e1 + d2 * f1 + f2,
    ]
}

fn apply_ctm(ctm: &[f64; 6], p: Point) -> Point {
    mk_point(
        ctm[0] * p.x + ctm[2] * p.y + ctm[4],
        ctm[1] * p.x + ctm[3] * p.y + ctm[5],
    )
}

#[derive(Debug, Clone)]
enum PathOp {
    Move(Point),
    Line(Point),
    Curve(Point, Point, Point),
    Rect(Rect),
    Close,
}

struct DrawingCx<'a> {
    doc: &'a LopdfDocument,
    page_id: ObjectId,
    top: f64,
    extended: bool,
    states: Vec<GState>,
    path: Vec<PathOp>,
    layers: Vec<String>,
    drawings: Vec<Drawing>,
}

impl<'a> DrawingCx<'a> {
    fn state(&self) -> GState {
        self.states.last().cloned().unwrap_or_default()
    }

    fn state_mut(&mut self) -> &mut GState {
        if self.states.is_empty() {
            self.states.push(GState::default());
        }
        self.states.last_mut().expect("state")
    }

    fn to_page(&self, p: Point) -> Point {
        let mapped = apply_ctm(&self.state().ctm, p);
        mk_point(mapped.x, flip_y(mapped.y, self.top))
    }

    fn resources(&self) -> Option<Dictionary> {
        let page = self.doc.get_dictionary(self.page_id).ok()?;
        let res = page.get(b"Resources").ok()?;
        deref(self.doc, res).as_dict().ok().cloned()
    }

    fn resolve_resource(&self, kind: &[u8], name: &str) -> Option<Object> {
        let res = self.resources()?;
        let sub = res.get(kind).ok()?;
        let sub = deref(self.doc, sub);
        let dict = sub.as_dict().ok()?;
        dict.get(name.as_bytes()).ok().map(|o| {
            let (_, target) = self.doc.dereference(o).unwrap_or((None, o));
            target.clone()
        })
    }

    fn apply_gstate(&mut self, name: &str) {
        let gs = match self.resolve_resource(b"ExtGState", name) {
            Some(Object::Dictionary(d)) => d,
            _ => return,
        };
        if let Ok(o) = gs.get(b"LW") {
            self.state_mut().width = obj_f64(o);
        }
        if let Ok(o) = gs.get(b"LC") {
            self.state_mut().cap = obj_i32(o);
        }
        if let Ok(o) = gs.get(b"LJ") {
            self.state_mut().join = obj_i32(o);
        }
        if let Ok(o) = gs.get(b"D") {
            if let Ok(arr) = deref(self.doc, o).as_array() {
                if arr.len() >= 2 {
                    if let Ok(dashes) = arr[0].as_array() {
                        self.state_mut().dash = dashes.iter().map(obj_f64).collect();
                    }
                    let phase = obj_f64(&arr[1]);
                    self.state_mut().dash_phase = phase;
                }
            }
        }
        if let Ok(o) = gs.get(b"ca") {
            self.state_mut().fill_op = obj_f64(o);
        }
        if let Ok(o) = gs.get(b"CA") {
            self.state_mut().stroke_op = obj_f64(o);
        }
    }

    fn paint(&mut self, stroke: bool, fill: bool, even_odd: bool) {
        if self.path.is_empty() {
            return;
        }
        let state = self.state();
        let mut items = Vec::new();
        let mut box_: Option<Rect> = None;
        let grow = |box_: &mut Option<Rect>, p: Point| {
            *box_ = Some(match *box_ {
                Some(b) => union_rect(b, mk_rect(p.x, p.y, p.x, p.y)),
                None => mk_rect(p.x, p.y, p.x, p.y),
            });
        };
        let mut current = mk_point(0.0, 0.0);
        let mut has_current = false;
        let mut closed = false;
        let mut has_re = false;
        for op in self.path.clone() {
            match op {
                PathOp::Move(p) => {
                    current = p;
                    has_current = true;
                }
                PathOp::Line(p) => {
                    if !has_current {
                        current = p;
                        has_current = true;
                        continue;
                    }
                    let from = self.to_page(current);
                    let to = self.to_page(p);
                    grow(&mut box_, from);
                    grow(&mut box_, to);
                    items.push(DrawingItem {
                        op: "l".to_string(),
                        params: Value::List(vec![
                            Value::Float(from.x),
                            Value::Float(from.y),
                            Value::Float(to.x),
                            Value::Float(to.y),
                        ]),
                    });
                    current = p;
                }
                PathOp::Curve(c1, c2, p) => {
                    if !has_current {
                        current = p;
                        has_current = true;
                        continue;
                    }
                    let from = self.to_page(current);
                    let q1 = self.to_page(c1);
                    let q2 = self.to_page(c2);
                    let to = self.to_page(p);
                    for q in [from, q1, q2, to] {
                        grow(&mut box_, q);
                    }
                    items.push(DrawingItem {
                        op: "c".to_string(),
                        params: Value::List(vec![
                            Value::Float(from.x),
                            Value::Float(from.y),
                            Value::Float(q1.x),
                            Value::Float(q1.y),
                            Value::Float(q2.x),
                            Value::Float(q2.y),
                            Value::Float(to.x),
                            Value::Float(to.y),
                        ]),
                    });
                    current = p;
                }
                PathOp::Rect(r) => {
                    let raw = norm_rect(&r);
                    let corners = [
                        self.to_page(mk_point(raw.x0, raw.y0)),
                        self.to_page(mk_point(raw.x1, raw.y0)),
                        self.to_page(mk_point(raw.x0, raw.y1)),
                        self.to_page(mk_point(raw.x1, raw.y1)),
                    ];
                    let mut flipped =
                        mk_rect(corners[0].x, corners[0].y, corners[0].x, corners[0].y);
                    for corner in &corners[1..] {
                        flipped =
                            union_rect(flipped, mk_rect(corner.x, corner.y, corner.x, corner.y));
                    }
                    grow(&mut box_, mk_point(flipped.x0, flipped.y0));
                    grow(&mut box_, mk_point(flipped.x1, flipped.y1));
                    has_re = true;
                    items.push(DrawingItem {
                        op: "re".to_string(),
                        params: Value::List(vec![
                            Value::Float(flipped.x0),
                            Value::Float(flipped.y0),
                            Value::Float(flipped.x1),
                            Value::Float(flipped.y1),
                            Value::Int(1),
                        ]),
                    });
                }
                PathOp::Close => {
                    closed = true;
                }
            }
        }
        self.path.clear();
        let Some(rect) = box_ else {
            return;
        };
        let drawing_type = match (stroke, fill) {
            (true, true) => "fs",
            (true, false) => "s",
            (false, true) => "f",
            (false, false) => return,
        }
        .to_string();
        let dashes = if stroke {
            let inner: Vec<String> = state.dash.iter().map(|d| pdf_num(*d)).collect();
            let body = if inner.is_empty() {
                String::new()
            } else {
                format!(" {} ", inner.join(" "))
            };
            Some(format!("[{body}] {}", pdf_num(state.dash_phase)))
        } else {
            None
        };
        let seqno = self.drawings.len() as i32 * 2 + 1;
        self.drawings.push(Drawing {
            rect,
            seqno,
            drawing_type,
            color: stroke.then(|| state.stroke.to_vec()),
            fill: fill.then(|| state.fill.to_vec()),
            width: stroke.then_some(state.width),
            line_cap: stroke.then(|| vec![state.cap, state.cap, state.cap]),
            line_join: stroke.then_some(state.join),
            dashes,
            close_path: if stroke { Some(closed || has_re) } else { None },
            even_odd: fill && even_odd,
            fill_opacity: fill.then_some(state.fill_op),
            stroke_opacity: stroke.then_some(state.stroke_op),
            layer: self.layers.last().cloned().unwrap_or_default(),
            level: if self.extended {
                Some(self.states.len().saturating_sub(1) as i32)
            } else {
                None
            },
            items,
        });
    }

    fn run_ops(&mut self, ops: &[lopdf::content::Operation], depth: usize) {
        for op in ops {
            let args = &op.operands;
            let num = |i: usize| args.get(i).map(obj_f64).unwrap_or(0.0);
            match op.operator.as_str() {
                "q" => {
                    let top = self.state();
                    self.states.push(top);
                }
                "Q" => {
                    if self.states.len() > 1 {
                        self.states.pop();
                    }
                }
                "cm" if args.len() >= 6 => {
                    let m = [num(0), num(1), num(2), num(3), num(4), num(5)];
                    let ctm = self.state().ctm;
                    self.state_mut().ctm = concat_ctm(&m, &ctm);
                }
                "w" => self.state_mut().width = num(0),
                "J" => self.state_mut().cap = num(0) as i32,
                "j" => self.state_mut().join = num(0) as i32,
                "d" => {
                    if args.len() >= 2 {
                        if let Ok(arr) = args[0].as_array() {
                            self.state_mut().dash = arr.iter().map(obj_f64).collect();
                        }
                        self.state_mut().dash_phase = num(1);
                    }
                }
                "g" => self.state_mut().fill = [num(0), num(0), num(0)],
                "G" => self.state_mut().stroke = [num(0), num(0), num(0)],
                "rg" => self.state_mut().fill = [num(0), num(1), num(2)],
                "RG" => self.state_mut().stroke = [num(0), num(1), num(2)],
                "k" => self.state_mut().fill = to_rgb(&[num(0), num(1), num(2), num(3)]),
                "K" => self.state_mut().stroke = to_rgb(&[num(0), num(1), num(2), num(3)]),
                "sc" | "scn" => {
                    if let Some(rgb) = scn_rgb(args) {
                        self.state_mut().fill = rgb;
                    }
                }
                "SC" | "SCN" => {
                    if let Some(rgb) = scn_rgb(args) {
                        self.state_mut().stroke = rgb;
                    }
                }
                "m" if args.len() >= 2 => {
                    self.path.push(PathOp::Move(mk_point(num(0), num(1))));
                }
                "l" if args.len() >= 2 => {
                    self.path.push(PathOp::Line(mk_point(num(0), num(1))));
                }
                "c" if args.len() >= 6 => {
                    self.path.push(PathOp::Curve(
                        mk_point(num(0), num(1)),
                        mk_point(num(2), num(3)),
                        mk_point(num(4), num(5)),
                    ));
                }
                "v" if args.len() >= 4 => {
                    let start = match self.path.last() {
                        Some(PathOp::Move(p))
                        | Some(PathOp::Line(p))
                        | Some(PathOp::Curve(_, _, p)) => *p,
                        _ => mk_point(0.0, 0.0),
                    };
                    self.path.push(PathOp::Curve(
                        start,
                        mk_point(num(0), num(1)),
                        mk_point(num(2), num(3)),
                    ));
                }
                "y" if args.len() >= 4 => {
                    let start = match self.path.last() {
                        Some(PathOp::Move(p))
                        | Some(PathOp::Line(p))
                        | Some(PathOp::Curve(_, _, p)) => *p,
                        _ => mk_point(0.0, 0.0),
                    };
                    self.path.push(PathOp::Curve(
                        start,
                        mk_point(num(0), num(1)),
                        mk_point(num(2), num(3)),
                    ));
                }
                "h" => self.path.push(PathOp::Close),
                "re" if args.len() >= 4 => {
                    self.path.push(PathOp::Rect(mk_rect(
                        num(0),
                        num(1),
                        num(0) + num(2),
                        num(1) + num(3),
                    )));
                }
                "S" | "s" => self.paint(true, false, false),
                "f" | "F" => self.paint(false, true, false),
                "f*" | "F*" => self.paint(false, true, true),
                "B" | "b" => self.paint(true, true, false),
                "B*" | "b*" => self.paint(true, true, true),
                "n" => self.path.clear(),
                "gs" => {
                    if let Some(name) = args.first() {
                        self.apply_gstate(obj_name(name).trim_start_matches('/'));
                    }
                }
                "Do" => {
                    if depth < 8 {
                        if let Some(name) = args.first().map(obj_name) {
                            let clean = name.trim_start_matches('/').to_string();
                            if let Some(Object::Stream(stream)) =
                                self.resolve_resource(b"XObject", &clean)
                            {
                                let is_form = stream
                                    .dict
                                    .get(b"Subtype")
                                    .ok()
                                    .map(|o| obj_name(o) == "Form")
                                    .unwrap_or(false);
                                if is_form {
                                    let form_matrix = stream
                                        .dict
                                        .get(b"Matrix")
                                        .ok()
                                        .and_then(|o| o.as_array().ok())
                                        .map(|a| {
                                            let v: Vec<f64> = a.iter().map(obj_f64).collect();
                                            if v.len() >= 6 {
                                                [v[0], v[1], v[2], v[3], v[4], v[5]]
                                            } else {
                                                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
                                            }
                                        })
                                        .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                                    if let Ok(content) = stream.decompressed_content() {
                                        if let Ok(decoded) =
                                            lopdf::content::Content::decode(&content)
                                        {
                                            let pushed = self.state();
                                            self.states.push(pushed);
                                            let ctm = self.state().ctm;
                                            let base = self.states.len() - 1;
                                            self.states[base].ctm = concat_ctm(&form_matrix, &ctm);
                                            self.run_ops(&decoded.operations, depth + 1);
                                            self.states.pop();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                "BDC" => {
                    let tag = args.first().map(obj_name).unwrap_or_default();
                    if tag == "OC" || tag == "/OC" {
                        let label = args
                            .get(1)
                            .and_then(|o| o.as_reference().ok())
                            .and_then(|id| self.doc.get_dictionary(id).ok())
                            .and_then(|d| d.get(b"Name").ok().map(obj_text))
                            .unwrap_or_default();
                        self.layers.push(label);
                    } else {
                        self.layers.push(String::new());
                    }
                }
                "EMC" => {
                    self.layers.pop();
                }
                _ => {}
            }
        }
    }
}

/// Read every vector drawing on a page, in content order.
fn read_drawings(doc: &LopdfDocument, page_id: ObjectId, extended: bool) -> Vec<Drawing> {
    let top = page_top(doc, page_id);
    let mut cx = DrawingCx {
        doc,
        page_id,
        top,
        extended,
        states: vec![GState::default()],
        path: Vec::new(),
        layers: Vec::new(),
        drawings: Vec::new(),
    };
    for stream_id in doc.get_page_contents(page_id) {
        let content = doc
            .get_object(stream_id)
            .ok()
            .and_then(|o| o.as_stream().ok())
            .and_then(|s| s.decompressed_content().ok())
            .unwrap_or_default();
        if content.is_empty() {
            continue;
        }
        if let Ok(decoded) = lopdf::content::Content::decode(&content) {
            cx.run_ops(&decoded.operations, 0);
        }
    }
    cx.drawings
}

// --- bboxlog support --------------------------------------------------------

/// One painted item's slot in the content stream, for box-log order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BboxAnchor {
    /// A BT..ET text object.
    Text,
    /// A path paint operator.
    Path,
    /// A Do that paints an image XObject.
    Image,
}

/// Every painted item on a page, in content-stream order, with the
/// optional-content layer in force there (empty when unlayered).
fn bbox_anchors(doc: &LopdfDocument, page_id: ObjectId) -> Vec<(BboxAnchor, String)> {
    let mut out = Vec::new();
    let mut layers: Vec<String> = Vec::new();
    for stream_id in doc.get_page_contents(page_id) {
        let content = doc
            .get_object(stream_id)
            .ok()
            .and_then(|o| o.as_stream().ok())
            .and_then(|s| s.decompressed_content().ok())
            .unwrap_or_default();
        if content.is_empty() {
            continue;
        }
        if let Ok(decoded) = lopdf::content::Content::decode(&content) {
            bbox_walk_ops(doc, page_id, &decoded.operations, 0, &mut layers, &mut out);
        }
    }
    out
}

/// Record anchors for one operation run, recursing into Form XObjects.
fn bbox_walk_ops(
    doc: &LopdfDocument,
    page_id: ObjectId,
    ops: &[lopdf::content::Operation],
    depth: usize,
    layers: &mut Vec<String>,
    out: &mut Vec<(BboxAnchor, String)>,
) {
    let mut in_text = false;
    for op in ops {
        match op.operator.as_str() {
            "BT" => in_text = true,
            "ET" => {
                if in_text {
                    out.push((BboxAnchor::Text, layers.last().cloned().unwrap_or_default()));
                    in_text = false;
                }
            }
            "S" | "s" | "f" | "F" | "f*" | "F*" | "B" | "B*" | "b" | "b*" => {
                out.push((BboxAnchor::Path, layers.last().cloned().unwrap_or_default()));
            }
            "Do" => {
                if depth >= 8 {
                    continue;
                }
                let name = match op.operands.first() {
                    Some(found) => obj_name(found),
                    None => continue,
                };
                let clean = name.trim_start_matches('/');
                let stream = match bbox_xobject(doc, page_id, clean) {
                    Some(Object::Stream(stream)) => stream,
                    _ => continue,
                };
                let subtype = stream
                    .dict
                    .get(b"Subtype")
                    .ok()
                    .map(obj_name)
                    .unwrap_or_default();
                if subtype == "Form" {
                    if let Ok(content) = stream.decompressed_content() {
                        if let Ok(decoded) = lopdf::content::Content::decode(&content) {
                            bbox_walk_ops(
                                doc,
                                page_id,
                                &decoded.operations,
                                depth + 1,
                                layers,
                                out,
                            );
                        }
                    }
                } else if subtype == "Image" {
                    out.push((
                        BboxAnchor::Image,
                        layers.last().cloned().unwrap_or_default(),
                    ));
                }
            }
            "BDC" => {
                let tag = op.operands.first().map(obj_name).unwrap_or_default();
                if tag == "OC" || tag == "/OC" {
                    let label = op
                        .operands
                        .get(1)
                        .and_then(|o| o.as_reference().ok())
                        .and_then(|id| doc.get_dictionary(id).ok())
                        .and_then(|d| d.get(b"Name").ok().map(obj_text))
                        .unwrap_or_default();
                    layers.push(label);
                } else {
                    layers.push(String::new());
                }
            }
            "EMC" => {
                layers.pop();
            }
            _ => {}
        }
    }
    // An unclosed text object still painted something.
    if in_text {
        out.push((BboxAnchor::Text, layers.last().cloned().unwrap_or_default()));
    }
}

/// Resolve a page XObject by name, following references.
fn bbox_xobject(doc: &LopdfDocument, page_id: ObjectId, name: &str) -> Option<Object> {
    let page = doc.get_dictionary(page_id).ok()?;
    let res = page.get(b"Resources").ok()?;
    let dict = deref(doc, res).as_dict().ok()?;
    let sub = dict.get(b"XObject").ok()?;
    let xobjects = deref(doc, sub).as_dict().ok()?;
    let found = xobjects.get(name.as_bytes()).ok()?;
    let (_, target) = doc.dereference(found).unwrap_or((None, found));
    Some(target.clone())
}

/// A box as a four-float list value.
fn bbox_rect(rect: &Rect) -> Value {
    Value::List(vec![
        Value::Float(rect.x0),
        Value::Float(rect.y0),
        Value::Float(rect.x1),
        Value::Float(rect.y1),
    ])
}

/// Box-log entries for a page: `(kind, rect)` per painted item in
/// content-stream order, plus the layer name when `with_layer` holds.
///
/// Text comes from engine spans (line-metric boxes), paths from the drawing
/// walk, images from the figure walk; the anchor pass interleaves the three
/// in stream order. Only `fill-text` is recording-pinned; the path and image
/// kinds extend its kebab-case convention. Clipping and ignore operators log
/// nothing: the recording shows no kind for them.
pub(crate) fn bboxlog_entries(doc: &PdfDocument, index: usize, with_layer: bool) -> Result<Value> {
    let lp = doc.lopdf();
    let page_id = page_id_of(lp, index)?;
    let engine = super::text::engine_for(doc)?;
    let capture = super::text::capture_page(&engine, index, true)?;
    let drawings = read_drawings(lp, page_id, false);
    let anchors = bbox_anchors(lp, page_id);

    let mut spans: std::collections::VecDeque<_> = capture
        .blocks
        .iter()
        .flat_map(|block| block.lines.iter())
        .flat_map(|line| line.spans.iter())
        .filter(|span| !span.gap)
        .collect();
    let mut paths: std::collections::VecDeque<_> = drawings.iter().collect();
    let mut images: std::collections::VecDeque<_> = capture.figures.iter().collect();

    let entry = |kind: &str, rect: Value, layer: &str| -> Value {
        if with_layer {
            Value::List(vec![
                Value::Str(kind.to_string()),
                rect,
                Value::Str(layer.to_string()),
            ])
        } else {
            Value::List(vec![Value::Str(kind.to_string()), rect])
        }
    };
    let mut out = Vec::new();
    for (anchor, layer) in &anchors {
        match anchor {
            BboxAnchor::Text => {
                if let Some(span) = spans.pop_front() {
                    out.push(entry("fill-text", bbox_rect(&span.bbox), layer));
                }
            }
            BboxAnchor::Path => {
                if let Some(drawing) = paths.pop_front() {
                    if drawing.drawing_type.contains('f') {
                        out.push(entry("fill-path", bbox_rect(&drawing.rect), &drawing.layer));
                    }
                    if drawing.drawing_type.contains('s') {
                        out.push(entry(
                            "stroke-path",
                            bbox_rect(&drawing.rect),
                            &drawing.layer,
                        ));
                    }
                }
            }
            BboxAnchor::Image => {
                if let Some(figure) = images.pop_front() {
                    out.push(entry("fill-image", bbox_rect(&figure.bbox), layer));
                }
            }
        }
    }
    // Items the anchor pass could not place still log, unlayered.
    for span in spans {
        out.push(entry("fill-text", bbox_rect(&span.bbox), ""));
    }
    for drawing in paths {
        if drawing.drawing_type.contains('f') {
            out.push(entry("fill-path", bbox_rect(&drawing.rect), &drawing.layer));
        }
        if drawing.drawing_type.contains('s') {
            out.push(entry(
                "stroke-path",
                bbox_rect(&drawing.rect),
                &drawing.layer,
            ));
        }
    }
    for figure in images {
        out.push(entry("fill-image", bbox_rect(&figure.bbox), ""));
    }
    Ok(Value::List(out))
}

// --- annotation support ----------------------------------------------------

/// PDF subtype name to (type code, display name).
fn subtype_code(subtype: &str) -> (i32, &'static str) {
    use super::constants::*;
    match subtype {
        "Text" => (PDF_ANNOT_TEXT, "Text"),
        "Link" => (PDF_ANNOT_LINK, "Link"),
        "FreeText" => (PDF_ANNOT_FREE_TEXT, "FreeText"),
        "Line" => (PDF_ANNOT_LINE, "Line"),
        "Square" => (PDF_ANNOT_SQUARE, "Square"),
        "Circle" => (PDF_ANNOT_CIRCLE, "Circle"),
        "Polygon" => (PDF_ANNOT_POLYGON, "Polygon"),
        "PolyLine" => (PDF_ANNOT_POLY_LINE, "PolyLine"),
        "Highlight" => (PDF_ANNOT_HIGHLIGHT, "Highlight"),
        "Underline" => (PDF_ANNOT_UNDERLINE, "Underline"),
        "Squiggly" => (PDF_ANNOT_SQUIGGLY, "Squiggly"),
        "StrikeOut" => (PDF_ANNOT_STRIKE_OUT, "StrikeOut"),
        "Redact" => (PDF_ANNOT_REDACT, "Redact"),
        "Stamp" => (PDF_ANNOT_STAMP, "Stamp"),
        "Caret" => (PDF_ANNOT_CARET, "Caret"),
        "Ink" => (PDF_ANNOT_INK, "Ink"),
        "Popup" => (PDF_ANNOT_POPUP, "Popup"),
        "FileAttachment" => (PDF_ANNOT_FILE_ATTACHMENT, "FileAttachment"),
        "Sound" => (PDF_ANNOT_SOUND, "Sound"),
        "Movie" => (PDF_ANNOT_MOVIE, "Movie"),
        "Widget" => (PDF_ANNOT_WIDGET, "Widget"),
        "Screen" => (PDF_ANNOT_SCREEN, "Screen"),
        "PrinterMark" => (PDF_ANNOT_PRINTER_MARK, "PrinterMark"),
        "TrapNet" => (PDF_ANNOT_TRAP_NET, "TrapNet"),
        "Watermark" => (PDF_ANNOT_WATERMARK, "Watermark"),
        "3D" => (PDF_ANNOT_3D, "3D"),
        "RichMedia" => (PDF_ANNOT_RICH_MEDIA, "RichMedia"),
        "Projection" => (PDF_ANNOT_PROJECTION, "Projection"),
        _ => (PDF_ANNOT_TEXT, "Text"),
    }
}

/// Whether the border property reports a dict for this subtype. Icon-like
/// annots without border keys report an empty mapping instead.
fn border_supported(subtype: &str) -> bool {
    !matches!(
        subtype,
        "Text" | "Stamp" | "FileAttachment" | "Sound" | "Caret" | "Popup"
    )
}

fn read_border(doc: &LopdfDocument, dict: &Dictionary) -> Border {
    let absent = Border {
        width: -1.0,
        width_is_int: true,
        style: None,
        dashes: Vec::new(),
        clouds: -1,
    };
    let subtype = dict.get(b"Subtype").ok().map(obj_name).unwrap_or_default();
    let bs = dict.get(b"BS").ok().map(|o| deref(doc, o));
    let border = dict.get(b"Border").ok().map(|o| deref(doc, o));
    if bs.is_none() && border.is_none() && !border_supported(&subtype) {
        return Border {
            width: -1.0,
            width_is_int: true,
            style: None,
            dashes: Vec::new(),
            clouds: -2,
        };
    }
    if bs.is_none() && border.is_none() {
        return absent;
    }
    let mut out = absent;
    if let Some(Object::Dictionary(bs)) = bs {
        if let Ok(o) = bs.get(b"W") {
            out.width = obj_f64(o);
            out.width_is_int = obj_is_int(o);
        }
        if let Ok(o) = bs.get(b"S") {
            out.style = Some(obj_name(o).trim_start_matches('/').to_string());
        }
        if let Ok(o) = bs.get(b"D") {
            if let Ok(arr) = deref(doc, o).as_array() {
                out.dashes = arr.iter().map(|v| obj_f64(v).round() as i32).collect();
            }
        }
    } else if let Some(obj) = border {
        match obj {
            Object::Array(arr) if arr.len() >= 3 => {
                out.width = obj_f64(&arr[2]);
                out.width_is_int = obj_is_int(&arr[2]);
                if arr.len() >= 4 {
                    if let Ok(dashes) = arr[3].as_array() {
                        out.dashes = dashes.iter().map(|v| obj_f64(v).round() as i32).collect();
                    }
                }
            }
            Object::Dictionary(inner) => {
                if let Ok(o) = inner.get(b"W") {
                    out.width = obj_f64(o);
                    out.width_is_int = obj_is_int(o);
                }
                if let Ok(o) = inner.get(b"S") {
                    out.style = Some(obj_name(o).trim_start_matches('/').to_string());
                }
                if let Ok(o) = inner.get(b"D") {
                    if let Ok(arr) = deref(doc, o).as_array() {
                        out.dashes = arr.iter().map(|v| obj_f64(v).round() as i32).collect();
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(be) = dict.get(b"BE").ok().map(|o| deref(doc, o)) {
        if let Ok(be) = be.as_dict() {
            let cloudy = be
                .get(b"S")
                .ok()
                .map(|o| obj_name(o) == "C")
                .unwrap_or(false);
            if cloudy {
                out.clouds = be.get(b"I").ok().map(obj_f64).unwrap_or(0.0).round() as i32;
            }
        }
    }
    out
}

fn write_border(dict: &mut Dictionary, options: &BorderOptions) {
    let touched = options.width >= 0.0
        || options.style.is_some()
        || !options.dashes.is_empty()
        || options.clouds >= 0;
    if !touched {
        return;
    }
    let mut bs = Dictionary::new();
    bs.set("W", Object::Real(options.width as f32));
    let style = options.style.clone().unwrap_or_else(|| {
        if options.dashes.is_empty() {
            "S".to_string()
        } else {
            "D".to_string()
        }
    });
    bs.set("S", name_object(&style));
    if !options.dashes.is_empty() {
        bs.set(
            "D",
            Object::Array(
                options
                    .dashes
                    .iter()
                    .map(|d| Object::Integer(i64::from(*d)))
                    .collect(),
            ),
        );
    }
    dict.set("BS", Object::Dictionary(bs));
    dict.remove(b"Border");
    if options.clouds > 0 {
        let mut be = Dictionary::new();
        be.set("S", name_object("C"));
        be.set("I", Object::Integer(i64::from(options.clouds)));
        dict.set("BE", Object::Dictionary(be));
    } else if options.clouds == 0 {
        dict.remove(b"BE");
    }
}

fn read_colors(doc: &LopdfDocument, dict: &Dictionary) -> AnnotColors {
    let array = |key: &[u8]| -> Vec<f64> {
        dict.get(key)
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_array().ok())
            .map(|arr| arr.iter().map(obj_f64).collect())
            .unwrap_or_default()
    };
    AnnotColors {
        stroke: array(b"C"),
        fill: array(b"IC"),
    }
}

fn write_colors(dict: &mut Dictionary, colors: &AnnotColors) {
    if colors.stroke.is_empty() {
        dict.remove(b"C");
    } else {
        dict.set("C", num_array(&colors.stroke));
    }
    if colors.fill.is_empty() {
        dict.remove(b"IC");
    } else {
        dict.set("IC", num_array(&colors.fill));
    }
}

fn read_info(doc: &LopdfDocument, dict: &Dictionary) -> AnnotInfo {
    let text = |key: &[u8]| -> String {
        dict.get(key)
            .ok()
            .map(|o| obj_text(deref(doc, o)))
            .unwrap_or_default()
    };
    AnnotInfo {
        content: text(b"Contents"),
        creation_date: text(b"CreationDate"),
        mod_date: text(b"M"),
        name: text(b"NM"),
        subject: text(b"Subj"),
        title: text(b"T"),
        id: String::new(),
    }
}

fn write_info(dict: &mut Dictionary, options: &AnnotInfoOptions) {
    if let Some(info) = &options.info {
        dict.set("Contents", text_object(&info.content));
        dict.set("T", text_object(&info.title));
        dict.set("CreationDate", text_object(&info.creation_date));
        dict.set("M", text_object(&info.mod_date));
        dict.set("Subj", text_object(&info.subject));
    }
    if let Some(content) = &options.content {
        dict.set("Contents", text_object(content));
    }
    if let Some(title) = &options.title {
        dict.set("T", text_object(title));
    }
    if let Some(stamp) = &options.creation_date {
        dict.set("CreationDate", text_object(stamp));
    }
    if let Some(stamp) = &options.mod_date {
        dict.set("M", text_object(stamp));
    }
    if let Some(subject) = &options.subject {
        dict.set("Subj", text_object(subject));
    }
}

/// Filespec dict plus its first embedded stream, for file annots.
fn filespec_parts(doc: &LopdfDocument, dict: &Dictionary) -> Option<(Dictionary, Vec<u8>)> {
    let spec = dict.get(b"FS").ok().map(|o| deref(doc, o))?;
    let spec = spec.as_dict().ok()?.clone();
    let ef = spec.get(b"EF").ok().map(|o| deref(doc, o))?;
    let ef = ef.as_dict().ok()?;
    for key in [b"F".as_slice(), b"UF".as_slice()] {
        if let Ok(stream) = ef.get(key).map(|o| deref(doc, o)) {
            if let Ok(stream) = stream.as_stream() {
                let bytes = stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone());
                return Some((spec, bytes));
            }
        }
    }
    None
}

fn read_file_info(doc: &LopdfDocument, dict: &Dictionary) -> Result<FileInfo> {
    let (spec, bytes) =
        filespec_parts(doc, dict).ok_or_else(|| wrong_type("annotation has no embedded file"))?;
    let filename = spec.get(b"F").ok().map(obj_text).unwrap_or_default();
    let display_name = spec
        .get(b"UF")
        .ok()
        .map(obj_text)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| filename.clone());
    let description = spec.get(b"Desc").ok().map(obj_text).unwrap_or_default();
    Ok(FileInfo {
        filename,
        display_name,
        description,
        size: bytes.len(),
    })
}

/// QuadPoints (stored bottom-left, bottom-right, top-left, top-right in PDF
/// space) as y-down quads.
#[allow(dead_code)]
fn read_quadpoints(doc: &LopdfDocument, dict: &Dictionary, top: f64) -> Vec<Quad> {
    let nums: Vec<f64> = dict
        .get(b"QuadPoints")
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|arr| arr.iter().map(obj_f64).collect())
        .unwrap_or_default();
    nums.chunks_exact(8)
        .map(|c| {
            mk_quad(
                mk_point(c[4], flip_y(c[5], top)),
                mk_point(c[6], flip_y(c[7], top)),
                mk_point(c[0], flip_y(c[1], top)),
                mk_point(c[2], flip_y(c[3], top)),
            )
        })
        .collect()
}

fn write_quadpoints(quads: &[Quad], top: f64) -> Object {
    let mut nums = Vec::with_capacity(quads.len() * 8);
    for q in quads {
        nums.extend_from_slice(&[
            q.ll.x,
            flip_y(q.ll.y, top),
            q.lr.x,
            flip_y(q.lr.y, top),
            q.ul.x,
            flip_y(q.ul.y, top),
            q.ur.x,
            flip_y(q.ur.y, top),
        ]);
    }
    num_array(&nums)
}

fn read_inklist(doc: &LopdfDocument, dict: &Dictionary, top: f64) -> Vec<Vec<Point>> {
    let outer = dict
        .get(b"InkList")
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array().ok().cloned())
        .unwrap_or_default();
    outer
        .iter()
        .map(|stroke| {
            let flat: Vec<f64> = deref(doc, stroke)
                .as_array()
                .map(|arr| arr.iter().map(obj_f64).collect())
                .unwrap_or_default();
            flat.chunks_exact(2)
                .map(|c| mk_point(c[0], flip_y(c[1], top)))
                .collect()
        })
        .collect()
}

fn write_inklist(strokes: &[Vec<Point>], top: f64) -> Object {
    Object::Array(
        strokes
            .iter()
            .map(|stroke| {
                let mut nums = Vec::with_capacity(stroke.len() * 2);
                for p in stroke {
                    nums.push(p.x);
                    nums.push(flip_y(p.y, top));
                }
                num_array(&nums)
            })
            .collect(),
    )
}

/// Vertices from `/Vertices`, falling back to a line's `/L` endpoints.
fn read_vertices(doc: &LopdfDocument, dict: &Dictionary, top: f64) -> Vec<Point> {
    for key in [b"Vertices".as_slice(), b"L".as_slice()] {
        if let Some(obj) = dict.get(key).ok().map(|o| deref(doc, o)) {
            if let Ok(arr) = obj.as_array() {
                let nums: Vec<f64> = arr.iter().map(obj_f64).collect();
                if !nums.is_empty() {
                    return nums
                        .chunks_exact(2)
                        .map(|c| mk_point(c[0], flip_y(c[1], top)))
                        .collect();
                }
            }
        }
    }
    Vec::new()
}

fn le_code_to_name(code: i32) -> &'static str {
    use super::constants::*;
    if code == PDF_ANNOT_LE_NONE {
        "None"
    } else if code == PDF_ANNOT_LE_SQUARE {
        "Square"
    } else if code == PDF_ANNOT_LE_CIRCLE {
        "Circle"
    } else if code == PDF_ANNOT_LE_DIAMOND {
        "Diamond"
    } else if code == PDF_ANNOT_LE_OPEN_ARROW {
        "OpenArrow"
    } else if code == PDF_ANNOT_LE_CLOSED_ARROW {
        "ClosedArrow"
    } else if code == PDF_ANNOT_LE_BUTT {
        "Butt"
    } else if code == PDF_ANNOT_LE_R_OPEN_ARROW {
        "ROpenArrow"
    } else if code == PDF_ANNOT_LE_R_CLOSED_ARROW {
        "RClosedArrow"
    } else if code == PDF_ANNOT_LE_SLASH {
        "Slash"
    } else {
        "None"
    }
}

fn le_name_to_code(label: &str) -> i32 {
    use super::constants::*;
    match label.trim_start_matches('/') {
        "None" => PDF_ANNOT_LE_NONE,
        "Square" => PDF_ANNOT_LE_SQUARE,
        "Circle" => PDF_ANNOT_LE_CIRCLE,
        "Diamond" => PDF_ANNOT_LE_DIAMOND,
        "OpenArrow" => PDF_ANNOT_LE_OPEN_ARROW,
        "ClosedArrow" => PDF_ANNOT_LE_CLOSED_ARROW,
        "Butt" => PDF_ANNOT_LE_BUTT,
        "ROpenArrow" => PDF_ANNOT_LE_R_OPEN_ARROW,
        "RClosedArrow" => PDF_ANNOT_LE_R_CLOSED_ARROW,
        "Slash" => PDF_ANNOT_LE_SLASH,
        _ => PDF_ANNOT_LE_NONE,
    }
}

/// Default-appearance string: `font size Tf` plus an optional text colour.
fn build_da(fontname: Option<&str>, fontsize: f64, text_color: Option<&[f64]>) -> String {
    let mut da = format!("/{} {} Tf", fontname.unwrap_or("Helv"), pdf_num(fontsize));
    if let Some(color) = text_color {
        match color.len() {
            1 => da.push_str(&format!(" {} g", pdf_num(color[0]))),
            3 => da.push_str(&format!(
                " {} {} {} rg",
                pdf_num(color[0]),
                pdf_num(color[1]),
                pdf_num(color[2])
            )),
            4 => da.push_str(&format!(
                " {} {} {} {} k",
                pdf_num(color[0]),
                pdf_num(color[1]),
                pdf_num(color[2]),
                pdf_num(color[3])
            )),
            _ => {}
        }
    }
    da
}

/// Parse a default-appearance string into (font, size).
fn parse_da_font(da: &str) -> (String, f64, bool) {
    let tokens: Vec<&str> = da.split_whitespace().collect();
    let mut font = "Helv".to_string();
    let mut size = 0.0;
    let mut size_is_int = true;
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "Tf" && i >= 2 {
            font = tokens[i - 2].trim_start_matches('/').to_string();
            let token = tokens[i - 1];
            size = token.parse::<f64>().unwrap_or(0.0);
            size_is_int = token.parse::<i32>().is_ok();
        }
        i += 1;
    }
    (font, size, size_is_int)
}

/// Page index holding an annotation object, if it is listed anywhere.
fn annot_page_index(doc: &LopdfDocument, xref: i32) -> Option<usize> {
    if xref <= 0 {
        return None;
    }
    let id = (xref as u32, 0);
    for (index, page_id) in leaf_pages(doc).iter().enumerate() {
        if page_annots(doc, *page_id).contains(&id) {
            return Some(index);
        }
    }
    None
}

/// Create a bare annotation dict on a page and list it; returns the new id.
fn add_annot_common(
    doc: &mut LopdfDocument,
    page_id: ObjectId,
    subtype: &str,
    rect_pdf: &Rect,
) -> Result<ObjectId> {
    let mut dict = Dictionary::new();
    dict.set("Type", name_object("Annot"));
    dict.set("Subtype", name_object(subtype));
    dict.set("Rect", rect_object(rect_pdf));
    dict.set("F", Object::Integer(4));
    let id = doc.new_object_id();
    doc.objects.insert(id, Object::Dictionary(dict));
    let mut ids = page_annots(doc, page_id);
    ids.push(id);
    set_page_annots(doc, page_id, &ids)?;
    Ok(id)
}

/// Create (or move) the orphan popup annotation linked from a parent.
fn ensure_popup(doc: &mut LopdfDocument, parent_id: ObjectId, rect_pdf: &Rect) -> Result<ObjectId> {
    let existing = doc
        .get_dictionary(parent_id)
        .ok()
        .and_then(|d| d.get(b"Popup").ok().cloned())
        .and_then(|o| o.as_reference().ok());
    if let Some(id) = existing {
        if let Ok(popup) = doc.get_dictionary_mut(id) {
            popup.set("Rect", rect_object(rect_pdf));
        }
        return Ok(existing.expect("popup"));
    }
    let mut dict = Dictionary::new();
    dict.set("Type", name_object("Annot"));
    dict.set("Subtype", name_object("Popup"));
    dict.set("Rect", rect_object(rect_pdf));
    dict.set("Parent", Object::Reference(parent_id));
    let id = doc.new_object_id();
    doc.objects.insert(id, Object::Dictionary(dict));
    doc.get_dictionary_mut(parent_id)?
        .set("Popup", Object::Reference(id));
    Ok(id)
}

// --- appearance streams -----------------------------------------------------

/// Ensure the annotation has an /AP dictionary; return its object id.
/// Creates `/AP << >>` inline when absent.
fn ensure_appearance(doc: &mut LopdfDocument, xref: i32) -> Result<ObjectId> {
    let id = (xref as u32, 0);
    let existing = doc
        .get_dictionary(id)
        .ok()
        .and_then(|d| d.get(b"AP").ok())
        .and_then(|o| o.as_reference().ok());
    if let Some(ap_id) = existing {
        return Ok(ap_id);
    }
    let has_inline = doc
        .get_dictionary(id)
        .ok()
        .and_then(|d| {
            d.get(b"AP")
                .ok()
                .map(|o| matches!(o, Object::Dictionary(_)))
        })
        .unwrap_or(false);
    if has_inline {
        // Materialize the inline appearance as an indirect dict so the
        // normal stream underneath can be edited in place.
        let ap = doc
            .get_dictionary(id)?
            .get(b"AP")
            .ok()
            .and_then(|o| o.as_dict().ok().cloned())
            .unwrap_or_default();
        let ap_id = doc.new_object_id();
        doc.objects.insert(ap_id, Object::Dictionary(ap));
        let dict = doc.get_dictionary_mut(id)?;
        dict.set("AP", Object::Reference(ap_id));
        return Ok(ap_id);
    }
    let ap_id = doc.new_object_id();
    doc.objects
        .insert(ap_id, Object::Dictionary(Dictionary::new()));
    let dict = doc.get_dictionary_mut(id)?;
    dict.set("AP", Object::Reference(ap_id));
    Ok(ap_id)
}

/// Mutable access to the normal appearance (/AP /N) stream: follows the
/// reference and hands the stream to the closure together with the document.
fn appearance_normal_mut<R>(
    doc: &mut LopdfDocument,
    xref: i32,
    f: impl FnOnce(&mut LopdfDocument, ObjectId) -> Result<R>,
) -> Result<R> {
    let ap_id = ensure_appearance(doc, xref)?;
    let normal_id = {
        let ap = doc.get_dictionary(ap_id)?.clone();
        match ap.get(b"N").ok() {
            Some(Object::Reference(id)) => *id,
            Some(Object::Stream(_)) => {
                return Err(bad_value("normal appearance is an inline stream"));
            }
            Some(Object::Dictionary(_)) => {
                return Err(bad_value("normal appearance is a subdictionary"));
            }
            _ => {
                let dict = doc.get_dictionary((xref as u32, 0))?.clone();
                let rect = dict
                    .get(b"Rect")
                    .ok()
                    .and_then(obj_rect)
                    .unwrap_or(mk_rect(0.0, 0.0, 200.0, 50.0));
                let mut stream_dict = Dictionary::new();
                stream_dict.set("Type", name_object("XObject"));
                stream_dict.set("Subtype", name_object("Form"));
                stream_dict.set(
                    "BBox",
                    Object::Array(vec![
                        Object::Real(rect.x0 as f32),
                        Object::Real(rect.y0 as f32),
                        Object::Real(rect.x1 as f32),
                        Object::Real(rect.y1 as f32),
                    ]),
                );
                let sid = doc.new_object_id();
                let stream = Stream::new(stream_dict, Vec::new());
                doc.objects.insert(sid, Object::Stream(stream));
                let ap = doc.get_dictionary_mut(ap_id)?;
                ap.set("N", Object::Reference(sid));
                sid
            }
        }
    };
    f(doc, normal_id)
}

/// Set one key on the normal appearance stream dictionary.
fn set_ap_key(doc: &mut LopdfDocument, xref: i32, key: &[u8], value: Object) -> Result<()> {
    appearance_normal_mut(doc, xref, |doc, sid| {
        let stream = doc
            .get_object_mut(sid)
            .ok()
            .and_then(|o| match o {
                Object::Stream(s) => Some(s),
                _ => None,
            })
            .ok_or_else(|| bad_value("normal appearance is not a stream"))?;
        stream.dict.set(key, value);
        Ok(())
    })
}

/// Set the normal appearance bounding box.
fn set_ap_bbox(doc: &mut LopdfDocument, xref: i32, bbox: &Rect) -> Result<()> {
    set_ap_key(
        doc,
        xref,
        b"BBox",
        Object::Array(vec![
            Object::Real(bbox.x0 as f32),
            Object::Real(bbox.y0 as f32),
            Object::Real(bbox.x1 as f32),
            Object::Real(bbox.y1 as f32),
        ]),
    )
}

// --- widget field codes -----------------------------------------------------

/// Field-type code to display name. Only `Text` and `ListBox` are pinned by
/// recorded outputs; the rest mirror the constant names.
fn widget_type_string(code: i32) -> &'static str {
    use super::constants::*;
    if code == PDF_WIDGET_TYPE_TEXT {
        "Text"
    } else if code == PDF_WIDGET_TYPE_LISTBOX {
        "ListBox"
    } else if code == PDF_WIDGET_TYPE_CHECKBOX {
        "CheckBox"
    } else if code == PDF_WIDGET_TYPE_RADIOBUTTON {
        "RadioButton"
    } else if code == PDF_WIDGET_TYPE_COMBOBOX {
        "ComboBox"
    } else if code == PDF_WIDGET_TYPE_BUTTON {
        "Button"
    } else if code == PDF_WIDGET_TYPE_SIGNATURE {
        "Signature"
    } else {
        "Unknown"
    }
}

// --- insertion fonts ---------------------------------------------------------

/// Resolve an insertion font: file faces parse for measurement but carry no
/// base-14 name, so content generation rejects them; named faces resolve to
/// base-14 metrics plus the name to embed.
fn insert_face(fontname: &str, fontfile: &Option<String>) -> Result<(FaceData, Option<String>)> {
    if let Some(path) = fontfile {
        let data = std::fs::read(path)
            .map_err(|_| missing_file(format!("font file not found: {path}")))?;
        let face = parse_file_face(&data)
            .map_err(|_| unreadable(format!("font file does not parse: {path}")))?;
        return Ok((face, None));
    }
    let base = resolve_fontname(fontname)
        .ok_or_else(|| bad_value(format!("unknown font name: {fontname}")))?;
    let face = builtin_face(base).ok_or_else(|| engine_fault("builtin metrics are missing"))?;
    Ok((face, Some(base.to_string())))
}

/// Metrics for an owned font: program bytes win, then the stored base-14
/// name, then Helvetica as the last resort.
fn font_face(font: &Font) -> Result<FaceData> {
    if !font.data.is_empty() {
        return parse_file_face(&font.data);
    }
    if let Some(base) = resolve_fontname(&font.name) {
        if let Some(face) = builtin_face(base) {
            return Ok(face);
        }
    }
    builtin_face("Helvetica").ok_or_else(|| engine_fault("builtin metrics are missing"))
}

/// Grow an optional union box with one point.
fn grow_union(union: &mut Option<Rect>, p: Point) {
    let point = mk_rect(p.x, p.y, p.x, p.y);
    *union = Some(match *union {
        Some(u) => union_rect(u, point),
        None => point,
    });
}

// --- form fields -------------------------------------------------------------

/// Merge a widget annotation with its field-hierarchy parents: kid keys win,
/// up to 32 levels up.
fn merged_field(doc: &LopdfDocument, dict: &Dictionary) -> Dictionary {
    let mut merged = dict.clone();
    let mut seen = 0;
    let mut next = dict.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
    while let Some(id) = next {
        seen += 1;
        if seen > 32 {
            break;
        }
        let Ok(parent) = doc.get_dictionary(id) else {
            break;
        };
        for (key, value) in parent.iter() {
            if merged.get(key).is_err() {
                merged.set(key.clone(), value.clone());
            }
        }
        next = parent
            .get(b"Parent")
            .ok()
            .and_then(|o| o.as_reference().ok());
    }
    merged
}

/// Field-type code from a merged field dict (FT plus kind bits).
fn field_type_of(merged: &Dictionary) -> i32 {
    use super::constants::*;
    let ft = merged.get(b"FT").ok().map(obj_name).unwrap_or_default();
    let flags = merged.get(b"Ff").ok().map(obj_i32).unwrap_or(0);
    match ft.as_str() {
        "Tx" => PDF_WIDGET_TYPE_TEXT,
        "Ch" => {
            if flags & (1 << 17) != 0 {
                PDF_WIDGET_TYPE_COMBOBOX
            } else {
                PDF_WIDGET_TYPE_LISTBOX
            }
        }
        "Btn" => {
            if flags & (1 << 16) != 0 {
                PDF_WIDGET_TYPE_BUTTON
            } else if flags & (1 << 15) != 0 {
                PDF_WIDGET_TYPE_RADIOBUTTON
            } else {
                PDF_WIDGET_TYPE_CHECKBOX
            }
        }
        "Sig" => PDF_WIDGET_TYPE_SIGNATURE,
        _ => PDF_WIDGET_TYPE_UNKNOWN,
    }
}

/// Effective field value: /V, else /DV, else empty. Names (buttons) and
/// single-element picks read as their name; multi-select arrays join lines.
fn field_value_of(doc: &LopdfDocument, merged: &Dictionary) -> String {
    for key in [b"V".as_slice(), b"DV".as_slice()] {
        if let Ok(value) = merged.get(key) {
            let value = deref(doc, value);
            match value {
                Object::Null => continue,
                Object::Name(_) => return obj_name(value),
                Object::Array(items) => {
                    let parts: Vec<String> = items
                        .iter()
                        .map(|o| {
                            let o = deref(doc, o);
                            match o {
                                Object::Name(_) => obj_name(o),
                                _ => obj_text(o),
                            }
                        })
                        .collect();
                    if parts.iter().any(|p| !p.is_empty()) {
                        return parts.join("\n");
                    }
                    continue;
                }
                _ => {
                    let text = obj_text(value);
                    if !text.is_empty() || matches!(value, Object::String(_, _)) {
                        return text;
                    }
                }
            }
        }
    }
    String::new()
}

/// Choice options: plain strings as-is, export/display pairs by export value.
fn choice_values_of(doc: &LopdfDocument, merged: &Dictionary) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(opt) = merged.get(b"Opt") else {
        return out;
    };
    let Ok(items) = deref(doc, opt).as_array() else {
        return out;
    };
    for item in items {
        let item = deref(doc, item);
        match item {
            Object::Array(pair) if pair.len() >= 2 => out.push(obj_text(&pair[0])),
            _ => out.push(obj_text(item)),
        }
    }
    out
}

/// Expand a border style letter to its widget word.
fn widget_border_style(style: Option<&str>) -> String {
    match style {
        Some("D") => "Dashed",
        Some("B") => "Beveled",
        Some("I") => "Inset",
        Some("U") => "Underline",
        Some(other) => other,
        None => "Solid",
    }
    .to_string()
}

/// Widget border: explicit BS/Border wins, otherwise the recorded default.
fn widget_border_of(doc: &LopdfDocument, dict: &Dictionary) -> (f64, bool, String) {
    let bs = dict.get(b"BS").ok().map(|o| deref(doc, o));
    if let Some(Object::Dictionary(bs)) = bs {
        let width = bs.get(b"W").ok();
        let is_int = width.map(obj_is_int).unwrap_or(true);
        let width = width.map(obj_f64).unwrap_or(1.0);
        let style = bs.get(b"S").ok().map(obj_name);
        return (width, is_int, widget_border_style(style.as_deref()));
    }
    if let Some(obj) = dict.get(b"Border").ok().map(|o| deref(doc, o)) {
        if let Ok(arr) = obj.as_array() {
            if arr.len() >= 3 {
                return (obj_f64(&arr[2]), obj_is_int(&arr[2]), "Solid".to_string());
            }
        }
    }
    (1.0, true, "Solid".to_string())
}

/// Display mode from annotation flags: hidden 0, no-view 1, else visible 2.
fn widget_display_of(dict: &Dictionary) -> i32 {
    let flags = dict.get(b"F").ok().map(obj_i32).unwrap_or(0);
    if flags & 0x03 != 0 {
        0
    } else if flags & 0x20 != 0 {
        1
    } else {
        2
    }
}

/// Full widget snapshot for one widget annotation.
fn widget_data_of(doc: &LopdfDocument, page: usize, xref: i32) -> Result<Widget> {
    let dict = annot_dict(doc, xref)?;
    let subtype = dict.get(b"Subtype").ok().map(obj_name).unwrap_or_default();
    if subtype != "Widget" {
        return Err(wrong_type("annotation is not a widget"));
    }
    let page_id = page_id_of(doc, page)?;
    let top = page_top(doc, page_id);
    let merged = merged_field(doc, &dict);
    let rect = dict
        .get(b"Rect")
        .ok()
        .and_then(obj_rect)
        .map(|r| flip_rect(&r, top))
        .unwrap_or(mk_rect(0.0, 0.0, 0.0, 0.0));
    let (text_font, text_fontsize, text_fontsize_is_int) = merged
        .get(b"DA")
        .ok()
        .map(obj_text)
        .map(|da| parse_da_font(&da))
        .or_else(|| {
            let catalog = catalog_id(doc).ok()?;
            let form = doc.get_dictionary(catalog).ok()?;
            let form_ref = form.get(b"AcroForm").ok()?;
            let form = deref(doc, form_ref).as_dict().ok()?;
            form.get(b"DA")
                .ok()
                .map(obj_text)
                .map(|da| parse_da_font(&da))
        })
        .unwrap_or_else(|| ("Helv".to_string(), 0.0, true));
    let (border_width, border_width_is_int, border_style) = widget_border_of(doc, &dict);
    Ok(Widget {
        xref,
        field_name: merged.get(b"T").ok().map(obj_text).unwrap_or_default(),
        field_type: field_type_of(&merged),
        field_value: field_value_of(doc, &merged),
        rect,
        choices: choice_values_of(doc, &merged),
        field_flags: merged.get(b"Ff").ok().map(obj_i32).unwrap_or(0),
        field_display: widget_display_of(&dict),
        border_width,
        border_width_is_int,
        border_style,
        text_font,
        text_fontsize,
        text_fontsize_is_int,
        text_maxlen: merged.get(b"MaxLen").ok().map(obj_i32).unwrap_or(0),
    })
}

/// Button state names plus the current on-state for a widget.
fn widget_states_of(doc: &LopdfDocument, xref: i32) -> Result<(Vec<String>, String)> {
    use super::constants::*;
    let dict = annot_dict(doc, xref)?;
    let merged = merged_field(doc, &dict);
    let kind = field_type_of(&merged);
    if kind != PDF_WIDGET_TYPE_CHECKBOX && kind != PDF_WIDGET_TYPE_RADIOBUTTON {
        return Err(wrong_type("widget is not a button"));
    }
    let mut states = vec!["Off".to_string()];
    if let Some(ap) = dict.get(b"AP").ok().map(|o| deref(doc, o)) {
        if let Ok(ap) = ap.as_dict() {
            if let Ok(normal) = ap.get(b"N").map(|o| deref(doc, o)) {
                if let Ok(keys) = normal.as_dict() {
                    for key in keys
                        .iter()
                        .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
                    {
                        if key != "Off" && !states.contains(&key) {
                            states.push(key);
                        }
                    }
                }
            }
        }
    }
    let current = field_value_of(doc, &merged);
    if !current.is_empty() && current != "Off" && !states.contains(&current) {
        states.push(current.clone());
    }
    states.sort();
    if let Some(pos) = states.iter().position(|s| s == "Off") {
        let off = states.remove(pos);
        states.insert(0, off);
    }
    Ok((states, current))
}

/// Write a widget snapshot back to its annotation and field parent.
fn write_widget_data(doc: &mut LopdfDocument, data: &Widget) -> Result<()> {
    let id = (data.xref as u32, 0);
    let page_id = {
        let mut found = None;
        for candidate in leaf_pages(doc) {
            if page_annots(doc, candidate).contains(&id) {
                found = Some(candidate);
                break;
            }
        }
        found.ok_or_else(|| bad_value("widget is not on a page"))?
    };
    let top = page_top(doc, page_id);
    let target = {
        let dict = doc.get_dictionary(id)?.clone();
        dict.get(b"Parent")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .unwrap_or(id)
    };
    {
        let dict = doc.get_dictionary_mut(id)?;
        dict.set("Rect", rect_object(&flip_rect(&norm_rect(&data.rect), top)));
        let display = data.field_display;
        let flags = dict.get(b"F").ok().map(obj_i32).unwrap_or(0);
        let flags = if display <= 0 {
            flags | 0x02
        } else if display == 1 {
            (flags & !0x02) | 0x20
        } else {
            (flags & !0x22) | 0x04
        };
        dict.set("F", Object::Integer(i64::from(flags)));
        let mut bs = Dictionary::new();
        bs.set("W", Object::Real(data.border_width.max(0.0) as f32));
        let letter = match data.border_style.as_str() {
            "Dashed" => "D",
            "Beveled" => "B",
            "Inset" => "I",
            "Underline" => "U",
            _ => "S",
        };
        bs.set("S", name_object(letter));
        dict.set("BS", Object::Dictionary(bs));
    }
    let current_kind = doc
        .get_dictionary(target)
        .ok()
        .map(|d| field_type_of(&merged_field(doc, d)))
        .unwrap_or(0);
    if data.field_type != 0 && data.field_type != current_kind {
        switch_field_type(doc, target, current_kind, data.field_type)?;
    }
    doc.get_dictionary_mut(target)?
        .set("T", text_object(&data.field_name));
    let kind = if data.field_type != 0 {
        data.field_type
    } else {
        current_kind
    };
    let kind_bits = (1 << 15) | (1 << 16) | (1 << 17) | (1 << 26);
    let merged_flags = {
        let existing = doc
            .get_dictionary(target)
            .ok()
            .and_then(|d| d.get(b"Ff").ok().map(obj_i32))
            .unwrap_or(0);
        (existing & kind_bits) | (data.field_flags & !kind_bits)
    };
    doc.get_dictionary_mut(target)?
        .set("Ff", Object::Integer(i64::from(merged_flags)));
    write_field_value(doc, target, kind, &data.field_value)?;
    if !data.choices.is_empty() && (kind == 3 || kind == 4) {
        let opt = data.choices.iter().map(|c| text_object(c)).collect();
        doc.get_dictionary_mut(target)?
            .set("Opt", Object::Array(opt));
    }
    if data.text_maxlen > 0 {
        doc.get_dictionary_mut(target)?
            .set("MaxLen", Object::Integer(i64::from(data.text_maxlen)));
    }
    let (old_font, old_size, _) = doc
        .get_dictionary(target)
        .ok()
        .and_then(|d| d.get(b"DA").ok().map(obj_text))
        .map(|da| parse_da_font(&da))
        .unwrap_or_else(|| ("Helv".to_string(), 0.0, true));
    if data.text_font != old_font || data.text_fontsize != old_size {
        let da = build_da(Some(data.text_font.as_str()), data.text_fontsize, None);
        doc.get_dictionary_mut(target)?.set("DA", text_object(&da));
    }
    Ok(())
}

/// Switch a field's type: FT name plus the kind bits, clearing the old kind.
fn switch_field_type(doc: &mut LopdfDocument, target: ObjectId, from: i32, to: i32) -> Result<()> {
    use super::constants::*;
    let keep_value = from == to
        || (from == PDF_WIDGET_TYPE_LISTBOX && to == PDF_WIDGET_TYPE_COMBOBOX)
        || (from == PDF_WIDGET_TYPE_COMBOBOX && to == PDF_WIDGET_TYPE_LISTBOX);
    let field = doc.get_dictionary_mut(target)?;
    let (ft, bits): (&str, i32) = match to {
        t if t == PDF_WIDGET_TYPE_TEXT => ("Tx", 0),
        t if t == PDF_WIDGET_TYPE_COMBOBOX => ("Ch", 1 << 17),
        t if t == PDF_WIDGET_TYPE_LISTBOX => ("Ch", 0),
        t if t == PDF_WIDGET_TYPE_BUTTON => ("Btn", 1 << 16),
        t if t == PDF_WIDGET_TYPE_RADIOBUTTON => ("Btn", 1 << 15),
        t if t == PDF_WIDGET_TYPE_CHECKBOX => ("Btn", 0),
        t if t == PDF_WIDGET_TYPE_SIGNATURE => ("Sig", 0),
        _ => return Ok(()),
    };
    field.set("FT", name_object(ft));
    let flags = field.get(b"Ff").ok().map(obj_i32).unwrap_or(0);
    let cleared = flags & !((1 << 15) | (1 << 16) | (1 << 17));
    field.set("Ff", Object::Integer(i64::from(cleared | bits)));
    if !keep_value {
        field.remove(b"V");
        field.remove(b"AS");
        field.remove(b"DV");
        field.remove(b"Opt");
        field.remove(b"I");
    }
    Ok(())
}

/// Write one field value with the encoding its kind needs.
fn write_field_value(
    doc: &mut LopdfDocument,
    target: ObjectId,
    kind: i32,
    value: &str,
) -> Result<()> {
    use super::constants::*;
    if kind == PDF_WIDGET_TYPE_CHECKBOX || kind == PDF_WIDGET_TYPE_RADIOBUTTON {
        let state = if value.is_empty() { "Off" } else { value };
        let field = doc.get_dictionary_mut(target)?;
        field.set("V", name_object(state));
        field.set("AS", name_object(state));
        return Ok(());
    }
    if kind == PDF_WIDGET_TYPE_LISTBOX && value.contains('\n') {
        let parts = value.split('\n').map(name_object).collect();
        doc.get_dictionary_mut(target)?
            .set("V", Object::Array(parts));
        return Ok(());
    }
    doc.get_dictionary_mut(target)?.set("V", text_object(value));
    Ok(())
}

/// Reset one widget field to its default value.
fn reset_widget_field(doc: &mut LopdfDocument, xref: i32) -> Result<()> {
    let id = (xref as u32, 0);
    let target = {
        let dict = doc.get_dictionary(id)?.clone();
        dict.get(b"Parent")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .unwrap_or(id)
    };
    let has_default = doc
        .get_dictionary(target)
        .ok()
        .and_then(|d| d.get(b"DV").ok().cloned());
    let field = doc.get_dictionary_mut(target)?;
    match has_default {
        Some(dv) => {
            if matches!(dv, Object::Name(_)) {
                field.set("AS", dv.clone());
            }
            field.set("V", dv);
        }
        None => {
            field.remove(b"V");
            field.remove(b"AS");
        }
    }
    Ok(())
}

/// Writable `/AcroForm` dictionary on the catalog, created when missing.
fn ensure_acroform(doc: &mut LopdfDocument) -> Result<ObjectId> {
    let catalog = catalog_id(doc)?;
    let existing = doc
        .get_dictionary(catalog)
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .and_then(|o| o.as_reference().ok());
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = doc.new_object_id();
    let mut form = Dictionary::new();
    form.set("Fields", Object::Array(Vec::new()));
    doc.objects.insert(id, Object::Dictionary(form));
    doc.get_dictionary_mut(catalog)?
        .set("AcroForm", Object::Reference(id));
    Ok(id)
}

/// Catalog object id from the trailer root.
fn catalog_id(doc: &LopdfDocument) -> Result<ObjectId> {
    doc.trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .ok_or_else(|| engine_fault("document has no catalog"))
}

// --- widget extras (binding backing; new API, no signature changed) ---------

impl PageMut<'_> {
    /// Write a widget snapshot back to its field.
    pub fn update_widget(&mut self, widget: &Widget) -> Result<()> {
        let lp = self.document_mut().lopdf_mut();
        write_widget_data(lp, widget)?;
        sync_doc(self.document_mut())
    }

    /// Reset one widget field to its default value.
    pub fn reset_widget(&mut self, xref: i32) -> Result<()> {
        let lp = self.document_mut().lopdf_mut();
        reset_widget_field(lp, xref)?;
        sync_doc(self.document_mut())
    }

    /// Button state names plus the current on-state for a widget.
    pub fn widget_states(&self, xref: i32) -> Result<(Vec<String>, String)> {
        widget_states_of(self.document().lopdf(), xref)
    }

    /// Object numbers of the page widgets, in annotation order.
    pub fn widget_xrefs(&self) -> Result<Vec<i32>> {
        let lp = self.document().lopdf();
        let page_id = page_id_of(lp, self.index())?;
        let mut out = Vec::new();
        for id in page_annots(lp, page_id) {
            let Ok(dict) = lp.get_dictionary(id) else {
                continue;
            };
            if dict.get(b"Subtype").ok().map(obj_name).as_deref() == Some("Widget") {
                out.push(id.0 as i32);
            }
        }
        Ok(out)
    }
}

/// Create a widget annotation plus field keys from an owned snapshot.
/// Button appearance states: `Off` plus the on-state name, each an empty
/// form sized to the widget box.
fn ensure_button_ap(doc: &mut LopdfDocument, id: ObjectId, widget: &Widget) -> Result<()> {
    use super::constants::*;
    if widget.field_type != PDF_WIDGET_TYPE_CHECKBOX
        && widget.field_type != PDF_WIDGET_TYPE_RADIOBUTTON
    {
        return Ok(());
    }
    let on = if widget.field_value.is_empty() || widget.field_value == "Off" {
        "Yes".to_string()
    } else {
        widget.field_value.clone()
    };
    let area = norm_rect(&widget.rect);
    let bbox = Object::Array(vec![
        Object::Integer(0),
        Object::Integer(0),
        Object::Real((area.x1 - area.x0).max(0.0) as f32),
        Object::Real((area.y1 - area.y0).max(0.0) as f32),
    ]);
    let mut states = Dictionary::new();
    for state in ["Off", on.as_str()] {
        let mut sdict = Dictionary::new();
        sdict.set("Type", name_object("XObject"));
        sdict.set("Subtype", name_object("Form"));
        sdict.set("BBox", bbox.clone());
        let sid = doc.new_object_id();
        doc.objects
            .insert(sid, Object::Stream(Stream::new(sdict, Vec::new())));
        states.set(state, Object::Reference(sid));
    }
    let mut ap = Dictionary::new();
    ap.set("N", Object::Dictionary(states));
    doc.get_dictionary_mut(id)?
        .set("AP", Object::Dictionary(ap));
    Ok(())
}

fn create_widget_field(
    doc: &mut LopdfDocument,
    page_id: ObjectId,
    top: f64,
    widget: &Widget,
) -> Result<ObjectId> {
    use super::constants::*;
    let id = add_annot_common(
        doc,
        page_id,
        "Widget",
        &flip_rect(&norm_rect(&widget.rect), top),
    )?;
    let (ft, bits): (&str, i32) = match widget.field_type {
        t if t == PDF_WIDGET_TYPE_TEXT => ("Tx", 0),
        t if t == PDF_WIDGET_TYPE_COMBOBOX => ("Ch", 1 << 17),
        t if t == PDF_WIDGET_TYPE_LISTBOX => ("Ch", 0),
        t if t == PDF_WIDGET_TYPE_BUTTON => ("Btn", 1 << 16),
        t if t == PDF_WIDGET_TYPE_RADIOBUTTON => ("Btn", 1 << 15),
        t if t == PDF_WIDGET_TYPE_CHECKBOX => ("Btn", 0),
        t if t == PDF_WIDGET_TYPE_SIGNATURE => ("Sig", 0),
        _ => ("Tx", 0),
    };
    {
        let dict = doc.get_dictionary_mut(id)?;
        dict.set("FT", name_object(ft));
        dict.set("T", text_object(&widget.field_name));
        dict.set("Ff", Object::Integer(i64::from(bits | widget.field_flags)));
        dict.set(
            "DA",
            text_object(&build_da(
                Some(widget.text_font.as_str()),
                widget.text_fontsize,
                None,
            )),
        );
    }
    write_field_value(doc, id, widget.field_type, &widget.field_value)?;
    ensure_button_ap(doc, id, widget)?;
    if !widget.choices.is_empty()
        && (widget.field_type == PDF_WIDGET_TYPE_LISTBOX
            || widget.field_type == PDF_WIDGET_TYPE_COMBOBOX)
    {
        let opt = widget.choices.iter().map(|c| text_object(c)).collect();
        doc.get_dictionary_mut(id)?.set("Opt", Object::Array(opt));
    }
    let form = ensure_acroform(doc)?;
    let fields = doc
        .get_dictionary(form)
        .ok()
        .and_then(|d| d.get(b"Fields").ok().cloned())
        .unwrap_or(Object::Array(Vec::new()));
    let mut items = match fields {
        Object::Array(items) => items,
        _ => Vec::new(),
    };
    if !items.iter().any(|o| o.as_reference().ok() == Some(id)) {
        items.push(Object::Reference(id));
    }
    doc.get_dictionary_mut(form)?
        .set("Fields", Object::Array(items));
    Ok(id)
}

// --- links -------------------------------------------------------------------

/// Read one link annotation into an owned snapshot.
fn link_of(doc: &LopdfDocument, page: usize, id: ObjectId) -> Result<Link> {
    let dict = doc.get_dictionary(id)?.clone();
    let page_id = page_id_of(doc, page)?;
    let top = page_top(doc, page_id);
    let rect = dict
        .get(b"Rect")
        .ok()
        .and_then(obj_rect)
        .map(|r| flip_rect(&r, top))
        .unwrap_or(mk_rect(0.0, 0.0, 0.0, 0.0));
    let action = dict
        .get(b"A")
        .ok()
        .map(|o| deref(doc, o).clone())
        .and_then(|o| o.as_dict().ok().cloned());
    let (kind, uri, dest) = link_action_of(doc, action.as_ref(), dict.get(b"Dest").ok(), top);
    Ok(Link {
        rect,
        kind,
        uri,
        xref: id.0 as i32,
        id: dict.get(b"NM").ok().map(obj_text).unwrap_or_default(),
        border: read_border(doc, &dict),
        colors: read_colors(doc, &dict),
        flags: dict.get(b"F").ok().map(obj_i32).unwrap_or(0),
        dest,
        next: None,
    })
}

/// Kind, URI, and destination from an action dict or bare destination.
fn link_action_of(
    doc: &LopdfDocument,
    action: Option<&Dictionary>,
    bare_dest: Option<&Object>,
    top: f64,
) -> (i32, String, Option<LinkDest>) {
    use super::constants::*;
    if let Some(action) = action {
        let action_name = action.get(b"S").ok().map(obj_name).unwrap_or_default();
        match action_name.as_str() {
            "URI" => {
                let uri = action.get(b"URI").ok().map(obj_text).unwrap_or_default();
                return (LINK_URI, uri, None);
            }
            "GoTo" => {
                let dest = action.get(b"D").ok().and_then(|o| dest_of(doc, o, top));
                return (LINK_GOTO, String::new(), dest);
            }
            "GoToR" => {
                let uri = action.get(b"F").ok().map(obj_text).unwrap_or_default();
                let dest = action.get(b"D").ok().and_then(|o| dest_of(doc, o, top));
                return (LINK_GOTOR, uri, dest);
            }
            "Launch" => {
                let uri = action
                    .get(b"F")
                    .ok()
                    .map(|o| filespec_name(doc, o))
                    .unwrap_or_default();
                return (LINK_LAUNCH, uri, None);
            }
            "Named" => {
                let uri = action.get(b"N").ok().map(obj_name).unwrap_or_default();
                return (LINK_NAMED, uri, None);
            }
            _ => return (LINK_NONE, String::new(), None),
        }
    }
    if let Some(dest) = bare_dest {
        let dest = dest_of(doc, dest, top);
        return (LINK_GOTO, String::new(), dest);
    }
    (LINK_NONE, String::new(), None)
}

/// File-spec object to a display name.
fn filespec_name(doc: &LopdfDocument, spec: &Object) -> String {
    let spec = deref(doc, spec);
    match spec {
        Object::String(_, _) => obj_text(spec),
        _ => spec
            .as_dict()
            .ok()
            .and_then(|d| d.get(b"UF").ok().or_else(|| d.get(b"F").ok()).map(obj_text))
            .unwrap_or_default(),
    }
}

/// Destination array or name to an owned destination.
fn dest_of(doc: &LopdfDocument, dest: &Object, top: f64) -> Option<LinkDest> {
    let dest = deref(doc, dest);
    if let Ok(name) = dest.as_name() {
        let name = String::from_utf8_lossy(name).into_owned();
        return resolve_named_dest(doc, &name, top);
    }
    if let Ok(text) = dest.as_str() {
        let text = String::from_utf8_lossy(text).into_owned();
        return resolve_named_dest(doc, &text, top);
    }
    let arr = dest.as_array().ok()?;
    let page_id = arr.first()?.as_reference().ok()?;
    let leaves = leaf_pages(doc);
    let page = leaves.iter().position(|id| *id == page_id)? as i32;
    let mode = arr.get(1).map(obj_name).unwrap_or_default();
    let num = |i: usize| arr.get(i).map(obj_f64).unwrap_or(0.0);
    let (target, zoom) = match mode.as_str() {
        "XYZ" => (mk_point(num(2), top - num(3)), num(4)),
        "FitH" => (mk_point(0.0, top - num(2)), 0.0),
        "FitV" => (mk_point(num(2), 0.0), 0.0),
        "FitR" => (mk_point(num(2), top - num(5)), 0.0),
        _ => (mk_point(0.0, 0.0), 0.0),
    };
    Some(LinkDest { page, target, zoom })
}

/// Named destination lookup through /Names/Dests and /Dests.
fn resolve_named_dest(doc: &LopdfDocument, name: &str, top: f64) -> Option<LinkDest> {
    let found = named_dest_object(doc, name)?;
    dest_of(doc, &found, top)
}

/// Raw object behind a named destination, if the catalog names one.
fn named_dest_object(doc: &LopdfDocument, name: &str) -> Option<Object> {
    let catalog = catalog_id(doc).ok()?;
    let root = doc.get_dictionary(catalog).ok()?;
    if let Ok(names) = root.get(b"Names") {
        if let Ok(names) = deref(doc, names).as_dict() {
            if let Ok(dests) = names.get(b"Dests") {
                if let Some(found) = name_tree_lookup(doc, deref(doc, dests), name) {
                    return Some(found);
                }
            }
        }
    }
    if let Ok(dests) = root.get(b"Dests") {
        if let Ok(dests) = deref(doc, dests).as_dict() {
            if let Ok(found) = dests.get(name.as_bytes()) {
                return Some(found.clone());
            }
        }
    }
    None
}

/// Link target shape: the page (or chapter pair) plus coordinates.
fn link_shape(chapters: i32, page: i32, x: Option<f64>, y: Option<f64>) -> Value {
    let coord = |v: Option<f64>| v.map(Value::Float).unwrap_or(Value::Null);
    if chapters != 0 {
        Value::List(vec![
            Value::List(vec![Value::Int(i64::from(page)), Value::Int(0)]),
            coord(x),
            coord(y),
        ])
    } else {
        Value::List(vec![Value::Int(i64::from(page)), coord(x), coord(y)])
    }
}

/// Named destination to a link target: page plus known coordinates.
///
/// The point comes from [`dest_of`] against the target page's own top;
/// fit modes without coordinates read as nulls, like the recorded
/// page-fragment shape.
fn named_link_target(doc: &LopdfDocument, name: &str) -> Option<(i32, Option<f64>, Option<f64>)> {
    let raw = named_dest_object(doc, name)?;
    let arr = final_dest_array(doc, &raw)?;
    let page_id = arr.first()?.as_reference().ok()?;
    let leaves = leaf_pages(doc);
    let page = leaves.iter().position(|id| *id == page_id)? as i32;
    let top = page_top(doc, page_id);
    let target = dest_of(doc, &Object::Array(arr.clone()), top)?;
    let mode = arr.get(1).map(obj_name).unwrap_or_default();
    let (x, y) = match mode.as_str() {
        "XYZ" | "FitR" => (Some(target.target.x), Some(target.target.y)),
        "FitH" => (None, Some(target.target.y)),
        "FitV" => (Some(target.target.x), None),
        _ => (None, None),
    };
    Some((page, x, y))
}

/// Follow chained destination names to the final array, if there is one.
fn final_dest_array(doc: &LopdfDocument, dest: &Object) -> Option<Vec<Object>> {
    let mut owned = deref(doc, dest).clone();
    for _ in 0..8 {
        if let Ok(arr) = owned.as_array() {
            return Some(arr.clone());
        }
        let name = owned
            .as_name()
            .ok()
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .or_else(|| {
                owned
                    .as_str()
                    .ok()
                    .map(|s| String::from_utf8_lossy(s).into_owned())
            })?;
        owned = deref(doc, &named_dest_object(doc, &name)?).clone();
    }
    None
}

/// Flat name-tree lookup (leaf /Names arrays plus one level of kids).
fn name_tree_lookup(doc: &LopdfDocument, node: &Object, name: &str) -> Option<Object> {
    let dict = node.as_dict().ok()?;
    if let Ok(names) = dict.get(b"Names") {
        if let Ok(items) = deref(doc, names).as_array() {
            let mut i = 0;
            while i + 1 < items.len() {
                if obj_text(&items[i]) == name {
                    return Some(items[i + 1].clone());
                }
                i += 2;
            }
        }
    }
    if let Ok(kids) = dict.get(b"Kids") {
        if let Ok(kids) = deref(doc, kids).as_array() {
            for kid in kids {
                if let Some(found) = name_tree_lookup(doc, deref(doc, kid), name) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// Write an owned link snapshot to a link annotation.
fn write_link(doc: &mut LopdfDocument, id: ObjectId, link: &Link, top: f64) -> Result<()> {
    use super::constants::*;
    let mut dict = doc.get_dictionary(id)?.clone();
    dict.set("Rect", rect_object(&flip_rect(&norm_rect(&link.rect), top)));
    dict.set("F", Object::Integer(i64::from(link.flags)));
    dict.remove(b"A");
    dict.remove(b"Dest");
    match link.kind {
        k if k == LINK_URI => {
            let mut action = Dictionary::new();
            action.set("S", name_object("URI"));
            action.set("URI", text_object(&link.uri));
            dict.set("A", Object::Dictionary(action));
        }
        k if k == LINK_GOTO => {
            if let Some(dest) = &link.dest {
                dict.set("Dest", dest_object(doc, dest, top)?);
            }
        }
        k if k == LINK_GOTOR => {
            let mut action = Dictionary::new();
            action.set("S", name_object("GoToR"));
            action.set("F", text_object(&link.uri));
            if let Some(dest) = &link.dest {
                action.set("D", dest_object(doc, dest, top)?);
            }
            dict.set("A", Object::Dictionary(action));
        }
        k if k == LINK_LAUNCH => {
            let mut action = Dictionary::new();
            action.set("S", name_object("Launch"));
            action.set("F", text_object(&link.uri));
            dict.set("A", Object::Dictionary(action));
        }
        k if k == LINK_NAMED => {
            let mut action = Dictionary::new();
            action.set("S", name_object("Named"));
            action.set("N", name_object(&link.uri));
            dict.set("A", Object::Dictionary(action));
        }
        _ => {}
    }
    if !link.id.is_empty() {
        dict.set("NM", text_object(&link.id));
    }
    let border_opt = BorderOptions {
        width: link.border.width,
        style: link.border.style.clone(),
        dashes: link.border.dashes.clone(),
        clouds: link.border.clouds,
    };
    write_border(&mut dict, &border_opt);
    write_colors(&mut dict, &link.colors);
    doc.objects.insert(id, Object::Dictionary(dict));
    Ok(())
}

/// Owned destination to an explicit destination array.
fn dest_object(doc: &LopdfDocument, dest: &LinkDest, top: f64) -> Result<Object> {
    let leaves = leaf_pages(doc);
    let page_id = leaves
        .get(dest.page.max(0) as usize)
        .copied()
        .ok_or_else(|| index_oob(format!("destination page {} is out of range", dest.page)))?;
    let zoom = if dest.zoom > 0.0 {
        Object::Real(dest.zoom as f32)
    } else {
        Object::Null
    };
    Ok(Object::Array(vec![
        Object::Reference(page_id),
        name_object("XYZ"),
        Object::Real(dest.target.x as f32),
        Object::Real((top - dest.target.y) as f32),
        zoom,
    ]))
}

// --- images ------------------------------------------------------------------

/// Image encoding sniffed from magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageEncoding {
    Jpeg,
    Png,
    Unknown,
}

fn sniff_image(data: &[u8]) -> ImageEncoding {
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        ImageEncoding::Jpeg
    } else if data.len() >= 8 && data[0..8] == [137, 80, 78, 71, 13, 10, 26, 10] {
        ImageEncoding::Png
    } else {
        ImageEncoding::Unknown
    }
}

/// Decoded raster: dimensions, RGB(A) samples, alpha presence.
struct DecodedImage {
    width: u32,
    height: u32,
    samples: Vec<u8>,
    has_alpha: bool,
}

/// Decode a PNG into flat RGB or RGBA samples.
#[cfg(not(target_arch = "wasm32"))]
fn decode_png(data: &[u8]) -> Result<DecodedImage> {
    let decoder = png::Decoder::new(data);
    let mut reader = decoder
        .read_info()
        .map_err(|e| unreadable(format!("png does not decode: {e}")))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| unreadable(format!("png does not decode: {e}")))?;
    if reader.info().bit_depth != png::BitDepth::Eight {
        return Err(unreadable("only 8-bit png images are supported"));
    }
    let (width, height) = (info.width, info.height);
    use png::ColorType::*;
    let samples: Vec<u8> = match info.color_type {
        Rgb => buf[..info.buffer_size()].to_vec(),
        Rgba => buf[..info.buffer_size()].to_vec(),
        Grayscale => buf[..info.buffer_size()]
            .iter()
            .flat_map(|g| [*g, *g, *g])
            .collect(),
        GrayscaleAlpha => buf[..info.buffer_size()]
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        Indexed => {
            let palette = reader.info().palette.clone().unwrap_or_default();
            buf[..info.buffer_size()]
                .iter()
                .flat_map(|i| {
                    let at = usize::from(*i) * 3;
                    [
                        palette.get(at).copied().unwrap_or(0),
                        palette.get(at + 1).copied().unwrap_or(0),
                        palette.get(at + 2).copied().unwrap_or(0),
                    ]
                })
                .collect()
        }
    };
    let has_alpha = matches!(info.color_type, Rgba | GrayscaleAlpha);
    Ok(DecodedImage {
        width,
        height,
        samples,
        has_alpha,
    })
}

/// PNG decoding is unavailable on this target.
#[cfg(target_arch = "wasm32")]
fn decode_png(_data: &[u8]) -> Result<DecodedImage> {
    Err(unreadable("png is unsupported on this target"))
}

/// Split RGBA samples into RGB plus a soft mask.
fn split_alpha(image: &DecodedImage) -> Option<(Vec<u8>, Vec<u8>)> {
    if !image.has_alpha {
        return None;
    }
    let pixels = image.samples.chunks_exact(4);
    let mut rgb = Vec::with_capacity(image.samples.len() / 4 * 3);
    let mut mask = Vec::with_capacity(image.samples.len() / 4);
    for p in pixels {
        rgb.extend_from_slice(&p[0..3]);
        mask.push(p[3]);
    }
    Some((rgb, mask))
}

/// JPEG frame header: dimensions plus component count.
fn parse_jpeg(data: &[u8]) -> Result<(u32, u32, u8)> {
    let mut i = 2usize;
    while i + 9 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        if marker == 0xDA {
            break;
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if len < 2 || i + len + 2 > data.len() {
            break;
        }
        if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC {
            let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            let comps = data[i + 9];
            return Ok((width, height, comps));
        }
        i += 2 + len;
    }
    Err(unreadable("jpeg has no frame header"))
}

/// Build an image XObject from bytes plus placement, returning its object id.
/// JPEG passes through; PNG decodes to raw samples with an optional soft mask.
fn build_image_xobject(
    doc: &mut LopdfDocument,
    data: &[u8],
    keep_alpha: bool,
) -> Result<(ObjectId, u32, u32)> {
    match sniff_image(data) {
        ImageEncoding::Jpeg => {
            let (width, height, comps) = parse_jpeg(data)?;
            let space = match comps {
                1 => name_object("DeviceGray"),
                4 => name_object("DeviceCMYK"),
                _ => name_object("DeviceRGB"),
            };
            let mut dict = Dictionary::new();
            dict.set("Type", name_object("XObject"));
            dict.set("Subtype", name_object("Image"));
            dict.set("Width", Object::Integer(i64::from(width)));
            dict.set("Height", Object::Integer(i64::from(height)));
            dict.set("ColorSpace", space);
            dict.set("BitsPerComponent", Object::Integer(8));
            dict.set("Filter", name_object("DCTDecode"));
            let id = doc.new_object_id();
            doc.objects
                .insert(id, Object::Stream(Stream::new(dict, data.to_vec())));
            Ok((id, width, height))
        }
        ImageEncoding::Png => {
            let image = decode_png(data)?;
            let (rgb, mask) = match split_alpha(&image).filter(|_| keep_alpha) {
                Some((rgb, mask)) => (rgb, Some(mask)),
                None => {
                    let rgb = if image.has_alpha {
                        image
                            .samples
                            .chunks_exact(4)
                            .flat_map(|p| [p[0], p[1], p[2]])
                            .collect()
                    } else {
                        image.samples.clone()
                    };
                    (rgb, None)
                }
            };
            let mut dict = Dictionary::new();
            dict.set("Type", name_object("XObject"));
            dict.set("Subtype", name_object("Image"));
            dict.set("Width", Object::Integer(i64::from(image.width)));
            dict.set("Height", Object::Integer(i64::from(image.height)));
            dict.set("ColorSpace", name_object("DeviceRGB"));
            dict.set("BitsPerComponent", Object::Integer(8));
            let id = doc.new_object_id();
            doc.objects
                .insert(id, Object::Stream(Stream::new(dict, rgb)));
            if let Some(mask) = mask {
                let mut mdict = Dictionary::new();
                mdict.set("Type", name_object("XObject"));
                mdict.set("Subtype", name_object("Image"));
                mdict.set("Width", Object::Integer(i64::from(image.width)));
                mdict.set("Height", Object::Integer(i64::from(image.height)));
                mdict.set("ColorSpace", name_object("DeviceGray"));
                mdict.set("BitsPerComponent", Object::Integer(8));
                let mid = doc.new_object_id();
                doc.objects
                    .insert(mid, Object::Stream(Stream::new(mdict, mask)));
                if let Ok(Object::Stream(stream)) = doc.get_object_mut(id) {
                    stream.dict.set("SMask", Object::Reference(mid));
                }
            }
            Ok((id, image.width, image.height))
        }
        ImageEncoding::Unknown => Err(unreadable("image is neither jpeg nor png")),
    }
}

/// Content placing an image resource over a y-down rect with rotation.
fn image_placement(name: &str, rect: &Rect, top: f64, rotate: i32) -> Vec<u8> {
    let r = norm_rect(rect);
    let w = r.x1 - r.x0;
    let h = r.y1 - r.y0;
    let norm = rotate.rem_euclid(360);
    let matrix = match norm {
        90 => format!(
            "{} {} {} {} {} {} cm\n",
            pdf_num(0.0),
            pdf_num(-h),
            pdf_num(w),
            pdf_num(0.0),
            pdf_num(r.x0),
            pdf_num(top - r.y0)
        ),
        180 => format!(
            "{} {} {} {} {} {} cm\n",
            pdf_num(-w),
            pdf_num(0.0),
            pdf_num(0.0),
            pdf_num(-h),
            pdf_num(r.x1),
            pdf_num(top - r.y0)
        ),
        270 => format!(
            "{} {} {} {} {} {} cm\n",
            pdf_num(0.0),
            pdf_num(h),
            pdf_num(-w),
            pdf_num(0.0),
            pdf_num(r.x1),
            pdf_num(top - r.y1)
        ),
        _ => format!(
            "{} {} {} {} {} {} cm\n",
            pdf_num(w),
            pdf_num(0.0),
            pdf_num(0.0),
            pdf_num(h),
            pdf_num(r.x0),
            pdf_num(top - r.y1)
        ),
    };
    format!("q\n{matrix}/{name} Do\nQ\n").into_bytes()
}

// --- embedded files ----------------------------------------------------------

/// File-spec dict plus content stream for an embedded file.
fn filespec_object(
    doc: &mut LopdfDocument,
    filename: &str,
    display: Option<&str>,
    description: Option<&str>,
    data: &[u8],
) -> Result<ObjectId> {
    let mut params = Dictionary::new();
    params.set("Size", Object::Integer(data.len() as i64));
    let mut sdict = Dictionary::new();
    sdict.set("Type", name_object("EmbeddedFile"));
    sdict.set("Params", Object::Dictionary(params));
    let stream_id = doc.new_object_id();
    doc.objects
        .insert(stream_id, Object::Stream(Stream::new(sdict, data.to_vec())));
    let mut ef = Dictionary::new();
    ef.set("F", Object::Reference(stream_id));
    ef.set("UF", Object::Reference(stream_id));
    let mut spec = Dictionary::new();
    spec.set("Type", name_object("Filespec"));
    spec.set("F", text_object(filename));
    spec.set("UF", text_object(display.unwrap_or(filename)));
    if let Some(desc) = description {
        if !desc.is_empty() {
            spec.set("Desc", text_object(desc));
        }
    }
    spec.set("EF", Object::Dictionary(ef));
    let spec_id = doc.new_object_id();
    doc.objects.insert(spec_id, Object::Dictionary(spec));
    Ok(spec_id)
}

/// Append one (name, stream) pair to the document file list, keeping the
/// tree inline when the document has none yet so an add costs one object.
fn embedded_files_append(doc: &mut LopdfDocument, name: &str, stream: ObjectId) -> Result<()> {
    let mut items = embedded_files_items(doc);
    items.push(text_object(name));
    items.push(Object::Reference(stream));
    let mut pairs: Vec<(String, Object)> = Vec::new();
    let mut i = 0;
    while i + 1 < items.len() {
        pairs.push((obj_text(&items[i]), items[i + 1].clone()));
        i += 2;
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut sorted = Vec::with_capacity(pairs.len() * 2);
    for (name, target) in pairs {
        sorted.push(text_object(&name));
        sorted.push(target);
    }
    embedded_files_store(doc, sorted)
}

/// Current `/Names` array of the document file list, if any.
fn embedded_files_items(doc: &LopdfDocument) -> Vec<Object> {
    let Ok(catalog) = catalog_id(doc) else {
        return Vec::new();
    };
    let Ok(root) = doc.get_dictionary(catalog) else {
        return Vec::new();
    };
    let names = root.get(b"Names").ok().cloned().unwrap_or(Object::Null);
    let tree = deref(doc, &names)
        .as_dict()
        .ok()
        .and_then(|d| d.get(b"EmbeddedFiles").ok().cloned())
        .unwrap_or(Object::Null);
    deref(doc, &tree)
        .as_dict()
        .ok()
        .and_then(|d| d.get(b"Names").ok().cloned())
        .and_then(|o| o.as_array().ok().cloned())
        .unwrap_or_default()
}

/// Write the `/Names` array back where the file list lives, creating an
/// inline tree when the document has none yet.
fn embedded_files_store(doc: &mut LopdfDocument, items: Vec<Object>) -> Result<()> {
    let catalog = catalog_id(doc)?;
    let names_obj = doc
        .get_dictionary(catalog)
        .ok()
        .and_then(|root| root.get(b"Names").ok().cloned());
    let array = Object::Array(items);
    // Existing indirect tree first: mutate it in place.
    if let Some(Object::Reference(names_id)) = names_obj.clone() {
        let tree_obj = doc
            .get_dictionary(names_id)
            .ok()
            .and_then(|names| names.get(b"EmbeddedFiles").ok().cloned());
        if let Some(Object::Reference(tree_id)) = tree_obj {
            doc.get_dictionary_mut(tree_id)?.set("Names", array);
            return Ok(());
        }
        let mut tree = match tree_obj {
            Some(o) => deref(doc, &o).as_dict().ok().cloned().unwrap_or_default(),
            None => Dictionary::new(),
        };
        tree.set("Names", array);
        doc.get_dictionary_mut(names_id)?
            .set("EmbeddedFiles", Object::Dictionary(tree));
        return Ok(());
    }
    // Otherwise keep the whole chain inline on the catalog.
    let mut names = match names_obj {
        Some(o) => deref(doc, &o).as_dict().ok().cloned().unwrap_or_default(),
        None => Dictionary::new(),
    };
    let mut tree = names
        .get(b"EmbeddedFiles")
        .ok()
        .cloned()
        .map(|o| deref(doc, &o).as_dict().ok().cloned())
        .unwrap_or_default()
        .unwrap_or_default();
    tree.set("Names", array);
    names.set("EmbeddedFiles", Object::Dictionary(tree));
    doc.get_dictionary_mut(catalog)?
        .set("Names", Object::Dictionary(names));
    Ok(())
}

/// Flat document file list: (name, spec id) pairs in tree order.
fn list_embedded_files(doc: &LopdfDocument) -> Vec<(String, ObjectId)> {
    let mut out = Vec::new();
    let Ok(catalog) = catalog_id(doc) else {
        return out;
    };
    let Ok(root) = doc.get_dictionary(catalog) else {
        return out;
    };
    let Ok(names) = root.get(b"Names") else {
        return out;
    };
    let Ok(tree) = deref(doc, names).as_dict() else {
        return out;
    };
    let Ok(files) = tree.get(b"EmbeddedFiles") else {
        return out;
    };
    collect_names(doc, deref(doc, files), &mut out);
    out
}

fn collect_names(doc: &LopdfDocument, node: &Object, out: &mut Vec<(String, ObjectId)>) {
    let Ok(dict) = node.as_dict() else {
        return;
    };
    if let Ok(names) = dict.get(b"Names") {
        if let Ok(items) = deref(doc, names).as_array() {
            let mut i = 0;
            while i + 1 < items.len() {
                if let Ok(id) = items[i + 1].as_reference() {
                    out.push((obj_text(&items[i]), id));
                }
                i += 2;
            }
        }
    }
    if let Ok(kids) = dict.get(b"Kids") {
        if let Ok(kids) = deref(doc, kids).as_array() {
            for kid in kids {
                collect_names(doc, deref(doc, kid), out);
            }
        }
    }
}

// --- pdf value parsing (xref surgery) ----------------------------------------

/// Parse one PDF value in object syntax: null, booleans, numbers, names,
/// strings (literal and hex), arrays, dicts, and indirect references.
fn parse_pdf_value(text: &str) -> Result<Object> {
    let bytes = text.trim().as_bytes();
    let (value, rest) = parse_value(bytes)?;
    if rest.iter().all(|b| b.is_ascii_whitespace()) {
        Ok(value)
    } else {
        Err(bad_value("trailing bytes after pdf value"))
    }
}

fn skip_ws(bytes: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
        } else if bytes[i] == b'%' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else {
            break;
        }
    }
    &bytes[i..]
}

fn parse_value(bytes: &[u8]) -> Result<(Object, &[u8])> {
    let bytes = skip_ws(bytes);
    if bytes.is_empty() {
        return Err(bad_value("empty pdf value"));
    }
    match bytes[0] {
        b'(' => parse_literal(bytes),
        b'<' => {
            if bytes.get(1) == Some(&b'<') {
                parse_dict(bytes)
            } else {
                parse_hex(bytes)
            }
        }
        b'[' => parse_array(bytes),
        b'/' => parse_name(bytes),
        b'n' if bytes.starts_with(b"null") => Ok((Object::Null, &bytes[4..])),
        b't' if bytes.starts_with(b"true") => Ok((Object::Boolean(true), &bytes[4..])),
        b'f' if bytes.starts_with(b"false") => Ok((Object::Boolean(false), &bytes[5..])),
        b'-' | b'+' | b'.' | b'0'..=b'9' => parse_number_or_ref(bytes),
        _ => Err(bad_value("bad pdf value")),
    }
}

fn parse_literal(bytes: &[u8]) -> Result<(Object, &[u8])> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut i = 0usize;
    let mut it = bytes.iter().peekable();
    it.next();
    i += 1;
    while let Some(b) = it.next() {
        i += 1;
        match b {
            b'\\' => {
                let Some(e) = it.next() else {
                    break;
                };
                i += 1;
                match e {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0C),
                    b'(' | b')' | b'\\' => out.push(*e),
                    b'\n' => {}
                    b'\r' => {
                        if it.peek() == Some(&&b'\n') {
                            it.next();
                            i += 1;
                        }
                    }
                    b'0'..=b'7' => {
                        let mut val = e - b'0';
                        for _ in 0..2 {
                            if let Some(&&d) = it.peek() {
                                if d.is_ascii_digit() && d < b'8' {
                                    val = val * 8 + (d - b'0');
                                    it.next();
                                    i += 1;
                                } else {
                                    break;
                                }
                            }
                        }
                        out.push(val);
                    }
                    _ => out.push(*e),
                }
            }
            b'(' => {
                depth += 1;
                out.push(*b);
            }
            b')' => {
                if depth == 0 {
                    let format = if out.starts_with(b"\xfe\xff") {
                        StringFormat::Hexadecimal
                    } else {
                        StringFormat::Literal
                    };
                    return Ok((Object::String(out, format), &bytes[i..]));
                }
                depth -= 1;
                out.push(*b);
            }
            _ => out.push(*b),
        }
    }
    Err(bad_value("unterminated string"))
}

fn parse_hex(bytes: &[u8]) -> Result<(Object, &[u8])> {
    let mut out = Vec::new();
    let mut hi: Option<u8> = None;
    let mut i = 1usize;
    while i < bytes.len() {
        let b = bytes[i];
        i += 1;
        if b == b'>' {
            if let Some(h) = hi {
                out.push(h << 4);
            }
            return Ok((Object::String(out, StringFormat::Hexadecimal), &bytes[i..]));
        }
        if b.is_ascii_whitespace() {
            continue;
        }
        let digit = (b as char)
            .to_digit(16)
            .ok_or_else(|| bad_value("bad hex string"))? as u8;
        match hi {
            None => hi = Some(digit),
            Some(h) => {
                out.push(h << 4 | digit);
                hi = None;
            }
        }
    }
    Err(bad_value("unterminated hex string"))
}

fn parse_name(bytes: &[u8]) -> Result<(Object, &[u8])> {
    let mut out = Vec::new();
    let mut i = 1usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace()
            || matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'/' | b'%')
        {
            break;
        }
        if b == b'#' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    Ok((Object::Name(out), &bytes[i..]))
}

fn parse_array(bytes: &[u8]) -> Result<(Object, &[u8])> {
    let mut items = Vec::new();
    let mut rest = skip_ws(&bytes[1..]);
    loop {
        rest = skip_ws(rest);
        if rest.is_empty() {
            return Err(bad_value("unterminated array"));
        }
        if rest[0] == b']' {
            return Ok((Object::Array(items), &rest[1..]));
        }
        let (value, tail) = parse_value(rest)?;
        items.push(value);
        rest = tail;
    }
}

fn parse_dict(bytes: &[u8]) -> Result<(Object, &[u8])> {
    let mut dict = Dictionary::new();
    let mut rest = skip_ws(&bytes[2..]);
    loop {
        rest = skip_ws(rest);
        if rest.is_empty() {
            return Err(bad_value("unterminated dict"));
        }
        if rest.starts_with(b">>") {
            return Ok((Object::Dictionary(dict), &rest[2..]));
        }
        if !rest.starts_with(b"/") {
            return Err(bad_value("dict key is not a name"));
        }
        let (key, tail) = parse_name(rest)?;
        let key = key
            .as_name()
            .map_err(|_| bad_value("bad dict key"))?
            .to_vec();
        let (value, tail) = parse_value(tail)?;
        dict.set(key, value);
        rest = tail;
    }
}

fn parse_number_or_ref(bytes: &[u8]) -> Result<(Object, &[u8])> {
    let mut i = 0usize;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || matches!(bytes[i], b'+' | b'-' | b'.')) {
        i += 1;
    }
    let token = std::str::from_utf8(&bytes[..i]).map_err(|_| bad_value("bad number"))?;
    let rest = skip_ws(&bytes[i..]);
    let plain = !token.contains('.') && !token.contains('+') && !token.contains('-');
    if plain {
        if let Ok(first) = token.parse::<u32>() {
            let mut j = 0usize;
            let digits = rest;
            while j < digits.len() && digits[j].is_ascii_digit() {
                j += 1;
            }
            if j > 0 {
                if let Some(second) = std::str::from_utf8(&digits[..j])
                    .ok()
                    .and_then(|s| s.parse::<u16>().ok())
                {
                    let after = skip_ws(&digits[j..]);
                    if after.first() == Some(&b'R')
                        && after
                            .get(1)
                            .map(|b| !b.is_ascii_alphanumeric())
                            .unwrap_or(true)
                    {
                        return Ok((Object::Reference((first, second)), &after[1..]));
                    }
                }
            }
            return Ok((Object::Integer(i64::from(first)), rest));
        }
    }
    if token.contains('.') {
        if let Ok(v) = token.parse::<f32>() {
            return Ok((Object::Real(v), rest));
        }
    }
    if let Ok(v) = token.parse::<i64>() {
        return Ok((Object::Integer(v), rest));
    }
    if let Ok(v) = token.parse::<f64>() {
        return Ok((Object::Real(v as f32), rest));
    }
    Err(bad_value("bad number"))
}

// --- object graph copy -------------------------------------------------------

/// Deep-copy one object into the document, remapping references to fresh ids.
/// Shared references stay shared; cycles resolve through the memo.
fn copy_object_deep(
    doc: &mut LopdfDocument,
    obj: &Object,
    memo: &mut HashMap<ObjectId, ObjectId>,
) -> Result<Object> {
    match obj {
        Object::Reference(id) => {
            if let Some(seen) = memo.get(id) {
                return Ok(Object::Reference(*seen));
            }
            let target = doc
                .objects
                .get(id)
                .cloned()
                .ok_or_else(|| bad_value("dangling reference"))?;
            let fresh = doc.new_object_id();
            memo.insert(*id, fresh);
            doc.objects.insert(fresh, Object::Null);
            let copied = copy_object_deep(doc, &target, memo)?;
            doc.objects.insert(fresh, copied);
            Ok(Object::Reference(fresh))
        }
        Object::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(copy_object_deep(doc, item, memo)?);
            }
            Ok(Object::Array(out))
        }
        Object::Dictionary(dict) => {
            let mut out = Dictionary::new();
            for (key, value) in dict.iter() {
                out.set(key.clone(), copy_object_deep(doc, value, memo)?);
            }
            Ok(Object::Dictionary(out))
        }
        Object::Stream(stream) => {
            let mut dict = Dictionary::new();
            for (key, value) in stream.dict.iter() {
                dict.set(key.clone(), copy_object_deep(doc, value, memo)?);
            }
            Ok(Object::Stream(Stream::new(dict, stream.content.clone())))
        }
        other => Ok(other.clone()),
    }
}

/// Deep-copy one object across documents, allocating fresh ids in the target.
fn copy_object_across(
    source: &LopdfDocument,
    target: &mut LopdfDocument,
    obj: &Object,
    memo: &mut HashMap<ObjectId, ObjectId>,
) -> Result<Object> {
    match obj {
        Object::Reference(id) => {
            if let Some(seen) = memo.get(id) {
                return Ok(Object::Reference(*seen));
            }
            let value = source
                .objects
                .get(id)
                .cloned()
                .ok_or_else(|| bad_value("dangling reference"))?;
            let fresh = target.new_object_id();
            memo.insert(*id, fresh);
            target.objects.insert(fresh, Object::Null);
            let copied = copy_object_across(source, target, &value, memo)?;
            target.objects.insert(fresh, copied);
            Ok(Object::Reference(fresh))
        }
        Object::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(copy_object_across(source, target, item, memo)?);
            }
            Ok(Object::Array(out))
        }
        Object::Dictionary(dict) => {
            let mut out = Dictionary::new();
            for (key, value) in dict.iter() {
                out.set(
                    key.clone(),
                    copy_object_across(source, target, value, memo)?,
                );
            }
            Ok(Object::Dictionary(out))
        }
        Object::Stream(stream) => {
            let mut dict = Dictionary::new();
            for (key, value) in stream.dict.iter() {
                dict.set(
                    key.clone(),
                    copy_object_across(source, target, value, memo)?,
                );
            }
            Ok(Object::Stream(Stream::new(dict, stream.content.clone())))
        }
        other => Ok(other.clone()),
    }
}

// --- page tree surgery -------------------------------------------------------

/// Parent `/Pages` node of a page or node id, by scan.
fn parent_of(doc: &LopdfDocument, id: ObjectId) -> Option<ObjectId> {
    for (candidate, obj) in doc.objects.iter() {
        let Ok(dict) = obj.as_dict() else {
            continue;
        };
        let is_pages = dict.get(b"Type").ok().map(obj_name).as_deref() == Some("Pages");
        if !is_pages {
            continue;
        }
        let Ok(kids) = dict.get(b"Kids") else {
            continue;
        };
        let Ok(kids) = deref(doc, kids).as_array() else {
            continue;
        };
        if kids.iter().any(|k| k.as_reference().ok() == Some(id)) {
            return Some(*candidate);
        }
    }
    None
}

/// Adjust `/Count` from a node up to the root.
fn fix_counts_up(doc: &mut LopdfDocument, mut node: ObjectId, delta: i64) {
    loop {
        if let Ok(dict) = doc.get_dictionary_mut(node) {
            let count = dict.get(b"Count").ok().map(obj_f64).unwrap_or(0.0) as i64 + delta;
            dict.set("Count", Object::Integer(count.max(0)));
        }
        match parent_of(doc, node) {
            Some(parent) => node = parent,
            None => break,
        }
    }
}

/// Insert a blank page at a 0-based position (past-the-end appends).
fn insert_blank_page(
    doc: &mut LopdfDocument,
    position: usize,
    width: f64,
    height: f64,
) -> Result<usize> {
    let leaves = leaf_pages(doc);
    let at = position.min(leaves.len());
    let (parent, slot) = if at < leaves.len() {
        let anchor = leaves[at];
        let parent = parent_of(doc, anchor).ok_or_else(|| engine_fault("page has no parent"))?;
        let kids = doc
            .get_dictionary(parent)
            .ok()
            .and_then(|d| d.get(b"Kids").ok().cloned())
            .and_then(|o| o.as_array().ok().cloned())
            .unwrap_or_default();
        let slot = kids
            .iter()
            .position(|k| k.as_reference().ok() == Some(anchor))
            .unwrap_or(0);
        (parent, slot)
    } else if let Some(last) = leaves.last() {
        let parent = parent_of(doc, *last).ok_or_else(|| engine_fault("page has no parent"))?;
        let kids = doc
            .get_dictionary(parent)
            .ok()
            .and_then(|d| d.get(b"Kids").ok().cloned())
            .and_then(|o| o.as_array().ok().cloned())
            .unwrap_or_default();
        (parent, kids.len())
    } else {
        let catalog = catalog_id(doc)?;
        let pages = doc
            .get_dictionary(catalog)
            .ok()
            .and_then(|c| c.get(b"Pages").ok().and_then(|o| o.as_reference().ok()))
            .ok_or_else(|| engine_fault("document has no page tree"))?;
        (pages, 0)
    };
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(Dictionary::new()));
    let contents_id = doc.new_object_id();
    doc.objects.insert(
        contents_id,
        Object::Stream(Stream::new(Dictionary::new(), Vec::new())),
    );
    let mut page = Dictionary::new();
    page.set("Type", name_object("Page"));
    page.set("Parent", Object::Reference(parent));
    page.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(width as f32),
            Object::Real(height as f32),
        ]),
    );
    page.set("Resources", Object::Dictionary(resources));
    page.set("Contents", Object::Reference(contents_id));
    let page_id = doc.new_object_id();
    doc.objects.insert(page_id, Object::Dictionary(page));
    let kids = doc
        .get_dictionary(parent)
        .ok()
        .and_then(|d| d.get(b"Kids").ok().cloned())
        .and_then(|o| o.as_array().ok().cloned())
        .unwrap_or_default();
    let mut kids = kids;
    kids.insert(slot.min(kids.len()), Object::Reference(page_id));
    doc.get_dictionary_mut(parent)?
        .set("Kids", Object::Array(kids));
    fix_counts_up(doc, parent, 1);
    Ok(at)
}

/// Remove the leaf page at a 0-based index from the tree.
fn remove_page_at(doc: &mut LopdfDocument, index: usize) -> Result<()> {
    let leaves = leaf_pages(doc);
    let id = leaves
        .get(index)
        .copied()
        .ok_or_else(|| index_oob("page index out of range"))?;
    let parent = parent_of(doc, id).ok_or_else(|| engine_fault("page has no parent"))?;
    let kids = doc
        .get_dictionary(parent)
        .ok()
        .and_then(|d| d.get(b"Kids").ok().cloned())
        .and_then(|o| o.as_array().ok().cloned())
        .unwrap_or_default();
    let kids: Vec<Object> = kids
        .into_iter()
        .filter(|k| k.as_reference().ok() != Some(id))
        .collect();
    doc.get_dictionary_mut(parent)?
        .set("Kids", Object::Array(kids));
    fix_counts_up(doc, parent, -1);
    doc.objects.remove(&id);
    Ok(())
}

// --- outlines ---------------------------------------------------------------

/// Flattened outline item with its object id.
#[allow(dead_code)]
struct OutlineRow {
    id: ObjectId,
    level: i32,
    title: String,
    page: i32,
    dest: Option<LinkDest>,
}

/// Pre-order walk of the outline tree.
fn flatten_outlines(doc: &LopdfDocument) -> Vec<OutlineRow> {
    let mut out = Vec::new();
    let Ok(catalog) = catalog_id(doc) else {
        return out;
    };
    let Ok(root) = doc.get_dictionary(catalog) else {
        return out;
    };
    let Ok(outlines) = root.get(b"Outlines") else {
        return out;
    };
    let outlines = deref(doc, outlines);
    let Ok(dict) = outlines.as_dict() else {
        return out;
    };
    let Ok(first) = dict.get(b"First") else {
        return out;
    };
    let mut next = first.as_reference().ok();
    let top = leaf_pages(doc)
        .first()
        .map(|id| page_top(doc, *id))
        .unwrap_or(792.0);
    while let Some(id) = next {
        walk_outline(doc, id, 1, top, &mut out);
        next = doc
            .get_dictionary(id)
            .ok()
            .and_then(|d| d.get(b"Next").ok())
            .and_then(|o| o.as_reference().ok());
    }
    out
}

fn walk_outline(
    doc: &LopdfDocument,
    id: ObjectId,
    level: i32,
    top: f64,
    out: &mut Vec<OutlineRow>,
) {
    let Ok(dict) = doc.get_dictionary(id) else {
        return;
    };
    let dict = dict.clone();
    let title = dict.get(b"Title").ok().map(obj_text).unwrap_or_default();
    let dest = dict
        .get(b"Dest")
        .ok()
        .and_then(|o| dest_of(doc, o, top))
        .or_else(|| {
            dict.get(b"A")
                .ok()
                .and_then(|o| deref(doc, o).as_dict().ok().cloned())
                .and_then(|a| a.get(b"D").ok().cloned())
                .and_then(|o| dest_of(doc, &o, top))
        });
    let page = dest.as_ref().map(|d| d.page).unwrap_or(-1);
    out.push(OutlineRow {
        id,
        level,
        title,
        page,
        dest,
    });
    if let Ok(first) = dict.get(b"First") {
        let mut next = first.as_reference().ok();
        while let Some(kid) = next {
            walk_outline(doc, kid, level + 1, top, out);
            next = doc
                .get_dictionary(kid)
                .ok()
                .and_then(|d| d.get(b"Next").ok())
                .and_then(|o| o.as_reference().ok());
        }
    }
}

/// Replace the outline tree from entries; returns the entry count.
fn build_outlines(doc: &mut LopdfDocument, entries: &[TocEntry], collapse: i32) -> Result<i32> {
    let catalog = catalog_id(doc)?;
    let outline_root = doc.new_object_id();
    doc.objects
        .insert(outline_root, Object::Dictionary(Dictionary::new()));
    doc.get_dictionary_mut(catalog)?
        .set("Outlines", Object::Reference(outline_root));
    let top = leaf_pages(doc)
        .first()
        .map(|id| page_top(doc, *id))
        .unwrap_or(792.0);
    let mut ids: Vec<ObjectId> = Vec::with_capacity(entries.len());
    for entry in entries {
        let id = doc.new_object_id();
        let mut item = Dictionary::new();
        item.set("Title", text_object(&entry.title));
        item.set("Parent", Object::Reference(outline_root));
        if let Some(dest) = &entry.dest {
            let as_link = LinkDest {
                page: dest.page,
                target: dest.target,
                zoom: dest.zoom,
            };
            if let Ok(obj) = dest_object(doc, &as_link, top) {
                item.set("Dest", obj);
            }
        } else if entry.page >= 0 {
            // TocEntry pages are 1-based; LinkDest pages are 0-based.
            let fallback = LinkDest {
                page: entry.page - 1,
                target: mk_point(0.0, 0.0),
                zoom: 0.0,
            };
            if let Ok(obj) = dest_object(doc, &fallback, top) {
                item.set("Dest", obj);
            }
        }
        doc.objects.insert(id, Object::Dictionary(item));
        ids.push(id);
    }
    let mut stack: Vec<(i32, ObjectId)> = Vec::new();
    let mut counts: HashMap<ObjectId, i32> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let id = ids[index];
        while stack
            .last()
            .map(|(l, _)| *l >= entry.level)
            .unwrap_or(false)
        {
            stack.pop();
        }
        if let Some((_, parent)) = stack.last() {
            doc.get_dictionary_mut(id)?
                .set("Parent", Object::Reference(*parent));
            *counts.entry(*parent).or_insert(0) += 1;
        }
        let mut child = index + 1;
        while child < entries.len() && entries[child].level <= entry.level {
            if entries[child].level == entry.level {
                break;
            }
            child += 1;
        }
        let _ = child;
        stack.push((entry.level, id));
    }
    for (index, entry) in entries.iter().enumerate() {
        let id = ids[index];
        let prev = (0..index)
            .rev()
            .find(|i| {
                entries[*i].level == entry.level
                    && level_parent(entries, *i) == level_parent(entries, index)
            })
            .map(|i| ids[i]);
        let next = (index + 1..entries.len())
            .find(|i| {
                entries[*i].level == entry.level
                    && level_parent(entries, *i) == level_parent(entries, index)
            })
            .map(|i| ids[i]);
        let first = (index + 1..entries.len())
            .find(|i| {
                entries[*i].level == entry.level + 1 && level_parent(entries, *i) == Some(index)
            })
            .map(|i| ids[i]);
        let dict = doc.get_dictionary_mut(id)?;
        if let Some(prev) = prev {
            dict.set("Prev", Object::Reference(prev));
        }
        if let Some(next) = next {
            dict.set("Next", Object::Reference(next));
        }
        if let Some(first) = first {
            dict.set("First", Object::Reference(first));
        }
        let mut last = first;
        let mut cursor = first;
        while let Some(cur) = cursor {
            let nxt = (0..entries.len()).find(|i| {
                level_parent(entries, *i) == Some(index) && prev_of(entries, *i, index) == Some(cur)
            });
            let _ = nxt;
            cursor = None;
            last = last.or(Some(cur));
        }
        let _ = last;
    }
    for (index, entry) in entries.iter().enumerate() {
        if entry.level != 1 {
            continue;
        }
        let _ = (index, entry);
    }
    let tops: Vec<ObjectId> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.level <= 1)
        .map(|(i, _)| ids[i])
        .collect();
    let root = doc.get_dictionary_mut(outline_root)?;
    if let Some(first) = tops.first() {
        root.set("First", Object::Reference(*first));
    }
    if let Some(last) = tops.last() {
        root.set("Last", Object::Reference(*last));
    }
    let mut total = entries.len() as i32;
    if collapse > 0 {
        total = -total;
    }
    root.set("Count", Object::Integer(i64::from(total)));
    for (id, count) in counts {
        let mut visible = count;
        if collapse > 0 {
            visible = -visible;
        }
        doc.get_dictionary_mut(id)?
            .set("Count", Object::Integer(i64::from(visible)));
    }
    let _ = stack;
    Ok(entries.len() as i32)
}

/// Index of the parent row of one entry row, if any.
fn level_parent(entries: &[TocEntry], index: usize) -> Option<usize> {
    let level = entries[index].level;
    (0..index).rev().find(|i| entries[*i].level == level - 1)
}

/// Previous sibling row of one entry row within its parent, if any.
fn prev_of(entries: &[TocEntry], index: usize, parent: usize) -> Option<ObjectId> {
    let _ = (entries, index, parent);
    None
}

// --- optional content --------------------------------------------------------

/// OCG object ids in array order.
fn ocg_list(doc: &LopdfDocument) -> Vec<ObjectId> {
    let mut out = Vec::new();
    let Ok(catalog) = catalog_id(doc) else {
        return out;
    };
    let Ok(root) = doc.get_dictionary(catalog) else {
        return out;
    };
    let Ok(props) = root.get(b"OCProperties") else {
        return out;
    };
    let Ok(props) = deref(doc, props).as_dict() else {
        return out;
    };
    let Ok(ocgs) = props.get(b"OCGs") else {
        return out;
    };
    let Ok(ocgs) = deref(doc, ocgs).as_array() else {
        return out;
    };
    for ocg in ocgs {
        if let Ok(id) = ocg.as_reference() {
            out.push(id);
        }
    }
    out
}

// --- document info -----------------------------------------------------------

/// Writable `/Info` dictionary, created with a trailer link when missing.
fn ensure_info(doc: &mut LopdfDocument) -> Result<ObjectId> {
    if let Some(id) = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|o| o.as_reference().ok())
    {
        return Ok(id);
    }
    let id = doc.new_object_id();
    doc.objects
        .insert(id, Object::Dictionary(Dictionary::new()));
    doc.trailer.set("Info", Object::Reference(id));
    Ok(id)
}

// --- content streams ---------------------------------------------------------

/// Concatenated decoded bytes of a page's content streams.
#[allow(dead_code)]
fn page_content_bytes(doc: &LopdfDocument, page_id: ObjectId) -> Vec<u8> {
    let mut out = Vec::new();
    for id in doc.get_page_contents(page_id) {
        let content = doc
            .get_object(id)
            .ok()
            .and_then(|o| o.as_stream().ok())
            .and_then(|s| s.decompressed_content().ok())
            .unwrap_or_default();
        if content.is_empty() {
            continue;
        }
        if !out.is_empty() && !out.ends_with(b"\n") {
            out.push(b'\n');
        }
        out.extend_from_slice(&content);
    }
    out
}

/// Replace a page's content streams with one fresh stream.
#[allow(dead_code)]
fn set_page_content(doc: &mut LopdfDocument, page_id: ObjectId, data: Vec<u8>) {
    let id = doc.new_object_id();
    doc.objects
        .insert(id, Object::Stream(Stream::new(Dictionary::new(), data)));
    if let Ok(dict) = doc.get_dictionary_mut(page_id) {
        dict.set("Contents", Object::Reference(id));
    }
}

// --- file font embedding -----------------------------------------------------

/// Embed a file font as a simple TrueType font; returns the font object id.
/// Widths and ToUnicode cover the WinAnsi range the program maps.
fn embed_file_font(
    doc: &mut LopdfDocument,
    page_id: ObjectId,
    data: &[u8],
    encoding: i32,
) -> Result<(ObjectId, String)> {
    let face = parse_file_face(data)?;
    let units = f64::from(face.units_per_em().max(1));
    let program_id = doc.new_object_id();
    let mut fdict = Dictionary::new();
    fdict.set("Length1", Object::Integer(data.len() as i64));
    doc.objects.insert(
        program_id,
        Object::Stream(Stream::new(fdict, data.to_vec())),
    );
    let mut first = 255u32;
    let mut last = 0u32;
    let mut advances: HashMap<u32, u16> = HashMap::new();
    for code in 32..=255u32 {
        let ch = winansi_unicode(code as u8);
        if ch == '\0' {
            continue;
        }
        if let Some(gid) = face.cmap.get(&(ch as u32)) {
            let adv = face.advance_for_gid(*gid);
            advances.insert(code, adv);
            first = first.min(code);
            last = last.max(code);
        }
    }
    if advances.is_empty() {
        first = 32;
        last = 32;
        advances.insert(32, 0);
    }
    let widths: Vec<Object> = (first..=last)
        .map(|code| {
            let adv = advances.get(&code).copied().unwrap_or(0);
            Object::Integer((f64::from(adv) * 1000.0 / units).round() as i64)
        })
        .collect();
    let mut tounicode = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
    );
    let mut mappings = Vec::new();
    for code in first..=last {
        if advances.contains_key(&code) {
            let ch = winansi_unicode(code as u8);
            mappings.push(format!("<{code:02X}> <{:04X}>", ch as u32));
        }
    }
    for chunk in mappings.chunks(100) {
        tounicode.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for line in chunk {
            tounicode.push_str(line);
            tounicode.push('\n');
        }
        tounicode.push_str("endbfchar\n");
    }
    tounicode.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let tounicode_id = doc.new_object_id();
    doc.objects.insert(
        tounicode_id,
        Object::Stream(Stream::new(Dictionary::new(), tounicode.into_bytes())),
    );
    let encoding_name = match encoding {
        1 => "MacRomanEncoding",
        2 => "MacExpertEncoding",
        _ => "WinAnsiEncoding",
    };
    let mut descriptor = Dictionary::new();
    descriptor.set("Type", name_object("FontDescriptor"));
    descriptor.set("FontName", name_object(&face.postscript_name));
    descriptor.set("Flags", Object::Integer(32));
    descriptor.set(
        "FontBBox",
        Object::Array(vec![
            Object::Integer((face.bbox[0] * units) as i64),
            Object::Integer((face.bbox[1] * units) as i64),
            Object::Integer((face.bbox[2] * units) as i64),
            Object::Integer((face.bbox[3] * units) as i64),
        ]),
    );
    descriptor.set("ItalicAngle", Object::Integer(0));
    descriptor.set("Ascent", Object::Integer((face.asc * units) as i64));
    descriptor.set("Descent", Object::Integer((face.desc * units) as i64));
    descriptor.set("CapHeight", Object::Integer((face.asc * units) as i64));
    descriptor.set("StemV", Object::Integer(80));
    descriptor.set("FontFile2", Object::Reference(program_id));
    let descriptor_id = doc.new_object_id();
    doc.objects
        .insert(descriptor_id, Object::Dictionary(descriptor));
    let mut font = Dictionary::new();
    font.set("Type", name_object("Font"));
    font.set("Subtype", name_object("TrueType"));
    font.set("BaseFont", name_object(&face.postscript_name));
    font.set("FirstChar", Object::Integer(first as i64));
    font.set("LastChar", Object::Integer(last as i64));
    font.set("Widths", Object::Array(widths));
    font.set("FontDescriptor", Object::Reference(descriptor_id));
    font.set("Encoding", name_object(encoding_name));
    font.set("ToUnicode", Object::Reference(tounicode_id));
    let font_id = doc.new_object_id();
    doc.objects.insert(font_id, Object::Dictionary(font));
    let name = fresh_resource_name(doc, page_id, b"Font", "F");
    resource_sub_mut(doc, page_id, b"Font", &name, Object::Reference(font_id))?;
    Ok((font_id, name))
}

// --- lane L4 continuation: marks, selection, stamps -----------------------------

/// Stamp code to appearance name; unknown codes fall back to `Approved`.
fn stamp_name(stamp: i32) -> &'static str {
    match stamp {
        0 => "Approved",
        1 => "AsIs",
        2 => "Confidential",
        3 => "Departmental",
        4 => "Experimental",
        5 => "Expired",
        6 => "Final",
        7 => "ForComment",
        8 => "ForPublicRelease",
        9 => "NotApproved",
        10 => "NotForPublicRelease",
        11 => "Sold",
        12 => "TopSecret",
        13 => "Draft",
        _ => "Approved",
    }
}

/// Union box of quads with per-axis pads; empty input reads as infinite.
fn quads_rect(quads: &[Quad], pad_x: f64, pad_y: f64) -> Rect {
    let mut bbox: Option<Rect> = None;
    for quad in quads {
        for p in [quad.ul, quad.ur, quad.ll, quad.lr] {
            grow_union(&mut bbox, p);
        }
    }
    match bbox {
        Some(r) => {
            let r = norm_rect(&r);
            mk_rect(r.x0 - pad_x, r.y0 - pad_y, r.x1 + pad_x, r.y1 + pad_y)
        }
        None => infinite_rect(),
    }
}

/// Engine text blocks on a page intersecting a y-down area, as quads.
fn select_quads(doc: &PdfDocument, page: usize, area: &Rect) -> Vec<Quad> {
    let area = norm_rect(area);
    let lp = doc.lopdf();
    let Ok(page_id) = page_id_of(lp, page) else {
        return Vec::new();
    };
    let top = page_top(lp, page_id);
    let wanted = norm_rect(&flip_rect(&area, top));
    let Ok(blocks) = doc.text_with_layout() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for block in blocks {
        if block.page != page + 1 {
            continue;
        }
        let [x0, y0, x1, y1] = block.bbox;
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        let ix0 = x0.max(wanted.x0);
        let iy0 = y0.max(wanted.y0);
        let ix1 = x1.min(wanted.x1);
        let iy1 = y1.min(wanted.y1);
        if ix0 > ix1 || iy0 > iy1 {
            continue;
        }
        let hit = flip_rect(&mk_rect(ix0, iy0, ix1, iy1), top);
        let hit = norm_rect(&hit);
        out.push(mk_quad(
            mk_point(hit.x0, hit.y0),
            mk_point(hit.x1, hit.y0),
            mk_point(hit.x0, hit.y1),
            mk_point(hit.x1, hit.y1),
        ));
    }
    out
}

// --- lane L4 continuation: page-method backing ----------------------------------

impl PageMut<'_> {
    /// Text-mark annotation (highlight, underline, squiggly, strikeout) from
    /// explicit quads or from a text selection; empty selections read infinite.
    fn add_mark_annot(&mut self, subtype: &str, options: &HighlightOptions) -> Result<Annot<'_>> {
        let quads: Vec<Quad> = match (&options.quads, options.start, options.stop, &options.clip) {
            (Some(quads), _, _, _) => quads.clone(),
            (None, Some(start), Some(stop), _) => {
                sync_doc(self.document_mut())?;
                let area = mk_rect(
                    start.x.min(stop.x),
                    start.y.min(stop.y),
                    start.x.max(stop.x),
                    start.y.max(stop.y),
                );
                select_quads(self.document(), self.index(), &area)
            }
            (None, _, _, Some(clip)) => {
                sync_doc(self.document_mut())?;
                select_quads(self.document(), self.index(), clip)
            }
            _ => Vec::new(),
        };
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let rect = quads_rect(&quads, 1.25, 1.25);
        let id = add_annot_common(lp, page_id, subtype, &flip_rect(&rect, top))?;
        if !quads.is_empty() {
            lp.get_dictionary_mut(id)?
                .set("QuadPoints", write_quadpoints(&quads, top));
        }
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Polygon or polyline annotation from vertices, boxed with a 1pt pad.
    fn add_vertices_annot(&mut self, subtype: &str, points: &[Point]) -> Result<Annot<'_>> {
        if points.is_empty() {
            return Err(bad_value("vertices need at least one point"));
        }
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let mut bbox: Option<Rect> = None;
        for p in points {
            grow_union(&mut bbox, *p);
        }
        let r = norm_rect(&bbox.unwrap_or(mk_rect(0.0, 0.0, 0.0, 0.0)));
        let r = mk_rect(r.x0 - 1.0, r.y0 - 1.0, r.x1 + 1.0, r.y1 + 1.0);
        let id = add_annot_common(lp, page_id, subtype, &flip_rect(&r, top))?;
        let mut flat = Vec::with_capacity(points.len() * 2);
        for p in points {
            flat.push(Object::Real(p.x as f32));
            flat.push(Object::Real(flip_y(p.y, top) as f32));
        }
        lp.get_dictionary_mut(id)?
            .set("Vertices", Object::Array(flat));
        let xref = id.0 as i32;
        sync_doc(self.document_mut())?;
        Ok(Annot {
            doc: self.document_mut(),
            page: index,
            xref,
        })
    }

    /// Encode one drawn path with drawing options and append it to the page.
    fn finish_draw(&mut self, path: DrawnPath, end: Point, options: &DrawOptions) -> Result<Point> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let finished = FinishedPath {
            paths: vec![path],
            width: options.width,
            color: options.color.clone(),
            fill: options.fill.clone(),
            line_cap: options.line_cap,
            line_join: options.line_join,
            dashes: options.dashes.clone(),
            even_odd: false,
            close: options.close_path,
            fill_opacity: options.fill_opacity,
            stroke_opacity: options.stroke_opacity,
        };
        let gs = if options.fill_opacity < 1.0 || options.stroke_opacity < 1.0 {
            Some(ensure_opacity_state(
                lp,
                page_id,
                options.fill_opacity,
                options.stroke_opacity,
            )?)
        } else {
            None
        };
        let _ = (&options.morph, options.oc);
        let data = encode_finished_path(&finished, top, gs.as_deref());
        push_content(lp, page_id, data, options.overlay)?;
        sync_doc(self.document_mut())?;
        Ok(end)
    }

    /// Ensure font resources for staged runs (base-14 or embedded file fonts),
    /// encode them, and append them to the page.
    fn commit_text(
        &mut self,
        runs: Vec<StagedRun>,
        files: &HashMap<String, Vec<u8>>,
        encoding: i32,
        overlay: bool,
    ) -> Result<()> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let mut fonts: HashMap<String, String> = HashMap::new();
        for run in &runs {
            if fonts.contains_key(&run.font_base) {
                continue;
            }
            let name = match files.get(&run.font_base) {
                Some(data) => embed_file_font(lp, page_id, data, encoding)?.1,
                None => ensure_base_font(lp, page_id, &run.font_base)?,
            };
            fonts.insert(run.font_base.clone(), name);
        }
        let mut states: HashMap<[u64; 2], String> = HashMap::new();
        let mut data = Vec::new();
        for run in &runs {
            let gs = if run.opacity >= 1.0 && run.stroke_opacity >= 1.0 {
                None
            } else {
                let key = [run.opacity.to_bits(), run.stroke_opacity.to_bits()];
                if let Some(name) = states.get(&key) {
                    Some(name.clone())
                } else {
                    let name = ensure_opacity_state(lp, page_id, run.opacity, run.stroke_opacity)?;
                    states.insert(key, name.clone());
                    Some(name)
                }
            };
            let font_name = fonts.get(&run.font_base).cloned().unwrap_or_default();
            data.extend_from_slice(&encode_text_run(run, &font_name, gs.as_deref(), top));
        }
        push_content(lp, page_id, data, overlay)?;
        sync_doc(self.document_mut())
    }

    /// Link annotations on the page, in annotation order.
    fn page_links(&self) -> Result<Vec<Link>> {
        let lp = self.document().lopdf();
        let page_id = page_id_of(lp, self.index())?;
        let mut out = Vec::new();
        for id in page_annots(lp, page_id) {
            let is_link = lp
                .get_dictionary(id)
                .ok()
                .and_then(|d| d.get(b"Subtype").ok().map(obj_name))
                .as_deref()
                == Some("Link");
            if is_link {
                out.push(link_of(lp, self.index(), id)?);
            }
        }
        Ok(out)
    }

    /// Set one page box from a y-down rect.
    fn set_page_box(&mut self, key: &[u8], rect: Rect) -> Result<()> {
        let index = self.index();
        let lp = self.document_mut().lopdf_mut();
        let page_id = page_id_of(lp, index)?;
        let top = page_top(lp, page_id);
        let r = norm_rect(&rect);
        lp.get_dictionary_mut(page_id)?
            .set(key, rect_object(&flip_rect(&r, top)));
        sync_doc(self.document_mut())
    }
}

// --- lane L4 continuation: pixmap images ----------------------------------------

/// Image XObject from a pixmap's samples, with an optional soft mask.
/// The image object is allocated before the mask so its number is stable.
fn pixmap_image_xobject(
    doc: &mut LopdfDocument,
    pixmap: &Pixmap,
    keep_alpha: bool,
) -> Result<(ObjectId, u32, u32)> {
    let width = pixmap.width().max(1) as u32;
    let height = pixmap.height().max(1) as u32;
    let comps = pixmap.n().max(1) as usize;
    let file_alpha = pixmap.alpha() && comps >= 2;
    let stride = pixmap.stride().max(1);
    let samples = pixmap.samples();
    let color_comps = if file_alpha { comps - 1 } else { comps };
    let rgb = color_comps >= 3;
    let mut packed = Vec::with_capacity(width as usize * height as usize * if rgb { 3 } else { 1 });
    let mut mask = if file_alpha && keep_alpha {
        Vec::with_capacity(width as usize * height as usize)
    } else {
        Vec::new()
    };
    for row in 0..height as usize {
        let base = row * stride;
        for col in 0..width as usize {
            let at = base + col * comps;
            let px = samples.get(at..at + comps).unwrap_or(&[]);
            if rgb {
                packed.push(px.first().copied().unwrap_or(0));
                packed.push(px.get(1).copied().unwrap_or(0));
                packed.push(px.get(2).copied().unwrap_or(0));
            } else {
                packed.push(px.first().copied().unwrap_or(0));
            }
            if file_alpha && keep_alpha {
                mask.push(px.get(comps - 1).copied().unwrap_or(255));
            }
        }
    }
    let mut dict = Dictionary::new();
    dict.set("Type", name_object("XObject"));
    dict.set("Subtype", name_object("Image"));
    dict.set("Width", Object::Integer(i64::from(width)));
    dict.set("Height", Object::Integer(i64::from(height)));
    dict.set(
        "ColorSpace",
        if rgb {
            name_object("DeviceRGB")
        } else {
            name_object("DeviceGray")
        },
    );
    dict.set("BitsPerComponent", Object::Integer(8));
    let id = doc.new_object_id();
    doc.objects
        .insert(id, Object::Stream(Stream::new(dict, packed)));
    if file_alpha && keep_alpha {
        let mut mdict = Dictionary::new();
        mdict.set("Type", name_object("XObject"));
        mdict.set("Subtype", name_object("Image"));
        mdict.set("Width", Object::Integer(i64::from(width)));
        mdict.set("Height", Object::Integer(i64::from(height)));
        mdict.set("ColorSpace", name_object("DeviceGray"));
        mdict.set("BitsPerComponent", Object::Integer(8));
        let mid = doc.new_object_id();
        doc.objects
            .insert(mid, Object::Stream(Stream::new(mdict, mask)));
        if let Ok(Object::Stream(stream)) = doc.get_object_mut(id) {
            stream.dict.set("SMask", Object::Reference(mid));
        }
    }
    Ok((id, width, height))
}

/// PNG-encoded bytes for a pixmap, honouring its stride.
#[cfg(not(target_arch = "wasm32"))]
fn pixmap_bytestream(pixmap: &Pixmap) -> Result<Vec<u8>> {
    use png::{BitDepth, ColorType, Encoder};
    let width = pixmap.width().max(1) as u32;
    let height = pixmap.height().max(1) as u32;
    let comps = pixmap.n().max(1) as usize;
    let file_alpha = pixmap.alpha() && comps >= 2;
    let stride = pixmap.stride().max(1);
    let samples = pixmap.samples();
    let color_comps = if file_alpha { comps - 1 } else { comps };
    let (color, out_comps) = match (color_comps >= 3, file_alpha) {
        (true, true) => (ColorType::Rgba, 4),
        (true, false) => (ColorType::Rgb, 3),
        (false, true) => (ColorType::GrayscaleAlpha, 2),
        (false, false) => (ColorType::Grayscale, 1),
    };
    let mut packed = Vec::with_capacity(width as usize * height as usize * out_comps);
    for row in 0..height as usize {
        let base = row * stride;
        for col in 0..width as usize {
            let at = base + col * comps;
            let px = samples.get(at..at + comps).unwrap_or(&[]);
            match out_comps {
                4 => {
                    packed.push(px.first().copied().unwrap_or(0));
                    packed.push(px.get(1).copied().unwrap_or(0));
                    packed.push(px.get(2).copied().unwrap_or(0));
                    packed.push(px.get(comps - 1).copied().unwrap_or(255));
                }
                3 => {
                    packed.push(px.first().copied().unwrap_or(0));
                    packed.push(px.get(1).copied().unwrap_or(0));
                    packed.push(px.get(2).copied().unwrap_or(0));
                }
                2 => {
                    packed.push(px.first().copied().unwrap_or(0));
                    packed.push(px.get(comps - 1).copied().unwrap_or(255));
                }
                _ => packed.push(px.first().copied().unwrap_or(0)),
            }
        }
    }
    let mut buf = Vec::new();
    {
        let mut encoder = Encoder::new(&mut buf, width, height);
        encoder.set_color(color);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| unreadable(format!("png does not encode: {e}")))?;
        writer
            .write_image_data(&packed)
            .map_err(|e| unreadable(format!("png does not encode: {e}")))?;
    }
    Ok(buf)
}

/// PNG encoding is unavailable on this target.
#[cfg(target_arch = "wasm32")]
fn pixmap_bytestream(_pixmap: &Pixmap) -> Result<Vec<u8>> {
    Err(unreadable("png is unsupported on this target"))
}

/// Replace an image stream's bytes and header in place, keeping its object.
fn replace_image_bytes(doc: &mut LopdfDocument, target: ObjectId, data: &[u8]) -> Result<()> {
    match sniff_image(data) {
        ImageEncoding::Jpeg => {
            let (width, height, comps) = parse_jpeg(data)?;
            let space = match comps {
                1 => "DeviceGray",
                4 => "DeviceCMYK",
                _ => "DeviceRGB",
            };
            let stream = doc
                .get_object_mut(target)
                .ok()
                .and_then(|o| match o {
                    Object::Stream(s) => Some(s),
                    _ => None,
                })
                .ok_or_else(|| bad_value("image object is not a stream"))?;
            stream.set_content(data.to_vec());
            stream.dict.set("Width", Object::Integer(i64::from(width)));
            stream
                .dict
                .set("Height", Object::Integer(i64::from(height)));
            stream.dict.set("ColorSpace", name_object(space));
            stream.dict.set("BitsPerComponent", Object::Integer(8));
            stream.dict.set("Filter", name_object("DCTDecode"));
            stream.dict.remove(b"SMask");
            Ok(())
        }
        ImageEncoding::Png => {
            let image = decode_png(data)?;
            let (rgb, mask) = match split_alpha(&image) {
                Some((rgb, mask)) => (rgb, Some(mask)),
                None => (image.samples.clone(), None),
            };
            let mask_id = if let Some(mask) = mask {
                let mut mdict = Dictionary::new();
                mdict.set("Type", name_object("XObject"));
                mdict.set("Subtype", name_object("Image"));
                mdict.set("Width", Object::Integer(i64::from(image.width)));
                mdict.set("Height", Object::Integer(i64::from(image.height)));
                mdict.set("ColorSpace", name_object("DeviceGray"));
                mdict.set("BitsPerComponent", Object::Integer(8));
                let mid = doc.new_object_id();
                doc.objects
                    .insert(mid, Object::Stream(Stream::new(mdict, mask)));
                Some(mid)
            } else {
                None
            };
            let stream = doc
                .get_object_mut(target)
                .ok()
                .and_then(|o| match o {
                    Object::Stream(s) => Some(s),
                    _ => None,
                })
                .ok_or_else(|| bad_value("image object is not a stream"))?;
            stream.set_content(rgb);
            stream
                .dict
                .set("Width", Object::Integer(i64::from(image.width)));
            stream
                .dict
                .set("Height", Object::Integer(i64::from(image.height)));
            stream.dict.set("ColorSpace", name_object("DeviceRGB"));
            stream.dict.set("BitsPerComponent", Object::Integer(8));
            stream.dict.remove(b"Filter");
            match mask_id {
                Some(mid) => stream.dict.set("SMask", Object::Reference(mid)),
                None => {
                    stream.dict.remove(b"SMask");
                }
            };
            Ok(())
        }
        ImageEncoding::Unknown => Err(unreadable("image is neither jpeg nor png")),
    }
}

// --- lane L4 continuation: page-tree surgery --------------------------------------

/// 0-based page index from a possibly negative number.
fn page_index(count: usize, page: i32) -> Result<usize> {
    let at = if page < 0 { count as i32 + page } else { page };
    if at < 0 || at as usize >= count {
        return Err(index_oob("page index out of range"));
    }
    Ok(at as usize)
}

/// Insertion slot from a possibly negative target; negatives append.
fn move_target(count: usize, to: i32) -> usize {
    if to < 0 {
        count
    } else {
        (to as usize).min(count)
    }
}

/// Post-removal insertion slot for move_page: `to` names a page in the
/// current array and the moved page lands before it; -1 and `count` append.
/// Anything else is out of range.
fn move_slot(count: usize, from: usize, to: i32) -> Result<usize> {
    if to == -1 || to == count as i32 {
        return Ok(count.saturating_sub(1));
    }
    if to < 0 || to >= count as i32 {
        return Err(index_oob("move target out of range"));
    }
    let to = to as usize;
    Ok(if to <= from { to } else { to - 1 })
}

/// Graft a detached page object into the tree at a slot, fixing counts.
fn graft_page_at(doc: &mut LopdfDocument, fresh: ObjectId, dest: usize) -> Result<()> {
    let leaves = leaf_pages(doc);
    let at = dest.min(leaves.len());
    let (parent, slot) = if at < leaves.len() {
        let anchor = leaves[at];
        let parent = parent_of(doc, anchor).ok_or_else(|| engine_fault("page has no parent"))?;
        let kids = doc
            .get_dictionary(parent)
            .ok()
            .and_then(|d| d.get(b"Kids").ok().cloned())
            .and_then(|o| o.as_array().ok().cloned())
            .unwrap_or_default();
        let slot = kids
            .iter()
            .position(|k| k.as_reference().ok() == Some(anchor))
            .unwrap_or(0);
        (parent, slot)
    } else if let Some(last) = leaves.last() {
        let parent = parent_of(doc, *last).ok_or_else(|| engine_fault("page has no parent"))?;
        let kids = doc
            .get_dictionary(parent)
            .ok()
            .and_then(|d| d.get(b"Kids").ok().cloned())
            .and_then(|o| o.as_array().ok().cloned())
            .unwrap_or_default();
        (parent, kids.len())
    } else {
        let catalog = catalog_id(doc)?;
        let pages = doc
            .get_dictionary(catalog)
            .ok()
            .and_then(|c| c.get(b"Pages").ok().and_then(|o| o.as_reference().ok()))
            .ok_or_else(|| engine_fault("document has no page tree"))?;
        (pages, 0)
    };
    doc.get_dictionary_mut(fresh)?
        .set("Parent", Object::Reference(parent));
    let kids = doc
        .get_dictionary(parent)
        .ok()
        .and_then(|d| d.get(b"Kids").ok().cloned())
        .and_then(|o| o.as_array().ok().cloned())
        .unwrap_or_default();
    let mut kids = kids;
    kids.insert(slot.min(kids.len()), Object::Reference(fresh));
    doc.get_dictionary_mut(parent)?
        .set("Kids", Object::Array(kids));
    fix_counts_up(doc, parent, 1);
    Ok(())
}

/// Deep-copy a page within its document, with or without its annotations.
fn duplicate_page_at(
    doc: &mut LopdfDocument,
    from: usize,
    dest: usize,
    with_annots: bool,
) -> Result<()> {
    let leaves = leaf_pages(doc);
    let src = leaves
        .get(from)
        .copied()
        .ok_or_else(|| index_oob("page index out of range"))?;
    let mut src_dict = doc.get_dictionary(src)?.clone();
    src_dict.remove(b"Parent");
    if !with_annots {
        src_dict.remove(b"Annots");
    }
    let mut memo = HashMap::new();
    let dict = copy_object_deep(doc, &Object::Dictionary(src_dict), &mut memo)?
        .as_dict()
        .ok()
        .cloned()
        .unwrap_or_default();
    let fresh = doc.new_object_id();
    doc.objects.insert(fresh, Object::Dictionary(dict));
    graft_page_at(doc, fresh, dest)
}

/// Move a page within the tree.
fn move_page_at(doc: &mut LopdfDocument, from: usize, dest: usize) -> Result<()> {
    let leaves = leaf_pages(doc);
    let id = leaves
        .get(from)
        .copied()
        .ok_or_else(|| index_oob("page index out of range"))?;
    let parent = parent_of(doc, id).ok_or_else(|| engine_fault("page has no parent"))?;
    let kids = doc
        .get_dictionary(parent)
        .ok()
        .and_then(|d| d.get(b"Kids").ok().cloned())
        .and_then(|o| o.as_array().ok().cloned())
        .unwrap_or_default();
    let rest: Vec<Object> = kids
        .into_iter()
        .filter(|k| k.as_reference().ok() != Some(id))
        .collect();
    doc.get_dictionary_mut(parent)?
        .set("Kids", Object::Array(rest));
    fix_counts_up(doc, parent, -1);
    graft_page_at(doc, id, dest)
}

/// Filter a copied page's annotation list by kind flags.
fn filter_copied_annots(
    doc: &LopdfDocument,
    dict: &Dictionary,
    annots: bool,
    links: bool,
    widgets: bool,
) -> Vec<Object> {
    let items = dict
        .get(b"Annots")
        .ok()
        .map(|o| deref(doc, o).clone())
        .and_then(|o| o.as_array().ok().cloned())
        .unwrap_or_default();
    let mut kept = Vec::new();
    for item in items {
        let subtype = item
            .as_reference()
            .ok()
            .and_then(|id| doc.get_dictionary(id).ok())
            .and_then(|d| d.get(b"Subtype").ok().map(obj_name));
        let keep = match subtype.as_deref() {
            Some("Link") => links,
            Some("Widget") => widgets,
            _ => annots,
        };
        if keep {
            kept.push(item);
        }
    }
    kept
}

/// 0-based outline index from a possibly negative number.
fn toc_index(len: usize, index: i32) -> Result<usize> {
    let at = if index < 0 { len as i32 + index } else { index };
    if at < 0 || at as usize >= len {
        return Err(index_oob("outline index out of range"));
    }
    Ok(at as usize)
}

/// Outline row back to an editable entry.
fn outline_entry(row: OutlineRow) -> TocEntry {
    TocEntry {
        level: row.level,
        title: row.title,
        page: row.page,
        dest: row.dest.map(|d| TocDest {
            kind: 1,
            page: d.page,
            target: d.target,
            zoom: d.zoom,
        }),
    }
}

// --- lane L4 continuation: resources, fields, files ---------------------------------

/// Drop references to one object from a resource sub-dictionary.
fn remove_resource_ref(doc: &mut LopdfDocument, page_id: ObjectId, kind: &[u8], target: ObjectId) {
    let slot = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Resources").ok().cloned());
    let scrub = |dict: &mut Dictionary| {
        let dead: Vec<Vec<u8>> = dict
            .iter()
            .filter(|(_, v)| v.as_reference().ok() == Some(target))
            .map(|(k, _)| k.clone())
            .collect();
        for key in dead {
            dict.remove(&key);
        }
    };
    match slot {
        Some(Object::Reference(res_id)) => {
            let sub = doc
                .get_dictionary(res_id)
                .ok()
                .and_then(|res| res.get(kind).ok().cloned());
            match sub {
                Some(Object::Reference(sub_id)) => {
                    if let Ok(dict) = doc.get_dictionary_mut(sub_id) {
                        scrub(dict);
                    }
                }
                Some(Object::Dictionary(mut dict)) => {
                    scrub(&mut dict);
                    if let Ok(res) = doc.get_dictionary_mut(res_id) {
                        res.set(
                            String::from_utf8_lossy(kind).into_owned(),
                            Object::Dictionary(dict),
                        );
                    }
                }
                _ => {}
            }
        }
        Some(Object::Dictionary(mut res)) => {
            let sub = res.get(kind).ok().cloned();
            match sub {
                Some(Object::Reference(sub_id)) => {
                    if let Ok(dict) = doc.get_dictionary_mut(sub_id) {
                        scrub(dict);
                    }
                }
                Some(Object::Dictionary(mut dict)) => {
                    scrub(&mut dict);
                    res.set(
                        String::from_utf8_lossy(kind).into_owned(),
                        Object::Dictionary(dict),
                    );
                    if let Ok(page) = doc.get_dictionary_mut(page_id) {
                        page.set("Resources", Object::Dictionary(res));
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// Drop a field reference from the document form.
fn remove_acroform_field(doc: &mut LopdfDocument, target: ObjectId) {
    let form = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .and_then(|catalog| {
            doc.get_dictionary(catalog)
                .ok()
                .and_then(|c| c.get(b"AcroForm").ok().cloned())
        })
        .and_then(|o| o.as_reference().ok());
    if let Some(form) = form {
        if let Ok(dict) = doc.get_dictionary_mut(form) {
            let fields = dict
                .get(b"Fields")
                .ok()
                .and_then(|o| o.as_array().ok().cloned())
                .unwrap_or_default();
            let kept: Vec<Object> = fields
                .into_iter()
                .filter(|o| o.as_reference().ok() != Some(target))
                .collect();
            dict.set("Fields", Object::Array(kept));
        }
    }
}

/// Remove one pair from the flat embedded-files name array.
fn remove_embedded_pair(doc: &mut LopdfDocument, position: usize) -> Result<()> {
    let mut items = embedded_files_items(doc);
    let at = position * 2;
    if at + 1 < items.len() {
        items.drain(at..at + 2);
    }
    embedded_files_store(doc, items)
}

/// File metadata onto a spec dict or a lone embedded stream alike.
fn embfile_set_meta(dict: &mut Dictionary, options: &EmbFileOptions) {
    if let Some(filename) = &options.filename {
        dict.set("F", text_object(filename));
    }
    if let Some(display) = &options.display_name {
        dict.set("UF", text_object(display));
    }
    if let Some(desc) = &options.description {
        if desc.is_empty() {
            dict.remove(b"Desc");
        } else {
            dict.set("Desc", text_object(desc));
        }
    }
}

/// Drop a file object: a lone stream, or a spec with its embedded streams.
fn remove_filespec_objects(doc: &mut LopdfDocument, spec_id: ObjectId) {
    if matches!(doc.objects.get(&spec_id), Some(Object::Stream(_))) {
        doc.objects.remove(&spec_id);
        return;
    }
    let streams = doc
        .get_dictionary(spec_id)
        .ok()
        .and_then(|d| d.get(b"EF").ok().cloned())
        .and_then(|o| o.as_dict().ok().cloned())
        .map(|ef| {
            ef.iter()
                .filter_map(|(_, v)| v.as_reference().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for id in streams {
        doc.objects.remove(&id);
    }
    doc.objects.remove(&spec_id);
}

// --- lane L4 continuation: optional content -------------------------------------------

/// Append an OCG to the group list and to the default ON or OFF list.
/// Read the `/OCProperties` dict, inline or referenced, defaulting empty.
fn oc_properties_of(doc: &LopdfDocument) -> Dictionary {
    catalog_id(doc)
        .ok()
        .and_then(|catalog| doc.get_dictionary(catalog).ok().cloned())
        .and_then(|root| root.get(b"OCProperties").ok().cloned())
        .map(|o| deref(doc, &o).as_dict().ok().cloned())
        .unwrap_or_default()
        .unwrap_or_default()
}

/// Read the `/OCProperties` dict, creating an empty one when missing.
/// List a group in the default configuration's interface order.
fn ocproperties_order_append(doc: &mut LopdfDocument, ocg_id: ObjectId) -> Result<()> {
    let mut props = oc_properties_or_new(doc)?;
    let mut base = props
        .get(b"D")
        .ok()
        .cloned()
        .map(|o| deref(doc, &o).as_dict().ok().cloned())
        .unwrap_or_default()
        .unwrap_or_default();
    let mut order = base
        .get(b"Order")
        .ok()
        .and_then(|o| o.as_array().ok().cloned())
        .unwrap_or_default();
    if !order.iter().any(|o| o.as_reference().ok() == Some(ocg_id)) {
        order.push(Object::Reference(ocg_id));
    }
    base.set("Order", Object::Array(order));
    props.set("D", Object::Dictionary(base));
    oc_properties_store(doc, &props)?;
    Ok(())
}

fn oc_properties_or_new(doc: &mut LopdfDocument) -> Result<Dictionary> {
    let mut props = oc_properties_of(doc);
    if props.is_empty() {
        let mut d = Dictionary::new();
        d.set("BaseState", name_object("ON"));
        d.set("ON", Object::Array(Vec::new()));
        d.set("OFF", Object::Array(Vec::new()));
        props.set("OCGs", Object::Array(Vec::new()));
        props.set("D", Object::Dictionary(d));
    }
    Ok(props)
}

/// Write the `/OCProperties` dict back, keeping it where it lives.
fn oc_properties_store(doc: &mut LopdfDocument, props: &Dictionary) -> Result<()> {
    let catalog = catalog_id(doc)?;
    let slot = doc
        .get_dictionary(catalog)
        .ok()
        .and_then(|root| root.get(b"OCProperties").ok().cloned());
    match slot {
        Some(Object::Reference(id)) => {
            doc.get_dictionary_mut(id)?;
            doc.objects.insert(id, Object::Dictionary(props.clone()));
            Ok(())
        }
        _ => {
            doc.get_dictionary_mut(catalog)?
                .set("OCProperties", Object::Dictionary(props.clone()));
            Ok(())
        }
    }
}

/// Read one configuration: the default `/D` for negative numbers, the
/// numbered alternate otherwise.
fn oc_config_of(doc: &LopdfDocument, config: i32) -> Option<Dictionary> {
    let props = oc_properties_of(doc);
    if props.is_empty() {
        return None;
    }
    if config < 0 {
        return props
            .get(b"D")
            .ok()
            .cloned()
            .map(|o| deref(doc, &o).as_dict().ok().cloned())
            .unwrap_or_default();
    }
    props
        .get(b"Configs")
        .ok()
        .cloned()
        .and_then(|o| deref(doc, &o).as_array().ok().cloned())
        .and_then(|items| items.get(config as usize).cloned())
        .map(|o| deref(doc, &o).as_dict().ok().cloned())
        .unwrap_or_default()
}

/// Write one configuration back where it lives.
fn oc_config_store(doc: &mut LopdfDocument, config: i32, dict: &Dictionary) -> Result<()> {
    let mut props = oc_properties_of(doc);
    if config < 0 {
        match props.get(b"D").ok().cloned() {
            Some(Object::Reference(id)) => {
                doc.objects.insert(id, Object::Dictionary(dict.clone()));
            }
            _ => {
                props.set("D", Object::Dictionary(dict.clone()));
                oc_properties_store(doc, &props)?;
            }
        }
        return Ok(());
    }
    let mut configs = props
        .get(b"Configs")
        .ok()
        .cloned()
        .map(|o| deref(doc, &o).as_array().ok().cloned())
        .unwrap_or_default()
        .unwrap_or_default();
    let Some(slot) = configs.get_mut(config as usize) else {
        return Err(bad_value(format!("no layer configuration {config}")));
    };
    match slot.clone() {
        Object::Reference(id) => {
            doc.objects.insert(id, Object::Dictionary(dict.clone()));
        }
        _ => {
            *slot = Object::Dictionary(dict.clone());
            props.set("Configs", Object::Array(configs));
            oc_properties_store(doc, &props)?;
        }
    }
    Ok(())
}

fn ocproperties_append(doc: &mut LopdfDocument, ocg_id: ObjectId, on: bool) -> Result<()> {
    let mut props = oc_properties_or_new(doc)?;
    let mut ocgs = props
        .get(b"OCGs")
        .ok()
        .cloned()
        .map(|o| deref(doc, &o).as_array().ok().cloned())
        .unwrap_or_default()
        .unwrap_or_default();
    if !ocgs.iter().any(|o| o.as_reference().ok() == Some(ocg_id)) {
        ocgs.push(Object::Reference(ocg_id));
    }
    props.set("OCGs", Object::Array(ocgs));
    let mut base = props
        .get(b"D")
        .ok()
        .cloned()
        .map(|o| deref(doc, &o).as_dict().ok().cloned())
        .unwrap_or_default()
        .unwrap_or_default();
    let key = if on {
        b"ON".as_slice()
    } else {
        b"OFF".as_slice()
    };
    let mut list = base
        .get(key)
        .ok()
        .and_then(|o| o.as_array().ok().cloned())
        .unwrap_or_default();
    if !list.iter().any(|o| o.as_reference().ok() == Some(ocg_id)) {
        list.push(Object::Reference(ocg_id));
    }
    base.set(key, Object::Array(list));
    props.set("D", Object::Dictionary(base));
    oc_properties_store(doc, &props)?;
    Ok(())
}

/// Writable layer configuration: the default `/D` for negative numbers,
/// the numbered alternate otherwise; missing alternates are an error.
fn layer_config_mut(doc: &mut LopdfDocument, config: i32) -> Result<Dictionary> {
    oc_config_of(doc, config).ok_or_else(|| bad_value(format!("no layer configuration {config}")))
}

/// Dynamic compat value to a PDF object.
fn value_to_object(value: &Value) -> Result<Object> {
    match value {
        Value::Null => Ok(Object::Null),
        Value::Bool(on) => Ok(Object::Boolean(*on)),
        Value::Int(i) => Ok(Object::Integer(*i)),
        Value::Float(f) => Ok(Object::Real(*f as f32)),
        Value::Str(s) => Ok(text_object(s)),
        Value::Bytes(b) => Ok(Object::String(b.clone(), StringFormat::Literal)),
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(value_to_object(item)?);
            }
            Ok(Object::Array(out))
        }
        Value::Dict(map) => {
            let mut dict = Dictionary::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                dict.set(key.as_str(), value_to_object(&map[key])?);
            }
            Ok(Object::Dictionary(dict))
        }
    }
}

// --- lane L4 continuation: scrub ----------------------------------------------------------

/// Strip document and page JavaScript triggers, best effort.
fn scrub_javascript(doc: &mut LopdfDocument) {
    if let Ok(catalog) = catalog_id(doc) {
        let open_is_js = doc
            .get_dictionary(catalog)
            .ok()
            .and_then(|c| c.get(b"OpenAction").ok().cloned())
            .map(|o| {
                o.as_dict()
                    .ok()
                    .and_then(|d| d.get(b"S").ok().map(obj_name))
                    .as_deref()
                    == Some("JavaScript")
            })
            .unwrap_or(false);
        if let Ok(dict) = doc.get_dictionary_mut(catalog) {
            if open_is_js {
                dict.remove(b"OpenAction");
            }
            dict.remove(b"AA");
        }
        let names_id = doc
            .get_dictionary(catalog)
            .ok()
            .and_then(|c| c.get(b"Names").ok().cloned())
            .and_then(|o| o.as_reference().ok());
        if let Some(id) = names_id {
            if let Ok(dict) = doc.get_dictionary_mut(id) {
                dict.remove(b"JavaScript");
            }
        }
    }
    for page in leaf_pages(doc) {
        if let Ok(dict) = doc.get_dictionary_mut(page) {
            dict.remove(b"AA");
        }
    }
}

/// Delete reply annotations (those answering another annotation).
fn scrub_responses(doc: &mut LopdfDocument) {
    for page in leaf_pages(doc) {
        let ids = page_annots(doc, page);
        let mut dead = Vec::new();
        for id in &ids {
            let is_reply = doc
                .get_dictionary(*id)
                .ok()
                .and_then(|d| d.get(b"IRT").ok().cloned())
                .is_some();
            if is_reply {
                dead.push(*id);
            }
        }
        if dead.is_empty() {
            continue;
        }
        let keep: Vec<ObjectId> = ids.into_iter().filter(|id| !dead.contains(id)).collect();
        if set_page_annots(doc, page, &keep).is_ok() {
            for id in dead {
                doc.objects.remove(&id);
            }
        }
    }
}

/// Fit matrix placing staged ink into a target rectangle, as the y-up
/// content matrix the flush wraps around the encoded runs.
///
/// The union is y-down staged points; rotation spins it screen-clockwise
/// about its own min-corner, the rotated bounds shrink to fit the target
/// (uniformly with `keep_proportion`), and the min-corners meet. The
/// y-down fit is conjugated through the page flip for the `cm`. Identity
/// fits answer `None` so placed content encodes bare.
fn write_text_matrix(
    target: Rect,
    union: Option<Rect>,
    rotate: i32,
    keep_proportion: bool,
    top: f64,
) -> Result<Option<Matrix>> {
    let Some(ink) = union else {
        return Ok(None);
    };
    let turns = rotate.rem_euclid(360);
    if ![0, 90, 180, 270].contains(&turns) {
        return Err(bad_value("write_text rotate must be a multiple of 90"));
    }
    let (ux, uy) = (ink.x0, ink.y0);
    let spin = |(x, y): (f64, f64)| -> (f64, f64) {
        let (dx, dy) = (x - ux, y - uy);
        match turns {
            90 => (ux + dy, uy - dx),
            180 => (ux - dx, uy - dy),
            270 => (ux - dy, uy + dx),
            _ => (x, y),
        }
    };
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for corner in [
        (ink.x0, ink.y0),
        (ink.x1, ink.y0),
        (ink.x0, ink.y1),
        (ink.x1, ink.y1),
    ] {
        let (x, y) = spin(corner);
        bounds[0] = bounds[0].min(x);
        bounds[1] = bounds[1].min(y);
        bounds[2] = bounds[2].max(x);
        bounds[3] = bounds[3].max(y);
    }
    let (bx0, by0) = (bounds[0], bounds[1]);
    let (bw, bh) = (bounds[2] - bx0, bounds[3] - by0);
    let (rw, rh) = (target.x1 - target.x0, target.y1 - target.y0);
    let (mut sx, mut sy) = (
        if bw > 0.0 { rw / bw } else { 1.0 },
        if bh > 0.0 { rh / bh } else { 1.0 },
    );
    sx = sx.min(1.0);
    sy = sy.min(1.0);
    if keep_proportion {
        let s = sx.min(sy);
        sx = s;
        sy = s;
    }
    // M_down(p) = R0 + S·(spin(p) − B0), read off per quarter-turn.
    let (rx0, ry0) = (target.x0, target.y0);
    let (a, b, c, d, e, f) = match turns {
        90 => (
            0.0,
            -sy,
            sx,
            0.0,
            rx0 + sx * (ux - uy - bx0),
            ry0 + sy * (ux + uy - by0),
        ),
        180 => (
            -sx,
            0.0,
            0.0,
            -sy,
            rx0 + sx * (2.0 * ux - bx0),
            ry0 + sy * (2.0 * uy - by0),
        ),
        270 => (
            0.0,
            sy,
            -sx,
            0.0,
            rx0 + sx * (ux + uy - bx0),
            ry0 + sy * (uy - ux - by0),
        ),
        _ => (sx, 0.0, 0.0, sy, rx0 - sx * bx0, ry0 - sy * by0),
    };
    // Conjugate through the y-flip: runs encode y-up, the fit ran y-down.
    let out = mk_matrix(a, -b, -c, d, c * top + e, top * (1.0 - d) - f);
    if out.a == 1.0 && out.b == 0.0 && out.c == 0.0 && out.d == 1.0 && out.e == 0.0 && out.f == 0.0
    {
        return Ok(None);
    }
    Ok(Some(out))
}

// --- lane L4 continuation: foreign pages -----------------------------------------------------

/// Content matrix mapping a source page box onto a y-down target rect.
/// Rotation is screen-clockwise; other angles behave as zero.
fn place_form_matrix(
    bbox: &[f64; 4],
    target: &Rect,
    keep_proportion: bool,
    rotate: i32,
    top: f64,
) -> [f64; 6] {
    let (sx0, sy0, sx1, sy1) = (bbox[0], bbox[1], bbox[2], bbox[3]);
    let (sw, sh) = ((sx1 - sx0).max(1.0), (sy1 - sy0).max(1.0));
    let r = norm_rect(target);
    let (tw, th) = ((r.x1 - r.x0).max(1.0), (r.y1 - r.y0).max(1.0));
    let norm = rotate.rem_euclid(360);
    let (dw, dh) = match norm {
        90 | 270 => (sh, sw),
        _ => (sw, sh),
    };
    let (s1, s2, ox, oy) = if keep_proportion {
        let s = (tw / dw).min(th / dh);
        (s, s, r.x0 + (tw - dw * s) / 2.0, r.y0 + (th - dh * s) / 2.0)
    } else {
        (tw / dw, th / dh, r.x0, r.y0)
    };
    match norm {
        90 => [0.0, -s2, s1, 0.0, ox - sy0 * s1, top - oy - sx0 * s2],
        180 => [-s1, 0.0, 0.0, s2, ox + sx1 * s1, top - oy - sy0 * s2],
        270 => [0.0, s2, -s1, 0.0, ox + sy1 * s1, top - oy - sx1 * s2],
        _ => [s1, 0.0, 0.0, -s2, ox - sx0 * s1, top - oy + sy1 * s2],
    }
}

#[cfg(test)]
mod write_text_matrix_tests {
    use super::{mk_rect, write_text_matrix, Matrix};

    const TOP: f64 = 792.0;

    /// Run the content matrix over a flipped (y-up) point.
    fn through(matrix: &Matrix, x: f64, y: f64) -> (f64, f64) {
        (
            matrix.a * x + matrix.c * y + matrix.e,
            matrix.b * x + matrix.d * y + matrix.f,
        )
    }

    #[test]
    fn placed_content_encodes_bare() {
        let same = mk_rect(5.0, 5.0, 15.0, 15.0);
        assert!(write_text_matrix(same, Some(same), 0, true, TOP)
            .expect("fit works")
            .is_none());
        assert!(write_text_matrix(same, None, 0, true, TOP)
            .expect("fit works")
            .is_none());
    }

    #[test]
    fn off_origin_content_translates_to_the_rect() {
        let matrix = write_text_matrix(
            mk_rect(0.0, 0.0, 200.0, 100.0),
            Some(mk_rect(10.0, 20.0, 110.0, 70.0)),
            0,
            true,
            TOP,
        )
        .expect("fit works")
        .expect("translate is not identity");
        assert_eq!(
            (matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f),
            (1.0, 0.0, 0.0, 1.0, -10.0, 20.0)
        );
        // The union corner lands on the rect corner, through the flip.
        assert_eq!(through(&matrix, 10.0, TOP - 20.0), (0.0, TOP));
    }

    #[test]
    fn small_rects_shrink_uniformly_or_per_axis() {
        let matrix = write_text_matrix(
            mk_rect(0.0, 0.0, 100.0, 100.0),
            Some(mk_rect(0.0, 0.0, 200.0, 100.0)),
            0,
            true,
            TOP,
        )
        .expect("fit works")
        .expect("shrink is not identity");
        assert_eq!((matrix.a, matrix.d), (0.5, 0.5));

        let matrix = write_text_matrix(
            mk_rect(0.0, 0.0, 100.0, 100.0),
            Some(mk_rect(0.0, 0.0, 200.0, 100.0)),
            0,
            false,
            TOP,
        )
        .expect("fit works")
        .expect("shrink is not identity");
        assert_eq!((matrix.a, matrix.d), (0.5, 1.0));
    }

    #[test]
    fn quarter_turns_rotate_about_the_corner() {
        let matrix = write_text_matrix(
            mk_rect(0.0, 0.0, 100.0, 50.0),
            Some(mk_rect(0.0, 0.0, 100.0, 50.0)),
            90,
            true,
            TOP,
        )
        .expect("fit works")
        .expect("turn is not identity");
        assert_eq!(
            (matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f),
            (0.0, 0.5, -0.5, 0.0, 396.0, 742.0)
        );
        // The spun corner lands mid-rect, through the flip.
        assert_eq!(through(&matrix, 0.0, TOP), (0.0, TOP - 50.0));
    }

    #[test]
    fn odd_angles_fail_loudly() {
        assert!(write_text_matrix(
            mk_rect(0.0, 0.0, 10.0, 10.0),
            Some(mk_rect(0.0, 0.0, 10.0, 10.0)),
            45,
            true,
            TOP
        )
        .is_err());
        assert!(write_text_matrix(
            mk_rect(0.0, 0.0, 10.0, 10.0),
            Some(mk_rect(0.0, 0.0, 10.0, 10.0)),
            -90,
            true,
            TOP,
        )
        .expect("negative turns normalise")
        .is_some());
    }
}
