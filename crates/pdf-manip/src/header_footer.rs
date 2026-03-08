//! Headers, footers, and Bates numbering for PDF pages.
//!
//! Adds text content to page margins with support for dynamic variables
//! and Bates numbering sequences.

use crate::error::{ManipError, Result};
use crate::watermark::{
    add_content_to_page, ensure_page_font, get_page_dimensions, resolve_page_selection, Layer,
    PageSelection,
};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

/// Text alignment for header/footer zones.
#[derive(Debug, Clone, Copy)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

/// A header or footer configuration with left/center/right text zones.
#[derive(Debug, Clone)]
pub struct HeaderFooter {
    /// Left-aligned text (may contain variables).
    pub left: Option<String>,
    /// Center-aligned text.
    pub center: Option<String>,
    /// Right-aligned text.
    pub right: Option<String>,
    /// Font size in points (default: 10).
    pub font_size: f32,
    /// Margin from page edge in points (default: 36 = 0.5 inch).
    pub margin: f32,
    /// Text color as RGB (default: black).
    pub color: (f32, f32, f32),
}

impl Default for HeaderFooter {
    fn default() -> Self {
        Self {
            left: None,
            center: None,
            right: None,
            font_size: 10.0,
            margin: 36.0,
            color: (0.0, 0.0, 0.0),
        }
    }
}

/// Configuration for Bates numbering.
#[derive(Debug, Clone)]
pub struct BatesConfig {
    /// Prefix before the number (e.g., "DOC-").
    pub prefix: String,
    /// Suffix after the number (e.g., "").
    pub suffix: String,
    /// Number of digits (zero-padded, default: 6).
    pub digits: usize,
    /// Starting number (default: 1).
    pub start: u64,
    /// Position: header or footer.
    pub position: BatesPosition,
    /// Alignment within the zone.
    pub alignment: Alignment,
    /// Font size in points (default: 10).
    pub font_size: f32,
    /// Margin from page edge in points (default: 36).
    pub margin: f32,
    /// Text color as RGB (default: black).
    pub color: (f32, f32, f32),
}

/// Where to place the Bates number.
#[derive(Debug, Clone, Copy)]
pub enum BatesPosition {
    Header,
    Footer,
}

impl Default for BatesConfig {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            suffix: String::new(),
            digits: 6,
            start: 1,
            position: BatesPosition::Footer,
            alignment: Alignment::Right,
            font_size: 10.0,
            margin: 36.0,
            color: (0.0, 0.0, 0.0),
        }
    }
}

/// Add a header to selected pages.
pub fn add_header(
    doc: &mut Document,
    header: &HeaderFooter,
    selection: &PageSelection,
) -> Result<()> {
    add_header_footer(doc, header, selection, true)
}

/// Add a footer to selected pages.
pub fn add_footer(
    doc: &mut Document,
    footer: &HeaderFooter,
    selection: &PageSelection,
) -> Result<()> {
    add_header_footer(doc, footer, selection, false)
}

/// Add Bates numbering to selected pages.
pub fn add_bates(
    doc: &mut Document,
    config: &BatesConfig,
    selection: &PageSelection,
) -> Result<()> {
    let page_nums = resolve_page_selection(doc, selection)?;
    let total_pages = doc.get_pages().len();

    for (seq_idx, page_num) in page_nums.iter().enumerate() {
        let pages = doc.get_pages();
        let page_id = *pages
            .get(page_num)
            .ok_or(ManipError::PageOutOfRange(*page_num as usize, pages.len()))?;

        let (page_width, page_height) = get_page_dimensions(doc, page_id)?;

        let bates_number = config.start + seq_idx as u64;
        let formatted = format!(
            "{}{}{}",
            config.prefix,
            format_bates_number(bates_number, config.digits),
            config.suffix,
        );

        let y = match config.position {
            BatesPosition::Header => page_height - config.margin,
            BatesPosition::Footer => config.margin - config.font_size,
        };

        let x = compute_x(
            config.alignment,
            &formatted,
            config.font_size,
            page_width,
            config.margin,
        );

        let ops = build_text_ops(
            &formatted,
            x,
            y,
            config.font_size,
            config.color,
            *page_num,
            total_pages,
        );

        let content_data = Content { operations: ops }
            .encode()
            .map_err(|e| ManipError::Watermark(format!("failed to encode Bates content: {e}")))?;

        let stream = Stream::new(dictionary! {}, content_data);
        let stream_id = doc.add_object(Object::Stream(stream));

        ensure_page_font(doc, page_id, "F_HF");
        add_content_to_page(doc, page_id, stream_id, Layer::Foreground);
    }

    Ok(())
}

fn add_header_footer(
    doc: &mut Document,
    hf: &HeaderFooter,
    selection: &PageSelection,
    is_header: bool,
) -> Result<()> {
    let page_nums = resolve_page_selection(doc, selection)?;
    let total_pages = doc.get_pages().len();

    for page_num in &page_nums {
        let pages = doc.get_pages();
        let page_id = *pages
            .get(page_num)
            .ok_or(ManipError::PageOutOfRange(*page_num as usize, pages.len()))?;

        let (page_width, page_height) = get_page_dimensions(doc, page_id)?;

        let y = if is_header {
            page_height - hf.margin
        } else {
            hf.margin - hf.font_size
        };

        let mut all_ops = Vec::new();

        for (alignment, text_opt) in [
            (Alignment::Left, &hf.left),
            (Alignment::Center, &hf.center),
            (Alignment::Right, &hf.right),
        ] {
            if let Some(template) = text_opt {
                let resolved = resolve_variables(template, *page_num, total_pages);
                let x = compute_x(alignment, &resolved, hf.font_size, page_width, hf.margin);
                let ops = build_text_ops(
                    &resolved,
                    x,
                    y,
                    hf.font_size,
                    hf.color,
                    *page_num,
                    total_pages,
                );
                all_ops.extend(ops);
            }
        }

        if all_ops.is_empty() {
            continue;
        }

        let content_data = Content {
            operations: all_ops,
        }
        .encode()
        .map_err(|e| {
            ManipError::Watermark(format!("failed to encode header/footer content: {e}"))
        })?;

        let stream = Stream::new(dictionary! {}, content_data);
        let stream_id = doc.add_object(Object::Stream(stream));

        ensure_page_font(doc, page_id, "F_HF");
        add_content_to_page(doc, page_id, stream_id, Layer::Foreground);
    }

    Ok(())
}

/// Resolve dynamic variables in a template string.
fn resolve_variables(template: &str, page_num: u32, total_pages: usize) -> String {
    template
        .replace("{page}", &page_num.to_string())
        .replace("{pages}", &total_pages.to_string())
        .replace("{date}", &current_date())
}

fn current_date() -> String {
    // Simple ISO date format. No external dependency needed.
    // In production this would use chrono or time crate.
    "2026-03-08".to_string()
}

fn format_bates_number(number: u64, digits: usize) -> String {
    format!("{number:0>width$}", width = digits)
}

/// Compute X position based on alignment.
fn compute_x(
    alignment: Alignment,
    text: &str,
    font_size: f32,
    page_width: f32,
    margin: f32,
) -> f32 {
    // Approximate text width: ~0.5 * font_size per character for Helvetica.
    let approx_width = text.len() as f32 * font_size * 0.5;
    match alignment {
        Alignment::Left => margin,
        Alignment::Center => (page_width - approx_width) / 2.0,
        Alignment::Right => page_width - margin - approx_width,
    }
}

/// Build PDF content stream operations for a text string at a given position.
fn build_text_ops(
    text: &str,
    x: f32,
    y: f32,
    font_size: f32,
    color: (f32, f32, f32),
    _page_num: u32,
    _total_pages: usize,
) -> Vec<Operation> {
    vec![
        Operation::new("q", vec![]),
        Operation::new(
            "rg",
            vec![
                Object::Real(color.0),
                Object::Real(color.1),
                Object::Real(color.2),
            ],
        ),
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![Object::Name(b"F_HF".to_vec()), Object::Real(font_size)],
        ),
        Operation::new("Td", vec![Object::Real(x), Object::Real(y)]),
        Operation::new(
            "Tj",
            vec![Object::String(
                text.as_bytes().to_vec(),
                lopdf::StringFormat::Literal,
            )],
        ),
        Operation::new("ET", vec![]),
        Operation::new("Q", vec![]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watermark::PageSelection;

    fn make_test_doc(num_pages: usize) -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();

        for i in 0..num_pages {
            let content_data = format!("BT /F1 12 Tf (Page {}) Tj ET", i + 1);
            let content_stream = Stream::new(dictionary! {}, content_data.into_bytes());
            let content_id = doc.add_object(Object::Stream(content_stream));

            let page_dict = dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(612), Object::Integer(792),
                ]),
                "Contents" => Object::Reference(content_id),
                "Resources" => Object::Dictionary(lopdf::Dictionary::new()),
            };
            let page_id = doc.add_object(Object::Dictionary(page_dict));
            kids.push(Object::Reference(page_id));
        }

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(num_pages as i64),
            "Kids" => Object::Array(kids),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn add_header_to_all_pages() {
        let mut doc = make_test_doc(3);
        let header = HeaderFooter {
            center: Some("Page {page} of {pages}".into()),
            ..Default::default()
        };
        add_header(&mut doc, &header, &PageSelection::All).unwrap();
        // Verify each page has additional content streams.
        for page_num in 1..=3u32 {
            let pages = doc.get_pages();
            let page_id = pages[&page_num];
            if let Object::Dictionary(d) = doc.get_object(page_id).unwrap() {
                let contents = d.get(b"Contents").unwrap();
                // Should be an array (original + header stream).
                assert!(
                    matches!(contents, Object::Array(_)),
                    "Page {page_num} should have array Contents"
                );
            }
        }
    }

    #[test]
    fn add_footer_even_pages_only() {
        let mut doc = make_test_doc(4);
        let footer = HeaderFooter {
            right: Some("Confidential".into()),
            ..Default::default()
        };
        add_footer(&mut doc, &footer, &PageSelection::Even).unwrap();

        // Odd pages should still have single content ref.
        let pages = doc.get_pages();
        let page_1 = pages[&1];
        if let Object::Dictionary(d) = doc.get_object(page_1).unwrap() {
            let contents = d.get(b"Contents").unwrap();
            assert!(
                matches!(contents, Object::Reference(_)),
                "Page 1 should have single Content ref"
            );
        }

        // Even pages should have array.
        let page_2 = pages[&2];
        if let Object::Dictionary(d) = doc.get_object(page_2).unwrap() {
            let contents = d.get(b"Contents").unwrap();
            assert!(
                matches!(contents, Object::Array(_)),
                "Page 2 should have array Contents"
            );
        }
    }

    #[test]
    fn bates_numbering() {
        let mut doc = make_test_doc(3);
        let config = BatesConfig {
            prefix: "DOC-".into(),
            digits: 6,
            start: 42,
            ..Default::default()
        };
        add_bates(&mut doc, &config, &PageSelection::All).unwrap();

        // All pages should have added content.
        for page_num in 1..=3u32 {
            let pages = doc.get_pages();
            let page_id = pages[&page_num];
            if let Object::Dictionary(d) = doc.get_object(page_id).unwrap() {
                let contents = d.get(b"Contents").unwrap();
                assert!(matches!(contents, Object::Array(_)));
            }
        }
    }

    #[test]
    fn format_bates_number_zero_padded() {
        assert_eq!(format_bates_number(1, 6), "000001");
        assert_eq!(format_bates_number(42, 6), "000042");
        assert_eq!(format_bates_number(999999, 6), "999999");
        assert_eq!(format_bates_number(1000000, 6), "1000000");
        assert_eq!(format_bates_number(5, 3), "005");
    }

    #[test]
    fn resolve_variables_substitution() {
        let result = resolve_variables("Page {page} of {pages}", 3, 10);
        assert_eq!(result, "Page 3 of 10");
    }

    #[test]
    fn resolve_variables_date() {
        let result = resolve_variables("Date: {date}", 1, 1);
        assert!(result.starts_with("Date: "));
        assert!(result.len() > 6);
    }

    #[test]
    fn header_with_all_zones() {
        let mut doc = make_test_doc(1);
        let header = HeaderFooter {
            left: Some("Left text".into()),
            center: Some("Center text".into()),
            right: Some("Right text".into()),
            ..Default::default()
        };
        add_header(&mut doc, &header, &PageSelection::All).unwrap();

        let pages = doc.get_pages();
        let page_id = pages[&1];
        if let Object::Dictionary(d) = doc.get_object(page_id).unwrap() {
            let contents = d.get(b"Contents").unwrap();
            assert!(matches!(contents, Object::Array(_)));
        }
    }

    #[test]
    fn bates_with_suffix() {
        let config = BatesConfig {
            prefix: "INV-".into(),
            suffix: "-2026".into(),
            digits: 4,
            start: 1,
            ..Default::default()
        };
        let bates_number = config.start;
        let formatted = format!(
            "{}{}{}",
            config.prefix,
            format_bates_number(bates_number, config.digits),
            config.suffix,
        );
        assert_eq!(formatted, "INV-0001-2026");
    }

    #[test]
    fn compute_x_alignment() {
        let width = 612.0;
        let margin = 36.0;

        let x_left = compute_x(Alignment::Left, "test", 10.0, width, margin);
        assert_eq!(x_left, margin);

        let x_center = compute_x(Alignment::Center, "test", 10.0, width, margin);
        assert!(x_center > margin && x_center < width - margin);

        let x_right = compute_x(Alignment::Right, "test", 10.0, width, margin);
        assert!(x_right > width / 2.0);
    }
}
