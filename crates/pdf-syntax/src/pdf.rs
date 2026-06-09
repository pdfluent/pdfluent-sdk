//! The starting point for reading PDF files.

use crate::PdfData;
use crate::object::Object;
use crate::page::Pages;
use crate::page::cached::CachedPages;
use crate::reader::Reader;
use crate::sync::Arc;
use crate::xref::{XRef, XRefError, fallback, root_xref};

pub use crate::crypto::DecryptionError;
use crate::metadata::Metadata;

/// Structural recovery that occurred while loading a [`Pdf`].
///
/// Recovery is always attempted automatically; these flags let a caller learn
/// that it happened, so a repaired document is distinguishable from a clean
/// one. They never change the (always-on) recovery behaviour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoadRecovery {
    /// The cross-reference table was invalid and rebuilt by scanning the file
    /// for objects. Object recovery may be incomplete.
    pub xref_rebuilt: bool,
    /// The page tree was invalid and pages were recovered by a brute-force
    /// scan. Page order may differ from the source.
    pub page_tree_rebuilt: bool,
}

/// A PDF file.
pub struct Pdf {
    xref: Arc<XRef>,
    header_version: PdfVersion,
    pages: CachedPages,
    data: PdfData,
    recovery: LoadRecovery,
}

/// Maximum number of xref entries (indirect objects) allowed in a single PDF.
///
/// PDFs exceeding this limit are rejected with [`LoadPdfError::TooLarge`] to
/// prevent unbounded memory growth. Corpus data shows legitimate documents
/// rarely exceed 50 K objects; 500 K is a safe, generous upper bound. (#497)
pub const MAX_OBJECTS: usize = 500_000;

/// Maximum number of pages allowed in a single PDF.
///
/// Traversal of the page tree is capped at this value and documents that
/// exceed it are rejected with [`LoadPdfError::TooLarge`]. (#497)
pub const MAX_PAGES: usize = 50_000;

/// Parser-internal limits applied while loading a PDF.
///
/// The default preserves the historical parser behavior (no caps). Callers
/// that need stricter limits can set individual caps before loading.
#[derive(Debug, Clone, Copy)]
pub struct PdfLoadLimits {
    /// `u32::MAX` is the "no cap" sentinel (same pattern as `max_image_pixels`).
    max_object_depth: u32,
    /// `u32::MAX` is the "no cap" sentinel.
    max_image_pixels: u32,
    /// Maximum decoded stream size in bytes.
    ///
    /// `u32::MAX` is the "no cap" sentinel (same pattern as `max_image_pixels`).
    /// When set below `u32::MAX`, `Stream::decoded()` / `Stream::decoded_image()`
    /// return `Err(DecodeFailure::StreamTooLarge { .. })` if the decoded payload
    /// exceeds this threshold. The raw (compressed) bytes are not checked;
    /// only the fully-decoded output is.
    ///
    /// Stored as `u32` (max ~4 GB) to keep `PdfLoadLimits` at 12 bytes, which
    /// prevents it from inflating `Array<'a>` and therefore `Object<'a>`.
    /// Caller-supplied `u64` values are clamped to `u32::MAX - 1` on conversion.
    max_stream_bytes: u32,
}

impl Default for PdfLoadLimits {
    fn default() -> Self {
        // `u32::MAX` is the "no cap" sentinel for all three limits. A raw
        // `Default::default()` would produce 0 everywhere, which would reject
        // every image / stream and silently break rendering.
        Self {
            max_object_depth: u32::MAX,
            max_image_pixels: u32::MAX,
            max_stream_bytes: u32::MAX,
        }
    }
}

impl PdfLoadLimits {
    /// Create a limit set with no caller overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum page-tree/object traversal depth.
    pub fn max_object_depth(mut self, depth: u32) -> Self {
        self.max_object_depth = depth;
        self
    }

    /// Set the maximum decoded image pixel count.
    pub fn max_image_pixels(mut self, pixels: u64) -> Self {
        self.max_image_pixels = u32::try_from(pixels).unwrap_or(u32::MAX);
        self
    }

    /// Set the maximum decoded stream size in bytes.
    ///
    /// Values above ~4 GB are clamped to `u32::MAX - 1` (≈ 4 GB − 1) due to
    /// internal storage as `u32`. For the intended use-case (decompression-bomb
    /// protection at 32 MB–1 GB), the 4 GB ceiling is more than sufficient.
    ///
    /// Any call to [`Stream::decoded`] or [`Stream::decoded_image`] that
    /// produces more bytes than `max_bytes` returns
    /// `Err(DecodeFailure::StreamTooLarge { observed, limit })`.
    pub fn max_stream_bytes(mut self, max_bytes: u64) -> Self {
        // Clamp to u32::MAX - 1 to distinguish from the "no limit" sentinel.
        self.max_stream_bytes = u32::try_from(max_bytes).unwrap_or(u32::MAX - 1);
        self
    }

    pub(crate) fn object_depth_limit(self) -> Option<u32> {
        if self.max_object_depth == u32::MAX {
            None
        } else {
            Some(self.max_object_depth)
        }
    }

    pub(crate) fn image_pixel_limit(self) -> Option<u32> {
        if self.max_image_pixels == u32::MAX {
            None
        } else {
            Some(self.max_image_pixels)
        }
    }

    pub(crate) fn stream_byte_limit(self) -> Option<u64> {
        if self.max_stream_bytes == u32::MAX {
            None
        } else {
            Some(u64::from(self.max_stream_bytes))
        }
    }
}

/// An error that occurred while loading a PDF file.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LoadPdfError {
    /// An error occurred while processing an encrypted document.
    Decryption(DecryptionError),
    /// The PDF was invalid or could not be parsed due to some other unknown reason.
    Invalid,
    /// The PDF exceeds a configured size limit (object count or page count).
    ///
    /// The first field is the number of xref objects; the second is the page
    /// count. Either or both may have triggered the limit. (#497)
    TooLarge(usize, usize),
}

#[allow(clippy::len_without_is_empty)]
impl Pdf {
    /// Try to read the given PDF file.
    ///
    /// Returns `Err` if it was unable to read it.
    pub fn new(data: impl Into<PdfData>) -> Result<Self, LoadPdfError> {
        Self::new_with_password(data, "")
    }

    /// Try to read the given PDF file with parser load limits.
    ///
    /// Returns `Err` if it was unable to read it.
    pub fn new_with_limits(
        data: impl Into<PdfData>,
        limits: PdfLoadLimits,
    ) -> Result<Self, LoadPdfError> {
        Self::new_with_password_and_limits(data, "", limits)
    }

    /// Try to read the given PDF file with a password.
    ///
    /// Returns `Err` if it was unable to read it or if the password is incorrect.
    pub fn new_with_password(
        data: impl Into<PdfData>,
        password: &str,
    ) -> Result<Self, LoadPdfError> {
        Self::new_with_password_and_limits(data, password, PdfLoadLimits::default())
    }

    /// Try to read the given PDF file with a password and parser load limits.
    ///
    /// Returns `Err` if it was unable to read it or if the password is incorrect.
    pub fn new_with_password_and_limits(
        data: impl Into<PdfData>,
        password: &str,
        limits: PdfLoadLimits,
    ) -> Result<Self, LoadPdfError> {
        let data = data.into();
        let password = password.as_bytes();
        let version = find_version(data.as_ref()).unwrap_or(PdfVersion::Pdf10);
        let mut xref_rebuilt = false;
        let xref = match root_xref(data.clone(), password, limits) {
            Ok(x) => x,
            Err(e) => match e {
                XRefError::Unknown => {
                    // The xref table was invalid; rebuild it by scanning objects.
                    xref_rebuilt = true;
                    fallback(data.clone(), password, limits).ok_or(LoadPdfError::Invalid)?
                }
                XRefError::Encryption(e) => return Err(LoadPdfError::Decryption(e)),
            },
        };
        let xref = Arc::new(xref);

        // Reject documents whose xref table exceeds the object limit.
        // This fires before we decode any object data, so the cost is minimal.
        // The limit prevents unbounded memory growth on adversarially large PDFs. (#497)
        let object_count = xref.len();
        if object_count > MAX_OBJECTS {
            return Err(LoadPdfError::TooLarge(object_count, 0));
        }

        let pages = CachedPages::new(xref.clone()).ok_or(LoadPdfError::Invalid)?;

        // Reject documents whose page tree resolves to more pages than allowed.
        // resolve_pages already caps traversal at MAX_PAGE_COUNT (100 K); checking
        // against our stricter MAX_PAGES (50 K) here gives a clean error instead
        // of silently truncating. (#497)
        let page_count = pages.get().len();
        if page_count > MAX_PAGES {
            return Err(LoadPdfError::TooLarge(object_count, page_count));
        }

        let recovery = LoadRecovery {
            xref_rebuilt,
            page_tree_rebuilt: pages.page_tree_rebuilt(),
        };

        Ok(Self {
            xref,
            header_version: version,
            pages,
            data,
            recovery,
        })
    }

    /// Structural recovery applied while loading this document (xref / page-tree
    /// rebuild). All-`false` for a document that parsed cleanly.
    pub fn load_recovery(&self) -> LoadRecovery {
        self.recovery
    }

    /// Return the number of objects present in the PDF file.
    pub fn len(&self) -> usize {
        self.xref.len()
    }

    /// Return an iterator over all objects defined in the PDF file.
    pub fn objects(&self) -> impl IntoIterator<Item = Object<'_>> {
        self.xref.objects()
    }

    /// Return the version of the PDF file.
    pub fn version(&self) -> PdfVersion {
        self.xref
            .trailer_data()
            .version
            .unwrap_or(self.header_version)
    }

    /// Return the underlying data of the PDF file.
    pub fn data(&self) -> &PdfData {
        &self.data
    }

    /// Return the pages of the PDF file.
    pub fn pages(&self) -> &Pages<'_> {
        self.pages.get()
    }

    /// Return the xref of the PDF file.
    pub fn xref(&self) -> &XRef {
        &self.xref
    }

    /// Return the metadata in the document information dictionary of the document.
    pub fn metadata(&self) -> &Metadata {
        self.xref.metadata()
    }
}

fn find_version(data: &[u8]) -> Option<PdfVersion> {
    let data = &data[..data.len().min(2000)];
    let mut r = Reader::new(data);

    while r.forward_tag(b"%PDF-").is_none() {
        r.read_byte()?;
    }

    PdfVersion::from_bytes(r.tail()?)
}

/// The version of a PDF document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PdfVersion {
    /// PDF 1.0.
    Pdf10,
    /// PDF 1.1.
    Pdf11,
    /// PDF 1.2.
    Pdf12,
    /// PDF 1.3.
    Pdf13,
    /// PDF 1.4.
    Pdf14,
    /// PDF 1.5.
    Pdf15,
    /// PDF 1.6.
    Pdf16,
    /// PDF 1.7.
    Pdf17,
    /// PDF 2.0.
    Pdf20,
}

impl PdfVersion {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes.get(..3)? {
            b"1.0" => Some(Self::Pdf10),
            b"1.1" => Some(Self::Pdf11),
            b"1.2" => Some(Self::Pdf12),
            b"1.3" => Some(Self::Pdf13),
            b"1.4" => Some(Self::Pdf14),
            b"1.5" => Some(Self::Pdf15),
            b"1.6" => Some(Self::Pdf16),
            b"1.7" => Some(Self::Pdf17),
            b"2.0" => Some(Self::Pdf20),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::pdf::{Pdf, PdfVersion};

    #[test]
    fn issue_49() {
        let _ = Pdf::new(Vec::new());
    }

    #[test]
    #[ignore = "requires hayro-tests corpus"]
    fn pdf_version_header() {
        let data = std::fs::read("../hayro-tests/downloads/pdfjs/alphatrans.pdf").unwrap();
        let pdf = Pdf::new(data).unwrap();

        assert_eq!(pdf.version(), PdfVersion::Pdf17);
    }

    #[test]
    #[ignore = "requires hayro-tests corpus"]
    fn pdf_version_catalog() {
        let data = std::fs::read("../hayro-tests/downloads/pdfbox/2163.pdf").unwrap();
        let pdf = Pdf::new(data).unwrap();

        assert_eq!(pdf.version(), PdfVersion::Pdf14);
    }
}
