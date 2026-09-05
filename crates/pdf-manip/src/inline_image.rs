// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Preserve encoded inline images across content edits. lopdf's generic content
//! parser does not decode filtered inline images and can return an empty BI
//! operation. Never turn that lossy parse into a successful document rewrite.

use crate::{ManipError, Result};
use lopdf::{
    content::{Content, Operation},
    Dictionary, Object, Stream,
};

fn invalid() -> ManipError {
    ManipError::Other("unsupported or malformed inline image; content left unchanged".into())
}
fn ws(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}
fn delimiter(b: u8) -> bool {
    ws(b) || b"()<>[]{}/%".contains(&b)
}

// Return lexical token spans, skipping comments and treating strings/names as
// indivisible tokens. A literal '( BI ... )' must not become an image.
fn token(data: &[u8], at: &mut usize) -> Result<Option<(usize, usize)>> {
    while *at < data.len() {
        if ws(data[*at]) {
            *at += 1;
            continue;
        }
        if data[*at] == b'%' {
            while *at < data.len() && !matches!(data[*at], b'\n' | b'\r') {
                *at += 1;
            }
            continue;
        }
        break;
    }
    if *at == data.len() {
        return Ok(None);
    }
    let start = *at;
    *at += 1;
    match data[start] {
        b'(' => {
            let mut depth = 1;
            while *at < data.len() && depth > 0 {
                match data[*at] {
                    b'\\' => {
                        *at += 1;
                        if *at < data.len() {
                            *at += 1;
                        }
                        continue;
                    }
                    b'(' => {
                        depth += 1;
                        if depth > 64 {
                            return Err(invalid());
                        }
                    }
                    b')' => depth -= 1,
                    _ => {}
                }
                *at += 1;
            }
            if depth != 0 {
                return Err(invalid());
            }
        }
        b'<' if data.get(*at) == Some(&b'<') => *at += 1,
        b'<' => {
            while data.get(*at).is_some_and(|b| *b != b'>') {
                *at += 1;
            }
            if *at == data.len() {
                return Err(invalid());
            }
            *at += 1;
        }
        b'>' if data.get(*at) == Some(&b'>') => *at += 1,
        b'/' => {
            while data.get(*at).is_some_and(|b| !delimiter(*b)) {
                *at += 1;
            }
        }
        b'[' | b']' | b'{' | b'}' | b')' | b'>' => {}
        _ => {
            while data.get(*at).is_some_and(|b| !delimiter(*b)) {
                *at += 1;
            }
        }
    }
    Ok(Some((start, *at)))
}

pub(crate) fn flate_frame_length(data: &[u8]) -> Option<usize> {
    use flate2::{Decompress, FlushDecompress, Status};
    let mut decoder = Decompress::new(true);
    let mut buffer = [0; 8192];
    loop {
        let before = (decoder.total_in(), decoder.total_out());
        let status = decoder
            .decompress(
                &data[before.0 as usize..],
                &mut buffer,
                FlushDecompress::None,
            )
            .ok()?;
        if decoder.total_out() > crate::flate_decode::MAX_DEFLATE_BYTES {
            return None;
        }
        if status == Status::StreamEnd {
            return Some(decoder.total_in() as usize);
        }
        if before == (decoder.total_in(), decoder.total_out()) {
            return None;
        }
    }
}

fn jpeg_length(data: &[u8]) -> Option<usize> {
    if data.get(..2) != Some(b"\xff\xd8") {
        return None;
    }
    let mut at = 2;
    loop {
        if *data.get(at)? != 0xff {
            at += 1;
            continue;
        }
        while data.get(at) == Some(&0xff) {
            at += 1;
        }
        let marker = *data.get(at)?;
        at += 1;
        match marker {
            0xd9 => return Some(at),
            0 | 0x01 | 0xd0..=0xd8 => continue,
            _ => {
                let length = u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?) as usize;
                if length < 2 {
                    return None;
                }
                at = at.checked_add(length)?;
            }
        }
    }
}

fn payload_length(dict: &Dictionary, data: &[u8]) -> Option<usize> {
    let filter = dict.get(b"F").or_else(|_| dict.get(b"Filter")).ok();
    if let Some(filter) = filter {
        let filter = match filter {
            Object::Array(a) => a.first()?,
            f => f,
        };
        return match filter.as_name().ok()? {
            b"Fl" | b"FlateDecode" => flate_frame_length(data),
            b"AHx" | b"ASCIIHexDecode" => {
                let end = data.iter().position(|b| *b == b'>')?;
                data[..end]
                    .iter()
                    .all(|b| ws(*b) || b.is_ascii_hexdigit())
                    .then_some(end + 1)
            }
            b"A85" | b"ASCII85Decode" => data.windows(2).position(|w| w == b"~>").map(|n| n + 2),
            b"DCT" | b"DCTDecode" => jpeg_length(data),
            b"RL" | b"RunLengthDecode" => {
                let mut at = 0;
                loop {
                    let n = *data.get(at)?;
                    at += 1;
                    if n == 128 {
                        break Some(at);
                    }
                    at = at.checked_add(if n < 128 { usize::from(n) + 1 } else { 1 })?;
                }
            }
            _ => None,
        };
    }
    let number = |short: &[u8], long: &[u8]| {
        dict.get(short)
            .or_else(|_| dict.get(long))
            .ok()?
            .as_i64()
            .ok()
            .and_then(|n| usize::try_from(n).ok())
    };
    let width = number(b"W", b"Width")?;
    let height = number(b"H", b"Height")?;
    let mask = dict
        .get(b"IM")
        .or_else(|_| dict.get(b"ImageMask"))
        .ok()
        .and_then(|o| o.as_bool().ok())
        == Some(true);
    let (bits, components) = if mask {
        (1, 1)
    } else {
        let cs = dict.get(b"CS").or_else(|_| dict.get(b"ColorSpace")).ok()?;
        let cs = match cs {
            Object::Array(a) => a.first()?,
            o => o,
        };
        let components = match cs.as_name().ok()? {
            b"G" | b"DeviceGray" | b"I" | b"Indexed" => 1,
            b"RGB" | b"DeviceRGB" => 3,
            b"CMYK" | b"DeviceCMYK" => 4,
            _ => return None,
        };
        (number(b"BPC", b"BitsPerComponent")?, components)
    };
    if width == 0 || height == 0 || !matches!(bits, 1 | 2 | 4 | 8 | 16) {
        return None;
    }
    width
        .checked_mul(components)?
        .checked_mul(bits)?
        .checked_add(7)?
        .checked_div(8)?
        .checked_mul(height)
}

pub(crate) fn decode(stream: &[u8]) -> Result<Content<Vec<Operation>>> {
    if !stream.windows(2).any(|w| w == b"BI") {
        if super::content_editor::content_stream_too_deeply_nested(stream) {
            return Err(invalid());
        }
        return Content::decode(stream)
            .map_err(|e| ManipError::Other(format!("content decode: {e}")));
    }

    let mut marker = "PdfManipInlineImage".to_string();
    while stream.windows(marker.len()).any(|w| w == marker.as_bytes()) {
        marker.push('X');
    }
    let mut rewritten = Vec::new();
    let mut images = Vec::new();
    let mut at = 0;
    let mut copied = 0;
    while let Some((start, end)) = token(stream, &mut at)? {
        if &stream[start..end] != b"BI" {
            continue;
        }
        let header_start = end;
        let mut depth = 0usize;
        let header_end = loop {
            let (a, b) = token(stream, &mut at)?.ok_or_else(invalid)?;
            match &stream[a..b] {
                b"[" | b"<<" => {
                    depth += 1;
                    if depth > 64 {
                        return Err(invalid());
                    }
                }
                b"]" | b">>" => depth = depth.checked_sub(1).ok_or_else(invalid)?,
                b"ID" if depth == 0 => break a,
                _ => {}
            }
        };
        let mut header = b"<<".to_vec();
        header.extend_from_slice(&stream[header_start..header_end]);
        header.extend_from_slice(b">> PdfInlineHeader");
        let parsed = Content::decode_strict(&header).map_err(|_| invalid())?;
        let dict = parsed
            .operations
            .first()
            .and_then(|o| o.operands.first())
            .and_then(|o| o.as_dict().ok())
            .ok_or_else(invalid)?
            .clone();
        if !stream.get(at).is_some_and(|b| ws(*b)) {
            return Err(invalid());
        }
        if stream.get(at..at + 2) == Some(b"\r\n") {
            at += 2;
        } else {
            at += 1;
        }
        let length = payload_length(&dict, &stream[at..]).ok_or_else(invalid)?;
        let payload_end = at
            .checked_add(length)
            .filter(|n| *n <= stream.len())
            .ok_or_else(invalid)?;
        let image = Stream::new(dict, stream[at..payload_end].to_vec());
        at = payload_end;
        while stream.get(at).is_some_and(|b| ws(*b)) {
            at += 1;
        }
        if stream.get(at..at + 2) != Some(b"EI")
            || stream.get(at + 2).is_some_and(|b| !delimiter(*b))
        {
            return Err(invalid());
        }
        at += 2;
        rewritten.extend_from_slice(&stream[copied..start]);
        rewritten.extend_from_slice(format!(" {} {marker} ", images.len()).as_bytes());
        images.push(image);
        copied = at;
    }
    rewritten.extend_from_slice(&stream[copied..]);
    // The caller's old depth guard must inspect syntax, never compressed bytes.
    if super::content_editor::content_stream_too_deeply_nested(&rewritten) {
        return Err(invalid());
    }
    let mut parsed = Content::decode(&rewritten)
        .map_err(|e| ManipError::Other(format!("content decode: {e}")))?;
    for op in &mut parsed.operations {
        if op.operator == marker {
            let n = op
                .operands
                .first()
                .and_then(|o| o.as_i64().ok())
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(invalid)?;
            let image = images.get(n).ok_or_else(invalid)?;
            *op = Operation::new("BI", vec![Object::Stream(image.clone())]);
        }
    }
    Ok(parsed)
}

/// Repair string operands without interpreting encoded image bytes as strings.
/// Unsupported image framing leaves the complete stream intact; preflight
/// reports that content editing is unavailable for this stream.
pub(crate) fn truncate_image_stream_strings(data: &[u8]) -> Vec<u8> {
    fn visit(object: &mut Object) -> bool {
        match object {
            Object::String(bytes, _) if bytes.len() > 32767 => {
                bytes.truncate(32767);
                true
            }
            Object::Array(items) => items
                .iter_mut()
                .fold(false, |changed, item| visit(item) | changed),
            Object::Dictionary(dict) => dict
                .iter_mut()
                .fold(false, |changed, (_, item)| visit(item) | changed),
            // Stream payloads and dictionaries describe images, not text operands.
            _ => false,
        }
    }
    let Ok(mut editor) = crate::content_editor::ContentEditor::from_stream(data) else {
        return data.to_vec();
    };
    let mut changed = false;
    for operation in editor.operations_mut() {
        for operand in &mut operation.operands {
            changed |= visit(operand);
        }
    }
    if changed {
        editor.encode().unwrap_or_else(|_| data.to_vec())
    } else {
        data.to_vec()
    }
}

/// Surface unsupported content edits in the conversion report instead of
/// letting best-effort callers silently skip an image-bearing stream.
pub(crate) fn editing_warnings(doc: &lopdf::Document) -> Vec<String> {
    let mut ids = std::collections::BTreeSet::new();
    for page in doc.get_pages().values() {
        ids.extend(crate::content_editor::get_content_stream_ids(doc, *page));
    }
    for (id, object) in &doc.objects {
        if let Ok(stream) = object.as_stream() {
            if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Form")
                || stream.dict.has(b"PatternType")
            {
                ids.insert(*id);
            }
        }
    }
    let mut warnings = Vec::new();
    for id in ids {
        let Ok(stream) = doc.get_object(id).and_then(Object::as_stream) else {
            continue;
        };
        let data = if stream.dict.has(b"Filter") {
            stream.decompressed_content().ok()
        } else {
            Some(stream.content.clone())
        };
        match data {
            Some(data) if data.windows(2).any(|w|w==b"BI") && decode(&data).is_err()=>warnings.push(format!("content {} {} contains an unsupported or malformed inline image; safe content editing is unavailable",id.0,id.1)),
            None=>warnings.push(format!("content {} {} could not be decoded for safe editing",id.0,id.1)),
            _=>{}
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_editor::ContentEditor;
    use std::io::Write;

    #[test]
    fn long_string_repair_preserves_binary_payload_and_following_text() {
        let mut data = b"q BI /IM true /W 1 /H 1 /F /CCF ID (".to_vec();
        data.extend(std::iter::repeat_n(b'X', 40000));
        data.extend_from_slice(b")\nEI Q BT (following text) Tj ET");
        assert_eq!(truncate_image_stream_strings(&data), data);
        let payload = vec![b'('; 40000];
        let image = Stream::new(
            lopdf::dictionary! {"IM"=>true,"W"=>320000,"H"=>1},
            payload.clone(),
        );
        let operations = vec![
            Operation::new("BI", vec![Object::Stream(image)]),
            Operation::new("Tj", vec![Object::string_literal(vec![b'A'; 40000])]),
            Operation::new("Tj", vec![Object::string_literal("tail")]),
        ];
        let encoded = ContentEditor::from_operations(operations).encode().unwrap();
        let repaired = truncate_image_stream_strings(&encoded);
        let parsed = ContentEditor::from_stream(&repaired).unwrap();
        assert_eq!(
            parsed.operations()[0].operands[0]
                .as_stream()
                .unwrap()
                .content,
            payload
        );
        assert_eq!(
            parsed.operations()[1].operands[0].as_str().unwrap().len(),
            32767
        );
        assert_eq!(
            parsed.operations()[2].operands[0].as_str().unwrap(),
            b"tail"
        );
    }

    #[test]
    fn editing_text_preserves_flate_payload_predictors_and_image_order() {
        let pixels = b"\nEI\n(BI)\x00\xff";
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::none());
        z.write_all(pixels).unwrap();
        let payload = z.finish().unwrap();
        let mut bytes=b"q BI /Width 10 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /FlateDecode /DecodeParms << /Predictor 1 >> ID ".to_vec();
        bytes.extend_from_slice(&payload);
        bytes.extend_from_slice(b"\nEI Q BT /F 12 Tf ( BI /W 1 ID fake EI ) Tj ET");
        let mut editor = ContentEditor::from_stream(&bytes).unwrap();
        let expected: Vec<_> = editor
            .operations()
            .iter()
            .map(|op| op.operator.clone())
            .collect();
        assert_eq!(expected, ["q", "BI", "Q", "BT", "Tf", "Tj", "ET"]);
        let image = editor.operations()[1].operands[0]
            .as_stream()
            .unwrap()
            .clone();
        assert_eq!(image.content, payload);
        editor.operations_mut()[5].operands[0] = Object::string_literal("edited (text)");
        let encoded = editor.encode().unwrap();
        let reparsed = ContentEditor::from_stream(&encoded).unwrap();
        let after = reparsed.operations()[1].operands[0].as_stream().unwrap();
        assert_eq!(after.content, payload);
        assert_eq!(
            after.dict.get(b"Filter").unwrap(),
            image.dict.get(b"Filter").unwrap()
        );
        assert_eq!(
            after.dict.get(b"DecodeParms").unwrap(),
            image.dict.get(b"DecodeParms").unwrap()
        );
        assert_eq!(
            reparsed.operations()[5].operands[0].as_str().unwrap(),
            b"edited (text)"
        );
    }

    #[test]
    fn encoded_masks_and_binary_array_bytes_are_not_discarded() {
        for (header, data) in [
            (b"/IM true /W 640 /H 1".as_slice(), vec![b'['; 80]),
            (b"/IM true /W 1 /H 1 /F /AHx".as_slice(), b"00>".to_vec()),
            (b"/IM true /W 1 /H 1 /F /A85".as_slice(), b"!!~>".to_vec()),
            (b"/IM true /W 1 /H 1 /F /RL".as_slice(), vec![0, 0, 128]),
        ] {
            let mut bytes = b"q BI ".to_vec();
            bytes.extend_from_slice(header);
            bytes.extend_from_slice(b" ID ");
            bytes.extend_from_slice(&data);
            bytes.extend_from_slice(b"\nEI Q");
            let editor = ContentEditor::from_stream(&bytes).unwrap();
            let encoded = editor.encode().unwrap();
            let result = ContentEditor::from_stream(&encoded).unwrap();
            assert_eq!(
                result.operations()[1].operands[0]
                    .as_stream()
                    .unwrap()
                    .content,
                data
            );
        }
    }

    #[test]
    fn unsupported_image_edits_are_reported_and_empty_bi_cannot_be_written() {
        use lopdf::dictionary;
        let mut doc = lopdf::Document::with_version("1.7");
        let id = doc.add_object(Stream::new(
            dictionary! {"Subtype"=>"Form"},
            b"BI /W 1 /H 1 /F /Unknown ID bytes EI".to_vec(),
        ));
        let warnings = editing_warnings(&doc);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains(&format!("{} {}", id.0, id.1)));
        assert!(
            ContentEditor::from_operations(vec![Operation::new("BI", vec![])])
                .encode()
                .is_err()
        );
    }

    #[test]
    fn unknown_filters_fail_without_authorizing_a_lossy_rewrite() {
        assert!(ContentEditor::from_stream(b"q BI /W 1 /H 1 /F /Unknown ID bytes EI Q").is_err());
        assert!(ContentEditor::from_stream(b"q BI /W 1 /H 1 /BPC 8 /CS /G ID ").is_err());
        let jpeg =
            b"\xff\xd8\xff\xe0\x00\x06\xff\xd9\x00\x00\xff\xda\x00\x02abc\nEI\n\xff\x00x\xff\xd9";
        assert_eq!(jpeg_length(jpeg), Some(jpeg.len()));
        assert_eq!(jpeg_length(&jpeg[..jpeg.len() - 1]), None);
    }
}
