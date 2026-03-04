//! SOM (Scripting Object Model) path parser and resolver.
//!
//! Implements XFA 3.3 §3: SOM expressions for navigating XFA DOMs.
//!
//! Grammar:
//! ```text
//! som_expr  := segment ('.' segment)*
//! segment   := (name | '#' class) index?
//! name      := XFA_NAME
//! class     := XFA_NAME
//! index     := '[' (integer | '*') ']'
//! ```

use crate::data_dom::{DataDom, DataNodeId};
use crate::error::{Result, XfaDomError};

/// A parsed SOM expression segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SomSegment {
    /// The name or class reference.
    pub selector: SomSelector,
    /// Optional index: `[n]`, `[*]`.
    pub index: SomIndex,
}

/// What a SOM segment selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SomSelector {
    /// Select by name: `fieldName`
    Name(String),
    /// Select by class: `#subform`
    Class(String),
    /// Select all children: `*`
    AllChildren,
}

/// Index specifier on a SOM segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SomIndex {
    /// No index specified — defaults to first match `[0]`.
    None,
    /// Explicit index: `[n]`.
    Specific(usize),
    /// All same-named siblings: `[*]`.
    All,
}

/// A fully parsed SOM expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SomExpression {
    /// Whether this is a shortcut-rooted expression.
    pub root: SomRoot,
    /// The path segments after the root.
    pub segments: Vec<SomSegment>,
}

/// The root of a SOM expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SomRoot {
    /// `xfa` or `$xfa` — absolute from XFA root
    Xfa,
    /// `$data` → `xfa.datasets.data`
    Data,
    /// `$template` → `xfa.template`
    Template,
    /// `$form` → `xfa.form`
    Form,
    /// `$record` → current data record
    Record,
    /// `$` — current container (relative)
    CurrentContainer,
    /// Unqualified — search from current scope outward
    Unqualified,
}

/// Parse a SOM expression string into structured form.
pub fn parse_som(input: &str) -> Result<SomExpression> {
    let input = input.trim();
    if input.is_empty() {
        return Err(XfaDomError::SomParseError {
            pos: 0,
            message: "empty SOM expression".to_string(),
        });
    }

    let (root, remainder) = parse_root(input);
    let segments = if remainder.is_empty() {
        Vec::new()
    } else {
        parse_segments(remainder)?
    };

    Ok(SomExpression { root, segments })
}

fn parse_root(input: &str) -> (SomRoot, &str) {
    // Check for shortcuts
    if let Some(rest) = input.strip_prefix("$data") {
        return (SomRoot::Data, strip_leading_dot(rest));
    }
    if let Some(rest) = input.strip_prefix("$template") {
        return (SomRoot::Template, strip_leading_dot(rest));
    }
    if let Some(rest) = input.strip_prefix("$form") {
        return (SomRoot::Form, strip_leading_dot(rest));
    }
    if let Some(rest) = input.strip_prefix("$record") {
        return (SomRoot::Record, strip_leading_dot(rest));
    }
    if let Some(rest) = input.strip_prefix("$xfa") {
        return (SomRoot::Xfa, strip_leading_dot(rest));
    }
    if let Some(rest) = input.strip_prefix("$.") {
        return (SomRoot::CurrentContainer, rest);
    }
    if input == "$" {
        return (SomRoot::CurrentContainer, "");
    }
    if let Some(rest) = input.strip_prefix("xfa.") {
        return (SomRoot::Xfa, rest);
    }
    if input == "xfa" {
        return (SomRoot::Xfa, "");
    }

    // Unqualified reference
    (SomRoot::Unqualified, input)
}

fn strip_leading_dot(s: &str) -> &str {
    s.strip_prefix('.').unwrap_or(s)
}

fn parse_segments(input: &str) -> Result<Vec<SomSegment>> {
    let mut segments = Vec::new();
    let mut remaining = input;

    while !remaining.is_empty() {
        let (segment, rest) = parse_one_segment(remaining)?;
        segments.push(segment);
        remaining = rest;
    }

    Ok(segments)
}

fn parse_one_segment(input: &str) -> Result<(SomSegment, &str)> {
    let (selector, rest) = parse_selector(input)?;
    let (index, rest) = parse_index(rest)?;

    // Strip trailing dot separator
    let rest = rest.strip_prefix('.').unwrap_or(rest);

    Ok((SomSegment { selector, index }, rest))
}

fn parse_selector(input: &str) -> Result<(SomSelector, &str)> {
    if let Some(rest) = input.strip_prefix('#') {
        // Class reference: #className
        let end = rest.find(['.', '[']).unwrap_or(rest.len());
        if end == 0 {
            return Err(XfaDomError::SomParseError {
                pos: 1,
                message: "expected class name after '#'".to_string(),
            });
        }
        Ok((SomSelector::Class(rest[..end].to_string()), &rest[end..]))
    } else if let Some(rest) = input.strip_prefix('*') {
        // Wildcard: all children
        Ok((SomSelector::AllChildren, rest))
    } else {
        // Name
        let end = input.find(['.', '[']).unwrap_or(input.len());
        if end == 0 {
            return Err(XfaDomError::SomParseError {
                pos: 0,
                message: "expected name".to_string(),
            });
        }
        Ok((SomSelector::Name(input[..end].to_string()), &input[end..]))
    }
}

fn parse_index(input: &str) -> Result<(SomIndex, &str)> {
    if let Some(rest) = input.strip_prefix('[') {
        if let Some(rest) = rest.strip_prefix('*') {
            let rest = rest
                .strip_prefix(']')
                .ok_or_else(|| XfaDomError::SomParseError {
                    pos: 0,
                    message: "expected ']' after '[*'".to_string(),
                })?;
            Ok((SomIndex::All, rest))
        } else {
            // Parse integer
            let bracket_end = rest.find(']').ok_or_else(|| XfaDomError::SomParseError {
                pos: 0,
                message: "expected ']'".to_string(),
            })?;
            let idx_str = &rest[..bracket_end];
            let idx: usize = idx_str.parse().map_err(|_| XfaDomError::SomParseError {
                pos: 0,
                message: format!("invalid index: '{idx_str}'"),
            })?;
            Ok((SomIndex::Specific(idx), &rest[bracket_end + 1..]))
        }
    } else {
        Ok((SomIndex::None, input))
    }
}

/// Resolve a SOM expression against a Data DOM.
///
/// Returns all matching node IDs. For `[*]` expressions, returns all same-named siblings.
/// For specific indexes or no index, returns at most one node.
pub fn resolve_data_som(
    dom: &DataDom,
    expr: &SomExpression,
    current: Option<DataNodeId>,
) -> Result<Vec<DataNodeId>> {
    let start = match &expr.root {
        SomRoot::Data | SomRoot::Xfa => {
            // Start from data root
            dom.root().ok_or_else(|| XfaDomError::SomResolutionFailed {
                path: format!("{:?}", expr),
            })?
        }
        SomRoot::CurrentContainer | SomRoot::Unqualified => {
            current.ok_or_else(|| XfaDomError::SomResolutionFailed {
                path: "no current container for relative SOM expression".to_string(),
            })?
        }
        _ => {
            return Err(XfaDomError::SomResolutionFailed {
                path: format!("unsupported root type {:?} for Data DOM", expr.root),
            });
        }
    };

    if expr.segments.is_empty() {
        return Ok(vec![start]);
    }

    let mut current_nodes = vec![start];

    for segment in &expr.segments {
        let mut next_nodes = Vec::new();

        for &node_id in &current_nodes {
            let matches = match &segment.selector {
                SomSelector::Name(name) => dom.children_by_name(node_id, name),
                SomSelector::AllChildren => dom.children(node_id).to_vec(),
                SomSelector::Class(_) => {
                    // Class references are mainly for Template/Form DOMs
                    // In Data DOM, #dataGroup and #dataValue can be used
                    dom.children(node_id).to_vec()
                }
            };

            // For AllChildren selector (`.*`), always return all matches
            if segment.selector == SomSelector::AllChildren {
                next_nodes.extend_from_slice(&matches);
            } else {
                match segment.index {
                    SomIndex::None if matches.is_empty() => {}
                    SomIndex::None => {
                        if let Some(&first) = matches.first() {
                            next_nodes.push(first);
                        }
                    }
                    SomIndex::Specific(idx) => {
                        if let Some(&node) = matches.get(idx) {
                            next_nodes.push(node);
                        }
                    }
                    SomIndex::All => {
                        next_nodes.extend_from_slice(&matches);
                    }
                }
            }
        }

        if next_nodes.is_empty() {
            return Ok(Vec::new());
        }
        current_nodes = next_nodes;
    }

    Ok(current_nodes)
}

/// Resolve a SOM expression and return the first match, or error.
pub fn resolve_data_som_single(
    dom: &DataDom,
    expr: &SomExpression,
    current: Option<DataNodeId>,
) -> Result<DataNodeId> {
    let results = resolve_data_som(dom, expr, current)?;
    results
        .into_iter()
        .next()
        .ok_or_else(|| XfaDomError::SomResolutionFailed {
            path: format!("{:?}", expr),
        })
}

/// Convenience: parse and resolve a SOM string against a Data DOM.
pub fn resolve_data_path(
    dom: &DataDom,
    path: &str,
    current: Option<DataNodeId>,
) -> Result<Vec<DataNodeId>> {
    let expr = parse_som(path)?;
    resolve_data_som(dom, &expr, current)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Parser tests ----

    #[test]
    fn parse_simple_path() {
        let expr = parse_som("Receipt.Tax").unwrap();
        assert_eq!(expr.root, SomRoot::Unqualified);
        assert_eq!(expr.segments.len(), 2);
        assert_eq!(
            expr.segments[0].selector,
            SomSelector::Name("Receipt".to_string())
        );
        assert_eq!(
            expr.segments[1].selector,
            SomSelector::Name("Tax".to_string())
        );
    }

    #[test]
    fn parse_data_shortcut() {
        let expr = parse_som("$data.Receipt.Tax").unwrap();
        assert_eq!(expr.root, SomRoot::Data);
        assert_eq!(expr.segments.len(), 2);
    }

    #[test]
    fn parse_indexed() {
        let expr = parse_som("Detail[2].Description").unwrap();
        assert_eq!(expr.segments[0].index, SomIndex::Specific(2));
        assert_eq!(expr.segments[1].index, SomIndex::None);
    }

    #[test]
    fn parse_wildcard_index() {
        let expr = parse_som("Detail[*]").unwrap();
        assert_eq!(expr.segments[0].index, SomIndex::All);
    }

    #[test]
    fn parse_class_ref() {
        let expr = parse_som("#subform[1]").unwrap();
        assert_eq!(
            expr.segments[0].selector,
            SomSelector::Class("subform".to_string())
        );
        assert_eq!(expr.segments[0].index, SomIndex::Specific(1));
    }

    #[test]
    fn parse_all_children() {
        let expr = parse_som("Receipt.*").unwrap();
        assert_eq!(expr.segments[1].selector, SomSelector::AllChildren);
    }

    #[test]
    fn parse_current_container() {
        let expr = parse_som("$.Tax").unwrap();
        assert_eq!(expr.root, SomRoot::CurrentContainer);
        assert_eq!(expr.segments.len(), 1);
    }

    #[test]
    fn parse_xfa_root() {
        let expr = parse_som("xfa.datasets.data").unwrap();
        assert_eq!(expr.root, SomRoot::Xfa);
        assert_eq!(expr.segments.len(), 2);
    }

    // ---- Resolver tests ----

    fn make_test_dom() -> DataDom {
        let xml = r#"<data>
            <Receipt>
                <Detail>
                    <Description>Widget A</Description>
                    <Units>2</Units>
                    <Unit_Price>10.00</Unit_Price>
                    <Total_Price>20.00</Total_Price>
                </Detail>
                <Detail>
                    <Description>Widget B</Description>
                    <Units>5</Units>
                    <Unit_Price>3.00</Unit_Price>
                    <Total_Price>15.00</Total_Price>
                </Detail>
                <Sub_Total>35.00</Sub_Total>
                <Tax>2.80</Tax>
                <Total_Price>37.80</Total_Price>
            </Receipt>
        </data>"#;
        DataDom::from_xml(xml).unwrap()
    }

    #[test]
    fn resolve_simple_path() {
        let dom = make_test_dom();
        let results = resolve_data_path(&dom, "$data.Receipt.Tax", None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(dom.value(results[0]).unwrap(), "2.80");
    }

    #[test]
    fn resolve_indexed_path() {
        let dom = make_test_dom();
        let results = resolve_data_path(&dom, "$data.Receipt.Detail[1].Description", None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(dom.value(results[0]).unwrap(), "Widget B");
    }

    #[test]
    fn resolve_first_by_default() {
        let dom = make_test_dom();
        // No index = [0]
        let results = resolve_data_path(&dom, "$data.Receipt.Detail.Description", None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(dom.value(results[0]).unwrap(), "Widget A");
    }

    #[test]
    fn resolve_all_siblings() {
        let dom = make_test_dom();
        let results = resolve_data_path(&dom, "$data.Receipt.Detail[*]", None).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn resolve_all_children() {
        let dom = make_test_dom();
        let results = resolve_data_path(&dom, "$data.Receipt.*", None).unwrap();
        // Detail, Detail, Sub_Total, Tax, Total_Price = 5 children
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn resolve_nonexistent_returns_empty() {
        let dom = make_test_dom();
        let results = resolve_data_path(&dom, "$data.Receipt.NonExistent", None).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn resolve_nested_wildcard() {
        let dom = make_test_dom();
        // All children of first Detail
        let results = resolve_data_path(&dom, "$data.Receipt.Detail[0].*", None).unwrap();
        assert_eq!(results.len(), 4); // Description, Units, Unit_Price, Total_Price
    }

    #[test]
    fn resolve_relative_from_current() {
        let dom = make_test_dom();
        let receipt = dom.children_by_name(dom.root().unwrap(), "Receipt")[0];
        let results = resolve_data_path(&dom, "$.Tax", Some(receipt)).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(dom.value(results[0]).unwrap(), "2.80");
    }

    #[test]
    fn resolve_unqualified_from_current() {
        let dom = make_test_dom();
        let receipt = dom.children_by_name(dom.root().unwrap(), "Receipt")[0];
        let results = resolve_data_path(&dom, "Tax", Some(receipt)).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(dom.value(results[0]).unwrap(), "2.80");
    }
}
