#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the FormCalc lexer, parser, and interpreter.
    // FormCalc scripts come from untrusted PDF forms — must never panic.
    if let Ok(source) = std::str::from_utf8(data) {
        if let Ok(tokens) = formcalc_interpreter::lexer::tokenize(source) {
            if let Ok(ast) = formcalc_interpreter::parser::parse(tokens) {
                let mut interp = formcalc_interpreter::interpreter::Interpreter::new();
                // Limit execution to prevent infinite loops in fuzzed scripts
                let _ = interp.exec(&ast);
            }
        }
    }
});
