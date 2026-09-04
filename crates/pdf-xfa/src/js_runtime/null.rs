//! NullRuntime — always-available stub backend.
//!
//! Used when:
//! - The Cargo feature `xfa-js-sandboxed` is not compiled in.
//! - A caller selects `JsExecutionMode::SandboxedRuntime` but the
//!   build did not link a real backend.
//!
//! Returning [`SandboxError::NotCompiledIn`] from every script call
//! lets the dispatch site fall back to the existing best-effort skip
//! path (`js_skipped += 1`, `js_runtime_errors += 1`) without changing
//! pipeline behaviour.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::{RuntimeMetadata, RuntimeOutcome, SandboxError, XfaJsRuntime};

/// Stub runtime that refuses every script call.
#[derive(Debug, Default)]
pub struct NullRuntime {
    metadata: RuntimeMetadata,
}

impl NullRuntime {
    /// Construct a new `NullRuntime` with empty metadata.
    pub fn new() -> Self {
        Self::default()
    }
}

impl XfaJsRuntime for NullRuntime {
    fn init(&mut self) -> Result<(), SandboxError> {
        Ok(())
    }

    fn reset_for_new_document(&mut self) -> Result<(), SandboxError> {
        self.metadata = RuntimeMetadata::default();
        Ok(())
    }

    fn execute_script(
        &mut self,
        _activity: Option<&str>,
        _body: &str,
    ) -> Result<RuntimeOutcome, SandboxError> {
        self.metadata.runtime_errors = self.metadata.runtime_errors.saturating_add(1);
        Err(SandboxError::NotCompiledIn)
    }

    fn take_metadata(&mut self) -> RuntimeMetadata {
        std::mem::take(&mut self.metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_always_returns_not_compiled_in() {
        let mut rt = NullRuntime::new();
        rt.init().unwrap();
        rt.reset_for_new_document().unwrap();
        let err = rt
            .execute_script(Some("calculate"), "Total.rawValue = 42;")
            .unwrap_err();
        assert_eq!(err, SandboxError::NotCompiledIn);
    }

    #[test]
    fn metadata_records_runtime_errors() {
        let mut rt = NullRuntime::new();
        rt.init().unwrap();
        rt.reset_for_new_document().unwrap();
        for _ in 0..3 {
            let _ = rt.execute_script(Some("calculate"), "1+1");
        }
        let md = rt.take_metadata();
        assert_eq!(md.runtime_errors, 3);
        assert_eq!(md.executed, 0);
        assert_eq!(md.timeouts, 0);
        assert_eq!(md.oom, 0);
    }

    #[test]
    fn take_metadata_resets_counters() {
        let mut rt = NullRuntime::new();
        let _ = rt.execute_script(None, "");
        let _ = rt.take_metadata();
        let md = rt.take_metadata();
        assert_eq!(md, RuntimeMetadata::default());
    }

    #[test]
    fn reset_clears_counters() {
        let mut rt = NullRuntime::new();
        let _ = rt.execute_script(None, "");
        rt.reset_for_new_document().unwrap();
        let md = rt.take_metadata();
        assert_eq!(md, RuntimeMetadata::default());
    }
}
