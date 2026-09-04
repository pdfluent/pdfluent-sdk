// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/*!
PDF content stream parser, state machine, and text-run locator.

This crate is the G3 foundation for Track G (Text Editing Fidelity).

# Quick start

```rust
use pdf_content_stream::{ContentStreamParser, ContentStateMachine, serialize};

let input = b"BT /F1 12 Tf 100 700 Td (Hello world) Tj ET";
let ops = ContentStreamParser::new(input).collect_ops().unwrap();

// Drive the state machine.
let mut machine = ContentStateMachine::new();
for op in &ops {
    machine.apply(&op.op);
}

// Round-trip: serialize produces byte-identical output.
let output = serialize(input, &ops);
assert_eq!(input as &[u8], output.as_slice());
```

# Operator coverage

All G3 operators are fully parsed into typed variants:

`BT`, `ET`, `q`, `Q`, `cm`, `Tf`, `rg`, `g`, `k`, `RG`, `G`, `K`,
`Tm`, `Td`, `TD`, `T*`, `Tc`, `Tw`, `Tz`, `TL`, `Ts`,
`Tj`, `TJ`, `'`, `"`

Operators outside this list produce [`ContentOp::Other`] (pass-through,
never silently dropped).

# Error handling

All failure modes are expressed as [`ContentStreamError`] variants.
The parser never panics on malformed input.
*/

pub mod error;
pub mod locator;
pub mod ops;
pub mod parser;
pub mod serialize;
pub mod state;
pub(crate) mod tokenizer;

#[cfg(test)]
mod tests;

// ── Public re-exports ─────────────────────────────────────────────────────────

pub use error::ContentStreamError;
pub use locator::{
    extract_text_runs, find_span, find_spans_containing, ExtractedRun, TextRunLocation,
};
pub use ops::{ContentOp, Matrix, RawOperand, TjItem};
pub use parser::{ContentStreamParser, ParsedOp};
pub use serialize::{serialize, verify_round_trip};
pub use state::{
    identity_matrix, matrix_concat, matrix_origin, matrix_translate, Color, ContentStateMachine,
    GraphicsState, TextState,
};
