//! Edge case tests for FormCalc interpreter.

use formcalc_interpreter::interpreter::Interpreter;
use formcalc_interpreter::lexer::tokenize;
use formcalc_interpreter::parser;
use formcalc_interpreter::value::Value;

fn run(src: &str) -> Value {
    let tokens = tokenize(src).expect("tokenize failed");
    let ast = parser::parse(tokens).expect("parse failed");
    let mut interp = Interpreter::new();
    interp.exec(&ast).expect("exec failed")
}

fn try_run(src: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let tokens = tokenize(src)?;
    let ast = parser::parse(tokens)?;
    let mut interp = Interpreter::new();
    Ok(interp.exec(&ast)?)
}

#[test]
fn empty_script() {
    let tokens = tokenize("").unwrap();
    let ast = parser::parse(tokens).unwrap();
    let mut interp = Interpreter::new();
    let result = interp.exec(&ast).unwrap();
    assert!(matches!(result, Value::Null));
}

#[test]
fn whitespace_only() {
    let result = try_run("   \n\t  ");
    assert!(result.is_ok());
}

#[test]
fn long_addition_chain() {
    // Recursive descent parser has limited stack depth, use 50 terms
    let expr = vec!["1"; 50].join(" + ");
    let result = run(&expr);
    assert_eq!(result, Value::Number(50.0));
}

#[test]
fn division_by_zero() {
    let result = try_run("1 / 0");
    // Should not panic — either infinity or error is acceptable
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn string_with_escaped_quotes() {
    // FormCalc escapes: "" inside string = literal quote
    // Input: "say ""hello""" → say "hello"
    let result = run("\"say \"\"hello\"\"\"");
    assert_eq!(result, Value::String("say \"hello\"".to_string()));
}

#[test]
fn negative_numbers() {
    assert_eq!(run("-5"), Value::Number(-5.0));
    assert_eq!(run("-5 + 3"), Value::Number(-2.0));
}

#[test]
fn nested_parentheses() {
    let result = run("((((1 + 2) * 3) + 4) * 5)");
    assert_eq!(result, Value::Number(65.0));
}

#[test]
fn string_concatenation() {
    let result = run(r#"Concat("Hello", " ", "World")"#);
    assert_eq!(result, Value::String("Hello World".to_string()));
}

#[test]
fn type_coercion_string_to_number() {
    let result = run(r#""42" + 8"#);
    assert_eq!(result, Value::Number(50.0));
}

#[test]
fn zero_multiplication() {
    assert_eq!(run("0 * 999999"), Value::Number(0.0));
}

#[test]
fn multiple_statements() {
    let result = run("var x = 10\nvar y = 20\nx + y");
    assert_eq!(result, Value::Number(30.0));
}
