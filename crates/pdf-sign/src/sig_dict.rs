//! Signature dictionary wrapper (ISO 32000-2 §12.8.1).

use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Array, Dict, Name};

use crate::cms::CmsSignedData;
use crate::string_util::pdf_string_to_string;
use crate::types::SubFilter;

/// A wrapper around a PDF signature dictionary (/Type /Sig).
#[derive(Debug, Clone)]
pub struct SigDict<'a> {
    dict: Dict<'a>,
}

impl<'a> SigDict<'a> {
    /// Wrap a dictionary as a signature dictionary.
    pub fn from_dict(dict: Dict<'a>) -> Self {
        Self { dict }
    }

    /// Return the raw dictionary.
    pub fn dict(&self) -> &Dict<'a> {
        &self.dict
    }

    /// Return the /Filter value (typically "Adobe.PPKLite").
    pub fn filter(&self) -> Option<String> {
        self.dict
            .get::<Name>(FILTER)
            .map(|n| n.as_str().to_string())
    }

    /// Return the /SubFilter value.
    pub fn sub_filter(&self) -> Option<SubFilter> {
        self.dict
            .get::<Name>(SUB_FILTER)
            .and_then(|n| SubFilter::from_name(n.as_ref()))
    }

    /// Return the /ByteRange array as `[offset1, len1, offset2, len2]`.
    pub fn byte_range(&self) -> Option<[usize; 4]> {
        let arr = self.dict.get::<Array<'_>>(BYTERANGE)?;
        let vals: Vec<i64> = arr.iter::<i64>().collect();
        if vals.len() != 4 {
            return None;
        }
        Some([
            vals[0] as usize,
            vals[1] as usize,
            vals[2] as usize,
            vals[3] as usize,
        ])
    }

    /// Return the raw /Contents bytes (the CMS signature).
    pub fn contents_raw(&self) -> Option<Vec<u8>> {
        self.dict
            .get::<pdf_syntax::object::String>(CONTENTS)
            .map(|s| s.as_bytes().to_vec())
    }

    /// Parse the /Contents as a CMS SignedData structure.
    pub fn cms_signed_data(&self) -> Option<CmsSignedData> {
        let raw = self.contents_raw()?;
        CmsSignedData::from_der(&raw)
    }

    /// Return the /Name value (signer name).
    pub fn signer_name(&self) -> Option<String> {
        self.dict
            .get::<pdf_syntax::object::String>(NAME)
            .map(|s| pdf_string_to_string(&s))
    }

    /// Return the /Reason value.
    pub fn reason(&self) -> Option<String> {
        self.dict
            .get::<pdf_syntax::object::String>(REASON)
            .map(|s| pdf_string_to_string(&s))
    }

    /// Return the /Location value.
    pub fn location(&self) -> Option<String> {
        self.dict
            .get::<pdf_syntax::object::String>(LOCATION)
            .map(|s| pdf_string_to_string(&s))
    }

    /// Return the /ContactInfo value.
    pub fn contact_info(&self) -> Option<String> {
        self.dict
            .get::<pdf_syntax::object::String>(CONTACT_INFO)
            .map(|s| pdf_string_to_string(&s))
    }

    /// Return the /M (signing time) value.
    pub fn signing_time(&self) -> Option<String> {
        self.dict
            .get::<pdf_syntax::object::String>(M)
            .map(|s| pdf_string_to_string(&s))
    }

    /// Return the /Cert value(s).
    pub fn certificates_raw(&self) -> Vec<Vec<u8>> {
        if let Some(s) = self.dict.get::<pdf_syntax::object::String>(CERT) {
            return vec![s.as_bytes().to_vec()];
        }
        if let Some(arr) = self.dict.get::<Array<'_>>(CERT) {
            return arr
                .iter::<pdf_syntax::object::String>()
                .map(|s| s.as_bytes().to_vec())
                .collect();
        }
        Vec::new()
    }

    /// Return the /Reference array (signature reference dictionaries).
    pub fn references(&self) -> Vec<Dict<'a>> {
        self.dict
            .get::<Array<'_>>(SIG_REF)
            .map(|arr| arr.iter::<Dict<'_>>().collect())
            .unwrap_or_default()
    }
}
