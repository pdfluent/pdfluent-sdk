# Changelog — pdfluent-lopdf

All notable changes are documented here.

## [Unreleased]

### Changed

- Merged upstream `lopdf` v0.39.0 → **v0.44.0** (104 commits). `Document::get_page_content`
  now returns `Vec<u8>` instead of `Result<Vec<u8>>`; `LoadOptions` gained upstream's
  `password`, `filter`, `strict` and `max_decompressed_size` alongside this fork's
  `max_file_bytes` and `lazy_objstm`; `Document::load_with_options` /
  `load_mem_with_options` take `LoadOptions` by value. `From<time::Time> for Object` is
  replaced by `From<time::PrimitiveDateTime>` (upstream 1efa270 — the old impl never
  compiled). MSRV rises to 1.88.

### Security

- **LOPDF-NEST-01**: array and dictionary nesting is now bounded (`MAX_NESTING_DEPTH`).
  Before this, a crafted deeply-nested object recursed until the stack ran out, which
  aborts the process rather than returning an error. The bound is **32**, not upstream's
  100: `Reader` parses objects on rayon workers whose default stack is 2 MiB, and at
  depth 100 an unoptimised build overflows it. Measured: 100 overflows, 80 survives.
- Stream decompression takes a per-call limit throughout, defaulting to 256 MiB.
  Upstream's default is `None`; ours keeps LOPDF-ZBOMB-01 on for callers who do not
  opt in.

### Fixed

- Page content streams are separated by a newline when concatenated, so the last
  operator of one stream no longer fuses with the first of the next.
- Objects inside an ObjStm that the cross-reference table does not list are kept
  rather than dropped (upstream 3bc6a52).
- FlateDecode streams with a corrupt adler32 checksum fall back to raw deflate.
- Text extraction handles the `'`, `"` and `T*` operators.

## [0.39.2] — 2026-05-07

### Security

- **LOPDF-ZBOMB-01**: FlateDecode and LZWDecode decompression is now capped at 256 MiB per stream (`MAX_DECOMPRESSED_BYTES`). Crafted zip-bomb PDFs that previously caused unbounded memory growth now return `Error::StreamTooLarge { limit }` once the cap is exceeded.

## [0.39.0] — 2026-05-02

Initial beta release as part of the PDFluent SDK 1.0.0-beta.1.
