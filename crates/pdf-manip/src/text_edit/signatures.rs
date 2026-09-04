//! lopdf-side digital-signature and permissions detection (design §6).
//!
//! `pdf-sign` validates signatures on `pdf_syntax::Pdf`; the edit engine works
//! on an in-memory `lopdf::Document`, so signature *presence* and DocMDP
//! levels are detected here directly from the AcroForm tree. No cryptographic
//! validation is performed — policy only needs to know that signatures exist.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{Dictionary, Document, Object};

use super::SignatureSummary;

/// Resolve an object that may be a direct value or a reference.
pub(crate) fn resolve<'a>(doc: &'a Document, obj: &'a Object) -> &'a Object {
    match obj {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(obj),
        other => other,
    }
}

pub(crate) fn resolve_dict<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Dictionary> {
    match resolve(doc, obj) {
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn catalog(doc: &Document) -> Option<&Dictionary> {
    let root = doc.trailer.get(b"Root").ok()?;
    resolve_dict(doc, root)
}

/// Find every filled signature field (`/FT /Sig` with a `/V` value) plus its
/// DocMDP certification level, if any.
pub(crate) fn detect_signatures(doc: &Document) -> Vec<SignatureSummary> {
    let mut found = Vec::new();
    let Some(cat) = catalog(doc) else {
        return found;
    };
    let Some(acroform) = cat.get(b"AcroForm").ok().and_then(|o| resolve_dict(doc, o)) else {
        return found;
    };
    let Some(fields) = acroform.get(b"Fields").ok().map(|o| resolve(doc, o)) else {
        return found;
    };
    let Object::Array(fields) = fields else {
        return found;
    };

    let mut stack: Vec<&Object> = fields.iter().collect();
    while let Some(field_obj) = stack.pop() {
        let Some(field) = resolve_dict(doc, field_obj) else {
            continue;
        };
        // Descend into non-terminal fields.
        if let Ok(Object::Array(kids)) = field.get(b"Kids").map(|o| resolve(doc, o)) {
            stack.extend(kids.iter());
        }
        let is_sig = matches!(
            field.get(b"FT").map(|o| resolve(doc, o)),
            Ok(Object::Name(n)) if n == b"Sig"
        );
        if !is_sig {
            continue;
        }
        let Some(value) = field.get(b"V").ok().and_then(|o| resolve_dict(doc, o)) else {
            continue; // unsigned signature field
        };

        let field_name = match field.get(b"T").map(|o| resolve(doc, o)) {
            Ok(Object::String(s, _)) => String::from_utf8_lossy(s).to_string(),
            _ => String::new(),
        };
        let docmdp_permission = docmdp_level(doc, value);
        found.push(SignatureSummary {
            field_name,
            docmdp_permission,
        });
    }
    found
}

/// Extract the DocMDP `/P` level from a signature value's `/Reference` array.
fn docmdp_level(doc: &Document, sig_value: &Dictionary) -> Option<u32> {
    let refs = match sig_value.get(b"Reference").map(|o| resolve(doc, o)) {
        Ok(Object::Array(a)) => a,
        _ => return None,
    };
    for entry in refs {
        let Some(sig_ref) = resolve_dict(doc, entry) else {
            continue;
        };
        let is_docmdp = matches!(
            sig_ref.get(b"TransformMethod").map(|o| resolve(doc, o)),
            Ok(Object::Name(n)) if n == b"DocMDP"
        );
        if !is_docmdp {
            continue;
        }
        let Some(params) = sig_ref
            .get(b"TransformParams")
            .ok()
            .and_then(|o| resolve_dict(doc, o))
        else {
            // DocMDP without params defaults to P=2 (ISO 32000-2 12.8.2.2).
            return Some(2);
        };
        return match params.get(b"P").map(|o| resolve(doc, o)) {
            Ok(Object::Integer(p)) => Some(*p as u32),
            _ => Some(2),
        };
    }
    None
}

/// Whether the document's encryption dictionary forbids content modification.
///
/// Conservative: if `/Encrypt` is present and the permission flags clear the
/// modify-contents bit (bit 4, value 8, ISO 32000-2 Table 22), editing is
/// denied. Owner-authenticated sessions cannot be distinguished at this
/// layer; callers with owner rights should remove encryption first.
pub(crate) fn modification_forbidden(doc: &Document) -> bool {
    let Ok(encrypt) = doc.trailer.get(b"Encrypt") else {
        return false;
    };
    let Some(enc) = resolve_dict(doc, encrypt) else {
        return false;
    };
    match enc.get(b"P").map(|o| resolve(doc, o)) {
        Ok(Object::Integer(p)) => *p & 8 == 0,
        _ => false,
    }
}
