//! Slide XML parsing: a minimal DOM plus the shape-to-block mapping.
//!
//! Slides are small, so each part is parsed once into [`Node`] and then walked
//! with plain recursive functions. Group shapes recurse through the same
//! walker, which keeps the nesting logic in one place.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::error::{PptxExtractError, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

/// Deepest `p:grpSp` nesting followed before shapes are dropped.
///
/// Real decks nest two or three deep; past this the file is hostile or broken,
/// and dropping beats recursing forever.
const MAX_GROUP_DEPTH: u8 = 32;

/// Largest table span honoured from `gridSpan`/`rowSpan`.
///
/// A corrupt value in the millions would otherwise size the occupancy grid.
const MAX_TABLE_SPAN: u16 = 64;

/// Deepest bullet level the engine model keeps; deeper levels fold into it,
/// the way [`pdf_engine::BlockKind::ListItem`] documents.
const MAX_LIST_LEVEL: u8 = 4;

/// One XML element with its attributes, children and direct text.
///
/// Element names are local names: the prefix is stripped (`p:sp` becomes
/// `sp`), so files that bind the DrawingML URIs to other prefixes still
/// parse. Attribute names are kept qualified as written, because
/// `<p:sldId id="256" r:id="rId2"/>` carries two attributes with the same
/// local name and stripping would return the numeric id for the relationship
/// lookup: read those with [`Node::rel_attr`].
#[derive(Debug, Default, Clone)]
pub(crate) struct Node {
    pub(crate) name: String,
    attrs: Vec<(String, String)>,
    pub(crate) children: Vec<Node>,
    /// Concatenated direct character data. Only read on leaf-ish elements
    /// (`a:t`, `c:v`); containers hold pretty-printing whitespace here.
    pub(crate) text: String,
}

impl Node {
    /// The attribute value, or `None` where the attribute is absent.
    ///
    /// Exact match on the qualified name, for the unprefixed attributes this
    /// format uses (`lvl`, `type`, `gridSpan`, ...).
    pub(crate) fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// A relationship attribute (`r:embed`, `r:id`) by local name, whatever
    /// prefix the file binds the relationships namespace to.
    ///
    /// Only prefixed attributes match, so the numeric `id` on `p:sldId` can
    /// never answer a lookup meant for `r:id`.
    pub(crate) fn rel_attr(&self, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key.rsplit_once(':').is_some_and(|(_, l)| l == local))
            .map(|(_, value)| value.as_str())
    }

    /// The first direct child with this local name.
    pub(crate) fn first_child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|child| child.name == name)
    }

    /// All descendants with this local name, depth-first in document order.
    pub(crate) fn descendants(&self, name: &str) -> Vec<&Node> {
        let mut out = Vec::new();
        collect_descendants(self, name, &mut out);
        out
    }
}

fn collect_descendants<'a>(node: &'a Node, name: &str, out: &mut Vec<&'a Node>) {
    for child in &node.children {
        if child.name == name {
            out.push(child);
        }
        collect_descendants(child, name, out);
    }
}

/// The local name of a qualified XML name: everything after the last colon.
fn local_name(qualified: &[u8]) -> String {
    let text = String::from_utf8_lossy(qualified);
    match text.rsplit_once(':') {
        Some((_, local)) => local.to_string(),
        None => text.into_owned(),
    }
}

/// Parse one package part into a [`Node`] tree under a synthetic root.
pub(crate) fn parse_xml(xml: &[u8]) -> Result<Node> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    // The stack always holds the synthetic root at index 0; it is never popped.
    let mut stack: Vec<Node> = vec![Node::default()];
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(element)) => {
                let node = start_node(&element)?;
                stack.push(node);
            }
            Ok(Event::Empty(element)) => {
                let node = start_node(&element)?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                }
            }
            Ok(Event::End(_)) => {
                if stack.len() > 1 {
                    if let Some(node) = stack.pop() {
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(node);
                        }
                    }
                }
            }
            Ok(Event::Text(text)) => {
                let decoded = text
                    .unescape()
                    .map_err(|err| PptxExtractError::Invalid(format!("bad XML text: {err}")))?;
                if let Some(current) = stack.last_mut() {
                    current.text.push_str(&decoded);
                }
            }
            Ok(Event::CData(data)) => {
                let decoded = data
                    .decode()
                    .map_err(|err| PptxExtractError::Invalid(format!("bad CDATA: {err}")))?;
                if let Some(current) = stack.last_mut() {
                    current.text.push_str(&decoded);
                }
            }
            Ok(Event::Eof) => break,
            // Declaration, processing instructions, comments, doctypes.
            Ok(_) => {}
            Err(err) => return Err(PptxExtractError::Xml(err)),
        }
        buf.clear();
    }
    // The root is never popped, so this always yields it.
    match stack.pop() {
        Some(root) => Ok(root),
        None => Err(PptxExtractError::Invalid("empty XML document".into())),
    }
}

/// A [`Node`] for a start or empty tag: local element name plus decoded
/// attribute names, qualified as written (see [`Node::rel_attr`]).
fn start_node(element: &quick_xml::events::BytesStart<'_>) -> Result<Node> {
    let mut node = Node {
        name: local_name(element.name().as_ref()),
        ..Node::default()
    };
    for attr in element.attributes() {
        let attr =
            attr.map_err(|err| PptxExtractError::Invalid(format!("bad attribute: {err}")))?;
        let value = attr
            .unescape_value()
            .map_err(|err| PptxExtractError::Invalid(format!("bad attribute value: {err}")))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        node.attrs.push((key, value.into_owned()));
    }
    Ok(node)
}

/// What kind of bullet a DrawingML paragraph carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bullet {
    /// No bullet properties: a plain paragraph.
    None,
    /// `a:buChar` or `a:buBlip`: an unordered item.
    Unordered,
    /// `a:buAutoNum`: an ordered item, with the `startAt` the file states.
    Ordered {
        /// The number the file assigns this item, if it assigns one.
        start_at: Option<u32>,
    },
}

/// One `a:p` paragraph: its text, bullet level and bullet kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Para {
    /// Bullet nesting from `a:pPr@lvl`, clamped to [`MAX_LIST_LEVEL`].
    pub(crate) level: u8,
    pub(crate) bullet: Bullet,
    /// Runs joined directly, `a:br` as a space, ends trimmed.
    pub(crate) text: String,
}

/// Whether the paragraph holds anything worth a block.
pub(crate) fn is_blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// What bullet one property element states, if it states any: `buNone` wins
/// over a bullet beside it, the way the paragraph level already read it.
fn bullet_at(props: Option<&Node>) -> Option<Bullet> {
    let props = props?;
    if props.first_child("buNone").is_some() {
        return Some(Bullet::None);
    }
    if let Some(auto) = props.first_child("buAutoNum") {
        return Some(Bullet::Ordered {
            start_at: auto.attr("startAt").and_then(|s| s.parse::<u32>().ok()),
        });
    }
    if props.first_child("buChar").is_some() || props.first_child("buBlip").is_some() {
        return Some(Bullet::Unordered);
    }
    None
}

/// A paragraph's level plus what its own `a:pPr` states, if anything.
fn para_level_and_bullet(p: &Node) -> (u8, Option<Bullet>) {
    let mut level = 0u8;
    let mut bullet = None;
    if let Some(pr) = p.first_child("pPr") {
        if let Some(lvl) = pr.attr("lvl").and_then(|s| s.parse::<u8>().ok()) {
            level = lvl.min(MAX_LIST_LEVEL);
        }
        bullet = bullet_at(Some(pr));
    }
    (level, bullet)
}

/// Parse one `a:p` element.
pub(crate) fn parse_para(p: &Node) -> Para {
    let (level, bullet) = para_level_and_bullet(p);
    let bullet = bullet.unwrap_or(Bullet::None);

    let mut text = String::new();
    for child in &p.children {
        match child.name.as_str() {
            "r" => push_run_text(&mut text, child),
            // A field contributes its cached runs, the slide-number field
            // included: the number on the slide is text, like the footer.
            "fld" => push_run_text(&mut text, child),
            // A line break inside a paragraph joins with a space, the way
            // `Block::text` joins the lines the measure made.
            "br" => text.push(' '),
            // Not legal DrawingML, but cheap to accept.
            "t" => text.push_str(&child.text),
            _ => {}
        }
    }

    Para {
        level,
        bullet,
        text: text.trim().to_string(),
    }
}

/// Append the `a:t` descendants of a run or field, in document order.
fn push_run_text(out: &mut String, run: &Node) {
    for t in run.descendants("t") {
        out.push_str(&t.text);
    }
}

/// One `a:tc` table cell: its grid position, spans and text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TableCellData {
    /// 0-based row inside the table.
    pub(crate) row: u16,
    /// 0-based column inside the table.
    pub(crate) column: u16,
    pub(crate) row_span: u16,
    pub(crate) column_span: u16,
    /// Whether the cell is in the header row.
    pub(crate) header: bool,
    /// The cell's paragraphs joined with a space.
    pub(crate) text: String,
}

/// One `a:tbl` table: its cells in row-major order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TableData {
    pub(crate) cells: Vec<TableCellData>,
}

/// Whether a merge or flag attribute is switched on (`"1"`/`"true"`).
fn is_on(node: &Node, name: &str) -> bool {
    matches!(node.attr(name), Some("1") | Some("true"))
}

/// Parse one `a:tbl` element into row-major cells.
///
/// `gridSpan` widens a cell over columns, `rowSpan` over rows; cells the file
/// marks `hMerge`/`vMerge` are the covered remainder of such a span and emit
/// nothing, while still advancing the grid cursor by one.
pub(crate) fn parse_table(tbl: &Node) -> TableData {
    let first_row = tbl
        .first_child("tblPr")
        .map(|pr| pr.attr("firstRow") != Some("0") && pr.attr("firstRow") != Some("false"))
        .unwrap_or(true);
    let mut cells = Vec::new();
    // Occupancy of the grid so far: `occupied[row][column]`.
    let mut occupied: Vec<Vec<bool>> = Vec::new();
    let mut row_idx: u16 = 0;
    for tr in tbl.children.iter().filter(|child| child.name == "tr") {
        let row = row_idx as usize;
        if occupied.len() <= row {
            occupied.resize(row + 1, Vec::new());
        }
        let mut column: usize = 0;
        for tc in tc_children(tr) {
            while is_occupied(&occupied, row, column) {
                column += 1;
            }
            if is_on(tc, "hMerge") || is_on(tc, "vMerge") {
                set_occupied(&mut occupied, row, column);
                column += 1;
                continue;
            }
            let column_span = span(tc.attr("gridSpan"));
            let row_span = span(tc.attr("rowSpan"));
            for dr in 0..row_span as usize {
                for dc in 0..column_span as usize {
                    set_occupied(&mut occupied, row + dr, column + dc);
                }
            }
            cells.push(TableCellData {
                row: row_idx,
                column: column as u16,
                row_span,
                column_span,
                header: row_idx == 0 && first_row,
                text: cell_text(tc),
            });
            column += column_span as usize;
        }
        row_idx = row_idx.saturating_add(1);
    }
    TableData { cells }
}

/// The direct `a:tc` children of a table row.
fn tc_children(tr: &Node) -> impl Iterator<Item = &Node> {
    tr.children.iter().filter(|child| child.name == "tc")
}

/// A span attribute value: at least 1, capped at [`MAX_TABLE_SPAN`].
fn span(raw: Option<&str>) -> u16 {
    raw.and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(1)
        .clamp(1, MAX_TABLE_SPAN)
}

/// A cell's paragraphs joined with a space; blank ones contribute nothing.
fn cell_text(tc: &Node) -> String {
    tc.descendants("p")
        .iter()
        .map(|p| parse_para(p).text)
        .filter(|text| !is_blank(text))
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_occupied(occupied: &[Vec<bool>], row: usize, column: usize) -> bool {
    occupied
        .get(row)
        .and_then(|cells| cells.get(column))
        .copied()
        .unwrap_or(false)
}

fn set_occupied(occupied: &mut Vec<Vec<bool>>, row: usize, column: usize) {
    if occupied.len() <= row {
        occupied.resize(row + 1, Vec::new());
    }
    if occupied[row].len() <= column {
        occupied[row].resize(column + 1, false);
    }
    occupied[row][column] = true;
}

/// A shape's position and size in EMU, from `a:xfrm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Xfrm {
    /// Offset from the slide's top-left, in EMU (y down).
    pub(crate) x: i64,
    /// Offset from the slide's top-left, in EMU (y down).
    pub(crate) y: i64,
    /// Extent in EMU.
    pub(crate) width: i64,
    /// Extent in EMU.
    pub(crate) height: i64,
}

/// Read the first `a:xfrm` descendant: `a:off@x,y` plus `a:ext@cx,cy`.
pub(crate) fn parse_xfrm(shape: &Node) -> Option<Xfrm> {
    let xfrm = shape.descendants("xfrm").into_iter().next()?;
    let off = xfrm.first_child("off")?;
    let ext = xfrm.first_child("ext")?;
    Some(Xfrm {
        x: off.attr("x")?.parse::<i64>().ok()?,
        y: off.attr("y")?.parse::<i64>().ok()?,
        width: ext.attr("cx")?.parse::<i64>().ok()?,
        height: ext.attr("cy")?.parse::<i64>().ok()?,
    })
}

/// The content of one shape on the slide tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShapeBody {
    /// A text shape's paragraphs (`p:txBody/a:p`).
    Text(Vec<Para>),
    /// A `p:graphicFrame` holding `a:tbl`.
    Table(TableData),
    /// A `p:pic` picture: alt text plus the `r:embed` relationship id,
    /// resolved against the slide's rels by the caller.
    Picture {
        /// `p:cNvPr@descr`, falling back to `@name`.
        alt: String,
        /// The `a:blip@embed` relationship id.
        embed: String,
    },
    /// A `p:graphicFrame` holding anything but `a:tbl`.
    ///
    /// PARTLY COVERED, by design of this MVP: SmartArt (`dgm:*`) and charts
    /// (`c:chart`) contribute whatever text their XML carries -- SmartArt
    /// paragraphs and chart titles/labels surface as `a:p`, cached chart
    /// values as `c:v` -- but there is no layout, no style, no diagram
    /// semantics, and no reading of a chart's external workbook.
    GraphicText {
        /// Every descendant `a:p`, in document order.
        paras: Vec<Para>,
        /// Every descendant `c:v`, trimmed, blanks dropped.
        values: Vec<String>,
    },
    /// A `p:grpSp` group's children, extracted by recursing.
    Group(Vec<Shape>),
    /// Nothing to emit: a skipped placeholder or an empty shape.
    Skip,
}

/// One shape on a slide: where it sits, whether it is the title, what it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shape {
    /// Position and size, or `None` where the shape states none.
    pub(crate) xfrm: Option<Xfrm>,
    /// Whether this is the slide's title placeholder.
    pub(crate) is_title: bool,
    /// The shape's content.
    pub(crate) body: ShapeBody,
}

/// Extract the shapes under one `p:spTree` or `p:grpSp`, in document order.
///
/// `depth` counts `p:grpSp` nesting from the slide root; past
/// [`MAX_GROUP_DEPTH`] the group is dropped. `ctx` is what the slide's
/// paragraphs inherit bullets from.
pub(crate) fn extract_shapes(sp_tree: &Node, depth: u8, ctx: &BulletContext) -> Vec<Shape> {
    let mut shapes = Vec::new();
    for child in &sp_tree.children {
        match child.name.as_str() {
            "sp" | "cxnSp" => shapes.push(text_shape(child, ctx)),
            "grpSp" => {
                if depth < MAX_GROUP_DEPTH {
                    // A group's `sp`, `grpSp`, `graphicFrame` and `pic`
                    // children sit directly under `p:grpSp`, next to
                    // `nvGrpSpPr`/`grpSpPr`: there is no `spTree` to descend
                    // into, so the group node itself is the walker input.
                    let grouped = extract_shapes(child, depth + 1, ctx);
                    shapes.push(Shape {
                        xfrm: parse_xfrm(child),
                        is_title: false,
                        body: ShapeBody::Group(grouped),
                    });
                }
            }
            "graphicFrame" => shapes.push(graphic_shape(child)),
            "pic" => shapes.push(picture_shape(child)),
            _ => {}
        }
    }
    shapes
}

/// The `p:ph@type` of a shape, if it is a placeholder.
pub(crate) fn placeholder_type(shape: &Node) -> Option<&str> {
    shape.descendants("ph").into_iter().next()?.attr("type")
}

/// The `p:ph@idx` of a shape, if its placeholder states one.
pub(crate) fn placeholder_idx(shape: &Node) -> Option<&str> {
    shape.descendants("ph").into_iter().next()?.attr("idx")
}

/// How many `lvlNpPr` levels a list style holds (`lvl1pPr` .. `lvl9pPr`).
const STYLE_LEVELS: usize = 9;

/// One list style's bullet per outline level: `None` states nothing.
type LevelBullets = [Option<Bullet>; STYLE_LEVELS];

/// The bullets of an `a:lstStyle` (or a master `*Style`, whose `lvlNpPr`
/// children sit directly under it): one per level.
fn level_bullets(style: Option<&Node>) -> LevelBullets {
    let mut out: LevelBullets = [None; STYLE_LEVELS];
    let Some(style) = style else {
        return out;
    };
    for (level, slot) in out.iter_mut().enumerate() {
        let name = format!("lvl{}pPr", level + 1);
        *slot = style
            .first_child(&name)
            .and_then(|props| bullet_at(Some(props)));
    }
    out
}

/// One layout placeholder: its identity plus its list-style bullets.
#[derive(Debug, Clone)]
struct LayoutAnchor {
    ph_type: Option<String>,
    ph_idx: Option<String>,
    bullets: LevelBullets,
}

/// What a slide's paragraphs inherit bullets from: the layout placeholders
/// plus the master text styles.
#[derive(Debug, Clone, Default)]
pub(crate) struct BulletContext {
    anchors: Vec<LayoutAnchor>,
    master_title: LevelBullets,
    master_body: LevelBullets,
    master_other: LevelBullets,
}

impl BulletContext {
    /// The inheritance of one slide: its layout placeholders plus its
    /// master's `p:txStyles`. Either part may be absent.
    pub(crate) fn new(layout: Option<&Node>, master: Option<&Node>) -> Self {
        let mut ctx = Self::default();
        if let Some(layout) = layout {
            for sp in layout.descendants("sp") {
                let lst = sp
                    .first_child("txBody")
                    .and_then(|tx| tx.first_child("lstStyle"));
                ctx.anchors.push(LayoutAnchor {
                    ph_type: placeholder_type(sp).map(str::to_string),
                    ph_idx: placeholder_idx(sp).map(str::to_string),
                    bullets: level_bullets(lst),
                });
            }
        }
        if let Some(master) = master {
            if let Some(tx) = master.descendants("txStyles").into_iter().next() {
                ctx.master_title = level_bullets(tx.first_child("titleStyle"));
                ctx.master_body = level_bullets(tx.first_child("bodyStyle"));
                ctx.master_other = level_bullets(tx.first_child("otherStyle"));
            }
        }
        ctx
    }

    /// The layout placeholder of the same type: footer, date and
    /// slide-number placeholders inherit through their own chain only, so the
    /// index never resolves one to a body placeholder's bullets.
    fn anchor_same_type(&self, ph_type: Option<&str>) -> Option<&LayoutAnchor> {
        let kind = ph_type?;
        self.anchors
            .iter()
            .find(|anchor| anchor.ph_type.as_deref() == Some(kind))
    }

    /// The layout placeholder a shape inherits from: by index first, then
    /// by type. A shape without a placeholder matches nothing.
    fn anchor(&self, ph_type: Option<&str>, ph_idx: Option<&str>) -> Option<&LayoutAnchor> {
        if let Some(idx) = ph_idx {
            if let Some(hit) = self
                .anchors
                .iter()
                .find(|anchor| anchor.ph_idx.as_deref() == Some(idx))
            {
                return Some(hit);
            }
        }
        if let Some(kind) = ph_type {
            if let Some(hit) = self
                .anchors
                .iter()
                .find(|anchor| anchor.ph_type.as_deref() == Some(kind))
            {
                return Some(hit);
            }
        }
        None
    }

    /// The master style a placeholder type inherits from: title, body, and
    /// `otherStyle` for everything else, free text boxes included.
    fn master_for(&self, ph_type: Option<&str>) -> &LevelBullets {
        match ph_type {
            Some("title" | "ctrTitle") => &self.master_title,
            Some("body") => &self.master_body,
            _ => &self.master_other,
        }
    }
}

/// A text shape's non-blank `p:txBody/a:p` paragraphs, in document order.
///
/// Each paragraph's bullet resolves outward: its own `a:pPr`, the shape's
/// `a:lstStyle`, the layout placeholder's, then the master's style for the
/// placeholder type -- `buNone` and bullets alike stop at the nearest level
/// that states one.
///
/// Footer, date and slide-number placeholders resolve through their own
/// chain only -- the shape's list style, the same-type layout placeholder's,
/// then the master's -- never through `bodyStyle`, and a paragraph's own
/// bullet does not apply: the type decides, so they are paragraphs unless
/// that chain itself states a bullet.
fn tx_paras_inherit(
    sp: &Node,
    anchor: Option<&LayoutAnchor>,
    ph_type: Option<&str>,
    ctx: &BulletContext,
) -> Vec<Para> {
    let Some(body) = sp.first_child("txBody") else {
        return Vec::new();
    };
    let shape = level_bullets(body.first_child("lstStyle"));
    let footer = matches!(ph_type, Some("ftr" | "dt" | "sldNum"));
    let layout = if footer {
        ctx.anchor_same_type(ph_type).map(|anchor| &anchor.bullets)
    } else {
        anchor.map(|anchor| &anchor.bullets)
    };
    let master = ctx.master_for(ph_type);
    body.children
        .iter()
        .filter(|child| child.name == "p")
        .map(|p| {
            let (level, own) = para_level_and_bullet(p);
            let at = level as usize;
            let inherited = shape[at]
                .or_else(|| layout.and_then(|bullets| bullets[at]))
                .or(master[at]);
            let bullet = if footer {
                inherited.unwrap_or(Bullet::None)
            } else {
                own.or(inherited).unwrap_or(Bullet::None)
            };
            let mut para = parse_para(p);
            para.bullet = bullet;
            para
        })
        .filter(|para| !is_blank(&para.text))
        .collect::<Vec<_>>()
}

/// One `p:sp` (or connector) shape.
fn text_shape(sp: &Node, ctx: &BulletContext) -> Shape {
    let own_type = placeholder_type(sp);
    let anchor = ctx.anchor(own_type, placeholder_idx(sp));
    // A partial `p:ph` (an index without a type) resolves through the
    // layout placeholder it points at.
    let ph_type = own_type.or_else(|| anchor.and_then(|anchor| anchor.ph_type.as_deref()));
    // `sldImg` is a picture slot with no text of its own. The slide number
    // itself is text, like the footer and the date.
    if matches!(own_type, Some("sldImg")) {
        return Shape {
            xfrm: parse_xfrm(sp),
            is_title: false,
            body: ShapeBody::Skip,
        };
    }
    let paras = tx_paras_inherit(sp, anchor, ph_type, ctx);
    let is_title = matches!(ph_type, Some("title") | Some("ctrTitle"));
    Shape {
        xfrm: parse_xfrm(sp),
        is_title,
        body: ShapeBody::Text(paras),
    }
}

/// One `p:graphicFrame`: a table where it holds `a:tbl`, else graphic text.
fn graphic_shape(frame: &Node) -> Shape {
    let xfrm = parse_xfrm(frame);
    let body = match frame.descendants("tbl").into_iter().next() {
        Some(tbl) => ShapeBody::Table(parse_table(tbl)),
        None => ShapeBody::GraphicText {
            paras: frame
                .descendants("p")
                .iter()
                .map(|p| parse_para(p))
                .filter(|para| !is_blank(&para.text))
                .collect(),
            values: frame
                .descendants("v")
                .iter()
                .map(|v| v.text.trim().to_string())
                .filter(|value| !is_blank(value))
                .collect(),
        },
    };
    Shape {
        xfrm,
        is_title: false,
        body,
    }
}

/// One `p:pic` picture shape: a reference, never rendered pixels.
fn picture_shape(pic: &Node) -> Shape {
    let props = pic.descendants("cNvPr").into_iter().next();
    let alt = props
        .and_then(|pr| pr.attr("descr"))
        .or_else(|| props.and_then(|pr| pr.attr("name")))
        .unwrap_or_default()
        .to_string();
    let embed = pic
        .descendants("blip")
        .into_iter()
        .next()
        .and_then(|blip| blip.rel_attr("embed"))
        .unwrap_or_default()
        .to_string();
    Shape {
        xfrm: parse_xfrm(pic),
        is_title: false,
        body: ShapeBody::Picture { alt, embed },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a fragment under a synthetic root for unit tests.
    fn fragment(xml: &str) -> Node {
        parse_xml(xml.as_bytes()).unwrap()
    }

    #[test]
    fn elements_drop_prefixes_but_relationship_attrs_keep_theirs() {
        let root = fragment("<p:sp xmlns:p=\"x\" r:embed=\"rId2\" xmlns:r=\"y\"/>");
        let sp = &root.children[0];
        assert_eq!(sp.name, "sp");
        assert_eq!(sp.attr("embed"), None);
        assert_eq!(sp.rel_attr("embed"), Some("rId2"));
    }

    #[test]
    fn rel_id_beats_numeric_id_on_sld_id() {
        let root = fragment("<p:sldId id=\"256\" r:id=\"rId2\"/>");
        let id = &root.children[0];
        assert_eq!(id.attr("id"), Some("256"));
        assert_eq!(id.rel_attr("id"), Some("rId2"));
    }

    #[test]
    fn lvl_clamps_to_model_maximum() {
        let root = fragment("<a:p><a:pPr lvl=\"9\"><a:buChar char=\"-\"/></a:pPr></a:p>");
        let para = parse_para(&root.children[0]);
        assert_eq!(para.level, MAX_LIST_LEVEL);
        assert_eq!(para.bullet, Bullet::Unordered);
    }

    #[test]
    fn bu_none_beats_a_bullet() {
        let root = fragment(
            "<a:p><a:pPr><a:buNone/><a:buChar char=\"-\"/></a:pPr><a:r><a:t>Hi</a:t></a:r></a:p>",
        );
        let para = parse_para(&root.children[0]);
        assert_eq!(para.bullet, Bullet::None);
        assert_eq!(para.text, "Hi");
    }

    #[test]
    fn slidenum_field_keeps_its_cached_text() {
        let root = fragment(
            "<a:p><a:r><a:t>Page </a:t></a:r><a:fld type=\"slidenum\"><a:r><a:t>3</a:t></a:r></a:fld></a:p>",
        );
        let para = parse_para(&root.children[0]);
        assert_eq!(para.text, "Page 3");
    }

    #[test]
    fn layout_anchor_matches_idx_before_type() {
        let layout = fragment(
            "<p:sldLayout><p:cSld><p:spTree>\
             <p:sp><p:nvSpPr><p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr>\
             <p:txBody><a:lstStyle><a:lvl1pPr><a:buChar char=\"-\"/></a:lvl1pPr></a:lstStyle></p:txBody></p:sp>\
             <p:sp><p:nvSpPr><p:nvPr><p:ph type=\"body\" idx=\"2\"/></p:nvPr></p:nvSpPr>\
             <p:txBody><a:lstStyle><a:lvl1pPr><a:buNone/></a:lvl1pPr></a:lstStyle></p:txBody></p:sp>\
             </p:spTree></p:cSld></p:sldLayout>",
        );
        let ctx = BulletContext::new(Some(&layout), None);
        // Index 2 wins over the shared type: its `buNone` answers.
        let anchor = ctx.anchor(Some("body"), Some("2")).unwrap();
        assert_eq!(anchor.bullets[0], Some(Bullet::None));
        // Without an index the first body placeholder answers.
        let anchor = ctx.anchor(Some("body"), None).unwrap();
        assert_eq!(anchor.bullets[0], Some(Bullet::Unordered));
        // A free text box matches no placeholder.
        assert!(ctx.anchor(None, None).is_none());
    }

    #[test]
    fn master_style_follows_placeholder_type() {
        let master = fragment(
            "<p:sldMaster><p:txStyles>\
             <p:titleStyle><a:lvl1pPr><a:buNone/></a:lvl1pPr></p:titleStyle>\
             <p:bodyStyle><a:lvl1pPr><a:buChar char=\"-\"/></a:lvl1pPr></p:bodyStyle>\
             <p:otherStyle><a:lvl1pPr><a:buAutoNum type=\"arabicPeriod\"/></a:lvl1pPr></p:otherStyle>\
             </p:txStyles></p:sldMaster>",
        );
        let ctx = BulletContext::new(None, Some(&master));
        assert_eq!(ctx.master_for(Some("title"))[0], Some(Bullet::None));
        assert_eq!(ctx.master_for(Some("ctrTitle"))[0], Some(Bullet::None));
        assert_eq!(ctx.master_for(Some("body"))[0], Some(Bullet::Unordered));
        assert!(matches!(
            ctx.master_for(None)[0],
            Some(Bullet::Ordered { .. })
        ));
        assert!(matches!(
            ctx.master_for(Some("obj"))[0],
            Some(Bullet::Ordered { .. })
        ));
    }

    #[test]
    fn group_children_sit_directly_under_grp_sp() {
        let root = fragment(
            "<p:grpSp><p:nvGrpSpPr/><p:grpSpPr/>\
             <p:sp><p:txBody><a:p><a:r><a:t>Grouped</a:t></a:r></a:p></p:txBody></p:sp>\
             <p:grpSp><p:nvGrpSpPr/><p:grpSpPr/>\
             <p:sp><p:txBody><a:p><a:r><a:t>Deep</a:t></a:r></a:p></p:txBody></p:sp>\
             </p:grpSp></p:grpSp>",
        );
        let shapes = extract_shapes(&root.children[0], 0, &BulletContext::default());
        assert_eq!(shapes.len(), 2);
        assert!(matches!(shapes[0].body, ShapeBody::Text(_)));
        let ShapeBody::Group(nested) = &shapes[1].body else {
            panic!("a nested group holds a group, got {:?}", shapes[1].body);
        };
        assert_eq!(nested.len(), 1);
        assert!(matches!(nested[0].body, ShapeBody::Text(_)));
    }
}
