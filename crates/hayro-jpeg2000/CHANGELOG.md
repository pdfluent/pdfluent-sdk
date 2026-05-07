# Changelog — pdfluent-jpeg2000

All notable changes are documented here.

## [0.3.3] — 2026-05-07

### Security

- **J2K-BUF-01**: `Image::decode()` now uses checked multiplication for buffer-size arithmetic. Images with extreme dimensions that overflow `usize` return `DecodeError::Validation(ValidationError::ImageTooLarge)` instead of allocating an incorrectly sized buffer.

## [0.3.2] — 2026-05-02

Initial beta release as part of the PDFluent SDK 1.0.0-beta.1.
