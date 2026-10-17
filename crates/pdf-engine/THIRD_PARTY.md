# Third-party code in pdf-engine

## table-transformer (MIT) — TATR post-processing

The table-structure post-processing behind the `ort-tables` feature is a Rust
port of Microsoft's Table Transformer post-processing
(`objects_to_cells` and everything it calls), in this crate at
`src/learned/tables_tatr.rs`. The Python original it was ported from is
vendored verbatim in the PDFluent repository at
`scripts/extraction/tables_b/tatr_postprocess.py`.

Upstream: <https://github.com/microsoft/table-transformer>
at commit `16d124f616109746b7785f03085100f1f6247575`.

Copyright (c) Microsoft Corporation.

The full entry is the table-transformer section of
`THIRD_PARTY_LICENSES.txt` in the repository root; the licence text travels
here as well, because a crates.io reader has that section in front of them
only through the repository.

--- MIT License (that commit's LICENSE, word for word) ---

MIT License

Copyright (c) Microsoft Corporation.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE
