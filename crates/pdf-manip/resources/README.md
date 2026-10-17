# Adobe CMap resources

Two directories, both holding unmodified files from Adobe's public CMap
resources (BSD-3-Clause; the licence is in the `%%Copyright` header of every
file and, in full, in `cid-tables/LICENSE-Adobe-CMap.txt`).

They are split because they are read by different code for different reasons,
and putting a file in the wrong one changes what the converter does:

* `cmap/` is the first entry of `pdfa_fonts::PREDEFINED_CMAP_SEARCH_DIRS`, the
  *host* search path used by the width and `.notdef` repairs. A file dropped
  here changes which documents those passes act on -- adding the five tables
  below to it embedded a 79 kB `/CMap` stream into `issue14438` and cost more
  than the subset it was there to enable.
* `cid-tables/` is read only by `cmap_codes.rs`, which compiles the files in
  with `include_bytes!` and uses them to decode the character codes a document
  draws into CIDs. Nothing searches this directory at run time.

## Provenance

Every file is byte-identical to the file of that name in
<https://github.com/adobe-type-tools/cmap-resources> at commit
`f5cf3bca7fdfeaceb77aa82847e974f2306c20b4` (2023-11-15) -- `GB-EUC-H`,
`GBK-EUC-H`, `GBKp-EUC-H` and `UniGB-UTF16-H` from `Adobe-GB1-6/CMap/`,
`90ms-RKSJ-H` and `UniJIS-UTF16-H` from `Adobe-Japan1-7/CMap/`.

`PROVENANCE.sha256` carries the checksums and can be re-checked with
`shasum -a 256 -c PROVENANCE.sha256` from this directory, so a build that
picked up a different copy is detectable rather than a matter of trust.

## Why these six

They are the predefined CMaps the pinned gate corpus names outside the Identity
pair. A CMap that is not here is not guessed at: `cmap_codes::predefined`
answers nothing, and the subsetter leaves the font whole -- the behaviour every
composite font under a predefined CMap had before #503.
