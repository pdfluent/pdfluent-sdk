//! Regression tests for FormCalc if/then/else parsing (issue #242).
//!
//! Based on scripts extracted from corpus/f211.pdf (IRS Form 211).
//! These verify the parser handles the `if (condition)\n\tthen` pattern
//! with SOM member access in conditions and deep SOM path method calls.

use formcalc_interpreter::interpreter::Interpreter;
use formcalc_interpreter::lexer::tokenize;
use formcalc_interpreter::parser;
use formcalc_interpreter::value::Value;

fn run(src: &str) -> Value {
    let tokens = tokenize(src).unwrap_or_else(|e| panic!("Tokenize failed for: {src}\n{e}"));
    let ast = parser::parse(tokens).unwrap_or_else(|e| panic!("Parse failed for: {src}\n{e}"));
    let mut interp = Interpreter::new();
    interp
        .exec(&ast)
        .unwrap_or_else(|e| panic!("Exec failed for: {src}\n{e}"))
}

// ============================================================
// Issue #242 — original 3 failing tests
// ============================================================

/// Full calculate script from f211.pdf — if/then/else/endif with
/// member access conditions and deep SOM path method calls.
/// Previously failed: ParseError "unexpected token: Then"
#[test]
fn test_f211_full_calculate_script() {
    let script = "\
//Hide/Show the Manually version when this is/is not checked
if (optionManually.rawValue == 1)
\tthen
\telectrically.addAdditionalTaxpayers.ifYes.taxpayerInformation.Row1.instanceManager.setInstances(1)
\telectrically.taxYearAmount.Row1.instanceManager.setInstances(1)
\txfa.host.resetData(electronically.somExpression)
\telectronically.presence = \"hidden\"
\tmanually.presence = \"visible\"
\telse
\telectronically.presence = \"visible\"
\txfa.host.resetData(manually.somExpression)
\tmanually.presence = \"hidden\"
endif";
    let result = run(script);
    assert_eq!(result, Value::String("hidden".to_string()));
}

/// Multiple consecutive if/then/else/endif blocks with `|` (OR) in conditions.
/// Previously failed: ParseError "unexpected token: Then"
#[test]
fn test_f211_other_scripts() {
    let script = "\
//Hide/Show currentFormerEmployee statement
if (currentEmployee.rawValue == 1 | formerEmployee.rawValue == 1)
\tthen
\tcurrentFormerEmployee.presence = \"visible\"
\telse
\tcurrentFormerEmployee.positionJobResponsibilities.rawValue = null
\tcurrentFormerEmployee.presence = \"hidden\"
endif

//Hide/Show attorneyQuestions subform
if (attorney.rawValue == 1)
\tthen
\tattorneyQuestions.presence = \"visible\"
\telse
\txfa.host.resetData(attorneyQuestions.somExpression)
\tattorneyQuestions.presence = \"hidden\"
endif

//Hide/Show cpaQuestions subform
if (cpa.rawValue == 1)
\tthen
\tcpaQuestions.presence = \"visible\"
\telse
\txfa.host.resetData(cpaQuestions.somExpression)
\tcpaQuestions.presence = \"hidden\"
endif";
    let result = run(script);
    assert_eq!(result, Value::String("hidden".to_string()));
}

/// Page calculation: member access assignment with method call RHS.
/// Previously failed: Runtime error "invalid assignment target"
#[test]
fn test_f211_page_calc() {
    let script = "this.rawValue = xfa.layout.page(this)\nthis.rawValue";
    let result = run(script);
    assert!(matches!(result, Value::Null));
}

// ============================================================
// Acceptance criteria — explicit coverage
// ============================================================

/// AC: `if <expr> then <stmts> endif` parsed correct.
#[test]
fn test_f211_if_then_endif() {
    let script = "\
if (field.rawValue == 1)
\tthen
\tresult.presence = \"visible\"
endif";
    // No error = parses and executes correctly.
    run(script);
}

/// AC: `if <expr> then <stmts> else <stmts> endif` parsed correct.
#[test]
fn test_f211_if_then_else_endif() {
    let script = "\
var x = 1
if (x == 1)
\tthen
\tvar r = \"yes\"
\telse
\tvar r = \"no\"
endif
r";
    assert_eq!(run(script), Value::String("yes".to_string()));
}

/// AC: `if <expr> then <stmts> elseif <expr> then <stmts> endif`.
#[test]
fn test_f211_if_elseif_endif() {
    let script = "\
var x = 2
if (x == 1)
\tthen
\tvar r = \"one\"
elseif (x == 2)
\tthen
\tvar r = \"two\"
else
\tvar r = \"other\"
endif
r";
    assert_eq!(run(script), Value::String("two".to_string()));
}

/// AC: Member access assignment (`this.rawValue = ...`) works.
#[test]
fn test_f211_member_access_assignment() {
    let script = "\
field.rawValue = 42
field.rawValue";
    assert_eq!(run(script), Value::Number(42.0));
}

/// `else if` (two-word variant) with nested endif — from f211 Section 7.
#[test]
fn test_f211_else_if_two_word() {
    let script = "\
var x = 2
if (x == 1)
\tthen
\tvar r = \"one\"
\telse if (x == 2)
\tthen
\tvar r = \"two\"
\telse
\tvar r = \"other\"
endif
endif
r";
    assert_eq!(run(script), Value::String("two".to_string()));
}

/// `if (expr)` backtracking: must not confuse if-statement with If() function.
#[test]
fn test_f211_if_paren_backtrack() {
    // If(cond, then, else) = function call with 3 args
    assert_eq!(run("If(1, 42, 99)"), Value::Number(42.0));
    // if (cond) then...endif = statement with parenthesized condition
    let script = "\
if (1)
\tthen
\tvar r = 42
endif
r";
    assert_eq!(run(script), Value::Number(42.0));
}
