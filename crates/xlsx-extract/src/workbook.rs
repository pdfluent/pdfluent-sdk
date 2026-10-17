//! The workbook as parsed OOXML: sheets, cells, shared strings and styles.
//!
//! An XLSX file is a zip archive of XML parts. This module reads the parts
//! the extractor needs and resolves every cell to its display text:
//!
//! * `xl/workbook.xml` — sheet names in order, and the 1900/1904 date system.
//! * `xl/_rels/workbook.xml.rels` — where each sheet's XML lives.
//! * `xl/sharedStrings.xml` — the string table `t="s"` cells index into.
//! * `xl/styles.xml` — the number format of each cell style, which is what
//!   says whether a serial like `44927` is a number or `2023-01-01`.
//! * `xl/worksheets/sheetN.xml` — the cells and merged ranges themselves.
//!
//! Formula cells keep their CACHED value. The `<v>` beside a `<f>` is what
//! Excel wrote when it last calculated; this crate never recalculates and
//! never claims to.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io::Read;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::error::{Result, XlsxExtractError};

/// Built-in number formats that render a serial as a date or time (ECMA-376
/// §18.8.30): `14`–`22`, `27`–`36`, `45`–`47`, `50`–`58`.
const BUILTIN_DATE_FMT_IDS: [u32; 31] = [
    14, 15, 16, 17, 18, 19, 20, 21, 22, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 45, 46, 47, 50, 51,
    52, 53, 54, 55, 56, 57, 58,
];

/// The built-ins above that carry a clock time as well as a date.
const BUILTIN_TIME_FMT_IDS: [u32; 8] = [18, 19, 20, 21, 22, 45, 46, 47];

/// Excel serial of 1970-01-01 in the 1900 date system: the bridge to Unix days.
const SERIAL_AT_UNIX_EPOCH_1900: i64 = 25_569;

/// Days between 1904-01-01 and 1970-01-01: the same bridge for workbooks
/// whose `workbookPr` sets `date1904`.
const DAYS_1904_TO_UNIX_EPOCH: i64 = 24_107;

/// One parsed workbook: sheets in file order.
#[derive(Debug)]
pub struct Workbook {
    /// Sheets in the order `workbook.xml` lists them.
    pub sheets: Vec<Sheet>,
}

/// A 0-based `(row, column)` cell position.
pub(crate) type CellPos = (u32, u32);

/// A merged range as its 0-based corners.
pub(crate) type MergeRange = (CellPos, CellPos);

/// One worksheet: its name, its resolved cells and its merged ranges.
#[derive(Debug)]
pub struct Sheet {
    /// The tab name; becomes the heading block.
    pub name: String,
    /// Resolved cells keyed by 0-based `(row, column)`.
    pub cells: BTreeMap<CellPos, Cell>,
    /// Merged ranges as 0-based corners.
    pub merges: Vec<MergeRange>,
}

/// One cell with its display text and, for formulas, the formula text.
#[derive(Debug, Clone)]
pub struct Cell {
    /// What the cell shows: the cached value, never a recalculation.
    pub text: String,
    /// The formula (without a leading `=`) when the cell holds one.
    pub formula: Option<String>,
}

/// Parse a whole workbook from the bytes of an `.xlsx` file.
pub fn parse_workbook(data: &[u8]) -> Result<Workbook> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)?;

    let workbook_xml = read_part(&mut archive, "xl/workbook.xml")?;
    let (sheet_refs, date1904) = parse_workbook_xml(&workbook_xml)?;
    if sheet_refs.is_empty() {
        return Err(XlsxExtractError::Invalid("workbook has no sheets".into()));
    }

    // The relationship file maps each sheet's rId to its part. It is
    // required by the format, but a fallback by position costs nothing.
    let rels = read_part(&mut archive, "xl/_rels/workbook.xml.rels")
        .ok()
        .map(|xml| parse_rels_xml(&xml))
        .transpose()?
        .unwrap_or_default();

    let shared = read_part(&mut archive, "xl/sharedStrings.xml")
        .ok()
        .map(|xml| parse_shared_strings(&xml))
        .transpose()?
        .unwrap_or_default();

    let styles = read_part(&mut archive, "xl/styles.xml")
        .ok()
        .map(|xml| parse_styles(&xml))
        .transpose()?
        .unwrap_or_default();

    let mut sheets = Vec::with_capacity(sheet_refs.len());
    for (position, sheet_ref) in sheet_refs.iter().enumerate() {
        let path = sheet_path(&rels, &sheet_ref.rid, position);
        let xml = read_part(&mut archive, &path).map_err(|_| {
            XlsxExtractError::Invalid(format!(
                "sheet \"{}\" points at missing part \"{path}\"",
                sheet_ref.name
            ))
        })?;
        let (cells, merges) = parse_sheet_xml(&xml, &shared, &styles, date1904)?;
        sheets.push(Sheet {
            name: sheet_ref.name.clone(),
            cells,
            merges,
        });
    }

    Ok(Workbook { sheets })
}

/// Read one part of the archive to a string.
fn read_part<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<String> {
    let mut file = archive.by_name(name)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

/// A sheet entry in `workbook.xml`: its tab name and relationship id.
struct SheetRef {
    name: String,
    rid: String,
}

/// Sheet names in order, plus the date system flag.
fn parse_workbook_xml(xml: &str) -> Result<(Vec<SheetRef>, bool)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut sheets = Vec::new();
    let mut date1904 = false;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(element) | Event::Empty(element) => {
                let name = element.name();
                if name.local_name().into_inner() == b"sheet" {
                    let attrs = attributes(&element)?;
                    sheets.push(SheetRef {
                        name: attrs.get("name").cloned().unwrap_or_default(),
                        rid: attrs.get("id").cloned().unwrap_or_default(),
                    });
                } else if name.local_name().into_inner() == b"workbookPr" {
                    let attrs = attributes(&element)?;
                    date1904 = matches!(
                        attrs.get("date1904").map(String::as_str),
                        Some("1" | "true")
                    );
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok((sheets, date1904))
}

/// `rId` to part path from `workbook.xml.rels`.
fn parse_rels_xml(xml: &str) -> Result<HashMap<String, String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut rels = HashMap::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(element) | Event::Empty(element) => {
                if element.name().local_name().into_inner() == b"Relationship" {
                    let attrs = attributes(&element)?;
                    if attrs.get("TargetMode").map(String::as_str) == Some("External") {
                        // An external link, not a sheet part.
                    } else if let (Some(id), Some(target)) = (attrs.get("Id"), attrs.get("Target"))
                    {
                        rels.insert(id.clone(), target.clone());
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(rels)
}

/// Resolve a sheet's part path: via its relationship, else by position
/// (`xl/worksheets/sheetN.xml`), which is what writers emit in practice.
fn sheet_path(rels: &HashMap<String, String>, rid: &str, position: usize) -> String {
    if let Some(target) = rels.get(rid) {
        let target = target.trim_start_matches('/');
        if target.starts_with("xl/") {
            return target.to_string();
        }
        return format!("xl/{target}");
    }
    format!("xl/worksheets/sheet{}.xml", position + 1)
}

/// The shared string table: each `<si>` concatenated from its `<t>` runs.
///
/// Rich text (`<r><t>`) and plain (`<t>`) both land here; runs join without
/// a separator because that is how Excel reads them back.
fn parse_shared_strings(xml: &str) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut strings = Vec::new();
    let mut current: Option<String> = None;
    let mut in_text = false;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(element) => {
                let name = element.name();
                if name.local_name().into_inner() == b"si" {
                    current = Some(String::new());
                } else if name.local_name().into_inner() == b"t" {
                    in_text = current.is_some();
                }
            }
            Event::Text(text) => {
                if in_text {
                    if let Some(current) = current.as_mut() {
                        current.push_str(&text.unescape()?);
                    }
                }
            }
            Event::End(element) => {
                let name = element.name();
                if name.local_name().into_inner() == b"si" {
                    if let Some(item) = current.take() {
                        strings.push(item);
                    }
                } else if name.local_name().into_inner() == b"t" {
                    in_text = false;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(strings)
}

/// Number formats: the `numFmtId` of each entry of `cellXfs`, plus any
/// custom format codes by id.
#[derive(Debug, Default)]
struct Styles {
    /// `cellXfs` in order: `xf_formats[style_index] = num_fmt_id`.
    xf_formats: Vec<u32>,
    /// Custom `numFmt` codes by id (`>= 164`).
    custom: HashMap<u32, String>,
}

/// Whether the style says a serial is a date, and whether it carries time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateShape {
    /// Not a date: render the number.
    Number,
    /// Date only: `YYYY-MM-DD`.
    Date,
    /// Date and clock time: `YYYY-MM-DDTHH:MM:SS`.
    DateTime,
}

impl Styles {
    /// The shape a cell style gives a numeric value.
    fn shape_of(&self, style: Option<usize>) -> DateShape {
        let Some(index) = style else {
            return DateShape::Number;
        };
        let Some(&num_fmt) = self.xf_formats.get(index) else {
            return DateShape::Number;
        };
        if BUILTIN_DATE_FMT_IDS.contains(&num_fmt) {
            return if BUILTIN_TIME_FMT_IDS.contains(&num_fmt) {
                DateShape::DateTime
            } else {
                DateShape::Date
            };
        }
        if num_fmt < 164 {
            return DateShape::Number;
        }
        match self.custom.get(&num_fmt) {
            Some(code) => custom_shape(code),
            None => DateShape::Number,
        }
    }
}

/// Read the shape of a custom format code from its date/time tokens.
///
/// Quoted literals (`"Total: "`), bracketed sections (`[Red]`) and escaped
/// characters carry no meaning and are stripped first; what remains is read
/// for `y`/`m`/`d` (date) and `h`/`s` (time). Conservative on purpose: an
/// unreadable code renders as a number rather than a wrong date.
fn custom_shape(code: &str) -> DateShape {
    let first_section = code.split(';').next().unwrap_or(code);
    let mut stripped = String::with_capacity(first_section.len());
    let mut chars = first_section.chars();
    while let Some(char) = chars.next() {
        match char {
            '"' => {
                for quoted in chars.by_ref() {
                    if quoted == '"' {
                        break;
                    }
                }
            }
            '[' => {
                for bracketed in chars.by_ref() {
                    if bracketed == ']' {
                        break;
                    }
                }
            }
            '\\' | '_' | '*' => {
                chars.next();
            }
            other => stripped.push(other),
        }
    }
    let is_date = stripped.contains(['y', 'Y', 'm', 'M', 'd', 'D']);
    let has_time = stripped.contains(['h', 'H', 's', 'S']);
    if has_time && is_date {
        DateShape::DateTime
    } else if is_date {
        DateShape::Date
    } else if has_time {
        // A time-only code (`h:mm:ss`) still rides on a date serial.
        DateShape::DateTime
    } else {
        DateShape::Number
    }
}

/// Parse `styles.xml`: `cellXfs` entries in order, custom `numFmt` codes.
///
/// Only `<xf>` elements inside `<cellXfs>` count — `<cellStyleXfs>` holds
/// lookalikes that no cell style index addresses.
fn parse_styles(xml: &str) -> Result<Styles> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut styles = Styles::default();
    let mut in_cell_xfs = false;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(element) => {
                let name = element.name();
                if name.local_name().into_inner() == b"cellXfs" {
                    in_cell_xfs = true;
                } else if name.local_name().into_inner() == b"numFmt" {
                    push_num_fmt(&element, &mut styles)?;
                }
            }
            Event::Empty(element) => {
                let name = element.name();
                if name.local_name().into_inner() == b"xf" && in_cell_xfs {
                    styles.xf_formats.push(xf_num_fmt(&element)?);
                } else if name.local_name().into_inner() == b"numFmt" {
                    push_num_fmt(&element, &mut styles)?;
                }
            }
            Event::End(element) => {
                if element.name().local_name().into_inner() == b"cellXfs" {
                    in_cell_xfs = false;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(styles)
}

/// The `numFmtId` of one `<xf>`, defaulting to `0` (General) when absent.
fn xf_num_fmt(element: &BytesStart) -> Result<u32> {
    Ok(attributes(element)?
        .get("numFmtId")
        .and_then(|id| id.parse().ok())
        .unwrap_or(0))
}

/// Record one custom `<numFmt numFmtId formatCode>`.
fn push_num_fmt(element: &BytesStart, styles: &mut Styles) -> Result<()> {
    let attrs = attributes(element)?;
    if let (Some(id), Some(code)) = (attrs.get("numFmtId"), attrs.get("formatCode")) {
        if let Ok(id) = id.parse::<u32>() {
            styles.custom.insert(id, code.clone());
        }
    }
    Ok(())
}

/// A cell while its `<c>` element is open.
#[derive(Debug, Default)]
struct OpenCell {
    row: Option<u32>,
    col: Option<u32>,
    kind: Option<String>,
    style: Option<usize>,
    value: String,
    formula: Option<String>,
    inline: String,
    in_value: bool,
    in_formula: bool,
    in_inline_text: bool,
}

impl OpenCell {
    /// Resolve the open cell to display text against shared strings and styles.
    fn finish(self, shared: &[String], styles: &Styles, date1904: bool) -> Option<Cell> {
        let text = match self.kind.as_deref() {
            Some("s") => shared
                .get(self.value.trim().parse::<usize>().ok()?)
                .cloned()
                .unwrap_or_default(),
            Some("b") => match self.value.trim() {
                "1" | "true" | "TRUE" => "TRUE".to_string(),
                "0" | "false" | "FALSE" => "FALSE".to_string(),
                other => other.to_string(),
            },
            Some("str") | Some("e") => self.value.trim().to_string(),
            Some("inlineStr") => self.inline,
            _ => {
                let raw = self.value.trim();
                if raw.is_empty() {
                    // No cached value: an unevaluated formula or a bare `<c/>`.
                    // The formula text below is all there is to show.
                    String::new()
                } else if let Ok(number) = raw.parse::<f64>() {
                    match styles.shape_of(self.style) {
                        DateShape::Number => format_number(number),
                        DateShape::Date => serial_to_date(number, date1904, false)
                            .unwrap_or_else(|| raw.to_string()),
                        DateShape::DateTime => serial_to_date(number, date1904, true)
                            .unwrap_or_else(|| raw.to_string()),
                    }
                } else {
                    raw.to_string()
                }
            }
        };
        Some(Cell {
            text,
            formula: self.formula.filter(|formula| !formula.trim().is_empty()),
        })
    }
}

/// Parse one worksheet: resolved cells plus merged ranges.
fn parse_sheet_xml(
    xml: &str,
    shared: &[String],
    styles: &Styles,
    date1904: bool,
) -> Result<(BTreeMap<CellPos, Cell>, Vec<MergeRange>)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut cells: BTreeMap<CellPos, Cell> = BTreeMap::new();
    let mut merges = Vec::new();
    let mut current_row: u32 = 0;
    let mut next_col: u32 = 0;
    let mut cell: Option<OpenCell> = None;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(element) => match element.name().local_name().into_inner() {
                b"row" => {
                    let attrs = attributes(&element)?;
                    current_row = attrs
                        .get("r")
                        .and_then(|r| r.parse::<u32>().ok())
                        .map(|r| r.saturating_sub(1))
                        .unwrap_or(current_row);
                    next_col = 0;
                }
                b"c" => {
                    cell = Some(open_cell(&element, current_row, &mut next_col)?);
                }
                b"v" => {
                    if let Some(cell) = cell.as_mut() {
                        cell.in_value = true;
                    }
                }
                b"f" => {
                    if let Some(cell) = cell.as_mut() {
                        cell.in_formula = true;
                        cell.formula = Some(String::new());
                    }
                }
                b"t" => {
                    if let Some(cell) = cell.as_mut() {
                        cell.in_inline_text = true;
                    }
                }
                _ => {}
            },
            Event::Empty(element) => match element.name().local_name().into_inner() {
                b"c" => {
                    // A bare `<c r="A1"/>`: style without content. It marks
                    // nothing to show, so only cells with content enter the map.
                    let opened = open_cell(&element, current_row, &mut next_col)?;
                    if opened.formula.is_some() {
                        let (Some(row), Some(col)) = (opened.row, opened.col) else {
                            continue;
                        };
                        if let Some(resolved) = opened.finish(shared, styles, date1904) {
                            cells.insert((row, col), resolved);
                        }
                    }
                }
                b"mergeCell" => {
                    let attrs = attributes(&element)?;
                    if let Some(range) = attrs.get("ref").and_then(|r| parse_range(r)) {
                        merges.push(range);
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if let Some(cell) = cell.as_mut() {
                    let decoded = text.unescape()?;
                    if cell.in_value {
                        cell.value.push_str(&decoded);
                    } else if cell.in_formula {
                        if let Some(formula) = cell.formula.as_mut() {
                            formula.push_str(&decoded);
                        }
                    } else if cell.in_inline_text {
                        cell.inline.push_str(&decoded);
                    }
                }
            }
            Event::End(element) => match element.name().local_name().into_inner() {
                b"c" => {
                    if let Some(opened) = cell.take() {
                        let (Some(row), Some(col)) = (opened.row, opened.col) else {
                            continue;
                        };
                        let has_content = !opened.value.trim().is_empty()
                            || !opened.inline.is_empty()
                            || opened
                                .formula
                                .as_ref()
                                .is_some_and(|f| !f.trim().is_empty());
                        if has_content {
                            if let Some(resolved) = opened.finish(shared, styles, date1904) {
                                cells.insert((row, col), resolved);
                            }
                        }
                    }
                }
                b"v" => {
                    if let Some(cell) = cell.as_mut() {
                        cell.in_value = false;
                    }
                }
                b"f" => {
                    if let Some(cell) = cell.as_mut() {
                        cell.in_formula = false;
                    }
                }
                b"t" => {
                    if let Some(cell) = cell.as_mut() {
                        cell.in_inline_text = false;
                    }
                }
                b"row" => {
                    current_row = current_row.saturating_add(1);
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok((cells, merges))
}

/// Open a `<c>` element: coordinates from `r`, else the row's running column.
fn open_cell(element: &BytesStart, current_row: u32, next_col: &mut u32) -> Result<OpenCell> {
    let attrs = attributes(element)?;
    let (row, col) = match attrs.get("r").and_then(|r| parse_cell_ref(r)) {
        Some((row, col)) => {
            *next_col = col.saturating_add(1);
            (row, col)
        }
        None => {
            let col = *next_col;
            *next_col = next_col.saturating_add(1);
            (current_row, col)
        }
    };
    Ok(OpenCell {
        row: Some(row),
        col: Some(col),
        kind: attrs.get("t").cloned(),
        style: attrs.get("s").and_then(|s| s.parse().ok()),
        ..OpenCell::default()
    })
}

/// An element's attributes keyed by local name (`r:id` reads as `id`).
fn attributes(element: &BytesStart) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for attr in element.attributes() {
        let attr = attr.map_err(|error| XlsxExtractError::Invalid(error.to_string()))?;
        let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned();
        let value = attr.unescape_value()?.into_owned();
        map.insert(key, value);
    }
    Ok(map)
}

/// `BC23` (or `$BC$23`) to 0-based `(row, column)`.
fn parse_cell_ref(reference: &str) -> Option<(u32, u32)> {
    let cleaned: String = reference.chars().filter(|&c| c != '$').collect();
    let split = cleaned.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = cleaned.split_at(split);
    if letters.is_empty() || digits.is_empty() {
        return None;
    }
    let mut col: u32 = 0;
    for letter in letters.chars() {
        if !letter.is_ascii_alphabetic() {
            return None;
        }
        col = col
            .checked_mul(26)?
            .checked_add(u32::from(letter.to_ascii_uppercase() as u8 - b'A') + 1)?;
    }
    let row: u32 = digits.parse().ok()?;
    if row == 0 || col == 0 {
        return None;
    }
    Some((row - 1, col - 1))
}

/// `A1:C3` to normalized 0-based corners. A single cell reads as a range of one.
fn parse_range(range: &str) -> Option<MergeRange> {
    let mut parts = range.split(':');
    let first = parse_cell_ref(parts.next()?)?;
    let second = match parts.next() {
        Some(end) => parse_cell_ref(end)?,
        None => first,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((
        (first.0.min(second.0), first.1.min(second.1)),
        (first.0.max(second.0), first.1.max(second.1)),
    ))
}

/// A general-format number: integers without a decimal point, otherwise the
/// shortest round-trip text.
fn format_number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e15 {
        // `as i64` is exact in this range; `-0.0` prints as `0`.
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// An Excel serial to an ISO date, or `None` where it is not a sane date.
///
/// `with_time` follows the number format: date-only codes drop the clock,
/// time-carrying codes keep it. Out-of-range serials (negative, past
/// year 9999) are not dates; the caller falls back to the raw number.
fn serial_to_date(serial: f64, date1904: bool, with_time: bool) -> Option<String> {
    if !serial.is_finite() || serial < 0.0 {
        return None;
    }
    // Year 9999 ends near serial 2.95M; anything past 5M is not a date, and
    // the guard also keeps the `as i64` casts below from saturating.
    if serial > 5_000_000.0 {
        return None;
    }
    let mut days = serial.floor() as i64;
    let mut seconds = ((serial - serial.floor()) * 86_400.0).round() as i64;
    if seconds >= 86_400 {
        days += 1;
        seconds = 0;
    }
    // The 1900 system's day 60 is the leap day 1900 never had; every real
    // date before it sits one serial higher than the day count says.
    let unix_days = if date1904 {
        days - DAYS_1904_TO_UNIX_EPOCH
    } else if days < 61 {
        days - SERIAL_AT_UNIX_EPOCH_1900 + 1
    } else {
        days - SERIAL_AT_UNIX_EPOCH_1900
    };
    let (year, month, day) = civil_from_days(unix_days);
    if !(1..=9999).contains(&year) {
        return None;
    }
    if with_time {
        let hour = (seconds / 3600) as u32;
        let minute = ((seconds % 3600) / 60) as u32;
        let second = (seconds % 60) as u32;
        Some(format!(
            "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
        ))
    } else {
        Some(format!("{year:04}-{month:02}-{day:02}"))
    }
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = (if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    }) as u32;
    (
        (if month <= 2 { year + 1 } else { year }) as i32,
        month,
        day,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_refs_count_from_zero() {
        assert_eq!(parse_cell_ref("A1"), Some((0, 0)));
        assert_eq!(parse_cell_ref("B2"), Some((1, 1)));
        assert_eq!(parse_cell_ref("Z1"), Some((0, 25)));
        assert_eq!(parse_cell_ref("AA1"), Some((0, 26)));
        assert_eq!(parse_cell_ref("$C$3"), Some((2, 2)));
        assert_eq!(parse_cell_ref("XFD1048576"), Some((1_048_575, 16_383)));
        assert_eq!(parse_cell_ref("A0"), None);
        assert_eq!(parse_cell_ref("1A"), None);
        assert_eq!(parse_cell_ref(""), None);
    }

    #[test]
    fn ranges_normalize_and_accept_single_cells() {
        assert_eq!(parse_range("A1:C3"), Some(((0, 0), (2, 2))));
        assert_eq!(parse_range("C3:A1"), Some(((0, 0), (2, 2))));
        assert_eq!(parse_range("B2"), Some(((1, 1), (1, 1))));
        assert_eq!(parse_range("A1:B2:C3"), None);
    }

    #[test]
    fn numbers_print_without_a_pointless_decimal() {
        assert_eq!(format_number(30.0), "30");
        assert_eq!(format_number(-7.0), "-7");
        assert_eq!(format_number(0.5), "0.5");
        assert_eq!(format_number(-0.0), "0");
    }

    #[test]
    fn serials_become_iso_dates() {
        // 44927 is 2023-01-01; 45161.5 is 2023-08-23 noon.
        assert_eq!(
            serial_to_date(44_927.0, false, false).as_deref(),
            Some("2023-01-01")
        );
        assert_eq!(
            serial_to_date(45_161.5, false, true).as_deref(),
            Some("2023-08-23T12:00:00")
        );
        assert_eq!(
            serial_to_date(1.0, false, false).as_deref(),
            Some("1900-01-01")
        );
        assert_eq!(
            serial_to_date(0.0, true, false).as_deref(),
            Some("1904-01-01")
        );
        assert_eq!(serial_to_date(-1.0, false, false), None);
    }

    #[test]
    fn custom_codes_read_through_their_literals() {
        assert_eq!(custom_shape("yyyy-mm-dd"), DateShape::Date);
        assert_eq!(custom_shape("m/d/yy h:mm"), DateShape::DateTime);
        assert_eq!(custom_shape("\"Total: \"#,##0"), DateShape::Number);
        assert_eq!(custom_shape("[Red]0.0%"), DateShape::Number);
        assert_eq!(custom_shape("h:mm:ss"), DateShape::DateTime);
    }
}
