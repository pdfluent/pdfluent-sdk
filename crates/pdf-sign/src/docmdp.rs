//! DocMDP and FieldMDP permission handling (ISO 32000-2 §12.8.4).

use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Array, Dict, Name};
use pdf_syntax::Pdf;

use crate::sig_dict::SigDict;
use crate::string_util::pdf_string_to_string;
use crate::types::{DocMdpPermission, LockAction};

// Keys not defined in pdf-syntax.
const LOCK: &[u8] = b"Lock";
const DOC_MDP: &[u8] = b"DocMDP";
const ACTION_KEY: &[u8] = b"Action";
const P_KEY: &[u8] = b"P";

/// DocMDP transform result — the permission level imposed by a certification signature.
#[derive(Debug, Clone)]
pub struct DocMdpInfo {
    /// The permission level.
    pub permission: DocMdpPermission,
    /// The field that contains the certification signature.
    pub certifying_field: Option<String>,
}

/// FieldMDP transform result — which fields are locked by a signature.
#[derive(Debug, Clone)]
pub struct FieldMdpInfo {
    /// The lock action (All, Include, or Exclude).
    pub action: LockAction,
    /// The field that imposed this lock.
    pub signing_field: Option<String>,
}

/// Seed value constraints for a signature field (ISO 32000-2 §12.8.5).
#[derive(Debug, Clone)]
pub struct SeedValueConstraints {
    /// Required SubFilter values.
    pub sub_filter: Vec<String>,
    /// Required digest methods.
    pub digest_method: Vec<String>,
    /// Required reasons.
    pub reasons: Vec<String>,
    /// Whether the signer must provide a reason.
    pub reason_required: bool,
}

/// Extract the DocMDP permission level from a PDF's certification signature.
pub fn get_docmdp_permission(pdf: &Pdf) -> Option<DocMdpInfo> {
    let xref = pdf.xref();
    let root: Dict<'_> = xref.get(xref.root_id())?;

    // Method 1: /Perms dictionary in the catalog.
    if let Some(perms) = root.get::<Dict<'_>>(PERMS) {
        if let Some(docmdp_sig) = perms.get::<Dict<'_>>(DOC_MDP) {
            let sig = SigDict::from_dict(docmdp_sig);
            if let Some(perm) = extract_docmdp_from_sig_refs(&sig) {
                return Some(DocMdpInfo {
                    permission: perm,
                    certifying_field: sig.signer_name(),
                });
            }
        }
    }

    // Method 2: Scan signature fields for /TransformMethod = /DocMDP.
    let sigs = crate::signature_fields(pdf);
    for info in &sigs {
        if let Some(perm) = extract_docmdp_from_sig_refs(&info.sig) {
            return Some(DocMdpInfo {
                permission: perm,
                certifying_field: Some(info.field_name.clone()),
            });
        }
    }

    None
}

fn extract_docmdp_from_sig_refs(sig: &SigDict<'_>) -> Option<DocMdpPermission> {
    for ref_dict in sig.references() {
        let method = ref_dict.get::<Name>(TRANSFORM_METHOD)?;
        if method.as_ref() != b"DocMDP" {
            continue;
        }
        if let Some(params) = ref_dict.get::<Dict<'_>>(TRANSFORM_PARAMS) {
            let p = params.get::<u32>(P_KEY).unwrap_or(2);
            return Some(DocMdpPermission::from_value(p));
        }
    }
    None
}

/// Extract FieldMDP lock information from signature fields.
pub fn get_field_mdp_locks(pdf: &Pdf) -> Vec<FieldMdpInfo> {
    let mut locks = Vec::new();
    let sigs = crate::signature_fields(pdf);

    for info in &sigs {
        // Check /Reference array for FieldMDP transforms.
        for ref_dict in info.sig.references() {
            let method = match ref_dict.get::<Name>(TRANSFORM_METHOD) {
                Some(m) => m,
                None => continue,
            };
            if method.as_ref() != b"FieldMDP" {
                continue;
            }
            if let Some(params) = ref_dict.get::<Dict<'_>>(TRANSFORM_PARAMS) {
                if let Some(action) = parse_lock_action(&params) {
                    locks.push(FieldMdpInfo {
                        action,
                        signing_field: Some(info.field_name.clone()),
                    });
                }
            }
        }

        // Check /Lock dictionary in the field itself.
        if let Some(lock_dict) = info.field_dict.get::<Dict<'_>>(LOCK) {
            if let Some(action) = parse_lock_action(&lock_dict) {
                locks.push(FieldMdpInfo {
                    action,
                    signing_field: Some(info.field_name.clone()),
                });
            }
        }
    }

    locks
}

/// Check if a specific field is locked by any FieldMDP constraint.
pub fn is_field_locked(field_name: &str, locks: &[FieldMdpInfo]) -> bool {
    for lock in locks {
        match &lock.action {
            LockAction::All => return true,
            LockAction::Include(fields) => {
                if fields.iter().any(|f| f == field_name) {
                    return true;
                }
            }
            LockAction::Exclude(fields) => {
                if !fields.iter().any(|f| f == field_name) {
                    return true;
                }
            }
        }
    }
    false
}

/// Parse seed value constraints from a signature field's /SV dictionary.
pub fn parse_seed_values(field_dict: &Dict<'_>) -> Option<SeedValueConstraints> {
    let sv = field_dict.get::<Dict<'_>>(SV)?;

    let sub_filter = sv
        .get::<Array<'_>>(SUB_FILTER)
        .map(|arr| arr.iter::<Name>().map(|n| n.as_str().to_string()).collect())
        .unwrap_or_default();

    let digest_method = sv
        .get::<Array<'_>>(DIGEST_METHOD)
        .map(|arr| arr.iter::<Name>().map(|n| n.as_str().to_string()).collect())
        .unwrap_or_default();

    let reasons = sv
        .get::<Array<'_>>(REASONS)
        .map(|arr| {
            arr.iter::<pdf_syntax::object::String>()
                .map(|s| pdf_string_to_string(&s))
                .collect()
        })
        .unwrap_or_default();

    let reason_required = sv.get::<u32>(REASON).unwrap_or(0) != 0;

    Some(SeedValueConstraints {
        sub_filter,
        digest_method,
        reasons,
        reason_required,
    })
}

fn parse_lock_action(dict: &Dict<'_>) -> Option<LockAction> {
    let action = dict.get::<Name>(ACTION_KEY)?;
    match action.as_ref() {
        b"All" => Some(LockAction::All),
        b"Include" => {
            let fields = extract_field_names(dict);
            Some(LockAction::Include(fields))
        }
        b"Exclude" => {
            let fields = extract_field_names(dict);
            Some(LockAction::Exclude(fields))
        }
        _ => None,
    }
}

fn extract_field_names(dict: &Dict<'_>) -> Vec<String> {
    dict.get::<Array<'_>>(FIELDS)
        .map(|arr| {
            arr.iter::<pdf_syntax::object::String>()
                .map(|s| pdf_string_to_string(&s))
                .collect()
        })
        .unwrap_or_default()
}
