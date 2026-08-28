//! Graphics and text state machine for PDF content streams.
//!
//! Tracks state changes driven by G3 operators.  The state machine is
//! **read-only** — it never modifies the content stream.

use crate::ops::{ContentOp, Matrix, TjItem};

/// An RGB, gray, or CMYK color value.
#[derive(Debug, Clone, PartialEq)]
pub enum Color {
    /// Device RGB components in [0, 1].
    Rgb(f32, f32, f32),
    /// Device gray level in [0, 1].
    Gray(f32),
    /// Device CMYK components in [0, 1].
    Cmyk(f32, f32, f32, f32),
}

impl Color {
    /// Return an approximate `[r, g, b]` in [0, 1], converting from other spaces.
    pub fn to_rgb(&self) -> [f32; 3] {
        match *self {
            Color::Rgb(r, g, b) => [r, g, b],
            Color::Gray(v) => [v, v, v],
            Color::Cmyk(c, m, y, k) => [
                (1.0 - c) * (1.0 - k),
                (1.0 - m) * (1.0 - k),
                (1.0 - y) * (1.0 - k),
            ],
        }
    }
}

/// The PDF text state (PDF 32000-1:2008 §9.3).
#[derive(Debug, Clone, PartialEq)]
pub struct TextState {
    /// Current font resource name (None if not yet set).
    pub font_name: Option<Vec<u8>>,
    /// Current font size in unscaled text space units.
    pub font_size: f32,
    /// Character spacing (Tc).
    pub char_spacing: f32,
    /// Word spacing (Tw).
    pub word_spacing: f32,
    /// Horizontal scaling as a percentage (Tz).
    pub horizontal_scale: f32,
    /// Text leading (TL).
    pub leading: f32,
    /// Text rise (Ts).
    pub rise: f32,
    /// Text matrix `Tm` — transforms text space to user space.
    pub text_matrix: Matrix,
    /// Text line matrix — reset at each `Td`/`TD`/`T*`/`Tm`.
    pub text_line_matrix: Matrix,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            font_name: None,
            font_size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scale: 100.0,
            leading: 0.0,
            rise: 0.0,
            text_matrix: identity_matrix(),
            text_line_matrix: identity_matrix(),
        }
    }
}

/// The full graphics state at a point in the stream.
///
/// Only the subset tracked by G3 operators is included.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphicsState {
    /// Current Transformation Matrix (CTM) — user space to device space.
    pub ctm: Matrix,
    /// Current fill color (`rg`/`g`/`k`).  `None` = not yet set.
    pub fill_color: Option<Color>,
    /// Current stroke color (`RG`/`G`/`K`).  `None` = not yet set.
    pub stroke_color: Option<Color>,
    /// Text state.
    pub text: TextState,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: identity_matrix(),
            fill_color: None,
            stroke_color: None,
            text: TextState::default(),
        }
    }
}

/// State machine that tracks the current graphics and text state as G3
/// operators are applied in sequence.
///
/// Usage:
/// ```
/// use pdf_content_stream::{ContentStreamParser, ContentStateMachine};
/// let ops = ContentStreamParser::new(b"BT /F1 12 Tf 100 700 Td ET")
///     .collect_ops().unwrap();
/// let mut machine = ContentStateMachine::new();
/// for op in &ops {
///     machine.apply(&op.op);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ContentStateMachine {
    stack: Vec<GraphicsState>,
    current: GraphicsState,
    /// True while inside a `BT`…`ET` text block.
    pub in_text_block: bool,
}

impl Default for ContentStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentStateMachine {
    /// Create a new state machine with PDF default initial state.
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            current: GraphicsState::default(),
            in_text_block: false,
        }
    }

    /// Return a reference to the current graphics state.
    pub fn state(&self) -> &GraphicsState {
        &self.current
    }

    /// Apply one content operator, updating state.
    ///
    /// Operators outside the G3 coverage (`ContentOp::Other`) are ignored
    /// (they may affect state outside G3 scope, but G3 does not track that).
    pub fn apply(&mut self, op: &ContentOp) {
        match op {
            ContentOp::BeginText => {
                self.in_text_block = true;
                // PDF spec: Tm and Tlm are set to identity at BT.
                self.current.text.text_matrix = identity_matrix();
                self.current.text.text_line_matrix = identity_matrix();
            }
            ContentOp::EndText => {
                self.in_text_block = false;
            }

            ContentOp::SaveState => {
                self.stack.push(self.current.clone());
            }
            ContentOp::RestoreState => {
                if let Some(saved) = self.stack.pop() {
                    self.current = saved;
                }
            }

            ContentOp::Transform(m) => {
                self.current.ctm = matrix_concat(&self.current.ctm, m);
            }

            ContentOp::SetFont { name, size } => {
                self.current.text.font_name = Some(name.clone());
                self.current.text.font_size = *size;
            }

            // Fill colors
            ContentOp::SetFillRgb(r, g, b) => {
                self.current.fill_color = Some(Color::Rgb(*r, *g, *b));
            }
            ContentOp::SetFillGray(v) => {
                self.current.fill_color = Some(Color::Gray(*v));
            }
            ContentOp::SetFillCmyk(c, m, y, k) => {
                self.current.fill_color = Some(Color::Cmyk(*c, *m, *y, *k));
            }

            // Stroke colors
            ContentOp::SetStrokeRgb(r, g, b) => {
                self.current.stroke_color = Some(Color::Rgb(*r, *g, *b));
            }
            ContentOp::SetStrokeGray(v) => {
                self.current.stroke_color = Some(Color::Gray(*v));
            }
            ContentOp::SetStrokeCmyk(c, m, y, k) => {
                self.current.stroke_color = Some(Color::Cmyk(*c, *m, *y, *k));
            }

            // Text state parameters
            ContentOp::SetCharSpacing(v) => self.current.text.char_spacing = *v,
            ContentOp::SetWordSpacing(v) => self.current.text.word_spacing = *v,
            ContentOp::SetHorizontalScale(v) => self.current.text.horizontal_scale = *v,
            ContentOp::SetLeading(v) => self.current.text.leading = *v,
            ContentOp::SetRise(v) => self.current.text.rise = *v,

            // Text matrix
            ContentOp::SetTextMatrix(m) => {
                self.current.text.text_matrix = *m;
                self.current.text.text_line_matrix = *m;
            }

            // Text position operators — update Tm and Tlm.
            ContentOp::MoveText(tx, ty) => {
                let tlm = self.current.text.text_line_matrix;
                let new_tlm = matrix_translate(&tlm, *tx, *ty);
                self.current.text.text_line_matrix = new_tlm;
                self.current.text.text_matrix = new_tlm;
            }
            ContentOp::MoveTextAndSetLeading(tx, ty) => {
                // TD ≡ -ty TL tx ty Td
                self.current.text.leading = -ty;
                let tlm = self.current.text.text_line_matrix;
                let new_tlm = matrix_translate(&tlm, *tx, *ty);
                self.current.text.text_line_matrix = new_tlm;
                self.current.text.text_matrix = new_tlm;
            }
            ContentOp::NextLine => {
                // T* ≡ 0 -TL Td
                let leading = self.current.text.leading;
                let tlm = self.current.text.text_line_matrix;
                let new_tlm = matrix_translate(&tlm, 0.0, -leading);
                self.current.text.text_line_matrix = new_tlm;
                self.current.text.text_matrix = new_tlm;
            }

            // Text-showing operators advance the text matrix by the string width.
            // For G3 (read-only), we don't compute advance widths from font metrics,
            // so Tm is left unchanged.  G2 would supply metrics for this.
            ContentOp::ShowText(_)
            | ContentOp::NextLineAndShowText(_)
            | ContentOp::ShowTextWithParams { .. } => {}

            ContentOp::ShowTexts(_) => {}

            ContentOp::Other { .. } => {}
        }
    }

    /// Apply a sequence of operators in order.
    pub fn apply_all<'a>(&mut self, ops: impl IntoIterator<Item = &'a ContentOp>) {
        for op in ops {
            self.apply(op);
        }
    }
}

// ── Matrix helpers ────────────────────────────────────────────────────────────

/// Identity matrix `[1 0 0 1 0 0]`.
#[inline]
pub fn identity_matrix() -> Matrix {
    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

/// Concatenate two 2D affine matrices: result = a × b.
///
/// PDF convention: `[a b c d e f]` represents the matrix:
/// ```text
/// [ a  b  0 ]
/// [ c  d  0 ]
/// [ e  f  1 ]
/// ```
pub fn matrix_concat(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

/// Apply a translation `(tx, ty)` to matrix `m` (result = translate × m).
pub fn matrix_translate(m: &Matrix, tx: f32, ty: f32) -> Matrix {
    // Translate matrix in text line coordinates: new_e = tx*a + ty*c + e, etc.
    [
        m[0],
        m[1],
        m[2],
        m[3],
        tx * m[0] + ty * m[2] + m[4],
        tx * m[1] + ty * m[3] + m[5],
    ]
}

/// Extract the origin (translation) of a matrix as `(x, y)` in user space.
#[inline]
pub fn matrix_origin(m: &Matrix) -> (f32, f32) {
    (m[4], m[5])
}

/// Compute the total decoded byte length of a `TJ` array (for advance-width estimation).
pub fn tj_text_len(items: &[TjItem]) -> usize {
    items
        .iter()
        .filter_map(|it| {
            if let TjItem::Text(t) = it {
                Some(t.len())
            } else {
                None
            }
        })
        .sum()
}

#[cfg(test)]
mod tj_len_tests {
    use super::tj_text_len;
    use crate::ops::TjItem;

    /// Een TJ-array wisselt tekst en kerningwaarden af. Wie de kerning meetelt
    /// als tekst, schat de breedte te hoog en zet alles daarna verkeerd — en dat
    /// is precies het soort fout dat er op het scherm uitziet als "de opmaak
    /// klopt niet" zonder dat iets faalt.
    #[test]
    fn only_text_counts_never_the_kerning() {
        let items = vec![
            TjItem::Text(b"Hallo".to_vec()),
            TjItem::Kern(-250.0),
            TjItem::Text(b" wereld".to_vec()),
        ];
        assert_eq!(tj_text_len(&items), 5 + 7);
    }

    #[test]
    fn kerning_alone_is_zero_length() {
        assert_eq!(
            tj_text_len(&[TjItem::Kern(-1000.0), TjItem::Kern(120.0)]),
            0
        );
    }

    #[test]
    fn an_empty_array_is_zero() {
        assert_eq!(tj_text_len(&[]), 0);
    }

    /// Bytes, geen tekens: de inhoud is ruwe PDF-codering, en hoeveel tekens
    /// dat voorstelt hangt van het lettertype af. Twee bytes tellen als twee.
    #[test]
    fn length_is_counted_in_bytes_not_characters() {
        // "é" in UTF-8 is twee bytes; als codering in het document telt dat als 2.
        assert_eq!(tj_text_len(&[TjItem::Text(vec![0xC3, 0xA9])]), 2);
        // Een lege string draagt niets bij.
        assert_eq!(tj_text_len(&[TjItem::Text(Vec::new())]), 0);
    }
}
