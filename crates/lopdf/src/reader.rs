use log::{error, warn};
use std::cmp;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::convert::TryInto;
#[cfg(not(feature = "async"))]
use std::fs::File;
#[cfg(not(feature = "async"))]
use std::io::Read;
use std::path::Path;
use std::sync::Mutex;

#[cfg(feature = "rayon")]
use rayon::prelude::*;
#[cfg(feature = "async")]
use tokio::fs::File;
#[cfg(feature = "async")]
use tokio::io::{AsyncRead, AsyncReadExt};
#[cfg(feature = "async")]
use tokio::pin;

use crate::common_data_structures;
use crate::encryption::{self, EncryptionState};
use crate::error::{ParseError, XrefError};
use crate::load_options::{FilterFunc, LoadOptions};
use crate::object_stream::ObjectStream;
use crate::parser;
use crate::xref::XrefEntry;
use crate::{Dictionary, Document, Error, IncrementalDocument, Object, ObjectId, Result};

#[cfg(not(feature = "async"))]
impl Document {
    /// Load a PDF document from a specified file path.
    #[inline]
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Document> {
        Self::load_with_options(path, LoadOptions::default())
    }

    /// Load a PDF document from a specified file path with the given options.
    #[inline]
    pub fn load_with_options<P: AsRef<Path>>(path: P, options: LoadOptions) -> Result<Document> {
        let file = File::open(path)?;
        let capacity = Some(file.metadata()?.len() as usize);
        Self::load_internal(file, capacity, options)
    }

    /// Load a PDF document from a specified file path with a password for encrypted PDFs.
    #[inline]
    pub fn load_with_password<P: AsRef<Path>>(path: P, password: &str) -> Result<Document> {
        Self::load_with_options(path, LoadOptions::with_password(password))
    }

    #[deprecated(since = "0.41.0", note = "Use load_with_options instead")]
    #[inline]
    pub fn load_filtered<P: AsRef<Path>>(path: P, filter_func: FilterFunc) -> Result<Document> {
        Self::load_with_options(path, LoadOptions::with_filter(filter_func))
    }

    /// Load a PDF document from an arbitrary source.
    #[inline]
    pub fn load_from<R: Read>(source: R) -> Result<Document> {
        Self::load_from_with_options(source, LoadOptions::default())
    }

    /// Load a PDF document from an arbitrary source with the given options.
    #[inline]
    pub fn load_from_with_options<R: Read>(source: R, options: LoadOptions) -> Result<Document> {
        Self::load_internal(source, None, options)
    }

    /// Load a PDF document from an arbitrary source with a password for encrypted PDFs.
    #[deprecated(since = "0.41.0", note = "Use load_from_with_options instead")]
    #[inline]
    pub fn load_from_with_password<R: Read>(source: R, password: &str) -> Result<Document> {
        Self::load_from_with_options(source, LoadOptions::with_password(password))
    }

    fn load_internal<R: Read>(mut source: R, capacity: Option<usize>, options: LoadOptions) -> Result<Document> {
        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer)?;

        // Same input-size check the memory entry points make. Upstream routes
        // load()/load_with_options()/load_from() through here and applies no
        // bound at all; without this the limit would only ever cover callers
        // who happened to use load_mem_with_options.
        if let Some(limit) = options.max_file_bytes
            && buffer.len() > limit
        {
            return Err(Error::DocumentTooLarge {
                size: buffer.len(),
                limit,
            });
        }

        let filter = options.filter;
        Reader {
            buffer: &buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: options.password.clone(),
            options,
        }
        .read(filter)
    }

    /// Load a PDF document from a memory slice.
    pub fn load_mem(buffer: &[u8]) -> Result<Document> {
        Self::load_mem_with_options(buffer, LoadOptions::default())
    }

    /// Load a PDF document from a memory slice with the given options.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::DocumentTooLarge)` if `options.max_file_bytes` is set
    /// and `buffer.len()` exceeds that limit.
    pub fn load_mem_with_options(buffer: &[u8], options: LoadOptions) -> Result<Document> {
        // Kept from this fork: reject inputs that exceed the configured size
        // limit before allocating the object graph. Upstream has no equivalent
        // check, so dropping it would remove the only bound on input size.
        if let Some(limit) = options.max_file_bytes
            && buffer.len() > limit
        {
            return Err(Error::DocumentTooLarge {
                size: buffer.len(),
                limit,
            });
        }
        let filter = options.filter;
        Reader {
            buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: options.password.clone(),
            options,
        }
        .read(filter)
    }

    /// Load a PDF document from a memory slice with a password for encrypted PDFs.
    #[deprecated(since = "0.41.0", note = "Use load_mem_with_options instead")]
    pub fn load_mem_with_password(buffer: &[u8], password: &str) -> Result<Document> {
        Self::load_mem_with_options(buffer, LoadOptions::with_password(password))
    }

    /// Load PDF metadata (title and page count) without loading the entire document.
    /// This is much faster for large PDFs when you only need basic information.
    #[inline]
    pub fn load_metadata<P: AsRef<Path>>(path: P) -> Result<PdfMetadata> {
        let file = File::open(path)?;
        let capacity = Some(file.metadata()?.len() as usize);
        Self::load_metadata_internal(file, capacity, None)
    }

    /// Load PDF metadata from a file path with a password for encrypted PDFs.
    #[inline]
    pub fn load_metadata_with_password<P: AsRef<Path>>(path: P, password: &str) -> Result<PdfMetadata> {
        let file = File::open(path)?;
        let capacity = Some(file.metadata()?.len() as usize);
        Self::load_metadata_internal(file, capacity, Some(password.to_string()))
    }

    /// Load PDF metadata from an arbitrary source without loading the entire document.
    #[inline]
    pub fn load_metadata_from<R: Read>(source: R) -> Result<PdfMetadata> {
        Self::load_metadata_internal(source, None, None)
    }

    /// Load PDF metadata from an arbitrary source with a password for encrypted PDFs.
    #[inline]
    pub fn load_metadata_from_with_password<R: Read>(source: R, password: &str) -> Result<PdfMetadata> {
        Self::load_metadata_internal(source, None, Some(password.to_string()))
    }

    /// Load PDF metadata from a memory slice without loading the entire document.
    #[inline]
    pub fn load_metadata_mem(buffer: &[u8]) -> Result<PdfMetadata> {
        Reader {
            buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: None,
            options: LoadOptions::default(),
        }
        .read_metadata()
    }

    /// Load PDF metadata from a memory slice with a password for encrypted PDFs.
    #[inline]
    pub fn load_metadata_mem_with_password(buffer: &[u8], password: &str) -> Result<PdfMetadata> {
        Reader {
            buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: Some(password.to_string()),
            options: LoadOptions::default(),
        }
        .read_metadata()
    }

    fn load_metadata_internal<R: Read>(
        mut source: R, capacity: Option<usize>, password: Option<String>,
    ) -> Result<PdfMetadata> {
        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer)?;

        Reader {
            buffer: &buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password,
            options: LoadOptions::default(),
        }
        .read_metadata()
    }
}

#[cfg(feature = "async")]
impl Document {
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Document> {
        Self::load_with_options(path, LoadOptions::default()).await
    }

    /// Load a PDF document from a specified file path with the given options.
    pub async fn load_with_options<P: AsRef<Path>>(path: P, options: LoadOptions) -> Result<Document> {
        let file = File::open(path).await?;
        let metadata = file.metadata().await?;
        let capacity = Some(metadata.len() as usize);
        Self::load_internal(file, capacity, options).await
    }

    /// Load a PDF document from a specified file path with a password for encrypted PDFs.
    pub async fn load_with_password<P: AsRef<Path>>(path: P, password: &str) -> Result<Document> {
        Self::load_with_options(path, LoadOptions::with_password(password)).await
    }

    #[deprecated(since = "0.41.0", note = "Use load_with_options instead")]
    pub async fn load_filtered<P: AsRef<Path>>(path: P, filter_func: FilterFunc) -> Result<Document> {
        Self::load_with_options(path, LoadOptions::with_filter(filter_func)).await
    }

    async fn load_internal<R: AsyncRead>(source: R, capacity: Option<usize>, options: LoadOptions) -> Result<Document> {
        pin!(source);

        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer).await?;

        let filter = options.filter;
        Reader {
            buffer: &buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: options.password.clone(),
            options,
        }
        .read(filter)
    }

    /// Load a PDF document from a memory slice.
    pub fn load_mem(buffer: &[u8]) -> Result<Document> {
        Self::load_mem_with_options(buffer, LoadOptions::default())
    }

    /// Load a PDF document from a memory slice with the given options.
    pub fn load_mem_with_options(buffer: &[u8], options: LoadOptions) -> Result<Document> {
        // Same input-size check as the sync path; see the note there.
        if let Some(limit) = options.max_file_bytes
            && buffer.len() > limit
        {
            return Err(Error::DocumentTooLarge {
                size: buffer.len(),
                limit,
            });
        }
        let filter = options.filter;
        Reader {
            buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: options.password.clone(),
            options,
        }
        .read(filter)
    }

    /// Load a PDF document from a memory slice with a password for encrypted PDFs.
    ///
    /// This is a synchronous helper available in both sync and async builds so
    /// that callers that already have the PDF in memory do not need to branch on
    /// the `async` feature flag.
    pub fn load_mem_with_password(buffer: &[u8], password: &str) -> Result<Document> {
        Reader {
            buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: Some(password.to_string()),
            options: LoadOptions::default(),
        }
        .read(None)
    }

    /// Load PDF metadata (title and page count) without loading the entire document.
    /// This is much faster for large PDFs when you only need basic information.
    #[inline]
    pub async fn load_metadata<P: AsRef<Path>>(path: P) -> Result<PdfMetadata> {
        let file = File::open(path).await?;
        let metadata = file.metadata().await?;
        let capacity = Some(metadata.len() as usize);
        Self::load_metadata_internal(file, capacity, None).await
    }

    /// Load PDF metadata from a file path with a password for encrypted PDFs.
    #[inline]
    pub async fn load_metadata_with_password<P: AsRef<Path>>(path: P, password: &str) -> Result<PdfMetadata> {
        let file = File::open(path).await?;
        let metadata = file.metadata().await?;
        let capacity = Some(metadata.len() as usize);
        Self::load_metadata_internal(file, capacity, Some(password.to_string())).await
    }

    /// Load PDF metadata from an arbitrary source without loading the entire document.
    #[inline]
    pub async fn load_metadata_from<R: AsyncRead>(source: R) -> Result<PdfMetadata> {
        Self::load_metadata_internal(source, None, None).await
    }

    /// Load PDF metadata from an arbitrary source with a password for encrypted PDFs.
    #[inline]
    pub async fn load_metadata_from_with_password<R: AsyncRead>(source: R, password: &str) -> Result<PdfMetadata> {
        Self::load_metadata_internal(source, None, Some(password.to_string())).await
    }

    /// Load PDF metadata from a memory slice without loading the entire document.
    #[inline]
    pub fn load_metadata_mem(buffer: &[u8]) -> Result<PdfMetadata> {
        Reader {
            buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: None,
            options: LoadOptions::default(),
        }
        .read_metadata()
    }

    /// Load PDF metadata from a memory slice with a password for encrypted PDFs.
    #[inline]
    pub fn load_metadata_mem_with_password(buffer: &[u8], password: &str) -> Result<PdfMetadata> {
        Reader {
            buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: Some(password.to_string()),
            options: LoadOptions::default(),
        }
        .read_metadata()
    }

    async fn load_metadata_internal<R: AsyncRead>(
        source: R, capacity: Option<usize>, password: Option<String>,
    ) -> Result<PdfMetadata> {
        pin!(source);

        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer).await?;

        Reader {
            buffer: &buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password,
            options: LoadOptions::default(),
        }
        .read_metadata()
    }
}

impl TryInto<Document> for &[u8] {
    type Error = Error;

    fn try_into(self) -> Result<Document> {
        Reader {
            buffer: self,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: None,
            options: LoadOptions::default(),
        }
        .read(None)
    }
}

#[cfg(not(feature = "async"))]
impl IncrementalDocument {
    /// Load a PDF document from a specified file path.
    #[inline]
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let capacity = Some(file.metadata()?.len() as usize);
        Self::load_internal(file, capacity)
    }

    /// Load a PDF document from an arbitrary source.
    #[inline]
    pub fn load_from<R: Read>(source: R) -> Result<Self> {
        Self::load_internal(source, None)
    }

    fn load_internal<R: Read>(mut source: R, capacity: Option<usize>) -> Result<Self> {
        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer)?;

        let document = Reader {
            buffer: &buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: None,
            options: LoadOptions::default(),
        }
        .read(None)?;

        Ok(IncrementalDocument::create_from(buffer, document))
    }

    /// Load a PDF document from a memory slice.
    pub fn load_mem(buffer: &[u8]) -> Result<Document> {
        buffer.try_into()
    }
}

#[cfg(feature = "async")]
impl IncrementalDocument {
    /// Load a PDF document from a specified file path.
    #[inline]
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path).await?;
        let metadata = file.metadata().await?;
        let capacity = Some(metadata.len() as usize);
        Self::load_internal(file, capacity).await
    }

    /// Load a PDF document from an arbitrary source.
    #[inline]
    pub async fn load_from<R: AsyncRead>(source: R) -> Result<Self> {
        Self::load_internal(source, None).await
    }

    async fn load_internal<R: AsyncRead>(source: R, capacity: Option<usize>) -> Result<Self> {
        pin!(source);

        let mut buffer = capacity.map(Vec::with_capacity).unwrap_or_default();
        source.read_to_end(&mut buffer).await?;

        let document = Reader {
            buffer: &buffer,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: None,
            options: LoadOptions::default(),
        }
        .read(None)?;

        Ok(IncrementalDocument::create_from(buffer, document))
    }

    /// Load a PDF document from a memory slice.
    pub fn load_mem(buffer: &[u8]) -> Result<Document> {
        buffer.try_into()
    }
}

impl TryInto<IncrementalDocument> for &[u8] {
    type Error = Error;

    fn try_into(self) -> Result<IncrementalDocument> {
        let document = Reader {
            buffer: self,
            document: Document::new(),
            encryption_state: None,
            raw_objects: BTreeMap::new(),
            password: None,
            options: LoadOptions::default(),
        }
        .read(None)?;

        Ok(IncrementalDocument::create_from(self.to_vec(), document))
    }
}

pub struct Reader<'a> {
    pub buffer: &'a [u8],
    pub document: Document,
    pub encryption_state: Option<EncryptionState>,
    pub raw_objects: BTreeMap<ObjectId, Vec<u8>>, // Store raw bytes for encrypted objects
    pub password: Option<String>,                 // Password for encrypted PDFs
    /// Everything that controls this load. Upstream keeps `strict` and
    /// `max_decompressed_size` as bare fields here; both live in `LoadOptions`
    /// in this fork, alongside `max_file_bytes` and `lazy_objstm`, so there is
    /// one place a caller has to look.
    pub options: LoadOptions,
}

/// Maximum allowed embedding of literal strings.
pub const MAX_BRACKET: usize = 100;

/// How deep an array or dictionary may nest before the parser refuses to
/// descend further.
///
/// Upstream lopdf sets this to 100 (c755394). We set it to 32, and the reason is
/// measurable rather than aesthetic: `Reader::read` fans object parsing out over
/// rayon workers, so the stack this recursion runs on is a spawned thread's
/// default 2 MiB, not the main thread's 8 MiB. The dictionary parser costs
/// roughly 20 KiB of stack per nesting level in an unoptimised build, and a
/// stack overflow on a rayon worker aborts the whole process -- no `Result` to
/// return, nothing for a caller to catch.
///
/// Measured on this crate at debug profile, feeding a 50 000-deep dictionary
/// through `Document::load_mem` on default thread stacks:
///
/// | limit | outcome           |
/// |------:|-------------------|
/// |   100 | stack overflow    |
/// |    80 | survives          |
/// |    64 | survives          |
/// |    32 | survives          |
///
/// 32 leaves a 2.5x margin over the measured edge, which is what covers a change
/// of compiler version, profile, or rayon's default stack. Raising it back to
/// upstream's 100 needs either a bigger worker stack or a parser that does not
/// recurse -- not a decision that a re-merge should make by accident.
///
/// Real documents do not come near 32: PDF 32000-1 defines no nesting this deep,
/// and nothing in the test corpus exceeds single digits.
pub const MAX_NESTING_DEPTH: usize = 32;

/// PDF metadata extracted without loading the entire document.
/// This is useful for quickly getting basic information about large PDFs.
#[derive(Debug, Clone)]
pub struct PdfMetadata {
    /// Document title from Info dictionary
    pub title: Option<String>,
    /// Document author from Info dictionary
    pub author: Option<String>,
    /// Document subject from Info dictionary
    pub subject: Option<String>,
    /// Document keywords from Info dictionary
    pub keywords: Option<String>,
    /// Application that created the document
    pub creator: Option<String>,
    /// Application that produced the document
    pub producer: Option<String>,
    /// Document creation date (PDF date format: D:YYYYMMDDHHmmSSOHH'mm')
    pub creation_date: Option<String>,
    /// Document modification date (PDF date format: D:YYYYMMDDHHmmSSOHH'mm')
    pub modification_date: Option<String>,
    /// Custom Info dictionary entries that are not part of the standard set above.
    ///
    /// Producers commonly use the Info dictionary to attach application-specific
    /// metadata (for example, Microsoft Information Protection labels stored as
    /// `MSIP_Label_{GUID}_{Property}` keys). Such entries used to be discarded
    /// by the metadata-only loader; they are now preserved here as raw `Object`
    /// values so callers can inspect or decode them as needed.
    pub custom: HashMap<Vec<u8>, Object>,
    /// Number of pages in the document
    pub page_count: u32,
    /// PDF version
    pub version: String,
    /// Whether the document declares encryption (an `/Encrypt` entry in the
    /// trailer). When `true`, string and stream contents are encrypted; the
    /// standard Info fields above are populated only if the document could be
    /// decrypted — e.g. an empty user password, or a password supplied via a
    /// `*_with_password` loader.
    pub encrypted: bool,
}

struct InfoMetadata {
    title: Option<String>,
    author: Option<String>,
    subject: Option<String>,
    keywords: Option<String>,
    creator: Option<String>,
    producer: Option<String>,
    creation_date: Option<String>,
    modification_date: Option<String>,
    custom: HashMap<Vec<u8>, Object>,
}

impl InfoMetadata {
    fn empty() -> Self {
        Self {
            title: None,
            author: None,
            subject: None,
            keywords: None,
            creator: None,
            producer: None,
            creation_date: None,
            modification_date: None,
            custom: HashMap::new(),
        }
    }
}

/// Standard Info dictionary keys defined in PDF 1.7 §14.3.3 ("Document
/// Information Dictionary"). Any other key is preserved on
/// `PdfMetadata::custom` rather than dropped.
const STANDARD_INFO_KEYS: &[&[u8]] = &[
    b"Title",
    b"Author",
    b"Subject",
    b"Keywords",
    b"Creator",
    b"Producer",
    b"CreationDate",
    b"ModDate",
    b"Trapped",
];

impl Reader<'_> {
    /// Read metadata (title and page count) without loading the entire document.
    /// This is much faster for large PDFs when you only need basic information.
    ///
    /// For encrypted PDFs, use `Document::load_metadata_with_password()` instead.
    pub fn read_metadata(mut self) -> Result<PdfMetadata> {
        let offset = self.buffer.windows(5).position(|w| w == b"%PDF-").unwrap_or(0);
        self.buffer = &self.buffer[offset..];

        let version = parser::header(self.buffer, self.options.strict).ok_or(ParseError::InvalidFileHeader)?;

        let xref_start = Self::get_xref_start(self.buffer)?;
        if xref_start > self.buffer.len() {
            return Err(Error::Xref(XrefError::Start));
        }

        let (mut xref, mut trailer) = parser::xref_and_trailer(&self.buffer[xref_start..], &self)?;

        let mut already_seen = HashSet::new();
        let mut prev_xref_start = trailer.remove(b"Prev");
        while let Some(prev) = prev_xref_start.and_then(|offset| offset.as_i64().ok()) {
            if already_seen.contains(&prev) {
                break;
            }
            already_seen.insert(prev);
            if prev < 0 || prev as usize > self.buffer.len() {
                return Err(Error::Xref(XrefError::PrevStart));
            }

            let (prev_xref, prev_trailer) = parser::xref_and_trailer(&self.buffer[prev as usize..], &self)?;
            xref.merge(prev_xref);

            let prev_xref_stream_start = trailer.remove(b"XRefStm");
            if let Some(prev) = prev_xref_stream_start.and_then(|offset| offset.as_i64().ok()) {
                if prev < 0 || prev as usize > self.buffer.len() {
                    return Err(Error::Xref(XrefError::StreamStart));
                }

                let (prev_xref, _) = parser::xref_and_trailer(&self.buffer[prev as usize..], &self)?;
                xref.merge(prev_xref);
            }

            prev_xref_start = prev_trailer.get(b"Prev").cloned().ok();
        }
        let xref_entry_count = xref.max_id().checked_add(1).ok_or(ParseError::InvalidXref)?;
        if xref.size != xref_entry_count {
            warn!(
                "Size entry of trailer dictionary is {}, correct value is {}.",
                xref.size, xref_entry_count
            );
            xref.size = xref_entry_count;
        }

        self.document.reference_table = xref;
        self.document.trailer = trailer.clone();

        let encrypted = self.document.trailer.get(b"Encrypt").is_ok();
        // For encrypted PDFs, set up decryption so the Info dictionary can be
        // read. If the document is protected by a password we cannot supply,
        // still report what we have (notably `encrypted`) rather than failing
        // the whole call; callers needing the Info fields should use a
        // `*_with_password` loader.
        let needs_password = if encrypted {
            match self.setup_encryption_for_metadata() {
                Ok(()) => false,
                // No usable password was available (empty password failed and
                // none was supplied): report `encrypted` without the Info
                // fields. A wrong supplied password still surfaces as an error.
                Err(Error::Unimplemented(_)) => true,
                Err(e) => return Err(e),
            }
        } else {
            false
        };

        let info_metadata = if needs_password {
            InfoMetadata::empty()
        } else {
            self.extract_info_metadata()?
        };
        let page_count = if needs_password { 0 } else { self.extract_page_count()? };

        Ok(PdfMetadata {
            title: info_metadata.title,
            author: info_metadata.author,
            subject: info_metadata.subject,
            keywords: info_metadata.keywords,
            creator: info_metadata.creator,
            producer: info_metadata.producer,
            creation_date: info_metadata.creation_date,
            modification_date: info_metadata.modification_date,
            custom: info_metadata.custom,
            page_count,
            version,
            encrypted,
        })
    }

    fn extract_info_metadata(&self) -> Result<InfoMetadata> {
        let info_ref = match self.document.trailer.get(b"Info") {
            Ok(obj) => obj.as_reference().ok(),
            Err(_) => return Ok(InfoMetadata::empty()),
        };

        let info_id = match info_ref {
            Some(id) => id,
            None => return Ok(InfoMetadata::empty()),
        };

        let mut already_seen = HashSet::new();
        let info_obj = match self.get_object(info_id, &mut already_seen) {
            Ok(obj) => obj,
            Err(_) => return Ok(InfoMetadata::empty()),
        };

        let info_dict = match info_obj.as_dict() {
            Ok(dict) => dict,
            Err(_) => return Ok(InfoMetadata::empty()),
        };

        let mut custom = HashMap::new();
        for (key, value) in info_dict.iter() {
            if STANDARD_INFO_KEYS.contains(&key.as_slice()) {
                continue;
            }
            custom.insert(key.clone(), value.clone());
        }

        Ok(InfoMetadata {
            title: Self::extract_string_field(info_dict, b"Title"),
            author: Self::extract_string_field(info_dict, b"Author"),
            subject: Self::extract_string_field(info_dict, b"Subject"),
            keywords: Self::extract_string_field(info_dict, b"Keywords"),
            creator: Self::extract_string_field(info_dict, b"Creator"),
            producer: Self::extract_string_field(info_dict, b"Producer"),
            creation_date: Self::extract_string_field(info_dict, b"CreationDate"),
            modification_date: Self::extract_string_field(info_dict, b"ModDate"),
            custom,
        })
    }

    fn extract_string_field(dict: &Dictionary, key: &[u8]) -> Option<String> {
        match dict.get(key) {
            Ok(obj) => match obj {
                Object::String(_bytes, _) => common_data_structures::decode_text_string(obj).ok(),
                _ => None,
            },
            Err(_) => None,
        }
    }

    fn extract_page_count(&self) -> Result<u32> {
        let root_ref = match self.document.trailer.get(b"Root").and_then(Object::as_reference) {
            Ok(id) => id,
            Err(_) => return Ok(0),
        };

        let mut already_seen = HashSet::new();
        let catalog_obj = match self.get_object(root_ref, &mut already_seen) {
            Ok(obj) => obj,
            Err(_) => return Ok(0),
        };

        let catalog_dict = match catalog_obj.as_dict() {
            Ok(dict) => dict,
            Err(_) => return Ok(0),
        };

        let pages_ref = match catalog_dict.get(b"Pages").and_then(Object::as_reference) {
            Ok(id) => id,
            Err(_) => return Ok(0),
        };

        self.get_pages_tree_count(pages_ref, &mut HashSet::new()).or(Ok(0))
    }

    fn get_pages_tree_count(&self, pages_id: ObjectId, seen: &mut HashSet<ObjectId>) -> Result<u32> {
        if seen.contains(&pages_id) {
            return Err(Error::ReferenceCycle(pages_id));
        }
        seen.insert(pages_id);

        let mut already_seen = HashSet::new();
        let pages_obj = match self.get_object(pages_id, &mut already_seen) {
            Ok(obj) => obj,
            Err(_) => return Ok(0),
        };

        let pages_dict = match pages_obj.as_dict() {
            Ok(dict) => dict,
            Err(_) => return Ok(0),
        };

        match pages_dict.get_type() {
            Ok(type_name) if type_name == b"Page" => Ok(1),
            Ok(type_name) if type_name == b"Pages" => {
                if let Ok(count_obj) = pages_dict.get(b"Count")
                    && let Ok(count) = count_obj.as_i64()
                    && count >= 0
                {
                    return Ok(count as u32);
                }

                let kids = match pages_dict.get(b"Kids").and_then(Object::as_array) {
                    Ok(arr) => arr,
                    Err(_) => return Ok(0),
                };

                let mut total = 0u32;
                for kid in kids.iter() {
                    if let Ok(kid_ref) = kid.as_reference()
                        && let Ok(count) = self.get_pages_tree_count(kid_ref, seen)
                    {
                        total += count;
                    }
                }
                Ok(total)
            }
            _ => Ok(1),
        }
    }

    /// Read whole document.
    pub fn read(mut self, filter_func: Option<FilterFunc>) -> Result<Document> {
        let offset = self.buffer.windows(5).position(|w| w == b"%PDF-").unwrap_or(0);
        self.buffer = &self.buffer[offset..];

        // The document structure can be expressed in PEG as:
        //   document <- header indirect_object* xref trailer xref_start
        let version = parser::header(self.buffer, self.options.strict).ok_or(ParseError::InvalidFileHeader)?;

        //The binary_mark is in line 2 after the pdf version. If at other line number, then will be declared as invalid
        // pdf.
        if let Some(pos) = self.buffer.iter().position(|&byte| byte == b'\n')
            && let Some(binary_mark) = parser::binary_mark(&self.buffer[pos + 1..])
            && binary_mark.iter().all(|&byte| byte >= 128)
        {
            self.document.binary_mark = binary_mark;
        }

        let xref_start = Self::get_xref_start(self.buffer)?;
        if xref_start > self.buffer.len() {
            return Err(Error::Xref(XrefError::Start));
        }
        self.document.xref_start = xref_start;

        let (mut xref, mut trailer) = parser::xref_and_trailer(&self.buffer[xref_start..], &self)?;

        // Read previous Xrefs of linearized or incremental updated document.
        let mut already_seen = HashSet::new();
        let mut prev_xref_start = trailer.remove(b"Prev");
        while let Some(prev) = prev_xref_start.and_then(|offset| offset.as_i64().ok()) {
            if already_seen.contains(&prev) {
                break;
            }
            already_seen.insert(prev);
            if prev < 0 || prev as usize > self.buffer.len() {
                return Err(Error::Xref(XrefError::PrevStart));
            }

            let (prev_xref, prev_trailer) = parser::xref_and_trailer(&self.buffer[prev as usize..], &self)?;
            xref.merge(prev_xref);

            // Read xref stream in hybrid-reference file
            let prev_xref_stream_start = trailer.remove(b"XRefStm");
            if let Some(prev) = prev_xref_stream_start.and_then(|offset| offset.as_i64().ok()) {
                if prev < 0 || prev as usize > self.buffer.len() {
                    return Err(Error::Xref(XrefError::StreamStart));
                }

                let (prev_xref, _) = parser::xref_and_trailer(&self.buffer[prev as usize..], &self)?;
                xref.merge(prev_xref);
            }

            prev_xref_start = prev_trailer.get(b"Prev").cloned().ok();
        }
        let xref_entry_count = xref.max_id().checked_add(1).ok_or(ParseError::InvalidXref)?;
        if xref.size != xref_entry_count {
            warn!(
                "Size entry of trailer dictionary is {}, correct value is {}.",
                xref.size, xref_entry_count
            );
            xref.size = xref_entry_count;
        }

        self.document.version = version;
        self.document.max_id = xref.size - 1;
        self.document.trailer = trailer;
        self.document.reference_table = xref;

        // Check if encrypted
        let is_encrypted = self.document.trailer.get(b"Encrypt").is_ok();

        if is_encrypted {
            // For encrypted PDFs, use a special loading strategy
            self.load_encrypted_document(filter_func)?;
        } else {
            // For non-encrypted PDFs, use the normal loading
            self.load_objects_raw(filter_func)?;
        }

        Ok(self.document)
    }

    fn load_encrypted_document(&mut self, _filter_func: Option<FilterFunc>) -> Result<()> {
        // First, extract all raw object bytes without parsing
        let entries: Vec<_> = self
            .document
            .reference_table
            .entries
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();

        let mut object_streams = Vec::new();

        for (obj_num, entry) in entries {
            match entry {
                XrefEntry::Normal { offset, .. } => {
                    if let Ok((obj_id, raw_bytes)) = self.extract_raw_object(offset as usize) {
                        self.raw_objects.insert(obj_id, raw_bytes);
                    }
                }
                XrefEntry::Compressed { container, index } => {
                    // Store compressed object info for later processing
                    object_streams.push((obj_num, container, index));
                }
                XrefEntry::Free | XrefEntry::UnusableFree => {
                    // Skip free entries
                }
            }
        }

        self.parse_encryption_dictionary()?;

        if self.authenticate_and_setup_encryption(false)?.is_none() {
            return Ok(());
        }

        if let Some(ref state) = self.encryption_state {
            let encrypt_ref = self
                .document
                .trailer
                .get(b"Encrypt")
                .ok()
                .and_then(|o| o.as_reference().ok());

            for (obj_id, raw_bytes) in &self.raw_objects {
                if let Some(enc_ref) = encrypt_ref
                    && *obj_id == enc_ref
                {
                    continue;
                }

                if let Ok((id, mut obj)) = self.parse_raw_object(raw_bytes) {
                    let _ = encryption::decrypt_object(state, *obj_id, &mut obj);
                    self.document.objects.insert(id, obj);
                }
            }

            let mut streams_to_process: std::collections::HashMap<u32, Vec<(u32, u16)>> =
                std::collections::HashMap::new();
            for (obj_num, container_id, index) in object_streams {
                streams_to_process
                    .entry(container_id)
                    .or_default()
                    .push((obj_num, index));
            }

            for (container_id, objects_in_stream) in streams_to_process {
                if let Some(container_obj) = self.document.objects.get_mut(&(container_id, 0))
                    && let Ok(stream) = container_obj.as_stream_mut()
                {
                    match ObjectStream::new_with_limit(stream, self.options.max_decompressed_size) {
                        Ok(object_stream) => {
                            for (obj_num, _index) in objects_in_stream {
                                let obj_id = (obj_num, 0);
                                if let Some(obj) = object_stream.objects.get(&obj_id) {
                                    self.document.objects.insert(obj_id, obj.clone());
                                }
                            }
                        }
                        Err(_e) => {}
                    }
                }
            }

            let mut stored_state = state.clone();
            // Record the /Encrypt dictionary's object id so that a later
            // incremental save can restore `/Encrypt N G R` in the appended
            // trailer. See `IncrementalDocument::save_internal`.
            stored_state.encrypt_object_id = encrypt_ref;
            self.document.encryption_state = Some(stored_state);

            if let Some(enc_ref) = encrypt_ref {
                self.document.objects.remove(&enc_ref);
            }
            self.document.trailer.remove(b"Encrypt");
        }

        Ok(())
    }

    fn parse_raw_object(&self, raw_bytes: &[u8]) -> Result<(ObjectId, Object)> {
        // Parse the raw bytes as an indirect object
        parser::indirect_object(raw_bytes, 0, None, self, &mut HashSet::new())
    }

    fn load_objects_raw(&mut self, filter_func: Option<FilterFunc>) -> Result<()> {
        let is_encrypted = self.document.trailer.get(b"Encrypt").is_ok();
        let zero_length_streams = Mutex::new(vec![]);
        let object_streams = Mutex::new(vec![]);
        // Phase 2 (Issue #468): track ObjStm containers kept for lazy resolution.
        let pending_obj_stream_ids: Mutex<Vec<ObjectId>> = Mutex::new(vec![]);
        // Copy bool so the closure captures it by value (no borrow of self.options).
        let lazy_objstm = self.options.lazy_objstm;

        let entries_filter_map = |(_, entry): (&_, &_)| {
            if let XrefEntry::Normal { offset, .. } = *entry {
                // read_object now handles decryption internally
                let result = self.read_object(offset as usize, None, &mut HashSet::new());
                let (object_id, mut object) = match result {
                    Ok(obj) => obj,
                    Err(e) => {
                        // Log error but continue
                        if is_encrypted {
                            // Expected for some encrypted objects - but log which ones
                            warn!("Skipping encrypted object at offset {}: {:?}", offset, e);
                        } else {
                            error!("Object load error at offset {}: {e:?}", offset);
                        }
                        return None;
                    }
                };
                if let Some(filter_func) = filter_func {
                    filter_func(object_id, &mut object)?;
                }

                if let Ok(ref mut stream) = object.as_stream_mut() {
                    if stream.dict.has_type(b"ObjStm") && !is_encrypted {
                        if lazy_objstm {
                            // Phase 2b (Issue #468): defer decompression.
                            // Keep the container in document.objects and record its
                            // ID so the caller can call resolve_pending_object_streams.
                            pending_obj_stream_ids.lock().unwrap().push(object_id);
                            // Fall through to Some((object_id, object)) below.
                        } else {
                            // Phase 2a (Issue #468): eager extraction, drop container.
                            // Extract contained objects now, then return None so the
                            // ObjStm container itself is NOT added to document.objects.
                            // This eliminates the decompressed-container double-memory
                            // problem: the decompressed bytes (stream.content) are freed
                            // when `object` is dropped at the end of this arm.
                            //
                            // Divergence from upstream, kept deliberately: upstream
                            // returns Some(container) here, so a document holds both
                            // the compressed container and its extracted objects.
                            //
                            // Taken from upstream: new_with_limit instead of new, so a
                            // crafted ObjStm cannot inflate past the configured bound
                            // while the document is still loading.
                            if let Ok(obj_stream) =
                                ObjectStream::new_with_limit(stream, self.options.max_decompressed_size)
                            {
                                let container_id = object_id;
                                let owned_objects = obj_stream.objects.into_iter().filter(|(nested_object_id, _)| {
                                    // Same ownership rule upstream expresses with its
                                    // compressed_obj_containers map: an object belongs
                                    // to the container the xref says it does, so a
                                    // stale ObjStm in a linearized file cannot win.
                                    self.document
                                        .reference_table
                                        .compressed_object_belongs_to(*nested_object_id, container_id)
                                });
                                let mut object_streams = object_streams.lock().unwrap();
                                if let Some(filter_func) = filter_func {
                                    let objects: BTreeMap<(u32, u16), Object> = owned_objects
                                        .filter_map(|(object_id, mut object)| filter_func(object_id, &mut object))
                                        .collect();
                                    object_streams.extend(objects);
                                } else {
                                    object_streams.extend(owned_objects);
                                }
                            }
                            // Return None: container is dropped here, freeing its bytes.
                            return None;
                        }
                    } else if stream.content.is_empty() {
                        let mut zero_length_streams = zero_length_streams.lock().unwrap();
                        zero_length_streams.push(object_id);
                    }
                }

                Some((object_id, object))
            } else {
                None
            }
        };

        #[cfg(feature = "rayon")]
        {
            self.document.objects = self
                .document
                .reference_table
                .entries
                .par_iter()
                .filter_map(entries_filter_map)
                .collect();
        }
        #[cfg(not(feature = "rayon"))]
        {
            self.document.objects = self
                .document
                .reference_table
                .entries
                .iter()
                .filter_map(entries_filter_map)
                .collect();
        }

        // Only add entries, but never replace entries
        for (id, entry) in object_streams.into_inner().unwrap() {
            self.document.objects.entry(id).or_insert(entry);
        }

        for object_id in zero_length_streams.into_inner().unwrap() {
            let _ = self.read_stream_content(object_id);
        }

        // Phase 2b (Issue #468): store pending ObjStm container IDs in the document
        // so the caller can resolve them later via resolve_pending_object_streams.
        self.document.pending_obj_streams = pending_obj_stream_ids.into_inner().unwrap();

        Ok(())
    }

    fn read_stream_content(&mut self, object_id: ObjectId) -> Result<()> {
        let length = self.get_stream_length(object_id)?;
        let stream = self
            .document
            .get_object_mut(object_id)
            .and_then(Object::as_stream_mut)?;
        let start = stream
            .start_position
            .ok_or(Error::InvalidStream("missing start position".to_string()))?;

        if length < 0 {
            return Err(Error::InvalidStream("negative stream length.".to_string()));
        }

        let length = usize::try_from(length).map_err(|e| Error::NumericCast(e.to_string()))?;
        let end = start + length;

        if end > self.buffer.len() {
            return Err(Error::InvalidStream("stream extends after document end.".to_string()));
        }

        stream.set_content(self.buffer[start..end].to_vec());
        Ok(())
    }

    fn get_stream_length(&self, object_id: ObjectId) -> Result<i64> {
        let object = self.document.get_object(object_id)?;
        let stream = object.as_stream()?;
        stream
            .dict
            .get(b"Length")
            .and_then(|value| self.document.dereference(value))
            .and_then(|(_id, obj)| obj.as_i64())
            .inspect_err(|_err| {
                error!(
                    "stream dictionary of '{} {} R' is missing the Length entry",
                    object_id.0, object_id.1
                );
            })
    }

    /// Get object offset by object ID.
    fn get_offset(&self, id: ObjectId) -> Result<u32> {
        let entry = self.document.reference_table.get(id.0).ok_or(Error::MissingXrefEntry)?;
        match *entry {
            XrefEntry::Normal { offset, generation } if generation == id.1 => Ok(offset),
            _ => Err(Error::MissingXrefEntry),
        }
    }

    /// Load a compressed object from an object stream (for lightweight metadata extraction)
    fn get_compressed_object(&self, id: ObjectId) -> Result<Object> {
        let entry = self.document.reference_table.get(id.0).ok_or(Error::MissingXrefEntry)?;

        let container_id = match entry {
            XrefEntry::Compressed { container, .. } => *container,
            _ => return Err(Error::MissingXrefEntry),
        };

        let container_id = (container_id, 0);
        let mut already_seen = HashSet::new();
        let container_obj = self.get_object(container_id, &mut already_seen)?;
        let mut container_stream = container_obj.as_stream()?.clone();
        let object_stream = ObjectStream::new_with_limit(&mut container_stream, self.options.max_decompressed_size)?;
        object_stream.objects.get(&id).cloned().ok_or(Error::MissingXrefEntry)
    }

    pub fn get_object(&self, id: ObjectId, already_seen: &mut HashSet<ObjectId>) -> Result<Object> {
        if already_seen.contains(&id) {
            warn!("reference cycle detected resolving object {} {}", id.0, id.1);
            return Err(Error::ReferenceCycle(id));
        }
        already_seen.insert(id);

        if let Some(entry) = self.document.reference_table.get(id.0)
            && matches!(entry, XrefEntry::Compressed { .. })
        {
            return self.get_compressed_object(id);
        }

        let offset = self.get_offset(id)?;
        let (_, mut obj) = self.read_object(offset as usize, Some(id), already_seen)?;

        if let Some(ref state) = self.encryption_state {
            let encrypt_ref = self
                .document
                .trailer
                .get(b"Encrypt")
                .ok()
                .and_then(|o| o.as_reference().ok());
            if let Some(enc_ref) = encrypt_ref
                && id != enc_ref
            {
                encryption::decrypt_object(state, id, &mut obj).map_err(Error::Decryption)?;
            }
        }

        Ok(obj)
    }

    fn parse_encryption_dictionary(&mut self) -> Result<()> {
        if let Ok(encrypt_ref) = self.document.trailer.get(b"Encrypt").and_then(|o| o.as_reference()) {
            if self.raw_objects.is_empty() {
                let offset = self.get_offset(encrypt_ref)?;
                let (_, encrypt_obj) = self.read_object(offset as usize, Some(encrypt_ref), &mut HashSet::new())?;
                self.document.objects.insert(encrypt_ref, encrypt_obj);
            } else if let Some(raw_bytes) = self.raw_objects.get(&encrypt_ref)
                && let Ok((_, obj)) = self.parse_raw_object(raw_bytes)
            {
                self.document.objects.insert(encrypt_ref, obj);
            }
        }
        Ok(())
    }

    fn authenticate_and_setup_encryption(&mut self, require_password: bool) -> Result<Option<String>> {
        let password_to_use: Option<String> = if self.document.authenticate_password("").is_ok() {
            Some(String::new())
        } else if let Some(ref pwd) = self.password {
            if self.document.authenticate_password(pwd).is_ok() {
                Some(pwd.clone())
            } else if require_password {
                return Err(Error::InvalidPassword);
            } else {
                warn!("Invalid password provided for encrypted PDF");
                return Err(Error::InvalidPassword);
            }
        } else if require_password {
            return Err(Error::Unimplemented(
                "PDF is encrypted and requires a password. Use Document::load_metadata_with_password() instead.",
            ));
        } else {
            warn!("PDF is encrypted and requires a password");
            return Ok(None);
        };

        if let Some(ref password) = password_to_use {
            let state = EncryptionState::decode(&self.document, password)?;
            self.encryption_state = Some(state);
        }

        Ok(password_to_use)
    }

    fn setup_encryption_for_metadata(&mut self) -> Result<()> {
        self.parse_encryption_dictionary()?;
        self.authenticate_and_setup_encryption(true)?;
        Ok(())
    }

    fn extract_raw_object(&mut self, offset: usize) -> Result<(ObjectId, Vec<u8>)> {
        if offset > self.buffer.len() {
            return Err(Error::InvalidOffset(offset));
        }

        // Find object header (e.g., "19 0 obj")
        let slice = &self.buffer[offset..];

        // Parse object ID
        let mut pos = 0;
        while pos < slice.len() && slice[pos].is_ascii_whitespace() {
            pos += 1;
        }

        // Get object number
        let num_start = pos;
        while pos < slice.len() && slice[pos].is_ascii_digit() {
            pos += 1;
        }
        let obj_num: u32 = std::str::from_utf8(&slice[num_start..pos])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or(Error::Parse(ParseError::InvalidXref))?;

        // Skip whitespace
        while pos < slice.len() && slice[pos].is_ascii_whitespace() {
            pos += 1;
        }

        // Get generation number
        let gen_start = pos;
        while pos < slice.len() && slice[pos].is_ascii_digit() {
            pos += 1;
        }
        let obj_gen: u16 = std::str::from_utf8(&slice[gen_start..pos])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or(Error::Parse(ParseError::InvalidXref))?;

        // Skip to "obj"
        while pos < slice.len() && slice[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos + 3 > slice.len() || &slice[pos..pos + 3] != b"obj" {
            return Err(Error::Parse(ParseError::InvalidXref));
        }
        pos += 3;

        // Find "endobj"
        let endobj_pattern = b"endobj";
        let mut end_pos = pos;
        while end_pos + endobj_pattern.len() <= slice.len() {
            if &slice[end_pos..end_pos + endobj_pattern.len()] == endobj_pattern {
                end_pos += endobj_pattern.len();
                break;
            }
            end_pos += 1;
        }

        if end_pos > slice.len() {
            return Err(Error::Parse(ParseError::InvalidXref));
        }

        // Extract raw object bytes (including header and trailer)
        let raw_bytes = slice[0..end_pos].to_vec();

        Ok(((obj_num, obj_gen), raw_bytes))
    }

    fn read_object(
        &self, offset: usize, expected_id: Option<ObjectId>, already_seen: &mut HashSet<ObjectId>,
    ) -> Result<(ObjectId, Object)> {
        if offset > self.buffer.len() {
            return Err(Error::InvalidOffset(offset));
        }

        // Just parse without decryption - we'll decrypt later
        parser::indirect_object(self.buffer, offset, expected_id, self, already_seen)
    }

    fn get_xref_start(buffer: &[u8]) -> Result<usize> {
        let seek_pos = buffer.len() - cmp::min(buffer.len(), 512);
        Self::search_substring(buffer, b"%%EOF", seek_pos)
            .filter(|&eof_pos| eof_pos > 25)
            .and_then(|eof_pos| Self::search_substring(&buffer[..eof_pos], b"startxref", eof_pos - 25))
            .ok_or(Error::Xref(XrefError::Start))
            .and_then(|xref_pos| {
                if xref_pos <= buffer.len() {
                    match parser::xref_start(&buffer[xref_pos..]) {
                        Some(startxref) => Ok(startxref as usize),
                        None => Err(Error::Xref(XrefError::Start)),
                    }
                } else {
                    Err(Error::Xref(XrefError::Start))
                }
            })
    }

    fn search_substring(buffer: &[u8], pattern: &[u8], start_pos: usize) -> Option<usize> {
        buffer
            .get(start_pos..)?
            .windows(pattern.len())
            .rposition(|window| window == pattern)
            .map(|pos| start_pos + pos)
    }
}

#[cfg(all(test, not(feature = "async")))]
#[test]
fn load_document() {
    let mut doc = Document::load("assets/example.pdf").unwrap();
    assert_eq!(doc.version, "1.5");

    // Create temporary folder to store file.
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test_2_load.pdf");
    doc.save(file_path).unwrap();
}

#[cfg(all(test, feature = "async"))]
#[tokio::test]
async fn load_document() {
    let mut doc = Document::load("assets/example.pdf").await.unwrap();
    assert_eq!(doc.version, "1.5");

    // Create temporary folder to store file.
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test_2_load.pdf");
    doc.save(file_path).unwrap();
}

#[test]
#[should_panic(expected = "Xref(Start)")]
fn load_short_document() {
    let _doc = Document::load_mem(b"%PDF-1.5\n%%EOF\n").unwrap();
}

#[test]
fn load_document_with_preceding_bytes() {
    let mut content = Vec::new();
    content.extend(b"garbage");
    content.extend(include_bytes!("../assets/example.pdf"));
    let doc = Document::load_mem(&content).unwrap();
    assert_eq!(doc.version, "1.5");
}

#[test]
fn load_many_shallow_brackets() {
    let content: String = std::iter::repeat_n("()", MAX_BRACKET * 10)
        .flat_map(|x| x.chars())
        .collect();
    const STREAM_CRUFT: usize = 33;
    let doc = format!(
        "%PDF-1.5
1 0 obj<</Type/Pages/Kids[5 0 R]/Count 1/Resources 3 0 R/MediaBox[0 0 595 842]>>endobj
2 0 obj<</Type/Font/Subtype/Type1/BaseFont/Courier>>endobj
3 0 obj<</Font<</F1 2 0 R>>>>endobj
5 0 obj<</Type/Page/Parent 1 0 R/Contents[4 0 R]>>endobj
6 0 obj<</Type/Catalog/Pages 1 0 R>>endobj
4 0 obj<</Length {}>>stream
BT
/F1 48 Tf
100 600 Td
({}) Tj
ET
endstream endobj\n",
        content.len() + STREAM_CRUFT,
        content
    );
    let doc = format!(
        "{}xref
0 7
0000000000 65535 f 
0000000009 00000 n 
0000000096 00000 n 
0000000155 00000 n 
0000000291 00000 n 
0000000191 00000 n 
0000000248 00000 n 
trailer
<</Root 6 0 R/Size 7>>
startxref
{}
%%EOF",
        doc,
        doc.len()
    );

    let _doc = Document::load_mem(doc.as_bytes()).unwrap();
}

#[test]
fn load_too_deep_brackets() {
    let content: Vec<u8> = std::iter::repeat_n(b'(', MAX_BRACKET + 1)
        .chain(std::iter::repeat_n(b')', MAX_BRACKET + 1))
        .collect();
    let content = String::from_utf8(content).unwrap();
    const STREAM_CRUFT: usize = 33;
    let doc = format!(
        "%PDF-1.5
1 0 obj<</Type/Pages/Kids[5 0 R]/Count 1/Resources 3 0 R/MediaBox[0 0 595 842]>>endobj
2 0 obj<</Type/Font/Subtype/Type1/BaseFont/Courier>>endobj
3 0 obj<</Font<</F1 2 0 R>>>>endobj
5 0 obj<</Type/Page/Parent 1 0 R/Contents[7 0 R 4 0 R]>>endobj
6 0 obj<</Type/Catalog/Pages 1 0 R>>endobj
7 0 obj<</Length 45>>stream
BT /F1 48 Tf 100 600 Td (Hello World!) Tj ET
endstream
endobj
4 0 obj<</Length {}>>stream
BT
/F1 48 Tf
100 600 Td
({}) Tj
ET
endstream endobj\n",
        content.len() + STREAM_CRUFT,
        content
    );
    let doc = format!(
        "{}xref
0 7
0000000000 65535 f 
0000000009 00000 n 
0000000096 00000 n 
0000000155 00000 n 
0000000387 00000 n 
0000000191 00000 n 
0000000254 00000 n 
0000000297 00000 n 
trailer
<</Root 6 0 R/Size 7>>
startxref
{}
%%EOF",
        doc,
        doc.len()
    );

    let doc = Document::load_mem(doc.as_bytes()).unwrap();
    let pages = doc.get_pages().keys().cloned().collect::<Vec<_>>();
    assert_eq!("Hello World!\n", doc.extract_text(&pages).unwrap());
}

#[cfg(all(test, not(feature = "async")))]
#[test]
fn search_substring_finds_last_occurrence() {
    assert_eq!(Reader::search_substring(b"hello world", b"xyz", 0), None);
    assert_eq!(Reader::search_substring(b"hello world", b"world", 0), Some(6));

    let buffer = b"%%EOF\ntest%%EOF\nend";
    assert_eq!(Reader::search_substring(buffer, b"%%EOF", 0), Some(10));
    assert_eq!(Reader::search_substring(buffer, b"%%EOF", 6), Some(10));
    assert_eq!(Reader::search_substring(buffer, b"%%EOF", 15), None);
    assert_eq!(Reader::search_substring(b"%%EOF", b"%%EOF", 0), Some(0));

    let buffer_with_many_percents = b"%%%PDF-1.3%%%comment%%%more%%EOF";
    assert_eq!(
        Reader::search_substring(buffer_with_many_percents, b"%%EOF", 0),
        Some(27)
    );
}

// ── Phase 1 & 2 tests (Issue #468) ───────────────────────────────────────────

/// A minimal but valid PDF containing a single page with no objects in ObjStm.
/// Used as a fixture for LoadOptions tests.
#[cfg(all(test, not(feature = "async")))]
fn minimal_pdf_bytes() -> &'static [u8] {
    include_bytes!("../assets/example.pdf")
}

#[cfg(all(test, not(feature = "async")))]
#[test]
fn load_with_options_accepts_normal_document() {
    // Default options (256 MiB limit) should accept the small example PDF.
    let data = minimal_pdf_bytes();
    let opts = LoadOptions::new();
    let doc =
        Document::load_mem_with_options(data, opts.clone()).expect("example.pdf should be accepted by default options");
    assert_eq!(doc.version, "1.5");
}

#[cfg(all(test, not(feature = "async")))]
#[test]
fn load_with_options_rejects_oversized_document() {
    // Set a 1-byte limit — any real PDF must exceed it.
    let data = minimal_pdf_bytes();
    let opts = LoadOptions::new().max_file_bytes(1usize);
    let err =
        Document::load_mem_with_options(data, opts.clone()).expect_err("document larger than 1 byte must be rejected");
    match err {
        Error::DocumentTooLarge { size, limit } => {
            assert_eq!(limit, 1);
            assert_eq!(size, data.len());
        }
        other => panic!("expected DocumentTooLarge, got {other:?}"),
    }
}

#[cfg(all(test, not(feature = "async")))]
#[test]
fn load_with_options_unlimited() {
    // None = no size check — should succeed for any document.
    let data = minimal_pdf_bytes();
    let opts = LoadOptions::new().max_file_bytes(None);
    let doc = Document::load_mem_with_options(data, opts.clone()).expect("unlimited options must not reject documents");
    assert_eq!(doc.version, "1.5");
}

#[cfg(all(test, not(feature = "async")))]
#[test]
fn load_mem_with_options_lazy_objstm_no_objects_lost() {
    // When lazy_objstm = true, objects inside ObjStm must be accessible after
    // calling resolve_pending_object_streams.
    //
    // example.pdf uses ObjStm (PDF 1.5 cross-reference streams), so this
    // exercises the lazy path on real data.
    let data = minimal_pdf_bytes();
    let opts = LoadOptions::new().lazy_objstm(true).max_file_bytes(None);
    let mut lazy_doc =
        Document::load_mem_with_options(data, opts.clone()).expect("lazy load of example.pdf should succeed");

    // Eager-loaded reference document.
    let eager_doc = Document::load_mem(data).expect("eager load of example.pdf should succeed");

    // Before resolving, the lazy doc may have fewer objects.
    // After resolving it must match the eager doc.
    lazy_doc
        .resolve_pending_object_streams()
        .expect("resolve_pending_object_streams should not fail on valid data");

    assert_eq!(
        lazy_doc.objects.len(),
        eager_doc.objects.len(),
        "after resolve, lazy doc must have same object count as eager doc"
    );
    assert!(
        lazy_doc.pending_obj_streams.is_empty(),
        "pending_obj_streams must be empty after resolve"
    );
}

#[cfg(all(test, not(feature = "async")))]
#[test]
fn resolve_pending_object_streams_skips_objects_reassigned_to_newer_container() {
    let mut doc = Document::new();
    doc.reference_table.insert(
        7,
        XrefEntry::Compressed {
            container: 20,
            index: 0,
        },
    );

    let mut old_stream = ObjectStream::builder().compression_level(0).build();
    old_stream
        .add_object((7, 0), Object::Integer(1))
        .expect("old ObjStm should accept object");
    doc.objects
        .insert((10, 0), Object::Stream(old_stream.to_stream_object().unwrap()));

    let mut new_stream = ObjectStream::builder().compression_level(0).build();
    new_stream
        .add_object((7, 0), Object::Integer(2))
        .expect("new ObjStm should accept object");
    doc.objects
        .insert((20, 0), Object::Stream(new_stream.to_stream_object().unwrap()));

    doc.pending_obj_streams = vec![(10, 0), (20, 0)];
    doc.resolve_pending_object_streams()
        .expect("lazy ObjStm resolution should succeed");

    let resolved = doc
        .get_object((7, 0))
        .expect("object should resolve from the current ObjStm");
    assert_eq!(resolved.as_i64().expect("resolved object should stay an integer"), 2);
    assert!(
        !doc.objects.contains_key(&(10, 0)),
        "old ObjStm container should be dropped after resolution"
    );
    assert!(
        !doc.objects.contains_key(&(20, 0)),
        "new ObjStm container should be dropped after resolution"
    );
}

#[test]
fn load_options_builder() {
    let opts = LoadOptions::new().max_file_bytes(64 * 1024 * 1024).lazy_objstm(true);
    assert_eq!(opts.max_file_bytes, Some(64 * 1024 * 1024));
    assert!(opts.lazy_objstm);

    let no_limit = LoadOptions::new().max_file_bytes(None);
    assert_eq!(no_limit.max_file_bytes, None);

    let default = LoadOptions::default();
    assert_eq!(
        default.max_file_bytes,
        Some(crate::load_options::DEFAULT_MAX_FILE_BYTES)
    );
    assert!(!default.lazy_objstm);
}

/// Regression: a stream whose declared /Length is short (govdocs holdout
/// 885_885832) used to fail the stream parse, and the object was silently
/// re-parsed as a bare dictionary — the page's content vanished from the
/// loaded document. The parser now recovers by scanning for `endstream`.
#[cfg(all(test, not(feature = "async")))]
#[test]
fn load_stream_with_short_declared_length() {
    let body_text = b"BT /F1 12 Tf (Hello, world!) Tj ET";
    // Declares 10 bytes; the real content is longer.
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = Vec::new();
    let objects: Vec<Vec<u8>> = vec![
        b"<</Type/Catalog/Pages 2 0 R>>".to_vec(),
        b"<</Type/Pages/Count 1/Kids[3 0 R]>>".to_vec(),
        b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/Contents 4 0 R>>".to_vec(),
        format!(
            "<</Length 10>>stream\n{}\nendstream",
            String::from_utf8_lossy(body_text)
        )
        .into_bytes(),
    ];
    for (i, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj", i + 1).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_pos = pdf.len();
    // xref entries are exactly 20 bytes (note the trailing space before EOL).
    pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for off in &offsets {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<</Size 5/Root 1 0 R>>\nstartxref\n{}\n%%EOF\n", xref_pos).as_bytes());

    let doc = Document::load_mem(&pdf).expect("load");
    let pages = doc.get_pages();
    assert_eq!(pages.len(), 1);
    let page = doc.get_object(*pages.get(&1).unwrap()).unwrap().as_dict().unwrap();
    let Object::Reference(content_id) = page.get(b"Contents").unwrap() else {
        panic!("contents not a reference")
    };
    let Object::Stream(stream) = doc.get_object(*content_id).unwrap() else {
        panic!("content object lost its stream: {:?}", doc.get_object(*content_id))
    };
    assert_eq!(stream.content, body_text.to_vec());
}

#[cfg(all(test, not(feature = "async")))]
#[test]
fn get_xref_start_ignores_startxref_past_eof() {
    // Simulate a PDF with two revisions where the second has a corrupted %%EOF.
    // The valid %%EOF is at a known position; a second startxref appears after it
    // but belongs to the corrupted revision. get_xref_start must pick the
    // startxref *before* the valid %%EOF.
    let mut buf = Vec::new();
    // Padding so the buffer is large enough
    buf.extend_from_slice(&[b' '; 200]);
    // First (correct) startxref + xref offset + valid %%EOF
    let xref_offset = 100usize;
    let startxref_block = format!("startxref\n{}\n%%EOF\n", xref_offset);
    let _startxref_pos = buf.len();
    buf.extend_from_slice(startxref_block.as_bytes());
    // Second (corrupted) revision: another startxref pointing elsewhere, with %%EO\0
    let bad_block = "startxref\n999\n%%EO\x00\n";
    buf.extend_from_slice(bad_block.as_bytes());

    let result = Reader::get_xref_start(&buf).unwrap();
    // Should find the xref_offset from the first startxref (before valid %%EOF)
    assert_eq!(result, xref_offset);
    // Verify it did NOT pick up 999 from the corrupted revision
    assert_ne!(result, 999);
}
