//! Extraction tests over PPTX packages built in memory.
//!
//! Each test assembles a small `.pptx` with [`PptxBuilder`] -- no downloads,
//! no fixture files -- and asserts the exact blocks it must produce. Every
//! assertion names the feature's own values (titles, levels, spans, prefixes),
//! so removing the feature fails the test rather than weakening it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::BlockKind;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// One text run.
fn run(text: &str) -> String {
    format!("<a:r><a:t>{text}</a:t></a:r>")
}

/// One paragraph: runs plus an optional full `<a:pPr>` element.
fn para(runs: &str, ppr: &str) -> String {
    format!("<a:p>{ppr}{runs}</a:p>")
}

/// Bullet properties for a level: an unordered bullet, or an ordered one.
fn bullet(lvl: u8, ordered_start: Option<u32>) -> String {
    let lvl = if lvl == 0 {
        String::new()
    } else {
        format!(" lvl=\"{lvl}\"")
    };
    match ordered_start {
        Some(start) => {
            format!("<a:pPr{lvl}><a:buAutoNum type=\"arabicPeriod\" startAt=\"{start}\"/></a:pPr>")
        }
        None => format!("<a:pPr{lvl}><a:buChar char=\"&#8226;\"/></a:pPr>"),
    }
}

/// A list-style bullet at level 1: `<a:buChar/>`, `<a:buNone/>` or
/// `<a:buAutoNum/>` inside `lvl1pPr`.
fn lvl_bullet(inner: &str) -> String {
    format!("<a:lstStyle><a:lvl1pPr>{inner}</a:lvl1pPr></a:lstStyle>")
}

/// A text shape with a placeholder type and/or index and explicit list-style
/// XML. Unpositioned: inheritance tests read document order, not geometry.
fn inherit_shape(
    id: u32,
    ph_type: Option<&str>,
    ph_idx: Option<&str>,
    lst: &str,
    paras: &str,
) -> String {
    let mut ph = String::new();
    if ph_type.is_some() || ph_idx.is_some() {
        let typ = ph_type.map_or(String::new(), |t| format!(" type=\"{t}\""));
        let idx = ph_idx.map_or(String::new(), |i| format!(" idx=\"{i}\""));
        ph = format!("<p:nvPr><p:ph{typ}{idx}/></p:nvPr>");
    }
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Shape {id}\"/>{ph}</p:nvSpPr>\
         <p:spPr/><p:txBody><a:bodyPr/>{lst}{paras}</p:txBody></p:sp>"
    )
}

/// A positioned text shape, optionally a placeholder.
fn text_shape(
    id: u32,
    placeholder: Option<&str>,
    xfrm: Option<(i64, i64, i64, i64)>,
    paras: &str,
) -> String {
    let ph = placeholder.map_or(String::new(), |kind| {
        format!("<p:nvPr><p:ph type=\"{kind}\"/></p:nvPr>")
    });
    let xfrm = xfrm.map_or(String::new(), |(x, y, w, h)| {
        format!("<a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm>")
    });
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Shape {id}\"/>{ph}</p:nvSpPr>\
         <p:spPr>{xfrm}</p:spPr>\
         <p:txBody><a:bodyPr/><a:lstStyle/>{paras}</p:txBody></p:sp>"
    )
}

/// A picture shape referencing `embed`.
fn pic_shape(
    id: u32,
    descr: &str,
    name: &str,
    embed: &str,
    xfrm: Option<(i64, i64, i64, i64)>,
) -> String {
    let xfrm = xfrm.map_or(String::new(), |(x, y, w, h)| {
        format!("<a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm>")
    });
    format!(
        "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"{name}\" descr=\"{descr}\"/>\
         <p:cNvPicPr/><p:nvPr/></p:nvPicPr>\
         <p:blipFill><a:blip r:embed=\"{embed}\"/></p:blipFill>\
         <p:spPr>{xfrm}</p:spPr></p:pic>"
    )
}

/// A group shape over nested shapes, offset at `(x, y)`. The children sit
/// directly under `p:grpSp`: a group has no `p:spTree` of its own.
fn group_shape(id: u32, x: i64, y: i64, shapes: &str) -> String {
    format!(
        "<p:grpSp><p:nvGrpSpPr><p:cNvPr id=\"{id}\" name=\"Group\"/>\
         <p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
         <p:grpSpPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/>\
         <a:ext cx=\"9144000\" cy=\"6858000\"/></a:xfrm></p:grpSpPr>\
         {shapes}</p:grpSp>"
    )
}

/// One table cell with optional spans.
fn tc(text: &str, grid_span: u16, row_span: u16, merged: bool) -> String {
    let mut attrs = String::new();
    if grid_span > 1 {
        attrs.push_str(&format!(" gridSpan=\"{grid_span}\""));
    }
    if row_span > 1 {
        attrs.push_str(&format!(" rowSpan=\"{row_span}\""));
    }
    if merged {
        attrs.push_str(" vMerge=\"1\"");
    }
    format!(
        "<a:tc{attrs}><a:txBody><a:bodyPr/><a:lstStyle/>{}</a:txBody><a:tcPr/></a:tc>",
        para(&run(text), "")
    )
}

/// A table graphic frame.
fn table_shape(id: u32, rows: &str) -> String {
    format!(
        "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{id}\" name=\"Table\"/>\
         <p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>\
         <p:xfrm><a:off x=\"914400\" y=\"2743200\"/>\
         <a:ext cx=\"7315200\" cy=\"2286000\"/></p:xfrm>\
         <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\">\
         <a:tbl><a:tblPr firstRow=\"1\"/>\
         <a:tblGrid><a:gridCol w=\"2438400\"/><a:gridCol w=\"2438400\"/>\
         <a:gridCol w=\"2438400\"/></a:tblGrid>{rows}</a:tbl>\
         </a:graphicData></a:graphic></p:graphicFrame>"
    )
}

/// A slide part over `p:spTree` children.
fn slide_xml(shapes: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <p:sld xmlns:p=\"{P}\" xmlns:a=\"{A}\" xmlns:r=\"{R}\">\
         <p:cSld><p:spTree>\
         <p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
         <p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/>\
         <a:ext cx=\"9144000\" cy=\"6858000\"/></a:xfrm></p:grpSpPr>\
         {shapes}</p:spTree></p:cSld></p:sld>"
    )
}

/// A slide-layout part over `p:spTree` children.
fn layout_xml(shapes: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <p:sldLayout xmlns:p=\"{P}\" xmlns:a=\"{A}\" xmlns:r=\"{R}\">\
         <p:cSld><p:spTree>{shapes}</p:spTree></p:cSld></p:sldLayout>"
    )
}

/// A slide-master part over one `p:txStyles` body.
fn master_xml(tx_styles: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <p:sldMaster xmlns:p=\"{P}\" xmlns:a=\"{A}\" xmlns:r=\"{R}\">\
         <p:txStyles>{tx_styles}</p:txStyles></p:sldMaster>"
    )
}

/// One master text style (`titleStyle`, `bodyStyle`, `otherStyle`) with a
/// level-1 bullet.
fn master_style(name: &str, inner: &str) -> String {
    format!("<p:{name}><a:lvl1pPr>{inner}</a:lvl1pPr></p:{name}>")
}

/// A notes-slide part over `p:spTree` children.
fn notes_xml(shapes: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <p:notes xmlns:p=\"{P}\" xmlns:a=\"{A}\" xmlns:r=\"{R}\">\
         <p:cSld><p:spTree>\
         <p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
         <p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/>\
         <a:ext cx=\"9144000\" cy=\"6858000\"/></a:xfrm></p:grpSpPr>\
         {shapes}</p:spTree></p:cSld></p:notes>"
    )
}

/// A `.rels` part from `(id, type, target)` entries.
fn rels_xml(entries: &[(&str, &str, &str)]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for (id, kind, target) in entries {
        out.push_str(&format!(
            "<Relationship Id=\"{id}\" Type=\"{R}/{kind}\" Target=\"{target}\"/>"
        ));
    }
    out.push_str("</Relationships>");
    out
}

/// Builds a minimal `.pptx` in memory: content types, presentation, slides.
struct PptxBuilder {
    /// `(file name, xml)` slide parts.
    slides: Vec<(String, String)>,
    /// Slide files in presentation order; defaults to insertion order.
    order: Option<Vec<String>>,
    /// `(slide file, rels xml)` per-slide relationships.
    slide_rels: Vec<(String, String)>,
    /// `(file name, xml)` notes parts.
    notes: Vec<(String, String)>,
    /// `(file name, xml)` layout parts.
    layouts: Vec<(String, String)>,
    /// `(layout file, rels xml)` per-layout relationships.
    layout_rels: Vec<(String, String)>,
    /// `(file name, xml)` master parts.
    masters: Vec<(String, String)>,
}

impl PptxBuilder {
    fn new() -> Self {
        Self {
            slides: Vec::new(),
            order: None,
            slide_rels: Vec::new(),
            notes: Vec::new(),
            layouts: Vec::new(),
            layout_rels: Vec::new(),
            masters: Vec::new(),
        }
    }

    fn slide(mut self, file: &str, xml: String) -> Self {
        self.slides.push((file.to_string(), xml));
        self
    }

    fn order(mut self, files: &[&str]) -> Self {
        self.order = Some(files.iter().map(|s| s.to_string()).collect());
        self
    }

    fn rels(mut self, slide_file: &str, xml: String) -> Self {
        self.slide_rels.push((slide_file.to_string(), xml));
        self
    }

    fn notes(mut self, file: &str, xml: String) -> Self {
        self.notes.push((file.to_string(), xml));
        self
    }

    fn layout(mut self, file: &str, xml: String) -> Self {
        self.layouts.push((file.to_string(), xml));
        self
    }

    fn layout_rels(mut self, layout_file: &str, xml: String) -> Self {
        self.layout_rels.push((layout_file.to_string(), xml));
        self
    }

    fn master(mut self, file: &str, xml: String) -> Self {
        self.masters.push((file.to_string(), xml));
        self
    }

    fn build(self) -> Vec<u8> {
        let order = self
            .order
            .unwrap_or_else(|| self.slides.iter().map(|(file, _)| file.clone()).collect());
        let mut sld_ids = String::new();
        let mut pres_rels = Vec::new();
        for (n, file) in order.iter().enumerate() {
            let rid = format!("rId{}", n + 2);
            sld_ids.push_str(&format!(
                "<p:sldId id=\"{}\" r:id=\"{rid}\"/>",
                256 + n as u32
            ));
            pres_rels.push((rid, file.clone()));
        }
        let presentation = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <p:presentation xmlns:p=\"{P}\" xmlns:r=\"{R}\">\
             <p:sldSz cx=\"9144000\" cy=\"6858000\"/>\
             <p:sldIdLst>{sld_ids}</p:sldIdLst></p:presentation>"
        );
        let entries: Vec<(&str, &str, String)> = pres_rels
            .iter()
            .map(|(rid, file)| (rid.as_str(), "slide", format!("slides/{file}")))
            .collect();
        let refs: Vec<(&str, &str, &str)> = entries
            .iter()
            .map(|(rid, kind, target)| (*rid, *kind, target.as_str()))
            .collect();
        let pres_rels_xml = rels_xml(&refs);
        let mut types = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
             <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
             <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
             <Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>",
        );
        for (file, _) in &self.slides {
            types.push_str(&format!(
                "<Override PartName=\"/ppt/slides/{file}\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
            ));
        }
        types.push_str("</Types>");
        let root_rels = rels_xml(&[("rId1", "officeDocument", "ppt/presentation.xml")]);

        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = SimpleFileOptions::default();
        let mut add = |name: &str, data: &str| {
            zip.start_file(name, options).unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        };
        add("[Content_Types].xml", &types);
        add("_rels/.rels", &root_rels);
        add("ppt/presentation.xml", &presentation);
        add("ppt/_rels/presentation.xml.rels", &pres_rels_xml);
        for (file, xml) in &self.slides {
            add(&format!("ppt/slides/{file}"), xml);
        }
        for (file, xml) in &self.slide_rels {
            add(&format!("ppt/slides/_rels/{file}.rels"), xml);
        }
        for (file, xml) in &self.notes {
            add(&format!("ppt/notesSlides/{file}"), xml);
        }
        for (file, xml) in &self.layouts {
            add(&format!("ppt/slideLayouts/{file}"), xml);
        }
        for (file, xml) in &self.layout_rels {
            add(&format!("ppt/slideLayouts/_rels/{file}.rels"), xml);
        }
        for (file, xml) in &self.masters {
            add(&format!("ppt/slideMasters/{file}"), xml);
        }
        zip.finish().unwrap().into_inner()
    }
}

/// Titles, slide order, untitled slides opening with their body, document
/// order, image references, slide numbers as text and the Markdown
/// serialization.
#[test]
fn title_order_fallback_images_and_markdown() {
    // slide2 comes first in presentation order though its file number is
    // higher; slide1 has no title placeholder at all.
    let slide2 = slide_xml(&format!(
        "{}{}{}{}{}",
        text_shape(
            2,
            Some("title"),
            Some((914400, 685800, 7315200, 1097280)),
            &para(&run("Quarterly Review"), ""),
        ),
        // Lower on the slide but first in the file: document order emits
        // it before the upper shape, whatever the geometry says.
        text_shape(
            3,
            None,
            Some((914400, 4114800, 7315200, 548640)),
            &para(&run("Lower box"), ""),
        ),
        text_shape(
            4,
            None,
            Some((914400, 2743200, 7315200, 548640)),
            &para(&run("Upper box"), ""),
        ),
        // A slide-number placeholder: the number on the slide is text.
        text_shape(5, Some("sldNum"), None, &para(&run("2"), "")),
        pic_shape(
            6,
            "Revenue chart",
            "Picture 1",
            "rId1",
            Some((914400, 4800600, 3657600, 1097280)),
        ),
    ));
    let slide1 = slide_xml(&text_shape(
        2,
        None,
        Some((914400, 1371600, 7315200, 548640)),
        &para(&run("Untitled body"), ""),
    ));
    let pptx = PptxBuilder::new()
        .slide("slide1.xml", slide1)
        .slide("slide2.xml", slide2)
        .order(&["slide2.xml", "slide1.xml"])
        .rels(
            "slide2.xml",
            rels_xml(&[("rId1", "image", "../media/chart.png")]),
        )
        .build();

    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();

    // Presentation order wins over file numbers: the titled slide is page 0.
    assert!(matches!(blocks[0].kind, BlockKind::Heading { level: 1 }));
    assert_eq!(blocks[0].text, "Quarterly Review");
    assert_eq!(blocks[0].page, 0);
    // EMU to points at 12700 per point, y flipped over a 540pt slide:
    // x=72, w=576, y-down=54, h=86.4 -> [72, 399.6, 648, 486].
    let bbox = blocks[0].bbox;
    assert!((bbox[0] - 72.0).abs() < 1e-6, "x0: {bbox:?}");
    assert!((bbox[1] - 399.6).abs() < 1e-6, "y0: {bbox:?}");
    assert!((bbox[2] - 648.0).abs() < 1e-6, "x1: {bbox:?}");
    assert!((bbox[3] - 486.0).abs() < 1e-6, "y1: {bbox:?}");

    // Document order, regardless of geometry.
    let page0: Vec<&str> = blocks
        .iter()
        .filter(|b| b.page == 0)
        .map(|b| b.text.as_str())
        .collect();
    assert_eq!(
        page0,
        [
            "Quarterly Review",
            "Lower box",
            "Upper box",
            "2",
            "![Revenue chart](ppt/media/chart.png)",
        ]
    );
    // The slide number reads as a paragraph in document order.
    let number = blocks.iter().find(|b| b.text == "2").unwrap();
    assert!(matches!(number.kind, BlockKind::Paragraph));
    assert_eq!(number.page, 0);

    // The untitled slide opens with its body: no synthetic heading.
    let page1: Vec<&str> = blocks
        .iter()
        .filter(|b| b.page == 1)
        .map(|b| b.text.as_str())
        .collect();
    assert_eq!(page1, ["Untitled body"]);

    // The blocks serialize through the engine's Markdown.
    let markdown = pptx_extract::extract_markdown(&pptx).unwrap();
    assert!(markdown.contains("# Quarterly Review"), "{markdown}");
    assert!(!markdown.contains("Slide 2"), "{markdown}");
    // The slide boundary stays as the page marker.
    assert!(markdown.contains("<!-- page 1 -->"), "{markdown}");
    assert!(
        markdown.contains("![Revenue chart](ppt/media/chart.png)"),
        "{markdown}"
    );
}

/// Every non-empty paragraph of a title placeholder is a heading of its
/// own; a bullet on one stays a heading, and `ctrTitle` reads as `title`.
#[test]
fn title_paragraphs_are_headings_each() {
    let slide1 = slide_xml(&text_shape(
        2,
        Some("title"),
        None,
        &format!(
            "{}{}{}",
            para(&run("Harbour survey"), ""),
            para("", ""),
            para(&run("Tide tables"), &bullet(0, None)),
        ),
    ));
    let slide2 = slide_xml(&text_shape(
        2,
        Some("ctrTitle"),
        None,
        &format!(
            "{}{}",
            para(&run("First line"), ""),
            para(&run("Second line"), ""),
        ),
    ));
    let pptx = PptxBuilder::new()
        .slide("slide1.xml", slide1)
        .slide("slide2.xml", slide2)
        .order(&["slide1.xml", "slide2.xml"])
        .build();
    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    assert_eq!(blocks.len(), 4);
    for (block, text) in
        blocks
            .iter()
            .zip(["Harbour survey", "Tide tables", "First line", "Second line"])
    {
        assert!(
            matches!(block.kind, BlockKind::Heading { level: 1 }),
            "{block:?}"
        );
        assert_eq!(block.text, text);
    }
    assert_eq!(blocks[0].page, 0);
    assert_eq!(blocks[1].page, 0);
    assert_eq!(blocks[2].page, 1);
    assert_eq!(blocks[3].page, 1);
}

/// Bullets become list items with their levels; an `a:buAutoNum` keeps its
/// `startAt` as the ordinal.
#[test]
fn bullet_levels_and_numbering() {
    let body = format!(
        "{}{}{}{}",
        para(&run("First"), &bullet(0, None)),
        para(&run("Nested"), &bullet(1, None)),
        para(&run("Deep"), &bullet(2, None)),
        para(&run("Numbered fifth"), &bullet(0, Some(5))),
    );
    let slide = slide_xml(&format!(
        "{}{}",
        text_shape(2, Some("title"), None, &para(&run("Agenda"), "")),
        text_shape(3, None, None, &body),
    ));
    let pptx = PptxBuilder::new().slide("slide1.xml", slide).build();

    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    let items: Vec<(BlockKind, &str)> = blocks[1..]
        .iter()
        .map(|b| (b.kind, b.text.as_str()))
        .collect();
    assert_eq!(
        items,
        [
            (
                BlockKind::ListItem {
                    level: 0,
                    ordinal: None
                },
                "First"
            ),
            (
                BlockKind::ListItem {
                    level: 1,
                    ordinal: None
                },
                "Nested"
            ),
            (
                BlockKind::ListItem {
                    level: 2,
                    ordinal: None
                },
                "Deep"
            ),
            (
                BlockKind::ListItem {
                    level: 0,
                    ordinal: Some(5)
                },
                "Numbered fifth"
            ),
        ]
    );
}

/// A shape's `lstStyle` bullets its plain paragraphs; a paragraph `buNone`
/// beats the shape bullet.
#[test]
fn shape_list_style_bullets_and_paragraph_bunone_wins() {
    let body = format!(
        "{}{}",
        para(&run("Shaped bullet item"), ""),
        para(
            &run("Plain despite the shape bullet"),
            "<a:pPr><a:buNone/></a:pPr>",
        ),
    );
    let slide = slide_xml(&format!(
        "{}{}",
        text_shape(2, Some("title"), None, &para(&run("Agenda"), "")),
        inherit_shape(
            3,
            Some("body"),
            Some("1"),
            &lvl_bullet("<a:buChar char=\"&#8226;\"/>"),
            &body,
        ),
    ));
    let pptx = PptxBuilder::new().slide("slide1.xml", slide).build();

    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    assert_eq!(blocks.len(), 3);
    assert!(matches!(
        blocks[1].kind,
        BlockKind::ListItem {
            level: 0,
            ordinal: None
        }
    ));
    assert_eq!(blocks[1].text, "Shaped bullet item");
    assert!(matches!(blocks[2].kind, BlockKind::Paragraph));
    assert_eq!(blocks[2].text, "Plain despite the shape bullet");
}

/// Layout placeholders inherit by index; a layout `buNone` beats a master
/// bullet, and a placeholder without a layout entry falls through to the
/// master's `bodyStyle`.
#[test]
fn layout_placeholders_inherit_by_index_and_beat_the_master() {
    let slide = slide_xml(&format!(
        "{}{}{}{}",
        text_shape(2, Some("title"), None, &para(&run("Agenda"), "")),
        inherit_shape(
            3,
            Some("body"),
            Some("1"),
            "<a:lstStyle/>",
            &para(&run("Layout-level bullet"), ""),
        ),
        inherit_shape(
            4,
            Some("body"),
            Some("2"),
            "<a:lstStyle/>",
            &para(&run("Layout silence beats master bullets"), ""),
        ),
        inherit_shape(
            5,
            Some("body"),
            Some("3"),
            "<a:lstStyle/>",
            &para(&run("Master body bullet"), ""),
        ),
    ));
    let layout = layout_xml(&format!(
        "{}{}",
        inherit_shape(
            2,
            Some("body"),
            Some("1"),
            &lvl_bullet("<a:buChar char=\"&#8226;\"/>"),
            "",
        ),
        inherit_shape(3, Some("body"), Some("2"), &lvl_bullet("<a:buNone/>"), "",),
    ));
    let master = master_xml(&master_style("bodyStyle", "<a:buChar char=\"&#8226;\"/>"));
    let pptx = PptxBuilder::new()
        .slide("slide1.xml", slide)
        .rels(
            "slide1.xml",
            rels_xml(&[("rId1", "slideLayout", "../slideLayouts/layout1.xml")]),
        )
        .layout("layout1.xml", layout)
        .layout_rels(
            "layout1.xml",
            rels_xml(&[("rId1", "slideMaster", "../slideMasters/master1.xml")]),
        )
        .master("master1.xml", master)
        .build();

    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    assert_eq!(blocks.len(), 4);
    assert!(matches!(
        blocks[1].kind,
        BlockKind::ListItem {
            level: 0,
            ordinal: None
        }
    ));
    assert_eq!(blocks[1].text, "Layout-level bullet");
    assert!(matches!(blocks[2].kind, BlockKind::Paragraph));
    assert_eq!(blocks[2].text, "Layout silence beats master bullets");
    assert!(matches!(
        blocks[3].kind,
        BlockKind::ListItem {
            level: 0,
            ordinal: None
        }
    ));
    assert_eq!(blocks[3].text, "Master body bullet");
}

/// Footer, date and slide-number placeholders are paragraphs: a bullet on
/// one stays a paragraph, the slide number reads as text, and a `slidenum`
/// field keeps its cached runs.
#[test]
fn footer_date_and_slide_number_are_paragraphs() {
    let slide = slide_xml(&format!(
        "{}{}{}{}",
        inherit_shape(
            2,
            Some("ftr"),
            Some("11"),
            "<a:lstStyle/>",
            &para(&run("Harbour survey 2024"), &bullet(0, None)),
        ),
        inherit_shape(
            3,
            Some("dt"),
            Some("10"),
            "<a:lstStyle/>",
            &para(&run("October 2024"), ""),
        ),
        inherit_shape(
            4,
            Some("sldNum"),
            Some("12"),
            "<a:lstStyle/>",
            &para(&run("3"), ""),
        ),
        inherit_shape(
            5,
            None,
            None,
            "<a:lstStyle/>",
            &para(
                &format!(
                    "{}<a:fld type=\"slidenum\"><a:r><a:t>3</a:t></a:r></a:fld>",
                    run("Page "),
                ),
                "",
            ),
        ),
    ));
    let pptx = PptxBuilder::new().slide("slide1.xml", slide).build();

    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    assert_eq!(blocks.len(), 4);
    for (block, text) in blocks
        .iter()
        .zip(["Harbour survey 2024", "October 2024", "3", "Page 3"])
    {
        assert!(matches!(block.kind, BlockKind::Paragraph), "{block:?}");
        assert_eq!(block.text, text);
    }
}

/// Footer-like placeholders inherit only through their own chain: a
/// same-type layout bullet applies, but a body placeholder sharing the
/// index and the master's `bodyStyle` never leak in.
#[test]
fn footer_placeholders_inherit_only_through_their_own_chain() {
    let slide = slide_xml(&format!(
        "{}{}{}",
        inherit_shape(
            2,
            Some("ftr"),
            Some("11"),
            "<a:lstStyle/>",
            &para(&run("Index never borrows body bullets"), ""),
        ),
        inherit_shape(
            3,
            Some("dt"),
            Some("10"),
            "<a:lstStyle/>",
            &para(&run("Same-type layout bullet applies"), ""),
        ),
        inherit_shape(
            4,
            Some("sldNum"),
            Some("12"),
            "<a:lstStyle/>",
            &para(&run("Master body never applies"), ""),
        ),
    ));
    let layout = layout_xml(&format!(
        "{}{}",
        inherit_shape(
            2,
            Some("body"),
            Some("11"),
            &lvl_bullet("<a:buChar char=\"&#8226;\"/>"),
            "",
        ),
        inherit_shape(
            3,
            Some("dt"),
            Some("10"),
            &lvl_bullet("<a:buChar char=\"&#8226;\"/>"),
            "",
        ),
    ));
    let master = master_xml(&format!(
        "{}{}",
        master_style("bodyStyle", "<a:buChar char=\"&#8226;\"/>"),
        "<p:otherStyle/>",
    ));
    let pptx = PptxBuilder::new()
        .slide("slide1.xml", slide)
        .rels(
            "slide1.xml",
            rels_xml(&[("rId1", "slideLayout", "../slideLayouts/layout1.xml")]),
        )
        .layout("layout1.xml", layout)
        .layout_rels(
            "layout1.xml",
            rels_xml(&[("rId1", "slideMaster", "../slideMasters/master1.xml")]),
        )
        .master("master1.xml", master)
        .build();

    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    assert_eq!(blocks.len(), 3);
    assert!(matches!(blocks[0].kind, BlockKind::Paragraph));
    assert_eq!(blocks[0].text, "Index never borrows body bullets");
    assert!(matches!(
        blocks[1].kind,
        BlockKind::ListItem {
            level: 0,
            ordinal: None
        }
    ));
    assert_eq!(blocks[1].text, "Same-type layout bullet applies");
    assert!(matches!(blocks[2].kind, BlockKind::Paragraph));
    assert_eq!(blocks[2].text, "Master body never applies");
}

/// Free text boxes inherit from the master's `otherStyle` only: a `bodyStyle`
/// bullet leaves them plain.
#[test]
fn free_boxes_read_only_other_style() {
    let slide = slide_xml(&inherit_shape(
        2,
        None,
        None,
        "<a:lstStyle/>",
        &para(&run("Free box, master says body"), ""),
    ));
    let master = master_xml(&format!(
        "{}{}",
        master_style("bodyStyle", "<a:buChar char=\"&#8226;\"/>"),
        "<p:otherStyle/>",
    ));
    let pptx = PptxBuilder::new()
        .slide("slide1.xml", slide)
        .rels(
            "slide1.xml",
            rels_xml(&[("rId1", "slideLayout", "../slideLayouts/layout1.xml")]),
        )
        .layout("layout1.xml", layout_xml(""))
        .layout_rels(
            "layout1.xml",
            rels_xml(&[("rId1", "slideMaster", "../slideMasters/master1.xml")]),
        )
        .master("master1.xml", master)
        .build();
    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    assert_eq!(blocks.len(), 1);
    assert!(matches!(blocks[0].kind, BlockKind::Paragraph));

    let slide = slide_xml(&inherit_shape(
        2,
        None,
        None,
        "<a:lstStyle/>",
        &para(&run("Free box master bullet"), ""),
    ));
    let master = master_xml(&master_style("otherStyle", "<a:buChar char=\"&#8226;\"/>"));
    let pptx = PptxBuilder::new()
        .slide("slide1.xml", slide)
        .rels(
            "slide1.xml",
            rels_xml(&[("rId1", "slideLayout", "../slideLayouts/layout1.xml")]),
        )
        .layout("layout1.xml", layout_xml(""))
        .layout_rels(
            "layout1.xml",
            rels_xml(&[("rId1", "slideMaster", "../slideMasters/master1.xml")]),
        )
        .master("master1.xml", master)
        .build();
    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    assert_eq!(blocks.len(), 1);
    assert!(matches!(
        blocks[0].kind,
        BlockKind::ListItem {
            level: 0,
            ordinal: None
        }
    ));
    assert_eq!(blocks[0].text, "Free box master bullet");
}

/// Tables emit row-major cells with `gridSpan`/`rowSpan` as spans; merged
/// cells marked `vMerge` emit nothing.
#[test]
fn table_with_merged_cell() {
    let rows = format!(
        "<a:tr>{}{}</a:tr><a:tr>{}{}{}</a:tr>",
        tc("H1-H2", 2, 1, false),
        tc("Side", 1, 2, false),
        tc("Q1", 1, 1, false),
        tc("Q2", 1, 1, false),
        // Covered by `Side`'s row span: must not become a block.
        tc("SHADOW", 1, 1, true),
    );
    let slide = slide_xml(&format!(
        "{}{}",
        text_shape(2, Some("title"), None, &para(&run("Roadmap"), "")),
        table_shape(3, &rows),
    ));
    let pptx = PptxBuilder::new().slide("slide1.xml", slide).build();

    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    assert_eq!(blocks[0].text, "Roadmap");
    let cells: Vec<(BlockKind, &str)> = blocks[1..]
        .iter()
        .map(|b| (b.kind, b.text.as_str()))
        .collect();
    assert_eq!(
        cells,
        [
            (
                BlockKind::TableCell {
                    row: 0,
                    column: 0,
                    row_span: 1,
                    column_span: 2,
                    header: true,
                },
                "H1-H2"
            ),
            (
                BlockKind::TableCell {
                    row: 0,
                    column: 2,
                    row_span: 2,
                    column_span: 1,
                    header: true,
                },
                "Side"
            ),
            (
                BlockKind::TableCell {
                    row: 1,
                    column: 0,
                    row_span: 1,
                    column_span: 1,
                    header: false,
                },
                "Q1"
            ),
            (
                BlockKind::TableCell {
                    row: 1,
                    column: 1,
                    row_span: 1,
                    column_span: 1,
                    header: false,
                },
                "Q2"
            ),
        ]
    );
}

/// Speaker notes are not body text: a notes slide in the package emits
/// nothing, and the slide itself still extracts.
#[test]
fn speaker_notes_are_not_body_text() {
    let slide = slide_xml(&text_shape(
        2,
        Some("title"),
        None,
        &para(&run("Intro"), ""),
    ));
    let notes = notes_xml(&format!(
        "{}{}{}",
        text_shape(
            2,
            Some("body"),
            None,
            &para(&run("Remember to mention Q3 results."), ""),
        ),
        text_shape(3, Some("hdr"), None, &para(&run("DECK-HDR"), "")),
        text_shape(4, Some("title"), None, &para(&run("Intro"), "")),
    ));
    let pptx = PptxBuilder::new()
        .slide("slide1.xml", slide)
        .notes("notesSlide1.xml", notes)
        .rels(
            "slide1.xml",
            rels_xml(&[("rId1", "notesSlide", "../notesSlides/notesSlide1.xml")]),
        )
        .build();

    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].text, "Intro");

    let markdown = pptx_extract::extract_markdown(&pptx).unwrap();
    assert!(!markdown.contains("Notes:"), "{markdown}");
    assert!(!markdown.contains("Q3 results"), "{markdown}");
}

/// A table graphic frame without its own position: inside a group it keeps
/// document order with its unpositioned siblings.
fn table_shape_plain(id: u32, rows: &str) -> String {
    format!(
        "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{id}\" name=\"Table\"/>\
         <p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>\
         <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\">\
         <a:tbl><a:tblPr firstRow=\"1\"/>\
         <a:tblGrid><a:gridCol w=\"2438400\"/></a:tblGrid>{rows}</a:tbl>\
         </a:graphicData></a:graphic></p:graphicFrame>"
    )
}

/// A group holds a text shape and a table, both read in document order.
#[test]
fn group_with_text_and_table() {
    let grouped = group_shape(
        50,
        914_400,
        2_743_200,
        &format!(
            "{}{}",
            text_shape(3, None, None, &para(&run("Grouped callout"), "")),
            table_shape_plain(4, &format!("<a:tr>{}</a:tr>", tc("Cell A", 1, 1, false))),
        ),
    );
    let slide = slide_xml(&format!(
        "{grouped}{}",
        text_shape(2, Some("title"), None, &para(&run("Diagram"), "")),
    ));
    let pptx = PptxBuilder::new().slide("slide1.xml", slide).build();

    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, ["Diagram", "Grouped callout", "Cell A"]);
    assert!(matches!(
        blocks[2].kind,
        BlockKind::TableCell {
            row: 0,
            column: 0,
            ..
        }
    ));
}

/// Group shapes recurse, including groups nested in groups; without
/// positions the children read in document order.
#[test]
fn group_shapes_recurse() {
    let inner = group_shape(
        51,
        0,
        3_000_000,
        &text_shape(4, None, None, &para(&run("Deep note"), "")),
    );
    let outer = group_shape(
        50,
        914_400,
        2_743_200,
        &format!(
            "{inner}{}",
            text_shape(3, None, None, &para(&run("Grouped callout"), "")),
        ),
    );
    let slide = slide_xml(&format!(
        "{}{outer}",
        text_shape(2, Some("title"), None, &para(&run("Diagram"), "")),
    ));
    let pptx = PptxBuilder::new().slide("slide1.xml", slide).build();

    let blocks = pptx_extract::extract_blocks(&pptx).unwrap();
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, ["Diagram", "Deep note", "Grouped callout"]);
}

/// Garbage is not a package, and an empty package has no slides.
#[test]
fn invalid_inputs_error() {
    assert!(pptx_extract::extract_blocks(b"not a zip").is_err());
    let cursor = Cursor::new(Vec::new());
    let empty = zip::ZipWriter::new(cursor).finish().unwrap().into_inner();
    let err = pptx_extract::extract_blocks(&empty).unwrap_err();
    assert!(err.to_string().contains("no slides"), "{err}");
}
