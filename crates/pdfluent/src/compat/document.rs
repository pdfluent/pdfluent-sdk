//! Document operations for the compatible API (lane L2).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Read and write entry points on [`PdfDocument`], the read-side `Page`
//! methods, and the link and outline types. `Document` is an alias for the
//! facade type: the lane adds inherent blocks rather than a wrapper, and the
//! few L2 members that mutate take `&mut self` like every other mutation.

use super::constants;
use super::edit::{AnnotColors, Border, BorderOptions};
use super::geometry::{CompareOp, Matrix, Point, Rect};
use super::{ErrorKind, Page, PageMut, Value};
use crate::document::PdfDocument;
use crate::error::Result;

/// The compatible document handle: the facade type under its lane name.
pub type Document = PdfDocument;

/// One outline entry: level, title, 1-based page, and optional destination.
#[derive(Debug, Clone, PartialEq)]
pub struct TocEntry {
    /// Nesting level, starting at 1.
    pub level: i32,
    /// Entry title.
    pub title: String,
    /// 1-based destination page number.
    pub page: i32,
    /// Follow-on destination, present on full outlines.
    pub dest: Option<TocDest>,
}

/// A follow-on outline destination: kind, page, target, and zoom.
#[derive(Debug, Clone, PartialEq)]
pub struct TocDest {
    /// Destination kind code.
    pub kind: i32,
    /// Zero-based destination page number.
    pub page: i32,
    /// Target point on the page, in points.
    pub target: Point,
    /// Zoom factor, or zero for the default.
    pub zoom: f64,
}

/// Document metadata: the info dict plus format and encryption notes.
#[derive(Debug, Clone, PartialEq)]
pub struct DocMetadata {
    /// Document title.
    pub title: String,
    /// Document author.
    pub author: String,
    /// Document subject.
    pub subject: String,
    /// Document keywords.
    pub keywords: String,
    /// Creating application.
    pub creator: String,
    /// Producing application.
    pub producer: String,
    /// Creation date stamp.
    pub creation_date: String,
    /// Modification date stamp.
    pub mod_date: String,
    /// Format note such as the PDF version.
    pub format: String,
    /// Encryption note, or `None` when unencrypted.
    pub encryption: Option<String>,
    /// Trapping note.
    pub trapped: String,
}

/// One page label run: start page, prefix, first number, and style.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLabel {
    /// Zero-based first page of the run.
    pub start_page: i32,
    /// Label prefix.
    pub prefix: String,
    /// First page number of the run.
    pub first_number: i32,
    /// Numbering style code.
    pub style: String,
}

/// A page set: an explicit list or an inclusive range.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PageSelection {
    /// Explicit zero-based page numbers.
    List(Vec<i32>),
    /// Inclusive zero-based page range.
    Range(i32, i32),
}

/// An embedded-file handle: index or name.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum EmbItem {
    /// Zero-based embedded-file index.
    Index(i32),
    /// Embedded-file name.
    Name(String),
}

/// Options for adding or updating an embedded file.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct EmbFileOptions {
    /// Stored filename.
    pub filename: Option<String>,
    /// Display name.
    pub display_name: Option<String>,
    /// Description text.
    pub description: Option<String>,
}

/// One embedded file: names, description, and size.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EmbeddedFile {
    /// Embedded-file name.
    pub name: String,
    /// Stored filename.
    pub filename: String,
    /// Description text.
    pub description: String,
    /// Data length, in bytes.
    pub size: usize,
}

/// A chapter/page position for chaptered documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Location {
    /// Chapter number.
    pub chapter: i32,
    /// Page number within the chapter.
    pub page: i32,
}

/// An optional-content group: its name and visibility.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Ocg {
    /// Group name.
    pub name: String,
    /// Whether the group is on.
    pub on: bool,
}

/// Layer visibility updates for `set_layer`.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct LayerOptions {
    /// Base visibility state name.
    pub base_state: Option<String>,
    /// Groups to switch on.
    pub on: Vec<i32>,
    /// Groups to switch off.
    pub off: Vec<i32>,
}

/// Membership updates for `set_ocmd`.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct OcmdOptions {
    /// Member group numbers.
    pub ocgs: Vec<i32>,
    /// Visibility policy name.
    pub policy: Option<String>,
}

/// An optional-content membership dict: its policy and members.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct OcmdInfo {
    /// Visibility policy name.
    pub policy: String,
    /// Member group numbers.
    pub members: Vec<i32>,
}

/// Options for opening a document: file, bytes, or a fresh empty shell.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct DocumentOpenOptions {
    /// Path to open, if any.
    pub filename: Option<String>,
    /// Bytes to open, if any.
    pub data: Option<Vec<u8>>,
    /// File type hint for bytes without a name.
    pub filetype: Option<String>,
    /// Page box for a fresh document.
    pub rect: Option<Rect>,
    /// Page width for a fresh document, in points.
    pub width: f64,
    /// Page height for a fresh document, in points.
    pub height: f64,
    /// Default font size, in points.
    pub fontsize: f64,
    /// Backing archive handle, if any.
    pub archive: Option<Value>,
}

impl DocumentOpenOptions {
    /// Open this path.
    pub fn with_filename(mut self, filename: &str) -> Self {
        self.filename = Some(filename.to_string());
        self
    }

    /// Open these bytes.
    pub fn with_data(mut self, data: Vec<u8>) -> Self {
        self.data = Some(data);
        self
    }

    /// Hint the file type of nameless bytes.
    pub fn with_filetype(mut self, filetype: &str) -> Self {
        self.filetype = Some(filetype.to_string());
        self
    }

    /// Give fresh pages this box.
    pub fn with_rect(mut self, rect: Rect) -> Self {
        self.rect = Some(rect);
        self
    }

    /// Give fresh pages this size in points.
    pub fn with_size(mut self, width: f64, height: f64) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Set the default font size in points.
    pub fn with_fontsize(mut self, fontsize: f64) -> Self {
        self.fontsize = fontsize;
        self
    }

    /// Attach this backing archive handle.
    pub fn with_archive(mut self, archive: Value) -> Self {
        self.archive = Some(archive);
        self
    }
}

/// Options for saving: shared by `save_full`, `write`, `ez_save`, and `tobytes`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SaveSettings {
    /// Garbage-collection level.
    pub garbage: i32,
    /// Clean content streams.
    pub clean: bool,
    /// Deflate streams.
    pub deflate: bool,
    /// Deflate image streams.
    pub deflate_images: bool,
    /// Deflate font streams.
    pub deflate_fonts: bool,
    /// Append incrementally instead of rewriting.
    pub incremental: bool,
    /// Write ASCII-only output.
    pub ascii: bool,
    /// Expand compressed objects.
    pub expand: bool,
    /// Linearise for fast web view.
    pub linear: bool,
    /// Keep the existing file id.
    pub no_new_id: bool,
    /// Regenerate appearances.
    pub appearance: bool,
    /// Pretty-print objects.
    pub pretty: bool,
    /// Encryption selector.
    pub encryption: i32,
    /// Permission bits.
    pub permissions: i32,
    /// Owner password.
    pub owner_password: Option<String>,
    /// User password.
    pub user_password: Option<String>,
    /// Keep metadata.
    pub preserve_metadata: bool,
    /// Pack objects into object streams.
    pub use_objstms: bool,
    /// Deflate effort level.
    pub compression_effort: i32,
    /// Fail when the source needed repair.
    pub raise_on_repair: bool,
    /// Write byte-reproducible output.
    pub reproducible: bool,
}

impl Default for SaveSettings {
    fn default() -> Self {
        Self {
            garbage: 0,
            clean: false,
            deflate: false,
            deflate_images: false,
            deflate_fonts: false,
            incremental: false,
            ascii: false,
            expand: false,
            linear: false,
            no_new_id: false,
            appearance: false,
            pretty: false,
            encryption: 1,
            permissions: 4095,
            owner_password: None,
            user_password: None,
            preserve_metadata: true,
            use_objstms: false,
            compression_effort: 0,
            raise_on_repair: false,
            reproducible: false,
        }
    }
}

impl SaveSettings {
    /// Set the garbage-collection level.
    pub fn with_garbage(mut self, garbage: i32) -> Self {
        self.garbage = garbage;
        self
    }

    /// Clean content streams.
    pub fn with_clean(mut self, clean: bool) -> Self {
        self.clean = clean;
        self
    }

    /// Deflate streams.
    pub fn with_deflate(mut self, deflate: bool) -> Self {
        self.deflate = deflate;
        self
    }

    /// Deflate image streams.
    pub fn with_deflate_images(mut self, deflate_images: bool) -> Self {
        self.deflate_images = deflate_images;
        self
    }

    /// Deflate font streams.
    pub fn with_deflate_fonts(mut self, deflate_fonts: bool) -> Self {
        self.deflate_fonts = deflate_fonts;
        self
    }

    /// Append incrementally instead of rewriting.
    pub fn with_incremental(mut self, incremental: bool) -> Self {
        self.incremental = incremental;
        self
    }

    /// Write ASCII-only output.
    pub fn with_ascii(mut self, ascii: bool) -> Self {
        self.ascii = ascii;
        self
    }

    /// Expand compressed objects.
    pub fn with_expand(mut self, expand: bool) -> Self {
        self.expand = expand;
        self
    }

    /// Linearise for fast web view.
    pub fn with_linear(mut self, linear: bool) -> Self {
        self.linear = linear;
        self
    }

    /// Keep the existing file id.
    pub fn with_no_new_id(mut self, no_new_id: bool) -> Self {
        self.no_new_id = no_new_id;
        self
    }

    /// Regenerate appearances.
    pub fn with_appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }

    /// Pretty-print objects.
    pub fn with_pretty(mut self, pretty: bool) -> Self {
        self.pretty = pretty;
        self
    }

    /// Set the encryption selector.
    pub fn with_encryption(mut self, encryption: i32) -> Self {
        self.encryption = encryption;
        self
    }

    /// Set the permission bits.
    pub fn with_permissions(mut self, permissions: i32) -> Self {
        self.permissions = permissions;
        self
    }

    /// Set the owner password.
    pub fn with_owner_password(mut self, owner_password: &str) -> Self {
        self.owner_password = Some(owner_password.to_string());
        self
    }

    /// Set the user password.
    pub fn with_user_password(mut self, user_password: &str) -> Self {
        self.user_password = Some(user_password.to_string());
        self
    }

    /// Keep metadata.
    pub fn with_preserve_metadata(mut self, preserve_metadata: bool) -> Self {
        self.preserve_metadata = preserve_metadata;
        self
    }

    /// Pack objects into object streams.
    pub fn with_use_objstms(mut self, use_objstms: bool) -> Self {
        self.use_objstms = use_objstms;
        self
    }

    /// Set the deflate effort level.
    pub fn with_compression_effort(mut self, compression_effort: i32) -> Self {
        self.compression_effort = compression_effort;
        self
    }

    /// Fail when the source needed repair.
    pub fn with_raise_on_repair(mut self, raise_on_repair: bool) -> Self {
        self.raise_on_repair = raise_on_repair;
        self
    }

    /// Write byte-reproducible output.
    pub fn with_reproducible(mut self, reproducible: bool) -> Self {
        self.reproducible = reproducible;
        self
    }
}

/// Options for page insertion: source span plus what travels with it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InsertPdfOptions {
    /// First source page (-1 for the first page).
    pub from_page: i32,
    /// Last source page (-1 for the last page).
    pub to_page: i32,
    /// Target position (-1 appends).
    pub start_at: i32,
    /// Rotation applied to inserted pages.
    pub rotate: i32,
    /// Carry links over.
    pub links: bool,
    /// Carry annotations over.
    pub annots: bool,
    /// Carry widgets over.
    pub widgets: bool,
    /// Merge duplicate resources.
    pub join_duplicates: bool,
    /// Report progress while inserting.
    pub show_progress: bool,
    /// Finish the insertion.
    pub finish: bool,
}

impl Default for InsertPdfOptions {
    fn default() -> Self {
        Self {
            from_page: -1,
            to_page: -1,
            start_at: -1,
            rotate: -1,
            links: true,
            annots: true,
            widgets: true,
            join_duplicates: false,
            show_progress: false,
            finish: true,
        }
    }
}

impl InsertPdfOptions {
    /// Take source pages `from_page..=to_page` (-1 means the far end).
    pub fn with_span(mut self, from_page: i32, to_page: i32) -> Self {
        self.from_page = from_page;
        self.to_page = to_page;
        self
    }

    /// Insert at this target position (-1 appends).
    pub fn with_start_at(mut self, start_at: i32) -> Self {
        self.start_at = start_at;
        self
    }

    /// Rotate inserted pages by this many degrees.
    pub fn with_rotate(mut self, rotate: i32) -> Self {
        self.rotate = rotate;
        self
    }

    /// Carry links over.
    pub fn with_links(mut self, links: bool) -> Self {
        self.links = links;
        self
    }

    /// Carry annotations over.
    pub fn with_annots(mut self, annots: bool) -> Self {
        self.annots = annots;
        self
    }

    /// Carry widgets over.
    pub fn with_widgets(mut self, widgets: bool) -> Self {
        self.widgets = widgets;
        self
    }

    /// Merge duplicate resources.
    pub fn with_join_duplicates(mut self, join_duplicates: bool) -> Self {
        self.join_duplicates = join_duplicates;
        self
    }

    /// Report progress while inserting.
    pub fn with_show_progress(mut self, show_progress: bool) -> Self {
        self.show_progress = show_progress;
        self
    }

    /// Finish the insertion.
    pub fn with_finish(mut self, finish: bool) -> Self {
        self.finish = finish;
        self
    }
}

/// Options for `scrub`: one switch per removable trace.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ScrubOptions {
    /// Drop attached files.
    pub attached_files: bool,
    /// Clean page contents.
    pub clean_pages: bool,
    /// Drop embedded files.
    pub embedded_files: bool,
    /// Drop hidden text.
    pub hidden_text: bool,
    /// Drop JavaScript.
    pub javascript: bool,
    /// Drop metadata.
    pub metadata: bool,
    /// Apply pending redactions.
    pub redactions: bool,
    /// Redact images at this effort level.
    pub redact_images: i32,
    /// Drop links.
    pub remove_links: bool,
    /// Reset form fields.
    pub reset_fields: bool,
    /// Drop annotation responses.
    pub reset_responses: bool,
    /// Drop thumbnails.
    pub thumbnails: bool,
    /// Drop XML metadata.
    pub xml_metadata: bool,
}

impl Default for ScrubOptions {
    fn default() -> Self {
        Self {
            attached_files: true,
            clean_pages: true,
            embedded_files: true,
            hidden_text: true,
            javascript: true,
            metadata: true,
            redactions: true,
            redact_images: 0,
            remove_links: true,
            reset_fields: true,
            reset_responses: true,
            thumbnails: true,
            xml_metadata: true,
        }
    }
}

impl ScrubOptions {
    /// Drop attached files.
    pub fn with_attached_files(mut self, attached_files: bool) -> Self {
        self.attached_files = attached_files;
        self
    }

    /// Clean page contents.
    pub fn with_clean_pages(mut self, clean_pages: bool) -> Self {
        self.clean_pages = clean_pages;
        self
    }

    /// Drop embedded files.
    pub fn with_embedded_files(mut self, embedded_files: bool) -> Self {
        self.embedded_files = embedded_files;
        self
    }

    /// Drop hidden text.
    pub fn with_hidden_text(mut self, hidden_text: bool) -> Self {
        self.hidden_text = hidden_text;
        self
    }

    /// Drop JavaScript.
    pub fn with_javascript(mut self, javascript: bool) -> Self {
        self.javascript = javascript;
        self
    }

    /// Drop metadata.
    pub fn with_metadata(mut self, metadata: bool) -> Self {
        self.metadata = metadata;
        self
    }

    /// Apply pending redactions.
    pub fn with_redactions(mut self, redactions: bool) -> Self {
        self.redactions = redactions;
        self
    }

    /// Redact images at this effort level.
    pub fn with_redact_images(mut self, redact_images: i32) -> Self {
        self.redact_images = redact_images;
        self
    }

    /// Drop links.
    pub fn with_remove_links(mut self, remove_links: bool) -> Self {
        self.remove_links = remove_links;
        self
    }

    /// Reset form fields.
    pub fn with_reset_fields(mut self, reset_fields: bool) -> Self {
        self.reset_fields = reset_fields;
        self
    }

    /// Drop annotation responses.
    pub fn with_reset_responses(mut self, reset_responses: bool) -> Self {
        self.reset_responses = reset_responses;
        self
    }

    /// Drop thumbnails.
    pub fn with_thumbnails(mut self, thumbnails: bool) -> Self {
        self.thumbnails = thumbnails;
        self
    }

    /// Drop XML metadata.
    pub fn with_xml_metadata(mut self, xml_metadata: bool) -> Self {
        self.xml_metadata = xml_metadata;
        self
    }
}

/// Options for `bake`: which live objects to flatten into content.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BakeOptions {
    /// Flatten annotations.
    pub annots: bool,
    /// Flatten widgets.
    pub widgets: bool,
}

impl Default for BakeOptions {
    fn default() -> Self {
        Self {
            annots: true,
            widgets: true,
        }
    }
}

/// Options for `set_toc_item`: the updatable destination fields.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct TocItemOptions {
    /// Destination kind code.
    pub kind: Option<i32>,
    /// Zero-based destination page.
    pub page: Option<i32>,
    /// Destination URI.
    pub uri: Option<String>,
    /// Entry title.
    pub title: Option<String>,
    /// Target point on the page.
    pub target: Option<Point>,
    /// Destination filename for remote links.
    pub filename: Option<String>,
    /// Zoom factor.
    pub zoom: f64,
}

/// Options for `insert_page`: text plus page styling.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InsertPageOptions {
    /// Text lines for the fresh page.
    pub text: Vec<String>,
    /// Font size, in points.
    pub fontsize: f64,
    /// Page width, in points.
    pub width: f64,
    /// Page height, in points.
    pub height: f64,
    /// Font name.
    pub fontname: String,
    /// Font file path, if any.
    pub fontfile: Option<String>,
    /// Text colour components.
    pub color: Vec<f64>,
}

impl Default for InsertPageOptions {
    fn default() -> Self {
        Self {
            text: Vec::new(),
            fontsize: 11.0,
            width: 595.0,
            height: 842.0,
            fontname: "helv".to_string(),
            fontfile: None,
            color: vec![0.0],
        }
    }
}

impl InsertPageOptions {
    /// Put these text lines on the fresh page.
    pub fn with_text(mut self, text: Vec<String>) -> Self {
        self.text = text;
        self
    }

    /// Set the font size in points.
    pub fn with_fontsize(mut self, fontsize: f64) -> Self {
        self.fontsize = fontsize;
        self
    }

    /// Set the page size in points.
    pub fn with_size(mut self, width: f64, height: f64) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Set the font name.
    pub fn with_fontname(mut self, fontname: &str) -> Self {
        self.fontname = fontname.to_string();
        self
    }

    /// Take the font from this file.
    pub fn with_fontfile(mut self, fontfile: &str) -> Self {
        self.fontfile = Some(fontfile.to_string());
        self
    }

    /// Set the text colour components.
    pub fn with_color(mut self, color: Vec<f64>) -> Self {
        self.color = color;
        self
    }
}

/// A document link: owned snapshot data shared with insert/update calls.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    /// Link rectangle, in points.
    pub rect: Rect,
    /// Link kind code.
    pub kind: i32,
    /// Target URI for remote links.
    pub uri: String,
    /// Link object number.
    pub xref: i32,
    /// Link identifier.
    pub id: String,
    /// Border styling.
    pub border: Border,
    /// Stroke and fill colours.
    pub colors: AnnotColors,
    /// Link flags bitfield.
    pub flags: i32,
    /// Follow-on destination for local links.
    pub dest: Option<LinkDest>,
    /// Next link on the page, filled in when links are listed.
    pub next: Option<Box<Link>>,
}

/// A link destination: page, target point, and zoom.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LinkDest {
    /// Zero-based destination page.
    pub page: i32,
    /// Target point on the page, in points.
    pub target: Point,
    /// Zoom factor.
    pub zoom: f64,
}

impl LinkDest {
    /// Build a destination from its page, target point, and zoom.
    pub fn new(page: i32, x: f64, y: f64, zoom: f64) -> Self {
        Self {
            page,
            target: Point { x, y },
            zoom,
        }
    }
}

/// A retained rendering of a page, replayed through `run`.
#[derive(Clone)]
pub struct DisplayList {
    rect: Rect,
}

impl DisplayList {
    /// Page area the rendering covers.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// Replay the rendering through a device.
    ///
    /// Replay stays unimplemented in this version; freezing (see
    /// [`Page::get_displaylist`]) works, running it back does not.
    pub fn run(&self, _matrix: Matrix) -> Result<()> {
        let _ = _matrix;
        Err(super::not_supported("DisplayList", "run"))
    }
}

/// Options for layout reads; reserved for the recorded keyword set.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct LayoutOptions {}

/// Resolve a possibly negative page index against a page count.
fn resolve_index(index: i32, count: usize) -> Result<usize> {
    let count_i64 = count as i64;
    let resolved = if index < 0 {
        count_i64 + index as i64
    } else {
        index as i64
    };
    if resolved < 0 || resolved >= count_i64 {
        return Err(ErrorKind::IndexOutOfRange
            .to_error(format!("page index {index} out of range ({count} pages)")));
    }
    Ok(resolved as usize)
}

/// Page object ids in page order.
fn page_ids(doc: &PdfDocument) -> Vec<lopdf::ObjectId> {
    let pages = doc.lopdf().get_pages();
    let mut numbers: Vec<u32> = pages.keys().copied().collect();
    numbers.sort_unstable();
    numbers
        .into_iter()
        .filter_map(|n| pages.get(&n).copied())
        .collect()
}

/// Number of an object as the spec's `i32` xref.
fn xref_of(id: lopdf::ObjectId) -> i32 {
    id.0 as i32
}

/// Look up an object by spec xref, rejecting unknown numbers.
fn object_by_xref(doc: &PdfDocument, xref: i32) -> Result<&lopdf::Object> {
    if xref <= 0 {
        return Err(ErrorKind::BadValue.to_error(format!("bad xref {xref}")));
    }
    // Object numbers are unique per generation; the live object is the one
    // the trailer generation points at, which lopdf stores under its id.
    let found = doc
        .lopdf()
        .objects
        .iter()
        .find(|(id, _)| id.0 == xref as u32)
        .map(|(_, obj)| obj);
    found.ok_or_else(|| ErrorKind::BadValue.to_error(format!("unknown xref {xref}")))
}

/// A PDF number (integer or real) as `f64`.
fn number_as_f64(obj: &lopdf::Object) -> Option<f64> {
    match obj {
        lopdf::Object::Integer(i) => Some(*i as f64),
        lopdf::Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

/// Read a rectangle array as y-up `[x0, y0, x1, y1]`.
fn array_as_rect(obj: &lopdf::Object) -> Option<[f64; 4]> {
    let items = obj.as_array().ok()?;
    if items.len() != 4 {
        return None;
    }
    Some([
        number_as_f64(&items[0])?,
        number_as_f64(&items[1])?,
        number_as_f64(&items[2])?,
        number_as_f64(&items[3])?,
    ])
}

/// Inherited page-tree lookup: walk from the page dict up through `/Parent`
/// until `key` is found.
fn inherited(
    lopdf: &lopdf::Document,
    mut dict_id: lopdf::ObjectId,
    key: &[u8],
) -> Option<lopdf::Object> {
    for _ in 0..64 {
        let dict = lopdf.get_dictionary(dict_id).ok()?;
        if let Ok(value) = dict.get(key) {
            return Some(value.clone());
        }
        dict_id = dict.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

/// Resolved page boxes (y-up) plus the inherited rotation.
struct PageFrames {
    media: [f64; 4],
    crop: [f64; 4],
    rotation: i32,
}

/// Read the media and crop boxes of a page, applying `/Parent` inheritance
/// and the ISO 32000 fallbacks (crop defaults to media).
fn page_frames(doc: &PdfDocument, index: usize) -> Result<PageFrames> {
    let ids = page_ids(doc);
    let id = *ids.get(index).ok_or_else(|| {
        ErrorKind::IndexOutOfRange.to_error(format!("page index {index} out of range"))
    })?;
    let lopdf = doc.lopdf();
    let media = inherited(lopdf, id, b"MediaBox")
        .as_ref()
        .and_then(array_as_rect)
        .unwrap_or([0.0, 0.0, 595.0, 842.0]);
    let crop = inherited(lopdf, id, b"CropBox")
        .as_ref()
        .and_then(array_as_rect)
        .unwrap_or(media);
    let rotation = inherited(lopdf, id, b"Rotate")
        .as_ref()
        .and_then(number_as_f64)
        .unwrap_or(0.0) as i32;
    Ok(PageFrames {
        media,
        crop,
        rotation: rotation.rem_euclid(360),
    })
}

/// Flip a y-up box into the lane's top-left, y-down [`Rect`].
fn flip_y(box_y_up: [f64; 4], page_top: f64) -> Rect {
    let (x0, y0, x1, y1) = (box_y_up[0], box_y_up[1], box_y_up[2], box_y_up[3]);
    Rect {
        x0: x0.min(x1),
        y0: page_top - y1.max(y0),
        x1: x0.max(x1),
        y1: page_top - y0.min(y1),
    }
}

/// Crop box of a page as a y-down [`Rect`].
fn page_rect(doc: &PdfDocument, index: usize) -> Result<Rect> {
    let frames = page_frames(doc, index)?;
    Ok(flip_y(frames.crop, frames.crop[3].max(frames.crop[1])))
}

/// Read one named page box, defaulting art/bleed/trim to the crop box.
fn named_page_box(doc: &PdfDocument, index: usize, key: &[u8]) -> Result<Rect> {
    let frames = page_frames(doc, index)?;
    let top = frames.crop[3].max(frames.crop[1]);
    if key == b"CropBox" {
        return Ok(flip_y(frames.crop, top));
    }
    if key == b"MediaBox" {
        return Ok(flip_y(frames.media, frames.media[3].max(frames.media[1])));
    }
    let ids = page_ids(doc);
    let id = ids[index];
    let found = inherited(doc.lopdf(), id, key)
        .as_ref()
        .and_then(array_as_rect);
    Ok(flip_y(found.unwrap_or(frames.crop), top))
}

/// Catalog dictionary reference, when the trailer names one.
fn catalog_id(doc: &PdfDocument) -> Option<lopdf::ObjectId> {
    doc.lopdf().trailer.get(b"Root").ok()?.as_reference().ok()
}

/// Minimal PDF object serialiser for the `pdf_catalog` / `pdf_trailer` /
/// `xref_object` reads: names, numbers, strings, arrays, dicts, references,
/// and a placeholder for stream payloads.
fn write_object(out: &mut String, obj: &lopdf::Object, ascii: bool) {
    match obj {
        lopdf::Object::Null => out.push_str("null"),
        lopdf::Object::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        lopdf::Object::Integer(i) => out.push_str(&i.to_string()),
        lopdf::Object::Real(r) => out.push_str(&format!("{r}")),
        lopdf::Object::Name(name) => {
            out.push('/');
            out.push_str(&String::from_utf8_lossy(name));
        }
        lopdf::Object::String(bytes, _) => {
            out.push('(');
            for byte in bytes {
                if ascii && !byte.is_ascii() {
                    out.push_str(&format!("\\{byte:03o}"));
                } else {
                    match byte {
                        b'(' | b')' | b'\\' => {
                            out.push('\\');
                            out.push(*byte as char);
                        }
                        _ if byte.is_ascii_graphic() || *byte == b' ' => out.push(*byte as char),
                        _ => out.push_str(&format!("\\{byte:03o}")),
                    }
                }
            }
            out.push(')');
        }
        lopdf::Object::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                write_object(out, item, ascii);
            }
            out.push(']');
        }
        lopdf::Object::Dictionary(dict) => write_dict(out, dict, ascii),
        lopdf::Object::Stream(stream) => {
            write_dict(out, &stream.dict, ascii);
            out.push_str(" stream ... endstream");
        }
        lopdf::Object::Reference(id) => out.push_str(&format!("{} {} R", id.0, id.1)),
    }
}

/// Serialise one dictionary.
fn write_dict(out: &mut String, dict: &lopdf::Dictionary, ascii: bool) {
    out.push_str("<<");
    for (key, value) in dict.iter() {
        out.push(' ');
        out.push('/');
        out.push_str(&String::from_utf8_lossy(key));
        out.push(' ');
        write_object(out, value, ascii);
    }
    out.push_str(" >>");
}

/// Serialise one object to text.
fn object_source(obj: &lopdf::Object, ascii: bool) -> String {
    let mut out = String::new();
    write_object(&mut out, obj, ascii);
    out
}

/// Short type name of an object for `xref_get_key`.
fn object_type_name(obj: &lopdf::Object) -> &'static str {
    match obj {
        lopdf::Object::Null => "null",
        lopdf::Object::Boolean(_) => "bool",
        lopdf::Object::Integer(_) => "int",
        lopdf::Object::Real(_) => "real",
        lopdf::Object::Name(_) => "name",
        lopdf::Object::String(_, _) => "str",
        lopdf::Object::Array(_) => "array",
        lopdf::Object::Dictionary(_) => "dict",
        lopdf::Object::Stream(_) => "stream",
        lopdf::Object::Reference(_) => "ref",
    }
}

/// Catalog name entry (`/PageLayout`, `/PageMode`, `/Lang`) as text.
fn catalog_text(doc: &PdfDocument, key: &[u8]) -> String {
    let Some(id) = catalog_id(doc) else {
        return String::new();
    };
    let Ok(dict) = doc.lopdf().get_dictionary(id) else {
        return String::new();
    };
    let Ok(obj) = dict.get(key) else {
        return String::new();
    };
    match obj {
        lopdf::Object::Name(name) => String::from_utf8_lossy(name).into_owned(),
        lopdf::Object::String(bytes, _) => String::from_utf8_lossy(bytes).into_owned(),
        _ => String::new(),
    }
}

/// The `/AcroForm` dictionary, resolved through references.
fn acroform_dict(doc: &PdfDocument) -> Option<lopdf::Dictionary> {
    let id = catalog_id(doc)?;
    let lopdf = doc.lopdf();
    let form = lopdf
        .get_dictionary(id)
        .ok()?
        .get(b"AcroForm")
        .ok()?
        .clone();
    match form {
        lopdf::Object::Dictionary(dict) => Some(dict.clone()),
        lopdf::Object::Reference(ref_id) => lopdf.get_dictionary(ref_id).ok().cloned(),
        lopdf::Object::Stream(ref stream) => Some(stream.dict.clone()),
        _ => None,
    }
}

/// Layer interface rows over the default configuration's `/Order`.
fn layer_ui_entries(doc: &PdfDocument) -> Vec<Value> {
    let lopdf = doc.lopdf();
    let Some(root) = catalog_id(doc) else {
        return Vec::new();
    };
    let props = lopdf
        .get_dictionary(root)
        .ok()
        .and_then(|catalog| catalog.get(b"OCProperties").ok().cloned())
        .and_then(|obj| match obj {
            lopdf::Object::Dictionary(dict) => Some(dict),
            lopdf::Object::Reference(id) => lopdf.get_dictionary(id).ok().cloned(),
            _ => None,
        });
    let Some(props) = props else {
        return Vec::new();
    };
    let base = props
        .get(b"D")
        .ok()
        .cloned()
        .and_then(|obj| match obj {
            lopdf::Object::Dictionary(dict) => Some(dict),
            lopdf::Object::Reference(id) => lopdf.get_dictionary(id).ok().cloned(),
            _ => None,
        })
        .unwrap_or_default();
    let order = base
        .get(b"Order")
        .ok()
        .cloned()
        .and_then(|obj| match obj {
            lopdf::Object::Array(items) => Some(items),
            lopdf::Object::Reference(id) => lopdf
                .get_object(id)
                .ok()
                .and_then(|o| o.as_array().ok().cloned()),
            _ => None,
        })
        .unwrap_or_default();
    let refs_of = |key: &[u8]| -> Vec<lopdf::ObjectId> {
        base.get(key)
            .ok()
            .and_then(|o| o.as_array().ok().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|o| o.as_reference().ok())
            .collect()
    };
    let ctx = LayerUiCtx {
        on: refs_of(b"ON"),
        off: refs_of(b"OFF"),
        locked: refs_of(b"Locked"),
        base_on: !matches!(
            base.get(b"BaseState"),
            Ok(lopdf::Object::Name(name)) if name.as_slice() != b"ON"
        ),
        rb_groups: base
            .get(b"RBGroups")
            .ok()
            .and_then(|o| o.as_array().ok().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|group| {
                let items = match group {
                    lopdf::Object::Array(items) => items,
                    lopdf::Object::Reference(id) => lopdf
                        .get_object(id)
                        .ok()
                        .and_then(|o| o.as_array().ok().cloned())?,
                    _ => return None,
                };
                let mut ids: Vec<lopdf::ObjectId> = items
                    .into_iter()
                    .filter_map(|o| o.as_reference().ok())
                    .collect();
                ids.sort_unstable();
                Some(ids)
            })
            .collect(),
    };
    let mut out = Vec::new();
    walk_layer_order(lopdf, &order, 0, false, &ctx, &mut out);
    out
}

/// Shared state for the `/Order` walk: visibility sets and radio groups.
struct LayerUiCtx {
    on: Vec<lopdf::ObjectId>,
    off: Vec<lopdf::ObjectId>,
    locked: Vec<lopdf::ObjectId>,
    base_on: bool,
    rb_groups: Vec<Vec<lopdf::ObjectId>>,
}

impl LayerUiCtx {
    /// Whether a group shows: the same rule `set_layer_ui_config` toggles.
    fn shown(&self, id: lopdf::ObjectId) -> bool {
        !self.off.contains(&id) && (self.on.contains(&id) || self.base_on)
    }
}

/// One `/Order` level into interface rows.
fn walk_layer_order(
    lopdf: &lopdf::Document,
    items: &[lopdf::Object],
    depth: i32,
    radio: bool,
    ctx: &LayerUiCtx,
    out: &mut Vec<Value>,
) {
    for item in items {
        let resolved = match item {
            lopdf::Object::Reference(id) => lopdf.get_object(*id).ok().cloned(),
            _ => Some(item.clone()),
        };
        match (item, resolved) {
            (lopdf::Object::Reference(id), Some(lopdf::Object::Dictionary(dict))) => {
                let text = match dict.get(b"Name") {
                    Ok(lopdf::Object::String(bytes, _)) => {
                        String::from_utf8_lossy(bytes).into_owned()
                    }
                    Ok(lopdf::Object::Name(name)) => String::from_utf8_lossy(name).into_owned(),
                    _ => String::new(),
                };
                push_layer_row(
                    out,
                    depth,
                    i32::from(ctx.locked.contains(id)),
                    i32::from(ctx.shown(*id)),
                    text,
                    if radio { "radiobox" } else { "checkbox" },
                );
            }
            (_, Some(lopdf::Object::Array(nested))) => {
                let mut ids: Vec<lopdf::ObjectId> = nested
                    .iter()
                    .filter_map(|o| o.as_reference().ok())
                    .collect();
                ids.sort_unstable();
                let nested_radio = ctx.rb_groups.contains(&ids);
                walk_layer_order(lopdf, &nested, depth + 1, nested_radio, ctx, out);
            }
            (_, Some(lopdf::Object::String(bytes, _))) => {
                push_layer_row(
                    out,
                    depth,
                    0,
                    1,
                    String::from_utf8_lossy(&bytes).into_owned(),
                    "label",
                );
            }
            _ => {}
        }
    }
}

/// Append one interface row; the number is the row's own position.
fn push_layer_row(
    out: &mut Vec<Value>,
    depth: i32,
    locked: i32,
    on: i32,
    text: String,
    kind: &str,
) {
    let number = out.len() as i32;
    let mut row = std::collections::HashMap::new();
    row.insert("depth".to_string(), Value::Int(i64::from(depth)));
    row.insert("locked".to_string(), Value::Int(i64::from(locked)));
    row.insert("number".to_string(), Value::Int(i64::from(number)));
    row.insert("on".to_string(), Value::Int(i64::from(on)));
    row.insert("text".to_string(), Value::Str(text));
    row.insert("type".to_string(), Value::Str(kind.to_string()));
    out.push(Value::Dict(row));
}

/// Permission bits: `/Encrypt /P` for encrypted documents, all-granted for
/// plain ones.
fn permission_bits(doc: &PdfDocument) -> i32 {
    let lopdf = doc.lopdf();
    let encrypt = lopdf.trailer.get(b"Encrypt").ok().cloned();
    let dict = encrypt.and_then(|obj| match obj {
        lopdf::Object::Dictionary(dict) => Some(dict.clone()),
        lopdf::Object::Reference(id) => lopdf.get_dictionary(id).ok().cloned(),
        _ => None,
    });
    dict.as_ref()
        .and_then(|d| d.get(b"P").ok()?.as_i64().ok())
        .map(|p| p as i32)
        .unwrap_or(-4)
}

/// Flatten the outline tree into spec entries.
fn flatten_toc(items: &[crate::structure::Outline], level: i32, out: &mut Vec<TocEntry>) {
    for item in items {
        let page = item.page.map(|p| p as i32 + 1).unwrap_or(0);
        out.push(TocEntry {
            level,
            title: item.title.clone(),
            page,
            dest: None,
        });
        flatten_toc(&item.children, level + 1, out);
    }
}

/// Flatten with follow-on destinations from the bookmark actions.
fn flatten_full_toc(
    doc: &PdfDocument,
    items: &[pdf_manip::bookmarks::OutlineEntry],
    level: i32,
    out: &mut Vec<TocEntry>,
) {
    use pdf_manip::bookmarks::{BookmarkAction, FitType};
    for item in items {
        let (kind, page_0, target, zoom) = match &item.action {
            Some(BookmarkAction::GoTo { page, fit }) => {
                let page_0 = (*page as i32).saturating_sub(1);
                let (target, zoom) = match fit {
                    FitType::Xyz { x, y, zoom } => {
                        let height = page_frames(doc, page_0.max(0) as usize)
                            .map(|f| f.crop[3].max(f.crop[1]) - f.crop[1].min(f.crop[3]))
                            .unwrap_or(0.0);
                        let tx = x.unwrap_or(0.0) as f64;
                        let ty = height - y.unwrap_or(0.0) as f64;
                        (Point { x: tx, y: ty }, zoom.unwrap_or(0.0) as f64)
                    }
                    FitType::FitH(top) => (
                        Point {
                            x: 0.0,
                            y: *top as f64,
                        },
                        0.0,
                    ),
                    FitType::FitV(left) => (
                        Point {
                            x: *left as f64,
                            y: 0.0,
                        },
                        0.0,
                    ),
                    FitType::Fit => (Point { x: 0.0, y: 0.0 }, 0.0),
                };
                (constants::LINK_GOTO, page_0, target, zoom)
            }
            Some(BookmarkAction::GoToR { page, .. }) => (
                constants::LINK_GOTOR,
                (*page as i32).saturating_sub(1),
                Point { x: 0.0, y: 0.0 },
                0.0,
            ),
            Some(BookmarkAction::Uri(_)) => {
                (constants::LINK_URI, -1, Point { x: 0.0, y: 0.0 }, 0.0)
            }
            Some(BookmarkAction::Named(_)) => {
                (constants::LINK_NAMED, -1, Point { x: 0.0, y: 0.0 }, 0.0)
            }
            None => (constants::LINK_NONE, -1, Point { x: 0.0, y: 0.0 }, 0.0),
        };
        // The full row mirrors the simple row's page: -1 without a target.
        let sheet = if item.action.is_none() {
            -1
        } else if page_0 >= 0 {
            page_0 + 1
        } else {
            0
        };
        out.push(TocEntry {
            level,
            title: item.title.clone(),
            page: sheet,
            dest: Some(TocDest {
                kind,
                page: page_0,
                target,
                zoom,
            }),
        });
        flatten_full_toc(doc, &item.children, level + 1, out);
    }
}

/// Object numbers of the outline items, in tree order.
fn outline_ids(lopdf: &lopdf::Document, mut dict_id: lopdf::ObjectId, out: &mut Vec<i32>) {
    for _ in 0..4096 {
        let Ok(dict) = lopdf.get_dictionary(dict_id) else {
            return;
        };
        out.push(dict_id.0 as i32);
        if let Ok(first) = dict.get(b"First").and_then(|o| o.as_reference()) {
            let mut kids = Vec::new();
            outline_ids(lopdf, first, &mut kids);
            out.append(&mut kids);
        }
        let Ok(next) = dict.get(b"Next").and_then(|o| o.as_reference()) else {
            return;
        };
        dict_id = next;
    }
}

/// Build the write bytes for `save_full` / `tobytes` / `write`: encryption is
/// applied to a scratch copy so `&self` methods never mutate the handle.
fn rendered_bytes(doc: &PdfDocument, options: &SaveSettings) -> Result<Vec<u8>> {
    if options.linear {
        return Err(crate::Error::Unsupported(
            "compat save with linear=true: linearization is not implemented; \
             save without it, or run qpdf --linearize afterwards."
                .to_string(),
        ));
    }
    if options.encryption == constants::PDF_ENCRYPT_NONE
        || options.encryption == constants::PDF_ENCRYPT_KEEP
    {
        if options.incremental {
            return doc.to_incremental_bytes();
        }
        return doc.to_bytes();
    }
    let algorithm = match options.encryption {
        v if v == constants::PDF_ENCRYPT_AES_256 => pdf_manip::encrypt::EncryptionAlgorithm::Aes256,
        v if v == constants::PDF_ENCRYPT_AES_128 => pdf_manip::encrypt::EncryptionAlgorithm::Aes128,
        v if v == constants::PDF_ENCRYPT_RC4_128 => {
            pdf_manip::encrypt::EncryptionAlgorithm::Rc4_128
        }
        v if v == constants::PDF_ENCRYPT_RC4_40 => pdf_manip::encrypt::EncryptionAlgorithm::Rc4_40,
        other => {
            return Err(ErrorKind::BadValue.to_error(format!("bad encryption {other}")));
        }
    };
    let user_password = options
        .user_password
        .clone()
        .unwrap_or_default()
        .into_bytes();
    let owner_password = options
        .owner_password
        .clone()
        .map(String::into_bytes)
        .unwrap_or_else(|| user_password.clone());
    let config = pdf_manip::encrypt::EncryptConfig {
        user_password,
        owner_password,
        algorithm,
        permissions: pdf_manip::encrypt::Permissions::from_p_value(options.permissions),
    };
    let mut scratch = doc.lopdf().clone();
    let mut buf = Vec::new();
    pdf_manip::encrypt::encrypt_and_save(&mut scratch, &config, &mut buf)?;
    Ok(buf)
}

/// External-object numbers named by a page's `/Resources /XObject`.
fn page_xobject_ids(doc: &PdfDocument, index: usize) -> Result<Vec<i32>> {
    let ids = page_ids(doc);
    let id = *ids.get(index).ok_or_else(|| {
        ErrorKind::IndexOutOfRange.to_error(format!("page index {index} out of range"))
    })?;
    let lopdf = doc.lopdf();
    let resources = inherited(lopdf, id, b"Resources");
    let dict = resources.as_ref().and_then(|obj| match obj {
        lopdf::Object::Dictionary(dict) => Some(dict.clone()),
        lopdf::Object::Reference(ref_id) => lopdf.get_dictionary(*ref_id).ok().cloned(),
        lopdf::Object::Stream(stream) => Some(stream.dict.clone()),
        _ => None,
    });
    let xobjects = dict.as_ref().and_then(|d| {
        d.get(b"XObject").ok().cloned().and_then(|obj| match obj {
            lopdf::Object::Dictionary(dict) => Some(dict.clone()),
            lopdf::Object::Reference(ref_id) => lopdf.get_dictionary(ref_id).ok().cloned(),
            _ => None,
        })
    });
    let mut out = Vec::new();
    if let Some(dict) = xobjects {
        for (_, value) in dict.iter() {
            if let Ok(ref_id) = value.as_reference() {
                out.push(xref_of(ref_id));
            }
        }
    }
    out.sort_unstable();
    Ok(out)
}

impl PdfDocument {
    /// Whether a chapter position exists.
    pub fn contains(&self, location: &Location) -> Result<bool> {
        if location.chapter != 0 || location.page < 0 {
            return Ok(false);
        }
        Ok((location.page as usize) < self.page_count())
    }

    /// Delete the page at an index.
    pub fn delete_at(&mut self, index: i32) -> Result<()> {
        let count = self.page_count();
        let target = resolve_index(index, count)?;
        let mut scratch = self.lopdf().clone();
        let pages = scratch.get_pages();
        let mut numbers: Vec<u32> = pages.keys().copied().collect();
        numbers.sort_unstable();
        let Some(page_number) = numbers.get(target).copied() else {
            return Err(
                ErrorKind::IndexOutOfRange.to_error(format!("page index {index} out of range"))
            );
        };
        let page_id = pages[&page_number];
        // Unlink the page from its parent's /Kids and fix the /Count chain.
        let parent = scratch
            .get_dictionary(page_id)
            .ok()
            .and_then(|d| d.get(b"Parent").ok()?.as_reference().ok());
        scratch.objects.remove(&page_id);
        if let Some(parent_id) = parent {
            if let Ok(dict) = scratch.get_dictionary_mut(parent_id) {
                if let Ok(kids) = dict.get_mut(b"Kids").and_then(|k| k.as_array_mut()) {
                    kids.retain(|kid| kid.as_reference().ok() != Some(page_id));
                }
                let count = dict
                    .get(b"Count")
                    .ok()
                    .and_then(|c| c.as_i64().ok())
                    .unwrap_or(0);
                dict.set(b"Count", lopdf::Object::Integer((count - 1).max(0)));
            }
        }
        *self = Self::from_lopdf(scratch)?;
        Ok(())
    }

    /// Borrow for a managed block.
    pub fn enter(&self) -> &Self {
        self
    }

    /// Leave a managed block.
    pub fn exit(&mut self) -> Result<()> {
        Ok(())
    }

    /// Page at an index.
    pub fn at(&self, index: i32) -> Result<Page<'_>> {
        let target = resolve_index(index, self.page_count())?;
        Ok(Page::new(self, target))
    }

    /// Open a file, bytes, or a fresh shell.
    pub fn open_compat(options: &DocumentOpenOptions) -> Result<Self> {
        if let Some(data) = &options.data {
            if data.is_empty() {
                return Err(ErrorKind::EmptyFile.to_error("empty stream"));
            }
            let pdf = super::open::input_to_pdf_bytes(
                data,
                options.filetype.as_deref(),
                options.filename.as_deref(),
            )?;
            return Self::from_bytes(&pdf);
        }
        if let Some(filename) = &options.filename {
            let bytes = std::fs::read(filename).map_err(|source| {
                if source.kind() == std::io::ErrorKind::NotFound {
                    ErrorKind::MissingFile.to_error(filename.clone())
                } else {
                    crate::Error::Io {
                        source,
                        path: Some(filename.into()),
                    }
                }
            })?;
            if bytes.is_empty() {
                return Err(ErrorKind::EmptyFile.to_error(format!("{filename} is empty")));
            }
            let pdf = super::open::input_to_pdf_bytes(
                &bytes,
                options.filetype.as_deref(),
                Some(filename),
            )?;
            return Self::from_bytes(&pdf);
        }
        // A bare rect or size still opens an empty shell: pages arrive later.
        Ok(Self::create())
    }

    /// Length without emptiness: the spec has no empty check here.
    #[allow(clippy::len_without_is_empty)]
    /// Page count.
    pub fn len(&self) -> usize {
        self.page_count()
    }

    /// Unlock with a password.
    pub fn authenticate(&self, _password: &str) -> Result<i32> {
        let lopdf = self.lopdf();
        // No early return for unencrypted bytes: a file whose empty user
        // password opened it on load still verifies against the retained
        // state, and a never-encrypted file fails both checks below.
        let user = if lopdf.authenticate_user_password(_password).is_ok() {
            2
        } else {
            0
        };
        let owner = if lopdf.authenticate_owner_password(_password).is_ok() {
            4
        } else {
            0
        };
        Ok(user | owner)
    }

    /// Whether incremental saving applies.
    pub fn can_save_incrementally(&self) -> Result<bool> {
        Ok(!self.lopdf().is_encrypted())
    }

    /// Chapter count.
    pub fn chapter_count(&self) -> Result<usize> {
        Ok(1)
    }

    /// Pages in a chapter.
    pub fn chapter_page_count(&self, chapter: i32) -> Result<usize> {
        if chapter != 0 {
            return Err(ErrorKind::BadValue.to_error(format!("no chapter {chapter}")));
        }
        Ok(self.page_count())
    }

    /// Close the document.
    pub fn close(self) {}

    /// Convert a page span to PDF content.
    ///
    /// Returns the bytes of a new PDF holding pages `from_page..=to_page`
    /// (negative indices count back from the last page, as the recorded
    /// `to_page=-1` default does), each rotated by `rotate` degrees. The
    /// source handle is unchanged.
    pub fn convert_to_pdf(&self, from_page: i32, to_page: i32, rotate: i32) -> Result<Vec<u8>> {
        if rotate % 90 != 0 {
            return Err(ErrorKind::BadValue
                .to_error(format!("rotate must be a multiple of 90, got {rotate}")));
        }
        let count = self.page_count();
        let from = resolve_index(from_page, count)?;
        let to = resolve_index(to_page, count)?;
        if to < from {
            return Err(ErrorKind::BadValue
                .to_error(format!("to_page {to_page} is before from_page {from_page}")));
        }
        let pages: Vec<u32> = (from..=to).map(|page| page as u32 + 1).collect();
        let mut out = pdf_manip::pages::extract_pages(self.lopdf(), &pages).map_err(|e| {
            ErrorKind::UnreadableFile.to_error(format!("cannot extract pages: {e}"))
        })?;
        for index in 1..=pages.len() as u32 {
            pdf_manip::pages::rotate_page(&mut out, index, i64::from(rotate)).map_err(|e| {
                ErrorKind::UnreadableFile.to_error(format!("cannot rotate page: {e}"))
            })?;
        }
        let mut bytes = Vec::new();
        out.save_to(&mut bytes)
            .map_err(|source| crate::Error::Io { source, path: None })?;
        Ok(bytes)
    }

    /// Embedded-file count.
    pub fn embfile_count(&self) -> Result<usize> {
        Ok(self.attachments()?.len())
    }

    /// Embedded-file bytes.
    pub fn embfile_get(&self, item: &EmbItem) -> Result<Vec<u8>> {
        let name = self.embfile_name(item)?;
        self.attachment_bytes(&name)?
            .ok_or_else(|| ErrorKind::UnknownKey.to_error(format!("no embedded file {name:?}")))
    }

    /// Embedded-file facts.
    pub fn embfile_info(&self, item: &EmbItem) -> Result<EmbeddedFile> {
        let name = self.embfile_name(item)?;
        let size = self
            .attachments()?
            .into_iter()
            .find(|a| a.name == name)
            .map(|a| a.size)
            .unwrap_or(0);
        Ok(EmbeddedFile {
            name: name.clone(),
            filename: name,
            description: String::new(),
            size,
        })
    }

    /// Embedded-file names.
    pub fn embfile_names(&self) -> Result<Vec<String>> {
        Ok(self.attachments()?.into_iter().map(|a| a.name).collect())
    }

    /// Resolve an embedded-file handle to its name.
    fn embfile_name(&self, item: &EmbItem) -> Result<String> {
        let names = self.embfile_names()?;
        match item {
            EmbItem::Name(name) => {
                if names.iter().any(|n| n == name) {
                    Ok(name.clone())
                } else {
                    Err(ErrorKind::UnknownKey.to_error(format!("no embedded file {name:?}")))
                }
            }
            EmbItem::Index(index) => {
                let target = resolve_index(*index, names.len())?;
                Ok(names[target].clone())
            }
        }
    }

    /// Save with cleanup defaults.
    pub fn ez_save(&self, path: &str, options: &SaveSettings) -> Result<()> {
        self.save_full(path, options)
    }

    /// Position behind a bookmark.
    pub fn find_bookmark(&self, mark: i64) -> Result<Location> {
        if mark < 0 || mark as usize >= self.page_count() {
            return Err(ErrorKind::BadValue.to_error(format!("bad bookmark {mark}")));
        }
        Ok(Location {
            chapter: 0,
            page: mark as i32,
        })
    }

    /// Optional-content group by number.
    pub fn get_layer(&self, config: i32) -> Result<Ocg> {
        let layers = self.get_layers()?;
        let target = resolve_index(config, layers.len())?;
        Ok(layers[target].clone())
    }

    /// Optional-content groups.
    pub fn get_layers(&self) -> Result<Vec<Ocg>> {
        self.get_ocgs()
    }

    /// Reserve a fresh object number.
    pub fn get_new_xref(&mut self) -> Result<i32> {
        let mut scratch = self.lopdf().clone();
        let fresh = scratch.max_id + 1;
        scratch.objects.insert((fresh, 0), lopdf::Object::Null);
        scratch.max_id = fresh;
        *self = Self::from_lopdf(scratch)?;
        Ok(fresh as i32)
    }

    /// Optional-content state of an object.
    pub fn get_oc(&self, xref: i32) -> Result<i32> {
        let obj = object_by_xref(self, xref)?.clone();
        let dict = match &obj {
            lopdf::Object::Dictionary(dict) => Some(dict.clone()),
            lopdf::Object::Stream(stream) => Some(stream.dict.clone()),
            _ => None,
        };
        let Some(dict) = dict else {
            return Ok(0);
        };
        Ok(i32::from(dict.get(b"OC").is_ok()))
    }

    /// Optional-content groups.
    pub fn get_ocgs(&self) -> Result<Vec<Ocg>> {
        let lopdf = self.lopdf();
        let Some(root) = catalog_id(self) else {
            return Ok(Vec::new());
        };
        let props = lopdf
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"OCProperties").ok()?.as_reference().ok())
            .and_then(|id| lopdf.get_dictionary(id).ok().cloned());
        let Some(props) = props else {
            return Ok(Vec::new());
        };
        let groups = props
            .get(b"OCGs")
            .ok()
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default();
        let off: Vec<lopdf::ObjectId> = props
            .get(b"D")
            .ok()
            .and_then(|d| match d {
                lopdf::Object::Dictionary(dict) => dict.get(b"OFF").ok().cloned(),
                lopdf::Object::Reference(id) => {
                    lopdf.get_dictionary(*id).ok()?.get(b"OFF").ok().cloned()
                }
                _ => None,
            })
            .and_then(|o| o.as_array().ok().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|o| o.as_reference().ok())
            .collect();
        let mut out = Vec::new();
        for group in &groups {
            let Ok(id) = group.as_reference() else {
                continue;
            };
            let name = lopdf
                .get_dictionary(id)
                .ok()
                .and_then(|d| d.get(b"Name").ok().cloned())
                .and_then(|o| match o {
                    lopdf::Object::String(bytes, _) => {
                        Some(String::from_utf8_lossy(&bytes).into_owned())
                    }
                    _ => None,
                })
                .unwrap_or_default();
            out.push(Ocg {
                name,
                on: !off.contains(&id),
            });
        }
        Ok(out)
    }

    /// Membership dict of an object.
    pub fn get_ocmd(&self, xref: i32) -> Result<OcmdInfo> {
        let obj = object_by_xref(self, xref)?.clone();
        let dict = match &obj {
            lopdf::Object::Dictionary(dict) => Some(dict.clone()),
            lopdf::Object::Stream(stream) => Some(stream.dict.clone()),
            _ => None,
        };
        let Some(dict) = dict else {
            return Err(ErrorKind::BadValue.to_error(format!("xref {xref} holds no dict")));
        };
        let policy = match dict.get(b"P").ok() {
            Some(lopdf::Object::Name(name)) => String::from_utf8_lossy(name).into_owned(),
            _ => "AnyOn".to_string(),
        };
        let mut members = Vec::new();
        if let Ok(lopdf::Object::Array(items)) = dict.get(b"OCGs") {
            for item in items {
                if let Ok(id) = item.as_reference() {
                    members.push(xref_of(id));
                }
            }
        }
        Ok(OcmdInfo { policy, members })
    }

    /// Object numbers of the outline.
    pub fn get_outline_xrefs(&self) -> Result<Vec<i32>> {
        let lopdf = self.lopdf();
        let Some(root) = catalog_id(self) else {
            return Ok(Vec::new());
        };
        let first = lopdf
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"Outlines").ok()?.as_reference().ok())
            .and_then(|id| {
                lopdf
                    .get_dictionary(id)
                    .ok()?
                    .get(b"First")
                    .ok()?
                    .as_reference()
                    .ok()
            });
        let Some(first) = first else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        outline_ids(lopdf, first, &mut out);
        Ok(out)
    }

    /// Page label runs.
    pub fn get_page_labels(&self) -> Result<Vec<PageLabel>> {
        let lopdf = self.lopdf();
        let Some(root) = catalog_id(self) else {
            return Ok(Vec::new());
        };
        let nums = lopdf
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"PageLabels").ok().cloned())
            .and_then(|obj| match obj {
                lopdf::Object::Dictionary(dict) => Some(dict.clone()),
                lopdf::Object::Reference(id) => lopdf.get_dictionary(id).ok().cloned(),
                _ => None,
            })
            .and_then(|dict| dict.get(b"Nums").ok().cloned());
        let Some(lopdf::Object::Array(nums)) = nums else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for pair in nums.chunks(2) {
            let [start, entry] = pair else { continue };
            let Ok(start_page) = start.as_i64() else {
                continue;
            };
            let dict = match entry {
                lopdf::Object::Dictionary(dict) => Some(dict.clone()),
                lopdf::Object::Reference(id) => lopdf.get_dictionary(*id).ok().cloned(),
                _ => None,
            };
            let Some(dict) = dict else { continue };
            let style = match dict.get(b"S").ok() {
                Some(lopdf::Object::Name(name)) => String::from_utf8_lossy(name).into_owned(),
                _ => "D".to_string(),
            };
            let prefix = match dict.get(b"P").ok() {
                Some(lopdf::Object::String(bytes, _)) => {
                    String::from_utf8_lossy(bytes).into_owned()
                }
                _ => String::new(),
            };
            let first_number = dict
                .get(b"St")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(1) as i32;
            out.push(PageLabel {
                start_page: start_page as i32,
                prefix,
                first_number,
                style,
            });
        }
        Ok(out)
    }

    /// Pages carrying a label.
    pub fn get_page_numbers(&self, label: &str, only_one: bool) -> Result<Vec<i32>> {
        let mut out = Vec::new();
        for (index, shown) in self.page_labels().into_iter().enumerate() {
            if shown == label {
                out.push(index as i32);
                if only_one {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// External objects on a page.
    pub fn get_page_xobjects(&self, page: i32) -> Result<Vec<i32>> {
        let target = resolve_index(page, self.page_count())?;
        page_xobject_ids(self, target)
    }

    /// Signature flags.
    pub fn get_sigflags(&self) -> Result<i32> {
        Ok(acroform_dict(self)
            .as_ref()
            .and_then(|form| form.get(b"SigFlags").ok()?.as_i64().ok())
            .unwrap_or(0) as i32)
    }

    /// Outline entries.
    pub fn get_toc(&self, simple: bool) -> Result<Vec<TocEntry>> {
        if simple {
            let mut out = Vec::new();
            flatten_toc(&self.outlines()?, 1, &mut out);
            return Ok(out);
        }
        let entries = pdf_manip::bookmarks::read_outline(self.lopdf()).unwrap_or_default();
        let mut out = Vec::new();
        flatten_full_toc(self, &entries, 1, &mut out);
        Ok(out)
    }

    /// XML metadata packet.
    pub fn get_xml_metadata(&self) -> Result<String> {
        let lopdf = self.lopdf();
        let Some(root) = catalog_id(self) else {
            return Ok(String::new());
        };
        let stream_id = lopdf
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"Metadata").ok()?.as_reference().ok());
        let Some(stream_id) = stream_id else {
            return Ok(String::new());
        };
        let bytes = lopdf
            .get_object(stream_id)
            .ok()
            .and_then(|o| o.as_stream().ok())
            .and_then(|s| s.decompressed_content().ok())
            .unwrap_or_default();
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Finish opening a fresh document.
    pub fn init_doc(&mut self) -> Result<()> {
        Ok(())
    }

    /// Whether unsaved changes exist.
    pub fn is_dirty(&self) -> Result<bool> {
        // The facade tracks no dirty flag; the bindings keep their own.
        Ok(false)
    }

    /// Whether linearised for fast web view.
    pub fn is_fast_webaccess(&self) -> Result<bool> {
        let linearised = self
            .lopdf()
            .objects
            .get(&(1, 0))
            .and_then(|o| o.as_dict().ok())
            .is_some_and(|dict| dict.get(b"Linearized").is_ok());
        Ok(linearised)
    }

    /// Whether the document holds a form.
    pub fn is_form_pdf(&self) -> Result<bool> {
        let has_fields = acroform_dict(self)
            .as_ref()
            .and_then(|form| form.get(b"Fields").ok())
            .and_then(|o| o.as_array().ok())
            .is_some_and(|fields| !fields.is_empty());
        Ok(has_fields)
    }

    /// Whether the document is a PDF.
    pub fn is_pdf(&self) -> Result<bool> {
        Ok(true)
    }

    /// Whether the document reflows.
    pub fn is_reflowable(&self) -> Result<bool> {
        Ok(false)
    }

    /// Whether the document needed repair.
    pub fn is_repaired(&self) -> Result<bool> {
        Ok(false)
    }

    /// Whether an object is a stream.
    pub fn is_stream(&self, xref: i32) -> Result<bool> {
        Ok(object_by_xref(self, xref)
            .ok()
            .is_some_and(|o| matches!(o, lopdf::Object::Stream(_))))
    }

    /// Document language.
    pub fn language(&self) -> Result<String> {
        Ok(catalog_text(self, b"Lang"))
    }

    /// Last chapter position.
    pub fn last_location(&self) -> Result<Location> {
        Ok(Location {
            chapter: 0,
            page: self.page_count().saturating_sub(1) as i32,
        })
    }

    /// Layer interface configurations.
    ///
    /// One entry per `/Order` node of the default configuration: optional
    /// groups read as checkboxes (radioboxes inside an `/RBGroups` set),
    /// bare strings as labels. Groups added without an order entry (see
    /// `add_layer`) have no interface row, as the recording pins.
    pub fn layer_ui_configs(&self) -> Result<Vec<Value>> {
        Ok(layer_ui_entries(self))
    }

    /// Reflow the document to a box.
    pub fn layout(
        &mut self,
        _rect: Option<Rect>,
        _width: f64,
        _height: f64,
        _fontsize: f64,
    ) -> Result<()> {
        Err(super::not_supported("Document", "layout"))
    }

    /// Page by number.
    pub fn load_page(&self, number: i32) -> Result<Page<'_>> {
        self.at(number)
    }

    /// Chapter position of a page.
    pub fn location_from_page_number(&self, page: i32) -> Result<Location> {
        let target = resolve_index(page, self.page_count())?;
        Ok(Location {
            chapter: 0,
            page: target as i32,
        })
    }

    /// Bookmark for a chapter position.
    pub fn make_bookmark(&self, location: Location) -> Result<i64> {
        if location.chapter != 0 || location.page < 0 || location.page as usize >= self.page_count()
        {
            return Err(ErrorKind::BadValue.to_error(format!(
                "bad location {}/{}",
                location.chapter, location.page
            )));
        }
        Ok(location.page as i64)
    }

    /// Marked-content info.
    pub fn markinfo(&self) -> Result<Value> {
        let lopdf = self.lopdf();
        let Some(root) = catalog_id(self) else {
            return Ok(Value::Dict(Default::default()));
        };
        let dict = lopdf
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"MarkInfo").ok().cloned())
            .and_then(|obj| match obj {
                lopdf::Object::Dictionary(dict) => Some(dict.clone()),
                lopdf::Object::Reference(id) => lopdf.get_dictionary(id).ok().cloned(),
                _ => None,
            });
        let mut map = std::collections::HashMap::new();
        if let Some(dict) = dict {
            for key in [b"Marked".as_slice(), b"UserProperties", b"Suspects"] {
                if let Ok(value) = dict.get(key) {
                    let name = String::from_utf8_lossy(key).into_owned();
                    let entry = match value {
                        lopdf::Object::Boolean(b) => Value::Bool(*b),
                        other => Value::Str(object_source(other, false)),
                    };
                    map.insert(name, entry);
                }
            }
        }
        Ok(Value::Dict(map))
    }

    /// Document name.
    pub fn name(&self) -> Result<String> {
        // The facade keeps no path; the bindings carry the name.
        Ok(String::new())
    }

    /// Whether appearances render on view.
    pub fn need_appearances(&self) -> Result<bool> {
        Ok(acroform_dict(self)
            .as_ref()
            .and_then(|form| form.get(b"NeedAppearances").ok())
            .is_some_and(|o| matches!(o, lopdf::Object::Boolean(true))))
    }

    /// Password state code.
    pub fn needs_pass(&self) -> Result<i32> {
        let lopdf = self.lopdf();
        if !lopdf.is_encrypted() {
            return Ok(0);
        }
        // An empty user password opens the file on its own, so no password
        // is needed; anything else asks for one.
        let open = lopdf.authenticate_user_password("").is_ok();
        Ok(i32::from(!open))
    }

    /// Chapter position after this one.
    pub fn next_location(&self, location: Location) -> Result<Location> {
        let count = self.page_count() as i32;
        if location.chapter != 0 || location.page < 0 || location.page + 1 >= count {
            return Err(ErrorKind::IndexOutOfRange.to_error("no next location".to_string()));
        }
        Ok(Location {
            chapter: 0,
            page: location.page + 1,
        })
    }

    /// Full outline entries.
    pub fn outline(&self) -> Result<Vec<TocEntry>> {
        self.get_toc(false)
    }

    /// Page and chaptered-page counts.
    pub fn page_count2(&self) -> Result<(usize, usize)> {
        let count = self.page_count();
        Ok((count, count))
    }

    /// Crop box of a page.
    pub fn page_cropbox(&self, page: i32) -> Result<Rect> {
        let target = resolve_index(page, self.page_count())?;
        page_rect(self, target)
    }

    /// Page number of a chapter position.
    pub fn page_number_from_location(&self, location: Location) -> Result<i32> {
        if location.chapter != 0 || location.page < 0 || location.page as usize >= self.page_count()
        {
            return Err(ErrorKind::BadValue.to_error(format!(
                "bad location {}/{}",
                location.chapter, location.page
            )));
        }
        Ok(location.page)
    }

    /// Object number of a page.
    pub fn page_xref(&self, page: i32) -> Result<i32> {
        let target = resolve_index(page, self.page_count())?;
        Ok(xref_of(page_ids(self)[target]))
    }

    /// Page layout name.
    pub fn pagelayout(&self) -> Result<String> {
        Ok(catalog_text(self, b"PageLayout"))
    }

    /// Page mode name.
    pub fn pagemode(&self) -> Result<String> {
        Ok(catalog_text(self, b"PageMode"))
    }

    /// Catalog source.
    pub fn pdf_catalog(&self) -> Result<String> {
        let Some(id) = catalog_id(self) else {
            return Ok(String::new());
        };
        let dict = self
            .lopdf()
            .get_dictionary(id)
            .map_err(|_| ErrorKind::BadValue.to_error("no catalog".to_string()))?;
        Ok(object_source(
            &lopdf::Object::Dictionary(dict.clone()),
            false,
        ))
    }

    /// Trailer source.
    pub fn pdf_trailer(&self, _compressed: bool, ascii: bool) -> Result<String> {
        Ok(object_source(
            &lopdf::Object::Dictionary(self.lopdf().trailer.clone()),
            ascii,
        ))
    }

    /// Permission bits.
    pub fn permissions(&self) -> Result<i32> {
        Ok(permission_bits(self))
    }

    /// Chapter position before this one.
    pub fn prev_location(&self, location: Location) -> Result<Location> {
        if location.chapter != 0 || location.page <= 0 {
            return Err(ErrorKind::IndexOutOfRange.to_error("no previous location".to_string()));
        }
        Ok(Location {
            chapter: 0,
            page: location.page - 1,
        })
    }

    /// Recolor grey content.
    pub fn recolor(&mut self, _components: i32) -> Result<()> {
        Err(ErrorKind::EngineFault.to_error("recolor is not supported".to_string()))
    }

    /// Fresh view of a page.
    pub fn reload_page(&self, page: Page<'_>) -> Result<Page<'_>> {
        self.at(page.index() as i32)
    }

    /// Resolve named destinations.
    pub fn resolve_names(&mut self) -> Result<()> {
        // Destinations resolve on read; there is nothing to precompute.
        Ok(())
    }

    /// Save to a path with full options.
    pub fn save_full(&self, path: &str, options: &SaveSettings) -> Result<()> {
        let bytes = rendered_bytes(self, options)?;
        std::fs::write(path, bytes).map_err(|source| crate::Error::Io {
            source,
            path: Some(path.into()),
        })?;
        Ok(())
    }

    /// Append unsaved changes.
    pub fn save_incr(&self) -> Result<()> {
        Err(ErrorKind::BadValue.to_error("no backing file to append to".to_string()))
    }

    /// Save a compact snapshot.
    pub fn save_snapshot(&self, path: &str) -> Result<()> {
        self.save_full(path, &SaveSettings::default())
    }

    /// Switch the layer configuration.
    pub fn switch_layer(&mut self, config: i32, _as_default: bool) -> Result<()> {
        let lopdf = self.lopdf_mut();
        let root = lopdf
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or_else(|| ErrorKind::BadValue.to_error("no catalog".to_string()))?;
        let props_id = lopdf
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"OCProperties").ok()?.as_reference().ok())
            .ok_or_else(|| ErrorKind::BadValue.to_error("no optional content".to_string()))?;
        let configs = lopdf
            .get_dictionary(props_id)
            .ok()
            .and_then(|props| props.get(b"Configs").ok()?.as_array().ok().cloned())
            .unwrap_or_default();
        let target = resolve_index(config, configs.len())?;
        let chosen = configs[target].clone();
        let props = lopdf
            .get_dictionary_mut(props_id)
            .map_err(|_| ErrorKind::BadValue.to_error("no optional content".to_string()))?;
        props.set(b"D", chosen);
        Ok(())
    }

    /// Opaque identity token.
    pub fn this(&self) -> usize {
        self as *const Self as usize
    }

    /// Whether the handle is a PDF.
    pub fn this_is_pdf(&self) -> Result<bool> {
        Ok(true)
    }

    /// Save to bytes.
    pub fn tobytes(&self, options: &SaveSettings) -> Result<Vec<u8>> {
        rendered_bytes(self, options)
    }

    /// Incremental version count.
    pub fn version_count(&self) -> Result<usize> {
        Ok(1)
    }

    /// Write to bytes.
    pub fn write(&self, options: &SaveSettings) -> Result<Vec<u8>> {
        rendered_bytes(self, options)
    }

    /// Highest object number.
    pub fn xref(&self) -> Result<i32> {
        Ok(self.lopdf().max_id as i32)
    }

    /// Type and value of an object key.
    pub fn xref_get_key(&self, xref: i32, key: &str) -> Result<(String, String)> {
        let obj = object_by_xref(self, xref)?.clone();
        let dict = match &obj {
            lopdf::Object::Dictionary(dict) => Some(dict.clone()),
            lopdf::Object::Stream(stream) => Some(stream.dict.clone()),
            _ => None,
        };
        let Some(dict) = dict else {
            return Err(ErrorKind::BadValue.to_error(format!("xref {xref} holds no dict")));
        };
        let value = dict
            .get(key.as_bytes())
            .ok()
            .cloned()
            .ok_or_else(|| ErrorKind::UnknownKey.to_error(format!("no key {key:?}")))?;
        Ok((
            object_type_name(&value).to_string(),
            object_source(&value, false),
        ))
    }

    /// Keys of an object.
    pub fn xref_get_keys(&self, xref: i32) -> Result<Vec<String>> {
        let obj = object_by_xref(self, xref)?.clone();
        let dict = match &obj {
            lopdf::Object::Dictionary(dict) => Some(dict.clone()),
            lopdf::Object::Stream(stream) => Some(stream.dict.clone()),
            _ => None,
        };
        let Some(dict) = dict else {
            return Err(ErrorKind::BadValue.to_error(format!("xref {xref} holds no dict")));
        };
        Ok(dict
            .iter()
            .map(|(key, _)| String::from_utf8_lossy(key).into_owned())
            .collect())
    }

    /// Whether an object is a stream.
    pub fn xref_is_stream(&self, xref: i32) -> Result<bool> {
        self.is_stream(xref)
    }

    /// Whether an object is an external object.
    pub fn xref_is_xobject(&self, xref: i32) -> Result<bool> {
        let obj = object_by_xref(self, xref)?.clone();
        let lopdf::Object::Stream(stream) = &obj else {
            return Ok(false);
        };
        Ok(matches!(
            stream.dict.get(b"Subtype").ok(),
            Some(lopdf::Object::Name(name))
                if name.as_slice() == b"Image"
                    || name.as_slice() == b"Form"
                    || name.as_slice() == b"PS"
        ))
    }

    /// Object count.
    pub fn xref_length(&self) -> Result<usize> {
        Ok(self.lopdf().objects.len())
    }

    /// Source of an object.
    pub fn xref_object(&self, xref: i32, _compressed: bool, ascii: bool) -> Result<String> {
        Ok(object_source(object_by_xref(self, xref)?, ascii))
    }

    /// Decoded stream bytes.
    pub fn xref_stream(&self, xref: i32) -> Result<Vec<u8>> {
        let obj = object_by_xref(self, xref)?.clone();
        let lopdf::Object::Stream(stream) = &obj else {
            return Err(ErrorKind::BadValue.to_error(format!("xref {xref} is no stream")));
        };
        stream
            .decompressed_content()
            .map_err(|_| ErrorKind::EngineFault.to_error(format!("cannot decode xref {xref}")))
    }

    /// Raw stream bytes.
    pub fn xref_stream_raw(&self, xref: i32) -> Result<Vec<u8>> {
        let obj = object_by_xref(self, xref)?.clone();
        let lopdf::Object::Stream(stream) = &obj else {
            return Err(ErrorKind::BadValue.to_error(format!("xref {xref} is no stream")));
        };
        Ok(stream.content.clone())
    }

    /// Object number of the XML metadata.
    pub fn xref_xml_metadata(&self) -> Result<i32> {
        let lopdf = self.lopdf();
        let Some(root) = catalog_id(self) else {
            return Ok(0);
        };
        Ok(lopdf
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"Metadata").ok()?.as_reference().ok())
            .map(xref_of)
            .unwrap_or(0))
    }

    /// Render appearances on view.
    pub fn set_need_appearances(&mut self, value: bool) -> Result<()> {
        let lopdf = self.lopdf_mut();
        let root = lopdf
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or_else(|| ErrorKind::BadValue.to_error("no catalog".to_string()))?;
        let form_id = lopdf
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"AcroForm").ok()?.as_reference().ok());
        if let Some(form_id) = form_id {
            let form = lopdf
                .get_dictionary_mut(form_id)
                .map_err(|_| ErrorKind::BadValue.to_error("no form".to_string()))?;
            form.set(b"NeedAppearances", lopdf::Object::Boolean(value));
        } else {
            let fresh = lopdf.max_id + 1;
            let mut form = lopdf::Dictionary::new();
            form.set(b"NeedAppearances", lopdf::Object::Boolean(value));
            lopdf
                .objects
                .insert((fresh, 0), lopdf::Object::Dictionary(form));
            lopdf.max_id = fresh;
            let catalog = lopdf
                .get_dictionary_mut(root)
                .map_err(|_| ErrorKind::BadValue.to_error("no catalog".to_string()))?;
            catalog.set(b"AcroForm", lopdf::Object::Reference((fresh, 0)));
        }
        Ok(())
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        Err(ErrorKind::BadValue.to_error("documents do not order".to_string()))
    }
}

/// Content stream object ids of a page, in `/Contents` order.
fn content_ids(doc: &PdfDocument, index: usize) -> Result<Vec<lopdf::ObjectId>> {
    let ids = page_ids(doc);
    let id = *ids.get(index).ok_or_else(|| {
        ErrorKind::IndexOutOfRange.to_error(format!("page index {index} out of range"))
    })?;
    let contents = inherited(doc.lopdf(), id, b"Contents");
    let mut out = Vec::new();
    match contents {
        Some(lopdf::Object::Reference(ref_id)) => out.push(ref_id),
        Some(lopdf::Object::Array(items)) => {
            for item in &items {
                if let Ok(ref_id) = item.as_reference() {
                    out.push(ref_id);
                }
            }
        }
        _ => {}
    }
    Ok(out)
}

/// Clockwise rotation matrix in the y-down frame: right turns down.
fn clockwise_matrix(degrees: i32) -> Matrix {
    match degrees.rem_euclid(360) {
        90 => Matrix {
            a: 0.0,
            b: 1.0,
            c: -1.0,
            d: 0.0,
            e: 0.0,
            f: 0.0,
        },
        180 => Matrix {
            a: -1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            e: 0.0,
            f: 0.0,
        },
        270 => Matrix {
            a: 0.0,
            b: -1.0,
            c: 1.0,
            d: 0.0,
            e: 0.0,
            f: 0.0,
        },
        _ => Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        },
    }
}

impl<'a> Page<'a> {
    /// Art box.
    pub fn artbox(&self) -> Result<Rect> {
        named_page_box(self.document(), self.index(), b"ArtBox")
    }

    /// Bleed box.
    pub fn bleedbox(&self) -> Result<Rect> {
        named_page_box(self.document(), self.index(), b"BleedBox")
    }

    /// Content bounding box.
    pub fn bound(&self) -> Result<Rect> {
        // The lane has no content walker; the crop box is the outer bound.
        self.cropbox()
    }

    /// Crop box.
    pub fn cropbox(&self) -> Result<Rect> {
        named_page_box(self.document(), self.index(), b"CropBox")
    }

    /// Crop box offset.
    pub fn cropbox_position(&self) -> Result<Point> {
        let frames = page_frames(self.document(), self.index())?;
        Ok(Point {
            x: frames.crop[0].min(frames.crop[2]) - frames.media[0].min(frames.media[2]),
            y: frames.media[3].max(frames.media[1]) - frames.crop[3].max(frames.crop[1]),
        })
    }

    /// Matrix undoing the rotation.
    pub fn derotation_matrix(&self) -> Result<Matrix> {
        let rotation = page_frames(self.document(), self.index())?.rotation;
        Ok(clockwise_matrix(-rotation))
    }

    /// Box change log: line-metric boxes in content-stream order.
    ///
    /// A request for layers (any `layers` but none, false or an empty list)
    /// adds the layer name to every entry; the recorded default and empty
    /// list stay two-element.
    pub fn get_bboxlog(&self, layers: Option<&Value>) -> Result<Value> {
        let with_layer = match layers {
            None | Some(Value::Null) => false,
            Some(Value::Bool(found)) => *found,
            Some(Value::List(items)) => !items.is_empty(),
            Some(_) => true,
        };
        super::edit::bboxlog_entries(self.document(), self.index(), with_layer)
    }

    /// Content object numbers.
    pub fn get_contents(&self) -> Result<Vec<i32>> {
        Ok(content_ids(self.document(), self.index())?
            .into_iter()
            .map(xref_of)
            .collect())
    }

    /// Retained rendering of the page.
    ///
    /// The recording pins the page box with and without marks, so the
    /// flag is accepted and the answer is the box either way.
    pub fn get_displaylist(&self, _annots: bool) -> Result<DisplayList> {
        let _ = _annots;
        Ok(DisplayList {
            rect: page_rect(self.document(), self.index())?,
        })
    }

    /// Page label.
    pub fn get_label(&self) -> Result<String> {
        self.document()
            .page_labels()
            .get(self.index())
            .cloned()
            .ok_or_else(|| {
                ErrorKind::IndexOutOfRange
                    .to_error(format!("page index {} out of range", self.index()))
            })
    }

    /// Reflowed layout, if any.
    pub fn get_layout(&self, _options: &LayoutOptions) -> Result<Option<Value>> {
        Ok(None)
    }

    /// Optional-content items.
    pub fn get_oc_items(&self) -> Result<Vec<Value>> {
        Ok(Vec::new())
    }

    /// External object numbers.
    pub fn get_xobjects(&self) -> Result<Vec<i32>> {
        page_xobject_ids(self.document(), self.index())
    }

    /// Whether contents are wrapped.
    pub fn is_wrapped(&self) -> Result<bool> {
        Ok(false)
    }

    /// Page language.
    pub fn language(&self) -> Result<String> {
        let doc = self.document();
        let ids = page_ids(doc);
        let id = *ids.get(self.index()).ok_or_else(|| {
            ErrorKind::IndexOutOfRange.to_error(format!("page index {} out of range", self.index()))
        })?;
        let lang = inherited(doc.lopdf(), id, b"Lang").and_then(|obj| match obj {
            lopdf::Object::String(bytes, _) => Some(String::from_utf8_lossy(&bytes).into_owned()),
            _ => None,
        });
        Ok(lang.unwrap_or_default())
    }

    /// Media box.
    pub fn mediabox(&self) -> Result<Rect> {
        named_page_box(self.document(), self.index(), b"MediaBox")
    }

    /// Media box size.
    pub fn mediabox_size(&self) -> Result<(f64, f64)> {
        let media = self.mediabox()?;
        Ok(((media.x1 - media.x0).abs(), (media.y1 - media.y0).abs()))
    }

    /// Content stream bytes.
    pub fn read_contents(&self) -> Result<Vec<u8>> {
        let doc = self.document();
        let mut out = Vec::new();
        for id in content_ids(doc, self.index())? {
            let bytes = doc
                .lopdf()
                .get_object(id)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .and_then(|s| s.decompressed_content().ok())
                .unwrap_or_default();
            if !out.is_empty() {
                out.push(b'\n');
            }
            out.extend_from_slice(&bytes);
        }
        Ok(out)
    }

    /// Page rectangle.
    pub fn rect(&self) -> Result<Rect> {
        page_rect(self.document(), self.index())
    }

    /// Rotation in degrees.
    pub fn rotation(&self) -> Result<i32> {
        Ok(page_frames(self.document(), self.index())?.rotation)
    }

    /// Replay a retained rendering.
    pub fn run(&self, _display_list: &DisplayList, _matrix: Matrix) -> Result<()> {
        Err(ErrorKind::EngineFault.to_error("page replay is not supported".to_string()))
    }

    /// Matrix applying the rotation.
    pub fn rotation_matrix(&self) -> Result<Matrix> {
        let rotation = page_frames(self.document(), self.index())?.rotation;
        Ok(clockwise_matrix(rotation))
    }

    /// Page transformation matrix.
    pub fn transformation_matrix(&self) -> Result<Matrix> {
        // Map y-up PDF points onto the y-down lane frame, then rotate.
        let frames = page_frames(self.document(), self.index())?;
        let top = frames.crop[3].max(frames.crop[1]);
        let rot = clockwise_matrix(frames.rotation);
        // Product `rot * flip`: flip first, then rotate.
        Ok(Matrix {
            a: rot.a,
            b: rot.b,
            c: -rot.c,
            d: -rot.d,
            e: rot.c.mul_add(top, rot.e),
            f: rot.d.mul_add(top, rot.f),
        })
    }

    /// Trim box.
    pub fn trimbox(&self) -> Result<Rect> {
        named_page_box(self.document(), self.index(), b"TrimBox")
    }

    /// Page object number.
    pub fn xref(&self) -> Result<i32> {
        let ids = page_ids(self.document());
        ids.get(self.index()).copied().map(xref_of).ok_or_else(|| {
            ErrorKind::IndexOutOfRange.to_error(format!("page index {} out of range", self.index()))
        })
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        Err(ErrorKind::BadValue.to_error("pages do not order".to_string()))
    }
}

impl<'a> PageMut<'a> {
    /// Clip contents to a rectangle.
    pub fn clip_to_rect(&mut self, rect: Rect) -> Result<()> {
        // Prepend a rectangular clip path; coordinates arrive y-down and go
        // back to y-up for the content stream.
        let frames = page_frames(self.document(), self.index())?;
        let top = frames.crop[3].max(frames.crop[1]);
        let x = rect.x0.min(rect.x1);
        let w = (rect.x1 - rect.x0).abs();
        let y_up_bottom = top - rect.y1.max(rect.y0);
        let h = (rect.y1 - rect.y0).abs();
        let prefix = format!("q {x} {y_up_bottom} {w} {h} re W n\n");
        self.splice_contents(prefix.as_bytes(), b"Q\n")
    }

    /// Recolor grey content.
    pub fn recolor(&mut self, _components: i32) -> Result<()> {
        Err(ErrorKind::EngineFault.to_error("recolor is not supported".to_string()))
    }

    /// Reload the page view.
    pub fn refresh(&mut self) -> Result<()> {
        // Reads always come straight from the document; nothing is cached.
        Ok(())
    }

    /// Wrap contents for isolated drawing.
    pub fn wrap_contents(&mut self) -> Result<()> {
        self.splice_contents(b"q\n", b"Q\n")
    }

    /// Surround every content stream of the page with two byte runs.
    fn splice_contents(&mut self, prefix: &[u8], suffix: &[u8]) -> Result<()> {
        let index = self.index();
        let doc = self.document_mut();
        let ids = content_ids(doc, index)?;
        for id in ids {
            let stream = doc
                .lopdf_mut()
                .get_object_mut(id)
                .ok()
                .and_then(|o| o.as_stream_mut().ok())
                .ok_or_else(|| ErrorKind::BadValue.to_error("page content is no stream"))?;
            let mut content = stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone());
            let mut joined = Vec::with_capacity(prefix.len() + content.len() + suffix.len());
            joined.extend_from_slice(prefix);
            joined.append(&mut content);
            joined.extend_from_slice(suffix);
            stream.set_content(joined);
        }
        Ok(())
    }
}

impl Link {
    /// Blank link to fill and insert.
    pub fn new() -> Self {
        Self {
            rect: Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0,
            },
            kind: constants::LINK_NONE,
            uri: String::new(),
            xref: 0,
            id: String::new(),
            border: Border {
                width: 0.0,
                width_is_int: false,
                style: None,
                dashes: Vec::new(),
                clouds: 0,
            },
            colors: AnnotColors {
                stroke: Vec::new(),
                fill: Vec::new(),
            },
            flags: 0,
            dest: None,
            next: None,
        }
    }

    /// Border styling.
    pub fn border(&self) -> Border {
        self.border.clone()
    }

    /// Stroke and fill colours.
    pub fn colors(&self) -> AnnotColors {
        self.colors.clone()
    }

    /// Follow-on destination for local links.
    pub fn dest(&self) -> Option<LinkDest> {
        self.dest.clone()
    }

    /// Link flags bitfield.
    pub fn flags(&self) -> i32 {
        self.flags
    }

    /// Whether the link leaves the document.
    pub fn is_external(&self) -> bool {
        !matches!(self.kind, v if v == constants::LINK_NONE || v == constants::LINK_GOTO)
    }

    /// Next link on the page.
    pub fn next(&self) -> Option<Link> {
        self.next.as_deref().cloned()
    }

    /// Link rectangle.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// Set the border styling.
    pub fn set_border(&mut self, options: &BorderOptions) {
        self.border = Border {
            width: options.width,
            width_is_int: false,
            style: options.style.clone(),
            dashes: options.dashes.clone(),
            clouds: options.clouds,
        };
    }

    /// Set stroke and fill colours.
    pub fn set_colors(&mut self, colors: &AnnotColors) {
        self.colors = colors.clone();
    }

    /// Set the flags bitfield.
    pub fn set_flags(&mut self, flags: i32) {
        self.flags = flags;
    }

    /// Target URI for remote links.
    pub fn uri(&self) -> String {
        self.uri.clone()
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        Err(ErrorKind::BadValue.to_error("links do not order".to_string()))
    }
}

impl Default for Link {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for PdfDocument {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl<'a> PartialEq for Page<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.index() == other.index() && std::ptr::eq(self.document(), other.document())
    }
}
