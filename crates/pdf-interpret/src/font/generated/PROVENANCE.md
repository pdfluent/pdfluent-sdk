# Provenance of the glyph-name data

The generator (`scripts/font/build_glyph_names.py`, phase B) rebuilds the
glyph-name table in this directory from permissively licensed sources only.
This file records every source used and every candidate excluded, with the
licence evidence for each. Checked 2026-10-09.

## Used

| # | File | URL (pinned) | Commit / date | Licence (SPDX) | sha256 | Size |
|---|------|--------------|---------------|----------------|--------|------|
| 1 | `glyphlist.txt` (AGL: name to Unicode value pairs) | <https://github.com/adobe-type-tools/agl-aglfn/blob/4036a9ca80a62f64f9de4f7321a9a045ad0ecfd6/glyphlist.txt> | `4036a9c` / 2019-10-31 | BSD-3-Clause | `a3b2f61ced9f3644cc0d4ecde5c59df34ca286c689d9484a43a710a81c466789` | 78,060 bytes, 4,281 records |
| 2 | `aglfn.txt` (AGLFN: recommended names for new fonts) | <https://github.com/adobe-type-tools/agl-aglfn/blob/4036a9ca80a62f64f9de4f7321a9a045ad0ecfd6/aglfn.txt> | `4036a9c` / 2019-10-31 | BSD-3-Clause | `ed735a9ea58549b4cb2b8e804341548adb25aff43e81001ac7f3917247cacca1` | 27,655 bytes, 586 records |
| 3 | AGL Specification text (rules only, no data table) | <https://github.com/adobe-type-tools/agl-specification/blob/60b5ffae620435936c6fcaf8c8c038aa8f49673f/README.md> | `60b5ffa` / 2019-08-22, doc v2.9 | Custom permissive Adobe documentation licence (derivative works allowed; no SPDX id) | `eb14692b0ff134fd44dd2dae014b7a0ea81fccb893982f950e6bf163eb6827a3` | 21,165 bytes |

Licence evidence:

- Sources 1-2: `LICENSE.md` in the pinned `agl-aglfn` repository, which is
  the three-clause BSD text (sha256
  `58147d341e7a34aa2196862395a34d2fd95716c41d5ed26efb59ab0e12f92089`).
  The licence permits redistribution, so the generator may vendor these
  files with the licence text.
- Source 3: the permission notice at the top of the specification document
  itself, which allows use, copying, distribution and derivative works.

What phase B takes from each source:

- Source 1 feeds the name-to-Unicode table.
- Source 2 feeds whatever the generator needs beyond source 1 (kept pinned
  together with it).
- Source 3 feeds the name-handling rules only: cut the name at the first
  `.`, split what remains on `_`, resolve each part, and decode the `uni`
  and `u` spellings. Our implementation and comments are written in our
  own words.

Two facts the tests rely on, both verified against the pinned files above:
`f_i` has no row in AGL, so it can only resolve through the
underscore-splitting rule; the `uni`/`u` spellings take uppercase hex
digits only, so the tests use uppercase.

## Excluded

No permissively licensed TeX-specific name source survived, so phase B
ships AGL+AGLFN only. Every candidate below was judged from its package
licence record; no candidate data file was downloaded or opened.

| Candidate | Why excluded | Licence evidence |
|-----------|--------------|------------------|
| `texglyphlist.txt` (lcdf-typetools) | GPL-licensed; fetching or reading it is forbidden | Job spec records it as GPL; never downloaded, never opened |
| dvips `.enc` encoding files | Package licence is CTAN `other-free`, which is not unambiguously public domain, BSD, MIT or equivalent | CTAN JSON package metadata for `dvips` (licence field), queried 2026-10-09 |
| pdftex `.enc` encoding files | GPL | CTAN JSON package metadata for `pdftex` (licence field), queried 2026-10-09 |
| fontinst encoding/metric files | LPPL-1.1 | CTAN JSON package metadata for `fontinst` (licence field), queried 2026-10-09 |

## AMS glyph names (`ams_glyph_names.rs`)

A second, hand-written table that the name reader consults only after every
AGL rule has failed. It holds names, never font programs or outlines.

| # | File | Source | Licence (SPDX) | Rows | Decision |
|---|------|--------|----------------|------|----------|
| 4 | `ams_glyph_names.rs` (from the approved `specs/text/ams-glyph-names.tsv`, `specs/text/ams-glyph-names-phase2.tsv`, `specs/text/AMS-GLYPH-NAMES-PHASE3.tsv`, `specs/text/AMS-GLYPH-NAMES-PHASE4.tsv` and `specs/text/AMS-GLYPH-NAMES-PHASE5.tsv`) | `legal/evidence/cleanroom-550`: AMSFonts 3.04 glyph names; Unicode 16.0 code charts | OFL-1.1 (AMSFonts names); Unicode-3.0 (code charts) | 10: `squaredot` U+22A1, `square` U+25A1, `dblarrowup` U+21C8, `dblarrowdwn` U+21CA, `triangle` U+25B3; phase 2: `circlecopyrt` U+20DD, owner decision 13-10 option B; phase 3: `negationslash` U+0338, spec C5a 13-10, reviewer CONFIRMED 13-10 23:14; phase 4: `angbracketleft` U+27E8, `angbracketright` U+27E9, spec C5c 14-10, reviewer CONFIRMED 14-10 02:31; phase 5: `visiblespace` U+2423, source row 5 | owner decision 13-10-2026 |
| 5 | `ams_glyph_names.rs`, phase 5 (from the approved `specs/text/AMS-GLYPH-NAMES-PHASE5.tsv`) | The glyph name `visiblespace` from `cmtt10.pfb`, AMSFonts 3.04 (sha256 `d6bdecc0378f3cd16683d1ac348bdbc4ccde9a552e87b53fe9e2de5cfc7aa61d`); no other cmtt10 name, no other font, no outlines or metrics. Value from the Unicode 16.0 code charts (U+2423 OPEN BOX). Copyright (c) 1997, 2009 American Mathematical Society; Reserved Font Name CMTT10 | OFL-1.1 (AMSFonts name); Unicode-3.0 (code charts) | 1: `visiblespace` U+2423, spec C5b 14-10, reviewer CONFIRMED 13-10 23:20 | owner GO 14-10-2026 (`release/DECISION-C5B-CMTT10-14-10.md`) |

Out of scope rather than excluded: `zapfdingbats.txt` sits in the same
pinned repository under the same BSD licence but feeds the separate
dingbats table, which this job does not rebuild. Knuth's public-domain
TeX sources carry no glyph-name data, so they were never candidates.
