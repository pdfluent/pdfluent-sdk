# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""The markdown switches for running heads and page separators.

``extract_markdown`` always dropped the running heads and always wrote
``<!-- page N -->``. ``include_headers=True`` writes the heads and
``include_footers=True`` the page numbers, as plain paragraphs at their
reading-order position, and ``page_separators="none"`` drops the marker.
The defaults are today's output, byte for byte.

``fixtures/running-head.pdf`` is the four-page report with a running head and
a page number on every page, built and pinned by
``crates/pdf-engine/tests/markdown_furniture_and_separators.rs``.

Run:
    cd crates/pdf-python
    maturin develop
    pytest tests/test_markdown_furniture.py
"""

import os

import pytest

from pdfluent import Document, PdfluentValidationError

FIXTURES = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures")
STRUCTURED_PDF = os.path.join(FIXTURES, "structured.pdf")
RUNNING_HEAD_PDF = os.path.join(FIXTURES, "running-head.pdf")
HEAD = "Annual Report 2026"


@pytest.fixture()
def doc():
    with Document(STRUCTURED_PDF) as opened:
        yield opened


@pytest.fixture()
def report():
    with Document(RUNNING_HEAD_PDF) as opened:
        yield opened


def test_the_default_is_pinned_byte_for_byte(doc):
    assert doc.extract_markdown() == (
        "# Safety rules\n"
        "\n"
        "- Wear the issued boots on the floor.\n"
        "- Keep the gangways clear of stock.\n"
        "- Report a spill before you clean it.\n"
        "\n"
        "<!-- page 1 -->\n"
        "\n"
        "| Band | Units | Rate |\n"
        "| --- | --- | --- |\n"
        "| Standing | none | 18.90 |\n"
        "| Day | 0 to 500 | 24.31 |\n"
        "| Night | 0 to 500 | 11.07 |\n"
    )


def test_the_default_drops_the_running_head(report):
    markdown = report.extract_markdown()

    assert HEAD not in markdown
    assert "Page 1 of 4" not in markdown
    assert "<!-- page 1 -->" in markdown


def test_furniture_is_written_as_plain_paragraphs(report):
    markdown = report.extract_markdown(include_headers=True, include_footers=True)

    assert markdown.count(HEAD) == 4
    assert "Page 1 of 4" in markdown
    assert not any(
        line.startswith("#") and "Annual Report" in line
        for line in markdown.splitlines()
    )


def test_headers_on_footers_off_keeps_only_the_running_head(report):
    markdown = report.extract_markdown(include_headers=True, include_footers=False)

    assert markdown.count(HEAD) == 4
    assert "Page 1 of 4" not in markdown
    assert "Page 2 of 4" not in markdown


def test_footers_on_headers_off_keeps_only_the_page_number(report):
    markdown = report.extract_markdown(include_headers=False, include_footers=True)

    assert HEAD not in markdown
    for page in (1, 2, 3, 4):
        assert f"Page {page} of 4" in markdown


def test_both_furniture_switches_off_is_the_default(report):
    assert report.extract_markdown(
        include_headers=False, include_footers=False
    ) == report.extract_markdown()


def test_no_separators_means_no_marker_and_paragraphs_still_apart(report):
    markdown = report.extract_markdown(page_separators="none")

    assert "<!-- page" not in markdown
    assert (
        "It runs on for a second line of its own body type.\n"
        "\n"
        "The 1th section opens here and says something." in markdown
    )


def test_an_unknown_separator_is_rejected(report):
    with pytest.raises(PdfluentValidationError):
        report.extract_markdown(page_separators="rule")
