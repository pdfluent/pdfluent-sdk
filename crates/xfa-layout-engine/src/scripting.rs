//! Scripting integration — run FormCalc calculate/validate scripts on form fields.
//!
//! Implements XFA Spec 3.3 §14.3.2 event model for calculate and validate events.
//! Before layout, the engine executes calculate scripts on fields to compute
//! derived values, then optionally runs validate scripts to check constraints.
//!
//! NOTE: This module handles simple calculate/validate scripts with a flat
//! interpreter.  The more advanced dynamic scripting (initialize events,
//! SOM-based field resolution, presence toggling) lives in
//! `pdf-xfa/src/dynamic.rs` which uses the full FormTree SOM resolver.

use std::sync::{atomic::AtomicBool, Arc};

use crate::form::{FormNodeId, FormNodeType, FormTree};
use xfa_js_sandboxed::{ExecCtx, FieldValues, XfaJsRuntime};

use formcalc_interpreter::interpreter::Interpreter;
use formcalc_interpreter::lexer::tokenize;
use formcalc_interpreter::parser;
use formcalc_interpreter::value::Value;

/// Errors from script execution.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("FormCalc error in node '{node}': {message}")]
    /// Script execution error.
    Execution {
        /// Node name.
        node: String,
        /// Error message.
        message: String,
    },
    #[error("Validation failed for node '{node}': {message}")]
    /// Validation failed error.
    ValidationFailed {
        /// Node name.
        node: String,
        /// Error message.
        message: String,
    },
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
            node.calculate
                .as_ref()
                .map(|script| (FormNodeId(i), node.name.clone(), script.clone()))
        })
        .collect();

    for (id, _name, script) in calc_nodes {
        // Gracefully skip scripts that fail (e.g. unrecognized JavaScript syntax,
        // unsupported FormCalc constructs). Matches Adobe's best-effort behavior.
        let value = match eval_script(&mut interpreter, &script) {
            Ok(v) => v,
            Err(_) => continue,
        };

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

/// Execute JavaScript calculate scripts on specific form fields.
///
/// This is the `application/x-javascript` dispatch path. Callers provide an
/// explicit list of `(target_field_id, js_script_body)` pairs — the language
/// detection happens at a higher layer (the template parser / XFA merger)
/// which knows the `contentType` of each `<script>` element.
///
/// For each pair the function:
/// 1. Snapshots all field raw-values into a [`FieldValues`] store.
/// 2. Executes the JS script body via the sandboxed [`XfaJsRuntime`].
/// 3. Flushes any `rawValue` mutations back into the `FormTree`.
///
/// Errors from individual scripts are swallowed (best-effort, matching Adobe
/// behaviour). Use the returned [`ScriptResult`] to inspect which fields were
/// updated.
pub fn run_js_calculations(
    form: &mut FormTree,
    scripts: &[(FormNodeId, &str)],
    runtime: &mut XfaJsRuntime,
    cancel: Arc<AtomicBool>,
) -> Result<ScriptResult, ScriptError> {
    let mut result = ScriptResult::default();

    for (target_id, script) in scripts {
        // Snapshot all field values.
        let mut field_values = FieldValues::new();
        for node in &form.nodes {
            if let FormNodeType::Field { value } = &node.node_type {
                if !node.name.is_empty() {
                    field_values.set(&node.name, value.as_str());
                }
            }
        }

        let ctx = ExecCtx {
            fields: &mut field_values,
            cancel: Arc::clone(&cancel),
            event_new_text: None,
        };

        if let Ok(js_val) = runtime.execute_calculate(script, ctx) {
            // If the script returned a non-null/undefined value AND the
            // target field exists, use it as the new rawValue (same
            // semantics as FormCalc calculate).
            let raw = js_val.to_raw_string();
            let mut wrote_via_return = false;
            if !raw.is_empty() {
                let node = &mut form.nodes[target_id.0];
                if let FormNodeType::Field { value } = &mut node.node_type {
                    if *value != raw {
                        *value = raw;
                        result.updated_fields.push(*target_id);
                        wrote_via_return = true;
                    }
                }
            }

            // Also apply explicit xfa.form.<field>.rawValue writes.
            for (i, node) in form.nodes.iter_mut().enumerate() {
                if let FormNodeType::Field { value } = &mut node.node_type {
                    if let Some(new_val) = field_values.get(&node.name) {
                        if new_val != value.as_str() {
                            let id = FormNodeId(i);
                            // Don't double-count if we already wrote this via return.
                            if !(wrote_via_return && id == *target_id) {
                                *value = new_val.to_string();
                                if !result.updated_fields.contains(&id) {
                                    result.updated_fields.push(id);
                                }
                            }
                        }
                    }
                }
            }
        }
        // Errors are swallowed; the script is skipped (best-effort).
    }

    Ok(result)
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
            column_widths: vec![],
            col_span: 1,
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
        make_field_with_calc(
            &mut tree,
            "Greeting",
            "",
            Some("Concat(\"Hello\", \" \", \"World\")"),
        );

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
            column_widths: vec![],
            col_span: 1,
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
            column_widths: vec![],
            col_span: 1,
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
            column_widths: vec![],
            col_span: 1,
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
        make_field_with_calc(&mut tree, "Tax", "0", Some("Round(100 * 0.21, 2)"));

        let result = run_calculations(&mut tree).unwrap();
        assert_eq!(result.updated_fields.len(), 1);
        if let FormNodeType::Field { value } = &tree.get(result.updated_fields[0]).node_type {
            assert_eq!(value, "21");
        }
    }
}
