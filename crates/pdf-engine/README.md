# pdf-engine Feature Flags

`pdf-engine` builds a small core by default and gates heavier capabilities
behind explicit Cargo features.

## Features

| Build | Cargo feature(s) | Includes |
|---|---|---|
| Base | none | Parse, render, text extraction, thumbnails, bookmarks, AcroForm text extraction |
| XFA | `xfa` | `pdf_xfa` bridge, `PdfDocument::flatten_xfa()`, `pdf_engine::xfa` helpers |
| OCR (pure Rust) | `ocr` | `OcrsBackend`, pure-Rust OCR, WASM-compatible |
| OCR (native ONNX) | `ocr-onnx` | `PaddleOnnxBackend` surface and native ONNX Runtime wiring |
| Full | `full` | `xfa`, `ocr`, and `ocr-onnx` together |

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

## Runtime notes

- `ocr` is the WASM-safe option.
- `ocr-onnx` is configured with ONNX Runtime dynamic loading so the crate can
  compile across the current native targets without bundling platform-specific
  ORT binaries in the crate itself.
