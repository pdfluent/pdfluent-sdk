# .notdef Glyph Fix Log (6.2.11.8:1)

ISO 19005-2, section 6.2.11.8:
"A conforming file shall not contain a reference to the .notdef glyph from any
of the text showing operators, regardless of text rendering mode."

## What works

### Simple fonts (Type1/TrueType) — non-subset
- Encoding Differences approach: add Differences entries for codes that map to .notdef
- Map to correct glyph name if font has it, or "space" as fallback
- Phase 1: fix existing ".notdef" entries in Differences
- Phase 2: fix codes not in Differences that map to .notdef via base encoding

### Simple fonts — subset (ABCDEF+FontName)
- Same approach but MORE aggressive: also allow "space" for codes >= 32
- Safe because subset fonts already had .notdef for these codes

### Symbolic simple fonts with standard encoding
- Don't skip symbolic fonts that have WinAnsiEncoding/MacRomanEncoding
- The Differences logic works fine with standard encodings regardless of Symbolic flag

### CID fonts (Type0) — content stream modification ✓ COMPLETE
- All 5 sole-blocker .notdef PDFs used Type0 CID fonts with Identity-H CMap
- Simple font approach can't work: CID fonts use CMap, not Differences
- Solution: `fix_cid_font_notdef` modifies content streams, replacing .notdef-producing
  2-byte CIDs with the space CID
- Handles both CIDFontType0 (CFF) via `cff_parser` and CIDFontType2 (TrueType) via `ttf_parser`
- **Cross-stream font state**: font name (`Tf`) must carry across consecutive content
  streams on the same page (ISO 32000-1 §7.8.2 — graphics state not reset between streams)
- **ASCIIHexDecode fallback**: lopdf may not decode ASCIIHexDecode streams; detect ASCII
  hex content and decode manually (truncate at `>` marker)
- **Empty-glyph fonts**: fonts with only .notdef (e.g. HiddenHorzOCR stub) get all text
  cleared entirely (empty string)

## Bugs found and fixed

1. **Subset font skip too aggressive**: initially skipped all subset fonts (ABCDEF+FontName),
   but all 5 blockers were subset CID fonts. Fixed: removed hard skip, added `is_subset` flag.

2. **Symbolic font skip too aggressive**: HiddenHorzOCR (0298) has Flags=4 (Symbolic) but
   uses WinAnsiEncoding. Fixed: allow symbolic fonts through if they have a standard encoding.

3. **DescendantFonts as Reference**: font dict stores DescendantFonts as Reference (not
   inline Array). Fixed: dereference before accessing array.

4. **Cross-stream font state lost**: op 1434 in 0502 was the first op in stream (174,0),
   but font was set in previous stream (169,0). `current_font_name` was reset per stream.
   Fixed: moved `current_font_name` outside the per-stream loop.

5. **ASCIIHexDecode not handled by lopdf**: 0298's HiddenHorzOCR CFF was stored with
   ASCIIHexDecode filter (565 ASCII hex bytes → 270 binary CFF bytes). lopdf's decompress()
   returned error. Fixed: detect hex content and decode manually.

6. **CIDFontType2 (TrueType CID) not handled**: 0502's FuturaBT-Light/LightItalic are
   CIDFontType2 with FontFile2. CFF parser fails on TrueType data. Fixed: check CIDFont
   subtype and use `ttf_parser::Face` for TrueType.

## Results

All 5 sole-blocker PDFs now COMPLIANT:
| PDF  | Font | Type | Fix |
|------|------|------|-----|
| 0259 | PYSIDE+Frutiger-Bold, JXOYXQ+Frutiger-Light | CIDFontType0 CFF | CID→space |
| 0273 | XPKBBM+Typewriter-Bold | CIDFontType0 CFF | CID→space |
| 0287 | CIBIZM+Futura-Bold, BARCLO+Garamond-Book | CIDFontType0 CFF | CID→space |
| 0298 | HiddenHorzOCR | CIDFontType0 CFF (stub, 1 glyph) | text cleared |
| 0502 | HZGCPF+Times-Roman (cross-stream) | CIDFontType0 CFF | CID→space |
| 0502 | XDWFFZ+FuturaBT-Light, JLWURB+FuturaBT-LightItalic | CIDFontType2 TT | now parsed |
