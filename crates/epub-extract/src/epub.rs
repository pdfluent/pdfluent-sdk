//! Opening the archive: `container.xml`, the package document, the spine.
//!
//! An EPUB is a ZIP whose reading order lives in two indirections: `container.xml`
//! names the package document, and the package document's manifest names the
//! files while its spine orders them. Chapters are read in spine order, never
//! in archive order, and each keeps its spine position as its chapter number.
//! Only spine items that read as chapters (an HTML media type, or an HTML
//! file extension) are parsed; the cover image, the navigation document and
//! the stylesheets the spine may also carry are not text.
//!
//! Hrefs are resolved the way a reader resolves them: the fragment is dropped,
//! percent escapes decoded, and the path joined onto the package document's
//! directory with `.` and `..` collapsed. A chapter that resolves nowhere is
//! an error, not a skip: silently dropping a chapter would renumber every
//! chapter after it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use quick_xml::events::Event;
use quick_xml::Reader;
use zip::result::ZipError;
use zip::ZipArchive;

use crate::blocks::Block;
use crate::error::{EpubError, Result};
use crate::xhtml::{attr_value, parse_chapter};

/// Path of the container document in every EPUB.
pub(crate) const CONTAINER_PATH: &str = "META-INF/container.xml";

/// Largest single archive entry read into memory (16 MiB). A chapter past
/// this is a corrupt or hostile archive, and refusing it bounds memory to one
/// entry rather than the size of the ZIP.
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;

/// Extract every spine chapter in spine order, numbered from 0.
pub(crate) fn extract(epub: &[u8]) -> Result<Vec<Block>> {
    let mut archive = ZipArchive::new(Cursor::new(epub))?;
    let container = read_entry(&mut archive, CONTAINER_PATH)?
        .ok_or_else(|| EpubError::MissingFile(CONTAINER_PATH.to_string()))?;
    let package_path = rootfile_path(&container)?;
    let package_dir = parent_dir(&package_path);
    let package = read_entry(&mut archive, &package_path)?
        .ok_or_else(|| EpubError::MissingFile(package_path.clone()))?;
    let chapters = spine_hrefs(&package, &package_dir)?;
    let mut blocks = Vec::new();
    for (chapter, href) in chapters.iter().enumerate() {
        let text =
            read_entry(&mut archive, href)?.ok_or_else(|| EpubError::MissingFile(href.clone()))?;
        blocks.extend(parse_chapter(&text, chapter, href, blocks.len())?);
    }
    Ok(blocks)
}

/// Whether `bytes` look like an EPUB: a readable ZIP carrying either the
/// EPUB mimetype or the container document. A rough sniff for routing, not a
/// validation: callers that need the book still open it and read the error.
pub(crate) fn is_epub(bytes: &[u8]) -> bool {
    let mut archive = match ZipArchive::new(Cursor::new(bytes)) {
        Ok(archive) => archive,
        Err(_) => return false,
    };
    if let Ok(entry) = archive.by_name("mimetype") {
        let mut text = String::new();
        if entry
            .take(MAX_ENTRY_BYTES.saturating_add(1))
            .read_to_string(&mut text)
            .is_ok()
            && text.trim() == "application/epub+zip"
        {
            return true;
        }
    }
    let found = archive.by_name(CONTAINER_PATH).is_ok();
    found
}

/// The text of the archive entry `name`, or `None` when it is absent. Entries
/// past [`MAX_ENTRY_BYTES`] are refused rather than read.
pub(crate) fn read_entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<Option<String>> {
    let entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(ZipError::FileNotFound) => return Ok(None),
        Err(other) => return Err(other.into()),
    };
    let mut text = String::new();
    entry
        .take(MAX_ENTRY_BYTES.saturating_add(1))
        .read_to_string(&mut text)?;
    if text.len() as u64 > MAX_ENTRY_BYTES {
        return Err(EpubError::InvalidFormat(format!(
            "archive entry exceeds size limit: {name}"
        )));
    }
    Ok(Some(text))
}

/// The package document's archive path, from the first `rootfile` in
/// `container.xml`.
pub(crate) fn rootfile_path(container: &str) -> Result<String> {
    let mut reader = Reader::from_str(container);
    reader.config_mut().check_end_names = false;
    loop {
        match reader.read_event()? {
            Event::Start(event) | Event::Empty(event) => {
                if event.local_name().as_ref() == b"rootfile" {
                    if let Some(path) = attr_value(&event, "full-path") {
                        return Ok(path);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Err(EpubError::InvalidFormat(
        "META-INF/container.xml names no rootfile".to_string(),
    ))
}

/// The archive paths of the spine's chapters, in spine order, resolved
/// against the package document's directory.
fn spine_hrefs(package: &str, package_dir: &str) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(package);
    reader.config_mut().check_end_names = false;
    let mut manifest: HashMap<String, (String, String)> = HashMap::new();
    let mut spine: Vec<String> = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(event) | Event::Empty(event) => {
                let name = event.local_name();
                if name.as_ref() == b"item" {
                    if let (Some(id), Some(href)) =
                        (attr_value(&event, "id"), attr_value(&event, "href"))
                    {
                        let media_type = attr_value(&event, "media-type").unwrap_or_default();
                        manifest.insert(id, (href, media_type));
                    }
                } else if name.as_ref() == b"itemref" {
                    if let Some(idref) = attr_value(&event, "idref") {
                        spine.push(idref);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if spine.is_empty() {
        return Err(EpubError::InvalidFormat(
            "package document has an empty spine".to_string(),
        ));
    }
    let mut chapters = Vec::with_capacity(spine.len());
    for idref in &spine {
        let (href, media_type) = manifest.get(idref).ok_or_else(|| {
            EpubError::InvalidFormat(format!("spine itemref without manifest entry: {idref}"))
        })?;
        if is_chapter(media_type, href) {
            chapters.push(resolve_href(package_dir, href));
        }
    }
    Ok(chapters)
}

/// Whether a spine item reads as a chapter: an HTML media type, or an HTML
/// file extension where the media type says nothing.
fn is_chapter(media_type: &str, href: &str) -> bool {
    if media_type.to_ascii_lowercase().contains("html") {
        return true;
    }
    let path = href
        .split(['?', '#'])
        .next()
        .unwrap_or(href)
        .to_ascii_lowercase();
    path.ends_with(".xhtml")
        || path.ends_with(".xht")
        || path.ends_with(".html")
        || path.ends_with(".htm")
        || path.ends_with(".xml")
}

/// A manifest href as an archive path: fragment dropped, percent escapes
/// decoded, joined onto the package document's directory.
pub(crate) fn resolve_href(package_dir: &str, href: &str) -> String {
    let path = href.split(['?', '#']).next().unwrap_or(href);
    let decoded = percent_decode(path);
    if package_dir.is_empty() {
        normalize_path(&decoded)
    } else {
        normalize_path(&format!("{package_dir}/{decoded}"))
    }
}

/// The directory part of an archive path, without the slash.
pub(crate) fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map_or(String::new(), |(dir, _)| dir.to_string())
}

/// Collapse `.` and `..` lexically. A `..` past the root is dropped: archive
/// paths have nowhere above the root to go.
fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

/// Decode `%XX` escapes, leaving malformed ones (a bare `%`, non-hex digits)
/// and non-UTF-8 results as written.
fn percent_decode(href: &str) -> String {
    let bytes = href.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let (Some(high), Some(low)) = (hex_at(bytes, i + 1), hex_at(bytes, i + 2)) {
                out.push(high * 16 + low);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| href.to_string())
}

fn hex_at(bytes: &[u8], at: usize) -> Option<u8> {
    match bytes.get(at) {
        Some(b'0'..=b'9') => Some(bytes[at] - b'0'),
        Some(b'a'..=b'f') => Some(bytes[at] - b'a' + 10),
        Some(b'A'..=b'F') => Some(bytes[at] - b'A' + 10),
        _ => None,
    }
}
