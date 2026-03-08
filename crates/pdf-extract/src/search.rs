//! Full-text search across PDF documents.
//!
//! Provides search functionality using extracted text and positioned characters.

use crate::text;

/// A search result with location information.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The page number (1-based) where the match was found.
    pub page: u32,
    /// The matched text.
    pub text: String,
    /// Bounding boxes of the matched characters.
    pub bounding_boxes: Vec<[f64; 4]>,
    /// Character offset within the page text.
    pub offset: usize,
}

/// Options for text search.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Whether to perform case-insensitive search.
    pub case_insensitive: bool,
    /// Maximum number of results to return (0 = unlimited).
    pub max_results: usize,
    /// Specific pages to search (empty = all pages).
    pub pages: Vec<u32>,
    /// Skip bounding box extraction for faster search when only text matches are needed.
    pub skip_bounding_boxes: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_insensitive: true,
            max_results: 0,
            pages: Vec::new(),
            skip_bounding_boxes: false,
        }
    }
}

/// Search for text across all pages of a document.
pub fn search_text(
    doc: &lopdf::Document,
    query: &str,
    options: &SearchOptions,
) -> Vec<SearchResult> {
    if query.is_empty() {
        return Vec::new();
    }

    let pages = doc.get_pages();
    let total = pages.len() as u32;
    let mut results = Vec::new();

    // Determine which pages to search.
    let page_nums: Vec<u32> = if options.pages.is_empty() {
        (1..=total).collect()
    } else {
        options
            .pages
            .iter()
            .copied()
            .filter(|&p| p >= 1 && p <= total)
            .collect()
    };

    let query_normalized = if options.case_insensitive {
        query.to_lowercase()
    } else {
        query.to_string()
    };

    // Process pages lazily — extract text (and optionally positioned chars)
    // one page at a time to avoid upfront cost on large documents.
    for page_num in &page_nums {
        let page_text = text::extract_page_text(doc, *page_num).unwrap_or_default();

        let haystack = if options.case_insensitive {
            page_text.to_lowercase()
        } else {
            page_text.clone()
        };

        // Only extract positioned chars when bounding boxes are actually needed.
        let positioned = if options.skip_bounding_boxes {
            Vec::new()
        } else {
            text::extract_positioned_chars(doc, *page_num).unwrap_or_default()
        };

        let mut start = 0;
        while let Some(pos) = haystack[start..].find(&query_normalized) {
            let offset = start + pos;
            let end = offset + query_normalized.len();

            let bboxes: Vec<[f64; 4]> = if options.skip_bounding_boxes {
                Vec::new()
            } else {
                positioned
                    .iter()
                    .skip(offset)
                    .take(end - offset)
                    .map(|c| c.bbox)
                    .collect()
            };

            let matched_text = page_text
                .chars()
                .skip(offset)
                .take(query_normalized.len())
                .collect::<String>();

            results.push(SearchResult {
                page: *page_num,
                text: matched_text,
                bounding_boxes: bboxes,
                offset,
            });

            if options.max_results > 0 && results.len() >= options.max_results {
                return results;
            }

            start = offset + 1;
        }
    }

    results
}

/// Count the total number of occurrences of a query across all pages.
pub fn count_occurrences(doc: &lopdf::Document, query: &str) -> usize {
    count_text_only(doc, query, &SearchOptions::default())
}

/// Fast text-only occurrence count — skips all bounding box extraction.
///
/// Accepts `SearchOptions` for page filtering and case sensitivity,
/// but ignores `max_results` (always counts all occurrences).
pub fn count_text_only(doc: &lopdf::Document, query: &str, options: &SearchOptions) -> usize {
    if query.is_empty() {
        return 0;
    }

    let pages = doc.get_pages();
    let total = pages.len() as u32;

    let page_nums: Vec<u32> = if options.pages.is_empty() {
        (1..=total).collect()
    } else {
        options
            .pages
            .iter()
            .copied()
            .filter(|&p| p >= 1 && p <= total)
            .collect()
    };

    let query_normalized = if options.case_insensitive {
        query.to_lowercase()
    } else {
        query.to_string()
    };

    let mut count = 0usize;
    for page_num in &page_nums {
        let page_text = text::extract_page_text(doc, *page_num).unwrap_or_default();
        let haystack = if options.case_insensitive {
            page_text.to_lowercase()
        } else {
            page_text
        };

        let mut start = 0;
        while let Some(pos) = haystack[start..].find(&query_normalized) {
            count += 1;
            start += pos + 1;
        }
    }

    count
}

/// Return a list of page numbers that contain the query text.
pub fn pages_containing(doc: &lopdf::Document, query: &str) -> Vec<u32> {
    if query.is_empty() {
        return Vec::new();
    }

    let pages = doc.get_pages();
    let total = pages.len() as u32;
    let query_lower = query.to_lowercase();
    let mut result = Vec::new();

    for page_num in 1..=total {
        let page_text = text::extract_page_text(doc, page_num).unwrap_or_default();
        if page_text.to_lowercase().contains(&query_lower) {
            result.push(page_num);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};

    /// Helper: create a doc with text content.
    fn make_doc_with_text(content: &[u8]) -> Document {
        let mut doc = Document::with_version("1.7");

        let content_stream = Stream::new(dictionary! {}, content.to_vec());
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    /// Helper: create a multi-page doc.
    fn make_multi_page_doc(contents: &[&[u8]]) -> Document {
        let mut doc = Document::with_version("1.7");
        let mut page_ids = Vec::new();

        for content in contents {
            let content_stream = Stream::new(dictionary! {}, content.to_vec());
            let content_id = doc.add_object(Object::Stream(content_stream));

            let page_dict = dictionary! {
                "Type" => "Page",
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => Object::Reference(content_id),
            };
            let page_id = doc.add_object(Object::Dictionary(page_dict));
            page_ids.push(page_id);
        }

        let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => Object::Integer(page_ids.len() as i64),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        for &page_id in &page_ids {
            if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
                d.set("Parent", Object::Reference(pages_id));
            }
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn search_single_page() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (Hello World) Tj ET");
        let options = SearchOptions::default();
        let results = search_text(&doc, "Hello", &options);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].page, 1);
        assert_eq!(results[0].text, "Hello");
    }

    #[test]
    fn search_case_insensitive() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (Hello World) Tj ET");
        let options = SearchOptions {
            case_insensitive: true,
            ..Default::default()
        };
        let results = search_text(&doc, "hello", &options);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "Hello");
    }

    #[test]
    fn search_case_sensitive() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (Hello World) Tj ET");
        let options = SearchOptions {
            case_insensitive: false,
            ..Default::default()
        };
        let results = search_text(&doc, "hello", &options);
        assert!(results.is_empty());
    }

    #[test]
    fn search_multiple_pages() {
        let doc = make_multi_page_doc(&[
            b"BT /F1 12 Tf (Hello) Tj ET",
            b"BT /F1 12 Tf (Hello again) Tj ET",
        ]);
        let options = SearchOptions::default();
        let results = search_text(&doc, "Hello", &options);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_specific_pages() {
        let doc = make_multi_page_doc(&[
            b"BT /F1 12 Tf (Hello) Tj ET",
            b"BT /F1 12 Tf (Hello again) Tj ET",
        ]);
        let options = SearchOptions {
            pages: vec![1],
            ..Default::default()
        };
        let results = search_text(&doc, "Hello", &options);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].page, 1);
    }

    #[test]
    fn search_max_results() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (aaa) Tj ET");
        let options = SearchOptions {
            max_results: 1,
            ..Default::default()
        };
        let results = search_text(&doc, "a", &options);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_empty_query() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (Hello) Tj ET");
        let options = SearchOptions::default();
        let results = search_text(&doc, "", &options);
        assert!(results.is_empty());
    }

    #[test]
    fn search_no_match() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (Hello) Tj ET");
        let options = SearchOptions::default();
        let results = search_text(&doc, "xyz", &options);
        assert!(results.is_empty());
    }

    #[test]
    fn count_occurrences_basic() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (abcabc) Tj ET");
        let count = count_occurrences(&doc, "abc");
        assert_eq!(count, 2);
    }

    #[test]
    fn pages_containing_basic() {
        let doc = make_multi_page_doc(&[
            b"BT /F1 12 Tf (Hello) Tj ET",
            b"BT /F1 12 Tf (World) Tj ET",
            b"BT /F1 12 Tf (Hello World) Tj ET",
        ]);
        let pages = pages_containing(&doc, "Hello");
        assert!(pages.contains(&1));
        assert!(!pages.contains(&2));
        assert!(pages.contains(&3));
    }

    #[test]
    fn search_results_have_bounding_boxes() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (Hello) Tj ET");
        let options = SearchOptions::default();
        let results = search_text(&doc, "Hello", &options);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].bounding_boxes.len(), 5); // 5 chars in "Hello"
    }
}
