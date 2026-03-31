//! Integration tests for the new stream filter decoders.

use lopdf::{Dictionary, Object, Stream};

#[test]
fn test_ascii_hex_decode_stream() {
    let hex_content = b"48656C6C6F20576F726C6421>".to_vec(); // "Hello World!"
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"ASCIIHexDecode".to_vec()));
    dict.set("Length", Object::Integer(hex_content.len() as i64));
    let stream = Stream::new(dict, hex_content);
    let decoded = stream.decompressed_content().expect("ASCIIHexDecode should work");
    assert_eq!(decoded, b"Hello World!");
}

#[test]
fn test_ascii_hex_decode_abbreviation() {
    let hex_content = b"4142>".to_vec(); // "AB"
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"AHx".to_vec()));
    dict.set("Length", Object::Integer(hex_content.len() as i64));
    let stream = Stream::new(dict, hex_content);
    let decoded = stream.decompressed_content().expect("AHx abbreviation should work");
    assert_eq!(decoded, b"AB");
}

#[test]
fn test_run_length_decode_stream() {
    // Literal run of 5 bytes, then repeat byte 3 four times, then EOD
    let rl_content = vec![4, 10, 11, 12, 13, 14, 253, 3, 128];
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"RunLengthDecode".to_vec()));
    dict.set("Length", Object::Integer(rl_content.len() as i64));
    let stream = Stream::new(dict, rl_content);
    let decoded = stream.decompressed_content().expect("RunLengthDecode should work");
    assert_eq!(decoded, vec![10, 11, 12, 13, 14, 3, 3, 3, 3]);
}

#[test]
fn test_run_length_abbreviation() {
    let rl_content = vec![0, 42, 128]; // literal 1 byte, EOD
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"RL".to_vec()));
    dict.set("Length", Object::Integer(rl_content.len() as i64));
    let stream = Stream::new(dict, rl_content);
    let decoded = stream.decompressed_content().expect("RL abbreviation should work");
    assert_eq!(decoded, vec![42]);
}

#[test]
fn test_dct_passthrough() {
    // DCTDecode should just return the input bytes (JPEG passthrough)
    let jpeg_stub = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x02, 0xFF, 0xD9];
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    dict.set("Length", Object::Integer(jpeg_stub.len() as i64));
    let stream = Stream::new(dict, jpeg_stub.clone());
    let decoded = stream.decompressed_content().expect("DCTDecode passthrough should work");
    assert_eq!(decoded, jpeg_stub);
}

#[test]
fn test_chained_ascii_hex_then_flate() {
    // Chain: ASCIIHexDecode → FlateDecode
    // First, create FlateDecode compressed data for "test data"
    use std::io::Write;
    let original = b"test data for chained filter";
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(original).unwrap();
    let compressed = encoder.finish().unwrap();

    // Hex-encode the compressed data
    let hex: Vec<u8> = compressed
        .iter()
        .flat_map(|b| format!("{b:02X}").into_bytes())
        .chain(b">".iter().copied())
        .collect();

    let mut dict = Dictionary::new();
    // Filter array: first ASCIIHexDecode, then FlateDecode
    dict.set(
        "Filter",
        Object::Array(vec![
            Object::Name(b"ASCIIHexDecode".to_vec()),
            Object::Name(b"FlateDecode".to_vec()),
        ]),
    );
    dict.set("Length", Object::Integer(hex.len() as i64));
    let stream = Stream::new(dict, hex);
    let decoded = stream
        .decompressed_content()
        .expect("Chained ASCIIHex+Flate should work");
    assert_eq!(decoded, original);
}
