//! DOCX OOXML writer using quick-xml and zip.

use crate::error::Result;
use crate::layout::{map_font_name, DocxImage, PageElement, Paragraph, Run, Table};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// EMU per inch (for image sizing).
const EMU_PER_INCH: i64 = 914400;

/// Default image DPI assumption.
const DEFAULT_DPI: f64 = 96.0;

/// Write a complete DOCX file from page elements, returning bytes.
pub fn write_docx(
    elements: &[Vec<PageElement>],
    images: &[DocxImage],
    output: &mut Vec<u8>,
) -> Result<()> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // [Content_Types].xml
    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(&write_content_types(images)?)?;

    // _rels/.rels
    zip.start_file("_rels/.rels", options)?;
    zip.write_all(&write_root_rels()?)?;

    // word/_rels/document.xml.rels
    zip.start_file("word/_rels/document.xml.rels", options)?;
    zip.write_all(&write_document_rels(images)?)?;

    // word/styles.xml
    zip.start_file("word/styles.xml", options)?;
    zip.write_all(&write_styles()?)?;

    // word/document.xml
    zip.start_file("word/document.xml", options)?;
    zip.write_all(&write_document(elements, images)?)?;

    // word/media/* (images)
    for img in images {
        let path = format!("word/media/{}", img.id);
        zip.start_file(path, options)?;
        zip.write_all(&img.data)?;
    }

    let cursor = zip.finish()?;
    *output = cursor.into_inner();
    Ok(())
}

fn write_content_types(images: &[DocxImage]) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);

    w.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;

    let mut types = BytesStart::new("Types");
    types.push_attribute((
        "xmlns",
        "http://schemas.openxmlformats.org/package/2006/content-types",
    ));
    w.write_event(Event::Start(types))?;

    // Default types
    write_default(
        &mut w,
        "rels",
        "application/vnd.openxmlformats-package.relationships+xml",
    )?;
    write_default(&mut w, "xml", "application/xml")?;

    // Image types
    let mut seen_ext: std::collections::HashSet<String> = std::collections::HashSet::new();
    for img in images {
        let ext = image_extension(&img.content_type);
        if seen_ext.insert(ext.to_string()) {
            write_default(&mut w, ext, &img.content_type)?;
        }
    }

    // Override for document
    write_override(
        &mut w,
        "/word/document.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
    )?;
    write_override(
        &mut w,
        "/word/styles.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
    )?;

    w.write_event(Event::End(BytesEnd::new("Types")))?;
    Ok(buf.into_inner())
}

fn write_default(w: &mut Writer<&mut Cursor<Vec<u8>>>, ext: &str, ct: &str) -> Result<()> {
    let mut el = BytesStart::new("Default");
    el.push_attribute(("Extension", ext));
    el.push_attribute(("ContentType", ct));
    w.write_event(Event::Empty(el))?;
    Ok(())
}

fn write_override(w: &mut Writer<&mut Cursor<Vec<u8>>>, part: &str, ct: &str) -> Result<()> {
    let mut el = BytesStart::new("Override");
    el.push_attribute(("PartName", part));
    el.push_attribute(("ContentType", ct));
    w.write_event(Event::Empty(el))?;
    Ok(())
}

fn write_root_rels() -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);

    w.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;

    let mut rels = BytesStart::new("Relationships");
    rels.push_attribute((
        "xmlns",
        "http://schemas.openxmlformats.org/package/2006/relationships",
    ));
    w.write_event(Event::Start(rels))?;

    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Id", "rId1"));
    rel.push_attribute((
        "Type",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
    ));
    rel.push_attribute(("Target", "word/document.xml"));
    w.write_event(Event::Empty(rel))?;

    w.write_event(Event::End(BytesEnd::new("Relationships")))?;
    Ok(buf.into_inner())
}

fn write_document_rels(images: &[DocxImage]) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);

    w.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;

    let mut rels = BytesStart::new("Relationships");
    rels.push_attribute((
        "xmlns",
        "http://schemas.openxmlformats.org/package/2006/relationships",
    ));
    w.write_event(Event::Start(rels))?;

    // Styles relationship
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Id", "rId1"));
    rel.push_attribute((
        "Type",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
    ));
    rel.push_attribute(("Target", "styles.xml"));
    w.write_event(Event::Empty(rel))?;

    // Image relationships
    for (i, img) in images.iter().enumerate() {
        let rid = format!("rId{}", i + 2);
        let target = format!("media/{}", img.id);
        let mut rel = BytesStart::new("Relationship");
        rel.push_attribute(("Id", rid.as_str()));
        rel.push_attribute((
            "Type",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
        ));
        rel.push_attribute(("Target", target.as_str()));
        w.write_event(Event::Empty(rel))?;
    }

    w.write_event(Event::End(BytesEnd::new("Relationships")))?;
    Ok(buf.into_inner())
}

fn write_styles() -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);

    w.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;

    let mut styles = BytesStart::new("w:styles");
    styles.push_attribute((
        "xmlns:w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    ));
    w.write_event(Event::Start(styles))?;

    // Default run properties
    w.write_event(Event::Start(BytesStart::new("w:docDefaults")))?;
    w.write_event(Event::Start(BytesStart::new("w:rPrDefault")))?;
    w.write_event(Event::Start(BytesStart::new("w:rPr")))?;

    let mut font = BytesStart::new("w:rFonts");
    font.push_attribute(("w:ascii", "Calibri"));
    font.push_attribute(("w:hAnsi", "Calibri"));
    w.write_event(Event::Empty(font))?;

    let mut sz = BytesStart::new("w:sz");
    sz.push_attribute(("w:val", "24")); // 12pt = 24 half-points
    w.write_event(Event::Empty(sz))?;

    w.write_event(Event::End(BytesEnd::new("w:rPr")))?;
    w.write_event(Event::End(BytesEnd::new("w:rPrDefault")))?;
    w.write_event(Event::End(BytesEnd::new("w:docDefaults")))?;

    // Normal style
    let mut style = BytesStart::new("w:style");
    style.push_attribute(("w:type", "paragraph"));
    style.push_attribute(("w:styleId", "Normal"));
    style.push_attribute(("w:default", "1"));
    w.write_event(Event::Start(style))?;

    w.write_event(Event::Start(BytesStart::new("w:name")))?;
    w.write_event(Event::Text(BytesText::new("Normal")))?;
    w.write_event(Event::End(BytesEnd::new("w:name")))?;

    w.write_event(Event::End(BytesEnd::new("w:style")))?;

    // Heading styles
    for level in 1..=6u8 {
        let style_id = format!("Heading{level}");
        let mut style = BytesStart::new("w:style");
        style.push_attribute(("w:type", "paragraph"));
        style.push_attribute(("w:styleId", style_id.as_str()));
        w.write_event(Event::Start(style))?;

        let name = format!("heading {level}");
        w.write_event(Event::Start(BytesStart::new("w:name")))?;
        w.write_event(Event::Text(BytesText::new(&name)))?;
        w.write_event(Event::End(BytesEnd::new("w:name")))?;

        w.write_event(Event::Start(BytesStart::new("w:rPr")))?;
        w.write_event(Event::Empty(BytesStart::new("w:b")))?;
        let size = match level {
            1 => "48",
            2 => "36",
            3 => "28",
            _ => "24",
        };
        let mut sz = BytesStart::new("w:sz");
        sz.push_attribute(("w:val", size));
        w.write_event(Event::Empty(sz))?;
        w.write_event(Event::End(BytesEnd::new("w:rPr")))?;

        w.write_event(Event::End(BytesEnd::new("w:style")))?;
    }

    w.write_event(Event::End(BytesEnd::new("w:styles")))?;
    Ok(buf.into_inner())
}

fn write_document(pages: &[Vec<PageElement>], images: &[DocxImage]) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);

    w.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;

    let mut doc = BytesStart::new("w:document");
    doc.push_attribute((
        "xmlns:w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    ));
    doc.push_attribute((
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ));
    doc.push_attribute((
        "xmlns:wp",
        "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    ));
    doc.push_attribute((
        "xmlns:a",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
    ));
    doc.push_attribute((
        "xmlns:pic",
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
    ));
    w.write_event(Event::Start(doc))?;

    w.write_event(Event::Start(BytesStart::new("w:body")))?;

    let mut img_idx = 0;

    for page_elements in pages {
        for element in page_elements {
            match element {
                PageElement::Para(para) => write_paragraph(&mut w, para)?,
                PageElement::Tbl(table) => write_table(&mut w, table)?,
                PageElement::Img(img) => {
                    let rid = format!("rId{}", find_image_rid(images, &img.id) + 2);
                    write_image_paragraph(&mut w, img, &rid, img_idx)?;
                    img_idx += 1;
                }
            }
        }
    }

    w.write_event(Event::End(BytesEnd::new("w:body")))?;
    w.write_event(Event::End(BytesEnd::new("w:document")))?;
    Ok(buf.into_inner())
}

fn find_image_rid(images: &[DocxImage], id: &str) -> usize {
    images.iter().position(|img| img.id == id).unwrap_or(0)
}

fn write_paragraph(w: &mut Writer<&mut Cursor<Vec<u8>>>, para: &Paragraph) -> Result<()> {
    w.write_event(Event::Start(BytesStart::new("w:p")))?;

    for run in &para.runs {
        write_run(w, run)?;
    }

    w.write_event(Event::End(BytesEnd::new("w:p")))?;
    Ok(())
}

fn write_run(w: &mut Writer<&mut Cursor<Vec<u8>>>, run: &Run) -> Result<()> {
    w.write_event(Event::Start(BytesStart::new("w:r")))?;

    // Run properties
    let has_props = run.bold || run.italic || run.font_size != 12.0 || !run.font_name.is_empty();
    if has_props {
        w.write_event(Event::Start(BytesStart::new("w:rPr")))?;

        if !run.font_name.is_empty() {
            let mapped = map_font_name(&run.font_name);
            let mut font = BytesStart::new("w:rFonts");
            font.push_attribute(("w:ascii", mapped));
            font.push_attribute(("w:hAnsi", mapped));
            w.write_event(Event::Empty(font))?;
        }

        if run.bold {
            w.write_event(Event::Empty(BytesStart::new("w:b")))?;
        }

        if run.italic {
            w.write_event(Event::Empty(BytesStart::new("w:i")))?;
        }

        if (run.font_size - 12.0).abs() > 0.5 {
            let half_pts = (run.font_size * 2.0).round() as i64;
            let mut sz = BytesStart::new("w:sz");
            sz.push_attribute(("w:val", half_pts.to_string().as_str()));
            w.write_event(Event::Empty(sz))?;
        }

        w.write_event(Event::End(BytesEnd::new("w:rPr")))?;
    }

    // Text content
    let mut t = BytesStart::new("w:t");
    t.push_attribute(("xml:space", "preserve"));
    w.write_event(Event::Start(t))?;
    w.write_event(Event::Text(BytesText::new(&run.text)))?;
    w.write_event(Event::End(BytesEnd::new("w:t")))?;

    w.write_event(Event::End(BytesEnd::new("w:r")))?;
    Ok(())
}

fn write_table(w: &mut Writer<&mut Cursor<Vec<u8>>>, table: &Table) -> Result<()> {
    w.write_event(Event::Start(BytesStart::new("w:tbl")))?;

    // Table properties
    w.write_event(Event::Start(BytesStart::new("w:tblPr")))?;

    let mut style = BytesStart::new("w:tblStyle");
    style.push_attribute(("w:val", "TableGrid"));
    w.write_event(Event::Empty(style))?;

    let mut width = BytesStart::new("w:tblW");
    width.push_attribute(("w:w", "0"));
    width.push_attribute(("w:type", "auto"));
    w.write_event(Event::Empty(width))?;

    // Table borders
    w.write_event(Event::Start(BytesStart::new("w:tblBorders")))?;
    for border_name in &[
        "w:top",
        "w:left",
        "w:bottom",
        "w:right",
        "w:insideH",
        "w:insideV",
    ] {
        let mut b = BytesStart::new(*border_name);
        b.push_attribute(("w:val", "single"));
        b.push_attribute(("w:sz", "4"));
        b.push_attribute(("w:space", "0"));
        b.push_attribute(("w:color", "auto"));
        w.write_event(Event::Empty(b))?;
    }
    w.write_event(Event::End(BytesEnd::new("w:tblBorders")))?;

    w.write_event(Event::End(BytesEnd::new("w:tblPr")))?;

    // Grid definition
    w.write_event(Event::Start(BytesStart::new("w:tblGrid")))?;
    let col_width = 9000 / table.col_count.max(1);
    for _ in 0..table.col_count {
        let mut gc = BytesStart::new("w:gridCol");
        gc.push_attribute(("w:w", col_width.to_string().as_str()));
        w.write_event(Event::Empty(gc))?;
    }
    w.write_event(Event::End(BytesEnd::new("w:tblGrid")))?;

    // Rows
    for row in &table.rows {
        w.write_event(Event::Start(BytesStart::new("w:tr")))?;

        for cell_text in row {
            w.write_event(Event::Start(BytesStart::new("w:tc")))?;
            w.write_event(Event::Start(BytesStart::new("w:p")))?;
            w.write_event(Event::Start(BytesStart::new("w:r")))?;

            let mut t = BytesStart::new("w:t");
            t.push_attribute(("xml:space", "preserve"));
            w.write_event(Event::Start(t))?;
            w.write_event(Event::Text(BytesText::new(cell_text)))?;
            w.write_event(Event::End(BytesEnd::new("w:t")))?;

            w.write_event(Event::End(BytesEnd::new("w:r")))?;
            w.write_event(Event::End(BytesEnd::new("w:p")))?;
            w.write_event(Event::End(BytesEnd::new("w:tc")))?;
        }

        w.write_event(Event::End(BytesEnd::new("w:tr")))?;
    }

    w.write_event(Event::End(BytesEnd::new("w:tbl")))?;
    Ok(())
}

fn write_image_paragraph(
    w: &mut Writer<&mut Cursor<Vec<u8>>>,
    img: &DocxImage,
    rid: &str,
    idx: usize,
) -> Result<()> {
    let cx = (img.width as f64 / DEFAULT_DPI * EMU_PER_INCH as f64) as i64;
    let cy = (img.height as f64 / DEFAULT_DPI * EMU_PER_INCH as f64) as i64;
    let cx_str = cx.to_string();
    let cy_str = cy.to_string();
    let id_str = (idx + 1).to_string();
    let name = format!("Image{}", idx + 1);

    w.write_event(Event::Start(BytesStart::new("w:p")))?;
    w.write_event(Event::Start(BytesStart::new("w:r")))?;
    w.write_event(Event::Start(BytesStart::new("w:drawing")))?;

    // Inline drawing
    let mut inline = BytesStart::new("wp:inline");
    inline.push_attribute(("distT", "0"));
    inline.push_attribute(("distB", "0"));
    inline.push_attribute(("distL", "0"));
    inline.push_attribute(("distR", "0"));
    w.write_event(Event::Start(inline))?;

    // Extent
    let mut extent = BytesStart::new("wp:extent");
    extent.push_attribute(("cx", cx_str.as_str()));
    extent.push_attribute(("cy", cy_str.as_str()));
    w.write_event(Event::Empty(extent))?;

    // DocPr
    let mut doc_pr = BytesStart::new("wp:docPr");
    doc_pr.push_attribute(("id", id_str.as_str()));
    doc_pr.push_attribute(("name", name.as_str()));
    w.write_event(Event::Empty(doc_pr))?;

    // Graphic
    let mut graphic = BytesStart::new("a:graphic");
    graphic.push_attribute((
        "xmlns:a",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
    ));
    w.write_event(Event::Start(graphic))?;

    let mut gd = BytesStart::new("a:graphicData");
    gd.push_attribute((
        "uri",
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
    ));
    w.write_event(Event::Start(gd))?;

    let mut pic = BytesStart::new("pic:pic");
    pic.push_attribute((
        "xmlns:pic",
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
    ));
    w.write_event(Event::Start(pic))?;

    // Non-visual properties
    w.write_event(Event::Start(BytesStart::new("pic:nvPicPr")))?;

    let mut cnv = BytesStart::new("pic:cNvPr");
    cnv.push_attribute(("id", id_str.as_str()));
    cnv.push_attribute(("name", name.as_str()));
    w.write_event(Event::Empty(cnv))?;

    w.write_event(Event::Empty(BytesStart::new("pic:cNvPicPr")))?;

    w.write_event(Event::End(BytesEnd::new("pic:nvPicPr")))?;

    // Blip fill
    w.write_event(Event::Start(BytesStart::new("pic:blipFill")))?;

    let mut blip = BytesStart::new("a:blip");
    blip.push_attribute(("r:embed", rid));
    w.write_event(Event::Empty(blip))?;

    w.write_event(Event::Start(BytesStart::new("a:stretch")))?;
    w.write_event(Event::Empty(BytesStart::new("a:fillRect")))?;
    w.write_event(Event::End(BytesEnd::new("a:stretch")))?;

    w.write_event(Event::End(BytesEnd::new("pic:blipFill")))?;

    // Shape properties
    w.write_event(Event::Start(BytesStart::new("pic:spPr")))?;

    let xfrm = BytesStart::new("a:xfrm");
    w.write_event(Event::Start(xfrm))?;

    let mut off = BytesStart::new("a:off");
    off.push_attribute(("x", "0"));
    off.push_attribute(("y", "0"));
    w.write_event(Event::Empty(off))?;

    let mut ext = BytesStart::new("a:ext");
    ext.push_attribute(("cx", cx_str.as_str()));
    ext.push_attribute(("cy", cy_str.as_str()));
    w.write_event(Event::Empty(ext))?;

    w.write_event(Event::End(BytesEnd::new("a:xfrm")))?;

    let mut prst = BytesStart::new("a:prstGeom");
    prst.push_attribute(("prst", "rect"));
    w.write_event(Event::Start(prst))?;
    w.write_event(Event::Empty(BytesStart::new("a:avLst")))?;
    w.write_event(Event::End(BytesEnd::new("a:prstGeom")))?;

    w.write_event(Event::End(BytesEnd::new("pic:spPr")))?;

    w.write_event(Event::End(BytesEnd::new("pic:pic")))?;
    w.write_event(Event::End(BytesEnd::new("a:graphicData")))?;
    w.write_event(Event::End(BytesEnd::new("a:graphic")))?;
    w.write_event(Event::End(BytesEnd::new("wp:inline")))?;
    w.write_event(Event::End(BytesEnd::new("w:drawing")))?;
    w.write_event(Event::End(BytesEnd::new("w:r")))?;
    w.write_event(Event::End(BytesEnd::new("w:p")))?;
    Ok(())
}

fn image_extension(content_type: &str) -> &str {
    match content_type {
        "image/jpeg" => "jpeg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/tiff" => "tiff",
        _ => "png",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_minimal_docx() {
        let para = Paragraph {
            runs: vec![Run {
                text: "Hello World".to_string(),
                font_name: String::new(),
                font_size: 12.0,
                bold: false,
                italic: false,
            }],
        };

        let pages = vec![vec![PageElement::Para(para)]];
        let mut output = Vec::new();
        write_docx(&pages, &[], &mut output).unwrap();

        // Verify it's a valid ZIP file.
        assert!(output.len() > 100);
        assert_eq!(&output[0..2], b"PK");
    }

    #[test]
    fn write_docx_with_table() {
        let table = Table {
            rows: vec![
                vec!["A".to_string(), "B".to_string()],
                vec!["1".to_string(), "2".to_string()],
            ],
            col_count: 2,
        };

        let pages = vec![vec![PageElement::Tbl(table)]];
        let mut output = Vec::new();
        write_docx(&pages, &[], &mut output).unwrap();
        assert!(output.len() > 100);
    }

    #[test]
    fn write_docx_with_formatting() {
        let para = Paragraph {
            runs: vec![
                Run {
                    text: "Bold ".to_string(),
                    font_name: "Arial-Bold".to_string(),
                    font_size: 14.0,
                    bold: true,
                    italic: false,
                },
                Run {
                    text: "Italic".to_string(),
                    font_name: "Times-Italic".to_string(),
                    font_size: 12.0,
                    bold: false,
                    italic: true,
                },
            ],
        };

        let pages = vec![vec![PageElement::Para(para)]];
        let mut output = Vec::new();
        write_docx(&pages, &[], &mut output).unwrap();
        assert!(output.len() > 100);
    }
}
