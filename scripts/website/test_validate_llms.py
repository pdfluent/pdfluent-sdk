#!/usr/bin/env python3
"""
Unit/integration tests for validate_llms.py.

Runs without any third-party deps — invoke directly:

    python3 scripts/website/test_validate_llms.py

Exits 0 on pass, non-zero on first failed assertion.
"""

from __future__ import annotations

import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import validate_llms as v  # noqa: E402


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _write(tmp: Path, name: str, content: str) -> Path:
    p = tmp / name
    p.write_text(content, encoding="utf-8")
    return p


def _run(tmp: Path, llms: str, llms_full: str, *, strict: bool = False,
         allow: str = "", deny: str = "") -> v.Report:
    a = _write(tmp, "llms.txt", llms)
    b = _write(tmp, "llms-full.txt", llms_full)
    al = _write(tmp, "allow.txt", allow) if allow else None
    dn = _write(tmp, "deny.txt", deny) if deny else None
    return v.run(a, b, al, dn, strict)


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


def test_clean_input_exits_zero():
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        clean = (
            "# PDFluent\n\n"
            "Install: cargo add pdfluent\n"
            "Docs: https://pdfluent.com/docs/rust\n"
        )
        report = _run(tmp, clean, clean)
        assert report.exit_code() == 0, f"expected 0, got {report.exit_code()} ({report.counts})"
        assert not report.violations, f"unexpected: {report.violations}"
        print("PASS  test_clean_input_exits_zero")


def test_locale_leak_is_error():
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        bad = "Docs: https://pdfluent.com/nl/docs/rust\n"
        report = _run(tmp, bad, "# clean\n")
        assert report.exit_code() != 0
        assert report.counts.get("locale_leak", 0) >= 1, report.counts
        print("PASS  test_locale_leak_is_error")


def test_locale_in_redirect_paragraph_is_allowed():
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        ok = (
            "## i18n note\n"
            "Other locale prefixes (e.g. /de/...) redirect to English today.\n"
        )
        report = _run(tmp, ok, ok)
        assert report.counts.get("locale_leak", 0) == 0, report.counts
        print("PASS  test_locale_in_redirect_paragraph_is_allowed")


def test_stale_package_is_error_when_unguarded():
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        bad = "npm install xfa-wasm\n"
        report = _run(tmp, bad, "# clean\n")
        assert report.counts.get("stale_package", 0) >= 1, report.counts
        assert report.exit_code() != 0
        print("PASS  test_stale_package_is_error_when_unguarded")


def test_stale_package_allowed_in_rename_note():
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        ok = (
            "## Rename note\n"
            "The package xfa-wasm was renamed to @pdfluent/sdk-wasm in 1.0.0-beta.4;\n"
            "old installs are deprecated and will not receive updates.\n"
        )
        report = _run(tmp, ok, ok)
        assert report.counts.get("stale_package", 0) == 0, report.counts
        print("PASS  test_stale_package_allowed_in_rename_note")


def test_feature_denylist_is_error():
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        bad = "PDFluent is a drop-in Adobe Reader replacement.\n"
        deny = "drop-in Adobe Reader replacement\n"
        report = _run(tmp, bad, "# clean\n", deny=deny)
        assert report.counts.get("feature_denied", 0) >= 1, report.counts
        assert report.exit_code() != 0
        print("PASS  test_feature_denylist_is_error")


def test_feature_denylist_skipped_in_negation():
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        ok = "PDFluent is **not** a drop-in Adobe Reader replacement.\n"
        deny = "drop-in Adobe Reader replacement\n"
        report = _run(tmp, ok, "# clean\n", deny=deny)
        assert report.counts.get("feature_denied", 0) == 0, (
            f"negated disclaimer should not fire: {report.counts}"
        )
        print("PASS  test_feature_denylist_skipped_in_negation")


def test_claim_unreviewed_is_warning_not_error():
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        line = "We are compatible with QuantumPDF Pro X.\n"
        report = _run(tmp, line, "# clean\n", allow="")
        # warning, not error: exit_code should still be 0 without --strict
        assert report.counts.get("claim_unreviewed", 0) >= 1, report.counts
        assert report.exit_code() == 0, "warnings must not fail without --strict"
        print("PASS  test_claim_unreviewed_is_warning_not_error")


def test_strict_promotes_warning_to_failure():
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        line = "We are compatible with QuantumPDF Pro X.\n"
        report = _run(tmp, line, "# clean\n", allow="", strict=True)
        assert report.exit_code() == 2, f"strict mode should exit 2, got {report.exit_code()}"
        print("PASS  test_strict_promotes_warning_to_failure")


def test_allowlisted_claim_is_silent():
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        line = "PDFluent supports rust on stable.\n"
        report = _run(tmp, line, "# clean\n", allow="supports rust\n")
        assert report.counts.get("claim_unreviewed", 0) == 0, report.counts
        print("PASS  test_allowlisted_claim_is_silent")


def main() -> int:
    tests = [
        test_clean_input_exits_zero,
        test_locale_leak_is_error,
        test_locale_in_redirect_paragraph_is_allowed,
        test_stale_package_is_error_when_unguarded,
        test_stale_package_allowed_in_rename_note,
        test_feature_denylist_is_error,
        test_feature_denylist_skipped_in_negation,
        test_claim_unreviewed_is_warning_not_error,
        test_strict_promotes_warning_to_failure,
        test_allowlisted_claim_is_silent,
    ]
    fail = 0
    for t in tests:
        try:
            t()
        except AssertionError as e:
            print(f"FAIL  {t.__name__}: {e}")
            fail += 1
        except Exception as e:  # noqa: BLE001
            print(f"ERROR {t.__name__}: {type(e).__name__}: {e}")
            fail += 1
    print(f"\n{len(tests) - fail}/{len(tests)} tests passed")
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())
