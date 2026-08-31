# Changelog — pdfluent-jbig2

All notable changes are documented here.

## [Unreleased]

### Security

- A text region may no longer claim more symbol instances than its own segment
  data could encode (`SymbolError::TooManyInstances`). Ported from
  LaurenzV/hayro#1278, which took the bound from Chromium's JBIG2 reader. This
  fork carried no bound at all, so a region declaring `u32::MAX` instances got
  that many decode rounds without ever panicking.
- `decode_bitmap_arithmetic_coding` returns early on a zero width or height
  (LaurenzV/hayro#1262). Behaviour is unchanged today — every loop below it is
  already empty — but the immunity was incidental, resting on `Bitmap::get_word`
  returning zero out of range.

### Not taken from upstream, deliberately

- **`src/simd.rs` and `src/integration.rs`.** Both exist upstream, both are
  behind default-on cargo features, and neither was ever vendored into this
  fork. `simd` pulls in `fearless_simd` and forces `std`; `image` pulls in the
  `image` crate to implement `ImageDecoder`. Our three consumers use exactly two
  items — `decode_embedded` and the `Decoder` trait — so the `image`
  integration has no caller here at all. Both features also force `std`, which
  would cost this crate its `no_std` support, and `fearless_simd` puts `unsafe`
  into the dependency tree of a codec whose whole job is reading untrusted
  input. Two optional dependencies, no caller: a cost without a return.
- **LaurenzV/hayro#1235 and #1236** (skip work for white pixels; fast path for
  default templates). Pure performance, and both are written against upstream's
  March 2026 rewrite of the decode loops — the `Word` buffers,
  `maybe_reload_buffers` and the macro-generated row loops — none of which this
  fork has. Porting them means adopting that rewrite, which changes decoded
  pixels on every image and therefore needs a corpus comparison rather than a
  unit test. The corpus is unavailable (#264).

## [0.2.1] — 2026-05-07

### Security

- **JBIG2-HUF-01**: Over-committed Huffman prefix trees no longer panic. Crafted JBIG2 streams where two length-1 codes fill the binary tree previously triggered an unreachable `panic!` in `HuffmanTable::set_child`. The path now returns `DecodeError::Huffman(HuffmanError::MalformedTable)`.

## [0.2.0] — 2026-05-02

Initial beta release as part of the PDFluent SDK 1.0.0-beta.1.
