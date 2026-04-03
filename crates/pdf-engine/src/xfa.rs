//! XFA feature surface for `pdf-engine`.
//!
//! This module is compiled only with the `xfa` feature and re-exports the
//! high-level helpers needed to extract and flatten XFA content without making
//! the base crate depend on the XFA stack by default.

use crate::PdfDocument;

pub use pdf_xfa::error::XfaError;
pub use pdf_xfa::extract::XfaPackets;
pub use xfa_json::{export_schema, form_tree_to_json, form_tree_to_value, FormData, FormSchema};
pub use xfa_layout_engine::form::{FormNodeId, FormTree};

/// Extract the XFA packets embedded in a document.
pub fn extract_packets(document: &PdfDocument) -> Result<XfaPackets, XfaError> {
    pdf_xfa::extract::extract_xfa(document.pdf())
}

/// Returns `true` if the document has an XFA template packet.
///
/// Only an active `/AcroForm` `/XFA` entry counts here. Orphaned XFA streams
/// left behind after flattening should not trigger another flatten/render pass.
/// XFA PDFs without a template are treated as non-XFA (they have no content to
/// flatten), so this returns `false` for those documents.
pub fn has_xfa(document: &PdfDocument) -> bool {
    pdf_xfa::extract::extract_xfa_from_acroform(document.pdf())
        .is_some_and(|packets| packets.template().is_some())
}

/// Flatten an XFA document into static PDF bytes.
pub fn flatten(document: &PdfDocument) -> Result<Vec<u8>, XfaError> {
    pdf_xfa::flatten_xfa_to_pdf(document.pdf().data().as_ref())
}
