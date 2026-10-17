# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Hierarchical chunking: section paths and split tables.

The chunker only ever sees the block stream of ``Document.extract_blocks``,
so these tests feed it representative streams through a stub document rather
than depending on what the heading and table models decide about a PDF --
those decisions belong to other teams. The real-PDF path stays covered by
``test_chunks.py`` over ``fixtures/structured.pdf``.
"""

import os
from types import SimpleNamespace

import pytest

import pdfluent.chunks
from pdfluent.chunks import iter_chunks

STRUCTURED_PDF = os.path.join(
    os.path.dirname(__file__), "..", "..", "..", "fixtures", "structured.pdf"
)


def make_block(kind, text, page=0, level=None, row=None, column=None):
    """One fake block with every attribute the chunker reads."""
    return SimpleNamespace(
        kind=kind,
        text=text,
        page=page,
        bbox=[0.0, 0.0, 10.0, 10.0],
        confidence=None,
        level=level,
        continues_previous=False,
        row=row,
        column=column,
    )


def make_table(rows, page=0):
    """Fake table cells for rows of cell texts, emitted row by row."""
    cells = []
    for row, texts in enumerate(rows):
        for column, text in enumerate(texts):
            cells.append(make_block("table_cell", text, page, row=row, column=column))
    return cells


class StubDocument:
    """An open document whose blocks and pages the test chose."""

    def __init__(self, blocks, pages=1, markdown=None, bookmarks=None):
        self._blocks = blocks
        self.page_count = pages
        self._markdown = markdown or {}
        self.bookmarks = bookmarks or []

    def extract_blocks(self, first_page=0, page_count=None):
        last = self.page_count if page_count is None else first_page + page_count
        return [b for b in self._blocks if first_page <= b.page < last]

    def extract_markdown(self, first_page=0, page_count=1):
        return self._markdown.get(first_page, "")


@pytest.fixture
def chunk(monkeypatch):
    """Chunk a block stream the way ``iter_chunks`` sees an open document."""

    def run(blocks, pages=1, markdown=None, **kwargs):
        monkeypatch.setattr(pdfluent.chunks, "Document", StubDocument)
        return list(iter_chunks(StubDocument(blocks, pages, markdown), **kwargs))

    return run


def nested_document():
    return [
        make_block("heading", "2 Methods", level=1),
        make_block("paragraph", "How we measured."),
        make_block("heading", "2.1 Data", level=2),
        make_block("paragraph", "Where it came from."),
        make_block("heading", "2.2 Model", level=2),
        make_block("paragraph", "What we fitted."),
        make_block("heading", "3 Results", level=1),
        make_block("paragraph", "What we found."),
    ]


def test_hierarchical_chunks_carry_the_enclosing_headings(chunk):
    chunks = chunk(nested_document(), hierarchical=True)

    assert [c.text for c in chunks] == [
        "2 Methods",
        "How we measured.",
        "2.1 Data",
        "Where it came from.",
        "2.2 Model",
        "What we fitted.",
        "3 Results",
        "What we found.",
    ]
    assert [c.metadata["section_path"] for c in chunks] == [
        [],
        ["2 Methods"],
        ["2 Methods"],
        ["2 Methods", "2.1 Data"],
        ["2 Methods"],
        ["2 Methods", "2.2 Model"],
        [],
        ["3 Results"],
    ]
    # The old keys stay what they were: `section` is the last heading above.
    assert chunks[3].metadata["section"] == "2.1 Data"
    assert chunks[5].metadata["section"] == "2.2 Model"
    assert chunks[5].metadata["kind"] == "paragraph"


def test_the_default_mode_adds_no_hierarchical_keys(chunk):
    assert all("section_path" not in c.metadata for c in chunk(nested_document()))
    long_table = make_table([["Name", "Role"]] + [[f"m{n}", "r"] for n in range(12)])
    whole = chunk(long_table, max_chars=80)
    assert len(whole) == 1
    assert "table_part" not in whole[0].metadata


def test_a_long_table_is_split_by_rows_with_the_header_repeated(chunk):
    header = ["Name", "Role"]
    data = [[f"member-{n:02d}", "reviewer"] for n in range(12)]
    rows = [header] + data
    chunks = chunk(make_table(rows), hierarchical=True, max_chars=80)

    assert len(chunks) > 1
    count = len(chunks)
    assert [c.metadata["table_part"] for c in chunks] == [
        f"{at} of {count}" for at in range(1, count + 1)
    ]
    for piece in chunks:
        assert piece.metadata["kind"] == "table"
        assert piece.text.splitlines()[0] == "Name | Role"
        assert len(piece.text) <= 80
    assert [
        line for piece in chunks for line in piece.text.splitlines()[1:]
    ] == ["member-%02d | reviewer" % n for n in range(12)]


def test_a_table_that_fits_stays_one_chunk_without_part_numbers(chunk):
    rows = [["Band", "Units"], ["Day", "500"], ["Night", "500"]]
    chunks = chunk(make_table(rows), hierarchical=True)

    assert len(chunks) == 1
    assert chunks[0].text.splitlines() == ["Band | Units", "Day | 500", "Night | 500"]
    assert "table_part" not in chunks[0].metadata
    assert chunks[0].metadata["section_path"] == []


def test_list_items_are_never_split(chunk):
    items = [make_block("list_item", f"item {n}: " + "x" * 300) for n in range(3)]
    chunks = chunk(items, hierarchical=True, max_chars=100)

    assert [c.metadata["kind"] for c in chunks] == ["list_item"] * 3
    assert [c.text for c in chunks] == [b.text for b in items]


def test_table_rows_are_never_split(chunk):
    huge = "target " + "y" * 500
    rows = [["Key", "Value"], ["a", "1"], ["big", huge], ["b", "2"]]
    chunks = chunk(make_table(rows), hierarchical=True, max_chars=100)

    assert len(chunks) > 1
    whole = "big | " + huge
    holders = [c for c in chunks if whole in c.text.splitlines()]
    assert len(holders) == 1
    for piece in chunks:
        assert piece.text.splitlines()[0] == "Key | Value"


def test_markdown_pages_carry_their_section_path(chunk):
    blocks = [
        make_block("heading", "2 Methods", page=0, level=1),
        make_block("paragraph", "How we measured.", page=0),
        make_block("heading", "2.1 Data", page=1, level=2),
    ]
    markdown = {0: "# 2 Methods\n\nHow we measured.", 1: "## 2.1 Data"}

    chunks = chunk(blocks, pages=2, markdown=markdown, mode="markdown",
                   hierarchical=True)

    assert [(c.metadata["page"], c.metadata["section_path"]) for c in chunks] == [
        (0, []),
        (1, ["2 Methods"]),
    ]
    plain = chunk(blocks, pages=2, markdown=markdown, mode="markdown")
    assert all("section_path" not in c.metadata for c in plain)


def test_a_non_positive_budget_is_refused(chunk):
    with pytest.raises(ValueError):
        chunk(nested_document(), hierarchical=True, max_chars=0)


def test_structured_pdf_hierarchical_paths_agree_with_its_section(chunk):
    chunks = list(iter_chunks(STRUCTURED_PDF, hierarchical=True))

    assert [c.metadata["kind"] for c in chunks] == [
        "heading",
        "list_item",
        "list_item",
        "list_item",
        "table",
    ]
    assert chunks[0].metadata["section_path"] == []
    for piece in chunks[1:]:
        assert piece.metadata["section_path"] == ["Safety rules"]
        assert piece.metadata["section"] == "Safety rules"
    assert "table_part" not in chunks[-1].metadata
