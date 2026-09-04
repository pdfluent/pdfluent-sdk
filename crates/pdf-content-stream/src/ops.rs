// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// A PDF matrix: [a b c d e f] — the six components of a 3×2 affine matrix.
pub type Matrix = [f32; 6];

/// An item inside a `TJ` array.
#[derive(Debug, Clone, PartialEq)]
pub enum TjItem {
    /// A string to show (raw PDF encoding bytes).
    Text(Vec<u8>),
    /// A kerning adjustment in thousandths of a text unit (negative = forward).
    Kern(f32),
}

/// A raw operand captured from the content stream for pass-through operators.
#[derive(Debug, Clone, PartialEq)]
pub enum RawOperand {
    Number(f32),
    Name(Vec<u8>),
    /// Raw bytes of a string (literal or hex, after decode).
    String(Vec<u8>),
    Array(Vec<RawOperand>),
}

/// A fully typed G3 content stream operator.
///
/// Every variant maps 1-to-1 to a PDF operator in the G3 coverage list.
/// Operators outside G3 scope pass through as [`ContentOp::Other`].
#[derive(Debug, Clone, PartialEq)]
pub enum ContentOp {
    // ── Text block ──────────────────────────────────────────────────────────
    /// `BT` — begin text object
    BeginText,
    /// `ET` — end text object
    EndText,

    // ── Graphics state ───────────────────────────────────────────────────────
    /// `q` — save graphics state
    SaveState,
    /// `Q` — restore graphics state
    RestoreState,
    /// `cm a b c d e f` — concatenate matrix to CTM
    Transform(Matrix),

    // ── Font ─────────────────────────────────────────────────────────────────
    /// `name size Tf` — set text font and size
    SetFont { name: Vec<u8>, size: f32 },

    // ── Fill color ───────────────────────────────────────────────────────────
    /// `r g b rg` — set fill color (DeviceRGB)
    SetFillRgb(f32, f32, f32),
    /// `gray g` — set fill color (DeviceGray)
    SetFillGray(f32),
    /// `c m y k k` — set fill color (DeviceCMYK)
    SetFillCmyk(f32, f32, f32, f32),

    // ── Stroke color ─────────────────────────────────────────────────────────
    /// `r g b RG` — set stroke color (DeviceRGB)
    SetStrokeRgb(f32, f32, f32),
    /// `gray G` — set stroke color (DeviceGray)
    SetStrokeGray(f32),
    /// `c m y k K` — set stroke color (DeviceCMYK)
    SetStrokeCmyk(f32, f32, f32, f32),

    // ── Text matrix / position ───────────────────────────────────────────────
    /// `a b c d e f Tm` — set text matrix and text line matrix
    SetTextMatrix(Matrix),
    /// `tx ty Td` — move text position
    MoveText(f32, f32),
    /// `tx ty TD` — move text position and set leading (TD ≡ -ty TL + Td)
    MoveTextAndSetLeading(f32, f32),
    /// `T*` — move to start of next text line (equivalent to `0 -TL Td`)
    NextLine,

    // ── Text state parameters ────────────────────────────────────────────────
    /// `ac Tc` — set character spacing
    SetCharSpacing(f32),
    /// `aw Tw` — set word spacing
    SetWordSpacing(f32),
    /// `scale Tz` — set horizontal scaling (percentage)
    SetHorizontalScale(f32),
    /// `leading TL` — set text leading
    SetLeading(f32),
    /// `rise Ts` — set text rise
    SetRise(f32),

    // ── Text showing ─────────────────────────────────────────────────────────
    /// `string Tj` — show text string
    ShowText(Vec<u8>),
    /// `array TJ` — show text, allowing individual glyph positioning
    ShowTexts(Vec<TjItem>),
    /// `string '` — move to next line and show text
    NextLineAndShowText(Vec<u8>),
    /// `aw ac string "` — set word/char spacing, move to next line, show text
    ShowTextWithParams { aw: f32, ac: f32, text: Vec<u8> },

    // ── Pass-through ─────────────────────────────────────────────────────────
    /// Any operator outside the G3 coverage list.
    ///
    /// The name and raw operands are preserved so round-trip serialization
    /// can reproduce the original bytes exactly.
    Other {
        /// Operator name bytes (e.g. `b"m"`, `b"f"`, `b"BI"`).
        name: Vec<u8>,
        /// Operands in the order they appeared before the operator.
        operands: Vec<RawOperand>,
    },
}

impl ContentOp {
    /// Returns the operator name bytes as they appear in the content stream.
    pub fn operator_name(&self) -> &[u8] {
        match self {
            ContentOp::BeginText => b"BT",
            ContentOp::EndText => b"ET",
            ContentOp::SaveState => b"q",
            ContentOp::RestoreState => b"Q",
            ContentOp::Transform(_) => b"cm",
            ContentOp::SetFont { .. } => b"Tf",
            ContentOp::SetFillRgb(..) => b"rg",
            ContentOp::SetFillGray(_) => b"g",
            ContentOp::SetFillCmyk(..) => b"k",
            ContentOp::SetStrokeRgb(..) => b"RG",
            ContentOp::SetStrokeGray(_) => b"G",
            ContentOp::SetStrokeCmyk(..) => b"K",
            ContentOp::SetTextMatrix(_) => b"Tm",
            ContentOp::MoveText(..) => b"Td",
            ContentOp::MoveTextAndSetLeading(..) => b"TD",
            ContentOp::NextLine => b"T*",
            ContentOp::SetCharSpacing(_) => b"Tc",
            ContentOp::SetWordSpacing(_) => b"Tw",
            ContentOp::SetHorizontalScale(_) => b"Tz",
            ContentOp::SetLeading(_) => b"TL",
            ContentOp::SetRise(_) => b"Ts",
            ContentOp::ShowText(_) => b"Tj",
            ContentOp::ShowTexts(_) => b"TJ",
            ContentOp::NextLineAndShowText(_) => b"'",
            ContentOp::ShowTextWithParams { .. } => b"\"",
            ContentOp::Other { name, .. } => name.as_slice(),
        }
    }

    /// Returns `true` if this is a text-showing operator.
    pub fn is_text_showing(&self) -> bool {
        matches!(
            self,
            ContentOp::ShowText(_)
                | ContentOp::ShowTexts(_)
                | ContentOp::NextLineAndShowText(_)
                | ContentOp::ShowTextWithParams { .. }
        )
    }
}
