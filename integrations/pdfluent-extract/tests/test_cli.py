# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The `pdfluent-extract` console script over `fixtures/structured.pdf`.

Every case runs the CLI as a subprocess — the installed console script when
this environment installed the package (as run_extract_tests.sh does), and
always `python -m pdfluent_extract.cli`, which is the same `main()`.
Markdown on stdout carries the fixture's heading; a bad `--format` or
`--pages` is exit 2; an unreadable input is exit 1 with a stderr message.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys

import pytest

pdfluent = pytest.importorskip("pdfluent")

STRUCTURED_PDF = os.path.join(
    os.path.dirname(__file__), "..", "..", "..", "fixtures", "structured.pdf"
)


def _commands() -> list[list[str]]:
    """The CLI under test: `-m` always, plus the console script when installed."""
    cmds = [[sys.executable, "-m", "pdfluent_extract.cli"]]
    script = shutil.which("pdfluent-extract")
    if script is not None:
        cmds.append([script])
    return cmds


def test_fixture_to_markdown_on_stdout():
    for cmd in _commands():
        proc = subprocess.run(
            [*cmd, STRUCTURED_PDF], capture_output=True, text=True, timeout=120
        )
        assert proc.returncode == 0, proc.stderr
        assert "# Safety rules" in proc.stdout
        assert "- Wear the issued boots on the floor." in proc.stdout


def test_bad_format_is_a_usage_error():
    for cmd in _commands():
        proc = subprocess.run(
            [*cmd, STRUCTURED_PDF, "--format", "yaml"],
            capture_output=True, text=True, timeout=120,
        )
        assert proc.returncode == 2, proc.stdout + proc.stderr
        assert proc.stderr.strip() != ""


def test_bad_pages_is_a_usage_error():
    for cmd in _commands():
        proc = subprocess.run(
            [*cmd, STRUCTURED_PDF, "--pages", "abc"],
            capture_output=True, text=True, timeout=120,
        )
        assert proc.returncode == 2, proc.stdout + proc.stderr
        assert "page" in proc.stderr.lower()


def test_pages_select_counting_from_one():
    for cmd in _commands():
        proc = subprocess.run(
            [*cmd, STRUCTURED_PDF, "--pages", "1"],
            capture_output=True, text=True, timeout=120,
        )
        assert proc.returncode == 0, proc.stderr
        assert "# Safety rules" in proc.stdout
        assert "| Band | Units | Rate |" not in proc.stdout


def test_missing_file_is_an_extraction_error():
    for cmd in _commands():
        proc = subprocess.run(
            [*cmd, "no-such-file.pdf"], capture_output=True, text=True, timeout=120
        )
        assert proc.returncode == 1, proc.stdout + proc.stderr
        assert "no-such-file.pdf" in proc.stderr


def test_json_format_parses_and_names_the_file():
    for cmd in _commands():
        proc = subprocess.run(
            [*cmd, STRUCTURED_PDF, "--format", "json"],
            capture_output=True, text=True, timeout=120,
        )
        assert proc.returncode == 0, proc.stderr
        doc = json.loads(proc.stdout)
        assert doc["name"] == "structured"


def test_chunks_format_is_a_json_list_with_provenance():
    for cmd in _commands():
        proc = subprocess.run(
            [*cmd, STRUCTURED_PDF, "--format", "chunks"],
            capture_output=True, text=True, timeout=120,
        )
        assert proc.returncode == 0, proc.stderr
        chunks = json.loads(proc.stdout)
        assert isinstance(chunks, list) and chunks
        assert chunks[0]["text"] == "Safety rules"
        assert chunks[0]["metadata"]["page"] == 0


def test_text_format_joins_chunk_texts():
    for cmd in _commands():
        proc = subprocess.run(
            [*cmd, STRUCTURED_PDF, "--format", "text"],
            capture_output=True, text=True, timeout=120,
        )
        assert proc.returncode == 0, proc.stderr
        assert "Safety rules" in proc.stdout


def test_stdin_and_stdout_dashes():
    with open(STRUCTURED_PDF, "rb") as fh:
        data = fh.read()
    for cmd in _commands():
        proc = subprocess.run(
            [*cmd, "-", "-o", "-"],
            input=data, capture_output=True, timeout=120,
        )
        assert proc.returncode == 0, proc.stderr.decode()
        assert "# Safety rules" in proc.stdout.decode()


def test_output_file_holds_the_markdown(tmp_path):
    out = tmp_path / "out.md"
    for cmd in _commands():
        proc = subprocess.run(
            [*cmd, STRUCTURED_PDF, "-o", str(out)],
            capture_output=True, text=True, timeout=120,
        )
        assert proc.returncode == 0, proc.stderr
        assert "# Safety rules" in out.read_text()


def test_unsupported_quality_passes_through_and_fails_loudly():
    # The CLI accepts the three documented qualities and the engine
    # decides: v1 only runs fast, so balanced fails with the engine's
    # refusal rather than a silent downgrade.
    for cmd in _commands():
        proc = subprocess.run(
            [*cmd, STRUCTURED_PDF, "--quality", "balanced"],
            capture_output=True, text=True, timeout=120,
        )
        assert proc.returncode == 1, proc.stdout + proc.stderr
        assert "fast" in proc.stderr
