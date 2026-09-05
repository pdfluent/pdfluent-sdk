// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `call_som_builtin` decides who handles `Get`, `Set` and `Exists` (#152).
//!
//! FormCalc knows those names twice over. XFA 3.3 chapter 25 defines them as
//! **URL** functions (`Get("https://…")`), and the same names also serve the
//! **SOM** model (`Get("form1.field")`). Which of the two applies depends on
//! the shape of the first argument.
//!
//! That decision was untested. Getting it wrong is silent: a script fetching a
//! SOM path takes the URL branch instead, or the other way round, and either
//! way an answer comes back -- just the wrong one.

use formcalc_interpreter::error::Result;
use formcalc_interpreter::som_bridge::{call_som_builtin, SomResolver};
use formcalc_interpreter::value::Value;

/// A resolver that remembers what it was asked.
#[derive(Default)]
struct Minutes {
    requested: Vec<String>,
    assigned: Vec<(String, String)>,
    answer: Option<Value>,
}

impl SomResolver for Minutes {
    fn resolve_path(&mut self, path: &str) -> Result<Option<Value>> {
        self.requested.push(path.to_string());
        Ok(self.answer.clone())
    }

    fn assign_path(&mut self, path: &str, value: Value) -> Result<bool> {
        self.assigned
            .push((path.to_string(), value.to_string_val()));
        Ok(true)
    }
}

fn call(name: &str, args: &[Value]) -> (Option<Value>, Minutes) {
    let mut resolver = Minutes {
        answer: Some(Value::String("value".into())),
        ..Default::default()
    };
    let out = call_som_builtin(&mut resolver, name, args).expect("no error");
    (out, resolver)
}

/// A SOM path is handled by the SOM branch.
#[test]
fn a_som_path_goes_to_the_som_branch() {
    for path in ["form1.field", "$data.record", "!field", "this", "list[0]"] {
        let (out, minutes) = call("Get", &[Value::String(path.into())]);
        assert!(
            out.is_some(),
            "{path:?} was not recognised as a SOM path; then it falls through to the URL branch"
        );
        assert_eq!(minutes.requested, vec![path.to_string()]);
    }
}

/// A URL stays away from the SOM branch.
///
/// This is the side that goes wrong most easily: `Get("https://…")` must fall
/// through to the general layer, not be looked up as a path.
#[test]
fn a_url_falls_through_to_the_general_layer() {
    for url in ["https://example.com/a.xml", "http://x/y", "ftp://host/path"] {
        let (out, minutes) = call("Get", &[Value::String(url.into())]);
        assert!(
            out.is_none(),
            "{url:?} was handled by the SOM branch; then Get does something other than the script asks"
        );
        assert!(minutes.requested.is_empty());
    }
}

/// And something that looks like neither is not a path either.
#[test]
fn a_bare_name_is_not_a_som_path() {
    // No dot, no bracket, no `$` or `!` -- by the shape the bridge uses that is
    // not a path.
    let (out, _) = call("Get", &[Value::String("plain".into())]);
    assert!(out.is_none());
}

/// Without arguments it falls through; there is nothing to decide on.
#[test]
fn without_arguments_it_falls_through() {
    let (out, _) = call("Get", &[]);
    assert!(out.is_none());
}

/// The name is compared case-insensitively.
#[test]
fn the_name_is_case_insensitive() {
    for name in ["Get", "get", "GET", "gEt"] {
        let (out, _) = call(name, &[Value::String("form1.field".into())]);
        assert!(out.is_some(), "{name:?} was not recognised");
    }
}

/// `Set` writes through the resolver.
#[test]
fn set_writes_the_path_and_the_value() {
    let mut resolver = Minutes::default();
    let out = call_som_builtin(
        &mut resolver,
        "Set",
        &[
            Value::String("form1.field".into()),
            Value::String("new".into()),
        ],
    )
    .expect("no error");
    assert!(out.is_some());
    assert_eq!(
        resolver.assigned,
        vec![("form1.field".to_string(), "new".to_string())]
    );
}

/// A name that is not a SOM function is not intercepted.
#[test]
fn an_unknown_name_is_passed_through() {
    let (out, _) = call("Concat", &[Value::String("form1.field".into())]);
    assert!(out.is_none());
}

// ---------------------------------------------------------------------------
// `call_dom_builtin` is the entry point the interpreter itself uses: it binds
// the same routing to a real Data DOM. That it genuinely forwards to
// `call_som_builtin` is not obvious enough to leave unguarded -- an empty
// pass-through implementation compiles just as well.

use formcalc_interpreter::som_bridge::{call_dom_builtin, DomContext};
use xfa_dom_resolver::data_dom::DataDom;

/// Get reads a real value out of the DOM.
#[test]
fn dom_get_reads_from_the_data_dom() {
    let mut dom = DataDom::from_xml("<form1><field>hello</field></form1>").unwrap();
    let mut ctx = DomContext::new(&mut dom);
    let out = call_dom_builtin(&mut ctx, "Get", &[Value::String("$data.field".into())])
        .expect("no error")
        .expect("the SOM branch should have handled this");
    assert_eq!(out.to_string_val(), "hello");
}

/// Set writes back, and Get sees it.
#[test]
fn dom_set_writes_back() {
    let mut dom = DataDom::from_xml("<form1><field>old</field></form1>").unwrap();
    let mut ctx = DomContext::new(&mut dom);
    call_dom_builtin(
        &mut ctx,
        "Set",
        &[
            Value::String("$data.field".into()),
            Value::String("new".into()),
        ],
    )
    .expect("no error")
    .expect("the SOM branch should have handled this");

    let out = call_dom_builtin(&mut ctx, "Get", &[Value::String("$data.field".into())])
        .expect("no error")
        .expect("the SOM branch should have handled this");
    assert_eq!(out.to_string_val(), "new");
}

/// A URL stays away from the SOM branch through the DOM entry point too.
#[test]
fn dom_lets_a_url_through() {
    let mut dom = DataDom::from_xml("<form1><field>x</field></form1>").unwrap();
    let mut ctx = DomContext::new(&mut dom);
    let out = call_dom_builtin(
        &mut ctx,
        "Get",
        &[Value::String("https://example.com/a.xml".into())],
    )
    .expect("no error");
    assert!(out.is_none());
}
