//! Markup annotations: Text, Highlight, Underline, StrikeOut, Squiggly, Caret, Popup.

extern crate alloc;

use crate::annotation::Annotation;
use crate::types::*;
use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Name, Rect};

/// A text (sticky note) annotation.
#[derive(Debug)]
pub struct TextAnnotation {
    /// Whether the annotation is initially open.
    pub open: bool,
    /// The icon name.
    pub icon: alloc::string::String,
}

impl TextAnnotation {
    /// Extract text annotation properties from an annotation.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let dict = annot.dict();
        let open = dict.get::<bool>(Name::new(b"Open")).unwrap_or(false);
        let icon = dict
            .get::<Name>(NAME)
            .map(|n| alloc::string::String::from(n.as_str()))
            .unwrap_or_else(|| alloc::string::String::from("Note"));
        Self { open, icon }
    }
}

/// The type of text markup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMarkupType {
    /// Highlight.
    Highlight,
    /// Underline.
    Underline,
    /// Squiggly underline.
    Squiggly,
    /// Strikeout.
    StrikeOut,
}

/// A text markup annotation (Highlight, Underline, Squiggly, StrikeOut).
#[derive(Debug)]
pub struct TextMarkupAnnotation {
    /// The markup type.
    pub markup_type: TextMarkupType,
    /// The quadrilateral points defining the marked-up region.
    pub quad_points: Option<QuadPoints>,
}

impl TextMarkupAnnotation {
    /// Extract text markup properties from an annotation.
    pub fn from_annot(annot: &Annotation<'_>) -> Option<Self> {
        let markup_type = match annot.annotation_type() {
            AnnotationType::Highlight => TextMarkupType::Highlight,
            AnnotationType::Underline => TextMarkupType::Underline,
            AnnotationType::Squiggly => TextMarkupType::Squiggly,
            AnnotationType::StrikeOut => TextMarkupType::StrikeOut,
            _ => return None,
        };
        let quad_points = annot.quad_points();
        Some(Self {
            markup_type,
            quad_points,
        })
    }
}

/// A caret annotation (ISO 32000-2 §12.5.6.18).
#[derive(Debug)]
pub struct CaretAnnotation {
    /// The caret symbol.
    pub symbol: alloc::string::String,
    /// Rectangle differences.
    pub rd: Option<Rect>,
}

impl CaretAnnotation {
    /// Extract caret annotation properties.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let dict = annot.dict();
        let symbol = dict
            .get::<Name>(SY)
            .map(|n| alloc::string::String::from(n.as_str()))
            .unwrap_or_else(|| alloc::string::String::from("None"));
        let rd = dict.get::<Rect>(RD);
        Self { symbol, rd }
    }
}

/// A popup annotation (ISO 32000-2 §12.5.6.14).
#[derive(Debug)]
pub struct PopupAnnotation {
    /// Whether the popup is initially open.
    pub open: bool,
}

impl PopupAnnotation {
    /// Extract popup annotation properties.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let open = annot
            .dict()
            .get::<bool>(Name::new(b"Open"))
            .unwrap_or(false);
        Self { open }
    }
}
