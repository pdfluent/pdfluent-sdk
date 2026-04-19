# XFA Feature Support Matrix

Status as of XFA Engine Milestone #47 (Phase 8/9).

| Feature | Status | Notes |
|---------|--------|-------|
| consumeData binding | Full | Default merge mode |
| matchTemplate binding | Full | Added in F3-01 |
| Transparent subforms | Full | Positional pass-through |
| Occur expansion | Full | occur min/max/initial + data-driven |
| anchorType | Full | All 9 variants (topLeft … bottomRight) |
| hAlign in TB parent | Full | Center, Right, Left |
| Text line splitting | Full | Split at line boundary across pages |
| Overflow (basic) | Full | Content flowing to next page |
| Overflow leader/trailer | Partial | Per-page leader/trailer; overflow/bookend leaders not yet |
| keep chains | Full | keep.next / keep.previous look-ahead |
| area | Full | Positioned and TB layout |
| exclGroup | Full | Exclusive radio-button groups |
| subformSet | Full | Transparent container |
| Table layout | Full | columnWidths, colSpan, row equalization |
| LR-TB / RL-TB layout | Full | Left-to-right-TB and right-to-left-TB |
| anchorType (Appendix A) | Full | All 9 coordinate algorithm variants |
| pageArea / contentArea | Full | Multi-page with fixed nodes |
| Static (XFAF) forms | Full | baseProfile=interactiveForms |
| Dynamic forms | Full | Full XFA grammar |
| Widget AP baking | Full | Checkbox/radio marks preserved |
| Font resolution | Full | Embedded PDF fonts + system fallbacks |
| CID fonts (/W arrays) | Full | Consecutive CID + range entries |
| Image (href) | Full | Embedded EmbeddedFiles resolution |
| Draw elements | Full | Line, rectangle, arc, text draws |
| FormCalc scripting | Partial | Basic expressions; complex scripts may not execute |
| JavaScript | Not supported | Would require full JS engine; viewer-version checks, runtime host scripts, and service-prefill flows are excluded as unsupported |
| Barcode rendering | Not supported | Rendered as empty placeholder boxes |
| Signature fields | Not supported | Stripped — no signature widget rendering |
| Rich text (xhtml) | Partial | Basic spans; complex HTML not fully rendered |
| Named colors | Full | All CSS named colors + PDF named colors |
| Encrypted PDFs | Partial | Empty-password (owner-only) auto-decrypted |
| Password-protected PDFs | Not supported | Returns error; requires user password |
| XFA 3.3 §8.10 bookend leaders | Not supported | Overflow/bookend leader placement not yet |

## Graceful Degradation

When unsupported features are detected the engine logs a warning via
`log::warn!` and continues processing rather than failing silently. Enable
with `RUST_LOG=pdf_xfa=warn`.

| Unsupported element detected | Behaviour |
|------------------------------|-----------|
| `<barcode>` | Warning logged; element rendered as empty box |
| `<signature>` | Warning logged; element skipped |
| `<script type="text/javascript">` | Warning logged; script not executed |
| Acrobat/XFA viewer checks or connectionSet prefill | Benchmark exclusion as unsupported | Forms that depend on host-version JavaScript or runtime service bindings are not treated as render regressions |
