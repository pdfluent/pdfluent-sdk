# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Door harness: spec discovery plus recorded-output helpers.

The behaviour spec lives outside the repo; ``PDFLUENT_DOOR_SPEC`` points
at it (defaulting to the shared location). Every module in this folder
skips cleanly when the spec is absent, so spec-less CI stays green.

Owner-approved differences (D7, 2026-10-10): three chunk tests whose only
remaining divergence is base-14 ink boxes (point boxes vs recorded ink
boxes, all four edges) report ``difference`` instead of failing. The
mechanism lives here so no test body changes: ``assert_data_equal``
re-raises an approved-only mismatch as ``DoorDifference``, and
``pytest_report_teststatus`` counts that outcome apart from pass and fail
(letter ``D``), never as a pass. A listed test with any other divergence
(text, class, counts, positions) still fails; see
``test_door_difference_self.py``.
"""

import json
import math
import os
import sys

import pytest

sys.path.insert(
    0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "python")
)

import pdfluent_extract as pe  # noqa: E402

SPEC_DIR = os.environ.get(
    "PDFLUENT_DOOR_SPEC",
    os.path.join(
        os.path.expanduser("~"), ".local", "share", "pdfluent", "spec-door-impl"
    ),
)
SPEC_MISSING = not (
    os.path.isdir(os.path.join(SPEC_DIR, "fixtures"))
    and os.path.isdir(os.path.join(SPEC_DIR, "results"))
)
SKIP_REASON = "door spec absent; set PDFLUENT_DOOR_SPEC to the spec checkout"


def FIX(name):
    """Absolute path of a spec fixture."""
    return os.path.join(SPEC_DIR, "fixtures", name)


def FIXR(name):
    """Spec-relative input path, for outputs that echo their input.

    The recordings were made with relative inputs ('fixtures/x'), so a
    test comparing a path echo (chunk file_path, JSON filename, batch
    logs) must pass the input relatively; run it with a working
    directory of SPEC_DIR, the progress tests' monkeypatch pattern.
    Absolute FIX() inputs behave identically in every other way.
    """
    return os.path.join("fixtures", name)


def RES(path):
    """Absolute path of a recorded result file."""
    return os.path.join(SPEC_DIR, "results", path)


def read_bytes(path):
    with open(RES(path), "rb") as handle:
        return handle.read()


def read_text(path):
    with open(RES(path), "r", encoding="utf-8", newline="") as handle:
        return handle.read()


def read_json(path):
    with open(RES(path), "r", encoding="utf-8") as handle:
        return json.load(handle)


def read_value(path):
    """Read a recorded probe output, stored as JSON or raw text."""
    try:
        return read_json(path)
    except ValueError:
        return read_text(path)


def read_report(path):
    """Parse a report.jsonl file into {id: entry}."""
    entries = {}
    with open(RES(path), "r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                entry = json.loads(line)
                entries[entry["id"]] = entry
    return entries


def probe_args(probes_path, pid):
    """Return (args, kwargs, out_relpath) for one recorded probe."""
    with open(RES(probes_path), "r", encoding="utf-8") as handle:
        probes = json.load(handle)
    for entry in probes:
        if entry.get("id") == pid:
            return entry.get("args", []), entry.get("kwargs", {}), entry.get("out")
    raise KeyError("no probe %r in %s" % (pid, probes_path))


class DoorDifference(AssertionError):
    """An owner-approved divergence: counted apart, never a pass."""


#: D7 (owner 2026-10-10): chunk tests whose ONLY divergence is base-14
#: ink boxes. Each reason carries the stamp; the full date is 2026-10-10.
DOOR_DIFFERENCES = {
    "test_g53_chunks_page_subset":
        "difference (owner 10-10): base-14 ink metrics",
    "test_g54_layout_metadata":
        "difference (owner 10-10): base-14 ink metrics",
    "test_g56_toc_per_page":
        "difference (owner 10-10): base-14 ink metrics",
}


def _door_diff_paths(got, want, path=""):
    """All differing leaf paths between decoded JSON, float-tolerant."""
    if isinstance(want, float) and isinstance(got, (int, float)):
        if math.isclose(got, want, rel_tol=1e-9, abs_tol=1e-12):
            return []
        return [path]
    if isinstance(want, dict):
        if not isinstance(got, dict):
            return [path]
        if set(got) != set(want):
            return [path + "#keys"]
        found = []
        for key in want:
            found += _door_diff_paths(got[key], want[key],
                                      "%s.%s" % (path, key))
        return found
    if isinstance(want, (list, tuple)):
        if not isinstance(got, (list, tuple)):
            return [path]
        if len(got) != len(want):
            return [path + "#len"]
        found = []
        for index, (g_item, w_item) in enumerate(zip(got, want)):
            found += _door_diff_paths(g_item, w_item,
                                      "%s[%d]" % (path, index))
        return found
    return [] if got == want else [path]


def _is_base14_box_only(got, want):
    """Whether every divergence sits in a page-box bbox coordinate."""
    diffs = _door_diff_paths(got, want)
    if not diffs:
        return False
    for path in diffs:
        # Chunks compare as [{page_boxes: [{bbox: [x0,y0,x1,y1], ...}]}];
        # only a bbox element (any of the four ink edges) is approved.
        if ".page_boxes[" not in path or ".bbox[" not in path:
            return False
        tail = path.rsplit(".bbox[", 1)[1]
        if tail not in ("0]", "1]", "2]", "3]"):
            return False
    return True


def door_difference_reason(test_name, got, want):
    """The difference reason when a listed test mismatches only on boxes.

    Pure, so the self-test drives it. Returns None for unlisted tests,
    for exact matches, and for any divergence outside page-box bboxes:
    those still fail.
    """
    reason = DOOR_DIFFERENCES.get(test_name)
    if reason is None:
        return None
    if _is_base14_box_only(got, want):
        return reason
    return None


def _current_door_test():
    """The running test's function name, from pytest's own variable."""
    current = os.environ.get("PYTEST_CURRENT_TEST", "")
    if "::" not in current:
        return None
    name = current.split("::")[-1].split(" ")[0]
    return name or None


def _assert_data_equal_inner(got, want):
    """Recursively compare decoded JSON, tolerating float noise."""
    if isinstance(want, float) and isinstance(got, (int, float)):
        assert math.isclose(got, want, rel_tol=1e-9, abs_tol=1e-12), (got, want)
    elif isinstance(want, dict):
        assert isinstance(got, dict), (got, want)
        assert set(got) == set(want), (sorted(got), sorted(want))
        for key in want:
            _assert_data_equal_inner(got[key], want[key])
    elif isinstance(want, (list, tuple)):
        assert isinstance(got, (list, tuple)), (got, want)
        assert len(got) == len(want), (len(got), len(want))
        for g_item, w_item in zip(got, want):
            _assert_data_equal_inner(g_item, w_item)
    else:
        assert got == want, (got, want)


def assert_data_equal(got, want):
    """Recursively compare decoded JSON, tolerating float noise.

    For a test in DOOR_DIFFERENCES whose mismatch sits only in page-box
    bboxes, re-raise as DoorDifference (the difference verdict). Every
    other mismatch -- unlisted tests, exact-match passes, new divergences
    on listed tests -- behaves exactly as before.
    """
    try:
        _assert_data_equal_inner(got, want)
    except AssertionError as exc:
        reason = door_difference_reason(_current_door_test(), got, want)
        if reason is not None:
            raise DoorDifference("%s -- %s" % (reason, exc)) from exc
        raise


def outcome(call):
    """Capture a call as ("ok", value) or ("raise", class name, message)."""
    try:
        return ("ok", call())
    except Exception as exc:  # noqa: BLE001 - the point is to record it
        return ("raise", type(exc).__name__, str(exc))


@pytest.fixture(autouse=True)
def _layout_mode():
    pe.use_layout(True)
    yield
    pe.use_layout(True)


#: Door tests that read live OCR text: without a recogniser binary they
#: cannot pass, so the hook below skips them (not a pass) with a reason.
_TESSERACT_TEXT_TESTS = frozenset(
    {
        "test_g133_ocr_words_not_listed",
        "test_g183_default_finds_words",
    }
)

_UNPROBED = object()
_TESSERACT_PROBE = _UNPROBED


def _tesseract_answers():
    """Whether a recogniser binary answers --version (probed once)."""
    global _TESSERACT_PROBE
    if _TESSERACT_PROBE is not _UNPROBED:
        return _TESSERACT_PROBE
    import shutil
    import subprocess

    override = os.environ.get("PDFLUENT_TESSERACT", "")
    binary = override or shutil.which("tesseract")
    answers = False
    if binary:
        try:
            answers = (
                subprocess.run(
                    [binary, "--version"],
                    capture_output=True,
                    timeout=30,
                    check=False,
                ).returncode
                == 0
            )
        except (OSError, ValueError):
            answers = False
    _TESSERACT_PROBE = answers
    return answers


def pytest_collection_modifyitems(items):
    """Skip the live-OCR tests where no recogniser binary answers."""
    if _tesseract_answers():
        return
    skip = pytest.mark.skip(
        reason="SKIPPED (not a pass): no tesseract binary installed"
    )
    for item in items:
        if item.name in _TESSERACT_TEXT_TESTS:
            item.add_marker(skip)


def pytest_report_teststatus(report, config):
    """Count an owner-approved divergence apart from pass and fail."""
    if report.when == "call" and report.failed:
        try:
            text = str(report.longrepr)
        except Exception:
            return None
        if "DoorDifference" in text and "difference (owner 10-10)" in text:
            return ("difference", "D", "DIFFERENCE")
    return None
