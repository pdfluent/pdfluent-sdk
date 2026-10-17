# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The metadata and README the PyPI page renders for langchain-pdfluent.

Every test pins one thing a published page must carry: the name and version,
the dual licence and its files, the classifiers, the docs and pricing URLs,
the Python floor, the dependency pins, and a README with an install command,
a ten-line example per mode and a licence section. Each test reads only this
package directory and needs nothing installed, so it also runs where the
`pdfluent` release the loader wraps is not available yet.
"""

from __future__ import annotations

import pathlib
import re
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
PROJECT = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))["project"]
README = (ROOT / "README.md").read_text(encoding="utf-8")

PRICING = "https://pdfluent.com/sdk/pricing"


def test_metadata_names_the_package_and_its_readme():
    assert PROJECT["name"] == "langchain-pdfluent"
    assert PROJECT["version"] == "0.1.0"
    assert PROJECT["description"].strip()
    assert PROJECT["readme"] == "README.md"
    assert PROJECT["authors"] == [
        {"name": "Innovation Trigger B.V.", "email": "team@pdfluent.com"}
    ]


def test_licence_is_dual_agpl_or_commercial_and_its_files_ship():
    assert PROJECT["license"] == "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"
    assert PROJECT["license-files"] == ["LICENSE", "LICENSE-COMMERCIAL"]
    for name in PROJECT["license-files"]:
        assert (ROOT / name).is_file()


def test_classifiers_cover_status_audience_platform_and_python():
    classifiers = PROJECT["classifiers"]
    assert "Development Status :: 4 - Beta" in classifiers
    assert "Intended Audience :: Developers" in classifiers
    assert "Operating System :: OS Independent" in classifiers
    assert "Programming Language :: Python :: 3" in classifiers
    for minor in ("3.9", "3.10", "3.11", "3.12"):
        assert f"Programming Language :: Python :: {minor}" in classifiers
    assert "Topic :: Scientific/Engineering :: Artificial Intelligence" in classifiers


def test_urls_point_at_docs_pricing_support_and_repository():
    urls = PROJECT["urls"]
    assert urls["Homepage"] == "https://pdfluent.com"
    assert urls["Documentation"] == "https://pdfluent.com/docs"
    assert urls["Pricing"] == PRICING
    assert urls["Repository"] == "https://github.com/pdfluent/pdfluent-sdk"
    assert urls["Bug Tracker"] == "https://github.com/pdfluent/pdfluent-sdk/issues"


def test_python_floor_keywords_and_dependency_pins():
    assert PROJECT["requires-python"] == ">=3.9"
    assert "pdfluent" in PROJECT["keywords"]
    assert "pdfluent>=1.1.0" in PROJECT["dependencies"]
    assert "langchain-core>=0.3" in PROJECT["dependencies"]


def test_readme_has_install_licence_and_the_pricing_link():
    assert "## Install" in README
    assert "pip install langchain-pdfluent" in README
    assert "## Licence" in README
    assert "AGPL-3.0 or a commercial licence" in README
    assert PRICING in README


def _mode_example(mode: str) -> list[str]:
    """The lines of the fenced example under the mode's own heading."""
    section = README.split(f"### {mode} mode", 1)[1]
    section = section.split("### ", 1)[0]
    match = re.search(r"```python\n(.*?)```", section, re.S)
    assert match is not None, f"no python example under {mode} mode"
    return match.group(1).splitlines()


def test_each_mode_has_a_ten_line_example():
    blocks = _mode_example("Blocks")
    assert len(blocks) == 10
    assert 'PDFluentLoader("report.pdf")' in "\n".join(blocks)

    markdown = _mode_example("Markdown")
    assert len(markdown) == 10
    assert 'mode="markdown"' in "\n".join(markdown)


def test_published_files_name_no_internal_tracker():
    # Written as two halves so this file itself never carries the term it
    # forbids; it ships in the sdist like every other test.
    tracker = "pdfluent-" + "internal"
    for text in (README, (ROOT / "pyproject.toml").read_text(encoding="utf-8")):
        assert tracker not in text
        assert re.search(r"(?<![\w/])#\d{3,4}\b", text) is None
