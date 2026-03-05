//! Scripting integration — run FormCalc calculate/validate scripts on form fields.
//!
//! Implements the XFA §14.3.2 event model for calculate and validate events.
//! Before layout, the engine executes calculate scripts on fields to compute
//! derived values, then optionally runs validate scripts to check constraints.

use crate::form::{FormNodeId, FormNodeType, FormTree};

use formcalc_interpreter::interpreter::Interpreter;
use formcalc_interpreter::lexer::tokenize;
use formcalc_interpreter::parser;
use formcalc_interpreter::value::Value;

/// Errors from script execution.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("FormCalc error in node '{node}': {message}")]
    Execution { node: String, message: String },
    #[error("Validation failed for node '{node}': {message}")]
    ValidationFailed { node: String, message: String },
}

/// Result of running all scripts on a form tree.
#[derive(Debug, Default)]
pub struct ScriptResult {
    /// Fields whose values were updated by calculate scripts.
    pub updated_fields: Vec<FormNodeId>,
    /// Validation failures (node id and error message).
    pub validation_errors: Vec<(FormNodeId, String)>,
}

/// Execute all calculate scripts in the form tree, updating field values.
///
/// Walks the tree depth-first. For each Field node with a `calculate` script,
/// evaluates the script and sets the field's value to the result.
/// Returns a summary of which fields were updated.
pub fn run_calculations(form: &mut FormTree) -> Result<ScriptResult, ScriptError> {
    let mut result = ScriptResult::default();
    let mut interpreter = Interpreter::new();

    // Collect all nodes with calculate scripts first (to avoid borrow issues)
    let calc_nodes: Vec<(FormNodeId, String, String)> = form
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| {
            node.calculate.as_ref().map(|script| {
                (FormNodeId(i), node.name.clone(), script.clone())
            })
        })
        .collect();

    for (id, name, script) in calc_nodes {
        let value = eval_script(&mut interpreter, &script).map_err(|e| ScriptError::Execution {
            node: name.clone(),
            message: e,
        })?;

        // Convert the FormCalc result to a string and set the field value
        let value_str = value_to_string(&value);

        let node = form.get_mut(id);
        if let FormNodeType::Field { ref mut value } = node.node_type {
            if *value != value_str {
                *value = value_str;
                result.updated_fields.push(id);
            }
        }
    }

    Ok(result)
}

/// Execute all validate scripts in the form tree, collecting failures.
///
/// For each Field node with a `validate` script, evaluates the script.
/// A validation passes if the result is truthy (non-zero number, non-empty string).
pub fn run_validations(form: &FormTree) -> Result<ScriptResult, ScriptError> {
    let mut result = ScriptResult::default();
    let mut interpreter = Interpreter::new();

    for (i, node) in form.nodes.iter().enumerate() {
        if let Some(ref script) = node.validate {
            let val =
                eval_script(&mut interpreter, script).map_err(|e| ScriptError::Execution {
                    node: node.name.clone(),
                    message: e,
                })?;

            if !is_truthy(&val) {
                let msg = format!(
                    "Validation script returned falsy value: {}",
                    value_to_string(&val)
                );
                result.validation_errors.push((FormNodeId(i), msg));
            }
        }
    }

    Ok(result)
}

/// Run calculate scripts, then layout. Convenience wrapper for the common flow.
///
/// Returns the script result so callers can inspect which fields changed
/// and whether validations passed.
pub fn prepare_form(form: &mut FormTree) -> Result<ScriptResult, ScriptError> {
    let mut calc_result = run_calculations(form)?;
    let val_result = run_validations(form)?;
    calc_result.validation_errors = val_result.validation_errors;
    Ok(calc_result)
}

/// Evaluate a FormCalc script string and return the result value.
fn eval_script(interpreter: &mut Interpreter, script: &str) -> Result<Value, String> {
    let tokens = tokenize(script).map_err(|e| format!("Tokenize error: {e}"))?;
    let ast = parser::parse(tokens).map_err(|e| format!("Parse error: {e}"))?;
    interpreter
        .exec(&ast)
        .map_err(|e| format!("Runtime error: {e}"))
}

/// Convert a FormCalc Value to a display string.
fn value_to_string(val: &Value) -> String {
    match val {
        Value::Number(n) => {
            // Format integers without decimal point
            if *n == n.floor() && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
    }
}

/// Check if a FormCalc value is truthy (for validation results).
fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Number(n) => *n != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Null => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::{FormNode, Occur};
    use crate::text::FontMetrics;
    use crate::types::{BoxModel, LayoutStrategy};

    fn make_field_with_calc(
        tree: &mut FormTree,
        name: &str,
        initial_value: &str,
        calculate: Option<&str>,
    ) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Field {
                value: initial_value.to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(20.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: calculate.map(|s| s.to_string()),
            validate: None,
        })
    }

    #[test]
    fn calculate_script_updates_field_value() {
        let mut tree = FormTree::new();
        make_field_with_calc(&mut tree, "Total", "0", Some("10 + 20"));

        let result = run_calculations(&mut tree).unwrap();

        assert_eq!(result.updated_fields.len(), 1);
        if let FormNodeType::Field { value } = &tree.get(result.updated_fields[0]).node_type {
            assert_eq!(value, "30");
        } else {
            panic!("Expected Field node");
        }
    }

    #[test]
    fn calculate_script_string_result() {
        let mut tree = FormTree::new();
        make_field_with_calc(&mut tree, "Greeting", "", Some("Concat(\"Hello\", \" \", \"World\")"));

        let result = run_calculations(&mut tree).unwrap();

        assert_eq!(result.updated_fields.len(), 1);
        if let FormNodeType::Field { value } = &tree.get(result.updated_fields[0]).node_type {
            assert_eq!(value, "Hello World");
        }
    }

    #[test]
    fn no_update_when_value_unchanged() {
        let mut tree = FormTree::new();
        make_field_with_calc(&mut tree, "Same", "42", Some("42"));

        let result = run_calculations(&mut tree).unwrap();

        assert_eq!(result.updated_fields.len(), 0); // Value didn't change
    }

    #[test]
    fn fields_without_scripts_are_untouched() {
        let mut tree = FormTree::new();
        make_field_with_calc(&mut tree, "Static", "original", None);

        let result = run_calculations(&mut tree).unwrap();

        assert_eq!(result.updated_fields.len(), 0);
        if let FormNodeType::Field { value } = &tree.get(FormNodeId(0)).node_type {
            assert_eq!(value, "original");
        }
    }

    #[test]
    fn validation_passes_for_truthy() {
        let mut tree = FormTree::new();
        let id = tree.add_node(FormNode {
            name: "Amount".to_string(),
            node_type: FormNodeType::Field {
                value: "100".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(20.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: Some("1".to_string()), // truthy
        });
        let _ = id;

        let result = run_validations(&tree).unwrap();
        assert!(result.validation_errors.is_empty());
    }

    #[test]
    fn validation_fails_for_falsy() {
        let mut tree = FormTree::new();
        tree.add_node(FormNode {
            name: "Required".to_string(),
            node_type: FormNodeType::Field {
                value: "".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(20.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: Some("0".to_string()), // falsy
        });

        let result = run_validations(&tree).unwrap();
        assert_eq!(result.validation_errors.len(), 1);
    }

    #[test]
    fn prepare_form_runs_both() {
        let mut tree = FormTree::new();
        // Field with calculate script
        make_field_with_calc(&mut tree, "Sum", "0", Some("5 * 3"));
        // Field with validation
        tree.add_node(FormNode {
            name: "Check".to_string(),
            node_type: FormNodeType::Field {
                value: "ok".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(20.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: Some("0".to_string()), // will fail
        });

        let result = prepare_form(&mut tree).unwrap();

        // Calculate ran
        assert_eq!(result.updated_fields.len(), 1);
        if let FormNodeType::Field { value } = &tree.get(FormNodeId(0)).node_type {
            assert_eq!(value, "15");
        }
        // Validation ran
        assert_eq!(result.validation_errors.len(), 1);
    }

    #[test]
    fn complex_calculation() {
        let mut tree = FormTree::new();
        make_field_with_calc(
            &mut tree,
            "Tax",
            "0",
            Some("Round(100 * 0.21, 2)"),
        );

        let result = run_calculations(&mut tree).unwrap();
        assert_eq!(result.updated_fields.len(), 1);
        if let FormNodeType::Field { value } = &tree.get(result.updated_fields[0]).node_type {
            assert_eq!(value, "21");
        }
    }
}
