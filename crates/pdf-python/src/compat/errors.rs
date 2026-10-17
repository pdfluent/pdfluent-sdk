//! Compatible exception classes: one definition each, shared by every lane.
//!
//! The lanes used to define their own copies of these classes, so an
//! `except` on the exported class could not catch what another lane raised.
//! Every lane imports from here instead, and [`register`] publishes the same
//! class objects on the module. Base classes follow the recorded `__mro__`
//! where a case pins one (the `p0-exceptions-*` cases) and the closest
//! builtin otherwise.
//!
//! This module also holds the ONE bad-argument mapping: every door raise for
//! a wrongly shaped or typed argument goes through a [`ValidationKind`],
//! and every facade error on a member whose recorded class differs from the
//! default goes through [`member_error`]. Cases only pin the class name,
//! never the message.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::compat::ErrorKind as FacadeKind;
use pyo3::exceptions::{
    PyAssertionError, PyAttributeError, PyDeprecationWarning, PyException, PyFileNotFoundError,
    PyIndexError, PyKeyError, PyModuleNotFoundError, PyRuntimeError, PySyntaxError, PyTypeError,
    PyValueError,
};
use pyo3::prelude::*;

use crate::{PdfluentError, PdfluentIoError, PdfluentParseError};

pyo3::create_exception!(
    pdfluent,
    FileDataError,
    PyRuntimeError,
    "Raised when file bytes do not parse as a PDF."
);

pyo3::create_exception!(
    pdfluent,
    EmptyFileError,
    FileDataError,
    "Raised when the input carries no bytes at all."
);

pyo3::create_exception!(
    pdfluent,
    FileNotFoundError,
    PyRuntimeError,
    "Raised when the named input file does not exist."
);

pyo3::create_exception!(
    pdfluent,
    FitzDeprecation,
    PyDeprecationWarning,
    "Warns about a deprecated compatible call."
);

pyo3::create_exception!(
    pdfluent,
    FzErrorArgument,
    PyValueError,
    "Raised when a compat call passes an unacceptable argument."
);

pyo3::create_exception!(
    pdfluent,
    FzErrorSystem,
    PyRuntimeError,
    "Raised when a compat call fails in file or engine handling."
);

pyo3::create_exception!(
    pdfluent,
    FzErrorFormat,
    PyValueError,
    "Raised when bytes are not a readable image or document."
);

pyo3::create_exception!(
    pdfluent,
    FzErrorSyntax,
    PySyntaxError,
    "Raised when a compat call hands the engine unparsable PDF syntax."
);

/// Build the file-data error.
pub(crate) fn file_data_error(message: impl Into<String>) -> PyErr {
    FileDataError::new_err(message.into())
}

/// Build the empty-input error.
///
/// Called by the compatible `Document` constructor once that lands
/// (P1 follow-up, #643); unused until then.
#[allow(dead_code)]
pub(crate) fn empty_file_error(message: impl Into<String>) -> PyErr {
    EmptyFileError::new_err(message.into())
}

/// Build the missing-file error.
///
/// Called by the compatible `Document` constructor once that lands
/// (P1 follow-up, #643); unused until then.
#[allow(dead_code)]
pub(crate) fn file_not_found_error(message: impl Into<String>) -> PyErr {
    FileNotFoundError::new_err(message.into())
}

/// Publish every exception class on the module, exactly once.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("FileDataError", m.py().get_type::<FileDataError>())?;
    m.add("EmptyFileError", m.py().get_type::<EmptyFileError>())?;
    m.add("FileNotFoundError", m.py().get_type::<FileNotFoundError>())?;
    m.add("FitzDeprecation", m.py().get_type::<FitzDeprecation>())?;
    m.add("FzErrorArgument", m.py().get_type::<FzErrorArgument>())?;
    m.add("FzErrorSystem", m.py().get_type::<FzErrorSystem>())?;
    m.add("FzErrorFormat", m.py().get_type::<FzErrorFormat>())?;
    m.add("FzErrorSyntax", m.py().get_type::<FzErrorSyntax>())?;
    Ok(())
}

/// One validation kind per recorded bad-argument class.
///
/// The door never raises a bare `PyTypeError`/`PyValueError` for an argument
/// shape again: it names the kind and this table picks the class the batch
/// recordings pin for it.
#[derive(Clone, Copy)]
pub(crate) enum ValidationKind {
    /// Bad drawing input (points, rects, quads, point lists): `ValueError`.
    DrawArg,
    /// Bad box where a page or annotation box is set: `AssertionError`.
    SetBox,
    /// Failed caller precondition (live handle, known format): `AssertionError`.
    Precondition,
    /// Non-string where text is required: `AttributeError`.
    TextArg,
    /// Missing key in a link or option dict: `KeyError`.
    MissingKey,
    /// Bad object number or reference: `ValueError`.
    BadXref,
    /// Unparsable PDF syntax handed to the engine: `FzErrorSyntax`.
    BadSyntax,
    /// Page or step number outside the document: `RuntimeError`.
    OutOfRange,
    /// Wrong type with no recorded special case: `TypeError`.
    WrongType,
    /// File failure inside a save path: `FzErrorSystem`.
    SaveFailed,
    /// Missing attribute access: `AttributeError`.
    NoAttr,
    /// Total refusal: `Exception`.
    Refused,
}

/// Build the recorded error for one validation kind.
pub(crate) fn validation_error(kind: ValidationKind, message: impl Into<String>) -> PyErr {
    let message = message.into();
    match kind {
        ValidationKind::DrawArg => PyValueError::new_err(message),
        ValidationKind::SetBox => PyAssertionError::new_err(message),
        ValidationKind::Precondition => PyAssertionError::new_err(message),
        ValidationKind::TextArg => PyAttributeError::new_err(message),
        ValidationKind::MissingKey => PyKeyError::new_err(message),
        ValidationKind::BadXref => PyValueError::new_err(message),
        ValidationKind::BadSyntax => FzErrorSyntax::new_err(message),
        ValidationKind::OutOfRange => PyRuntimeError::new_err(message),
        ValidationKind::WrongType => PyTypeError::new_err(message),
        ValidationKind::SaveFailed => FzErrorSystem::new_err(message),
        ValidationKind::NoAttr => PyAttributeError::new_err(message),
        ValidationKind::Refused => PyException::new_err(message),
    }
}

/// Converter failures to the open classes: EPUB failures stay parse errors,
/// undecodable images read as bad file data, temp-file faults read as I/O,
/// and unsupported formats or a missing Chrome surface verbatim.
pub(crate) fn open_convert_error(error: pdfluent::Error) -> PyErr {
    match &error {
        pdfluent::Error::InvalidPdf { reason, .. } => PdfluentParseError::new_err(reason.clone()),
        pdfluent::Error::Unsupported(reason) => PdfluentError::new_err(reason.clone()),
        pdfluent::Error::Io { source, path } => match path {
            Some(path) => PdfluentIoError::new_err(format!("{}: {source}", path.display())),
            None => PdfluentIoError::new_err(source.to_string()),
        },
        _ => {
            if matches!(error.compat_kind(), Some(FacadeKind::UnreadableFile)) {
                file_data_error(error.to_string())
            } else {
                PdfluentError::new_err(error.to_string())
            }
        }
    }
}

/// Facade errors to the recorded Python exception classes.
///
/// This is the default every member inherits; [`member_error`] overrides it
/// per member where a recording pins a different class.
pub(crate) fn facade_error(error: pdfluent::Error) -> PyErr {
    if let pdfluent::Error::Unsupported(reason) = &error {
        return PdfluentError::new_err(reason.clone());
    }
    if let Some(kind) = error.compat_kind() {
        let message = error.to_string();
        return match kind {
            FacadeKind::WrongType => PyTypeError::new_err(message),
            FacadeKind::BadValue => FzErrorArgument::new_err(message),
            FacadeKind::UnknownKey => PyKeyError::new_err(message),
            FacadeKind::IndexOutOfRange => PyIndexError::new_err(message),
            FacadeKind::MissingFile
            | FacadeKind::UnreadableFile
            | FacadeKind::EmptyFile
            | FacadeKind::MissingComponent => FzErrorSystem::new_err(message),
            FacadeKind::EngineFault => PyException::new_err(message),
            FacadeKind::Deprecation => PyRuntimeError::new_err(message),
            _ => PyException::new_err(message),
        };
    }
    match error {
        pdfluent::Error::FileNotFound { .. } => FzErrorSystem::new_err(error.to_string()),
        pdfluent::Error::InvalidPdf { .. } => FzErrorSystem::new_err(error.to_string()),
        other => PdfluentError::new_err(other.to_string()),
    }
}

/// Members whose recorded class differs from [`facade_error`].
#[derive(Clone, Copy)]
pub(crate) enum Member {
    UpdateStream,
    XrefSetKey,
    SetContents,
    EmbfileDel,
    EmbfileUpd,
    UpdateObject,
    PageAnnotXrefs,
    InsertFile,
    SetLayer,
    SetOcmd,
    SetOc,
    SetPagelayout,
    SetPagemode,
    DeleteImage,
    ReplaceImage,
    GetBboxlog,
    TextCapture,
    ConvertToPdf,
    WriteText,
}

/// Facade error on one member: the override table, else [`facade_error`].
pub(crate) fn member_error(member: Member, error: pdfluent::Error) -> PyErr {
    if let pdfluent::Error::Unsupported(reason) = &error {
        return PdfluentError::new_err(reason.clone());
    }
    let kind = error.compat_kind();
    if matches!(member, Member::GetBboxlog | Member::TextCapture) {
        // L1/L2 recordings pin the plain builtins here, not the `FzError*`
        // default: bad values stay `ValueError`, engine faults stay
        // `RuntimeError`, and kind-less engine errors stay `RuntimeError`.
        let message = error.to_string();
        return match kind {
            Some(FacadeKind::WrongType) => validation_error(ValidationKind::WrongType, message),
            Some(FacadeKind::BadValue) => validation_error(ValidationKind::BadXref, message),
            Some(FacadeKind::UnknownKey) => validation_error(ValidationKind::MissingKey, message),
            Some(FacadeKind::IndexOutOfRange) => PyIndexError::new_err(message),
            Some(FacadeKind::MissingFile) => PyFileNotFoundError::new_err(message),
            Some(FacadeKind::UnreadableFile) => {
                PyRuntimeError::new_err(format!("cannot parse document: {error}"))
            }
            Some(FacadeKind::MissingComponent) => PyModuleNotFoundError::new_err(message),
            _ => validation_error(ValidationKind::OutOfRange, message),
        };
    }
    if matches!(member, Member::InsertFile) && matches!(kind, Some(FacadeKind::MissingFile)) {
        return FileNotFoundError::new_err(error.to_string());
    }
    let over = match (member, kind) {
        (Member::UpdateStream, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        (Member::XrefSetKey, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        (Member::SetContents, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        (Member::EmbfileDel, Some(FacadeKind::UnknownKey)) => Some(ValidationKind::BadXref),
        (Member::EmbfileUpd, Some(FacadeKind::UnknownKey)) => Some(ValidationKind::BadXref),
        (Member::UpdateObject, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadSyntax),
        (Member::PageAnnotXrefs, Some(_)) => Some(ValidationKind::OutOfRange),
        (Member::SetLayer, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        (Member::SetOcmd, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        (Member::SetOc, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        (Member::SetPagelayout, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        (Member::SetPagemode, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        (Member::DeleteImage, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        (Member::ReplaceImage, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        (Member::ConvertToPdf, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        (Member::ConvertToPdf, Some(FacadeKind::IndexOutOfRange)) => Some(ValidationKind::BadXref),
        (Member::WriteText, Some(FacadeKind::BadValue)) => Some(ValidationKind::BadXref),
        _ => None,
    };
    match over {
        Some(kind) => validation_error(kind, error.to_string()),
        None => facade_error(error),
    }
}
