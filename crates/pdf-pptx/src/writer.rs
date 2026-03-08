//! PPTX OOXML writer using quick-xml and zip.
//!
//! Each PDF page becomes a PowerPoint slide with text shapes and images.

use crate::error::Result;
use pdf_extract::{ExtractedImage, ImageFilter, TextBlock};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// EMU per point (1 pt = 12700 EMU).
const EMU_PER_PT: i64 = 12700;

/// Default slide width in EMU (10 inches = 9144000 EMU).
const SLIDE_WIDTH: i64 = 9144000;

/// Default slide height in EMU (7.5 inches = 6858000 EMU).
const SLIDE_HEIGHT: i64 = 6858000;

/// An image to embed in the presentation.
#[derive(Debug)]
pub struct PptxImage {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub content_type: String,
    pub filename: String,
}

/// Data for a single slide.
pub struct SlideData {
    pub text_blocks: Vec<TextBlock>,
    pub images: Vec<PptxImage>,
    /// Page width in PDF points.
    pub page_width: f64,
    /// Page height in PDF points.
    pub page_height: f64,
}

/// Write a complete PPTX file from slide data.
pub fn write_pptx(slides: &[SlideData], output: &mut Vec<u8>) -> Result<()> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // [Content_Types].xml
    zip.start_file("[Content_Types].xml", opts)?;
    zip.write_all(&write_content_types(slides)?)?;

    // _rels/.rels
    zip.start_file("_rels/.rels", opts)?;
    zip.write_all(&write_root_rels()?)?;

    // ppt/presentation.xml
    zip.start_file("ppt/presentation.xml", opts)?;
    zip.write_all(&write_presentation(slides.len())?)?;

    // ppt/_rels/presentation.xml.rels
    zip.start_file("ppt/_rels/presentation.xml.rels", opts)?;
    zip.write_all(&write_presentation_rels(slides)?)?;

    // Slide layout and master (minimal)
    zip.start_file("ppt/slideMasters/slideMaster1.xml", opts)?;
    zip.write_all(&write_slide_master()?)?;

    zip.start_file("ppt/slideMasters/_rels/slideMaster1.xml.rels", opts)?;
    zip.write_all(&write_slide_master_rels()?)?;

    zip.start_file("ppt/slideLayouts/slideLayout1.xml", opts)?;
    zip.write_all(&write_slide_layout()?)?;

    zip.start_file("ppt/slideLayouts/_rels/slideLayout1.xml.rels", opts)?;
    zip.write_all(&write_slide_layout_rels()?)?;

    // Slides + their relationships + images
    let mut global_img_idx = 0;
    for (i, slide) in slides.iter().enumerate() {
        let slide_num = i + 1;

        // Slide XML
        let slide_path = format!("ppt/slides/slide{slide_num}.xml");
        zip.start_file(slide_path, opts)?;
        zip.write_all(&write_slide(slide, global_img_idx)?)?;

        // Slide rels
        let rels_path = format!("ppt/slides/_rels/slide{slide_num}.xml.rels");
        zip.start_file(rels_path, opts)?;
        zip.write_all(&write_slide_rels(slide, global_img_idx)?)?;

        // Images
        for img in &slide.images {
            let img_path = format!("ppt/media/{}", img.filename);
            zip.start_file(img_path, opts)?;
            zip.write_all(&img.data)?;
        }

        global_img_idx += slide.images.len();
    }

    let cursor = zip.finish()?;
    *output = cursor.into_inner();
    Ok(())
}

// --- XML generation helpers ---

fn xml_decl(w: &mut Writer<&mut Cursor<Vec<u8>>>) -> Result<()> {
    w.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;
    Ok(())
}

fn write_content_types(slides: &[SlideData]) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);
    xml_decl(&mut w)?;

    let mut types = BytesStart::new("Types");
    types.push_attribute((
        "xmlns",
        "http://schemas.openxmlformats.org/package/2006/content-types",
    ));
    w.write_event(Event::Start(types))?;

    // Defaults
    empty_with_attrs(
        &mut w,
        "Default",
        &[
            ("Extension", "rels"),
            (
                "ContentType",
                "application/vnd.openxmlformats-package.relationships+xml",
            ),
        ],
    )?;
    empty_with_attrs(
        &mut w,
        "Default",
        &[("Extension", "xml"), ("ContentType", "application/xml")],
    )?;

    // Image types
    let mut seen = std::collections::HashSet::new();
    for slide in slides {
        for img in &slide.images {
            let ext = image_ext(&img.content_type);
            if seen.insert(ext.to_string()) {
                empty_with_attrs(
                    &mut w,
                    "Default",
                    &[("Extension", ext), ("ContentType", &img.content_type)],
                )?;
            }
        }
    }

    // Overrides
    empty_with_attrs(&mut w, "Override", &[
        ("PartName", "/ppt/presentation.xml"),
        ("ContentType", "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"),
    ])?;
    empty_with_attrs(
        &mut w,
        "Override",
        &[
            ("PartName", "/ppt/slideMasters/slideMaster1.xml"),
            (
                "ContentType",
                "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml",
            ),
        ],
    )?;
    empty_with_attrs(
        &mut w,
        "Override",
        &[
            ("PartName", "/ppt/slideLayouts/slideLayout1.xml"),
            (
                "ContentType",
                "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml",
            ),
        ],
    )?;

    for i in 1..=slides.len() {
        let part = format!("/ppt/slides/slide{i}.xml");
        empty_with_attrs(
            &mut w,
            "Override",
            &[
                ("PartName", &part),
                (
                    "ContentType",
                    "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
                ),
            ],
        )?;
    }

    w.write_event(Event::End(BytesEnd::new("Types")))?;
    Ok(buf.into_inner())
}

fn write_root_rels() -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);
    xml_decl(&mut w)?;

    let mut rels = BytesStart::new("Relationships");
    rels.push_attribute((
        "xmlns",
        "http://schemas.openxmlformats.org/package/2006/relationships",
    ));
    w.write_event(Event::Start(rels))?;

    empty_with_attrs(&mut w, "Relationship", &[
        ("Id", "rId1"),
        ("Type", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"),
        ("Target", "ppt/presentation.xml"),
    ])?;

    w.write_event(Event::End(BytesEnd::new("Relationships")))?;
    Ok(buf.into_inner())
}

fn write_presentation(slide_count: usize) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);
    xml_decl(&mut w)?;

    let mut pres = BytesStart::new("p:presentation");
    pres.push_attribute((
        "xmlns:a",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
    ));
    pres.push_attribute((
        "xmlns:p",
        "http://schemas.openxmlformats.org/presentationml/2006/main",
    ));
    pres.push_attribute((
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ));
    w.write_event(Event::Start(pres))?;

    // Slide master ID list
    w.write_event(Event::Start(BytesStart::new("p:sldMasterIdLst")))?;
    let mut smid = BytesStart::new("p:sldMasterId");
    smid.push_attribute(("id", "2147483648"));
    smid.push_attribute(("r:id", "rId1"));
    w.write_event(Event::Empty(smid))?;
    w.write_event(Event::End(BytesEnd::new("p:sldMasterIdLst")))?;

    // Slide ID list
    w.write_event(Event::Start(BytesStart::new("p:sldIdLst")))?;
    for i in 0..slide_count {
        let id = (256 + i).to_string();
        let rid = format!("rId{}", i + 2);
        let mut sid = BytesStart::new("p:sldId");
        sid.push_attribute(("id", id.as_str()));
        sid.push_attribute(("r:id", rid.as_str()));
        w.write_event(Event::Empty(sid))?;
    }
    w.write_event(Event::End(BytesEnd::new("p:sldIdLst")))?;

    // Slide size
    let w_str = SLIDE_WIDTH.to_string();
    let h_str = SLIDE_HEIGHT.to_string();
    let mut ssz = BytesStart::new("p:sldSz");
    ssz.push_attribute(("cx", w_str.as_str()));
    ssz.push_attribute(("cy", h_str.as_str()));
    ssz.push_attribute(("type", "screen4x3"));
    w.write_event(Event::Empty(ssz))?;

    let mut nsz = BytesStart::new("p:notesSz");
    nsz.push_attribute(("cx", "6858000"));
    nsz.push_attribute(("cy", "9144000"));
    w.write_event(Event::Empty(nsz))?;

    w.write_event(Event::End(BytesEnd::new("p:presentation")))?;
    Ok(buf.into_inner())
}

fn write_presentation_rels(slides: &[SlideData]) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);
    xml_decl(&mut w)?;

    let mut rels = BytesStart::new("Relationships");
    rels.push_attribute((
        "xmlns",
        "http://schemas.openxmlformats.org/package/2006/relationships",
    ));
    w.write_event(Event::Start(rels))?;

    // Slide master
    empty_with_attrs(
        &mut w,
        "Relationship",
        &[
            ("Id", "rId1"),
            (
                "Type",
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster",
            ),
            ("Target", "slideMasters/slideMaster1.xml"),
        ],
    )?;

    // Slides
    for i in 0..slides.len() {
        let rid = format!("rId{}", i + 2);
        let target = format!("slides/slide{}.xml", i + 1);
        empty_with_attrs(
            &mut w,
            "Relationship",
            &[
                ("Id", &rid),
                (
                    "Type",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide",
                ),
                ("Target", &target),
            ],
        )?;
    }

    w.write_event(Event::End(BytesEnd::new("Relationships")))?;
    Ok(buf.into_inner())
}

fn write_slide_master() -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);
    xml_decl(&mut w)?;

    let mut sm = BytesStart::new("p:sldMaster");
    sm.push_attribute((
        "xmlns:a",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
    ));
    sm.push_attribute((
        "xmlns:p",
        "http://schemas.openxmlformats.org/presentationml/2006/main",
    ));
    sm.push_attribute((
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ));
    w.write_event(Event::Start(sm))?;

    w.write_event(Event::Start(BytesStart::new("p:cSld")))?;
    w.write_event(Event::Start(BytesStart::new("p:spTree")))?;

    // Group shape properties
    w.write_event(Event::Start(BytesStart::new("p:nvGrpSpPr")))?;
    let mut cnv = BytesStart::new("p:cNvPr");
    cnv.push_attribute(("id", "1"));
    cnv.push_attribute(("name", ""));
    w.write_event(Event::Empty(cnv))?;
    w.write_event(Event::Empty(BytesStart::new("p:cNvGrpSpPr")))?;
    w.write_event(Event::Empty(BytesStart::new("p:nvPr")))?;
    w.write_event(Event::End(BytesEnd::new("p:nvGrpSpPr")))?;

    w.write_event(Event::Start(BytesStart::new("p:grpSpPr")))?;
    write_xfrm_group(&mut w)?;
    w.write_event(Event::End(BytesEnd::new("p:grpSpPr")))?;

    w.write_event(Event::End(BytesEnd::new("p:spTree")))?;
    w.write_event(Event::End(BytesEnd::new("p:cSld")))?;

    // Slide layout ID list
    w.write_event(Event::Start(BytesStart::new("p:sldLayoutIdLst")))?;
    let mut lid = BytesStart::new("p:sldLayoutId");
    lid.push_attribute(("id", "2147483649"));
    lid.push_attribute(("r:id", "rId1"));
    w.write_event(Event::Empty(lid))?;
    w.write_event(Event::End(BytesEnd::new("p:sldLayoutIdLst")))?;

    w.write_event(Event::End(BytesEnd::new("p:sldMaster")))?;
    Ok(buf.into_inner())
}

fn write_slide_master_rels() -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);
    xml_decl(&mut w)?;

    let mut rels = BytesStart::new("Relationships");
    rels.push_attribute((
        "xmlns",
        "http://schemas.openxmlformats.org/package/2006/relationships",
    ));
    w.write_event(Event::Start(rels))?;

    empty_with_attrs(
        &mut w,
        "Relationship",
        &[
            ("Id", "rId1"),
            (
                "Type",
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout",
            ),
            ("Target", "../slideLayouts/slideLayout1.xml"),
        ],
    )?;

    w.write_event(Event::End(BytesEnd::new("Relationships")))?;
    Ok(buf.into_inner())
}

fn write_slide_layout() -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);
    xml_decl(&mut w)?;

    let mut sl = BytesStart::new("p:sldLayout");
    sl.push_attribute((
        "xmlns:a",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
    ));
    sl.push_attribute((
        "xmlns:p",
        "http://schemas.openxmlformats.org/presentationml/2006/main",
    ));
    sl.push_attribute((
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ));
    sl.push_attribute(("type", "blank"));
    w.write_event(Event::Start(sl))?;

    w.write_event(Event::Start(BytesStart::new("p:cSld")))?;
    w.write_event(Event::Start(BytesStart::new("p:spTree")))?;

    w.write_event(Event::Start(BytesStart::new("p:nvGrpSpPr")))?;
    let mut cnv = BytesStart::new("p:cNvPr");
    cnv.push_attribute(("id", "1"));
    cnv.push_attribute(("name", ""));
    w.write_event(Event::Empty(cnv))?;
    w.write_event(Event::Empty(BytesStart::new("p:cNvGrpSpPr")))?;
    w.write_event(Event::Empty(BytesStart::new("p:nvPr")))?;
    w.write_event(Event::End(BytesEnd::new("p:nvGrpSpPr")))?;

    w.write_event(Event::Start(BytesStart::new("p:grpSpPr")))?;
    write_xfrm_group(&mut w)?;
    w.write_event(Event::End(BytesEnd::new("p:grpSpPr")))?;

    w.write_event(Event::End(BytesEnd::new("p:spTree")))?;
    w.write_event(Event::End(BytesEnd::new("p:cSld")))?;

    w.write_event(Event::End(BytesEnd::new("p:sldLayout")))?;
    Ok(buf.into_inner())
}

fn write_slide_layout_rels() -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);
    xml_decl(&mut w)?;

    let mut rels = BytesStart::new("Relationships");
    rels.push_attribute((
        "xmlns",
        "http://schemas.openxmlformats.org/package/2006/relationships",
    ));
    w.write_event(Event::Start(rels))?;

    empty_with_attrs(
        &mut w,
        "Relationship",
        &[
            ("Id", "rId1"),
            (
                "Type",
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster",
            ),
            ("Target", "../slideMasters/slideMaster1.xml"),
        ],
    )?;

    w.write_event(Event::End(BytesEnd::new("Relationships")))?;
    Ok(buf.into_inner())
}

fn write_slide(slide: &SlideData, _img_offset: usize) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);
    xml_decl(&mut w)?;

    let mut sld = BytesStart::new("p:sld");
    sld.push_attribute((
        "xmlns:a",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
    ));
    sld.push_attribute((
        "xmlns:p",
        "http://schemas.openxmlformats.org/presentationml/2006/main",
    ));
    sld.push_attribute((
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ));
    w.write_event(Event::Start(sld))?;

    w.write_event(Event::Start(BytesStart::new("p:cSld")))?;
    w.write_event(Event::Start(BytesStart::new("p:spTree")))?;

    // Group shape properties
    w.write_event(Event::Start(BytesStart::new("p:nvGrpSpPr")))?;
    let mut cnv = BytesStart::new("p:cNvPr");
    cnv.push_attribute(("id", "1"));
    cnv.push_attribute(("name", ""));
    w.write_event(Event::Empty(cnv))?;
    w.write_event(Event::Empty(BytesStart::new("p:cNvGrpSpPr")))?;
    w.write_event(Event::Empty(BytesStart::new("p:nvPr")))?;
    w.write_event(Event::End(BytesEnd::new("p:nvGrpSpPr")))?;

    w.write_event(Event::Start(BytesStart::new("p:grpSpPr")))?;
    write_xfrm_group(&mut w)?;
    w.write_event(Event::End(BytesEnd::new("p:grpSpPr")))?;

    // Scale factors: PDF points → slide EMU
    let sx = SLIDE_WIDTH as f64 / (slide.page_width * EMU_PER_PT as f64 / EMU_PER_PT as f64);
    let sy = SLIDE_HEIGHT as f64 / (slide.page_height * EMU_PER_PT as f64 / EMU_PER_PT as f64);
    let scale = sx.min(sy);

    // Shape ID counter
    let mut shape_id = 2u32;

    // Text shapes
    for block in &slide.text_blocks {
        write_text_shape(&mut w, block, slide.page_height, scale, shape_id)?;
        shape_id += 1;
    }

    // Image shapes
    for (i, img) in slide.images.iter().enumerate() {
        let rid = format!("rId{}", i + 2); // rId1 = slideLayout
        write_image_shape(&mut w, img, &rid, shape_id)?;
        shape_id += 1;
    }

    w.write_event(Event::End(BytesEnd::new("p:spTree")))?;
    w.write_event(Event::End(BytesEnd::new("p:cSld")))?;
    w.write_event(Event::End(BytesEnd::new("p:sld")))?;
    Ok(buf.into_inner())
}

fn write_slide_rels(slide: &SlideData, _img_offset: usize) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = Writer::new_with_indent(&mut buf, b' ', 2);
    xml_decl(&mut w)?;

    let mut rels = BytesStart::new("Relationships");
    rels.push_attribute((
        "xmlns",
        "http://schemas.openxmlformats.org/package/2006/relationships",
    ));
    w.write_event(Event::Start(rels))?;

    // Slide layout
    empty_with_attrs(
        &mut w,
        "Relationship",
        &[
            ("Id", "rId1"),
            (
                "Type",
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout",
            ),
            ("Target", "../slideLayouts/slideLayout1.xml"),
        ],
    )?;

    // Images
    for (i, img) in slide.images.iter().enumerate() {
        let rid = format!("rId{}", i + 2);
        let target = format!("../media/{}", img.filename);
        empty_with_attrs(
            &mut w,
            "Relationship",
            &[
                ("Id", &rid),
                (
                    "Type",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
                ),
                ("Target", &target),
            ],
        )?;
    }

    w.write_event(Event::End(BytesEnd::new("Relationships")))?;
    Ok(buf.into_inner())
}

/// Write a text block as a PPTX shape.
fn write_text_shape(
    w: &mut Writer<&mut Cursor<Vec<u8>>>,
    block: &TextBlock,
    page_height: f64,
    scale: f64,
    shape_id: u32,
) -> Result<()> {
    // Convert PDF coordinates (origin bottom-left) to PPTX (origin top-left).
    let x_emu = (block.bbox[0] * EMU_PER_PT as f64 * scale / EMU_PER_PT as f64) as i64;
    let y_pdf = page_height - block.bbox[3]; // flip y
    let y_emu = (y_pdf * EMU_PER_PT as f64 * scale / EMU_PER_PT as f64) as i64;
    let w_emu =
        ((block.bbox[2] - block.bbox[0]) * EMU_PER_PT as f64 * scale / EMU_PER_PT as f64) as i64;
    let h_emu = (block.font_size * EMU_PER_PT as f64 * scale / EMU_PER_PT as f64 * 1.4) as i64;

    let x_emu = x_emu.max(0);
    let y_emu = y_emu.max(0);
    let w_emu = w_emu.max(EMU_PER_PT);
    let h_emu = h_emu.max(EMU_PER_PT);

    w.write_event(Event::Start(BytesStart::new("p:sp")))?;

    // Non-visual properties
    w.write_event(Event::Start(BytesStart::new("p:nvSpPr")))?;
    let name = format!("TextBox {shape_id}");
    let id_str = shape_id.to_string();
    let mut cnv = BytesStart::new("p:cNvPr");
    cnv.push_attribute(("id", id_str.as_str()));
    cnv.push_attribute(("name", name.as_str()));
    w.write_event(Event::Empty(cnv))?;

    let mut cnv_sp = BytesStart::new("p:cNvSpPr");
    cnv_sp.push_attribute(("txBox", "1"));
    w.write_event(Event::Empty(cnv_sp))?;

    w.write_event(Event::Empty(BytesStart::new("p:nvPr")))?;
    w.write_event(Event::End(BytesEnd::new("p:nvSpPr")))?;

    // Shape properties
    w.write_event(Event::Start(BytesStart::new("p:spPr")))?;
    write_xfrm_positioned(w, x_emu, y_emu, w_emu, h_emu)?;

    let mut prst = BytesStart::new("a:prstGeom");
    prst.push_attribute(("prst", "rect"));
    w.write_event(Event::Start(prst))?;
    w.write_event(Event::Empty(BytesStart::new("a:avLst")))?;
    w.write_event(Event::End(BytesEnd::new("a:prstGeom")))?;

    w.write_event(Event::Empty(BytesStart::new("a:noFill")))?;
    w.write_event(Event::End(BytesEnd::new("p:spPr")))?;

    // Text body
    w.write_event(Event::Start(BytesStart::new("p:txBody")))?;

    let mut body_pr = BytesStart::new("a:bodyPr");
    body_pr.push_attribute(("wrap", "square"));
    w.write_event(Event::Empty(body_pr))?;

    w.write_event(Event::Empty(BytesStart::new("a:lstStyle")))?;

    // Paragraph
    w.write_event(Event::Start(BytesStart::new("a:p")))?;
    w.write_event(Event::Start(BytesStart::new("a:r")))?;

    // Run properties
    let font_size_hundredths = ((block.font_size * scale) * 100.0) as i64;
    let sz_str = font_size_hundredths.to_string();
    let mut rpr = BytesStart::new("a:rPr");
    rpr.push_attribute(("lang", "en-US"));
    rpr.push_attribute(("sz", sz_str.as_str()));

    let is_bold = block.font_name.contains("Bold") || block.font_name.contains("bold");
    let is_italic = block.font_name.contains("Italic")
        || block.font_name.contains("italic")
        || block.font_name.contains("Oblique");

    if is_bold {
        rpr.push_attribute(("b", "1"));
    }
    if is_italic {
        rpr.push_attribute(("i", "1"));
    }

    w.write_event(Event::Empty(rpr))?;

    w.write_event(Event::Start(BytesStart::new("a:t")))?;
    w.write_event(Event::Text(BytesText::new(&block.text)))?;
    w.write_event(Event::End(BytesEnd::new("a:t")))?;

    w.write_event(Event::End(BytesEnd::new("a:r")))?;
    w.write_event(Event::End(BytesEnd::new("a:p")))?;

    w.write_event(Event::End(BytesEnd::new("p:txBody")))?;
    w.write_event(Event::End(BytesEnd::new("p:sp")))?;

    Ok(())
}

/// Write an image as a PPTX picture shape.
fn write_image_shape(
    w: &mut Writer<&mut Cursor<Vec<u8>>>,
    img: &PptxImage,
    rid: &str,
    shape_id: u32,
) -> Result<()> {
    let dpi = 96.0f64;
    let cx = (img.width as f64 / dpi * 914400.0) as i64;
    let cy = (img.height as f64 / dpi * 914400.0) as i64;

    w.write_event(Event::Start(BytesStart::new("p:pic")))?;

    // Non-visual properties
    w.write_event(Event::Start(BytesStart::new("p:nvPicPr")))?;
    let name = format!("Image {shape_id}");
    let id_str = shape_id.to_string();
    let mut cnv = BytesStart::new("p:cNvPr");
    cnv.push_attribute(("id", id_str.as_str()));
    cnv.push_attribute(("name", name.as_str()));
    w.write_event(Event::Empty(cnv))?;

    let cnv_pic = BytesStart::new("p:cNvPicPr");
    w.write_event(Event::Start(cnv_pic))?;
    w.write_event(Event::Empty(BytesStart::new("a:picLocks")))?;
    w.write_event(Event::End(BytesEnd::new("p:cNvPicPr")))?;

    w.write_event(Event::Empty(BytesStart::new("p:nvPr")))?;
    w.write_event(Event::End(BytesEnd::new("p:nvPicPr")))?;

    // Blip fill
    w.write_event(Event::Start(BytesStart::new("p:blipFill")))?;
    let mut blip = BytesStart::new("a:blip");
    blip.push_attribute(("r:embed", rid));
    w.write_event(Event::Empty(blip))?;

    w.write_event(Event::Start(BytesStart::new("a:stretch")))?;
    w.write_event(Event::Empty(BytesStart::new("a:fillRect")))?;
    w.write_event(Event::End(BytesEnd::new("a:stretch")))?;
    w.write_event(Event::End(BytesEnd::new("p:blipFill")))?;

    // Shape properties
    w.write_event(Event::Start(BytesStart::new("p:spPr")))?;
    write_xfrm_positioned(w, 0, 0, cx, cy)?;

    let mut prst = BytesStart::new("a:prstGeom");
    prst.push_attribute(("prst", "rect"));
    w.write_event(Event::Start(prst))?;
    w.write_event(Event::Empty(BytesStart::new("a:avLst")))?;
    w.write_event(Event::End(BytesEnd::new("a:prstGeom")))?;
    w.write_event(Event::End(BytesEnd::new("p:spPr")))?;

    w.write_event(Event::End(BytesEnd::new("p:pic")))?;
    Ok(())
}

// --- Utility functions ---

fn empty_with_attrs(
    w: &mut Writer<&mut Cursor<Vec<u8>>>,
    tag: &str,
    attrs: &[(&str, &str)],
) -> Result<()> {
    let mut el = BytesStart::new(tag);
    for (k, v) in attrs {
        el.push_attribute((*k, *v));
    }
    w.write_event(Event::Empty(el))?;
    Ok(())
}

fn write_xfrm_group(w: &mut Writer<&mut Cursor<Vec<u8>>>) -> Result<()> {
    w.write_event(Event::Start(BytesStart::new("a:xfrm")))?;

    empty_with_attrs(w, "a:off", &[("x", "0"), ("y", "0")])?;
    empty_with_attrs(w, "a:ext", &[("cx", "0"), ("cy", "0")])?;
    empty_with_attrs(w, "a:chOff", &[("x", "0"), ("y", "0")])?;
    empty_with_attrs(w, "a:chExt", &[("cx", "0"), ("cy", "0")])?;

    w.write_event(Event::End(BytesEnd::new("a:xfrm")))?;
    Ok(())
}

fn write_xfrm_positioned(
    w: &mut Writer<&mut Cursor<Vec<u8>>>,
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
) -> Result<()> {
    w.write_event(Event::Start(BytesStart::new("a:xfrm")))?;

    let xs = x.to_string();
    let ys = y.to_string();
    let cxs = cx.to_string();
    let cys = cy.to_string();

    empty_with_attrs(w, "a:off", &[("x", &xs), ("y", &ys)])?;
    empty_with_attrs(w, "a:ext", &[("cx", &cxs), ("cy", &cys)])?;

    w.write_event(Event::End(BytesEnd::new("a:xfrm")))?;
    Ok(())
}

fn image_ext(content_type: &str) -> &str {
    match content_type {
        "image/jpeg" => "jpeg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/tiff" => "tiff",
        _ => "png",
    }
}

/// Convert ExtractedImage to PptxImage.
pub fn extracted_to_pptx_image(img: &ExtractedImage, idx: usize) -> PptxImage {
    let (content_type, ext) = match img.filter {
        ImageFilter::Jpeg => ("image/jpeg", "jpeg"),
        _ => ("image/png", "png"),
    };

    PptxImage {
        data: img.data.clone(),
        width: img.width,
        height: img.height,
        content_type: content_type.to_string(),
        filename: format!("image{idx}.{ext}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_minimal_pptx() {
        let slides = vec![SlideData {
            text_blocks: vec![TextBlock {
                text: "Hello World".to_string(),
                page: 1,
                bbox: [72.0, 700.0, 200.0, 712.0],
                font_name: "F1".to_string(),
                font_size: 12.0,
            }],
            images: vec![],
            page_width: 612.0,
            page_height: 792.0,
        }];

        let mut output = Vec::new();
        write_pptx(&slides, &mut output).unwrap();
        assert!(output.len() > 100);
        assert_eq!(&output[0..2], b"PK");
    }

    #[test]
    fn write_empty_pptx() {
        let slides = vec![SlideData {
            text_blocks: vec![],
            images: vec![],
            page_width: 612.0,
            page_height: 792.0,
        }];

        let mut output = Vec::new();
        write_pptx(&slides, &mut output).unwrap();
        assert!(output.len() > 100);
    }

    #[test]
    fn write_multi_slide_pptx() {
        let slides = vec![
            SlideData {
                text_blocks: vec![TextBlock {
                    text: "Slide 1".to_string(),
                    page: 1,
                    bbox: [72.0, 700.0, 200.0, 712.0],
                    font_name: "F1".to_string(),
                    font_size: 24.0,
                }],
                images: vec![],
                page_width: 612.0,
                page_height: 792.0,
            },
            SlideData {
                text_blocks: vec![TextBlock {
                    text: "Slide 2".to_string(),
                    page: 2,
                    bbox: [72.0, 700.0, 200.0, 712.0],
                    font_name: "F1".to_string(),
                    font_size: 18.0,
                }],
                images: vec![],
                page_width: 612.0,
                page_height: 792.0,
            },
        ];

        let mut output = Vec::new();
        write_pptx(&slides, &mut output).unwrap();
        assert!(output.len() > 100);
    }
}
