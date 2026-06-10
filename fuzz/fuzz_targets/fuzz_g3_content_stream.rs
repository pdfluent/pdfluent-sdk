#![no_main]

use libfuzzer_sys::fuzz_target;
use pdf_content_stream::{ContentStreamParser, ContentStateMachine, serialize};

fuzz_target!(|data: &[u8]| {
    // Parse in lenient mode so we can test round-trip on partial inputs.
    let (ops, _err) = ContentStreamParser::new(data).collect_ops_lenient();

    // Round-trip must be byte-identical for the parsed prefix.
    if !ops.is_empty() {
        // The ops cover only the successfully-parsed prefix of `data`.
        let covered_len = ops.last().map(|op| op.byte_end).unwrap_or(0);
        let covered = &data[..covered_len];
        // Reconstruct the parsed prefix.
        let reconstructed = serialize(covered, &ops);
        assert_eq!(
            covered,
            reconstructed.as_slice(),
            "round-trip failed on parsed prefix"
        );
    }

    // Drive the state machine — must not panic.
    let mut machine = ContentStateMachine::new();
    for op in &ops {
        machine.apply(&op.op);
    }

    // Strict parse — must not panic even on error.
    let _ = ContentStreamParser::new(data).collect_ops();
});
