# Changelog — pdfluent-jpeg2000

All notable changes are documented here.

## [0.4.0] — 2026-08-31

Re-based on upstream `hayro-jpeg2000` 0.4.0 (LaurenzV/hayro, commit `5a5f0e24`).
Our fork had been sitting on upstream `4d923bd4` (0.3.3) with two later commits
cherry-picked on top, so this closes 38 upstream commits at once.

### Changed — breaking

- `Image::decode()` now takes a caller-owned `DecoderContext` and returns a
  `DecodedImage` instead of a `Vec<u8>`. The scratch buffers live in the context
  so they can be reused across images. Call `.data_u8()` for the old return
  value, or `.store_u8_into(&mut buf)` to decode into a buffer you own.
- `ComponentData`, `DecoderContext` and `DecodedImage` are now public, so the
  decoded `f32` samples can be inspected without going through the 8-bit packing.
- `Image::decode_into()` is gone; use `DecodedImage::store_u8_into()`.

### Fixed

- **Tile-count overflow.** `SizeData::num_tiles()` multiplied the two SIZ tile
  counts unchecked. A grid claiming `u32::MAX` by `u32::MAX` one-pixel tiles
  wrapped that product, and the wrapped value is also what per-tile indices are
  later validated against — so the failure mode was a wrong answer, not a crash.
  Now rejected as `ValidationError::ImageTooLarge`. Upstream `a7e3aace` (#1355).
- **Division by zero on a zero target resolution.** A `DecodeSettings` with
  `target_resolution: Some((0, 0))` reached `image_width() / target_width`.
  Upstream `ff7e2e40` (#1237). Our fork carried an equivalent guard written a
  different way; upstream's `checked_div` replaces it.
- **LAB colour conversion read the wrong range.** The b\* range was taken from
  `lab.ra` instead of `lab.rb`, so every JP2 with an explicit `Rb` decoded its
  b\* channel against the a\* range. Upstream `c2df2014` (#1313).

### Changed

- The hardcoded 60000-pixel dimension cap is gone, replaced by checked
  allocation arithmetic (`checked_add` / `usize::try_from` on tile and sub-band
  areas). Images upstream can decode are no longer refused on dimensions alone;
  allocation overflow is still an error, not a wrap. Upstream `a7e3aace` (#1355).
- Irreversible (9/7) coefficients now get a reconstruction mid-point applied,
  both when truncated and when fully decoded. This changes decoded pixel values
  for every irreversible image. Upstream `9cce046b` (#1284) and `49037586` (#1340).
- The coefficient buffer is sized by the resolution actually requested, so a
  `target_resolution` below the full image no longer allocates for the levels it
  skips. Upstream `5a5f0e24` (#1352).
- `fearless_simd` 0.3 → 0.6, following upstream.

### Kept

- YCCK enumerated colour space handling (`EnumeratedColorspace::Ycck` → CMYK).
  Upstream parses the tag but has no conversion for it.

## [0.3.3] — 2026-05-07

### Security

- **J2K-BUF-01**: `Image::decode()` now uses checked multiplication for buffer-size arithmetic. Images with extreme dimensions that overflow `usize` return `DecodeError::Validation(ValidationError::ImageTooLarge)` instead of allocating an incorrectly sized buffer.

## [0.3.2] — 2026-05-02

Initial beta release as part of the PDFluent SDK 1.0.0-beta.1.
