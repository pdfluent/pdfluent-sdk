//! Integration tests for the new stream filter decoders.

use pdfluent_lopdf::{Dictionary, Error, Object, Stream};

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
    let decoded = stream
        .decompressed_content()
        .expect("DCTDecode passthrough should work");
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

// ══════════════════════════════════════════════════════════════════════════════
// ASCIIHexDecode edge cases
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_ascii_hex_decode_odd_nibble_trailing() {
    // Odd number of hex digits: trailing nibble padded with 0 (per PDF spec)
    // "A" alone should produce [0xA0] (padded with 0)
    let hex_content = b"A".to_vec();
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"ASCIIHexDecode".to_vec()));
    dict.set("Length", Object::Integer(hex_content.len() as i64));
    let stream = Stream::new(dict, hex_content);
    let decoded = stream.decompressed_content().expect("odd hex should pad with 0");
    assert_eq!(decoded, vec![0xA0]);
}

#[test]
fn test_ascii_hex_decode_missing_eod_marker() {
    // Missing EOD marker should still work - just no padding
    let hex_content = b"48656C6C6F".to_vec(); // "Hello" without '>'
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"ASCIIHexDecode".to_vec()));
    dict.set("Length", Object::Integer(hex_content.len() as i64));
    let stream = Stream::new(dict, hex_content);
    let decoded = stream.decompressed_content().expect("missing EOD should still decode");
    assert_eq!(decoded, b"Hello");
}

#[test]
fn test_ascii_hex_decode_with_whitespace() {
    // Whitespace should be ignored per PDF spec
    let hex_content = b"48 65 6C 6C 6F>".to_vec();
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"ASCIIHexDecode".to_vec()));
    dict.set("Length", Object::Integer(hex_content.len() as i64));
    let stream = Stream::new(dict, hex_content);
    let decoded = stream.decompressed_content().expect("whitespace should be ignored");
    assert_eq!(decoded, b"Hello");
}

#[test]
fn test_ascii_hex_decode_lowercase() {
    // Lowercase hex digits should be accepted
    let hex_content = b"68656c6c6f>".to_vec();
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"ASCIIHexDecode".to_vec()));
    dict.set("Length", Object::Integer(hex_content.len() as i64));
    let stream = Stream::new(dict, hex_content);
    let decoded = stream.decompressed_content().expect("lowercase hex should work");
    assert_eq!(decoded, b"hello");
}

#[test]
fn test_ascii_hex_decode_invalid_digit() {
    // Invalid hex digit should return error
    let hex_content = b"48XX6C6C6F>".to_vec();
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"ASCIIHexDecode".to_vec()));
    dict.set("Length", Object::Integer(hex_content.len() as i64));
    let stream = Stream::new(dict, hex_content);
    let result = stream.decompressed_content();
    assert!(result.is_err(), "invalid hex digit should error");
    let err = result.unwrap_err();
    assert!(matches!(err, Error::Decompress(_)), "should be Decompress error");
}

#[test]
fn test_ascii_hex_decode_all_whitespace() {
    // All whitespace with EOD should return empty vec
    let hex_content = b"   \t\n  >".to_vec();
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"ASCIIHexDecode".to_vec()));
    dict.set("Length", Object::Integer(hex_content.len() as i64));
    let stream = Stream::new(dict, hex_content);
    let decoded = stream
        .decompressed_content()
        .expect("all whitespace should return empty");
    assert_eq!(decoded, vec![]);
}

// ══════════════════════════════════════════════════════════════════════════════
// RunLengthDecode edge cases
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_run_length_decode_just_eod() {
    // Just EOD marker should return empty vec
    let rl_content = vec![128];
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"RunLengthDecode".to_vec()));
    dict.set("Length", Object::Integer(rl_content.len() as i64));
    let stream = Stream::new(dict, rl_content);
    let decoded = stream.decompressed_content().expect("EOD only should return empty");
    assert_eq!(decoded, vec![]);
}

#[test]
fn test_run_length_decode_truncated_literal() {
    // Truncated literal run should return what was read before truncation
    // [5, 10, 11, 12] - says "read 6 bytes" but only 3 available
    let rl_content = vec![5, 10, 11, 12];
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"RunLengthDecode".to_vec()));
    dict.set("Length", Object::Integer(rl_content.len() as i64));
    let stream = Stream::new(dict, rl_content);
    let decoded = stream
        .decompressed_content()
        .expect("truncated literal should return partial");
    assert_eq!(decoded, vec![10, 11, 12]);
}

#[test]
fn test_run_length_decode_truncated_repeat() {
    // Truncated repeat run at the very end
    // [255, 10] - says repeat 2 times but EOD immediately after
    let rl_content = vec![255, 10];
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"RunLengthDecode".to_vec()));
    dict.set("Length", Object::Integer(rl_content.len() as i64));
    let stream = Stream::new(dict, rl_content);
    let decoded = stream
        .decompressed_content()
        .expect("truncated repeat should handle gracefully");
    assert_eq!(decoded, vec![10, 10]);
}

#[test]
fn test_run_length_decode_max_literal() {
    // 127 literal bytes (length=127 means 128 bytes)
    let mut rl_content = vec![127];
    rl_content.extend(vec![42u8; 128]);
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"RunLengthDecode".to_vec()));
    dict.set("Length", Object::Integer(rl_content.len() as i64));
    let stream = Stream::new(dict, rl_content);
    let decoded = stream.decompressed_content().expect("max literal should work");
    assert_eq!(decoded.len(), 128);
    assert!(decoded.iter().all(|&b| b == 42));
}

#[test]
fn test_run_length_decode_max_repeat() {
    // 128 repeat (length=129 means repeat 128 times)
    let rl_content = vec![129, 255];
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"RunLengthDecode".to_vec()));
    dict.set("Length", Object::Integer(rl_content.len() as i64));
    let stream = Stream::new(dict, rl_content);
    let decoded = stream.decompressed_content().expect("max repeat should work");
    assert_eq!(decoded.len(), 128);
    assert!(decoded.iter().all(|&b| b == 255));
}

#[test]
fn test_run_length_decode_multiple_literal_runs() {
    // Multiple consecutive literal runs
    // [1, 10, 1, 11, 128] = literal 2 bytes (10,11), literal 2 bytes (11,11?), EOD
    let rl_content = vec![1, 10, 11, 1, 20, 21, 128];
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"RunLengthDecode".to_vec()));
    dict.set("Length", Object::Integer(rl_content.len() as i64));
    let stream = Stream::new(dict, rl_content);
    let decoded = stream
        .decompressed_content()
        .expect("multiple literal runs should work");
    // 1 = copy 2 bytes: [10, 11]
    // 1 = copy 2 bytes: [20, 21]
    // 128 = EOD
    assert_eq!(decoded, vec![10, 11, 20, 21]);
}

// ══════════════════════════════════════════════════════════════════════════════
// FlateDecode edge cases
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_flate_decode_empty_input() {
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    dict.set("Length", Object::Integer(0_i64));
    let stream = Stream::new(dict, vec![]);
    let result = stream.decompressed_content();
    assert!(result.is_ok(), "empty FlateDecode should not error");
    assert_eq!(result.unwrap(), vec![]);
}

#[test]
fn test_flate_decode_truncated_data() {
    use std::io::Write;
    let original = b"test data for truncated flate";
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(original).unwrap();
    let compressed = encoder.finish().unwrap();
    let truncated = &compressed[..compressed.len() / 2];
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    dict.set("Length", Object::Integer(truncated.len() as i64));
    let stream = Stream::new(dict, truncated.to_vec());
    let result = stream.decompressed_content();
    assert!(result.is_ok(), "truncated FlateDecode should return partial");
}

#[test]
fn test_flate_decode_single_byte_zlib_header() {
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    dict.set("Length", Object::Integer(1_i64));
    let stream = Stream::new(dict, vec![0x78]);
    let result = stream.decompressed_content();
    assert!(result.is_ok(), "single byte zlib header should not panic");
}

#[test]
fn test_flate_decode_with_predictor() {
    use std::io::Write;
    let original = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&original).unwrap();
    let compressed = encoder.finish().unwrap();
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    dict.set(
        "DecodeParms",
        Object::Dictionary({
            let mut parms = Dictionary::new();
            parms.set("Predictor", Object::Integer(1));
            parms
        }),
    );
    dict.set("Length", Object::Integer(compressed.len() as i64));
    let stream = Stream::new(dict, compressed);
    let result = stream.decompressed_content();
    assert!(result.is_ok(), "FlateDecode with predictor=1 should work");
}

// ══════════════════════════════════════════════════════════════════════════════
// LZWDecode edge cases
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_lzw_decode_empty_input() {
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"LZWDecode".to_vec()));
    dict.set("Length", Object::Integer(0_i64));
    let stream = Stream::new(dict, vec![]);
    let result = stream.decompressed_content();
    assert!(result.is_ok(), "empty LZWDecode should not error");
    assert_eq!(result.unwrap(), vec![]);
}

#[test]
fn test_lzw_decode_early_change_zero() {
    let input = vec![
        0x80, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F, 0x80,
    ];
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"LZWDecode".to_vec()));
    dict.set(
        "DecodeParms",
        Object::Dictionary({
            let mut parms = Dictionary::new();
            parms.set("EarlyChange", Object::Integer(0));
            parms
        }),
    );
    dict.set("Length", Object::Integer(input.len() as i64));
    let stream = Stream::new(dict, input);
    let result = stream.decompressed_content();
    assert!(result.is_ok(), "LZWDecode with EarlyChange=0 should work");
}

#[test]
fn test_lzw_decode_early_change_one() {
    let input = vec![
        0x80, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F, 0x80,
    ];
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"LZWDecode".to_vec()));
    dict.set(
        "DecodeParms",
        Object::Dictionary({
            let mut parms = Dictionary::new();
            parms.set("EarlyChange", Object::Integer(1));
            parms
        }),
    );
    dict.set("Length", Object::Integer(input.len() as i64));
    let stream = Stream::new(dict, input);
    let result = stream.decompressed_content();
    assert!(result.is_ok(), "LZWDecode with EarlyChange=1 should work");
}

#[test]
fn test_lzw_decode_invalid_sequence() {
    let input = vec![0xFF, 0xFF, 0xFF, 0xFF];
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"LZWDecode".to_vec()));
    dict.set("Length", Object::Integer(input.len() as i64));
    let stream = Stream::new(dict, input);
    let result = stream.decompressed_content();
    assert!(result.is_ok(), "invalid LZW should not panic, just return partial");
}

// ══════════════════════════════════════════════════════════════════════════════
// ASCII85Decode edge cases
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_ascii85_decode_z_shorthand() {
    let input = b"z~>";
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"ASCII85Decode".to_vec()));
    dict.set("Length", Object::Integer(input.len() as i64));
    let stream = Stream::new(dict, input.to_vec());
    let decoded = stream
        .decompressed_content()
        .expect("z shorthand should produce 4 null bytes");
    assert_eq!(decoded, vec![0, 0, 0, 0]);
}

#[test]
fn test_ascii85_decode_missing_eod() {
    let input = b"BOu!rDZ";
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"ASCII85Decode".to_vec()));
    dict.set("Length", Object::Integer(input.len() as i64));
    let stream = Stream::new(dict, input.to_vec());
    let result = stream.decompressed_content();
    assert!(result.is_ok(), "missing EOD should still decode");
}

#[test]
fn test_ascii85_decode_partial_group() {
    let input = b"AB";
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"ASCII85Decode".to_vec()));
    dict.set("Length", Object::Integer(input.len() as i64));
    let stream = Stream::new(dict, input.to_vec());
    let decoded = stream
        .decompressed_content()
        .expect("partial group should decode with padding");
    assert_eq!(decoded, vec![100]); // 'd'
}

#[test]
fn test_ascii85_decode_empty_input() {
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"ASCII85Decode".to_vec()));
    dict.set("Length", Object::Integer(0_i64));
    let stream = Stream::new(dict, vec![]);
    let result = stream.decompressed_content();
    assert!(result.is_ok(), "empty ASCII85 should return empty");
    assert_eq!(result.unwrap(), vec![]);
}

// ══════════════════════════════════════════════════════════════════════════════
// DCTDecode (JPEG) edge cases
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_dct_decode_empty_input() {
    // Empty DCT input - should pass through
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    dict.set("Length", Object::Integer(0_i64));
    let stream = Stream::new(dict, vec![]);
    let result = stream.decompressed_content();
    // DCT is passthrough, so empty should return empty vec
    assert!(result.is_ok(), "empty DCT should not panic");
    assert_eq!(result.unwrap(), vec![]);
}

#[test]
fn test_dct_decode_truncated_jpeg() {
    // Truncated JPEG data should pass through (DCT is passthrough)
    let truncated_jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0]; // SOI and APP0 header only
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    dict.set("Length", Object::Integer(truncated_jpeg.len() as i64));
    let stream = Stream::new(dict, truncated_jpeg.clone());
    let decoded = stream
        .decompressed_content()
        .expect("truncated JPEG should pass through");
    assert_eq!(decoded, truncated_jpeg);
}

// ══════════════════════════════════════════════════════════════════════════════
// Chained filter edge cases
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_chained_multiple_filters() {
    // Chain: ASCIIHexDecode → FlateDecode
    use std::io::Write;
    let original = b"multi-chain test data here";
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(original).unwrap();
    let compressed = encoder.finish().unwrap();

    // Hex-encode
    let hex: Vec<u8> = compressed
        .iter()
        .flat_map(|b| format!("{b:02X}").into_bytes())
        .chain(b">".iter().copied())
        .collect();

    let mut dict = Dictionary::new();
    dict.set(
        "Filter",
        Object::Array(vec![
            Object::Name(b"ASCIIHexDecode".to_vec()),
            Object::Name(b"FlateDecode".to_vec()),
        ]),
    );
    dict.set("Length", Object::Integer(hex.len() as i64));
    let stream = Stream::new(dict, hex);
    let decoded = stream.decompressed_content().expect("chained filters should work");
    assert_eq!(decoded, original);
}
