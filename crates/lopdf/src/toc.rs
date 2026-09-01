use indexmap::IndexMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::{Document, Error, Object, ObjectId, Outline, Result};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct TocType {
    pub level: usize,
    pub title: String,
    pub page: usize,
}

#[allow(dead_code)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Default)]
pub struct Toc {
    pub toc: Vec<TocType>,
    pub errors: Vec<String>,
}

impl Toc {
    pub fn new() -> Self {
        Toc {
            toc: Vec::new(),
            errors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Destination {
    map: IndexMap<Vec<u8>, Object>,
}

#[allow(dead_code)]
impl Destination {
    pub fn new(title: Object, page: Object, typ: Object) -> Self {
        let mut map = IndexMap::new();
        map.insert(b"Title".to_vec(), title);
        map.insert(b"Page".to_vec(), page);
        map.insert(b"Type".to_vec(), typ);
        Destination { map }
    }

    pub fn set(&mut self, key: Vec<u8>, value: Object) {
        self.map.insert(key, value);
    }

    pub fn title(&self) -> Option<&Object> {
        self.map.get(b"Title".as_slice())
    }

    pub fn page(&self) -> Option<&Object> {
        self.map.get(b"Page".as_slice())
    }
}

type OutlinePageIds = Vec<(Vec<u8>, ObjectId, usize)>;

fn setup_outline_page_ids<'a>(
    outlines: &'a Vec<Outline>, result: &mut OutlinePageIds, level: usize,
) -> Result<&'a Vec<Outline>> {
    for outline in outlines.iter() {
        match outline {
            Outline::Destination(destination) => {
                result.push((
                    destination.title()?.as_str()?.to_vec(),
                    destination.page()?.as_reference()?,
                    level,
                ));
            }
            Outline::SubOutlines(sub_outlines) => {
                setup_outline_page_ids(sub_outlines, result, level + 1)?;
            }
        }
    }
    Ok(outlines)
}

impl Document {
    fn setup_page_id_to_num(&self) -> IndexMap<(u32, u16), u32> {
        let mut result = IndexMap::new();
        for (page_num, page_id) in self.get_pages() {
            result.insert(page_id, page_num);
        }
        result
    }

    pub fn get_toc(&self) -> Result<Toc> {
        let mut toc: Toc = Toc {
            toc: Vec::new(),
            errors: Vec::new(),
        };
        let mut named_destinations = IndexMap::new();

        let Some(outlines) = self.get_outlines(None, None, &mut named_destinations)? else {
            return Err(Error::NoOutline);
        };

        let mut outline_page_ids = Vec::new();
        setup_outline_page_ids(&outlines, &mut outline_page_ids, 1)?;
        let page_id_to_page_numbers = self.setup_page_id_to_num();
        for (title, page_id, level) in outline_page_ids {
            if let Some(page_num) = page_id_to_page_numbers.get(&page_id) {
                let s;
                if title.len() < 2 {
                    s = String::from_utf8_lossy(&title).to_string();
                } else if title[0] == 0xfe && title[1] == 0xff {
                    if title.len() & 1 != 0 {
                        toc.errors
                            .push(format!("Title encoded UTF16_BE {title:?} has invalid length!"));
                        continue;
                    }
                    let t16: Vec<u16> = title
                        .chunks(2)
                        .skip(1)
                        .map(|x| ((x[0] as u16) << 8) | x[1] as u16)
                        .collect();
                    s = String::from_utf16_lossy(&t16);
                } else if title[0] == 0xff && title[1] == 0xfe {
                    if title.len() & 1 != 0 {
                        toc.errors
                            .push(format!("Title encoded UTF16_LE {title:?} has invalid length!"));
                        continue;
                    }
                    let t16: Vec<u16> = title
                        .chunks(2)
                        .skip(1)
                        .map(|x| ((x[1] as u16) << 8) | x[0] as u16)
                        .collect();
                    s = String::from_utf16_lossy(&t16);
                } else {
                    s = String::from_utf8_lossy(&title).to_string();
                }
                toc.toc.push(TocType {
                    level,
                    title: s,
                    page: *page_num as usize,
                });
            }
        }
        Ok(toc)
    }
}

#[cfg(not(feature = "async"))]
#[cfg(test)]
mod tests {
    use crate::{Document, Object, TocType};

    /// Build a document with `page_count` pages and an outline tree over them.
    ///
    /// Upstream's test for `get_toc` loads `assets/test.pdf`, a 38 MB fixture
    /// this fork does not ship. It came across in the 0.44.0 merge and has been
    /// failing ever since, unnoticed, because no gate runs this crate's tests.
    /// A document assembled here costs nothing to carry and, unlike the fixture,
    /// says in the test itself what shape is being parsed.
    /// Each entry is (title, page index, depth), depth 0 being top level.
    fn document_with_outline(titles: &[(&[u8], usize, usize)]) -> Document {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let page_ids: Vec<_> = (0..3)
            .map(|_| {
                doc.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                })
            })
            .collect();

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
                "Count" => page_ids.len() as i64,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );

        // Reserve every outline item's id first, so /Next and /First can point
        // forwards. Writing the chain backwards instead would work, but reads
        // as the opposite of the order the parser walks.
        let item_ids: Vec<_> = titles.iter().map(|_| doc.new_object_id()).collect();

        for (i, (title, page_index, depth)) in titles.iter().enumerate() {
            let mut item = dictionary! {
                "Title" => Object::String(title.to_vec(), crate::StringFormat::Literal),
                "Dest" => vec![
                    Object::Reference(page_ids[*page_index]),
                    Object::Name(b"XYZ".to_vec()),
                    Object::Null,
                    Object::Null,
                    Object::Null,
                ],
            };
            // /Next is the next sibling: the next entry at the same depth,
            // searching no further than the first entry that is shallower --
            // that one belongs to an enclosing parent. Linking to it instead is
            // what a first attempt at this helper did, and it silently reparents
            // the rest of the tree one level down.
            let sibling = titles[i + 1..]
                .iter()
                .position(|(_, _, d)| d <= depth)
                .filter(|off| titles[i + 1 + off].2 == *depth)
                .map(|off| item_ids[i + 1 + off]);
            if let Some(next) = sibling {
                item.set("Next", Object::Reference(next));
            }
            // /First is the first child, which in a well-formed tree is the very
            // next entry when it is one level deeper.
            if let Some((_, _, d)) = titles.get(i + 1)
                && *d == depth + 1
            {
                item.set("First", Object::Reference(item_ids[i + 1]));
            }
            doc.objects.insert(item_ids[i], Object::Dictionary(item));
        }

        let outlines_id = doc.add_object(dictionary! {
            "Type" => "Outlines",
            "First" => Object::Reference(item_ids[0]),
            "Last" => Object::Reference(*item_ids.last().unwrap()),
        });

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "Outlines" => outlines_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    #[test]
    fn a_nested_outline_becomes_a_table_of_contents() {
        let doc = document_with_outline(&[
            (b"1. Introduction", 0, 0),
            (b"1.1. Details", 1, 1),
            (b"2. The End", 2, 0),
        ]);

        assert_eq!(
            doc.get_toc().unwrap().toc,
            vec![
                TocType {
                    level: 1,
                    title: String::from("1. Introduction"),
                    page: 1,
                },
                // Nesting is what distinguishes this from a flat list, and it is
                // the only thing setup_outline_page_ids computes.
                TocType {
                    level: 2,
                    title: String::from("1.1. Details"),
                    page: 2,
                },
                TocType {
                    level: 1,
                    title: String::from("2. The End"),
                    page: 3,
                },
            ]
        );
    }

    #[test]
    fn a_utf16_be_title_is_decoded_rather_than_read_as_bytes() {
        // "Kapitel" with the byte-order mark PDF requires for UTF-16 text
        // strings. Read as Latin-1 this is nine characters of noise, so the
        // assertion fails if the BOM branch stops firing.
        let mut title = vec![0xfe, 0xff];
        for c in "Kapitel".encode_utf16() {
            title.extend_from_slice(&c.to_be_bytes());
        }
        let doc = document_with_outline(&[(&title, 0, 0)]);

        let toc = doc.get_toc().unwrap();
        assert_eq!(toc.toc.len(), 1);
        assert_eq!(toc.toc[0].title, "Kapitel");
        assert!(toc.errors.is_empty(), "{:?}", toc.errors);
    }

    #[test]
    fn a_truncated_utf16_title_is_reported_and_not_decoded() {
        // An odd byte count cannot be UTF-16. The parser records it and drops
        // the entry rather than reading one byte past the end.
        let title = vec![0xfe, 0xff, 0x00, 0x4b, 0x00];
        let doc = document_with_outline(&[(&title, 0, 0)]);

        let toc = doc.get_toc().unwrap();
        assert!(toc.toc.is_empty(), "the entry should have been dropped");
        assert_eq!(toc.errors.len(), 1);
    }
}
