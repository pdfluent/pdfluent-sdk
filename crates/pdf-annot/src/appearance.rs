//! Appearance dictionary handling (ISO 32000-2 §12.5.5).

use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Dict, Name, Stream};

/// Wrapper for an annotation's `/AP` (appearance) dictionary.
#[derive(Debug, Clone)]
pub struct AppearanceDict<'a> {
    dict: Dict<'a>,
}

impl<'a> AppearanceDict<'a> {
    /// Try to extract the appearance dictionary from an annotation dict.
    pub fn from_annot(annot_dict: &Dict<'a>) -> Option<Self> {
        let dict = annot_dict.get::<Dict<'_>>(AP)?;
        Some(Self { dict })
    }

    /// Return the normal appearance stream, resolving `/AS` if needed.
    pub fn normal(&self, annot_dict: &Dict<'a>) -> Option<Stream<'a>> {
        resolve_appearance(&self.dict, N, annot_dict)
    }

    /// Return the rollover appearance stream.
    pub fn rollover(&self, annot_dict: &Dict<'a>) -> Option<Stream<'a>> {
        resolve_appearance(&self.dict, R, annot_dict)
    }

    /// Return the down (mouse-pressed) appearance stream.
    pub fn down(&self, annot_dict: &Dict<'a>) -> Option<Stream<'a>> {
        resolve_appearance(&self.dict, D, annot_dict)
    }
}

/// Resolve an appearance entry.
fn resolve_appearance<'a>(
    ap_dict: &Dict<'a>,
    key: &[u8],
    annot_dict: &Dict<'a>,
) -> Option<Stream<'a>> {
    if let Some(stream) = ap_dict.get::<Stream<'_>>(key) {
        return Some(stream);
    }
    if let Some(sub_dict) = ap_dict.get::<Dict<'_>>(key) {
        let appearance_state = annot_dict.get::<Name>(AS)?;
        return sub_dict.get::<Stream<'_>>(appearance_state.as_ref());
    }
    None
}
