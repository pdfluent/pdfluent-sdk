# Changelog — pdfluent-jbig2

All notable changes are documented here.

## [0.2.1] — 2026-05-07

### Security

- **JBIG2-HUF-01**: Over-committed Huffman prefix trees no longer panic. Crafted JBIG2 streams where two length-1 codes fill the binary tree previously triggered an unreachable `panic!` in `HuffmanTable::set_child`. The path now returns `DecodeError::Huffman(HuffmanError::MalformedTable)`.

## [0.2.0] — 2026-05-02

Initial beta release as part of the PDFluent SDK 1.0.0-beta.1.
