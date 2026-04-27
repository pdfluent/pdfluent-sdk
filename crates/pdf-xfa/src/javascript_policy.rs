//! Central JavaScript handling policy for PDF/XFA hardening paths.

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId};

use crate::error::XfaError;

/// JavaScript may be parsed and inspected so hardening code can locate and
/// remove active content, but this crate must never execute document-supplied
/// JavaScript. PDF/XFA inputs are untrusted and JavaScript actions can reach
/// viewer APIs, network/file operations, and mutable form state. Keeping parse,
/// execution, and flatten behavior as explicit policy values prevents hidden
/// no-op fallbacks from becoming accidental execution paths in future work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaScriptPolicy {
    /// JavaScript syntax/payloads may be parsed or inspected for audit/strip.
    AllowParse,
    /// JavaScript execution is denied for all document-supplied entrypoints.
    DenyExecution,
    /// JavaScript-bearing actions are stripped from flattened/hardened output.
    StripOnFlatten,
}

pub const ALLOW_PARSE: JavaScriptPolicy = JavaScriptPolicy::AllowParse;
pub const DENY_EXECUTION: JavaScriptPolicy = JavaScriptPolicy::DenyExecution;
pub const STRIP_ON_FLATTEN: JavaScriptPolicy = JavaScriptPolicy::StripOnFlatten;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaScriptEntryPoint {
    PdfOpenAction,
    AnnotationAdditionalAction,
    FieldAction,
    XfaEventHook,
}

impl JavaScriptEntryPoint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PdfOpenAction => "PDF /OpenAction JavaScript",
            Self::AnnotationAdditionalAction => "annotation /AA JavaScript",
            Self::FieldAction => "field /A JavaScript",
            Self::XfaEventHook => "XFA event JavaScript",
        }
    }
}

pub fn parse_policy() -> JavaScriptPolicy {
    ALLOW_PARSE
}

pub fn execution_policy(_entrypoint: JavaScriptEntryPoint) -> JavaScriptPolicy {
    DENY_EXECUTION
}

pub fn flatten_policy(_entrypoint: JavaScriptEntryPoint) -> JavaScriptPolicy {
    STRIP_ON_FLATTEN
}

pub fn reject_execution(entrypoint: JavaScriptEntryPoint) -> XfaError {
    debug_assert_eq!(execution_policy(entrypoint), DENY_EXECUTION);
    XfaError::UnsupportedFeature("javascript".to_string())
}

pub fn execution_denied_message(entrypoint: JavaScriptEntryPoint) -> String {
    format!(
        "{} denied by policy: JavaScript is parsed for inspection but never executed",
        entrypoint.as_str()
    )
}

pub fn template_mentions_javascript(template_xml: &str) -> bool {
    let lower = template_xml.to_ascii_lowercase();
    lower.contains("text/javascript")
        || lower.contains("application/javascript")
        || lower.contains("application/x-javascript")
        || lower.contains("/x-javascript")
}

pub fn is_javascript_action_dict(dict: &lopdf::Dictionary) -> bool {
    matches!(
        dict.get(b"S").ok(),
        Some(Object::Name(name)) if name == b"JavaScript"
    )
}

pub fn catalog_has_javascript_open_action(doc: &Document) -> bool {
    let Some(catalog_id) = catalog_id(doc) else {
        return false;
    };
    let Some(Object::Dictionary(catalog)) = doc.objects.get(&catalog_id) else {
        return false;
    };
    catalog
        .get(b"OpenAction")
        .ok()
        .is_some_and(|action| object_is_javascript_action(doc, action))
}

pub fn dict_has_javascript_additional_actions(doc: &Document, dict: &lopdf::Dictionary) -> bool {
    dict.get(b"AA")
        .ok()
        .is_some_and(|aa| additional_actions_contain_javascript(doc, aa))
}

pub fn dict_has_javascript_field_action(doc: &Document, dict: &lopdf::Dictionary) -> bool {
    dict.get(b"A")
        .ok()
        .is_some_and(|action| object_is_javascript_action(doc, action))
}

/// Strip JavaScript-bearing PDF actions from XFA flattened output.
///
/// Returns the number of executable entrypoints neutralized. This is a
/// hardening count, not a complete object garbage-collection report.
pub fn strip_javascript_for_flatten(doc: &mut Document) -> usize {
    debug_assert_eq!(
        flatten_policy(JavaScriptEntryPoint::PdfOpenAction),
        STRIP_ON_FLATTEN
    );

    let mut count = 0;
    count += strip_javascript_name_tree(doc);

    if let Some(catalog_id) = catalog_id(doc) {
        let remove_open_action = doc
            .objects
            .get(&catalog_id)
            .and_then(|object| match object {
                Object::Dictionary(catalog) => catalog.get(b"OpenAction").ok(),
                _ => None,
            })
            .is_some_and(|action| object_is_javascript_action(doc, action));

        if remove_open_action {
            if let Some(Object::Dictionary(catalog)) = doc.objects.get_mut(&catalog_id) {
                catalog.remove(b"OpenAction");
                count += 1;
            }
        }
    }

    // Two-phase strip: detection runs read-only against the original
    // document state, then mutation applies the collected decisions. A
    // single-pass loop is order-dependent because mutating a JS-action
    // object first can hide it from later `dict_has_javascript_field_action`
    // / `dict_has_javascript_additional_actions` checks on dictionaries
    // that reference it indirectly, leaving `/A` or `/AA` keys behind
    // even though the original graph was JS-bearing. Splitting detection
    // and mutation makes the outcome deterministic regardless of the
    // underlying object iteration order.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut decisions: Vec<(ObjectId, StripDecision)> = Vec::new();
    for id in ids {
        let decision = match doc.objects.get(&id) {
            Some(Object::Dictionary(dict)) => StripDecision {
                is_js_action: is_javascript_action_dict(dict),
                has_js_field_action: dict_has_javascript_field_action(doc, dict),
                has_js_aa: dict_has_javascript_additional_actions(doc, dict),
            },
            _ => StripDecision::default(),
        };
        if decision.has_any() {
            decisions.push((id, decision));
        }
    }

    for (id, decision) in decisions {
        if let Some(Object::Dictionary(dict)) = doc.objects.get_mut(&id) {
            if decision.is_js_action {
                dict.remove(b"JS");
                dict.remove(b"S");
                count += 1;
            }
            if decision.has_js_field_action {
                dict.remove(b"A");
                count += 1;
            }
            if decision.has_js_aa {
                dict.remove(b"AA");
                count += 1;
            }
        }
    }

    count
}

#[derive(Default, Clone, Copy)]
struct StripDecision {
    is_js_action: bool,
    has_js_field_action: bool,
    has_js_aa: bool,
}

impl StripDecision {
    fn has_any(&self) -> bool {
        self.is_js_action || self.has_js_field_action || self.has_js_aa
    }
}

fn strip_javascript_name_tree(doc: &mut Document) -> usize {
    let Some(catalog_id) = catalog_id(doc) else {
        return 0;
    };

    let names_id = doc
        .objects
        .get(&catalog_id)
        .and_then(|object| match object {
            Object::Dictionary(catalog) => match catalog.get(b"Names").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            },
            _ => None,
        });

    let mut count = 0;
    if let Some(names_id) = names_id {
        if let Some(Object::Dictionary(names)) = doc.objects.get_mut(&names_id) {
            if names.has(b"JavaScript") {
                names.remove(b"JavaScript");
                count += 1;
            }
        }
    }

    let has_inline_js = doc.objects.get(&catalog_id).is_some_and(|object| {
        matches!(object, Object::Dictionary(catalog) if matches!(
            catalog.get(b"Names").ok(),
            Some(Object::Dictionary(names)) if names.has(b"JavaScript")
        ))
    });

    if has_inline_js {
        if let Some(Object::Dictionary(catalog)) = doc.objects.get_mut(&catalog_id) {
            if let Ok(Object::Dictionary(names)) = catalog.get_mut(b"Names") {
                names.remove(b"JavaScript");
                count += 1;
            }
        }
    }

    count
}

fn additional_actions_contain_javascript(doc: &Document, aa: &Object) -> bool {
    ActionGraphWalk::default().additional_actions_contain_javascript(doc, aa, 0)
}

fn object_is_javascript_action(doc: &Document, action: &Object) -> bool {
    ActionGraphWalk::default().object_is_javascript_action(doc, action, 0)
}

const MAX_ACTION_GRAPH_DEPTH: usize = 64;
const MAX_ACTION_GRAPH_REFERENCES: usize = 128;

#[derive(Default)]
struct ActionGraphWalk {
    visiting: HashSet<ObjectId>,
    resolved_references: usize,
}

impl ActionGraphWalk {
    fn additional_actions_contain_javascript(
        &mut self,
        doc: &Document,
        aa: &Object,
        depth: usize,
    ) -> bool {
        if depth > MAX_ACTION_GRAPH_DEPTH {
            return true;
        }

        match aa {
            Object::Dictionary(dict) => dict
                .iter()
                .any(|(_, action)| self.object_is_javascript_action(doc, action, depth + 1)),
            Object::Reference(id) => {
                self.referenced_additional_actions_contain_javascript(doc, *id, depth + 1)
            }
            _ => false,
        }
    }

    fn referenced_additional_actions_contain_javascript(
        &mut self,
        doc: &Document,
        id: ObjectId,
        depth: usize,
    ) -> bool {
        if !self.enter_reference(id) {
            return true;
        }
        let contains_js = doc.objects.get(&id).is_some_and(|object| {
            self.additional_actions_contain_javascript(doc, object, depth + 1)
        });
        self.visiting.remove(&id);
        contains_js
    }

    fn object_is_javascript_action(
        &mut self,
        doc: &Document,
        action: &Object,
        depth: usize,
    ) -> bool {
        if depth > MAX_ACTION_GRAPH_DEPTH {
            return true;
        }

        match action {
            Object::Dictionary(dict) => self.action_dict_contains_javascript(doc, dict, depth + 1),
            Object::Reference(id) => {
                self.referenced_action_contains_javascript(doc, *id, depth + 1)
            }
            Object::Array(actions) => actions
                .iter()
                .any(|action| self.object_is_javascript_action(doc, action, depth + 1)),
            _ => false,
        }
    }

    fn referenced_action_contains_javascript(
        &mut self,
        doc: &Document,
        id: ObjectId,
        depth: usize,
    ) -> bool {
        if !self.enter_reference(id) {
            return true;
        }
        let contains_js = doc
            .objects
            .get(&id)
            .is_some_and(|object| self.object_is_javascript_action(doc, object, depth + 1));
        self.visiting.remove(&id);
        contains_js
    }

    fn action_dict_contains_javascript(
        &mut self,
        doc: &Document,
        dict: &lopdf::Dictionary,
        depth: usize,
    ) -> bool {
        if is_javascript_action_dict(dict) {
            return true;
        }
        dict.get(b"Next")
            .ok()
            .is_some_and(|next| self.object_is_javascript_action(doc, next, depth + 1))
    }

    fn enter_reference(&mut self, id: ObjectId) -> bool {
        if self.resolved_references >= MAX_ACTION_GRAPH_REFERENCES {
            return false;
        }
        if self.visiting.contains(&id) {
            return false;
        }
        self.resolved_references += 1;
        self.visiting.insert(id)
    }
}

fn catalog_id(doc: &Document) -> Option<ObjectId> {
    match doc.trailer.get(b"Root").ok()? {
        Object::Reference(id) => Some(*id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object};

    fn basic_doc_with_catalog(catalog: lopdf::Dictionary) -> Document {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        }));
        let mut catalog = catalog;
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(pages_id));
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc
    }

    fn js_action(source: &[u8]) -> Object {
        Object::Dictionary(dictionary! {
            "S" => Object::Name(b"JavaScript".to_vec()),
            "JS" => Object::String(source.to_vec(), lopdf::StringFormat::Literal),
        })
    }

    fn hide_action_dict(next: Option<Object>) -> lopdf::Dictionary {
        let mut dict = dictionary! {
            "S" => Object::Name(b"Hide".to_vec()),
        };
        if let Some(next) = next {
            dict.set("Next", next);
        }
        dict
    }

    fn hide_action(next: Option<Object>) -> Object {
        Object::Dictionary(hide_action_dict(next))
    }

    fn set_catalog_open_action(doc: &mut Document, action: Object) {
        let catalog_id = catalog_id(doc).expect("catalog id");
        let Some(Object::Dictionary(catalog)) = doc.objects.get_mut(&catalog_id) else {
            panic!("catalog dictionary");
        };
        catalog.set("OpenAction", action);
    }

    fn catalog_has_open_action(doc: &Document) -> bool {
        let catalog_id = catalog_id(doc).expect("catalog id");
        matches!(
            doc.objects.get(&catalog_id),
            Some(Object::Dictionary(catalog)) if catalog.has(b"OpenAction")
        )
    }

    #[test]
    fn policy_constants_are_explicit() {
        assert_eq!(parse_policy(), ALLOW_PARSE);
        assert_eq!(
            execution_policy(JavaScriptEntryPoint::XfaEventHook),
            DENY_EXECUTION
        );
        assert_eq!(
            flatten_policy(JavaScriptEntryPoint::AnnotationAdditionalAction),
            STRIP_ON_FLATTEN
        );
    }

    #[test]
    fn reject_execution_returns_unsupported_javascript() {
        let err = reject_execution(JavaScriptEntryPoint::XfaEventHook);
        assert_eq!(format!("{err}"), "unsupported feature: javascript");
    }

    #[test]
    fn detects_open_action_javascript() {
        let doc = basic_doc_with_catalog(dictionary! {
            "OpenAction" => js_action(b"app.alert('x')"),
        });

        assert!(catalog_has_javascript_open_action(&doc));
    }

    #[test]
    fn strips_open_action_javascript_on_flatten() {
        let mut doc = basic_doc_with_catalog(dictionary! {
            "OpenAction" => js_action(b"app.alert('x')"),
        });

        assert_eq!(strip_javascript_for_flatten(&mut doc), 1);
        assert!(!catalog_has_javascript_open_action(&doc));
    }

    #[test]
    fn strips_open_action_when_next_dict_contains_javascript() {
        let mut doc = basic_doc_with_catalog(dictionary! {
            "OpenAction" => hide_action(Some(js_action(b"app.alert('next')"))),
        });

        assert!(catalog_has_javascript_open_action(&doc));
        assert_eq!(strip_javascript_for_flatten(&mut doc), 1);
        assert!(!catalog_has_open_action(&doc));
    }

    #[test]
    fn strips_open_action_when_next_array_contains_javascript() {
        let mut doc = basic_doc_with_catalog(dictionary! {
            "OpenAction" => hide_action(Some(Object::Array(vec![
                hide_action(None),
                js_action(b"app.alert('array')"),
            ]))),
        });

        assert!(catalog_has_javascript_open_action(&doc));
        assert_eq!(strip_javascript_for_flatten(&mut doc), 1);
        assert!(!catalog_has_open_action(&doc));
    }

    #[test]
    fn cyclic_next_chain_is_fail_closed_without_looping() {
        let mut doc = basic_doc_with_catalog(dictionary! {});
        let action_a_id = doc.new_object_id();
        let action_b_id = doc.new_object_id();
        doc.objects.insert(
            action_a_id,
            Object::Dictionary(hide_action_dict(Some(Object::Reference(action_b_id)))),
        );
        doc.objects.insert(
            action_b_id,
            Object::Dictionary(hide_action_dict(Some(Object::Reference(action_a_id)))),
        );
        set_catalog_open_action(&mut doc, Object::Reference(action_a_id));

        assert!(catalog_has_javascript_open_action(&doc));
        assert_eq!(strip_javascript_for_flatten(&mut doc), 1);
        assert!(!catalog_has_open_action(&doc));
    }

    #[test]
    fn detects_annotation_additional_action_javascript() {
        let doc = basic_doc_with_catalog(dictionary! {});
        let annot = dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "AA" => Object::Dictionary(dictionary! {
                "E" => js_action(b"app.alert('enter')"),
            }),
        };

        assert!(dict_has_javascript_additional_actions(&doc, &annot));
    }

    #[test]
    fn strips_additional_action_entry_with_next_chain_javascript() {
        let mut doc = basic_doc_with_catalog(dictionary! {});
        let action_id = doc.add_object(hide_action(Some(js_action(
            b"app.alert('additional action')",
        ))));
        let annot_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "AA" => Object::Dictionary(dictionary! {
                "U" => Object::Reference(action_id),
            }),
        }));

        let annot = match doc.objects.get(&annot_id) {
            Some(Object::Dictionary(annot)) => annot,
            _ => panic!("annotation dictionary"),
        };
        assert!(dict_has_javascript_additional_actions(&doc, annot));

        assert_eq!(strip_javascript_for_flatten(&mut doc), 1);
        let annot = match doc.objects.get(&annot_id) {
            Some(Object::Dictionary(annot)) => annot,
            _ => panic!("annotation dictionary"),
        };
        assert!(!annot.has(b"AA"));
    }

    #[test]
    fn detects_field_action_javascript() {
        let doc = basic_doc_with_catalog(dictionary! {});
        let field = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "A" => js_action(b"app.alert('field')"),
        };

        assert!(dict_has_javascript_field_action(&doc, &field));
    }

    #[test]
    fn strips_field_action_with_next_chain_javascript() {
        let mut doc = basic_doc_with_catalog(dictionary! {});
        let field_id = doc.add_object(Object::Dictionary(dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "A" => hide_action(Some(js_action(b"app.alert('field next')"))),
        }));

        assert_eq!(strip_javascript_for_flatten(&mut doc), 1);
        let field = match doc.objects.get(&field_id) {
            Some(Object::Dictionary(field)) => field,
            _ => panic!("field dictionary"),
        };
        assert!(!field.has(b"A"));
    }

    #[test]
    fn pure_non_javascript_action_chain_is_preserved() {
        let mut doc = basic_doc_with_catalog(dictionary! {
            "OpenAction" => hide_action(Some(hide_action(Some(hide_action(None))))),
        });

        assert!(!catalog_has_javascript_open_action(&doc));
        assert_eq!(strip_javascript_for_flatten(&mut doc), 0);
        assert!(catalog_has_open_action(&doc));
    }

    #[test]
    fn malformed_javascript_payload_is_never_parsed_for_execution() {
        let mut doc = basic_doc_with_catalog(dictionary! {
            "OpenAction" => js_action(b"\0}\xff{not valid js"),
        });

        assert!(catalog_has_javascript_open_action(&doc));
        assert_eq!(strip_javascript_for_flatten(&mut doc), 1);
    }

    #[test]
    fn indirect_field_action_to_javascript_is_always_stripped() {
        // Codex P2 regression-guard: a field whose `/A` is an indirect
        // reference to a JavaScript action object must always be stripped,
        // even when the underlying object iteration order would visit the
        // JS-action object before the field dict that points to it. The
        // single-pass loop used to mutate the JS-action first and then
        // see no JS in the field's referenced action graph; the two-phase
        // detect-then-mutate split keeps the result deterministic.
        let mut doc = basic_doc_with_catalog(dictionary! {});
        let js_action_id = doc.add_object(js_action(b"app.alert('indirect')"));
        let field_id = doc.add_object(Object::Dictionary(dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "A" => Object::Reference(js_action_id),
        }));

        let count = strip_javascript_for_flatten(&mut doc);
        // Field /A removal + JS action /JS+/S removal = 2 strip events.
        assert_eq!(count, 2, "both field /A and JS action body must be stripped");

        let field = match doc.objects.get(&field_id) {
            Some(Object::Dictionary(field)) => field,
            _ => panic!("field dictionary"),
        };
        assert!(
            !field.has(b"A"),
            "field /A pointing indirectly to JS action must be stripped"
        );

        let js_obj = match doc.objects.get(&js_action_id) {
            Some(Object::Dictionary(action)) => action,
            _ => panic!("js action dictionary"),
        };
        assert!(
            !js_obj.has(b"JS"),
            "indirect JS action /JS payload must be stripped"
        );
        assert!(
            !js_obj.has(b"S"),
            "indirect JS action /S name must be stripped"
        );
    }

    #[test]
    fn strip_outcome_is_independent_of_object_insertion_order() {
        // Build the same logical graph (field /A → indirect JS action) twice,
        // but allocate the objects in opposite orders so HashMap iteration
        // hits them in different sequence. The two-phase strip must produce
        // identical post-state in both arrangements.
        let mut doc_action_first = basic_doc_with_catalog(dictionary! {});
        let action_id_a = doc_action_first.add_object(js_action(b"app.alert('A')"));
        let field_id_a = doc_action_first.add_object(Object::Dictionary(dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "A" => Object::Reference(action_id_a),
        }));

        let mut doc_field_first = basic_doc_with_catalog(dictionary! {});
        let field_id_b = doc_field_first.add_object(Object::Dictionary(dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "A" => Object::Reference(lopdf::ObjectId::default()), // placeholder
        }));
        let action_id_b = doc_field_first.add_object(js_action(b"app.alert('A')"));
        // Patch the placeholder so the field really points at the action.
        if let Some(Object::Dictionary(d)) = doc_field_first.objects.get_mut(&field_id_b) {
            d.set("A", Object::Reference(action_id_b));
        }

        let count_a = strip_javascript_for_flatten(&mut doc_action_first);
        let count_b = strip_javascript_for_flatten(&mut doc_field_first);
        assert_eq!(count_a, count_b, "strip count must match across orderings");

        for (doc, fid, aid) in [
            (&doc_action_first, field_id_a, action_id_a),
            (&doc_field_first, field_id_b, action_id_b),
        ] {
            let field = match doc.objects.get(&fid) {
                Some(Object::Dictionary(f)) => f,
                _ => panic!("field"),
            };
            assert!(!field.has(b"A"), "field /A must be stripped in both orderings");
            let act = match doc.objects.get(&aid) {
                Some(Object::Dictionary(a)) => a,
                _ => panic!("action"),
            };
            assert!(!act.has(b"JS"), "action /JS must be stripped in both orderings");
            assert!(!act.has(b"S"), "action /S must be stripped in both orderings");
        }
    }

    #[test]
    fn non_javascript_action_chain_with_indirect_targets_is_preserved() {
        // Pure non-JS chain reachable through indirect references must not
        // be stripped by the two-phase pass. Confirms that only JS-bearing
        // graphs are removed, not innocent action chains.
        let mut doc = basic_doc_with_catalog(dictionary! {});
        let inner_hide_id = doc.add_object(hide_action(None));
        let outer_hide = hide_action(Some(Object::Reference(inner_hide_id)));
        let field_id = doc.add_object(Object::Dictionary(dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "A" => outer_hide,
        }));

        assert_eq!(
            strip_javascript_for_flatten(&mut doc),
            0,
            "non-JS action chain must be preserved"
        );

        let field = match doc.objects.get(&field_id) {
            Some(Object::Dictionary(f)) => f,
            _ => panic!("field"),
        };
        assert!(field.has(b"A"), "non-JS /A must remain on field");
        let inner = match doc.objects.get(&inner_hide_id) {
            Some(Object::Dictionary(a)) => a,
            _ => panic!("inner hide action"),
        };
        assert!(inner.has(b"S"), "non-JS action /S must remain");
    }
}
