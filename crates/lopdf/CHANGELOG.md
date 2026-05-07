# Changelog — pdfluent-lopdf

All notable changes are documented here.

## [0.39.2] — 2026-05-07

### Security

- **LOPDF-ZBOMB-01**: FlateDecode and LZWDecode decompression is now capped at 256 MiB per stream (`MAX_DECOMPRESSED_BYTES`). Crafted zip-bomb PDFs that previously caused unbounded memory growth now return `Error::StreamTooLarge { limit }` once the cap is exceeded.

## [0.39.0] — 2026-05-02

Initial beta release as part of the PDFluent SDK 1.0.0-beta.1.
