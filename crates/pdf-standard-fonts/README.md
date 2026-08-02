# pdf-standard-fonts

The PDF Standard 14 font programs, compiled into the binary.

## Why

A PDF may reference Helvetica, Times or Courier without embedding them, because
every viewer is required to have them. A *converter* has no such luxury: PDF/A
forbids non-embedded fonts, so something must be embedded, and whatever gets
embedded determines the glyph widths the output declares.

Resolving that from the host filesystem makes the output depend on what happens
to be installed. Measured on 300 real-world documents, the same code scored
286/300 on macOS and 275/300 on Debian, purely because the substitute font
differed. This crate removes that variable.

## Why these particular fonts

Their advance widths match the Adobe AFM metrics for the Standard 14 exactly,
which is what ISO 19005-2 §6.2.11.5 compares a font dictionary against.

A merely similar font is not good enough. Liberation Sans is metric-compatible
with Arial, not with Helvetica; substituting it produces widths that disagree
with what the source document declared, and the document then fails validation
through no fault of its own.

## Format

Bare CFF, 14–29 KB per face, 259 KB for the set. Embed as `/FontFile3` with
`/Subtype /Type1C`. There is no OpenType wrapper, so `/Subtype /OpenType` would
be incorrect.

The files carry a `.cff` extension. The same bytes live in
`crates/pdf-interpret/assets/` under a `.pfb` extension, which is a misnomer
inherited from upstream: the contents are CFF, not Type 1.

## Scope

`StandardFont::from_base_font` resolves the canonical names, a leading
`ABCDEF+` subset prefix, and the three metric-compatible families producers
actually emit (Arial, Times New Roman, Courier New).

It deliberately returns `None` for anything else. Palatino, Bookman, Century
Schoolbook and the Narrow variants have different metrics; substituting a
Standard 14 face for them would reflow the page, which is worse than the
validation failure it would paper over.

## Licensing

The font programs come from PDFium, © 2014 Foxit Software Inc., under the
BSD-3-Clause licence reproduced in `LICENSE-FOXIT`. That licence requires the
copyright notice to accompany binary redistributions, so it ships inside the
crate. The Rust code around them is under the licence in `LICENSE`.
