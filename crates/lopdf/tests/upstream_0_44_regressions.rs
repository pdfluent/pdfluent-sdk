//! Regression tests for the upstream fixes taken in the v0.39.0 -> v0.44.0 merge.
//!
//! Every test here is paired with a named mutation: revert the fix, and the test
//! must fail with the real symptom. A test that stays green against unfixed code
//! proves that nothing broke, not that it catches anything, and does not belong
//! in this file.

use pdfluent_lopdf::content::Content;
use pdfluent_lopdf::{Document, LoadOptions, Object, Stream, dictionary};

/// Upstream c755394 -- "fix(parser): limit array and dictionary nesting depth".
///
/// Before it, `_direct_object` recursed once per nesting level with nothing to
/// stop it. Our fork bounded literal-string brackets (`MAX_BRACKET`) and nothing
/// else, so a crafted array or dictionary recursed as deep as the file asked.
/// That is the shape of RUSTSEC-2026-0187: a stack overflow aborts the process,
/// which no `Result` can catch and no caller can survive.
///
/// Both cases run on a thread with an explicit 16 MiB stack so that a failure
/// here means "the parser recursed without a bound" rather than "this thread was
/// small". `nesting_bound_survives_a_default_worker_stack` below covers the
/// small-stack case separately, because that is a different property.
///
/// MUTATION: replace `crate::reader::MAX_NESTING_DEPTH` with `usize::MAX` at the
/// `_direct_object` / `array` / `_dictionary` / `operand` entry points.
#[test]
fn deeply_nested_array_does_not_exhaust_the_stack() {
    on_a_thread_with_a_known_stack(16 << 20, || {
        let depth = 50_000;
        let mut body = Vec::new();
        body.extend(std::iter::repeat_n(b'[', depth));
        body.extend(std::iter::repeat_n(b']', depth));
        // Reaching the next line at all is the property under test: without the
        // bound the process aborts here and no assertion runs.
        let _ = Document::load_mem(&single_object_pdf(&body));
    });
}

/// Same bound, reached through a dictionary rather than an array. Worth its own
/// test: the dictionary chain runs through more combinator layers per level than
/// the array chain, so it is the one that costs stack.
#[test]
fn deeply_nested_dictionary_does_not_exhaust_the_stack() {
    on_a_thread_with_a_known_stack(16 << 20, || {
        let depth = 50_000;
        let mut body = Vec::new();
        for _ in 0..depth {
            body.extend_from_slice(b"<</K ");
        }
        body.extend_from_slice(b"0");
        for _ in 0..depth {
            body.extend_from_slice(b">>");
        }
        let _ = Document::load_mem(&single_object_pdf(&body));
    });
}

/// The bound has to survive the smallest stack this parser actually runs on.
///
/// `Reader::read` fans object parsing out over rayon workers, so the budget is a
/// spawned thread's default 2 MiB, not the main thread's 8 MiB -- and a stack
/// overflow on a worker aborts the process rather than returning an error.
///
/// Measured while merging, debug profile, 50 000-deep dictionary through
/// `Document::load_mem` on default stacks: limit 100 overflows, 80 survives.
/// `MAX_NESTING_DEPTH` is therefore 32 here rather than upstream's 100.
///
/// MUTATION: raise `MAX_NESTING_DEPTH` back to upstream's 100.
#[test]
fn nesting_bound_survives_a_default_worker_stack() {
    on_a_thread_with_a_known_stack(2 << 20, || {
        let depth = 50_000;
        let mut body = Vec::new();
        for _ in 0..depth {
            body.extend_from_slice(b"<</K ");
        }
        body.extend_from_slice(b"0");
        for _ in 0..depth {
            body.extend_from_slice(b">>");
        }
        let _ = Document::load_mem(&single_object_pdf(&body));
    });
}

fn on_a_thread_with_a_known_stack(bytes: usize, f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(bytes)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("the parser must return rather than abort the process");
}

/// Upstream afb9f00 -- "fix(document): insert newline between concatenated
/// content streams".
///
/// A page may carry several content streams that are logically one stream. Glued
/// together without a separator, the last token of one and the first of the next
/// fuse: `... 0 -14 TD` followed by `[(x)] TJ ...` yields the operator `TDTJ`,
/// which no interpreter knows. pdf-manip hit exactly this (#474) and worked
/// around it in content_editor.rs rather than in lopdf.
///
/// MUTATION: delete the `content.push(b'\n');` lines in
/// `Document::get_page_content`.
#[test]
fn page_content_streams_are_separated() {
    let mut doc = Document::with_version("1.5");

    // The first stream ends with an operator and the second begins with one, so
    // without a separator the two fuse into the single token `qQ`.
    let first = doc.add_object(Object::Stream(Stream::new(dictionary! {}, b"q".to_vec())));
    let second = doc.add_object(Object::Stream(Stream::new(dictionary! {}, b"Q".to_vec())));

    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => vec![first.into(), second.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1,
            "Kids" => vec![page_id.into()],
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);

    let content = doc.get_page_content(page_id);
    let raw = String::from_utf8_lossy(&content).into_owned();
    let decoded = Content::decode(&content).expect("page content must decode");
    let operators: Vec<&str> = decoded.operations.iter().map(|op| op.operator.as_str()).collect();

    assert_eq!(
        operators,
        vec!["q", "Q"],
        "content streams were concatenated without a separator, so the last operator \
         of one stream and the first of the next fused into a single token; raw bytes: {raw:?}"
    );
}

/// LOPDF-ZBOMB-01, and the divergence that keeps it.
///
/// Upstream v0.44.0 threads a `limit: Option<usize>` through the decoders and
/// defaults it to `None` -- no bound at all. This fork defaults
/// `LoadOptions::max_decompressed_size` to 256 MiB instead, because taking
/// upstream's default verbatim would have quietly removed a guarantee this
/// crate already shipped.
///
/// MUTATION: change `max_decompressed_size` in `LoadOptions::default` from
/// `Some(MAX_DECOMPRESSED_BYTES)` to `None`.
#[test]
fn default_load_options_bound_decompression() {
    assert!(
        LoadOptions::default().max_decompressed_size.is_some(),
        "the default LoadOptions must bound stream decompression; upstream's default \
         is None, and adopting it silently revokes LOPDF-ZBOMB-01 for every caller"
    );
}

/// A stream that inflates past an explicit bound is refused rather than
/// allocated. Exercises the limit plumbing taken from upstream, through the
/// public `decompressed_content_with_limit`.
///
/// MUTATION: make `Stream::decode_filters` ignore its `limit` argument.
#[test]
fn oversized_stream_is_refused_not_allocated() {
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    // 8 MiB of zeros compresses to a few KiB: a small file, a large expansion.
    let plaintext = vec![0u8; 8 * 1024 * 1024];
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&plaintext).unwrap();
    let compressed = encoder.finish().unwrap();
    assert!(compressed.len() < 64 * 1024, "fixture should be small when compressed");

    let stream = Stream::new(dictionary! { "Filter" => "FlateDecode" }, compressed);

    assert!(
        stream.decompressed_content_with_limit(64 * 1024).is_err(),
        "a stream inflating to 8 MiB must be refused under a 64 KiB limit"
    );
    assert_eq!(
        stream.decompressed_content_with_limit(16 * 1024 * 1024).unwrap().len(),
        plaintext.len(),
        "the same stream must decode fine under a limit it fits in"
    );
}

/// Build a one-page PDF whose page /Contents is replaced by `body`, so the
/// parser has to read `body` as a direct object.
fn single_object_pdf(body: &[u8]) -> Vec<u8> {
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = Vec::new();
    let objects: Vec<Vec<u8>> = vec![
        b"<</Type/Catalog/Pages 2 0 R>>".to_vec(),
        b"<</Type/Pages/Count 1/Kids[3 0 R]>>".to_vec(),
        b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/Contents 4 0 R>>".to_vec(),
        body.to_vec(),
    ];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj", i + 1).as_bytes());
        pdf.extend_from_slice(obj);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_pos = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for off in &offsets {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<</Size 5/Root 1 0 R>>\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes());
    pdf
}
