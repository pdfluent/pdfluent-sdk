# Fase D Decision Log — Hayro Integration Bridge

## Overview
Fase D implements the bridge between the XFA engine crates and the hayro PDF stack
(pdf-syntax, pdf-font, pdf-interpret). All code lives in `crates/pdf-xfa/`.

## Decisions Made

### D.1: XFA Packet Extraction — pdf-syntax instead of lopdf
- **Decision:** Use `pdf-syntax` (hayro fork) for XFA packet extraction, replacing the lopdf-based extraction in `pdfium-ffi-bridge`.
- **Rationale:** pdf-syntax is the hayro stack's native PDF parser. Using it directly avoids the lopdf dependency for new code paths.
- **Implementation:** `extract.rs` navigates Catalog → AcroForm → XFA, supporting both single Stream and Name/Stream Array formats (XFA spec §2.1). Includes fallback scan for `<xdp:xdp>` markers across all PDF objects.
- **Note:** The existing lopdf-based extraction in `pdfium-ffi-bridge` remains for backward compatibility; migration is a separate concern.

### D.2: Render Bridge — PDF Operator Generation (not Device trait)
- **Decision:** Generate raw PDF content stream operators directly instead of using the `pdf-interpret::Device` trait.
- **Rationale:** The `Device` trait's `Color::new()` constructor is `pub(crate)` in pdf-interpret, making it impossible to construct `Color` values from outside the crate. Additionally, raw PDF operators (q/Q, re, f, S, BT/ET, Tf, Td, Tj) are more appropriate for overlay composition on existing PDF pages.
- **Trade-off:** Slightly less type-safe than Device calls, but more practical and avoids API visibility issues.

### D.2: Coordinate Mapping — Top-left to Bottom-left
- **Decision:** Implement `CoordinateMapper` to convert XFA coordinates (top-left origin, y grows down) to PDF coordinates (bottom-left origin, y grows up).
- **Formula:** `pdf_y = page_height - xfa_y - element_height`
- **Rationale:** XFA 3.3 §4 uses top-left origin; PDF spec uses bottom-left. This mapping is applied at the render bridge layer.

### D.3: Appearance Stream Architecture — Self-contained PDF operators
- **Decision:** Appearance streams are generated as self-contained `Vec<u8>` byte buffers with PDF operators, independent of the render bridge.
- **Rationale:** Appearance streams have their own coordinate space (relative to the field's bounding box), while the render bridge works in page coordinates. Keeping them separate allows caching appearance streams independently of page position.
- **Caching:** `AppearanceCache` uses `(FormNodeId, value_hash)` keys with per-field invalidation.

### D.4: Font Resolution — Fallback Chain
- **Decision:** Font resolution follows a priority chain: embedded PDF fonts → system fonts (by filename) → base name matching (strip Bold/Italic suffixes) → common fallback fonts.
- **Fallback list:** Helvetica, Arial, DejaVuSans, LiberationSans
- **Rationale:** XFA forms often reference fonts by name without embedding them. The fallback chain provides best-effort resolution. System font scanning is platform-aware (macOS/Linux/Windows).
- **Note:** `pdf-font` crate is declared as a dependency for future CFF/Type1 integration but font metrics currently use `ttf-parser` directly for TTF/OTF fonts.

### General: Unused Dependencies
- **pdf-font** and **pdf-interpret** are declared as dependencies but not directly imported in the current implementation. They are included for:
  1. Future integration (CFF font parsing, Device-based rendering)
  2. Ensuring they remain in the workspace dependency graph
- **roxmltree** is declared for future XFA template XML parsing within the bridge layer.

## Files Created
| File | Issue | Purpose |
|------|-------|---------|
| `error.rs` | — | Shared error types for all bridge modules |
| `extract.rs` | D.1 (#162) | XFA packet extraction via pdf-syntax |
| `render_bridge.rs` | D.2 (#163) | LayoutDom → PDF content stream overlays |
| `appearance_bridge.rs` | D.3 (#164) | FormCalc → appearance streams with caching |
| `font_bridge.rs` | D.4 (#165) | XFA font → system/embedded font resolution |

## Test Coverage
- 17 unit tests across all modules
- All pass on macOS (stable toolchain)
- `cargo clippy -- -D warnings` clean
