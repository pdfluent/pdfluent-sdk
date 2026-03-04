//! SOM bridge — connects FormCalc interpreter to the XFA Data DOM.
//!
//! Provides DOM-aware built-in functions that resolve SOM paths and
//! read/write Data DOM nodes from within FormCalc scripts.

use xfa_dom_resolver::data_dom::{DataDom, DataNodeId};
use xfa_dom_resolver::som;

use crate::error::{FormCalcError, Result};
use crate::value::Value;

/// Binding between the FormCalc interpreter and a Data DOM.
pub struct DomContext<'a> {
    pub dom: &'a mut DataDom,
    /// Current context node for relative SOM paths.
    pub current_node: Option<DataNodeId>,
}

impl<'a> DomContext<'a> {
    pub fn new(dom: &'a mut DataDom) -> Self {
        Self {
            dom,
            current_node: None,
        }
    }

    pub fn with_current(dom: &'a mut DataDom, current: DataNodeId) -> Self {
        Self {
            dom,
            current_node: Some(current),
        }
    }
}

/// Try to handle a DOM-aware built-in function call.
///
/// Returns `Ok(Some(value))` if the function was handled,
/// `Ok(None)` if the function name is not a DOM built-in.
pub fn call_dom_builtin(
    ctx: &mut DomContext<'_>,
    name: &str,
    args: &[Value],
) -> Result<Option<Value>> {
    match name.to_ascii_lowercase().as_str() {
        "get" => Ok(Some(dom_get(ctx, args)?)),
        "set" => Ok(Some(dom_set(ctx, args)?)),
        "exists" => Ok(Some(dom_exists(ctx, args)?)),
        "nodes" => Ok(Some(dom_nodes(ctx, args)?)),
        "addnode" => Ok(Some(dom_add_node(ctx, args)?)),
        "removenode" => Ok(Some(dom_remove_node(ctx, args)?)),
        _ => Ok(None),
    }
}

/// Get(som_path) — resolve a SOM path and return its value.
fn dom_get(ctx: &DomContext<'_>, args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "Get".to_string(),
            expected: "1".to_string(),
            got: 0,
        });
    }
    let path = args[0].to_string_val();
    let results = som::resolve_data_path(ctx.dom, &path, ctx.current_node).map_err(|e| {
        FormCalcError::RuntimeError(format!("SOM resolution failed for '{path}': {e}"))
    })?;

    if results.is_empty() {
        return Ok(Value::Null);
    }

    // Return value of first match
    match ctx.dom.value(results[0]) {
        Ok(v) => {
            // Try to parse as number
            if let Ok(n) = v.parse::<f64>() {
                Ok(Value::Number(n))
            } else {
                Ok(Value::String(v.to_string()))
            }
        }
        Err(_) => Ok(Value::Null), // DataGroup nodes have no value
    }
}

/// Set(som_path, value) — resolve a SOM path and set its value.
fn dom_set(ctx: &mut DomContext<'_>, args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(FormCalcError::ArityError {
            name: "Set".to_string(),
            expected: "2".to_string(),
            got: args.len(),
        });
    }
    let path = args[0].to_string_val();
    let value = args[1].to_string_val();

    let results = som::resolve_data_path(ctx.dom, &path, ctx.current_node).map_err(|e| {
        FormCalcError::RuntimeError(format!("SOM resolution failed for '{path}': {e}"))
    })?;

    if results.is_empty() {
        return Err(FormCalcError::RuntimeError(format!(
            "Set: no node found for path '{path}'"
        )));
    }

    ctx.dom
        .set_value(results[0], value.clone())
        .map_err(|e| FormCalcError::RuntimeError(format!("Set failed for '{path}': {e}")))?;

    Ok(Value::String(value))
}

/// Exists(som_path) — check if a SOM path resolves to any node.
fn dom_exists(ctx: &DomContext<'_>, args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "Exists".to_string(),
            expected: "1".to_string(),
            got: 0,
        });
    }
    let path = args[0].to_string_val();
    let results = som::resolve_data_path(ctx.dom, &path, ctx.current_node).unwrap_or_default();

    Ok(Value::Number(if results.is_empty() { 0.0 } else { 1.0 }))
}

/// Nodes(som_path) — return the count of nodes matching a SOM path.
fn dom_nodes(ctx: &DomContext<'_>, args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "Nodes".to_string(),
            expected: "1".to_string(),
            got: 0,
        });
    }
    let path = args[0].to_string_val();
    let results = som::resolve_data_path(ctx.dom, &path, ctx.current_node).unwrap_or_default();

    Ok(Value::Number(results.len() as f64))
}

/// AddNode(parent_path, name, value?) — create a new DataValue node.
fn dom_add_node(ctx: &mut DomContext<'_>, args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(FormCalcError::ArityError {
            name: "AddNode".to_string(),
            expected: "2-3".to_string(),
            got: args.len(),
        });
    }
    let parent_path = args[0].to_string_val();
    let name = args[1].to_string_val();
    let value = if args.len() > 2 {
        args[2].to_string_val()
    } else {
        String::new()
    };

    let parents = som::resolve_data_path(ctx.dom, &parent_path, ctx.current_node).map_err(|e| {
        FormCalcError::RuntimeError(format!("SOM resolution failed for '{parent_path}': {e}"))
    })?;

    if parents.is_empty() {
        return Err(FormCalcError::RuntimeError(format!(
            "AddNode: no parent found for path '{parent_path}'"
        )));
    }

    ctx.dom
        .create_value(parents[0], &name, &value)
        .map_err(|e| FormCalcError::RuntimeError(format!("AddNode failed: {e}")))?;

    Ok(Value::Number(1.0))
}

/// RemoveNode(som_path) — remove a node from its parent.
fn dom_remove_node(ctx: &mut DomContext<'_>, args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "RemoveNode".to_string(),
            expected: "1".to_string(),
            got: 0,
        });
    }
    let path = args[0].to_string_val();
    let results = som::resolve_data_path(ctx.dom, &path, ctx.current_node).map_err(|e| {
        FormCalcError::RuntimeError(format!("SOM resolution failed for '{path}': {e}"))
    })?;

    if results.is_empty() {
        return Ok(Value::Number(0.0));
    }

    ctx.dom
        .detach(results[0])
        .map_err(|e| FormCalcError::RuntimeError(format!("RemoveNode failed for '{path}': {e}")))?;

    Ok(Value::Number(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::Interpreter;
    use crate::lexer::tokenize;
    use crate::parser;

    fn make_dom() -> DataDom {
        let xml = r#"<data>
            <Invoice>
                <Customer>
                    <Name>Acme Corp</Name>
                    <Address>123 Main St</Address>
                </Customer>
                <Item>
                    <Description>Widget A</Description>
                    <Qty>10</Qty>
                    <Price>5.00</Price>
                </Item>
                <Item>
                    <Description>Widget B</Description>
                    <Qty>5</Qty>
                    <Price>12.50</Price>
                </Item>
                <Total>112.50</Total>
            </Invoice>
        </data>"#;
        DataDom::from_xml(xml).unwrap()
    }

    fn run_with_dom(script: &str, dom: &mut DataDom) -> Value {
        let tokens = tokenize(script).unwrap();
        let ast = parser::parse(tokens).unwrap();
        let mut interp = Interpreter::new();
        let mut ctx = DomContext::new(dom);
        interp.exec_with_dom(&ast, &mut ctx).unwrap()
    }

    #[test]
    fn get_som_value() {
        let mut dom = make_dom();
        let result = run_with_dom(r#"Get("$data.Invoice.Total")"#, &mut dom);
        assert_eq!(result, Value::Number(112.50));
    }

    #[test]
    fn get_som_string() {
        let mut dom = make_dom();
        let result = run_with_dom(r#"Get("$data.Invoice.Customer.Name")"#, &mut dom);
        assert_eq!(result, Value::String("Acme Corp".to_string()));
    }

    #[test]
    fn get_nonexistent_returns_null() {
        let mut dom = make_dom();
        let result = run_with_dom(r#"Get("$data.Invoice.Missing")"#, &mut dom);
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn set_som_value() {
        let mut dom = make_dom();
        run_with_dom(r#"Set("$data.Invoice.Total", "200.00")"#, &mut dom);
        let result = run_with_dom(r#"Get("$data.Invoice.Total")"#, &mut dom);
        assert_eq!(result, Value::Number(200.0));
    }

    #[test]
    fn exists_check() {
        let mut dom = make_dom();
        let yes = run_with_dom(r#"Exists("$data.Invoice.Total")"#, &mut dom);
        assert_eq!(yes, Value::Number(1.0));
        let no = run_with_dom(r#"Exists("$data.Invoice.Missing")"#, &mut dom);
        assert_eq!(no, Value::Number(0.0));
    }

    #[test]
    fn nodes_count() {
        let mut dom = make_dom();
        let count = run_with_dom(r#"Nodes("$data.Invoice.Item[*]")"#, &mut dom);
        assert_eq!(count, Value::Number(2.0));
    }

    #[test]
    fn add_and_read_node() {
        let mut dom = make_dom();
        run_with_dom(r#"AddNode("$data.Invoice", "Discount", "10.00")"#, &mut dom);
        let result = run_with_dom(r#"Get("$data.Invoice.Discount")"#, &mut dom);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn remove_node() {
        let mut dom = make_dom();
        let before = run_with_dom(r#"Exists("$data.Invoice.Total")"#, &mut dom);
        assert_eq!(before, Value::Number(1.0));

        run_with_dom(r#"RemoveNode("$data.Invoice.Total")"#, &mut dom);

        let after = run_with_dom(r#"Exists("$data.Invoice.Total")"#, &mut dom);
        assert_eq!(after, Value::Number(0.0));
    }

    #[test]
    fn compute_and_store() {
        let mut dom = make_dom();
        let script = r#"
            var qty = Get("$data.Invoice.Item[0].Qty")
            var price = Get("$data.Invoice.Item[0].Price")
            var line_total = qty * price
            Set("$data.Invoice.Total", line_total)
            Get("$data.Invoice.Total")
        "#;
        let result = run_with_dom(script, &mut dom);
        assert_eq!(result, Value::Number(50.0));
    }
}
