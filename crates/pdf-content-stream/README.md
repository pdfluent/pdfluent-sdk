# pdf-content-stream

PDF content stream parser, state machine, and text-run locator.

This crate is the **G3 foundation** for Track G (Text Editing Fidelity). It provides:

- **Tokenizer** — byte-exact parser covering all G3 operators (BT/ET, q/Q, cm, Tf, rg/g/k, RG/G/K, Tm, Td, TD, T\*, Tj, TJ, ', ", Ts, Tz, Tw, Tc, TL)
- **State machine** — tracks current font, fill/stroke color, text matrix, text position, and graphics state stack
- **Text-run locator** — maps a logical text span (text + index) to its operator position(s) and operative state
- **Round-trip serializer** — parse → serialize produces byte-identical output for any accepted input
- **Typed error catalogue** — `ContentStreamError` variants for every failure mode; no silent drops

## Design principles

- No regex or string-search mutation
- Every byte in an accepted stream is accounted for (round-trip guarantee)
- Unknown/out-of-scope operators pass through as `ContentOp::Other` — never silently dropped
- Malformed operands return `ContentStreamError`, not panics

## Usage

```rust
use pdf_content_stream::{ContentStreamParser, ContentStateMachine};

let stream = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
let ops = ContentStreamParser::new(stream).collect_ops().unwrap();

let mut machine = ContentStateMachine::new();
for op in &ops {
    machine.apply(&op.op);
}
```

## Round-trip

```rust
use pdf_content_stream::{ContentStreamParser, serialize};

let input = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
let ops = ContentStreamParser::new(input).collect_ops().unwrap();
let output = serialize(input, &ops);
assert_eq!(input as &[u8], output.as_slice());
```

## Error handling

```rust
use pdf_content_stream::{ContentStreamParser, ContentStreamError};

let input = b"BT (unclosed string";
let result = ContentStreamParser::new(input).collect_ops();
match result {
    Err(ContentStreamError::UnterminatedString { at }) => {
        eprintln!("Unterminated string at offset {at}");
    }
    _ => {}
}
```

## Status

G3 — read-only introspection. Write paths (G4+) are not part of this crate.
