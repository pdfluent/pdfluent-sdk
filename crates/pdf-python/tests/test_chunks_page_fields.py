# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The per-page fields of markdown-mode chunks: ``toc_items`` and ``tables``.

The shape a vector-store pipeline ingests, read from this binding's own
outline and blocks. Real PDFs pin the values;
stubs pin the grouping rules, so a model changing its mind cannot turn these
red -- those decisions belong to other teams.
"""

import os
from types import SimpleNamespace

import pdfluent.chunks
from pdfluent.chunks import iter_chunks

FIXTURES = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures")
STRUCTURED_PDF = os.path.join(FIXTURES, "structured.pdf")
OUTLINED_PDF = os.path.join(FIXTURES, "outlined.pdf")


def make_block(kind, text, page=0, row=None, column=None):
    """One fake block with every attribute the chunker reads."""
    return SimpleNamespace(
        kind=kind,
        text=text,
        page=page,
        bbox=[0.0, 0.0, 10.0, 10.0],
        confidence=None,
        level=None,
        continues_previous=False,
        row=row,
        column=column,
    )


def make_bookmark(title, page, children=()):
    return SimpleNamespace(title=title, page=page, children=list(children))


class StubDocument:
    """An open document whose blocks, pages and outline the test chose."""

    def __init__(self, blocks, pages=1, markdown=None, bookmarks=()):
        self._blocks = blocks
        self.page_count = pages
        self._markdown = markdown or {}
        self.bookmarks = list(bookmarks)

    def extract_blocks(self, first_page=0, page_count=None):
        last = self.page_count if page_count is None else first_page + page_count
        return [b for b in self._blocks if first_page <= b.page < last]

    def extract_markdown(self, first_page=0, page_count=1):
        return self._markdown.get(first_page, "")


def chunk(monkeypatch, blocks, pages=1, markdown=None, bookmarks=(), **kwargs):
    monkeypatch.setattr(pdfluent.chunks, "Document", StubDocument)
    return list(
        iter_chunks(StubDocument(blocks, pages, markdown, bookmarks), **kwargs)
    )


def test_tables_holds_one_entry_with_the_tables_box_and_extent():
    chunks = list(iter_chunks(STRUCTURED_PDF, mode="markdown"))

    assert [c.metadata["page"] for c in chunks] == [0, 1]
    assert chunks[0].metadata["tables"] == []
    (table,) = chunks[1].metadata["tables"]
    assert table["rows"] == 4
    assert table["columns"] == 3
    # The same box the table chunk carries in blocks mode: one grouping.
    blocks = list(iter_chunks(STRUCTURED_PDF))
    assert table["bbox"] == blocks[-1].metadata["bbox"]


def test_tables_are_grouped_per_page_like_blocks_mode_groups_them(monkeypatch):
    cells = []
    for page in (0, 1):
        for row in range(3):
            for column in range(2):
                cells.append(
                    make_block("table_cell", f"p{page}r{row}c{column}",
                               page, row, column)
                )
    markdown = {0: "first", 1: "second"}

    chunks = chunk(monkeypatch, cells, pages=2, markdown=markdown,
                   mode="markdown")

    assert [len(c.metadata["tables"]) for c in chunks] == [1, 1]
    assert chunks[0].metadata["tables"][0]["rows"] == 3
    assert chunks[0].metadata["tables"][0]["columns"] == 2


def test_toc_items_holds_the_entries_for_its_page():
    chunks = list(iter_chunks(OUTLINED_PDF, mode="markdown"))

    assert [c.metadata["toc_items"] for c in chunks] == [
        [[1, "Chapter 1", 0]],
        [[1, "Chapter 2", 1], [2, "Section 2.1", 1]],
        [[1, "Appendix A", 2]],
    ]


def test_a_page_without_an_entry_gets_an_empty_list():
    chunks = list(iter_chunks(STRUCTURED_PDF, mode="markdown"))

    assert chunks[0].metadata["toc_items"] == []
    assert chunks[1].metadata["toc_items"] == []


def test_entries_without_a_page_are_left_out_but_keep_their_children(monkeypatch):
    bookmarks = [
        make_bookmark("Part", None, [make_bookmark("Chapter 1", 0)]),
    ]

    chunks = chunk(monkeypatch, [], pages=1, markdown={0: "text"},
                   bookmarks=bookmarks, mode="markdown")

    # The child keeps its depth: the level is the depth in the tree, not in
    # what survived.
    assert chunks[0].metadata["toc_items"] == [[2, "Chapter 1", 0]]


def test_blocks_mode_gains_no_page_keys():
    for piece in iter_chunks(OUTLINED_PDF):
        assert "toc_items" not in piece.metadata
        assert "tables" not in piece.metadata


def test_images_and_words_stay_absent():
    # The binding exposes neither the boxes behind the <!-- image --> markers
    # nor word boxes (`extract_words` has not landed), and an empty guess
    # would be worse than no key. When word boxes land, this test changes to
    # expect them.
    for piece in iter_chunks(OUTLINED_PDF, mode="markdown"):
        assert "images" not in piece.metadata
        assert "words" not in piece.metadata
