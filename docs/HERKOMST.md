# Herkomst per crate

**Gegenereerd** door `scripts/ci/herkomsttabel.py`. Niet met de hand bewerken —
een tabel die je overschrijft is op de dag van publicatie al oud.

Dit beantwoordt één vraag: *wie is de rechthebbende, en waaruit blijkt dat?*
Zonder dat antwoord is herlicentiëren weggeven wat misschien niet van jou is.

## Wat vaststaat

**Eigen code is schoon.** 3.274 commits in deze repo, van één persoon plus een
CI-bot. Geen enkele externe menselijke bijdrager, dus herlicentiëren vereist
niemands toestemming.

De `Co-Authored-By`-trailers waren geen auteursrechtclaim — een assistent is geen
rechthebbende. Ze zijn uit de historie geschreven (#229).

**Correctie, 02-09-2026.** Hier stond dat ze er sinds 26-08-2026 niet meer in
kunnen komen, "`scripts/ci/no_ai_attribution.py` plus een `commit-msg`-haak".
Dat was niet waar toen het werd opgeschreven en het is vier maanden lang niet
waar geweest. Gemeten op 02-09: `core.hooksPath` wees naar `.githooks`, die map
bevatte `pre-commit` en `pre-push` en géén `commit-msg`, en het genoemde script
stond op een tak die nooit gemerged is — geschreven op 31-08, vijf dagen ná de
datum die deze zin claimde.

Sinds 02-09-2026 klopt de zin wel, en hij is nu ook controleerbaar:

- `.githooks/commit-msg` roept beide boodschapwachten aan;
- `scripts/ci/the_commit_msg_hook_is_wired.py` faalt als die haak ontbreekt, als
  hij een van de twee niet aanroept, of als `core.hooksPath` in déze kloon niet
  naar `.githooks` wijst — die laatste laag is per kloon en is precies de laag
  die niemand controleerde;
- de wacht draait in de lokale poort én in CI.

Wie deze alinea in de toekomst wil aanpassen: laat die wacht meeveranderen, of
laat de alinea weg. Een afspraak die alleen in een document staat is geen
afspraak, en dit document heeft er vier maanden een beschreven die er niet was.

**De Foxit-fonts zijn in orde.** `crates/pdf-interpret/assets/*.pfb` draagt
BSD-3-Clause van de PDFium Authors, volledig herdistribueerbaar. Dat is het
klassieke struikelpunt bij open source; hier is het al geregeld.

## Eigen crates

39 crates, 389 `.rs`-bestanden, waarvan **5**
de proprietary header dragen.

| crate | licentie | .rs | met header |
|---|---|---:|---:|
| `formcalc-interpreter` | AGPL-3.0-or-later | 9 | 0 |
| `pdf-annot` | AGPL-3.0-or-later | 16 | 0 |
| `pdf-bench` | — | 7 | 0 |
| `pdf-capi` | AGPL-3.0-or-later | 5 | 0 |
| `pdf-compliance` | AGPL-3.0-or-later | 11 | 0 |
| `pdf-content-stream` | AGPL-3.0-or-later | 12 | 0 |
| `pdf-desktop` | — | 5 | 0 |
| `pdf-diff` | AGPL-3.0-or-later | 4 | 0 |
| `pdf-docx` | AGPL-3.0-or-later | 4 | 0 |
| `pdf-engine` | AGPL-3.0-or-later | 15 | 0 |
| `pdf-extract` | AGPL-3.0-or-later | 5 | 0 |
| `pdf-forms` | AGPL-3.0-or-later | 15 | 0 |
| `pdf-invoice` | AGPL-3.0-or-later | 9 | 0 |
| `pdf-java` | AGPL-3.0-or-later | 1 | 0 |
| `pdf-manip` | AGPL-3.0-or-later | 35 | 3 |
| `pdf-node` | AGPL-3.0-or-later | 9 | 0 |
| `pdf-ocr` | AGPL-3.0-or-later | 13 | 1 |
| `pdf-pptx` | AGPL-3.0-or-later | 3 | 0 |
| `pdf-python` | AGPL-3.0-or-later | 2 | 0 |
| `pdf-redact` | AGPL-3.0-or-later | 5 | 0 |
| `pdf-sign` | AGPL-3.0-or-later | 16 | 0 |
| `pdf-standard-fonts` | AGPL-3.0-or-later | 1 | 0 |
| `pdf-text-format` | AGPL-3.0-or-later | 1 | 0 |
| `pdf-xfa` | AGPL-3.0-or-later | 25 | 1 |
| `pdf-xlsx` | AGPL-3.0-or-later | 3 | 0 |
| `pdfluent` | AGPL-3.0-or-later | 23 | 0 |
| `pdfluent-cli` | AGPL-3.0-or-later | 2 | 0 |
| `xfa-api-server` | — | 4 | 0 |
| `xfa-cli` | AGPL-3.0-or-later | 24 | 0 |
| `xfa-dom-resolver` | AGPL-3.0-or-later | 6 | 0 |
| `xfa-golden-tests` | — | 2 | 0 |
| `xfa-js-sandboxed` | AGPL-3.0-or-later | 5 | 0 |
| `xfa-json` | AGPL-3.0-or-later | 6 | 0 |
| `xfa-layout-engine` | AGPL-3.0-or-later | 7 | 0 |
| `xfa-license` | AGPL-3.0-or-later | 5 | 0 |
| `xfa-license-gen` | — | 1 | 0 |
| `xfa-pdfrest-compare` | AGPL-3.0-or-later | 1 | 0 |
| `xfa-test-runner` | — | 65 | 0 |
| `xfa-wasm` | AGPL-3.0-or-later | 7 | 0 |

## Andermans werk

Allemaal permissief en geattribueerd in `NOTICE` en `THIRD_PARTY_LICENSES.txt`.
**Deze crates horen onze header níét te krijgen** — een sweep die dat wel doet,
claimt andermans werk.

| crate | upstream | licentie | .rs | met header |
|---|---|---|---:|---:|
| `cff-parser` | ttf-parser / pdf.js (Reizner, Muizelaar) | MIT OR Apache-2.0 | 10 | 0 |
| `hayro-ccitt` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 4 | 0 |
| `hayro-jbig2` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 21 | 0 |
| `hayro-jpeg2000` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 26 | 0 |
| `lopdf` | lopdf | MIT | 38 | 0 |
| `pdf-font` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 36 | 0 |
| `pdf-interpret` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 42 | 0 |
| `pdf-render` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 2 | 0 |
| `pdf-syntax` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT | 47 | 0 |

## Gekopieerde fragmenten, met naam

| plek | bron | licentie |
|---|---|---|
| `pdfluent-jpeg2000/src/lib.rs` | OpenJPEG | BSD-2-Clause |
| `pdf-interpret/src/color.rs:551` | [pdf.js `colorspace.js#L846`](https://github.com/mozilla/pdf.js/blob/06f44916/src/core/colorspace.js#L846) | Apache-2.0 |

Die tweede stond in #213 als *"dat 'there' heeft geen naam"*. Hij heeft er wel
een: de regel erbóven draagt de volledige pdf.js-permalink.

## Eigen werk, geen fork

`pdf-content-stream` en `pdf-diff` stonden in #213 als *"eigen keuze of fork?"*.
Nagetrokken in de historie: allebei beginnen met een commit die *"New crate"*
zegt en een eigen ontwerp beschrijft. Geen fork.
