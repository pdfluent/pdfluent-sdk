# Fase M — PDF Manipulation (`pdf-manip`) — Autonomous Decisions

## Crate Design

- **Single crate, 6 modules**: `pages`, `encrypt`, `watermark`, `optimize`, `bookmarks`, `error`
- Chose `lopdf` as sole PDF backend (consistent with rest of workspace)
- Added `flate2` for FlateDecode stream compression (optimize module)
- `serde` feature flag for bookmark JSON import/export (optional, default on)
- `watermark-image` feature flag reserved for future image watermark support

## M1 — Page Manipulation (`pages.rs`)

- All page indices are **1-based** to match PDF conventions
- `extract_pages` clones the document and deletes unwanted pages (simpler than selective copying)
- `insert_pages` copies all objects from source with ID remapping to avoid collisions
- `merge` / `merge_documents` delegate to `insert_pages` for consistency

## M2 — Encryption (`encrypt.rs`)

- `encrypt_and_save` writes encryption metadata (dictionary) but relies on lopdf's save pipeline for actual byte-level encryption
- Placeholder hashes (32/48 bytes of zeros) are used for O/U/OE/UE entries — lopdf computes real values during save
- Password parameters use `&str` (not `&[u8]`) to match lopdf 0.39 API
- Permissions follow ISO 32000-2 Table 22 bit layout

## M3 — Watermarking (`watermark.rs`)

- Text watermarks only in initial implementation; image watermarks reserved behind `watermark-image` feature
- Uses ExtGState (`/ca` + `/CA`) for opacity control
- Watermark rendered as PDF content stream operators (BT/ET text objects with Tm matrix for rotation)
- Tiling generates a grid of positions across the page based on configurable spacing
- Layer control: Background = prepend content stream, Foreground = append
- Helvetica used as default watermark font (Type1, universally available in PDF readers)

## M4 — Optimization (`optimize.rs`)

- Stream compression: only compresses streams that don't already have a `/Filter` and where compressed output is actually smaller
- Deduplication: keyed on raw stream content bytes (simple but effective)
- Unused object removal: transitive reference walk from trailer
- Metadata stripping: removes `/Info` from trailer and `/Metadata` from catalog

## M5 — Bookmarks (`bookmarks.rs`)

- Outline tree uses PDF linked-list structure (/First, /Last, /Next, /Prev, /Parent)
- `write_bookmark_siblings` returns `(first_id, last_id)` tuple for proper tree construction
- JSON roundtrip preserves title, page, URI, open state, bold/italic style
- Circular reference protection in `read_siblings` via visited set
- Supports GoTo, GoToR, URI, and Named action types
