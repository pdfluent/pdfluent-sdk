//! Geometric annotations: Line, Square, Circle, Polygon, PolyLine, Ink.

use crate::annotation::Annotation;
use crate::types::*;
use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Array, Dict, Name, Rect};

/// A line annotation (ISO 32000-2 §12.5.6.7).
#[derive(Debug)]
pub struct LineAnnotation {
    /// The line endpoints [x1, y1, x2, y2].
    pub endpoints: Option<[f32; 4]>,
    /// Line ending styles [start, end].
    pub line_endings: (LineEnding, LineEnding),
    /// Whether a caption is shown.
    pub caption: bool,
    /// Caption offset [horizontal, vertical].
    pub caption_offset: Option<[f32; 2]>,
    /// Leader line length.
    pub leader_line: f32,
    /// Leader line extension.
    pub leader_line_extension: f32,
    /// Leader line offset.
    pub leader_line_offset: f32,
}

impl LineAnnotation {
    /// Extract line annotation properties.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let dict = annot.dict();
        let endpoints = dict.get::<Array<'_>>(L).and_then(|arr| {
            let mut iter = arr.iter::<f32>();
            Some([iter.next()?, iter.next()?, iter.next()?, iter.next()?])
        });
        let line_endings = parse_line_endings(dict);
        let caption = dict.get::<bool>(CAP).unwrap_or(false);
        let caption_offset = dict.get::<Array<'_>>(CO).and_then(|arr| {
            let mut iter = arr.iter::<f32>();
            Some([iter.next()?, iter.next()?])
        });
        let leader_line = dict.get::<f32>(LL).unwrap_or(0.0);
        let leader_line_extension = dict.get::<f32>(LLE).unwrap_or(0.0);
        let leader_line_offset = dict.get::<f32>(LLO).unwrap_or(0.0);
        Self {
            endpoints,
            line_endings,
            caption,
            caption_offset,
            leader_line,
            leader_line_extension,
            leader_line_offset,
        }
    }
}

/// A Square or Circle annotation.
#[derive(Debug)]
pub struct SquareCircleAnnotation {
    /// `true` for Circle, `false` for Square.
    pub is_circle: bool,
    /// Rectangle differences.
    pub rd: Option<Rect>,
}

impl SquareCircleAnnotation {
    /// Extract square/circle annotation properties.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let is_circle = annot.annotation_type() == AnnotationType::Circle;
        let rd = annot.dict().get::<Rect>(RD);
        Self { is_circle, rd }
    }
}

/// A Polygon or PolyLine annotation.
#[derive(Debug)]
pub struct PolygonAnnotation {
    /// `true` for Polygon (closed), `false` for PolyLine (open).
    pub closed: bool,
    /// The vertices as pairs of coordinates.
    pub vertices: Vec<f32>,
    /// Line ending styles [start, end].
    pub line_endings: (LineEnding, LineEnding),
}

impl PolygonAnnotation {
    /// Extract polygon/polyline annotation properties.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let dict = annot.dict();
        let closed = annot.annotation_type() == AnnotationType::Polygon;
        let vertices = dict
            .get::<Array<'_>>(VERTICES)
            .map(|arr| arr.iter::<f32>().collect())
            .unwrap_or_default();
        let line_endings = parse_line_endings(dict);
        Self {
            closed,
            vertices,
            line_endings,
        }
    }
}

/// An Ink (freehand drawing) annotation.
#[derive(Debug)]
pub struct InkAnnotation {
    /// List of ink strokes.
    pub ink_list: Vec<Vec<f32>>,
}

impl InkAnnotation {
    /// Extract ink annotation properties.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let dict = annot.dict();
        let ink_list = dict
            .get::<Array<'_>>(INKLIST)
            .map(|outer| {
                outer
                    .iter::<Array<'_>>()
                    .map(|inner| inner.iter::<f32>().collect())
                    .collect()
            })
            .unwrap_or_default();
        Self { ink_list }
    }
}

/// Parse line ending styles from `/LE` array.
fn parse_line_endings(dict: &Dict<'_>) -> (LineEnding, LineEnding) {
    dict.get::<Array<'_>>(LE)
        .and_then(|arr| {
            let mut iter = arr.iter::<Name>();
            let start = iter.next().map(|n| LineEnding::from_name(n.as_ref()))?;
            let end = iter.next().map(|n| LineEnding::from_name(n.as_ref()))?;
            Some((start, end))
        })
        .unwrap_or((LineEnding::None, LineEnding::None))
}
