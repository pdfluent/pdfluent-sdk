#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every branch of the flavour-claim guard, against a fake tree (#443).

The guard reads three real files. This drives it against small stand-ins for
them, so a case can say "the header claims 3b and no baseline exists" without
editing the header, and so the guard's red path is exercised on every run
rather than only on the day somebody breaks it.

Both directions matter. A guard that never goes green is noise; a guard that
never goes red is decoration. Each case below asserts one of the two.

Exit codes:
    0  every case passed
    1  a case failed
"""

from __future__ import annotations

import importlib.util
import io
import json
import pathlib
import sys
import tempfile
from contextlib import redirect_stdout, redirect_stderr

HERE = pathlib.Path(__file__).resolve().parent

failures: list[str] = []
done = 0


def load(name: str, path: pathlib.Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


# The real module name, so the guard's `from pdfa_flavours import …` and this
# file's patches reach the same object.
flavours = load("pdfa_flavours", HERE / "pdfa_flavours.py")
guard = load("pdfa_flavour_guard_under_test", HERE / "a_pdfa_flavour_has_a_baseline.py")

HEADER_CONVERTS = "    /** PDF/A-{f} — ISO 19005-{p} basic. Converts and validates. */"
HEADER_VALIDATES = "    /** PDF/A-{f} — ISO 19005-{p}. Validates only. */"


def write_tree(tmp: pathlib.Path, header_lines: list[str], site_calls: list[str],
               register_levels: list[str], measured: list[str]) -> None:
    baselines = tmp / "baselines"
    baselines.mkdir(exist_ok=True)
    flavours.BASELINE_DIR = baselines
    for name in measured:
        (baselines / flavours.FLAVOURS[name].baseline_name).write_text(
            json.dumps({"platforms": {}}) + "\n"
        )

    register = tmp / "CAPABILITY_REGISTER.md"
    body = "\n".join(f"| **Conformance levels measured** | {lv} |" for lv in register_levels)
    register.write_text(
        "### Digitally sign PDF — `shipped`\n\nPDF/A-3b is mentioned here and must not count.\n\n"
        f"### PDF to PDF/A — `shipped`\n\n{body}\n\n### Next thing\n\nPDF/A-1b here either.\n"
    )

    header = tmp / "pdfluent.h"
    header.write_text("typedef enum {\n" + "\n".join(header_lines) + "\n} PdfALevel;\n")

    site = tmp / "snippets.json"
    site.write_text(json.dumps({f"s{i}": c for i, c in enumerate(site_calls)}, indent=2))

    guard.REGISTER = register
    guard.C_HEADER = header
    guard.SITE_SOURCES = (site,)
    guard.BASELINE_DIR = baselines


def run_guard() -> tuple[int, str]:
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        code = guard.main()
    return code, out.getvalue() + err.getvalue()


def case(name: str, fn) -> None:
    global done
    done += 1
    try:
        with tempfile.TemporaryDirectory(prefix="pdfa-flavour-case-") as d:
            fn(pathlib.Path(d))
    except AssertionError as e:
        failures.append(f"{name}: {e}")
    except Exception as e:  # noqa: BLE001 - a crashing case is a failing case
        failures.append(f"{name}: raised {type(e).__name__}: {e}")


# --------------------------------------------------------------------------- #
def every_claim_measured(tmp: pathlib.Path) -> None:
    write_tree(
        tmp,
        header_lines=[
            HEADER_CONVERTS.format(f="1B", p=1),
            HEADER_CONVERTS.format(f="2B", p=2),
            HEADER_VALIDATES.format(f="4", p=4),
        ],
        site_calls=["doc.convert_to_pdfa(PdfAProfile::A1b)?;"],
        register_levels=["PDF/A-1b · PDF/A-2b"],
        measured=["1b", "2b"],
    )
    code, said = run_guard()
    assert code == 0, f"expected green, got {code}: {said}"


case("a claim with a baseline behind it is green", every_claim_measured)


# --------------------------------------------------------------------------- #
def header_claim_without_baseline(tmp: pathlib.Path) -> None:
    write_tree(
        tmp,
        header_lines=[HEADER_CONVERTS.format(f="3B", p=3), HEADER_CONVERTS.format(f="2B", p=2)],
        site_calls=[],
        register_levels=["PDF/A-2b"],
        measured=["2b"],
    )
    code, said = run_guard()
    assert code == 1, f"a header claim with no baseline must be red, got {code}: {said}"
    assert "PDF/A-3b" in said, said
    assert "C header" in said, said


case("the C header claiming a flavour nothing measured is red", header_claim_without_baseline)


# --------------------------------------------------------------------------- #
def site_claim_without_baseline(tmp: pathlib.Path) -> None:
    write_tree(
        tmp,
        header_lines=[HEADER_CONVERTS.format(f="2B", p=2)],
        site_calls=["doc.convert_to_pdfa(PdfAProfile::A3u)?;"],
        register_levels=["PDF/A-2b"],
        measured=["2b"],
    )
    code, said = run_guard()
    assert code == 1, f"a website claim with no baseline must be red: {said}"
    assert "PDF/A-3u" in said, said


case("a published snippet converting to an unmeasured flavour is red", site_claim_without_baseline)


# --------------------------------------------------------------------------- #
def site_validation_is_not_a_conversion_claim(tmp: pathlib.Path) -> None:
    """`validate_pdfa(PdfAProfile::A1b)` promises nothing about producing one."""
    write_tree(
        tmp,
        header_lines=[HEADER_CONVERTS.format(f="2B", p=2)],
        site_calls=["let r = doc.validate_pdfa(PdfAProfile::A1b)?;"],
        register_levels=["PDF/A-2b"],
        measured=["2b"],
    )
    code, said = run_guard()
    assert code == 0, f"validation is not a conversion claim: {said}"


case("a validation snippet is not read as a conversion claim",
     site_validation_is_not_a_conversion_claim)


# --------------------------------------------------------------------------- #
def header_says_neither(tmp: pathlib.Path) -> None:
    write_tree(
        tmp,
        header_lines=[
            HEADER_CONVERTS.format(f="2B", p=2),
            "    /** PDF/A-3B — ISO 19005-3 basic (allows embedded files). */",
        ],
        site_calls=[],
        register_levels=["PDF/A-2b"],
        measured=["2b"],
    )
    code, said = run_guard()
    assert code == 1, f"an ambiguous header comment must be red: {said}"
    assert "says neither" in said, said


case("a header comment that says neither is red", header_says_neither)


# --------------------------------------------------------------------------- #
def conversion_to_a_level_we_cannot_produce(tmp: pathlib.Path) -> None:
    write_tree(
        tmp,
        header_lines=[HEADER_CONVERTS.format(f="2B", p=2), HEADER_CONVERTS.format(f="4", p=4)],
        site_calls=[],
        register_levels=["PDF/A-2b"],
        measured=["2b"],
    )
    code, said = run_guard()
    assert code == 1, f"claiming conversion to PDF/A-4 must be red: {said}"
    assert "does not produce it" in said, said


case("claiming conversion to a validate-only level is red",
     conversion_to_a_level_we_cannot_produce)


# --------------------------------------------------------------------------- #
def stray_baseline_file(tmp: pathlib.Path) -> None:
    write_tree(
        tmp,
        header_lines=[HEADER_CONVERTS.format(f="2B", p=2)],
        site_calls=[],
        register_levels=["PDF/A-2b"],
        measured=["2b"],
    )
    (guard.BASELINE_DIR / "conformance_baseline_9z.json").write_text("{}\n")
    code, said = run_guard()
    assert code == 1, f"a baseline for no declared flavour must be red: {said}"
    assert "conformance_baseline_9z.json" in said, said


case("a baseline file for no declared flavour is red", stray_baseline_file)


# --------------------------------------------------------------------------- #
def register_mentions_outside_the_section(tmp: pathlib.Path) -> None:
    """PDF/A named under another capability is not a conversion claim here."""
    write_tree(
        tmp,
        header_lines=[HEADER_CONVERTS.format(f="2B", p=2)],
        site_calls=[],
        register_levels=["PDF/A-2b"],
        measured=["2b"],
    )
    code, said = run_guard()
    assert code == 0, (
        "PDF/A-3b under 'Digitally sign PDF' and PDF/A-1b under 'Next thing' are "
        f"outside the section and must not count: {said}"
    )


case("only the PDF to PDF/A section of the register counts",
     register_mentions_outside_the_section)


# --------------------------------------------------------------------------- #
def header_claims_an_undeclared_flavour(tmp: pathlib.Path) -> None:
    """A level nobody declared is the case this guard is for, and it used to be
    the one case it dropped. (codex, #443)"""
    write_tree(
        tmp,
        header_lines=[
            HEADER_CONVERTS.format(f="2B", p=2),
            "    /** PDF/A-5B — ISO 19005-5 basic. Converts and validates. */",
        ],
        site_calls=[],
        register_levels=["PDF/A-2b"],
        measured=["2b"],
    )
    code, said = run_guard()
    assert code == 1, f"an undeclared conversion claim must be red: {said}"
    assert "PDF/A-5B" in said, said
    assert "does not declare" in said, said


case("the C header claiming a flavour nobody declared is red",
     header_claims_an_undeclared_flavour)


# --------------------------------------------------------------------------- #
def site_claims_an_undeclared_flavour(tmp: pathlib.Path) -> None:
    """The same hole as the header's, on the side customers read. (codex, #443)"""
    write_tree(
        tmp,
        header_lines=[HEADER_CONVERTS.format(f="2B", p=2)],
        site_calls=["doc.convert_to_pdfa(PdfAProfile::A5b)?;"],
        register_levels=["PDF/A-2b"],
        measured=["2b"],
    )
    code, said = run_guard()
    assert code == 1, f"an undeclared conversion claim on the site must be red: {said}"
    assert "A5b" in said, said
    assert "does not declare" in said, said


case("a published snippet converting to a flavour nobody declared is red",
     site_claims_an_undeclared_flavour)


print(f"test_a_pdfa_flavour_has_a_baseline: {done} case(s), {len(failures)} failure(s)")
for f in failures:
    print(f"  FAIL {f}")
sys.exit(1 if failures else 0)
