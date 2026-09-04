//! Shared application state.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Stored form data for retrieval by ID.
#[derive(Debug, Clone)]
pub struct StoredForm {
    /// The original PDF bytes.
    pub pdf_bytes: Vec<u8>,
}

/// Shared application state passed to all route handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    /// In-memory form store (for schema endpoint).
    pub forms: Arc<Mutex<HashMap<String, StoredForm>>>,
}

impl AppState {
    /// Create a new empty application state.
    pub fn new() -> Self {
        Self {
            forms: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
