#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for the CLI consumer smoke (T6 §6).

Each case builds a release-shaped archive holding a shim `pdfluent` (a shell
script speaking the post-m32 interface) and runs smoke_cli.sh against it
end to end: PASS on a good archive, FAIL naming the defect on a bad one,
SKIPPED-not-a-pass before m32 delivers `extract` in a dev lane, FAIL in a
release lane. The shim asserts the licence key is unset, so the free-tier
property is checked, not assumed.
"""
from __future__ import annotations

import io
import os
import pathlib
import subprocess
import tarfile
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
SMOKE = REPO / "scripts" / "release" / "consumer_smokes" / "smoke_cli.sh"

MINIMUM_CASES = 8  # FLOOR

SENTENCE = "Safety rules"


def _shim(
    *,
    help_text: str,
    markdown: str,
    extra: str = "",
    fail_format: str = "",
    extract_help: str = "",
    strict_unknown: bool = False,
) -> str:
    fail_branch = (
        f'      {fail_format}) echo "boom" >&2; exit 1;;\n' if fail_format else ""
    )
    unknown_branch = (
        '      *) echo "error: invalid value for \'--format\'" >&2; exit 2;;\n'
        if strict_unknown
        else '      *) printf \'output for %s\\n\' "$fmt";;\n'
    )
    return f"""#!/bin/sh
# A post-m32 `pdfluent`: --version, --help, extract --format.
case "$1" in
  --version) echo "pdfluent 1.0.0";;
  --help) printf '%s\\n' "{help_text}";;
  extract)
    if [ "$2" = "--help" ]; then printf '%s\\n' "{extract_help or help_text}"; exit 0; fi
    if [ -n "${{PDFLUENT_LICENSE_KEY:-}}" ]; then
      echo "licence key leaked into the smoke" >&2; exit 3
    fi
    fmt="md"
    prev=""
    for a in "$@"; do
      if [ "$prev" = "--format" ]; then fmt="$a"; fi
      prev="$a"
    done
    case "$fmt" in
{fail_branch}      md) printf '%s\\n' "{markdown}";;
      json) printf '%s\\n' '{extra}';;
      chunks) printf '%s\\n' "chunk one||chunk two";;
      text) printf '%s\\n' "plain text output";;
{unknown_branch}    esac
    ;;
  *) echo "unknown command $1" >&2; exit 2;;
esac
"""


HELP_FULL = "Usage: pdfluent extract INPUT --format md json chunks"
HELP_WITH_TEXT = "Usage: pdfluent extract INPUT --format md json chunks text"
HELP_CLAP_TOP = "Usage: pdfluent extract INPUT"
HELP_CLAP_EXTRACT = "Options: --format <FORMAT> [possible values: md, json, chunks, text]"
# The layout a real clap binary prints: `Possible values:` plus a `- value:`
# list under --format AND --quality (m492: the old parser read the doc words
# as `--format with`, `--format headings` and every format step failed).
HELP_REAL_CLAP_EXTRACT = (
    "Extract a document as Markdown, JSON, chunks, or text\n"
    "\n"
    "Usage: pdfluent extract [OPTIONS] <INPUTS>...\n"
    "\n"
    "Options:\n"
    "      --format <FORMAT>\n"
    "          Output format\n"
    "\n"
    "          Possible values:\n"
    "          - md:     Markdown with headings, lists, and tables\n"
    "          - json:   Block-model JSON\n"
    "          - chunks: Docling JSON chunks for RAG ingestion\n"
    "          - text:   Plain text, one form feed per page\n"
    "\n"
    "      --quality <QUALITY>\n"
    "          Extraction quality\n"
    "\n"
    "          Possible values:\n"
    "          - fast:     The shipping engine\n"
    "          - balanced: The layout model tier\n"
    "          - best:     Needs unshipped models\n"
)
HELP_PREM32 = "Usage: pdfluent extract-text INPUT"
MARKDOWN_GOOD = f"# Title\\n\\n{SENTENCE} for the workshop.\\n"
MARKDOWN_EMPTY_OF_SENTENCE = "# Title\\n\\nNothing to see here.\\n"
JSON_GOOD = '{"text": "Safety rules for the workshop."}'


def _archive(root: pathlib.Path, shim_text: str) -> pathlib.Path:
    archive = root / "pdfluent-1.0.0-x86_64-unknown-linux-gnu.tar.gz"
    data = shim_text.encode()
    info = tarfile.TarInfo("pdfluent-1.0.0-x86_64-unknown-linux-gnu/pdfluent")
    info.size = len(data)
    info.mode = 0o755
    with tarfile.open(archive, "w:gz") as tf:
        tf.addfile(info, io.BytesIO(data))
    return archive


def _run_smoke(
    archive: pathlib.Path | None,
    fixture: pathlib.Path,
    *,
    release_lane: bool = False,
    use_bin: bool = False,
    version: str | None = "1.0.0",
) -> subprocess.CompletedProcess[str]:
    env = dict(os.environ)
    env.pop("PDFLUENT_RELEASE_LANE", None)
    if release_lane:
        env["PDFLUENT_RELEASE_LANE"] = "1"
    env["PDFLUENT_LICENSE_KEY"] = "leaked-on-purpose"
    cmd = [str(SMOKE), "--fixture", str(fixture), "--sentence", SENTENCE]
    if archive is not None and not use_bin:
        cmd += ["--artifact", str(archive)]
    if version is not None:
        cmd += ["--version", version]
    if use_bin:
        assert archive is not None
        # --bin mode: point at the shim extracted from the archive.
        bindir = archive.parent / "unpacked"
        bindir.mkdir(exist_ok=True)
        with tarfile.open(archive, "r:gz") as tf:
            tf.extractall(bindir)
        shim = next(bindir.rglob("pdfluent"))
        shim.chmod(0o755)
        cmd += ["--bin", str(shim)]
    return subprocess.run(cmd, capture_output=True, text=True, timeout=120,
                          env=env)


class SmokeCliTests(unittest.TestCase):
    def test_good_archive_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root,
                _shim(help_text=HELP_FULL, markdown=MARKDOWN_GOOD,
                      extra=JSON_GOOD),
            )
            proc = _run_smoke(archive, fixture)
            self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
            self.assertIn("PASS", proc.stdout)

    def test_markdown_without_sentence_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root,
                _shim(help_text=HELP_FULL,
                      markdown=MARKDOWN_EMPTY_OF_SENTENCE, extra=JSON_GOOD),
            )
            proc = _run_smoke(archive, fixture)
            self.assertEqual(proc.returncode, 1, proc.stdout)
            self.assertIn(SENTENCE, proc.stdout)

    def test_pre_m32_help_skips_in_dev_lane(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root,
                _shim(help_text="Usage: pdfluent info INPUT", markdown="x"),
            )
            proc = _run_smoke(archive, fixture)
            self.assertEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("SKIPPED (not a pass)", proc.stdout)

    def test_pre_m32_help_fails_in_release_lane(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root,
                _shim(help_text="Usage: pdfluent info INPUT", markdown="x"),
            )
            proc = _run_smoke(archive, fixture, release_lane=True)
            self.assertEqual(proc.returncode, 1, proc.stdout)

    def test_extract_text_prefix_does_not_count_as_extract(self):
        # `extract-text` contains "extract" but is not the `extract`
        # subcommand: a prefix match would run a command that is not there.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root, _shim(help_text=HELP_PREM32, markdown="x")
            )
            proc = _run_smoke(archive, fixture)
            self.assertEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("SKIPPED (not a pass)", proc.stdout)

    def test_invalid_json_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root,
                _shim(help_text=HELP_FULL, markdown=MARKDOWN_GOOD,
                      extra="not json{"),
            )
            proc = _run_smoke(archive, fixture)
            self.assertEqual(proc.returncode, 1, proc.stdout)
            self.assertIn("json", proc.stdout)

    def test_bin_mode_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root,
                _shim(help_text=HELP_FULL, markdown=MARKDOWN_GOOD,
                      extra=JSON_GOOD),
            )
            proc = _run_smoke(archive, fixture, use_bin=True)
            self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
            self.assertIn("PASS", proc.stdout)

    def test_version_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root,
                _shim(help_text=HELP_FULL, markdown=MARKDOWN_GOOD,
                      extra=JSON_GOOD),
            )
            proc = _run_smoke(archive, fixture, version="2.0.0")
            self.assertEqual(proc.returncode, 1, proc.stdout)
            self.assertIn("version mismatch", proc.stdout)

    def test_advertised_new_format_is_exercised(self):
        # --help lists `text` and `text` fails: the smoke must run it and
        # fail naming it, not pass on markdown/json/chunks alone.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root,
                _shim(help_text=HELP_WITH_TEXT, markdown=MARKDOWN_GOOD,
                      extra=JSON_GOOD, fail_format="text"),
            )
            proc = _run_smoke(archive, fixture)
            self.assertEqual(proc.returncode, 1, proc.stdout)
            self.assertIn("text", proc.stdout)

    def test_advertised_new_format_passes_when_good(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root,
                _shim(help_text=HELP_WITH_TEXT, markdown=MARKDOWN_GOOD,
                      extra=JSON_GOOD),
            )
            proc = _run_smoke(archive, fixture)
            self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
            self.assertIn("--format text: OK", proc.stdout)

    def test_possible_values_shape_is_parsed(self):
        # The clap spelling: --format <FORMAT> plus a possible-values list.
        # `text` is advertised there and fails, so the smoke fails on it.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root,
                _shim(help_text=HELP_CLAP_TOP, markdown=MARKDOWN_GOOD,
                      extra=JSON_GOOD, fail_format="text",
                      extract_help=HELP_CLAP_EXTRACT),
            )
            proc = _run_smoke(archive, fixture)
            self.assertEqual(proc.returncode, 1, proc.stdout)
            self.assertIn("text", proc.stdout)

    def test_real_clap_list_layout_runs_formats_not_doc_words(self):
        # A real binary's help: `- value:` lists under --format and
        # --quality. The smoke must run json/chunks/text — never the doc
        # words (`with`, `headings`, ...) and never the quality values.
        # The shim rejects unknown formats the way clap does, so a parser
        # that reads doc words fails here instead of passing spuriously.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            fixture = root / "sample.pdf"
            fixture.write_bytes(b"%PDF-1.4 fake\n")
            archive = _archive(
                root,
                _shim(help_text=HELP_CLAP_TOP, markdown=MARKDOWN_GOOD,
                      extra=JSON_GOOD, extract_help=HELP_REAL_CLAP_EXTRACT,
                      strict_unknown=True),
            )
            proc = _run_smoke(archive, fixture)
            self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
            self.assertIn("--format text: OK", proc.stdout)
            self.assertNotIn("--format with", proc.stdout)
            self.assertNotIn("--format fast", proc.stdout)


if __name__ == "__main__":
    unittest.main()
