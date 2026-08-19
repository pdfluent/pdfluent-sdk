# pdf-ocr

OCR integration for PDFluent — bring your own recognizer, we do the PDF.

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Turns a scanned PDF into a searchable one. You provide the character
recognition; this crate renders the pages, hands them to your recognizer, and
writes the words back as an invisible text layer positioned over the image.

The recognizer is a trait:

```rust
pub trait OcrEngine: Send + Sync {
    fn recognize(&self, image_data: &[u8], width: u32, height: u32, dpi: u32)
        -> Result<OcrPageResult, String>;
    fn supported_languages(&self) -> Vec<String>;
}
```

Implement it against whatever you already use — Google Cloud Vision, AWS
Textract, Azure Document Intelligence, a self-hosted service, a local binary —
and pass it to `make_searchable` together with a page-render callback. You get
back an `OcrReport` per page.

Two local backends ship behind their own feature flags, and they are not
equivalent:

**`paddle`** — PaddleOCR via ONNX. Detection with DBNet, recognition with SVTR,
optional angle classification. Handles English, Latin scripts, Chinese, Japanese,
Korean and Arabic. It uses `ort` with `load-dynamic`, so ONNX Runtime is loaded as
a shared library at run time rather than compiled in, and the model weights are
downloaded once (detection is ~2.3 MB for V3, ~84 MB for V5) and cached in the OS
cache directory. After that first fetch it runs entirely offline, in-process, with
no cloud calls.

**`tesseract`** — Tesseract 4 or 5 through `leptess`. Requires the Tesseract and
Leptonica system libraries to be installed.

Neither is on by default, and the `pdfluent` facade wires neither, so a plain
`pdfluent` dependency pulls in no system libraries and downloads no models.
Opting in is a decision you make rather than a side effect of using the SDK.

## Status

The seam and the text-layer work are stable in shape. Recognition quality is
entirely your provider's.


## Usage

Depend on this crate directly. The `ocr-tesseract` and `ocr-paddle` flags on the
`pdfluent` facade are reserved names that enable nothing — the facade does not
wire OCR at all, precisely because the local engines need C libraries.

```rust
use pdf_ocr::{make_searchable, OcrConfig, OcrEngine, OcrPageResult, OcrWord};

struct MyCloudOcr { /* http client, credentials */ }

impl OcrEngine for MyCloudOcr {
    fn recognize(&self, rgb: &[u8], width: u32, height: u32, dpi: u32)
        -> Result<OcrPageResult, String>
    {
        // POST the image to your provider, then map each word onto
        //   OcrWord { text, bbox_px: [x0, y0, x1, y1], confidence }
        // with bbox_px in pixels of the image you were handed, and return
        //   OcrPageResult { words, confidence, image_width, image_height }
        // so the text layer can be scaled back onto the page.
        todo!()
    }
    fn supported_languages(&self) -> Vec<String> { vec!["eng".into()] }
}
```

Then call `make_searchable(&mut doc, &engine, &config, render_fn)`.

For PaddleOCR or Tesseract instead of a cloud service, enable the matching feature
and use the engine it provides — the rest of the call is identical, because they
implement the same trait:

```toml
pdf-ocr = { version = "1", features = ["paddle"] }     # or ["tesseract"]
```

```rust
// Weights you already have on disk. Nothing is fetched.
let config = pdf_ocr::paddle::PaddleOcrConfig {
    model_dir: "/opt/ocr-models".into(),
    ..Default::default()
};
let engine = pdf_ocr::PaddleOcrEngine::with_config(config)?;

// or Tesseract, which uses the system installation:
let engine = pdf_ocr::TesseractEngine::new("eng", None)?;
```

### Model weights are never fetched behind your back

Constructing a PaddleOCR engine does not download anything. If the weights are not
in `model_dir` you get an error naming the missing files and the directory they
belong in — not a silent ~84 MB transfer from a host you did not choose.

If you do want this crate to fetch them, say so and pin what you expect:

```rust
use std::collections::BTreeMap;
use pdf_ocr::paddle::models::ModelSource;

let mut digests = BTreeMap::new();
digests.insert("detection/v3/det.onnx".into(), "…64 hex chars…".into());
digests.insert("latin/rec.onnx".into(),        "…".into());
digests.insert("latin/dict.txt".into(),        "…".into());

config.model_source = ModelSource::Verified {
    base_url: pdf_ocr::paddle::models::REFERENCE_MODEL_BASE_URL.into(),
    digests,
};
```

Each file is held in memory, hashed, and only written if it matches. A file with no
pinned digest is refused rather than fetched — something unverifiable should not
land on your disk at all. `REFERENCE_MODEL_BASE_URL` points at the public
third-party repository the ONNX exports come from; it is offered for convenience,
not as an endorsement, which is exactly why pinning is your call and not our
default.

For the full signature, see <https://pdfluent.com/docs>.

## Licensing

- Free for evaluation, development, and testing
- Production use requires a valid PDFluent commercial license
- Redistribution requires the OEM Redistribution add-on

See [LICENSE](LICENSE) for full terms, or visit <https://pdfluent.com/terms>.

## Links

- Main crate: <https://crates.io/crates/pdfluent>
- Documentation: <https://pdfluent.com/docs>
- Trial: <https://pdfluent.com/trial>
- Pricing: <https://pdfluent.com/pricing>
