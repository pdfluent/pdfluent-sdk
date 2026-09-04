//! Field value store and execution context for JS scripts.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, Arc};

/// A flat key-value store mapping XFA field names to their raw string values.
///
/// This is the form-state view exposed to scripts via `xfa.form.<name>.rawValue`.
/// The runtime borrows this per-execution; it never takes ownership.
#[derive(Debug, Default, Clone)]
pub struct FieldValues {
    map: HashMap<String, String>,
}

impl FieldValues {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or update a field value.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.map.insert(name.into(), value.into());
    }

    /// Read a field value. Returns `None` if the field is not in the store.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(String::as_str)
    }

    /// Iterate over all (name, value) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Number of fields in the store.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// True when the store is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Execution context passed to [`crate::XfaJsRuntime::execute_calculate`].
///
/// The runtime borrows field state for the duration of the call; it does not
/// own form state. Any `rawValue` writes made by the script are reflected back
/// into `fields` when the call returns.
pub struct ExecCtx<'a> {
    /// Form field values. Scripts may read and write these via
    /// `xfa.form.<fieldName>.rawValue`.
    pub fields: &'a mut FieldValues,
    /// Cancellation token. When set to `true` the interrupt handler fires on
    /// the next QuickJS instruction check and the script is aborted with
    /// [`crate::XfaJsError::Cancelled`].
    pub cancel: Arc<AtomicBool>,
    /// `xfa.event.newText` — the new text in a change event. `None` outside
    /// change-event context; scripts should not write to this.
    pub event_new_text: Option<&'a str>,
}

impl<'a> ExecCtx<'a> {
    /// Convenience constructor.
    pub fn new(fields: &'a mut FieldValues, cancel: Arc<AtomicBool>) -> Self {
        Self {
            fields,
            cancel,
            event_new_text: None,
        }
    }
}
