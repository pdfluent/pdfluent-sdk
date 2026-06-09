//! Reading PDF page labels (ISO 32000-1 §12.4.2).
//!
//! A document may map physical page indices to display labels (e.g. front
//! matter as `i, ii, iii` followed by body pages `1, 2, 3`, or prefixed labels
//! like `A-1, A-2`). The mapping lives in the catalog's `/PageLabels` number
//! tree: each entry keys a 0-based page index at which a new labelling range
//! begins, pointing at a label dictionary (`/S` style, `/P` prefix, `/St`
//! start). This module resolves that tree into a per-page label string.

use lopdf::{Dictionary, Document, Object};

/// Numbering style of a page-label range (`/S` in the label dictionary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageLabelStyle {
    Decimal,      // /D  -> 1, 2, 3
    RomanUpper,   // /R  -> I, II, III
    RomanLower,   // /r  -> i, ii, iii
    LettersUpper, // /A  -> A, B, … Z, AA
    LettersLower, // /a  -> a, b, … z, aa
}

struct LabelRange {
    start_page: i64,
    style: Option<PageLabelStyle>,
    prefix: String,
    start_at: i64,
}

fn style_from_name(name: &[u8]) -> Option<PageLabelStyle> {
    match name {
        b"D" => Some(PageLabelStyle::Decimal),
        b"R" => Some(PageLabelStyle::RomanUpper),
        b"r" => Some(PageLabelStyle::RomanLower),
        b"A" => Some(PageLabelStyle::LettersUpper),
        b"a" => Some(PageLabelStyle::LettersLower),
        _ => None,
    }
}

/// Classic roman numeral for `n >= 1`; falls back to decimal for non-positive
/// values (roman numerals are undefined there).
fn to_roman(mut n: i64) -> String {
    if n <= 0 {
        return n.to_string();
    }
    const TABLE: &[(i64, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for &(value, sym) in TABLE {
        while n >= value {
            out.push_str(sym);
            n -= value;
        }
    }
    out
}

/// Alphabetic label using the PDF repeated-letter scheme (§12.4.2):
/// 1→A, 26→Z, 27→AA, 52→ZZ, 53→AAA. Falls back to decimal for `n < 1`.
fn to_alpha(n: i64) -> String {
    if n <= 0 {
        return n.to_string();
    }
    let count = ((n - 1) / 26) + 1;
    let letter = (b'A' + ((n - 1) % 26) as u8) as char;
    core::iter::repeat(letter).take(count as usize).collect()
}

fn format_label(range: &LabelRange, offset: i64) -> String {
    let number = range.start_at + offset;
    let mut out = range.prefix.clone();
    match range.style {
        Some(PageLabelStyle::Decimal) => out.push_str(&number.to_string()),
        Some(PageLabelStyle::RomanUpper) => out.push_str(&to_roman(number)),
        Some(PageLabelStyle::RomanLower) => out.push_str(&to_roman(number).to_lowercase()),
        Some(PageLabelStyle::LettersUpper) => out.push_str(&to_alpha(number)),
        Some(PageLabelStyle::LettersLower) => out.push_str(&to_alpha(number).to_lowercase()),
        // No /S: the label is the prefix alone (no numeric portion).
        None => {}
    }
    out
}

fn resolve_dict(doc: &Document, obj: &Object) -> Option<Dictionary> {
    match obj {
        Object::Dictionary(d) => Some(d.clone()),
        Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
        _ => None,
    }
}

/// Walk a `/PageLabels` number-tree node, collecting `(page_index, label_dict)`
/// leaves from `/Nums` and recursing into `/Kids`. Depth-guarded against cycles.
fn collect_nums(doc: &Document, node: &Dictionary, out: &mut Vec<(i64, Dictionary)>, depth: usize) {
    if depth > 64 {
        return;
    }
    if let Ok(Object::Array(nums)) = node.get_deref(b"Nums", doc) {
        let mut i = 0;
        while i + 1 < nums.len() {
            if let Object::Integer(key) = nums[i] {
                if let Some(dict) = resolve_dict(doc, &nums[i + 1]) {
                    out.push((key, dict));
                }
            }
            i += 2;
        }
    }
    if let Ok(Object::Array(kids)) = node.get_deref(b"Kids", doc) {
        for kid in kids {
            if let Object::Reference(id) = kid {
                if let Ok(child) = doc.get_dictionary(*id) {
                    collect_nums(doc, child, out, depth + 1);
                }
            }
        }
    }
}

fn catalog_page_labels(doc: &Document) -> Option<Dictionary> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let catalog = doc.get_dictionary(root).ok()?;
    match catalog.get_deref(b"PageLabels", doc).ok()? {
        Object::Dictionary(d) => Some(d.clone()),
        _ => None,
    }
}

fn read_label_ranges(doc: &Document) -> Vec<LabelRange> {
    let Some(node) = catalog_page_labels(doc) else {
        return Vec::new();
    };
    let mut nums = Vec::new();
    collect_nums(doc, &node, &mut nums, 0);

    let mut ranges: Vec<LabelRange> = nums
        .into_iter()
        .map(|(key, dict)| {
            let style = match dict.get(b"S") {
                Ok(Object::Name(name)) => style_from_name(name),
                _ => None,
            };
            let prefix = match dict.get(b"P") {
                Ok(Object::String(bytes, _)) => String::from_utf8_lossy(bytes).into_owned(),
                _ => String::new(),
            };
            // /St defaults to 1 and must be >= 1 per spec.
            let start_at = match dict.get(b"St") {
                Ok(Object::Integer(n)) => (*n).max(1),
                _ => 1,
            };
            LabelRange {
                start_page: key.max(0),
                style,
                prefix,
                start_at,
            }
        })
        .collect();
    ranges.sort_by_key(|r| r.start_page);
    ranges
}

/// Compute the display label for every page (ISO 32000-1 §12.4.2).
///
/// Returns exactly `page_count` entries. If the document declares no
/// `/PageLabels`, each page gets its decimal physical number (`"1"`, `"2"`, …),
/// matching the viewer default. Pages before the first labelling range (an
/// unusual document) likewise fall back to the decimal physical number.
pub(crate) fn read_page_labels(doc: &Document, page_count: usize) -> Vec<String> {
    let ranges = read_label_ranges(doc);
    if ranges.is_empty() {
        return (1..=page_count).map(|n| n.to_string()).collect();
    }
    (0..page_count as i64)
        .map(
            |page| match ranges.iter().rev().find(|r| r.start_page <= page) {
                Some(range) => format_label(range, page - range.start_page),
                None => (page + 1).to_string(),
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object, StringFormat};

    fn doc_with_page_labels(nums: Vec<Object>) -> Document {
        let mut doc = Document::with_version("1.7");
        let labels_id = doc.add_object(dictionary! { "Nums" => Object::Array(nums) });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "PageLabels" => Object::Reference(labels_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc
    }

    #[test]
    fn roman_and_alpha_formatting() {
        assert_eq!(to_roman(1), "I");
        assert_eq!(to_roman(4), "IV");
        assert_eq!(to_roman(9), "IX");
        assert_eq!(to_roman(2024), "MMXXIV");
        assert_eq!(to_alpha(1), "A");
        assert_eq!(to_alpha(26), "Z");
        assert_eq!(to_alpha(27), "AA");
        assert_eq!(to_alpha(53), "AAA");
    }

    #[test]
    fn no_page_labels_defaults_to_decimal() {
        let doc = Document::with_version("1.7");
        assert_eq!(read_page_labels(&doc, 3), vec!["1", "2", "3"]);
    }

    #[test]
    fn roman_front_matter_then_decimal_body() {
        // Pages 0-2 lowercase roman; pages 3+ decimal starting at 1.
        let nums = vec![
            Object::Integer(0),
            Object::Dictionary(dictionary! { "S" => "r" }),
            Object::Integer(3),
            Object::Dictionary(dictionary! { "S" => "D", "St" => 1 }),
        ];
        let doc = doc_with_page_labels(nums);
        assert_eq!(
            read_page_labels(&doc, 6),
            vec!["i", "ii", "iii", "1", "2", "3"]
        );
    }

    #[test]
    fn prefix_and_start_offset() {
        let nums = vec![
            Object::Integer(0),
            Object::Dictionary(dictionary! {
                "S" => "D",
                "P" => Object::String(b"A-".to_vec(), StringFormat::Literal),
                "St" => 5,
            }),
        ];
        let doc = doc_with_page_labels(nums);
        assert_eq!(read_page_labels(&doc, 3), vec!["A-5", "A-6", "A-7"]);
    }

    #[test]
    fn prefix_only_without_style() {
        let nums = vec![
            Object::Integer(0),
            Object::Dictionary(dictionary! {
                "P" => Object::String(b"Cover".to_vec(), StringFormat::Literal),
            }),
        ];
        let doc = doc_with_page_labels(nums);
        assert_eq!(read_page_labels(&doc, 1), vec!["Cover"]);
    }
}
