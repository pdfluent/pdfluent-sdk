//! Link annotations, actions, and destinations.

extern crate alloc;

use crate::annotation::Annotation;
use crate::types::*;
use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Dict, Name, Object};

/// A link annotation (ISO 32000-2 §12.5.6.5).
#[derive(Debug)]
pub struct LinkAnnotation {
    /// The action associated with the link.
    pub action: Option<Action>,
    /// A direct destination.
    pub destination: Option<Destination>,
    /// The highlight mode.
    pub highlight_mode: HighlightMode,
    /// Optional quad points for the link region.
    pub quad_points: Option<QuadPoints>,
}

impl LinkAnnotation {
    /// Extract link annotation properties.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let dict = annot.dict();
        let action = dict.get::<Dict<'_>>(A).map(|d| Action::from_dict(&d));
        let destination = if action.is_none() {
            dict.get::<Object<'_>>(DEST).and_then(parse_destination)
        } else {
            None
        };
        let highlight_mode = dict
            .get::<Name>(H)
            .map(|n| match n.as_ref() {
                b"N" => HighlightMode::None,
                b"O" => HighlightMode::Outline,
                b"P" => HighlightMode::Push,
                _ => HighlightMode::Invert,
            })
            .unwrap_or(HighlightMode::Invert);
        let quad_points = annot.quad_points();
        Self {
            action,
            destination,
            highlight_mode,
            quad_points,
        }
    }
}

/// Known PDF action types per ISO 32000-2 §12.6.4.
///
/// The non-`Unknown` variants are the action subtypes the parser
/// recognizes and decodes into typed [`Action`] values. `Unknown`
/// preserves the original `/S` name so callers can still log or audit
/// vendor-extension actions without losing information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionType {
    /// `/URI` — open a URL in the default browser.
    Uri,
    /// `/GoTo` — jump to a destination within the same document.
    GoTo,
    /// `/GoToR` — jump to a destination in another (remote) PDF file.
    GoToR,
    /// `/Named` — execute one of the PDF-defined named actions
    /// (NextPage, FirstPage, Print, Find, …).
    Named,
    /// `/JavaScript` — execute embedded JavaScript. Security-sensitive;
    /// see [`is_inert_on_flatten`](ActionType::is_inert_on_flatten).
    JavaScript,
    /// `/SubmitForm` — POST form data to a remote URL.
    /// Security-sensitive.
    SubmitForm,
    /// `/Launch` — launch an external application or open a local file.
    /// Highly security-sensitive (arbitrary code execution path).
    Launch,
    /// `/ImportData` — import FDF form data from an external file.
    /// Security-sensitive.
    ImportData,
    /// An action subtype the parser did not recognize. The inner string
    /// preserves the original `/S` name verbatim so callers can audit
    /// vendor extensions.
    Unknown(alloc::string::String),
}

impl ActionType {
    /// Map a PDF `/S` action-type name (without leading slash) to an
    /// `ActionType`. Unknown names are preserved verbatim in the
    /// [`Unknown`](ActionType::Unknown) variant.
    pub fn from_name(name: &str) -> Self {
        match name {
            "URI" => Self::Uri,
            "GoTo" => Self::GoTo,
            "GoToR" => Self::GoToR,
            "Named" => Self::Named,
            "JavaScript" => Self::JavaScript,
            "SubmitForm" => Self::SubmitForm,
            "Launch" => Self::Launch,
            "ImportData" => Self::ImportData,
            _ => Self::Unknown(name.into()),
        }
    }

    /// Whether this action type should be stripped (made inert) when
    /// flattening the document.
    ///
    /// `true` for actions that have side effects beyond navigation —
    /// `JavaScript`, `SubmitForm`, `Launch`, `ImportData`. A flattened
    /// PDF is meant to be a static archival artifact; any of these
    /// actions surviving into the flattened output is a security and
    /// archival-fidelity concern. Use this to decide whether to drop
    /// the action during flattening.
    pub fn is_inert_on_flatten(&self) -> bool {
        matches!(
            self,
            Self::JavaScript | Self::SubmitForm | Self::Launch | Self::ImportData
        )
    }
}

/// An action (ISO 32000-2 §12.6).
#[derive(Debug, Clone)]
pub enum Action {
    /// A URI action.
    Uri(alloc::string::String),
    /// A GoTo action.
    GoTo(Destination),
    /// A GoToR action.
    GoToR {
        /// The file specification.
        file: alloc::string::String,
        /// The destination.
        destination: Option<Destination>,
    },
    /// A Named action.
    Named(alloc::string::String),
    /// A JavaScript action.
    JavaScript(alloc::string::String),
    /// Submit form data to a target URL.
    SubmitForm {
        /// The target file or URL from `/F`, if present.
        target: Option<alloc::string::String>,
    },
    /// Launch an external application or document.
    Launch {
        /// The target file specification from `/F`, if present.
        file: Option<alloc::string::String>,
    },
    /// Import form data from an external FDF file.
    ImportData {
        /// The target file specification from `/F`, if present.
        file: Option<alloc::string::String>,
    },
    /// Unknown action type.
    Unknown(alloc::string::String),
}

impl Action {
    /// Parse an action from an action dictionary.
    pub fn from_dict(dict: &Dict<'_>) -> Self {
        let action_type = dict
            .get::<Name>(S)
            .map(|n| alloc::string::String::from(n.as_str()))
            .unwrap_or_default();
        match ActionType::from_name(action_type.as_str()) {
            ActionType::Uri => {
                let uri = dict
                    .get::<pdf_syntax::object::String>(URI)
                    .map(|s| crate::annotation::pdf_string_to_string(&s))
                    .unwrap_or_default();
                Self::Uri(uri)
            }
            ActionType::GoTo => {
                let dest = dict
                    .get::<Object<'_>>(D)
                    .and_then(parse_destination)
                    .unwrap_or(Destination::Fit { page_index: None });
                Self::GoTo(dest)
            }
            ActionType::GoToR => {
                let file = file_spec_string(dict).unwrap_or_default();
                let destination = dict.get::<Object<'_>>(D).and_then(parse_destination);
                Self::GoToR { file, destination }
            }
            ActionType::Named => {
                let name = dict
                    .get::<Name>(N)
                    .map(|n| alloc::string::String::from(n.as_str()))
                    .unwrap_or_default();
                Self::Named(name)
            }
            ActionType::JavaScript => {
                let js = dict
                    .get::<pdf_syntax::object::String>(JS)
                    .map(|s| crate::annotation::pdf_string_to_string(&s))
                    .unwrap_or_default();
                Self::JavaScript(js)
            }
            ActionType::SubmitForm => Self::SubmitForm {
                target: file_spec_string(dict),
            },
            ActionType::Launch => Self::Launch {
                file: file_spec_string(dict),
            },
            ActionType::ImportData => Self::ImportData {
                file: file_spec_string(dict),
            },
            ActionType::Unknown(action_type) => Self::Unknown(action_type),
        }
    }

    /// The [`ActionType`] discriminator for this action — useful when
    /// you want to filter or count action types without matching every
    /// concrete variant. For [`Action::Unknown`], returns the preserved
    /// `/S` name inside `ActionType::Unknown`.
    pub fn action_type(&self) -> ActionType {
        match self {
            Self::Uri(_) => ActionType::Uri,
            Self::GoTo(_) => ActionType::GoTo,
            Self::GoToR { .. } => ActionType::GoToR,
            Self::Named(_) => ActionType::Named,
            Self::JavaScript(_) => ActionType::JavaScript,
            Self::SubmitForm { .. } => ActionType::SubmitForm,
            Self::Launch { .. } => ActionType::Launch,
            Self::ImportData { .. } => ActionType::ImportData,
            Self::Unknown(action_type) => ActionType::Unknown(action_type.clone()),
        }
    }
}

fn file_spec_string(dict: &Dict<'_>) -> Option<alloc::string::String> {
    dict.get::<pdf_syntax::object::String>(F)
        .map(|s| crate::annotation::pdf_string_to_string(&s))
        .or_else(|| {
            dict.get::<Dict<'_>>(F).and_then(|fs| {
                fs.get::<pdf_syntax::object::String>(UF)
                    .or_else(|| fs.get::<pdf_syntax::object::String>(F))
                    .map(|s| crate::annotation::pdf_string_to_string(&s))
            })
        })
}

/// A PDF destination (ISO 32000-2 §12.3.2) — a target location and
/// viewport recipe used by GoTo / GoToR actions and by direct `/Dest`
/// link entries.
///
/// `page_index` is 0-based and may be `None` when the source PDF stored
/// the destination as an indirect-reference array the parser could not
/// resolve back to a page index. The remaining fields encode the
/// "where on the page and at what zoom" portion of the destination.
#[derive(Debug, Clone)]
pub enum Destination {
    /// `/XYZ left top zoom` — go to a specific position with optional
    /// zoom factor. `None` for any field means "preserve current
    /// viewer setting".
    Xyz {
        /// 0-based page index, or `None` if unresolved.
        page_index: Option<u32>,
        /// Horizontal scroll position in PDF user-space points.
        left: Option<f32>,
        /// Vertical scroll position in PDF user-space points.
        top: Option<f32>,
        /// Zoom factor (1.0 == 100%). `None` preserves current zoom.
        zoom: Option<f32>,
    },
    /// `/Fit` — fit the entire page into the viewer window.
    Fit {
        /// 0-based page index, or `None` if unresolved.
        page_index: Option<u32>,
    },
    /// `/FitH top` — fit page width; align so `top` is at the top of
    /// the viewer.
    FitH {
        /// 0-based page index, or `None` if unresolved.
        page_index: Option<u32>,
        /// Vertical alignment in PDF user-space points.
        top: Option<f32>,
    },
    /// `/FitV left` — fit page height; align so `left` is at the left
    /// of the viewer.
    FitV {
        /// 0-based page index, or `None` if unresolved.
        page_index: Option<u32>,
        /// Horizontal alignment in PDF user-space points.
        left: Option<f32>,
    },
    /// `/FitR left bottom right top` — fit the given rectangle into
    /// the viewer window.
    FitR {
        /// 0-based page index, or `None` if unresolved.
        page_index: Option<u32>,
        /// Rectangle's left edge in PDF user-space points.
        left: f32,
        /// Rectangle's bottom edge in PDF user-space points.
        bottom: f32,
        /// Rectangle's right edge in PDF user-space points.
        right: f32,
        /// Rectangle's top edge in PDF user-space points.
        top: f32,
    },
    /// `/FitB` — fit the page's bounding box (the area containing
    /// non-blank content) into the viewer.
    FitB {
        /// 0-based page index, or `None` if unresolved.
        page_index: Option<u32>,
    },
    /// `/FitBH top` — fit the page bounding-box width; align so `top`
    /// is at the top of the viewer.
    FitBH {
        /// 0-based page index, or `None` if unresolved.
        page_index: Option<u32>,
        /// Vertical alignment in PDF user-space points.
        top: Option<f32>,
    },
    /// `/FitBV left` — fit the page bounding-box height; align so
    /// `left` is at the left of the viewer.
    FitBV {
        /// 0-based page index, or `None` if unresolved.
        page_index: Option<u32>,
        /// Horizontal alignment in PDF user-space points.
        left: Option<f32>,
    },
    /// A named destination — an indirection through the document's
    /// `/Names` tree. The string is the destination name; resolution
    /// to a concrete location requires looking it up in the document
    /// catalog's `/Names /Dests` entry.
    Named(alloc::string::String),
}

/// Link highlight mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightMode {
    /// No highlighting.
    None,
    /// Invert contents.
    Invert,
    /// Invert border.
    Outline,
    /// Push effect.
    Push,
}

/// Parse a destination from an Object.
pub fn parse_destination(obj: Object<'_>) -> Option<Destination> {
    match obj {
        Object::Array(arr) => {
            let mut iter = arr.flex_iter();
            let page_index = iter.next::<i32>().map(|n| n as u32);
            let dest_type = iter.next::<Name>()?;
            match dest_type.as_ref() {
                b"XYZ" => Some(Destination::Xyz {
                    page_index,
                    left: iter.next::<f32>(),
                    top: iter.next::<f32>(),
                    zoom: iter.next::<f32>(),
                }),
                b"Fit" => Some(Destination::Fit { page_index }),
                b"FitB" => Some(Destination::FitB { page_index }),
                b"FitH" => Some(Destination::FitH {
                    page_index,
                    top: iter.next::<f32>(),
                }),
                b"FitBH" => Some(Destination::FitBH {
                    page_index,
                    top: iter.next::<f32>(),
                }),
                b"FitV" => Some(Destination::FitV {
                    page_index,
                    left: iter.next::<f32>(),
                }),
                b"FitBV" => Some(Destination::FitBV {
                    page_index,
                    left: iter.next::<f32>(),
                }),
                b"FitR" => Some(Destination::FitR {
                    page_index,
                    left: iter.next::<f32>().unwrap_or(0.0),
                    bottom: iter.next::<f32>().unwrap_or(0.0),
                    right: iter.next::<f32>().unwrap_or(0.0),
                    top: iter.next::<f32>().unwrap_or(0.0),
                }),
                _ => None,
            }
        }
        Object::Name(name) => Some(Destination::Named(alloc::string::String::from(
            name.as_str(),
        ))),
        Object::String(s) => Some(Destination::Named(crate::annotation::pdf_string_to_string(
            &s,
        ))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_sensitive_actions_are_known_types() {
        assert_eq!(ActionType::from_name("SubmitForm"), ActionType::SubmitForm);
        assert_eq!(ActionType::from_name("Launch"), ActionType::Launch);
        assert_eq!(ActionType::from_name("ImportData"), ActionType::ImportData);
    }

    #[test]
    fn security_sensitive_actions_are_inert_on_flatten() {
        assert!(ActionType::JavaScript.is_inert_on_flatten());
        assert!(ActionType::SubmitForm.is_inert_on_flatten());
        assert!(ActionType::Launch.is_inert_on_flatten());
        assert!(ActionType::ImportData.is_inert_on_flatten());
        assert!(!ActionType::GoTo.is_inert_on_flatten());
        assert!(!ActionType::Uri.is_inert_on_flatten());
    }

    #[test]
    fn unknown_action_type_remains_auditable() {
        assert_eq!(
            ActionType::from_name("VendorAction"),
            ActionType::Unknown("VendorAction".into())
        );
    }
}
