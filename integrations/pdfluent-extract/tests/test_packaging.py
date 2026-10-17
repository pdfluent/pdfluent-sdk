# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The metadata and README the PyPI page renders for pdfluent-extract.

Every test pins one thing a published page must carry: the ≤90-character
summary, the dual licence and its files, the classifiers, the docs/pricing/
source/changelog/issues URLs, the Python floor, the dependency and extras
pins, and a README with the 3-line quickstart, the two loader examples and a
licence section. Each test reads only this package directory and needs
nothing installed.
"""

from __future__ import annotations

import pathlib
import re
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
PROJECT = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))["project"]
README = (ROOT / "README.md").read_text(encoding="utf-8")

PRICING = "https://pdfluent.com/sdk/pricing/"


def test_metadata_names_the_package_and_a_short_summary():
    assert PROJECT["name"] == "pdfluent-extract"
    assert PROJECT["version"] == "0.1.0"
    assert PROJECT["readme"] == "README.md"
    assert PROJECT["authors"] == [
        {"name": "Innovation Trigger B.V.", "email": "team@pdfluent.com"}
    ]
    # The summary is the PyPI listing line: the first words count, ≤ 90 chars.
    assert PROJECT["description"] == (
        "PDF to Markdown, JSON and RAG chunks — fast, local, no system dependencies"
    )
    assert len(PROJECT["description"]) <= 90


def test_licence_is_dual_agpl_or_commercial_and_its_files_ship():
    assert PROJECT["license"] == "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"
    assert PROJECT["license-files"] == ["LICENSE", "LICENSE-COMMERCIAL"]
    for name in PROJECT["license-files"]:
        assert (ROOT / name).is_file()


def test_classifiers_cover_python_3_8_to_3_13_platforms_and_text():
    classifiers = PROJECT["classifiers"]
    assert "Development Status :: 4 - Beta" in classifiers
    for minor in ("3.8", "3.9", "3.10", "3.11", "3.12", "3.13"):
        assert f"Programming Language :: Python :: {minor}" in classifiers
    assert "Operating System :: MacOS" in classifiers
    assert "Operating System :: Microsoft :: Windows" in classifiers
    assert "Operating System :: POSIX :: Linux" in classifiers
    assert "Topic :: Text Processing" in classifiers


def test_keywords_cover_the_search_terms():
    for keyword in (
        "pdf",
        "markdown",
        "rag",
        "llm",
        "chunking",
        "langchain",
        "llamaindex",
        "docling",
        "table-extraction",
        "pdf-to-text",
    ):
        assert keyword in PROJECT["keywords"]


def test_urls_point_at_docs_pricing_source_changelog_and_issues():
    urls = PROJECT["urls"]
    assert urls["Homepage"] == "https://pdfluent.com"
    assert urls["Documentation"] == "https://pdfluent.com/docs"
    assert urls["Pricing"] == PRICING
    assert urls["Source"] == "https://github.com/pdfluent/pdfluent-sdk"
    assert urls["Changelog"] == "https://github.com/pdfluent/pdfluent-sdk/blob/main/CHANGELOG.md"
    assert urls["Issues"] == "https://github.com/pdfluent/pdfluent-sdk/issues"


def test_python_floor_dependency_and_extras_pins():
    assert PROJECT["requires-python"] == ">=3.8"
    assert "pdfluent>=1.1.0,<2" in PROJECT["dependencies"]
    optional = PROJECT["optional-dependencies"]
    assert "langchain-pdfluent>=0.1" in optional["langchain"]
    assert "llama-index-readers-pdfluent>=0.1" in optional["llamaindex"]


def test_console_script_is_wired():
    assert PROJECT["scripts"] == {"pdfluent-extract": "pdfluent_extract.cli:main"}


def test_readme_opens_with_the_three_line_quickstart():
    assert "pip install pdfluent-extract" in README
    assert "from pdfluent_extract import to_markdown" in README
    assert 'print(to_markdown("report.pdf"))' in README


def test_readme_covers_chunks_loaders_platforms_and_licence():
    assert "## Chunks for RAG" in README
    assert "to_chunks" in README
    assert "PDFluentLoader" in README
    assert "PDFluentReader" in README
    assert "## Supported platforms" in README
    assert "manylinux2014" in README
    assert "musllinux_1_2" in README
    assert "## Licence" in README
    assert "AGPL-3.0 or a commercial licence" in README
    assert PRICING in README


def test_readme_links_are_absolute_because_pypi_renders_no_relative_paths():
    for match in re.finditer(r"\]\(([^)]+)\)", README):
        target = match.group(1)
        assert target.startswith(("https://", "mailto:", "#", "pip install")), target
    for match in re.finditer(r"!\[[^\]]*\]\(([^)]+)\)", README):
        assert match.group(1).startswith("https://"), match.group(1)


def test_published_files_name_no_internal_tracker():
    # Written as two halves so this file itself never carries the term it
    # forbids; it ships in the sdist like every other test.
    tracker = "pdfluent-" + "internal"
    for text in (README, (ROOT / "pyproject.toml").read_text(encoding="utf-8")):
        assert tracker not in text
        assert re.search(r"(?<![\w/])#\d{3,4}\b", text) is None
