//! Stamp, FileAttachment, FreeText, Sound, and Movie annotations.

extern crate alloc;

use crate::annotation::{self, Annotation};
use crate::types::LineEnding;
use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Array, Dict, Name, Rect, Stream};

/// Standard stamp names (ISO 32000-2 Table 181).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StampName {
    Approved,
    Experimental,
    NotApproved,
    AsIs,
    Expired,
    NotForPublicRelease,
    Confidential,
    Final,
    Sold,
    Departmental,
    ForComment,
    TopSecret,
    Draft,
    ForPublicRelease,
    /// A custom stamp name.
    Custom(alloc::string::String),
}

impl StampName {
    /// Parse from a PDF name.
    pub fn from_name(name: &[u8]) -> Self {
        match name {
            b"Approved" => Self::Approved,
            b"Experimental" => Self::Experimental,
            b"NotApproved" => Self::NotApproved,
            b"AsIs" => Self::AsIs,
            b"Expired" => Self::Expired,
            b"NotForPublicRelease" => Self::NotForPublicRelease,
            b"Confidential" => Self::Confidential,
            b"Final" => Self::Final,
            b"Sold" => Self::Sold,
            b"Departmental" => Self::Departmental,
            b"ForComment" => Self::ForComment,
            b"TopSecret" => Self::TopSecret,
            b"Draft" => Self::Draft,
            b"ForPublicRelease" => Self::ForPublicRelease,
            other => {
                let s = core::str::from_utf8(other).unwrap_or("Unknown").to_owned();
                Self::Custom(s)
            }
        }
    }
}

/// A Stamp annotation.
#[derive(Debug)]
pub struct StampAnnotation {
    /// The stamp name.
    pub name: StampName,
}

impl StampAnnotation {
    /// Extract stamp annotation properties.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let name = annot
            .dict()
            .get::<Name>(NAME)
            .map(|n: Name| StampName::from_name(n.as_ref()))
            .unwrap_or(StampName::Draft);
        Self { name }
    }
}

/// A FileAttachment annotation.
#[derive(Debug)]
pub struct FileAttachmentAnnotation<'a> {
    dict: Dict<'a>,
    /// The icon name.
    pub icon: alloc::string::String,
}

impl<'a> FileAttachmentAnnotation<'a> {
    /// Extract file attachment annotation properties.
    pub fn from_annot(annot: &Annotation<'a>) -> Self {
        let dict = annot.dict().clone();
        let icon = dict
            .get::<Name>(NAME)
            .map(|n: Name| alloc::string::String::from(n.as_str()))
            .unwrap_or_else(|| alloc::string::String::from("PushPin"));
        Self { dict, icon }
    }

    /// Return the file specification dictionary.
    pub fn file_spec(&self) -> Option<Dict<'a>> {
        self.dict.get::<Dict<'_>>(FS)
    }

    /// Return the embedded file stream.
    pub fn embedded_file(&self) -> Option<Stream<'a>> {
        let fs = self.file_spec()?;
        let ef = fs.get::<Dict<'_>>(EF)?;
        ef.get::<Stream<'_>>(F).or_else(|| ef.get::<Stream<'_>>(UF))
    }

    /// Return the filename.
    pub fn filename(&self) -> Option<alloc::string::String> {
        let fs = self.file_spec()?;
        fs.get::<pdf_syntax::object::String>(UF)
            .or_else(|| fs.get::<pdf_syntax::object::String>(F))
            .map(|s| annotation::pdf_string_to_string(&s))
    }
}

/// A FreeText annotation.
#[derive(Debug)]
pub struct FreeTextAnnotation {
    /// The default appearance string.
    pub default_appearance: alloc::string::String,
    /// Text justification (0=left, 1=center, 2=right).
    pub justification: u32,
    /// Default style string.
    pub default_style: Option<alloc::string::String>,
    /// Rich text content.
    pub rich_content: Option<alloc::string::String>,
    /// Callout line points.
    pub callout_line: Option<Vec<f32>>,
    /// Intent.
    pub intent: Option<alloc::string::String>,
    /// Line ending style for callout.
    pub line_ending: LineEnding,
    /// Rectangle differences.
    pub rd: Option<Rect>,
}

impl FreeTextAnnotation {
    /// Extract free text annotation properties.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let dict = annot.dict();
        let default_appearance = dict
            .get::<pdf_syntax::object::String>(DA)
            .map(|s| annotation::pdf_string_to_string(&s))
            .unwrap_or_default();
        let justification = dict.get::<u32>(Q).unwrap_or(0);
        let default_style = dict
            .get::<pdf_syntax::object::String>(DS)
            .map(|s| annotation::pdf_string_to_string(&s));
        let rich_content = dict
            .get::<pdf_syntax::object::String>(RC)
            .map(|s| annotation::pdf_string_to_string(&s));
        let callout_line: Option<Vec<f32>> = dict
            .get::<Array<'_>>(CL)
            .map(|arr: Array<'_>| arr.iter::<f32>().collect());
        let intent = dict
            .get::<Name>(IT)
            .map(|n: Name| alloc::string::String::from(n.as_str()));
        let line_ending = dict
            .get::<Name>(LE)
            .map(|n: Name| LineEnding::from_name(n.as_ref()))
            .unwrap_or(LineEnding::None);
        let rd = dict.get::<Rect>(RD);
        Self {
            default_appearance,
            justification,
            default_style,
            rich_content,
            callout_line,
            intent,
            line_ending,
            rd,
        }
    }
}

/// A Sound annotation.
#[derive(Debug)]
pub struct SoundAnnotation {
    /// The icon name.
    pub icon: alloc::string::String,
}

impl SoundAnnotation {
    /// Extract sound annotation properties.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let icon = annot
            .dict()
            .get::<Name>(NAME)
            .map(|n: Name| alloc::string::String::from(n.as_str()))
            .unwrap_or_else(|| alloc::string::String::from("Speaker"));
        Self { icon }
    }
}

/// A Movie annotation.
#[derive(Debug)]
pub struct MovieAnnotation {
    /// The movie title.
    pub title: Option<alloc::string::String>,
}

impl MovieAnnotation {
    /// Extract movie annotation properties.
    pub fn from_annot(annot: &Annotation<'_>) -> Self {
        let title = annot
            .dict()
            .get::<pdf_syntax::object::String>(T)
            .map(|s| annotation::pdf_string_to_string(&s));
        Self { title }
    }
}
