//! DocMDP and FieldMDP permission handling (ISO 32000-2 §12.8.4).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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
        let Some(method) = ref_dict.get::<Name>(TRANSFORM_METHOD) else {
            continue;
        };
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

#[cfg(test)]
mod field_mdp_tests {
    use super::*;

    fn lock(action: LockAction) -> FieldMdpInfo {
        FieldMdpInfo {
            action,
            signing_field: Some("Signature1".into()),
        }
    }

    /// FieldMDP is de reden dat een handtekening iets waard is: hij legt vast
    /// welke velden ná ondertekening nog gewijzigd mogen worden (ISO 32000-2
    /// §12.8.2.4). `Include` en `Exclude` omdraaien is hier de gevaarlijke
    /// fout — dan laat je precies de velden bewerken die de ondertekenaar heeft
    /// dichtgezet, en het bestand ziet er verder normaal uit.
    #[test]
    fn include_locks_only_the_named_fields() {
        let locks = vec![lock(LockAction::Include(vec![
            "Bedrag".into(),
            "Datum".into(),
        ]))];
        assert!(
            is_field_locked("Bedrag", &locks),
            "genoemd veld hoort vergrendeld"
        );
        assert!(is_field_locked("Datum", &locks));
        assert!(
            !is_field_locked("Opmerking", &locks),
            "een veld dat er niet in staat blijft bewerkbaar"
        );
    }

    #[test]
    fn exclude_locks_everything_except_the_named_fields() {
        let locks = vec![lock(LockAction::Exclude(vec!["Opmerking".into()]))];
        assert!(
            !is_field_locked("Opmerking", &locks),
            "het uitgezonderde veld blijft juist bewerkbaar"
        );
        assert!(
            is_field_locked("Bedrag", &locks),
            "al het andere is vergrendeld"
        );
    }

    #[test]
    fn all_locks_every_field() {
        let locks = vec![lock(LockAction::All)];
        assert!(is_field_locked("wat dan ook", &locks));
        assert!(is_field_locked("", &locks));
    }

    #[test]
    fn without_locks_nothing_is_locked() {
        assert!(!is_field_locked("Bedrag", &[]));
    }

    /// Meerdere handtekeningen kunnen elk hun eigen slot leggen. Vergrendeld
    /// door één ervan is vergrendeld — een later slot kan een eerder slot niet
    /// opheffen.
    #[test]
    fn locks_from_several_signatures_accumulate() {
        let locks = vec![
            lock(LockAction::Include(vec!["Bedrag".into()])),
            lock(LockAction::Include(vec!["Datum".into()])),
        ];
        assert!(is_field_locked("Bedrag", &locks));
        assert!(is_field_locked("Datum", &locks));
        assert!(!is_field_locked("Opmerking", &locks));
    }

    /// Veldnamen zijn hoofdlettergevoelig in PDF. Een vergelijking die dat
    /// negeert zou een veld vergrendelen dat de ondertekenaar niet bedoelde.
    #[test]
    fn field_names_are_matched_exactly() {
        let locks = vec![lock(LockAction::Include(vec!["Bedrag".into()]))];
        assert!(!is_field_locked("bedrag", &locks));
        assert!(!is_field_locked("Bedrag ", &locks));
    }
}
