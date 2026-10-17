//! The model tiers' shared surface: specs, failures and install paths (#641).
//!
//! Always compiled, with or without the `ort-layout` cargo feature: the
//! specs, the failure types and the path resolution of the shared ONNX
//! Runtime layer are plain Rust, so a build without the tier still reports a
//! missing model with the typed [`ModelError`] rather than a string. Only
//! the session half of `pdf_ocr::runtime` -- `init`, `builder`, `session`,
//! `SharedSession` -- needs the `ort` feature and the `ort` crate.
//!
//! [`LAYOUT_EP03`] is the balanced tier's model. [`balanced_unavailable`]
//! is what [`Quality::Balanced`](crate::Quality) answers on a build without
//! the tier.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub use pdf_ocr::runtime::{
    default_model_root, model_error, resolve_model_file, resolve_model_root, resolve_ort_dylib,
    ModelError, ModelErrorKind, ModelSpec, RuntimeConfig, ORT_VERSION,
};

#[cfg(not(feature = "ort-layout"))]
use crate::EngineError;
use crate::Result;

/// The layout model: Heron-ft ep03 (RT-DETRv2), ONNX export.
///
/// One definition for both builds: the session loader reads it behind
/// `ort-layout`, and the balanced error below names it without the feature.
pub const LAYOUT_EP03: ModelSpec = ModelSpec::new(
    "layout-heron-ep03",
    "heron_ep03.onnx",
    "b7cc5d31f3223c74cc477a1386b894793618aeb398ca59cf5eb7264ec61d8afa",
    "pdfluent models install layout",
);

/// The tables model: TATR v1.1-all (Table Transformer structure), ONNX export.
///
/// One definition for the installer on every build: `pdfluent models install
/// tables` resolves this spec to the install path, and the TATR stage resolves
/// it the same way through its own [`TatrOptions`](crate::TatrOptions) default
/// -- so an install is found without flags. The stage itself reads
/// `learned::tables_tatr::TATR_SPEC` behind `ort-tables`; the name, file and
/// digest here MUST match it, pinned by
/// `the_installer_tables_spec_matches_the_stage_spec` in
/// `tests/tatr_engine_hook.rs`, which runs under the `ort-tables` CI job.
pub const TABLES_TATR: ModelSpec = ModelSpec::new(
    "tatr-v1.1-all",
    "tatr-v1.1-all.onnx",
    "8c28418690b896a845a53633736a62c22ef8c9a515a23c06e9976e61360f1fcf",
    "pdfluent models install tables",
);

/// The [`Quality::Balanced`](crate::Quality) failure on a build without the
/// tier: `MODEL_NOT_AVAILABLE` naming the cargo feature to rebuild with and
/// the install command, carried as [`EngineError::Io`] -- no new variants.
#[cfg(not(feature = "ort-layout"))]
pub(crate) fn balanced_unavailable() -> EngineError {
    EngineError::Io(std::io::Error::from(ModelError::feature_not_compiled(
        &LAYOUT_EP03,
        "ort-layout",
    )))
}

/// Run the balanced tier over the document, or fail loud without the feature.
///
/// With `ort-layout` this is `balanced_markdown` with the model dir and the
/// TATR opt-in the caller gave; without it, `MODEL_NOT_AVAILABLE` naming
/// the feature and the install command. `PdfDocument` calls this before
/// validating, because the balanced tier is markdown-only and the block
/// validation would refuse it first.
pub(crate) fn balanced_markdown_for(
    doc: &crate::PdfDocument,
    options: &crate::ExtractOptions,
) -> Result<String> {
    #[cfg(feature = "ort-layout")]
    {
        let mut config = RuntimeConfig::new();
        if let Some(dir) = &options.model_dir {
            config = config.with_model_dir(dir.clone());
        }
        let mut balanced =
            crate::balanced::BalancedOptions::default().keep_furniture(options.keep_furniture);
        // The TATR opt-in rides into the balanced tier: `Quality::Balanced`
        // plus `with_tatr` structures the kept table regions with TATR
        // first (track B, P4). Unset, the tier runs as it always has.
        if let Some(tatr) = options.tatr() {
            balanced = balanced.with_tatr(tatr.clone());
        }
        crate::balanced::balanced_markdown(doc, &config, &balanced)
    }
    #[cfg(not(feature = "ort-layout"))]
    {
        let _ = (doc, options);
        Err(balanced_unavailable())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_spec_names_the_ep03_package() {
        assert_eq!(LAYOUT_EP03.name, "layout-heron-ep03");
        assert_eq!(LAYOUT_EP03.file, "heron_ep03.onnx");
        assert_eq!(
            LAYOUT_EP03.sha256,
            "b7cc5d31f3223c74cc477a1386b894793618aeb398ca59cf5eb7264ec61d8afa"
        );
        assert_eq!(
            LAYOUT_EP03.install_command,
            "pdfluent models install layout"
        );
        assert_eq!(ORT_VERSION, "1.23.2");
    }

    #[test]
    fn the_tables_spec_names_the_tatr_package() {
        assert_eq!(TABLES_TATR.name, "tatr-v1.1-all");
        assert_eq!(TABLES_TATR.file, "tatr-v1.1-all.onnx");
        assert_eq!(
            TABLES_TATR.sha256,
            "8c28418690b896a845a53633736a62c22ef8c9a515a23c06e9976e61360f1fcf"
        );
        assert_eq!(
            TABLES_TATR.install_command,
            "pdfluent models install tables"
        );
    }

    #[cfg(not(feature = "ort-layout"))]
    #[test]
    fn balanced_unavailable_names_the_feature_and_the_install() {
        let error = balanced_unavailable();
        let EngineError::Io(io) = &error else {
            panic!("a model failure rides as Io");
        };
        let model = model_error(io).expect("the chain carries the ModelError");
        assert_eq!(model.kind, ModelErrorKind::ModelNotAvailable);
        assert_eq!(model.kind.code(), "MODEL_NOT_AVAILABLE");
        let message = error.to_string();
        assert!(
            message.contains("`ort-layout`"),
            "the message names the cargo feature: {message}"
        );
        assert!(
            message.contains("pdfluent models install layout"),
            "the message names the install command: {message}"
        );
    }
}
