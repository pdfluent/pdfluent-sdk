//! SOM bridge — connects FormCalc interpreter to SOM-backed stores.
//!
//! The primary implementation in this crate targets the XFA Data DOM, but the
//! interpreter can also be bound to other resolvers such as the merged FormTree.

use xfa_dom_resolver::data_dom::{DataDom, DataNodeId};
use xfa_dom_resolver::som;

use crate::error::{FormCalcError, Result};
use crate::value::Value;

/// Generic SOM resolver used by the FormCalc interpreter.
pub trait SomResolver {
    /// Resolve a SOM path to a runtime value.
    fn resolve_path(&mut self, path: &str) -> Result<Option<Value>>;

    /// Assign a runtime value to a SOM path.
    ///
    /// Returns `true` when a matching path was updated or successfully handled.
    fn assign_path(&mut self, path: &str, value: Value) -> Result<bool>;

    /// Count the nodes matched by a SOM path.
    fn count_path_matches(&mut self, path: &str) -> Result<usize> {
        Ok(usize::from(self.resolve_path(path)?.is_some()))
    }

    /// Check whether a SOM path resolves to at least one node.
    fn exists_path(&mut self, path: &str) -> Result<bool> {
        Ok(self.count_path_matches(path)? > 0)
    }

    /// Add a node below a SOM path. Unsupported by default.
    fn add_node(&mut self, _parent_path: &str, _name: &str, _value: Value) -> Result<bool> {
        Ok(false)
    }

    /// Remove a node resolved by a SOM path. Unsupported by default.
    fn remove_node(&mut self, _path: &str) -> Result<bool> {
        Ok(false)
    }
}

/// Binding between the FormCalc interpreter and a Data DOM.
pub struct DomContext<'a> {
    /// Reference to the Data DOM.
    pub dom: &'a mut DataDom,
    /// Current context node for relative SOM paths.
    pub current_node: Option<DataNodeId>,
}

impl<'a> DomContext<'a> {
    /// Create a new DOM context.
    pub fn new(dom: &'a mut DataDom) -> Self {
        Self {
            dom,
            current_node: None,
        }
    }

    /// Create a DOM context with a current node.
    pub fn with_current(dom: &'a mut DataDom, current: DataNodeId) -> Self {
        Self {
            dom,
            current_node: Some(current),
        }
    }
}

impl SomResolver for DomContext<'_> {
    fn resolve_path(&mut self, path: &str) -> Result<Option<Value>> {
        let results = som::resolve_data_path(self.dom, path, self.current_node).map_err(|e| {
            FormCalcError::RuntimeError(format!("SOM resolution failed for '{path}': {e}"))
        })?;

        let Some(first) = results.first().copied() else {
            return Ok(None);
        };

        match self.dom.value(first) {
            Ok(v) => {
                if let Ok(n) = v.parse::<f64>() {
                    Ok(Some(Value::Number(n)))
                } else {
                    Ok(Some(Value::String(v.to_string())))
                }
            }
            Err(_) => Ok(Some(Value::Null)),
        }
    }

    fn assign_path(&mut self, path: &str, value: Value) -> Result<bool> {
        let results = som::resolve_data_path(self.dom, path, self.current_node).map_err(|e| {
            FormCalcError::RuntimeError(format!("SOM resolution failed for '{path}': {e}"))
        })?;

        let Some(first) = results.first().copied() else {
            return Ok(false);
        };

        self.dom
            .set_value(first, value.to_string_val())
            .map_err(|e| FormCalcError::RuntimeError(format!("Set failed for '{path}': {e}")))?;

        Ok(true)
    }

    fn count_path_matches(&mut self, path: &str) -> Result<usize> {
        let results = som::resolve_data_path(self.dom, path, self.current_node).map_err(|e| {
            FormCalcError::RuntimeError(format!("SOM resolution failed for '{path}': {e}"))
        })?;
        Ok(results.len())
    }

    fn add_node(&mut self, parent_path: &str, name: &str, value: Value) -> Result<bool> {
        let parents =
            som::resolve_data_path(self.dom, parent_path, self.current_node).map_err(|e| {
                FormCalcError::RuntimeError(format!(
                    "SOM resolution failed for '{parent_path}': {e}"
                ))
            })?;

        let Some(parent) = parents.first().copied() else {
            return Ok(false);
        };

        self.dom
            .create_value(parent, name, &value.to_string_val())
            .map_err(|e| FormCalcError::RuntimeError(format!("AddNode failed: {e}")))?;

        Ok(true)
    }

    fn remove_node(&mut self, path: &str) -> Result<bool> {
        let results = som::resolve_data_path(self.dom, path, self.current_node).map_err(|e| {
            FormCalcError::RuntimeError(format!("SOM resolution failed for '{path}': {e}"))
        })?;

        let Some(first) = results.first().copied() else {
            return Ok(false);
        };

        self.dom.detach(first).map_err(|e| {
            FormCalcError::RuntimeError(format!("RemoveNode failed for '{path}': {e}"))
        })?;

        Ok(true)
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
    call_som_builtin(ctx, name, args)
}

/// Try to handle a SOM-aware built-in function call.
pub fn call_som_builtin(
    resolver: &mut dyn SomResolver,
    name: &str,
    args: &[Value],
) -> Result<Option<Value>> {
    match name.to_ascii_lowercase().as_str() {
        // XFA Spec 3.3 Chapter 25 also defines URL Get/Post/Put built-ins.
        // Only intercept the overlapping names when the first argument looks
        // like a SOM path, otherwise let the generic built-in layer handle it.
        "get" if first_arg_looks_like_som_path(args) => Ok(Some(som_get(resolver, args)?)),
        "set" if first_arg_looks_like_som_path(args) => Ok(Some(som_set(resolver, args)?)),
        "exists" if first_arg_looks_like_som_path(args) => Ok(Some(som_exists(resolver, args)?)),
        "nodes" if first_arg_looks_like_som_path(args) => Ok(Some(som_nodes(resolver, args)?)),
        "addnode" if first_arg_looks_like_som_path(args) => Ok(Some(som_add_node(resolver, args)?)),
        "removenode" if first_arg_looks_like_som_path(args) => {
            Ok(Some(som_remove_node(resolver, args)?))
        }
        _ => Ok(None),
    }
}

fn first_arg_looks_like_som_path(args: &[Value]) -> bool {
    let Some(first) = args.first() else {
        return false;
    };
    let text = first.to_string_val();
    !text.contains("://")
        && (text.starts_with('$')
            || text.starts_with('!')
            || text == "this"
            || text.contains('.')
            || text.contains('['))
}

fn som_get(resolver: &mut dyn SomResolver, args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "Get".to_string(),
            expected: "1".to_string(),
            got: 0,
        });
    }

    let path = args[0].to_string_val();
    Ok(resolver.resolve_path(&path)?.unwrap_or(Value::Null))
}

fn som_set(resolver: &mut dyn SomResolver, args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(FormCalcError::ArityError {
            name: "Set".to_string(),
            expected: "2".to_string(),
            got: args.len(),
        });
    }

    let path = args[0].to_string_val();
    let value = args[1].clone();

    if resolver.assign_path(&path, value.clone())? {
        Ok(value)
    } else {
        Err(FormCalcError::RuntimeError(format!(
            "Set: no node found for path '{path}'"
        )))
    }
}

fn som_exists(resolver: &mut dyn SomResolver, args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "Exists".to_string(),
            expected: "1".to_string(),
            got: 0,
        });
    }

    let path = args[0].to_string_val();
    Ok(Value::Number(if resolver.exists_path(&path)? {
        1.0
    } else {
        0.0
    }))
}

fn som_nodes(resolver: &mut dyn SomResolver, args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "Nodes".to_string(),
            expected: "1".to_string(),
            got: 0,
        });
    }

    let path = args[0].to_string_val();
    Ok(Value::Number(resolver.count_path_matches(&path)? as f64))
}

fn som_add_node(resolver: &mut dyn SomResolver, args: &[Value]) -> Result<Value> {
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
        args[2].clone()
    } else {
        Value::Null
    };

    if resolver.add_node(&parent_path, &name, value)? {
        Ok(Value::Number(1.0))
    } else {
        Err(FormCalcError::RuntimeError(format!(
            "AddNode: no parent found for path '{parent_path}'"
        )))
    }
}

fn som_remove_node(resolver: &mut dyn SomResolver, args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "RemoveNode".to_string(),
            expected: "1".to_string(),
            got: 0,
        });
    }

    let path = args[0].to_string_val();
    Ok(Value::Number(if resolver.remove_node(&path)? {
        1.0
    } else {
        0.0
    }))
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
