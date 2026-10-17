//! Book metadata and navigation: title, author and the table of contents.
//!
//! [`extract_metadata`] reads the package document's Dublin Core title and
//! creator, the two fields a catalogue shows. [`extract_toc`] reads the
//! navigation the book carries, if any: the NCX map of an EPUB 2 book or the
//! navigation document of an EPUB 3 one, whichever the manifest names. A book
//! with neither has an empty table of contents, which is the common case for
//! single-chapter fixtures rather than an error.
//!
//! Hrefs resolve to archive paths the way chapter hrefs do, against the
//! navigation file's own directory.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::io::Cursor;

use quick_xml::events::Event;
use quick_xml::Reader;
use zip::ZipArchive;

use crate::epub::{parent_dir, read_entry, resolve_href, rootfile_path, CONTAINER_PATH};
use crate::error::{EpubError, Result};
use crate::xhtml::{attr_value, normalize, text_content};

/// The catalogue fields of a book: its Dublin Core title and creator.
///
/// `#[non_exhaustive]` like [`crate::Block`]: metadata keeps gaining fields
/// and reading code must not break when it does.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpubMetadata {
    /// The first `dc:title` in the package document, or empty when absent.
    pub title: String,
    /// The first `dc:creator`, read as the author, or empty when absent.
    pub author: String,
}

/// One entry of the book's table of contents, with nested entries beneath it.
///
/// `#[non_exhaustive]` like [`crate::Block`]: navigation keeps gaining
/// fields and reading code must not break when it does.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    /// The entry's label, whitespace normalised.
    pub title: String,
    /// The entry's target as an archive path, fragment dropped.
    pub href: String,
    /// Entries nested under this one, in document order.
    pub children: Vec<TocEntry>,
}

/// The title and author of an EPUB archive.
///
/// Reads `container.xml`, then the package document, then the first
/// `dc:title` and the first `dc:creator` inside its metadata. Either reads
/// empty when the book does not state it.
pub fn extract_metadata(epub: &[u8]) -> Result<EpubMetadata> {
    let (_, _, package) = read_package(epub)?;
    let mut reader = Reader::from_str(&package);
    reader.config_mut().check_end_names = false;
    let mut in_metadata = false;
    let mut capture: Option<&[u8]> = None;
    let mut text = String::new();
    let mut title = String::new();
    let mut author = String::new();
    let mut have_title = false;
    let mut have_author = false;
    loop {
        match reader.read_event()? {
            Event::Start(event) => {
                let name = event.local_name();
                if name.as_ref() == b"metadata" {
                    in_metadata = true;
                } else if in_metadata && capture.is_none() {
                    if name.as_ref() == b"title" && !have_title {
                        capture = Some(b"title");
                        text.clear();
                    } else if name.as_ref() == b"creator" && !have_author {
                        capture = Some(b"creator");
                        text.clear();
                    }
                }
            }
            Event::Empty(event) => {
                if event.local_name().as_ref() == b"metadata" {
                    in_metadata = true;
                }
            }
            Event::Text(event) => {
                if capture.is_some() {
                    text.push_str(&text_content(event));
                }
            }
            Event::CData(event) => {
                if capture.is_some() {
                    text.push_str(&String::from_utf8_lossy(&event.into_inner()));
                }
            }
            Event::End(event) => {
                let name = event.local_name();
                if name.as_ref() == b"metadata" {
                    in_metadata = false;
                } else if Some(name.as_ref()) == capture {
                    if name.as_ref() == b"title" {
                        title = normalize(&text);
                        have_title = true;
                    } else {
                        author = normalize(&text);
                        have_author = true;
                    }
                    capture = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(EpubMetadata { title, author })
}

/// The table of contents of an EPUB archive, in document order.
///
/// The NCX map wins where the manifest names one; otherwise the EPUB 3
/// navigation document (the manifest item with the `nav` property) is read;
/// otherwise the book has no navigation and the table is empty. Hrefs are
/// archive paths resolved against the navigation file's directory.
pub fn extract_toc(epub: &[u8]) -> Result<Vec<TocEntry>> {
    let (_, package_dir, package) = read_package(epub)?;
    let items = manifest_items(&package);
    if let Some(href) = items.iter().find_map(|item| {
        item.media_type
            .to_ascii_lowercase()
            .contains("ncx")
            .then(|| resolve_href(&package_dir, &item.href))
    }) {
        let mut archive = ZipArchive::new(Cursor::new(epub))?;
        let ncx =
            read_entry(&mut archive, &href)?.ok_or_else(|| EpubError::MissingFile(href.clone()))?;
        return toc_from_ncx(&ncx, &parent_dir(&href));
    }
    if let Some(href) = items.iter().find_map(|item| {
        item.properties
            .to_ascii_lowercase()
            .split_whitespace()
            .any(|word| word == "nav")
            .then(|| resolve_href(&package_dir, &item.href))
    }) {
        let mut archive = ZipArchive::new(Cursor::new(epub))?;
        let nav =
            read_entry(&mut archive, &href)?.ok_or_else(|| EpubError::MissingFile(href.clone()))?;
        return toc_from_nav(&nav, &parent_dir(&href));
    }
    Ok(Vec::new())
}

/// The package document's archive path, its directory, and its text.
fn read_package(epub: &[u8]) -> Result<(String, String, String)> {
    let mut archive = ZipArchive::new(Cursor::new(epub))?;
    let container = read_entry(&mut archive, CONTAINER_PATH)?
        .ok_or_else(|| EpubError::MissingFile(CONTAINER_PATH.to_string()))?;
    let package_path = rootfile_path(&container)?;
    let package = read_entry(&mut archive, &package_path)?
        .ok_or_else(|| EpubError::MissingFile(package_path.clone()))?;
    let package_dir = parent_dir(&package_path);
    Ok((package_path, package_dir, package))
}

/// One manifest item: where it is and what it claims to be.
struct ManifestItem {
    href: String,
    media_type: String,
    properties: String,
}

/// Every `item` in the package document's manifest.
fn manifest_items(package: &str) -> Vec<ManifestItem> {
    let mut reader = Reader::from_str(package);
    reader.config_mut().check_end_names = false;
    let mut items = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(event) | Event::Empty(event)) => {
                if event.local_name().as_ref() == b"item" {
                    if let Some(href) = attr_value(&event, "href") {
                        items.push(ManifestItem {
                            href,
                            media_type: attr_value(&event, "media-type").unwrap_or_default(),
                            properties: attr_value(&event, "properties").unwrap_or_default(),
                        });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    items
}

/// A `navPoint` frame: the label so far, its target, and finished children.
struct NavPoint {
    title: String,
    href: String,
    children: Vec<TocEntry>,
}

/// An XML element name against a lowercase want: navigation files mix
/// cases (`navPoint` in NCX, `li` in XHTML), so the match ignores case.
fn is_element(name: &[u8], want: &[u8]) -> bool {
    name.eq_ignore_ascii_case(want)
}

/// The NCX `navMap` as entries: nested `navPoint`s become nested entries,
/// each labelled by its `navLabel` and aimed at its `content` source.
fn toc_from_ncx(ncx: &str, nav_dir: &str) -> Result<Vec<TocEntry>> {
    let mut reader = Reader::from_str(ncx);
    reader.config_mut().check_end_names = false;
    let mut roots: Vec<TocEntry> = Vec::new();
    let mut stack: Vec<NavPoint> = Vec::new();
    let mut in_label = false;
    loop {
        match reader.read_event()? {
            Event::Start(event) => {
                let name = event.local_name();
                if is_element(name.as_ref(), b"navpoint") {
                    stack.push(NavPoint {
                        title: String::new(),
                        href: String::new(),
                        children: Vec::new(),
                    });
                } else if is_element(name.as_ref(), b"navlabel") {
                    in_label = true;
                } else if is_element(name.as_ref(), b"content") {
                    if let (Some(frame), Some(src)) = (stack.last_mut(), attr_value(&event, "src"))
                    {
                        frame.href = resolve_href(nav_dir, &src);
                    }
                }
            }
            Event::Empty(event) => {
                let name = event.local_name();
                if is_element(name.as_ref(), b"content") {
                    if let (Some(frame), Some(src)) = (stack.last_mut(), attr_value(&event, "src"))
                    {
                        frame.href = resolve_href(nav_dir, &src);
                    }
                }
            }
            Event::Text(event) => {
                if in_label {
                    if let Some(frame) = stack.last_mut() {
                        frame.title.push_str(&text_content(event));
                    }
                }
            }
            Event::End(event) => {
                let name = event.local_name();
                if is_element(name.as_ref(), b"navlabel") {
                    in_label = false;
                } else if is_element(name.as_ref(), b"navpoint") {
                    if let Some(frame) = stack.pop() {
                        let entry = TocEntry {
                            title: normalize(&frame.title),
                            href: frame.href,
                            children: frame.children,
                        };
                        match stack.last_mut() {
                            Some(parent) => parent.children.push(entry),
                            None => roots.push(entry),
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(roots)
}

/// An `li` frame: the link text and target, the direct text fallback, and
/// finished children from a nested list.
struct ListItem {
    link_title: String,
    link_href: String,
    direct: String,
    children: Vec<TocEntry>,
}

/// The first `nav` element's lists as entries: each `li` is labelled by its
/// link, or by its direct text where it has none, and a nested list becomes
/// nested entries. Items with neither a label nor a target are dropped.
fn toc_from_nav(nav: &str, nav_dir: &str) -> Result<Vec<TocEntry>> {
    let mut reader = Reader::from_str(nav);
    reader.config_mut().check_end_names = false;
    let mut roots: Vec<TocEntry> = Vec::new();
    let mut stack: Vec<ListItem> = Vec::new();
    let mut in_nav = false;
    let mut nav_done = false;
    let mut in_link = false;
    loop {
        match reader.read_event()? {
            Event::Start(event) => {
                let name = event.local_name();
                if is_element(name.as_ref(), b"nav") {
                    if !nav_done {
                        in_nav = true;
                    }
                } else if !in_nav {
                    continue;
                } else if is_element(name.as_ref(), b"li") {
                    stack.push(ListItem {
                        link_title: String::new(),
                        link_href: String::new(),
                        direct: String::new(),
                        children: Vec::new(),
                    });
                } else if is_element(name.as_ref(), b"a") {
                    in_link = true;
                    if let (Some(frame), Some(href)) =
                        (stack.last_mut(), attr_value(&event, "href"))
                    {
                        frame.link_href = resolve_href(nav_dir, &href);
                    }
                }
            }
            Event::Text(event) => {
                if !in_nav {
                    continue;
                }
                if let Some(frame) = stack.last_mut() {
                    let text = text_content(event);
                    if in_link {
                        frame.link_title.push_str(&text);
                    } else {
                        frame.direct.push_str(&text);
                    }
                }
            }
            Event::Empty(_) => {}
            Event::End(event) => {
                let name = event.local_name();
                if is_element(name.as_ref(), b"nav") {
                    in_nav = false;
                    nav_done = true;
                } else if is_element(name.as_ref(), b"a") {
                    in_link = false;
                } else if is_element(name.as_ref(), b"li") {
                    if let Some(frame) = stack.pop() {
                        let title = normalize(if frame.link_title.is_empty() {
                            &frame.direct
                        } else {
                            &frame.link_title
                        });
                        if title.is_empty() && frame.link_href.is_empty() {
                            continue;
                        }
                        let entry = TocEntry {
                            title,
                            href: frame.link_href,
                            children: frame.children,
                        };
                        match stack.last_mut() {
                            Some(parent) => parent.children.push(entry),
                            None => roots.push(entry),
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(roots)
}
