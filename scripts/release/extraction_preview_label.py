#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Structured extraction ships as Preview in 1.1.0, on every surface (#636).

Owner decision 06-10-2026: the calls that return document structure --
``extract_blocks`` and ``extract_markdown`` with their per-binding spellings,
``extract_docling``, the block serializers and the chunkers built on them --
carry one exact sentence in their doc comments. Plain-text extraction
(``extract_text`` and friends) is not preview and must not carry it.

This guard reads the doc block attached to each listed entry point --
the comment lines directly above the definition, attributes skipped --
and refuses when the sentence is missing from one that must carry it,
present on one that must not, or when an anchor no longer names exactly
one definition (a rename that silently drops a symbol from this list).
"""
from __future__ import annotations

import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]

LABEL = "Preview: output quality is still improving and the API may change in a minor release."

# (file, definition anchor) pairs whose doc block must contain LABEL.
LABELLED: list[tuple[str, str]] = [
    # Rust facade + engine.
    ("crates/pdfluent/src/document.rs", "pub fn extract_blocks(&self, options: &ExtractOptions)"),
    ("crates/pdfluent/src/document.rs", "pub fn extract_markdown(&self, options: &ExtractOptions)"),
    ("crates/pdfluent/src/document.rs", "pub fn extract_docling(&self, name: &str, options: &ExtractOptions)"),
    ("crates/pdf-engine/src/document.rs", "pub fn extract_page_blocks(&self, index: usize, first_index: usize)"),
    ("crates/pdf-engine/src/document.rs", "pub fn extract_blocks(&self, options: &ExtractOptions)"),
    ("crates/pdf-engine/src/document.rs", "pub fn extract_markdown(&self, options: &ExtractOptions)"),
    ("crates/pdf-engine/src/document.rs", "pub fn extract_docling(&self, name: &str, options: &ExtractOptions)"),
    ("crates/pdf-engine/src/document.rs", "pub fn extract_words(&self, options: &ExtractOptions)"),
    ("crates/pdf-engine/src/json.rs", "pub fn to_json(blocks: &[Block])"),
    ("crates/pdf-engine/src/markdown.rs", "pub fn to_markdown(blocks: &[Block])"),
    ("crates/pdf-engine/src/markdown.rs", "pub fn to_markdown_with_code(blocks: &[Block], code: bool)"),
    ("crates/pdf-engine/src/markdown.rs", "pub fn to_markdown_with_options(blocks: &[Block], options: &MarkdownOptions)"),
    ("crates/pdf-engine/src/docling.rs", "pub fn to_docling_json(name: &str, pages: &[DoclingPage], blocks: &[Block])"),
    # Python: Rust sources, stubs, chunker.
    ("crates/pdf-python/src/lib.rs", "fn extract_words("),
    ("crates/pdf-python/python/pdfluent/__init__.pyi", "def extract_words("),
    # Node: napi sources, generated d.ts, hand-written chunker.
    ("crates/pdf-node/src/document.rs", "pub fn extract_markdown("),
    ("crates/pdf-node/src/document.rs", "pub fn extract_docling("),
    ("crates/pdf-node/src/document.rs", "pub fn extract_blocks("),
    ("crates/pdf-node/src/document.rs", "pub fn extract_words(&self, options: Option<ExtractOpts>)"),
    ("crates/pdf-node/index.d.ts", "extractMarkdown(options"),
    ("crates/pdf-node/index.d.ts", "extractDocling(options"),
    ("crates/pdf-node/index.d.ts", "extractBlocks(options"),
    ("crates/pdf-node/index.d.ts", "extractWords(options"),
    ("crates/pdf-node/chunks.js", "function iterChunks("),
    ("crates/pdf-node/chunks.d.ts", "function iterChunks("),
    # WASM: Rust sources (no generated d.ts is checked in).
    ("crates/xfa-wasm/src/lib.rs", "pub fn extract_markdown("),
    ("crates/xfa-wasm/src/lib.rs", "pub fn extract_blocks("),
    ("crates/xfa-wasm/src/lib.rs", "pub fn extract_docling("),
    # Java + JNI.
    ("bindings/java/src/main/java/com/pdfluent/PdfluentDocument.java", "public String extractMarkdown() {"),
    ("bindings/java/src/main/java/com/pdfluent/PdfluentDocument.java", "public String extractMarkdown(int firstPage, int pageCount) {"),
    ("bindings/java/src/main/java/com/pdfluent/PdfluentDocument.java", "public String extractDocling() {"),
    ("bindings/java/src/main/java/com/pdfluent/PdfluentDocument.java", "public String extractDocling(int firstPage, int pageCount, String name) {"),
    ("bindings/java/src/main/java/com/pdfluent/PdfluentDocument.java", "public java.util.List<Block> extractBlocks() {"),
    ("bindings/java/src/main/java/com/pdfluent/PdfluentDocument.java", "public java.util.List<Block> extractBlocks(int firstPage, int pageCount) {"),
    ("crates/pdf-java/src/lib.rs", "Document_nativeExtractMarkdown<'a>("),
    ("crates/pdf-java/src/lib.rs", "Document_nativeExtractDocling<'a>("),
    ("crates/pdf-java/src/lib.rs", "Document_nativeExtractBlocks<'a>("),
    ("crates/pdf-java/src/lib.rs", "Document_nativeExtractBlocksWithConfidence<'a>("),
    # .NET.
    ("bindings/dotnet/src/PDFluent/PdfDocument.cs", "public string ExtractMarkdown("),
    ("bindings/dotnet/src/PDFluent/PdfDocument.cs", "public string ExtractBlocks("),
    ("bindings/dotnet/src/PDFluent/PdfDocument.cs", "public string ExtractDocling("),
    # C header + Rust mirrors.
    ("crates/pdf-capi/include/pdfluent.h", "pdf_document_extract_markdown(const PdfDocument *doc,"),
    ("crates/pdf-capi/include/pdfluent.h", "pdf_document_extract_blocks_json(const PdfDocument *doc,"),
    ("crates/pdf-capi/include/pdfluent.h", "pdf_document_extract_docling_json(const PdfDocument *doc,"),
    ("crates/pdf-capi/src/lib.rs", "pub unsafe extern \"C\" fn pdf_document_extract_markdown("),
    ("crates/pdf-capi/src/lib.rs", "pub unsafe extern \"C\" fn pdf_document_extract_blocks_json("),
    ("crates/pdf-capi/src/lib.rs", "pub unsafe extern \"C\" fn pdf_document_extract_docling_json("),
]

# (file, definition anchor) pairs whose doc block must NOT contain LABEL:
# plain-text extraction is not preview.
UNLABELLED: list[tuple[str, str]] = [
    ("crates/pdfluent/src/document.rs", "pub fn extract_text(&self) -> Result<String> {"),
    ("crates/pdfluent/src/document.rs", "pub fn extract_text_pages(&self) -> Result<Vec<String>> {"),
    ("crates/pdf-engine/src/document.rs", "pub fn extract_text(&self, index: usize) -> Result<String> {"),
    ("crates/pdf-python/src/lib.rs", "fn extract_text(&self) -> PyResult<String> {"),
    ("crates/pdf-node/src/document.rs", "pub fn extract_text(&self, index: u32) -> Result<String> {"),
    ("crates/pdf-node/index.d.ts", "extractText(index: number): string"),
    ("crates/xfa-wasm/src/lib.rs", "pub fn text(&self, page_index: usize) -> String {"),
    ("bindings/java/src/main/java/com/pdfluent/PdfluentDocument.java", "public String extractText(int pageIndex) {"),
    ("bindings/dotnet/src/PDFluent/PdfDocument.cs", "public string ExtractText(int pageIndex)"),
    ("crates/pdf-capi/include/pdfluent.h", "pdf_page_extract_text(const PdfDocument *doc, int32_t page_index);"),
]


def _comment_block(lines: list[str], anchor_at: int) -> list[str]:
    """The doc lines directly above line `anchor_at`, attributes skipped.

    Rust/C# docs are `///` lines, Java/C/JS/TS `*` lines inside a block
    comment; a Rust attribute (`#[...]`) between the docs and the item is
    skipped rather than ending the block.
    """
    block: list[str] = []
    i = anchor_at - 1
    while i >= 0:
        stripped = lines[i].strip()
        if stripped.startswith("///") or stripped.startswith("#["):
            block.append(lines[i])
        elif stripped.startswith("*") or stripped.startswith("/*") or stripped == "*/":
            block.append(lines[i])
        else:
            break
        i -= 1
    block.reverse()
    return block


def _docstring_block(lines: list[str], anchor_at: int) -> list[str]:
    """The docstring directly below a Python `def` line."""
    i = anchor_at
    while i < len(lines) and not lines[i].rstrip().endswith(":"):
        i += 1
    i += 1
    while i < len(lines) and not lines[i].strip():
        i += 1
    if i >= len(lines) or '\"\"\"' not in lines[i]:
        return []
    block = [lines[i]]
    if lines[i].count('\"\"\"') >= 2 and lines[i].strip() != '\"\"\"':
        return block
    i += 1
    while i < len(lines):
        block.append(lines[i])
        if '\"\"\"' in lines[i]:
            break
        i += 1
    return block


def _doc_block(path: pathlib.Path, lines: list[str], anchor_at: int) -> list[str]:
    if path.suffix in {".py", ".pyi"}:
        return _docstring_block(lines, anchor_at)
    return _comment_block(lines, anchor_at)


def main() -> int:
    failures: list[str] = []
    checked = 0
    for rel, anchor in LABELLED + [(r, a) for r, a in UNLABELLED]:
        path = REPO / rel
        if not path.is_file():
            failures.append(f"{rel}: file is missing")
            continue
        lines = path.read_text().splitlines()
        hits = [n for n, line in enumerate(lines) if anchor in line]
        if len(hits) != 1:
            failures.append(f"{rel}: anchor {anchor!r} found {len(hits)} times, want exactly once")
            continue
        block = _doc_block(path, lines, hits[0])
        if not block:
            failures.append(f"{rel}: no doc block attached to {anchor!r}")
            continue
        has = any(LABEL in line for line in block)
        must = (rel, anchor) in LABELLED
        checked += 1
        if must and not has:
            failures.append(f"{rel}: {anchor!r} is missing the Preview sentence")
        elif not must and has:
            failures.append(f"{rel}: {anchor!r} carries the Preview sentence but is plain-text extraction")
    if LABEL not in (REPO / "CHANGELOG.md").read_text():
        failures.append("CHANGELOG.md: the Preview sentence is missing")
    else:
        checked += 1
    print(f"[preview-label] {checked} doc block(s) checked, {len(failures)} failure(s)")
    for failure in failures:
        print(f"FAIL: {failure}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
