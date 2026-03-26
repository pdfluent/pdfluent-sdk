# pdf-engine

Unified PDF rendering engine for rendering, text extraction, thumbnails, XFA handling, and OCR integration.

## Features

| Build | Cargo feature(s) | Includes |
|---|---|---|
| Base | none | Parse, render, text extraction, thumbnails, bookmarks, AcroForm text extraction |
| XFA | `xfa` | `pdf_xfa` bridge, `PdfDocument::flatten_xfa()`, `pdf_engine::xfa` helpers |
| OCR (pure Rust) | `ocr` | `OcrsBackend`, pure-Rust OCR, WASM-compatible |
| OCR (native ONNX) | `ocr-onnx` | `PaddleOnnxBackend` surface and native ONNX Runtime wiring |
| OCR (Mistral) | `ocr-mistral` | Hosted Mistral OCR adapter |
| OCR (Google Vision) | `ocr-google` | Hosted Google Vision adapter |
| OCR (AWS Textract) | `ocr-aws` | Hosted AWS Textract adapter |
| OCR (Azure Doc Intel) | `ocr-azure` | Hosted Azure Document Intelligence adapter |
| OCR (All cloud) | `ocr-cloud` | All hosted OCR adapters |
| Full | `full` | `xfa`, `ocr`, `ocr-onnx`, and `ocr-cloud` together |

## Build matrix

```bash
CARGO_TARGET_DIR=/tmp/codex-features-target cargo build -p pdf-engine
CARGO_TARGET_DIR=/tmp/codex-features-target cargo build -p pdf-engine --features xfa
CARGO_TARGET_DIR=/tmp/codex-features-target cargo build -p pdf-engine --features ocr
CARGO_TARGET_DIR=/tmp/codex-features-target cargo build -p pdf-engine --features full
CARGO_TARGET_DIR=/tmp/codex-features-target cargo build -p pdf-engine --target wasm32-unknown-unknown
CARGO_TARGET_DIR=/tmp/codex-features-target cargo build -p pdf-engine --target wasm32-unknown-unknown --features ocr
```

## Measured release sizes

Measured on 2026-03-25 with release builds in `/tmp/codex-features-target`.

| Variant | Native `libpdf_engine.rlib` | WASM `pdf_engine.wasm` |
|---|---:|---:|
| Base | 600K | 36K |
| `xfa` | 608K | N/A |
| `ocr` | 639K | 2.5M |
| `ocr-onnx` | 650K | N/A |
| `full` | 712K | N/A |

Notes:

- The WASM sizes above come from the generated `cdylib` artifact.
- `ocr-onnx` is native-only and is not built for `wasm32-unknown-unknown`.
- PaddleOCR model files are not bundled into the crate.

## OCR Cloud Providers

| Provider | Feature | Env Vars | Auth |
|----------|---------|----------|------|
| Mistral | `ocr-mistral` | `MISTRAL_API_KEY` | API key |
| Google Vision | `ocr-google` | `GOOGLE_VISION_API_KEY` or `GOOGLE_APPLICATION_CREDENTIALS` | API key or service account |
| AWS Textract | `ocr-aws` | `AWS_REGION` + `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` | IAM credentials |
| Azure Doc Intel | `ocr-azure` | `AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT` + `AZURE_DOCUMENT_INTELLIGENCE_KEY` | API key |

Enable all hosted OCR adapters together with:

```bash
CARGO_TARGET_DIR=/tmp/codex-features-target cargo test -p pdf-engine --features ocr-cloud
```

Notes:

- `best_available_backend()` prefers cloud providers before local OCR fallbacks.
- Google auto-detection checks `GOOGLE_VISION_API_KEY` first, then `GOOGLE_APPLICATION_CREDENTIALS`, then gcloud application-default credentials when `GOOGLE_CLOUD_PROJECT` is set.
- Azure uses API version `2024-11-30` by default. Override with `AZURE_DOCUMENT_INTELLIGENCE_API_VERSION` if your deployment requires a different version.

## OCR model discovery

`ocr-onnx` checks these inputs in order:

1. `PADDLE_DET_MODEL`
2. `PADDLE_REC_MODEL`
3. `PADDLE_DICT`
4. Fallback cache dir: `~/.cache/paddle-ocr/`

Expected default filenames:

- `ch_PP-OCRv4_det.onnx`
- `ch_PP-OCRv4_rec.onnx`
- `ppocr_keys_v1.txt`

## Public API by feature

- Base build: `PdfDocument`, rendering, text extraction, thumbnails, bookmarks, OCR traits.
- `xfa`: enables `pdf_engine::xfa` plus `PdfDocument::flatten_xfa()`.
- `ocr`: enables `OcrsBackend` and `ocr_page_default()`.
- `ocr-onnx`: enables `PaddleOnnxBackend` on native targets.
- `ocr-mistral`: enables `MistralOcrBackend`.
- `ocr-google`: enables `GoogleVisionBackend`.
- `ocr-aws`: enables `AwsTextractBackend`.
- `ocr-azure`: enables `AzureDocIntelBackend`.

## Runtime notes

- `ocr` is the WASM-safe option.
- `ocr-onnx` is configured with ONNX Runtime dynamic loading so the crate can
  compile across the current native targets without bundling platform-specific
  ORT binaries in the crate itself.
