//! Content stream parser — converts raw bytes into typed [`ParsedOp`] items.
//!
//! Every byte in the input is covered by exactly one `ParsedOp`'s byte range.
//! Adjacent `ParsedOp` ranges are contiguous:
//! `ops[i+1].byte_start == ops[i].byte_end` for all `i`.

use crate::error::ContentStreamError;
use crate::ops::{ContentOp, Matrix, RawOperand, TjItem};
use crate::tokenizer::Tokenizer;

/// A parsed operator together with its exact byte range in the input.
///
/// `raw = &input[byte_start..byte_end]` includes all whitespace and comments
/// that precede the operator group, so that concatenating the `raw` slices
/// of all ops in order reproduces the entire input byte-for-byte.
#[derive(Debug, Clone)]
pub struct ParsedOp {
    /// Byte offset where this instruction begins (including leading whitespace).
    pub byte_start: usize,
    /// Byte offset right after the last byte of the operator keyword.
    pub byte_end: usize,
    /// The typed content operator.
    pub op: ContentOp,
}

impl ParsedOp {
    /// Returns the raw bytes of this instruction from the given input buffer.
    #[inline]
    pub fn raw<'a>(&self, input: &'a [u8]) -> &'a [u8] {
        &input[self.byte_start..self.byte_end]
    }
}

/// Iterator over parsed content stream operators.
///
/// Yields `Result<ParsedOp, ContentStreamError>`.  On error the iterator
/// terminates; subsequent calls return `None`.
pub struct ContentStreamParser<'a> {
    tok: Tokenizer<'a>,
    done: bool,
}

impl<'a> ContentStreamParser<'a> {
    /// Create a new parser for the given content stream bytes.
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            tok: Tokenizer::new(data),
            done: false,
        }
    }

    /// Parse the entire content stream, collecting all operators.
    ///
    /// Returns `Err` on the first parse failure.
    pub fn collect_ops(mut self) -> Result<Vec<ParsedOp>, ContentStreamError> {
        let mut ops = Vec::new();
        for item in &mut self {
            ops.push(item?);
        }
        Ok(ops)
    }

    /// Parse the entire content stream, collecting all operators.
    ///
    /// On error, keeps the successfully parsed ops so far and returns both.
    pub fn collect_ops_lenient(mut self) -> (Vec<ParsedOp>, Option<ContentStreamError>) {
        let mut ops = Vec::new();
        let mut err = None;
        for item in &mut self {
            match item {
                Ok(op) => ops.push(op),
                Err(e) => {
                    err = Some(e);
                    break;
                }
            }
        }
        (ops, err)
    }
}

impl<'a> Iterator for ContentStreamParser<'a> {
    type Item = Result<ParsedOp, ContentStreamError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        // byte_start covers leading whitespace before this operator group.
        let byte_start = self.tok.pos;

        self.tok.skip_whitespace();

        if self.tok.at_end() {
            return None;
        }

        // Collect operands.
        let mut operands: Vec<RawOperand> = Vec::new();
        loop {
            match self.tok.try_read_operand() {
                Err(e) => {
                    self.done = true;
                    return Some(Err(e));
                }
                Ok(Some(op)) => operands.push(op),
                Ok(None) => break,
            }
        }

        // At this point the next token must be an operator keyword.
        self.tok.skip_whitespace();
        if self.tok.at_end() {
            // Trailing operands with no operator — treat as parse error.
            if !operands.is_empty() {
                self.done = true;
                return Some(Err(ContentStreamError::UnexpectedEndOfStream {
                    at: self.tok.pos,
                }));
            }
            return None;
        }

        // Consume the operator keyword (advances past its bytes, incl. BI body).
        self.tok.pos += 1; // consume first byte before read_operator_keyword
        let name = self.tok.read_operator_keyword();

        let byte_end = self.tok.pos;

        let op = match dispatch_op(&name, operands) {
            Ok(op) => op,
            Err(e) => {
                self.done = true;
                return Some(Err(e));
            }
        };

        Some(Ok(ParsedOp {
            byte_start,
            byte_end,
            op,
        }))
    }
}

// ── Operator dispatch ─────────────────────────────────────────────────────────

fn dispatch_op(name: &[u8], ops: Vec<RawOperand>) -> Result<ContentOp, ContentStreamError> {
    match name {
        b"BT" => Ok(ContentOp::BeginText),
        b"ET" => Ok(ContentOp::EndText),
        b"q" => Ok(ContentOp::SaveState),
        b"Q" => Ok(ContentOp::RestoreState),

        b"cm" => {
            let m = extract_matrix(&ops, "cm")?;
            Ok(ContentOp::Transform(m))
        }
        b"Tf" => {
            let name = extract_name(&ops, 0, "Tf")?;
            let size = extract_number(&ops, 1, "Tf")?;
            Ok(ContentOp::SetFont { name, size })
        }

        b"rg" => {
            let r = extract_number(&ops, 0, "rg")?;
            let g = extract_number(&ops, 1, "rg")?;
            let b = extract_number(&ops, 2, "rg")?;
            Ok(ContentOp::SetFillRgb(r, g, b))
        }
        b"g" => {
            let gray = extract_number(&ops, 0, "g")?;
            Ok(ContentOp::SetFillGray(gray))
        }
        b"k" => {
            let c = extract_number(&ops, 0, "k")?;
            let m = extract_number(&ops, 1, "k")?;
            let y = extract_number(&ops, 2, "k")?;
            let k = extract_number(&ops, 3, "k")?;
            Ok(ContentOp::SetFillCmyk(c, m, y, k))
        }
        b"RG" => {
            let r = extract_number(&ops, 0, "RG")?;
            let g = extract_number(&ops, 1, "RG")?;
            let b = extract_number(&ops, 2, "RG")?;
            Ok(ContentOp::SetStrokeRgb(r, g, b))
        }
        b"G" => {
            let gray = extract_number(&ops, 0, "G")?;
            Ok(ContentOp::SetStrokeGray(gray))
        }
        b"K" => {
            let c = extract_number(&ops, 0, "K")?;
            let m = extract_number(&ops, 1, "K")?;
            let y = extract_number(&ops, 2, "K")?;
            let k = extract_number(&ops, 3, "K")?;
            Ok(ContentOp::SetStrokeCmyk(c, m, y, k))
        }

        b"Tm" => {
            let m = extract_matrix(&ops, "Tm")?;
            Ok(ContentOp::SetTextMatrix(m))
        }
        b"Td" => {
            let tx = extract_number(&ops, 0, "Td")?;
            let ty = extract_number(&ops, 1, "Td")?;
            Ok(ContentOp::MoveText(tx, ty))
        }
        b"TD" => {
            let tx = extract_number(&ops, 0, "TD")?;
            let ty = extract_number(&ops, 1, "TD")?;
            Ok(ContentOp::MoveTextAndSetLeading(tx, ty))
        }
        b"T*" => Ok(ContentOp::NextLine),

        b"Tc" => {
            let ac = extract_number(&ops, 0, "Tc")?;
            Ok(ContentOp::SetCharSpacing(ac))
        }
        b"Tw" => {
            let aw = extract_number(&ops, 0, "Tw")?;
            Ok(ContentOp::SetWordSpacing(aw))
        }
        b"Tz" => {
            let scale = extract_number(&ops, 0, "Tz")?;
            Ok(ContentOp::SetHorizontalScale(scale))
        }
        b"TL" => {
            let leading = extract_number(&ops, 0, "TL")?;
            Ok(ContentOp::SetLeading(leading))
        }
        b"Ts" => {
            let rise = extract_number(&ops, 0, "Ts")?;
            Ok(ContentOp::SetRise(rise))
        }

        b"Tj" => {
            let text = extract_string(&ops, 0, "Tj")?;
            Ok(ContentOp::ShowText(text))
        }
        b"TJ" => {
            let arr = extract_array(&ops, 0, "TJ")?;
            let items = convert_tj_array(arr)?;
            Ok(ContentOp::ShowTexts(items))
        }
        b"'" => {
            let text = extract_string(&ops, 0, "'")?;
            Ok(ContentOp::NextLineAndShowText(text))
        }
        b"\"" => {
            let aw = extract_number(&ops, 0, "\"")?;
            let ac = extract_number(&ops, 1, "\"")?;
            let text = extract_string(&ops, 2, "\"")?;
            Ok(ContentOp::ShowTextWithParams { aw, ac, text })
        }

        // Pass-through for all other operators.
        _ => Ok(ContentOp::Other {
            name: name.to_vec(),
            operands: ops,
        }),
    }
}

// ── Operand extractors ───────────────────────────────────────────────────────

fn extract_number(ops: &[RawOperand], idx: usize, op: &str) -> Result<f32, ContentStreamError> {
    match ops.get(idx) {
        Some(RawOperand::Number(n)) => Ok(*n),
        Some(other) => Err(ContentStreamError::MalformedOperand {
            op: op.to_owned(),
            reason: format!("operand {idx} expected number, got {other:?}"),
        }),
        None => Err(ContentStreamError::MalformedOperand {
            op: op.to_owned(),
            reason: format!("missing operand {idx}"),
        }),
    }
}

fn extract_name(ops: &[RawOperand], idx: usize, op: &str) -> Result<Vec<u8>, ContentStreamError> {
    match ops.get(idx) {
        Some(RawOperand::Name(n)) => Ok(n.clone()),
        Some(other) => Err(ContentStreamError::MalformedOperand {
            op: op.to_owned(),
            reason: format!("operand {idx} expected name, got {other:?}"),
        }),
        None => Err(ContentStreamError::MalformedOperand {
            op: op.to_owned(),
            reason: format!("missing operand {idx}"),
        }),
    }
}

fn extract_string(ops: &[RawOperand], idx: usize, op: &str) -> Result<Vec<u8>, ContentStreamError> {
    match ops.get(idx) {
        Some(RawOperand::String(s)) => Ok(s.clone()),
        Some(other) => Err(ContentStreamError::MalformedOperand {
            op: op.to_owned(),
            reason: format!("operand {idx} expected string, got {other:?}"),
        }),
        None => Err(ContentStreamError::MalformedOperand {
            op: op.to_owned(),
            reason: format!("missing operand {idx}"),
        }),
    }
}

fn extract_array(
    ops: &[RawOperand],
    idx: usize,
    op: &str,
) -> Result<Vec<RawOperand>, ContentStreamError> {
    match ops.get(idx) {
        Some(RawOperand::Array(a)) => Ok(a.clone()),
        Some(other) => Err(ContentStreamError::MalformedOperand {
            op: op.to_owned(),
            reason: format!("operand {idx} expected array, got {other:?}"),
        }),
        None => Err(ContentStreamError::MalformedOperand {
            op: op.to_owned(),
            reason: format!("missing operand {idx}"),
        }),
    }
}

fn extract_matrix(ops: &[RawOperand], op: &str) -> Result<Matrix, ContentStreamError> {
    if ops.len() < 6 {
        return Err(ContentStreamError::MalformedOperand {
            op: op.to_owned(),
            reason: format!("expected 6 operands, got {}", ops.len()),
        });
    }
    let mut m = [0f32; 6];
    for (i, item) in m.iter_mut().enumerate() {
        *item = extract_number(ops, i, op)?;
    }
    Ok(m)
}

fn convert_tj_array(arr: Vec<RawOperand>) -> Result<Vec<TjItem>, ContentStreamError> {
    let mut items = Vec::with_capacity(arr.len());
    for elem in arr {
        match elem {
            RawOperand::String(s) => items.push(TjItem::Text(s)),
            RawOperand::Number(n) => items.push(TjItem::Kern(n)),
            other => {
                return Err(ContentStreamError::MalformedOperand {
                    op: "TJ".to_owned(),
                    reason: format!("unexpected array element {other:?}"),
                })
            }
        }
    }
    Ok(items)
}
