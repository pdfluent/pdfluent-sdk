//! Content stream round-trip: parse, modify, serialize.
//!
//! Wraps `lopdf::content::Content` to provide a high-level API for
//! editing PDF content streams. Tracks graphics state (CTM, color,
//! font, text matrix) as operations are iterated.

use crate::error::{ManipError, Result};
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId, Stream};
use std::io::Write;
use std::ops::Range;

/// Editor for PDF content stream operations.
///
/// Decodes a raw content stream into `Vec<Operation>`, allows mutation
/// (remove, replace, insert), and encodes back to bytes.
#[derive(Debug, Clone)]
pub struct ContentEditor {
    operations: Vec<Operation>,
}

impl ContentEditor {
    /// Decode a content stream from raw bytes.
    pub fn from_stream(stream: &[u8]) -> Result<Self> {
        let content = Content::decode(stream)
            .map_err(|e| ManipError::Other(format!("content decode: {e}")))?;
        Ok(Self {
            operations: content.operations,
        })
    }

    /// Create an editor from a pre-existing list of operations.
    pub fn from_operations(operations: Vec<Operation>) -> Self {
        Self { operations }
    }

    /// Create an empty editor.
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    /// Return a reference to the operations.
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Return a mutable reference to the operations.
    pub fn operations_mut(&mut self) -> &mut Vec<Operation> {
        &mut self.operations
    }

    /// Number of operations.
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// Whether the editor has no operations.
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Remove operations in the given range.
    pub fn remove_range(&mut self, range: Range<usize>) {
        if range.start < self.operations.len() {
            let end = range.end.min(self.operations.len());
            self.operations.drain(range.start..end);
        }
    }

    /// Replace the operation at `index` with one or more operations.
    pub fn replace_operation(&mut self, index: usize, ops: Vec<Operation>) {
        if index < self.operations.len() {
            self.operations.splice(index..=index, ops);
        }
    }

    /// Insert operations at the given position.
    pub fn insert_operations(&mut self, at: usize, ops: Vec<Operation>) {
        let at = at.min(self.operations.len());
        self.operations.splice(at..at, ops);
    }

    /// Append operations at the end.
    pub fn push(&mut self, op: Operation) {
        self.operations.push(op);
    }

    /// Remove all operations matching a predicate.
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&Operation) -> bool,
    {
        self.operations.retain(f);
    }

    /// Encode operations back to content stream bytes.
    ///
    /// Uses custom encoding for inline images (BI/ID/EI), because
    /// lopdf's Content::encode() incorrectly serializes them as
    /// regular stream objects.
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.operations.iter().any(|op| op.operator == "BI") {
            self.encode_with_inline_images()
        } else {
            let content = Content {
                operations: self.operations.clone(),
            };
            content
                .encode()
                .map_err(|e| ManipError::Other(format!("content encode: {e}")))
        }
    }

    /// Custom encoder that handles inline images correctly.
    fn encode_with_inline_images(&self) -> Result<Vec<u8>> {
        // Split operations: inline images get custom encoding,
        // non-BI segments use lopdf's standard encoder.
        let mut buf = Vec::new();
        let mut segment: Vec<Operation> = Vec::new();

        for op in &self.operations {
            if op.operator == "BI" {
                // Flush any pending normal operations
                if !segment.is_empty() {
                    let content = Content {
                        operations: std::mem::take(&mut segment),
                    };
                    let encoded = content
                        .encode()
                        .map_err(|e| ManipError::Other(format!("content encode: {e}")))?;
                    if !buf.is_empty() {
                        buf.push(b'\n');
                    }
                    buf.extend_from_slice(&encoded);
                }

                // Encode inline image: BI <dict> ID <data> EI
                if let Some(Object::Stream(ref stream)) = op.operands.first() {
                    if !buf.is_empty() {
                        buf.push(b'\n');
                    }
                    buf.extend_from_slice(b"BI\n");
                    for (key, val) in &stream.dict {
                        // Skip internal Stream keys not part of inline image dict
                        if key == b"Length" || key == b"Filter" || key == b"DecodeParms" {
                            continue;
                        }
                        buf.push(b'/');
                        buf.extend_from_slice(key);
                        buf.push(b' ');
                        write_inline_value(&mut buf, val);
                        buf.push(b'\n');
                    }
                    buf.extend_from_slice(b"ID ");
                    buf.extend_from_slice(&stream.content);
                    buf.extend_from_slice(b"\nEI");
                }
            } else {
                segment.push(op.clone());
            }
        }

        // Flush remaining normal operations
        if !segment.is_empty() {
            let content = Content {
                operations: segment,
            };
            let encoded = content
                .encode()
                .map_err(|e| ManipError::Other(format!("content encode: {e}")))?;
            if !buf.is_empty() {
                buf.push(b'\n');
            }
            buf.extend_from_slice(&encoded);
        }

        Ok(buf)
    }

    /// Encode and compress with flate.
    pub fn encode_compressed(&self) -> Result<Vec<u8>> {
        let raw = self.encode()?;
        let compressed = compress_flate(&raw);
        if compressed.len() < raw.len() {
            Ok(compressed)
        } else {
            Ok(raw)
        }
    }

    /// Build a `GraphicsStateTracker` that walks all operations and
    /// records the graphics state at each operation index.
    pub fn track_state(&self) -> GraphicsStateTracker {
        GraphicsStateTracker::from_operations(&self.operations)
    }
}

impl Default for ContentEditor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Graphics state tracking
// ---------------------------------------------------------------------------

/// Snapshot of graphics + text state at a point in the content stream.
#[derive(Debug, Clone)]
pub struct GraphicsSnapshot {
    /// Current transformation matrix [a, b, c, d, e, f].
    pub ctm: [f64; 6],
    /// Fill color (RGB).
    pub fill_color: [f64; 3],
    /// Stroke color (RGB).
    pub stroke_color: [f64; 3],
    /// Current font name (from Tf operator).
    pub font_name: String,
    /// Current font size (from Tf operator).
    pub font_size: f64,
    /// Text matrix [a, b, c, d, e, f].
    pub text_matrix: [f64; 6],
    /// Text line matrix [a, b, c, d, e, f].
    pub text_line_matrix: [f64; 6],
    /// Character spacing (Tc).
    pub char_spacing: f64,
    /// Word spacing (Tw).
    pub word_spacing: f64,
    /// Horizontal scaling (Tz), percentage.
    pub horiz_scaling: f64,
    /// Text leading (TL).
    pub leading: f64,
    /// Text rise (Ts).
    pub text_rise: f64,
    /// Whether we are inside a BT..ET block.
    pub in_text_object: bool,
}

impl Default for GraphicsSnapshot {
    fn default() -> Self {
        Self {
            ctm: IDENTITY,
            fill_color: [0.0; 3],
            stroke_color: [0.0; 3],
            font_name: String::new(),
            font_size: 0.0,
            text_matrix: IDENTITY,
            text_line_matrix: IDENTITY,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horiz_scaling: 100.0,
            leading: 0.0,
            text_rise: 0.0,
            in_text_object: false,
        }
    }
}

const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Tracks graphics state across a content stream.
///
/// After construction, `state_at(index)` returns the graphics state
/// *before* that operation executes.
#[derive(Debug, Clone)]
pub struct GraphicsStateTracker {
    snapshots: Vec<GraphicsSnapshot>,
}

impl GraphicsStateTracker {
    /// Walk operations and record a snapshot before each one.
    pub fn from_operations(ops: &[Operation]) -> Self {
        let mut snapshots = Vec::with_capacity(ops.len());
        let mut state = GraphicsSnapshot::default();
        let mut gs_stack: Vec<SavedState> = Vec::new();

        for op in ops {
            snapshots.push(state.clone());

            match op.operator.as_str() {
                "q" => {
                    gs_stack.push(SavedState {
                        ctm: state.ctm,
                        fill_color: state.fill_color,
                        stroke_color: state.stroke_color,
                        font_name: state.font_name.clone(),
                        font_size: state.font_size,
                        char_spacing: state.char_spacing,
                        word_spacing: state.word_spacing,
                        horiz_scaling: state.horiz_scaling,
                        leading: state.leading,
                        text_rise: state.text_rise,
                    });
                }
                "Q" => {
                    if let Some(saved) = gs_stack.pop() {
                        state.ctm = saved.ctm;
                        state.fill_color = saved.fill_color;
                        state.stroke_color = saved.stroke_color;
                        state.font_name = saved.font_name;
                        state.font_size = saved.font_size;
                        state.char_spacing = saved.char_spacing;
                        state.word_spacing = saved.word_spacing;
                        state.horiz_scaling = saved.horiz_scaling;
                        state.leading = saved.leading;
                        state.text_rise = saved.text_rise;
                    }
                }
                "cm" => {
                    if let Some(m) = extract_matrix(&op.operands) {
                        state.ctm = multiply_matrix(&state.ctm, &m);
                    }
                }
                "BT" => {
                    state.in_text_object = true;
                    state.text_matrix = IDENTITY;
                    state.text_line_matrix = IDENTITY;
                }
                "ET" => {
                    state.in_text_object = false;
                }
                "Tf" => {
                    if op.operands.len() >= 2 {
                        if let Object::Name(ref name) = op.operands[0] {
                            state.font_name = String::from_utf8_lossy(name).to_string();
                        }
                        if let Some(size) = as_number(&op.operands[1]) {
                            state.font_size = size;
                        }
                    }
                }
                "Tm" => {
                    if let Some(m) = extract_matrix(&op.operands) {
                        state.text_matrix = m;
                        state.text_line_matrix = m;
                    }
                }
                "Td" => {
                    if op.operands.len() >= 2 {
                        let tx = as_number(&op.operands[0]).unwrap_or(0.0);
                        let ty = as_number(&op.operands[1]).unwrap_or(0.0);
                        let new_tlm =
                            multiply_matrix(&state.text_line_matrix, &[1.0, 0.0, 0.0, 1.0, tx, ty]);
                        state.text_line_matrix = new_tlm;
                        state.text_matrix = new_tlm;
                    }
                }
                "TD" => {
                    if op.operands.len() >= 2 {
                        let tx = as_number(&op.operands[0]).unwrap_or(0.0);
                        let ty = as_number(&op.operands[1]).unwrap_or(0.0);
                        state.leading = -ty;
                        let new_tlm =
                            multiply_matrix(&state.text_line_matrix, &[1.0, 0.0, 0.0, 1.0, tx, ty]);
                        state.text_line_matrix = new_tlm;
                        state.text_matrix = new_tlm;
                    }
                }
                "T*" => {
                    let new_tlm = multiply_matrix(
                        &state.text_line_matrix,
                        &[1.0, 0.0, 0.0, 1.0, 0.0, -state.leading],
                    );
                    state.text_line_matrix = new_tlm;
                    state.text_matrix = new_tlm;
                }
                "Tc" => {
                    if let Some(v) = op.operands.first().and_then(as_number) {
                        state.char_spacing = v;
                    }
                }
                "Tw" => {
                    if let Some(v) = op.operands.first().and_then(as_number) {
                        state.word_spacing = v;
                    }
                }
                "Tz" => {
                    if let Some(v) = op.operands.first().and_then(as_number) {
                        state.horiz_scaling = v;
                    }
                }
                "TL" => {
                    if let Some(v) = op.operands.first().and_then(as_number) {
                        state.leading = v;
                    }
                }
                "Ts" => {
                    if let Some(v) = op.operands.first().and_then(as_number) {
                        state.text_rise = v;
                    }
                }
                "rg" => {
                    if op.operands.len() >= 3 {
                        state.fill_color = [
                            as_number(&op.operands[0]).unwrap_or(0.0),
                            as_number(&op.operands[1]).unwrap_or(0.0),
                            as_number(&op.operands[2]).unwrap_or(0.0),
                        ];
                    }
                }
                "g" => {
                    if let Some(v) = op.operands.first().and_then(as_number) {
                        state.fill_color = [v, v, v];
                    }
                }
                "RG" => {
                    if op.operands.len() >= 3 {
                        state.stroke_color = [
                            as_number(&op.operands[0]).unwrap_or(0.0),
                            as_number(&op.operands[1]).unwrap_or(0.0),
                            as_number(&op.operands[2]).unwrap_or(0.0),
                        ];
                    }
                }
                "G" => {
                    if let Some(v) = op.operands.first().and_then(as_number) {
                        state.stroke_color = [v, v, v];
                    }
                }
                _ => {}
            }
        }

        Self { snapshots }
    }

    /// Get the graphics state snapshot *before* operation at `index`.
    pub fn state_at(&self, index: usize) -> Option<&GraphicsSnapshot> {
        self.snapshots.get(index)
    }

    /// Number of tracked snapshots (equals number of operations).
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Whether there are no snapshots.
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Page-level helpers
// ---------------------------------------------------------------------------

/// Load a `ContentEditor` from a page in a document.
pub fn editor_for_page(doc: &Document, page_num: u32) -> Result<ContentEditor> {
    let pages = doc.get_pages();
    let total = pages.len() as u32;
    let &page_id = pages.get(&page_num).ok_or(ManipError::PageOutOfRange(
        page_num as usize,
        total as usize,
    ))?;

    let content_bytes = doc
        .get_page_content(page_id)
        .map_err(|e| ManipError::Other(format!("get page content: {e}")))?;

    ContentEditor::from_stream(&content_bytes)
}

/// Write modified content back to a page's content stream(s).
pub fn write_editor_to_page(
    doc: &mut Document,
    page_num: u32,
    editor: &ContentEditor,
) -> Result<()> {
    let pages = doc.get_pages();
    let total = pages.len() as u32;
    let &page_id = pages.get(&page_num).ok_or(ManipError::PageOutOfRange(
        page_num as usize,
        total as usize,
    ))?;

    let encoded = editor.encode()?;
    let compressed = compress_flate(&encoded);

    let (stream_bytes, use_flate) = if compressed.len() < encoded.len() {
        (compressed, true)
    } else {
        (encoded, false)
    };

    let content_ids = get_content_stream_ids(doc, page_id);

    if let Some(&first_id) = content_ids.first() {
        if let Ok(Object::Stream(ref mut s)) = doc.get_object_mut(first_id) {
            s.content = stream_bytes;
            if use_flate {
                s.dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
            } else {
                s.dict.remove(b"Filter");
            }
            s.dict
                .set("Length", Object::Integer(s.content.len() as i64));
        }

        if content_ids.len() > 1 {
            if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
                page_dict.set("Contents", Object::Reference(first_id));
            }
        }
    } else {
        let mut dict = lopdf::Dictionary::new();
        if use_flate {
            dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
        }
        dict.set("Length", Object::Integer(stream_bytes.len() as i64));
        let stream = Stream::new(dict, stream_bytes);
        let new_id = doc.add_object(Object::Stream(stream));

        if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
            page_dict.set("Contents", Object::Reference(new_id));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SavedState {
    ctm: [f64; 6],
    fill_color: [f64; 3],
    stroke_color: [f64; 3],
    font_name: String,
    font_size: f64,
    char_spacing: f64,
    word_spacing: f64,
    horiz_scaling: f64,
    leading: f64,
    text_rise: f64,
}

/// Get content stream object IDs for a page.
pub fn get_content_stream_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let page_obj = match doc.get_object(page_id) {
        Ok(obj) => obj,
        Err(_) => return Vec::new(),
    };
    let page_dict = match page_obj {
        Object::Dictionary(ref d) => d,
        _ => return Vec::new(),
    };
    match page_dict.get(b"Contents") {
        Ok(Object::Reference(id)) => {
            // Dereference: if the target is an Array (indirect content array),
            // extract stream IDs from it. Otherwise treat as a single stream.
            match doc.get_object(*id) {
                Ok(Object::Array(arr)) => arr
                    .iter()
                    .filter_map(|obj| {
                        if let Object::Reference(ref_id) = obj {
                            Some(*ref_id)
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => vec![*id],
            }
        }
        Ok(Object::Array(arr)) => arr
            .iter()
            .filter_map(|obj| {
                if let Object::Reference(id) = obj {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Convert a PDF object to f64.
pub(crate) fn as_number(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(f) => Some(*f as f64),
        _ => None,
    }
}

/// Extract a 6-element transformation matrix from operands.
fn extract_matrix(operands: &[Object]) -> Option<[f64; 6]> {
    if operands.len() < 6 {
        return None;
    }
    Some([
        as_number(&operands[0])?,
        as_number(&operands[1])?,
        as_number(&operands[2])?,
        as_number(&operands[3])?,
        as_number(&operands[4])?,
        as_number(&operands[5])?,
    ])
}

/// Multiply two 3x3 transformation matrices stored as [a, b, c, d, e, f].
pub(crate) fn multiply_matrix(m1: &[f64; 6], m2: &[f64; 6]) -> [f64; 6] {
    [
        m1[0] * m2[0] + m1[1] * m2[2],
        m1[0] * m2[1] + m1[1] * m2[3],
        m1[2] * m2[0] + m1[3] * m2[2],
        m1[2] * m2[1] + m1[3] * m2[3],
        m1[4] * m2[0] + m1[5] * m2[2] + m2[4],
        m1[4] * m2[1] + m1[5] * m2[3] + m2[5],
    ]
}

/// Write a PDF object value in inline image dictionary format.
fn write_inline_value(buf: &mut Vec<u8>, obj: &Object) {
    match obj {
        Object::Integer(n) => buf.extend_from_slice(n.to_string().as_bytes()),
        Object::Real(n) => {
            // Use compact float formatting
            let s = if n.fract() == 0.0 {
                format!("{n:.1}")
            } else {
                format!("{n}")
            };
            buf.extend_from_slice(s.as_bytes());
        }
        Object::Boolean(b) => {
            buf.extend_from_slice(if *b { b"true" } else { b"false" });
        }
        Object::Name(name) => {
            buf.push(b'/');
            buf.extend_from_slice(name);
        }
        Object::String(s, _) => {
            buf.push(b'(');
            buf.extend_from_slice(s);
            buf.push(b')');
        }
        Object::Array(arr) => {
            buf.push(b'[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    buf.push(b' ');
                }
                write_inline_value(buf, item);
            }
            buf.push(b']');
        }
        Object::Null => buf.extend_from_slice(b"null"),
        _ => buf.extend_from_slice(b"null"),
    }
}

/// Strip `BI … EI` inline-image blocks from a content stream.
///
/// `lopdf::content::Content::decode` cannot handle the binary payload that
/// follows the `ID` keyword inside an inline image block.  This function
/// removes each block and returns:
///   - the cleaned bytes (each block replaced by a single `\n`), and
///   - the verbatim raw bytes for every extracted block (in document order).
///
/// Callers that need to preserve inline images should prepend the raw blobs
/// back to the re-encoded content after editing.
pub fn strip_inline_images(content: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut stripped = Vec::with_capacity(content.len());
    let mut images: Vec<Vec<u8>> = Vec::new();
    let mut i = 0;

    while i < content.len() {
        // Detect the "BI" keyword at a word boundary.
        if i + 2 <= content.len()
            && content[i] == b'B'
            && content[i + 1] == b'I'
            && (i == 0 || is_ws_or_delim(content[i - 1]))
            && (i + 2 >= content.len() || is_ws_or_delim(content[i + 2]))
        {
            if let Some(ei_end) = find_ei_end(&content[i..]) {
                images.push(content[i..i + ei_end].to_vec());
                stripped.push(b'\n');
                i += ei_end;
                continue;
            }
        }
        stripped.push(content[i]);
        i += 1;
    }

    (stripped, images)
}

/// Find the offset just past the `EI` keyword that closes the `BI` block
/// starting at `content[0]`.  Returns `None` if the block is malformed.
fn find_ei_end(content: &[u8]) -> Option<usize> {
    // Skip to the "ID" keyword (separates the dict from the binary data).
    let mut j = 2; // skip "BI"
    while j + 2 <= content.len() {
        if content[j] == b'I'
            && content[j + 1] == b'D'
            && (j == 0 || is_ws_or_delim(content[j - 1]))
            && (j + 2 >= content.len()
                || content[j + 2] == b'\r'
                || content[j + 2] == b'\n'
                || is_ws_or_delim(content[j + 2]))
        {
            // Skip the single CRLF / LF that follows "ID".
            let mut k = j + 2;
            if k < content.len() && content[k] == b'\r' {
                k += 1;
            }
            if k < content.len() && content[k] == b'\n' {
                k += 1;
            }
            // Search for "EI" preceded by whitespace.
            while k + 2 <= content.len() {
                if content[k] == b'E'
                    && content[k + 1] == b'I'
                    && k > 0
                    && is_ws_or_delim(content[k - 1])
                    && (k + 2 >= content.len() || is_ws_or_delim(content[k + 2]))
                {
                    return Some(k + 2);
                }
                k += 1;
            }
            return None;
        }
        j += 1;
    }
    None
}

fn is_ws_or_delim(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'\x0C'
            | b'\x00'
            | b'('
            | b')'
            | b'<'
            | b'>'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'/'
            | b'%'
    )
}

/// Compress data with flate/zlib.
fn compress_flate(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    if encoder.write_all(data).is_ok() {
        encoder.finish().unwrap_or_else(|_| data.to_vec())
    } else {
        data.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};

    fn make_doc_with_content(content: &[u8]) -> Document {
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

    #[test]
    fn round_trip_preserves_operators() {
        let input = b"q 1 0 0 1 100 200 cm BT /F1 12 Tf (Hello) Tj ET Q";
        let editor = ContentEditor::from_stream(input).unwrap();
        assert!(!editor.is_empty());

        let encoded = editor.encode().unwrap();
        let editor2 = ContentEditor::from_stream(&encoded).unwrap();
        assert_eq!(editor.len(), editor2.len());

        for (a, b) in editor.operations().iter().zip(editor2.operations().iter()) {
            assert_eq!(a.operator, b.operator);
            assert_eq!(a.operands.len(), b.operands.len());
        }
    }

    #[test]
    fn remove_range() {
        let input = b"q 1 0 0 RG 0 0 100 100 re S Q";
        let mut editor = ContentEditor::from_stream(input).unwrap();
        let original_len = editor.len();
        editor.remove_range(1..2);
        assert_eq!(editor.len(), original_len - 1);
        assert_eq!(editor.operations()[0].operator, "q");
        assert_eq!(editor.operations()[1].operator, "re");
    }

    #[test]
    fn replace_operation() {
        let input = b"1 0 0 rg 0 0 100 100 re f";
        let mut editor = ContentEditor::from_stream(input).unwrap();
        editor.replace_operation(
            0,
            vec![Operation::new(
                "rg",
                vec![Object::Real(0.0), Object::Real(0.0), Object::Real(1.0)],
            )],
        );
        assert_eq!(editor.operations()[0].operator, "rg");
    }

    #[test]
    fn insert_operations() {
        let input = b"q Q";
        let mut editor = ContentEditor::from_stream(input).unwrap();
        assert_eq!(editor.len(), 2);
        editor.insert_operations(
            1,
            vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)]),
                Operation::new(
                    "Tj",
                    vec![Object::String(
                        b"Hello".to_vec(),
                        lopdf::StringFormat::Literal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ],
        );
        assert_eq!(editor.len(), 6);
        assert_eq!(editor.operations()[0].operator, "q");
        assert_eq!(editor.operations()[1].operator, "BT");
        assert_eq!(editor.operations()[5].operator, "Q");
    }

    #[test]
    fn retain_filter() {
        let input = b"q BT /F1 12 Tf (Hello) Tj ET Q";
        let mut editor = ContentEditor::from_stream(input).unwrap();
        editor.retain(|op| !matches!(op.operator.as_str(), "Tj" | "TJ" | "'" | "\""));
        assert!(editor.operations().iter().all(|op| op.operator != "Tj"));
    }

    #[test]
    fn state_tracking_font() {
        let input = b"BT /F1 12 Tf (Hello) Tj /F2 24 Tf (World) Tj ET";
        let editor = ContentEditor::from_stream(input).unwrap();
        let tracker = editor.track_state();

        let s = tracker.state_at(2).unwrap();
        assert_eq!(s.font_name, "F1");
        assert_eq!(s.font_size, 12.0);
        assert!(s.in_text_object);

        let s = tracker.state_at(4).unwrap();
        assert_eq!(s.font_name, "F2");
        assert_eq!(s.font_size, 24.0);
    }

    #[test]
    fn state_tracking_ctm() {
        let input = b"q 2 0 0 2 10 20 cm Q";
        let editor = ContentEditor::from_stream(input).unwrap();
        let tracker = editor.track_state();
        let s = tracker.state_at(2).unwrap();
        assert!((s.ctm[0] - 2.0).abs() < 1e-9);
        assert!((s.ctm[3] - 2.0).abs() < 1e-9);
        assert!((s.ctm[4] - 10.0).abs() < 1e-9);
        assert!((s.ctm[5] - 20.0).abs() < 1e-9);
    }

    #[test]
    fn state_tracking_text_position() {
        let input = b"BT 1 0 0 1 100 700 Tm 50 -20 Td ET";
        let editor = ContentEditor::from_stream(input).unwrap();
        let tracker = editor.track_state();

        let s = tracker.state_at(2).unwrap();
        assert!((s.text_matrix[4] - 100.0).abs() < 1e-9);
        assert!((s.text_matrix[5] - 700.0).abs() < 1e-9);

        let s = tracker.state_at(3).unwrap();
        assert!((s.text_matrix[4] - 150.0).abs() < 1e-9);
        assert!((s.text_matrix[5] - 680.0).abs() < 1e-9);
    }

    #[test]
    fn state_tracking_color() {
        let input = b"1 0 0 rg 0 1 0 RG";
        let editor = ContentEditor::from_stream(input).unwrap();
        let tracker = editor.track_state();
        let s = tracker.state_at(1).unwrap();
        assert!((s.fill_color[0] - 1.0).abs() < 1e-9);
        assert!((s.fill_color[1]).abs() < 1e-9);
    }

    #[test]
    fn state_tracking_save_restore() {
        let input = b"q 1 0 0 rg q 0 1 0 rg Q Q";
        let editor = ContentEditor::from_stream(input).unwrap();
        let tracker = editor.track_state();
        let s = tracker.state_at(4).unwrap();
        assert!((s.fill_color[1] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn editor_for_page_works() {
        let doc = make_doc_with_content(b"BT /F1 12 Tf (Hello) Tj ET");
        let editor = editor_for_page(&doc, 1).unwrap();
        assert!(!editor.is_empty());
    }

    #[test]
    fn write_editor_to_page_roundtrip() {
        let mut doc = make_doc_with_content(b"BT /F1 12 Tf (Hello) Tj ET");
        let editor = editor_for_page(&doc, 1).unwrap();
        let original_len = editor.len();
        write_editor_to_page(&mut doc, 1, &editor).unwrap();
        let editor2 = editor_for_page(&doc, 1).unwrap();
        assert_eq!(editor2.len(), original_len);
    }

    #[test]
    fn write_modified_content() {
        let mut doc = make_doc_with_content(b"BT /F1 12 Tf (Hello) Tj ET");
        let mut editor = editor_for_page(&doc, 1).unwrap();
        editor.insert_operations(0, vec![Operation::new("q", vec![])]);
        editor.push(Operation::new("Q", vec![]));
        write_editor_to_page(&mut doc, 1, &editor).unwrap();
        let editor2 = editor_for_page(&doc, 1).unwrap();
        assert_eq!(editor2.operations()[0].operator, "q");
        assert_eq!(editor2.operations()[editor2.len() - 1].operator, "Q");
    }

    #[test]
    fn empty_content_stream() {
        let editor = ContentEditor::from_stream(b"").unwrap();
        assert!(editor.is_empty());
        assert_eq!(editor.len(), 0);
    }

    #[test]
    fn minimal_inline_image_roundtrip() {
        // Minimal BI/ID/EI test: 4x4 1-bit gray image
        let input =
            b"q 1 0 0 1 0 0 cm BI /W 4 /H 4 /BPC 1 /CS /DeviceGray ID \xF0\xA0\x50\x0F EI Q";
        let editor = ContentEditor::from_stream(input).unwrap();
        let encoded = editor.encode().unwrap();
        let editor2 = ContentEditor::from_stream(&encoded).unwrap();
        assert_eq!(editor.len(), editor2.len());
    }

    #[test]
    fn inline_image_with_text_roundtrip() {
        // BI with surrounding text operations including Tj with strings
        let input = b"q 1 0 0 1 0 0 cm BI /W 4 /H 4 /BPC 1 /CS /DeviceGray ID \xF0\xA0\x50\x0F EI Q BT /F0 12 Tf (Hello World) Tj ET q 1 0 0 1 0 0 cm BI /W 4 /H 4 /BPC 1 /CS /DeviceGray ID \xAA\xBB\xCC\xDD EI Q";
        let editor = ContentEditor::from_stream(input).unwrap();
        let encoded = editor.encode().unwrap();
        let editor2 = ContentEditor::from_stream(&encoded).unwrap();
        assert_eq!(editor.len(), editor2.len(), "op count mismatch");
    }

    #[test]
    fn roundtrip_regression_0119() {
        // Regression: inline images (BI/ID/EI) caused op count loss during roundtrip.
        let pdf_data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/regression-388/roundtrip_0119.pdf"
        ));
        let pdf_data = match pdf_data {
            Ok(d) => d,
            Err(_) => return, // skip if fixture not available
        };
        let doc = Document::load_mem(&pdf_data).unwrap();
        let pages = doc.get_pages();
        let mut tested = 0;
        for (&page_num, &page_id) in &pages {
            let content = match doc.get_page_content(page_id) {
                Ok(c) if !c.is_empty() => c,
                _ => continue,
            };
            let editor = match ContentEditor::from_stream(&content) {
                Ok(e) => e,
                Err(_) => continue, // skip pages lopdf can't decode
            };
            let orig_count = editor.len();
            let encoded = editor.encode().unwrap();
            let editor2 = ContentEditor::from_stream(&encoded).unwrap();
            assert_eq!(
                orig_count,
                editor2.len(),
                "page {page_num}: op count mismatch {orig_count} → {}",
                editor2.len()
            );
            tested += 1;
        }
        assert!(tested > 0, "should test at least one page");
    }

    #[test]
    fn encode_compressed_smaller() {
        let mut editor = ContentEditor::new();
        for _ in 0..100 {
            editor.push(Operation::new(
                "rg",
                vec![Object::Real(1.0), Object::Real(0.0), Object::Real(0.0)],
            ));
        }
        let raw = editor.encode().unwrap();
        let compressed = editor.encode_compressed().unwrap();
        assert!(compressed.len() < raw.len());
    }
}
