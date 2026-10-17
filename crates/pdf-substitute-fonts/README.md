# pdf-substitute-fonts

PDF/A requires every font to be embedded. Plenty of documents only *reference*
a typeface — Tahoma, Verdana, Calibri — without carrying it, so a converter has
to supply one.

Taking that substitute from the host means the output depends on what happens
to be installed. Measured on 119 govdocs holdout documents (24-08-2026): 37 of
them borrowed a font from the machine, 129 times. The same corpus scored
286/300 on macOS and 300/300 on Linux for exactly this reason.

This crate carries the substitutes instead, so the answer is the same
everywhere. Liberation is metric-compatible with Helvetica, Times and Courier:
same advance widths, so text does not reflow.

Liberation covers Latin, Greek and Cyrillic and no Arabic, so a document naming
an Arabic typeface it does not embed was given a face without one glyph it
needed. Noto Naskh Arabic is here for that, and it covers every character the
measured case (`2033091`) declares — presentation forms included.

The Standard 14 are not here — those come from `pdf-standard-fonts`, which has
been compiled in for longer and is far smaller because they are bare CFF.

## Licence

The fonts are third-party, and both families are under the SIL Open Font
License 1.1:

| family | version | upstream | licence text here |
|---|---|---|---|
| Liberation | 2.1.5 | [liberationfonts/liberation-fonts](https://github.com/liberationfonts/liberation-fonts) | `LICENSE-Liberation.txt` |
| Noto Naskh Arabic | v2.021 | [notofonts/arabic](https://github.com/notofonts/arabic/releases/tag/NotoNaskhArabic-v2.021) | `LICENSE-NotoNaskhArabic.txt` |

OFL 1.1 §3 permits bundling, embedding, redistributing and selling fonts *with
software*, and forbids selling them by themselves; the OFL text and the
copyright travel with the bytes, which is what those two files are. Neither
family is modified here, so the reserved names (`Liberation`, `Arimo`, `Tinos`,
`Cousine`, `Noto`) raise no derivative-name question. "Noto" is a trademark of
Google LLC and is named here as a fact about the bytes, not as an endorsement.

`PROVENANCE.sha256` records the exact bytes, and the upstream archive they came
from, so a build that picked up a different copy is detectable:
`sha256sum -c ../PROVENANCE.sha256` from `assets/`.

The rest of the crate is AGPLv3-or-commercial like the rest of PDFluent; see
`LICENSE` and `LICENSE-COMMERCIAL`. The commercial licence is sold yearly and
self-service at [pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing).

## Size

Fourteen faces, 4.7 MB. `default-features = false` on `pdf-manip` drops the bytes
and makes every lookup here answer `None`; the conversion then falls back to the
compiled-in Standard 14, which is a different face with different widths, so the
text can reflow.

**The browser does not get here at all, and not by that switch.** `pdf-manip`
declares this crate under `[target.'cfg(not(target_arch = "wasm32"))']`, so it is
absent from a wasm build whatever any feature says. A feature could not have held
it: `xfa-wasm` reaches `pdf-manip` along four paths — directly, through
`pdfluent`, through `pdf-redact` and through `pdf-text-format` — and cargo
unifies features across the graph, so one dependant with defaults on turns it
back on for all of them. The published bundle size (claim B07, 9.6 MB unpacked)
is not something a flag any dependant can undo should be holding.
`scripts/ci/the_browser_bundle_carries_no_fonts.py` is the guard.
