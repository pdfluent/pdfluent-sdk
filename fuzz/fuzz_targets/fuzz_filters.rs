#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz PDF stream filters (FlateDecode, ASCII85, LZW, etc.)
    if data.len() < 2 {
        return;
    }

    let filter_name = match data[0] % 5 {
        0 => "FlateDecode",
        1 => "ASCII85Decode",
        2 => "ASCIIHexDecode",
        3 => "LZWDecode",
        4 => "RunLengthDecode",
        _ => unreachable!(),
    };

    let stream_data = &data[1..];
    let stream_len = stream_data.len();

    let mut pdf_bytes = b"%PDF-1.4\n1 0 obj\n<< /Length ".to_vec();
    pdf_bytes.extend_from_slice(format!("{stream_len}").as_bytes());
    pdf_bytes.extend_from_slice(b" /Filter /");
    pdf_bytes.extend_from_slice(filter_name.as_bytes());
    pdf_bytes.extend_from_slice(b" >>\nstream\n");
    pdf_bytes.extend_from_slice(stream_data);
    pdf_bytes.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_start = pdf_bytes.len();
    pdf_bytes.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n0000000009 00000 n \n");
    pdf_bytes.extend_from_slice(b"trailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n");
    pdf_bytes.extend_from_slice(format!("{xref_start}").as_bytes());
    pdf_bytes.extend_from_slice(b"\n%%EOF");

    let _ = pdf_syntax::Pdf::new(pdf_bytes);
});
